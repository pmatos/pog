# pog

A fast log file viewer built with Rust and GTK4. Supports both local and remote files via SSH.

## Features

- **Memory-mapped local files** for efficient handling of large log files
- **Remote SSH file support** with on-demand line fetching (`host:/path/to/file`)
- **Virtual scrolling** - only fetches and renders visible lines
- **Line numbers** displayed alongside content
- **Mouse wheel scrolling** and scrollbar navigation

## Installation

```bash
cargo build --release
```

The binary will be at `target/release/pog`.

## Usage

```bash
# View a local file
pog /path/to/logfile.log

# View a remote file via SSH
pog myserver:/var/log/syslog
pog user@host:/path/to/file.log
```

## Requirements

- Rust 1.70+
- GTK4 development libraries
- For remote files: SSH client with key-based authentication configured

## How It Works

### Local Files
Uses memory-mapped files (`memmap2`) with a pre-built line index for O(1) access to any line. The entire file is mapped into memory but only visible lines are rendered.

### Remote Files
Fetches lines on-demand using SSH commands (`tail -n +N | head -n M`). Includes an LRU cache to minimize repeated fetches. Only the lines you're viewing are transferred over the network.

## License

MIT
