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
    // A weak reference to the session to avoid reference cycles.
    session: Weak<SessionInner>,
    // Buffer for incoming data.
    read_buffer: Mutex<VecDeque<Bytes>>,
    // Notifies waiting read calls that new data has arrived or the stream has closed.
    read_notifier: Notify,
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

The `AsyncRead` implementation will read from the `read_buffer`.

```mermaid
graph TD
    A[AsyncRead::poll_read()] --> B{Lock read_buffer};
    B --> C{Buffer has data?};
    C -- Yes --> D[Copy data to user buffer];
    D --> E[Update flow control window];
    E --> F[Return Poll::Ready(Ok(n))];

    C -- No --> G{Is stream read-closed?};
    G -- Yes --> H[Return Poll::Ready(Ok(0)) - EOF];
    G -- No --> I[Wait on read_notifier];
    I --> J[Return Poll::Pending];

    subgraph "Session's recv_loop"
        K[Receives PSH frame] --> L[Pushes data to read_buffer];
        L --> M[Calls read_notifier.notify_one()];
    end

    M --> I;
```

1.  **`poll_read`** is called on the `Stream`.
2.  It acquires a lock on the `read_buffer`.
3.  If the buffer contains data, it's copied to the user-provided buffer. The number of bytes copied is returned. For v2, an `UPD` frame might be sent to the peer to update the send window.
4.  If the buffer is empty, the task registers with the `read_notifier` and returns `Poll::Pending`.
5.  When the `Session`'s `recv_loop` receives a `PSH` frame for this stream, it pushes the data into the `read_buffer` and calls `read_notifier.notify_one()`, waking up the waiting read task.
6.  If the stream has been read-closed (a `FIN` was received), `poll_read` on an empty buffer will return `Ok(0)`, signaling EOF.

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
4.  A `PSH` frame is created for each chunk.
5.  The frame is sent to the `Session`'s `send_loop` via its `mpsc` channel.
6.  The number of bytes from the user's buffer that were successfully sent is returned.

## 5. Closing a Stream

*   **Write-Side Close**: When `poll_shutdown` is called (or the `Stream` is dropped), a `FIN` frame is sent to the `Session`'s `send_loop`. This signals to the peer that no more data will be sent. The stream can still receive data.
*   **Read-Side Close**: When the `Session` receives a `FIN` frame for this stream, it marks the stream as read-closed and notifies any pending read operations.
*   **Full Close**: A stream is fully closed when it is both read-closed and write-closed. At this point, the `Session` removes it from its active streams map.

## 6. Flow Control (Version 2)

For v2 sessions, each `Stream` will manage its own flow control window.

*   **Receiving Data**: When the application reads data from the `Stream`, the stream will periodically send `UPD` frames to the peer. This `UPD` frame tells the sender how much data has been consumed and what the new window size is, allowing the sender to transmit more data.
*   **Sending Data**: Before sending a `PSH` frame, the `Stream` will check its local view of the peer's receive window. If the window is full (i.e., the number of in-flight bytes equals the window size), the write operation will be suspended until an `UPD` frame arrives from the peer, increasing the window.
