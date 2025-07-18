
# Task: Stream Implementation

This task is to implement the `Stream` component as specified in `specs/006-stream.md`.

## Code Design Plan

1.  **`Stream` Struct:**
    *   Define the `Stream` struct with the fields specified in the spec: `id`, `frame_rx`, `frame_tx`, `read_buffer`, `is_read_closed`, `is_write_closed`.

2.  **`AsyncRead` Implementation:**
    *   Implement `tokio::io::AsyncRead` for `Stream`.
    *   `poll_read` will consume frames from `frame_rx` and put data into `read_buffer`.

3.  **`AsyncWrite` Implementation:**
    *   Implement `tokio::io::AsyncWrite` for `Stream`.
    *   `poll_write` will fragment data into `PSH` frames and send them via `frame_tx`.
    *   `poll_shutdown` will send a `FIN` frame.

4.  **Drop Implementation:**
    *   Implement `Drop` to ensure `FIN` is sent if the stream is dropped without being explicitly closed.

## Test Plan

1.  **Unit Test `AsyncRead`:**
    *   Test reading from the stream with incoming `PSH` frames.
    *   Test EOF condition upon receiving a `FIN` frame.

2.  **Unit Test `AsyncWrite`:**
    *   Test writing data and verify correct `PSH` frames are generated.
    *   Test fragmentation of large writes.

3.  **Unit Test Closing:**
    *   Test `poll_shutdown` and `Drop` to ensure `FIN` frames are sent correctly.

4.  **Integration Test with `Session`:**
    *   Perform a full data transfer scenario between two streams over a session.
