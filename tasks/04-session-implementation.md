
# Task: Session Implementation

This task is to implement the `Session` component as specified in `specs/005-session.md`.

## Code Design Plan

1.  **`Session` and `SessionInner` Structs:**
    *   Define the public-facing `Session<T>` struct containing an `Arc<SessionInner<T>>`.
    *   Define the internal `SessionInner<T>` struct. It will hold:
        *   `streams`: A `DashMap<u32, mpsc::Sender<Frame>>` for managing active streams.
        *   `config`: An `Arc<Config>` for session settings.
        *   `incoming_streams_sender`: An `mpsc::Sender<Stream>` to accept new streams from the peer.
        *   `next_stream_id`: An `AtomicU32` for generating unique stream IDs.
        *   `die`: A `tokio::sync::Notify` for graceful shutdown signaling.

2.  **`Session` Creation:**
    *   Implement the public `Session::client` and `Session::server` methods, which will handle setting up the session for either client or server roles.
    *   These methods will likely call a private `new` function that takes the I/O transport, config, and a flag indicating its role.
    *   The internal constructor will be responsible for:
        *   Creating the `SessionInner` struct.
        *   Initializing the channels for incoming streams and outgoing frames.
        *   Splitting the transport into read and write halves.
        *   Spawning the dedicated `recv_loop` and `send_loop` tasks.

3.  **`recv_loop` Task:**
    *   Takes ownership of the read half of the transport and a `Codec`.
    *   Continuously reads `Frame`s from the transport.
    *   Dispatches frames based on their `Command`.

4.  **`send_loop` Task:**
    *   Takes ownership of the write half of the transport, a `Codec`, and a receiver for outgoing frames.
    *   Continuously receives `Frame`s and writes them to the transport.

5.  **Public `Session` Methods:**
    *   `open_stream()`: Generates a new stream ID, creates a `Stream`, sends a `SYN` frame, adds the new stream to the `streams` map, and returns the `Stream`.
    *   `accept_stream()`: Asynchronously waits for a new `Stream` on an incoming stream channel.
    *   `close()`: Signals the `die` notifier to gracefully shut down the background tasks.

## Test Plan

1.  **Unit Test `Session::open_stream`:**
    *   Verify that `open_stream` returns a new `Stream` instance.
    *   Check that a `SYN` frame is correctly sent.
    *   Ensure the new stream is added to the `streams` map.

2.  **Unit Test `Session::accept_stream`:**
    *   Simulate a `SYN` frame arriving.
    *   Verify `accept_stream` successfully receives the new `Stream`.

3.  **Integration Test Frame Dispatching:**
    *   Create a mock transport.
    *   Send various frames (`PSH`, `FIN`) and verify they are forwarded to the correct stream.

4.  **Integration Test Session Lifecycle:**
    *   Establish a full client-server connection using mock transports.
    *   Test data transfer and stream closing.
    *   Shut down the session and ensure all tasks terminate cleanly.
