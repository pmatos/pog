# pog-lang: Command Protocol Reference

pog includes a built-in TCP server that accepts commands to control the viewer programmatically.

## Connection

- **Address**: `127.0.0.1` (localhost only)
- **Default port**: `9876`
- **Protocol**: Text-based, newline-delimited

## CLI Options

```bash
pog [OPTIONS] <FILE>

Options:
    --port <PORT>    Port for the command server [default: 9876]
    --no-server      Disable the command server
```

## Protocol Format

### Request
```
<command> [arguments]\n
```

Commands are case-insensitive. Arguments are separated by whitespace.

### Response
```
OK [message]\n
ERROR <message>\n
```

## Commands

### goto

Navigate to a specific line number.

**Syntax:**
```
goto <line_number>
```

**Arguments:**
- `line_number`: 1-based line number (first line is 1)

**Response:**
- `OK` on success
- `ERROR line out of range: requested <N>, file has <M> lines` if line number is invalid

**Examples:**
```
goto 1
OK

goto 500
OK

goto 999999999
ERROR line out of range: requested 999999999, file has 1000 lines

goto 0
ERROR line number must be >= 1
```

### lines

Get the total number of lines in the file.

**Syntax:**
```
lines
```

**Response:**
- `OK <count>` - the total line count

**Examples:**
```
lines
OK 35655272
```

### top

Get the current top visible line number.

**Syntax:**
```
top
```

**Response:**
- `OK <line_number>` - 1-based line number of the topmost visible line

**Examples:**
```
top
OK 500
```

### size

Get the file size in bytes.

**Syntax:**
```
size
```

**Response:**
- `OK <bytes>` - file size in bytes

**Examples:**
```
size
OK 52428800
```

### mark

Highlight a specific line or column range with a color.

**Syntax:**
```
mark <line_number> <color>
mark <line_number> <start_col>-<end_col> <color>
```

**Arguments:**
- `line_number`: 1-based line number
- `start_col`: 1-based starting column (inclusive)
- `end_col`: 1-based ending column (exclusive)
- `color`: Any valid CSS color (named colors like `red`, `blue`, or hex codes like `#FF0000`)

**Response:**
- `OK` on success
- `ERROR line out of range: requested <N>, file has <M> lines` if line number is invalid
- `ERROR column numbers must be >= 1` if column is 0
- `ERROR start column must be less than end column` if range is invalid

**Examples:**
```
mark 100 red
OK

mark 200 #00FF00
OK

mark 300 light blue
OK

mark 100 5-20 yellow
OK

mark 100 1-10 #FF0000
OK
```

**Notes:**
- Multiple regions can be marked on the same line with different colors
- Region marks override full-line marks where they overlap
- Column ranges are 1-based, with end column being exclusive

### unmark

Remove highlighting from a marked line or specific region.

**Syntax:**
```
unmark <line_number>
unmark <line_number> <start_col>-<end_col>
```

**Arguments:**
- `line_number`: 1-based line number
- `start_col`: 1-based starting column (must match exactly)
- `end_col`: 1-based ending column (must match exactly)

**Response:**
- `OK` on success
- `ERROR line <N> is not marked` if the line/region wasn't marked
- `ERROR line out of range: requested <N>, file has <M> lines` if line number is invalid

**Examples:**
```
unmark 100
OK

unmark 100 5-20
OK

unmark 999
ERROR line 999 is not marked
```

**Notes:**
- `unmark <line>` removes all marks (full-line and all regions) from that line
- `unmark <line> <start>-<end>` removes only the specific region with matching bounds

## Usage Examples

### Using netcat
```bash
# Navigate to line 100
echo "goto 100" | nc localhost 9876

# Interactive session
nc localhost 9876
goto 500
OK
goto 1
OK
```

### Using telnet
```bash
telnet localhost 9876
Trying 127.0.0.1...
Connected to localhost.
goto 42
OK
```

### Shell script example
```bash
#!/bin/bash
send_command() {
    echo "$1" | nc -q 1 localhost 9876
}

# Navigate through a file
send_command "goto 1"
sleep 1
send_command "goto 100"
sleep 1
send_command "goto 500"
```

## Error Handling

All errors are returned in the format:
```
ERROR <description>
```

Common errors:
- `empty command` - No command provided
- `unknown command: <cmd>` - Unrecognized command
- `usage: goto <line_number>` - Missing argument for goto
- `usage: mark <line_number> [<start>-<end>] <color>` - Missing arguments for mark
- `usage: unmark <line_number> [<start>-<end>]` - Missing argument for unmark
- `invalid line number: <value>` - Non-numeric line argument
- `line number must be >= 1` - Line 0 is invalid
- `column numbers must be >= 1` - Column 0 is invalid
- `start column must be less than end column` - Invalid column range
- `line out of range: requested <N>, file has <M> lines` - Line beyond file end
- `line <N> is not marked` - Trying to unmark a line that isn't marked
