# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

pog is a fast log file viewer built with Rust and GTK4. It uses memory-mapped files for efficient handling of large log files with virtual scrolling.

## Build Commands

```bash
# Build release (optimized with LTO)
cargo build --release

# Build debug
cargo build

# Run with a file
cargo run --release -- <logfile>

# Run tests
cargo test
```

## Architecture

The application consists of two main components:

- **main.rs**: GTK4 application setup and UI. Implements virtual scrolling through `populate_lines()` which only renders visible lines plus a buffer. The `VISIBLE_LINES` and `BUFFER_LINES` constants control the virtual scrolling behavior.

- **file_loader.rs**: Memory-mapped file handling via `MappedFile` struct. Builds a line index on load for O(1) line access. Handles both Unix (LF) and Windows (CRLF) line endings.

## Dependencies

- **gtk4**: UI framework
- **memmap2**: Memory-mapped file access
- **clap**: Command-line argument parsing with derive macros
