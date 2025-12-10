# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

pog is a fast log file viewer built with Rust and GTK4. It uses memory-mapped files for efficient handling of large log files with virtual scrolling. Supports both local and remote files via SSH.

## Build Commands

```bash
cargo build --release    # Build optimized with LTO
cargo build              # Build debug
cargo test               # Run tests
cargo run --release -- <logfile>              # Run with local file
cargo run --release -- host:/path/to/file     # Run with remote file
```

## Architecture

### Core Modules

- **main.rs**: GTK4 application, UI setup, virtual scrolling (`LINES_PER_PAGE` constant), and socket command handler
- **file_source.rs**: `FileSource` trait defining the interface for file access (line_count, file_size, get_line, get_lines)
- **file_loader.rs**: `MappedFile` - memory-mapped local files with pre-built line index for O(1) access
- **remote_loader.rs**: `RemoteFile` - SSH-based remote file access using `tail`/`head` commands with retry logic
- **cache.rs**: `LineCache` - LRU cache for remote file chunks
- **commands.rs**: `PogCommand` enum and `parse_command()` for socket protocol
- **server.rs**: TCP server for external control (default port 9876)
- **error.rs**: Custom error types (`PogError`)

### Data Flow

1. File worker thread (`spawn_file_worker`) handles `FileRequest::GetLines` requests
2. Main thread receives `FileResponse::Lines` and calls `populate_lines()` to render
3. Socket server runs in separate thread, sends `CommandRequest` to main thread via async channel
4. Commands like `mark`/`unmark` update CSS dynamically via `CssProvider`

### Socket Command Protocol

TCP server at `127.0.0.1:9876` accepts text commands. See `doc/pog-lang.md` for full protocol reference.

Commands: `goto`, `lines`, `top`, `size`, `mark`, `unmark`

## Dependencies

- **gtk4**: UI framework (requires `v4_12` feature for `load_from_string`)
- **memmap2**: Memory-mapped file access
- **clap**: CLI argument parsing
- **async-channel**: Cross-thread communication
