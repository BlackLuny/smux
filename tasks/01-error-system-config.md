# Task: Error System & Configuration

## Overview

Establish the foundational error handling system and configuration management for the smux library. This task creates the core types that all other components will depend on.

## Code Design Plan

### Error System (`src/error.rs`)

*   Create a `SmuxError` enum using `thiserror::Error` for structured error handling.
*   Include variants for I/O errors, invalid protocol versions, oversized frames, session and stream management issues, and configuration problems.
*   Implement a `is_recoverable` method to distinguish between transient and fatal errors.
*   Define a `Result<T>` type alias for `std::result::Result<T, SmuxError>`.

### Configuration System (`src/config.rs`)

*   Define a `Config` struct to hold all session parameters, such as version, keep-alive settings, max frame size, and buffer sizes.
*   Implement `Default` to provide sensible defaults.
*   Provide a `validate` method to check for invalid configuration combinations (e.g., keep-alive timeout less than interval, buffer size relationships).
*   Implement a builder pattern for ergonomic configuration creation.

## Test Plan

### Error System Tests

*   Verify that each error variant formats its display message correctly.
*   Test the conversion from `std::io::Error` into `SmuxError::Io`.
*   Unit test the `is_recoverable` logic for different error types.

### Configuration Tests

*   Test that the default configuration is valid.
*   Write tests for the `validate` method, covering both successful and failing cases for each configuration parameter.
*   Test the builder pattern to ensure it correctly constructs a `Config` object.
*   Verify that keep-alive validation is skipped when it's disabled.
