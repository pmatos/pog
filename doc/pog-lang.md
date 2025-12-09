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
- `invalid line number: <value>` - Non-numeric line argument
- `line number must be >= 1` - Line 0 is invalid
- `line out of range: requested <N>, file has <M> lines` - Line beyond file end
