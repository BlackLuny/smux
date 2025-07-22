# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

This is a Rust implementation of the [smux protocol](https://github.com/xtaci/smux) - a stream multiplexing library that enables multiple logical streams over a single connection. The library is built on tokio for async I/O.

## Key Commands

### Build & Test
- `make build` or `cargo build` - Build the project
- `make test` or `cargo nextest run --all-features` - Run tests (uses nextest)
- `cargo test` - Standard cargo test (fallback if nextest unavailable)

### Release
- `make release` - Full release process (tag, changelog, push)

## Architecture Overview

The library follows a modular design with these core components:

- **Session**: Manages the underlying connection and multiplexes streams
- **Stream**: Individual logical communication channels (implements AsyncRead/AsyncWrite)
- **Frame**: Protocol data units with header and payload
- **Codec**: Handles frame encoding/decoding using tokio codecs
- **Config**: Session configuration and tuning parameters

### Module Structure
- `smux::config` - Configuration management with validation
- `smux::frame` - Frame types and Command enum (Syn, Fin, Psh, Nop, Upd)
- `smux::codec` - tokio Encoder/Decoder implementation
- `smux::session` - Core session management with receive/send loops
- `smux::stream` - Stream implementation with flow control
- `smux::error` - Unified error handling with thiserror

### Concurrency Model
The Session spawns two main tasks:
1. **Receive Loop**: Reads frames and dispatches to streams
2. **Send Loop**: Serializes frame writes via mpsc channel

## Project Structure

- `specs/` - Detailed specification documents for each component
- `tasks/` - Implementation task breakdowns with test plans
- `reference/smux-golang/` - Reference Go implementation
- `examples/` - Usage examples
- `fixtures/` - Test fixtures

## Development Notes

- Uses `tokio` for async I/O with multi-threaded runtime
- Error handling via `thiserror` with SmuxError enum
- Frame protocol supports flow control with sliding window (v2)
- Configuration includes keep-alive, buffer sizes, and protocol version
- Library is currently minimal (basic test in lib.rs) - implementation in progress per task files
- Follow specs under specs/
- Tasks are described under tasks/
- Should run make check and make test after finish a task. Make sure test pass rate is 100%
