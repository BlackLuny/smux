# 006: Stream Specification

This document specifies the design of the `Stream`, which provides an ordered, reliable, bidirectional byte stream over a `Session`.

## 1. Overview

A `Stream` is the primary interface for application data transfer. It behaves like a standard network connection and implements `tokio::io::AsyncRead` and `tokio::io::AsyncWrite` for seamless integration with the Rust async ecosystem.

Key responsibilities of a `Stream`:
*   Buffering incoming data received from the `Session`.
*   Providing a non-blocking read interface for application code.
*   Accepting data from application code to be written.
*   Fragmenting large writes into `smux` frames.
*   Interacting with the `Session` to send frames and manage flow control.

## 2. `Stream` Structure

```rust
pub struct Stream {
    id: u32,
    // Receives Frames from the session's recv_loop.
    frame_rx: mpsc::Receiver<Frame>,
    // Sends Frames to the session's send_loop. This is a clone of the session's global send channel.
    frame_tx: mpsc::Sender<Frame>,
    // A buffer for data that has been received from the channel but not yet read by the application.
    // This is needed because a read might not consume a full `Bytes` chunk.
    read_buffer: VecDeque<Bytes>,
    // Set when a FIN frame is received, indicating the peer will send no more data.
    is_read_closed: AtomicBool,
    // Set when the stream is closed for writing.
    is_write_closed: AtomicBool,
    // Per-stream window size for v2 flow control.
    window: AtomicU32,
    // ... other state for v2 flow control
}
```

## 3. Read Logic

The `AsyncRead` implementation translates the incoming `Frame` stream into a simple byte stream for the application.

```mermaid
graph TD
    A[AsyncRead::poll_read()] --> B{read_buffer has data?};
    B -- Yes --> C[Copy data to user & return Ready(Ok(n))];

    B -- No --> D{Poll frame_rx channel};
    D -- Pending --> E[Return Pending];
    D -- Closed --> F{Is buffer empty?};
    F -- Yes --> G[Return Ready(Ok(0)) - EOF];
    F -- No --> B;

    D -- Ready(Some(frame)) --> H{match frame.cmd};
    H -- PSH --> I[Push data to read_buffer];
    I --> B;
    H -- FIN --> J[Mark as read-closed & notify];
    J --> F;
    H -- UPD --> K[Update send window];
    K --> D; // Loop to process next frame
```

1.  **`poll_read`** is called. It first checks if the `read_buffer` already contains data. If so, it copies it to the user's buffer and returns `Poll::Ready`.
2.  If the `read_buffer` is empty, the function begins to process incoming frames by polling the `frame_rx` channel in a loop.
3.  If the channel is `Pending`, the current task is parked and will be woken when a new frame arrives.
4.  If the channel returns a `Frame`, the function inspects the frame's command:
    *   **`Command::Psh`**: The data payload from the frame is pushed to the back of the `read_buffer`. The loop is broken, and control returns to step 1, which will now find data in the buffer.
    *   **`Command::Fin`**: The stream's `is_read_closed` flag is set. Any subsequent poll on an empty buffer will result in EOF.
    *   **`Command::Upd`**: The stream's internal state for flow control is updated. The loop continues to poll for more frames.
5.  If the channel is closed, it means the `Session` has shut down. `poll_read` will return EOF once the `read_buffer` is fully drained.

## 4. Write Logic

The `AsyncWrite` implementation will fragment the user's data into `smux` frames and send them to the `Session`'s `send_loop`.

```mermaid
graph TD
    A[AsyncWrite::poll_write()] --> B{Fragment user data into a Frame};
    B --> C{Respect v2 window size?};
    C -- Yes --> D[Wait for window update if needed];
    C -- No (v1) --> E[Send PSH frame to Session's send_loop];
    D --> E;
    E --> F[Return Poll::Ready(Ok(n))];
```

1.  **`poll_write`** is called with a buffer of data to write.
2.  The data is chunked into frames, respecting `config.max_frame_size`.
3.  For v2, it checks the available send window. If the window is too small, it will wait for an `UPD` frame from the peer.
4.  A `PSH` frame is created for each chunk with the stream's ID and data payload.
5.  The frame is sent to the `Session`'s `send_loop` via the stream's `frame_tx` channel.
6.  The number of bytes from the user's buffer that were successfully sent is returned.

## 5. Closing a Stream

*   **Write-Side Close**: When `poll_shutdown` is called (or the `Stream` is dropped), a `FIN` frame is sent to the `Session`'s `send_loop` via the stream's `frame_tx` channel. This signals to the peer that no more data will be sent. The stream can still receive data.
*   **Read-Side Close**: When the `Session` receives a `FIN` frame for this stream, it marks the stream as read-closed and notifies any pending read operations.
*   **Full Close**: A stream is fully closed when it is both read-closed and write-closed. At this point, the `Session` removes it from its active streams map.

## 6. Flow Control (Version 2)

For v2 sessions, each `Stream` will manage its own flow control window.

*   **Receiving Data**: When the application reads data from the `Stream`, the stream will periodically send `UPD` frames to the peer. This `UPD` frame tells the sender how much data has been consumed and what the new window size is, allowing the sender to transmit more data.
*   **Sending Data**: Before sending a `PSH` frame, the `Stream` will check its local view of the peer's receive window. If the window is full (i.e., the number of in-flight bytes equals the window size), the write operation will be suspended until an `UPD` frame arrives from the peer, increasing the window.
