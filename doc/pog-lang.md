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

### cursor

Get or set the cursor position. The cursor is used by search-next/search-prev to determine where to search from. The `goto` command also updates the cursor position.

**Syntax:**
```
cursor
cursor <line_number>
```

**Arguments:**
- `line_number`: Optional 1-based line number to set cursor position

**Response:**
- `OK <line_number>` - Current cursor position (when getting)
- `OK` - Success (when setting)
- `ERROR line out of range: requested <N>, file has <M> lines` - If line number is invalid

**Examples:**
```
cursor
OK 1

cursor 35655272
OK

cursor
OK 35655272
```

**Notes:**
- The cursor starts at line 1
- `goto` automatically updates the cursor to the target line
- `search-next` and `search-prev` search from the cursor position and update it when a match is found

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

### search

Start a regex search and highlight matches in the viewport.

**Syntax:**
```
search <regex_pattern>
```

**Arguments:**
- `regex_pattern`: A valid Rust regex pattern

**Response:**
- `OK <count>` - The number of matches found in the current viewport
- `ERROR invalid regex: <details>` - If the pattern is not a valid regex

**Examples:**
```
search error
OK 5

search ERROR|WARN
OK 12

search [0-9]{4}-[0-9]{2}-[0-9]{2}
OK 3

search (invalid
ERROR invalid regex: regex parse error: ...
```

**Notes:**
- Search is viewport-only with a buffer around visible lines for efficiency
- Matches are automatically highlighted with a gold color
- The view navigates to the first match
- Search highlights coexist with manual marks (marks take precedence)

### search-next

Navigate to the next search match.

**Syntax:**
```
search-next
```

**Response:**
- `OK <line> <column> <length>` - Match location (1-based line and column, match length in characters)
- `ERROR no active search` - If no search has been started
- `ERROR no more matches` - If there are no more matches forward

**Examples:**
```
search-next
OK 12345 10 7
```

The response `OK 12345 10 7` means: match found at line 12345, starting at column 10, with length 7 characters.

### search-prev

Navigate to the previous search match.

**Syntax:**
```
search-prev
```

**Response:**
- `OK <line> <column> <length>` - Match location (1-based line and column, match length in characters)
- `ERROR no active search` - If no search has been started
- `ERROR no more matches` - If there are no more matches backward

**Examples:**
```
search-prev
OK 35655226 45 7
```

The response `OK 35655226 45 7` means: match found at line 35655226, starting at column 45, with length 7 characters.

### search-clear

Clear the current search and remove highlights.

**Syntax:**
```
search-clear
```

**Response:**
- `OK` always succeeds

**Examples:**
```
search-clear
OK
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
- `usage: search <regex_pattern>` - Missing pattern for search
- `invalid line number: <value>` - Non-numeric line argument
- `line number must be >= 1` - Line 0 is invalid
- `column numbers must be >= 1` - Column 0 is invalid
- `start column must be less than end column` - Invalid column range
- `line out of range: requested <N>, file has <M> lines` - Line beyond file end
- `line <N> is not marked` - Trying to unmark a line that isn't marked
- `no active search` - Trying to navigate search results without an active search
- `invalid regex: <details>` - Invalid regex pattern provided to search
