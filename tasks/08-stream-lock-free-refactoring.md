# Stream Refactoring Plan: Lock-Free Implementation

## 1. Executive Summary

This document outlines a plan to refactor the `Stream` implementation in `src/stream.rs` from a lock-based concurrency model to a more performant, lock-free design. The current implementation relies heavily on `Arc<Mutex<T>>` for managing shared state, which can lead to performance bottlenecks and complex, error-prone code.

The proposed refactoring will leverage atomic operations, message passing, and careful state management to eliminate mutexes, thereby reducing contention and improving throughput.

## 2. Current Implementation Analysis

The current `Stream` struct in `src/stream.rs` uses `Arc<Mutex<...>>` for several key components:

*   `frame_rx: Arc<Mutex<mpsc::Receiver<Frame>>>`: The receiver for incoming frames is wrapped in a mutex, creating a major contention point. Only one task can poll for frames at a time.
*   `read_buffer: Arc<Mutex<VecDeque<Bytes>>>`: The buffer for incoming data is protected by a mutex. Both the session's `recv_loop` (writer) and the stream's `poll_read` (reader) compete for this lock.
*   `read_waker: Arc<Mutex<Option<Waker>>>`: The waker for pending read operations is also behind a mutex, which adds overhead to the async polling mechanism.

This design leads to several issues:
*   **Performance Bottlenecks:** Lock contention can serialize access to critical resources, limiting concurrency.
*   **Complexity:** The `poll_read` method is complex due to the need to handle `try_lock` failures and manage wakers manually.
*   **Deadlock Potential:** While not immediately obvious, complex interactions between locks could introduce deadlocks under high load.

## 3. Proposed Lock-Free Architecture

The core idea is to replace the lock-based read buffer with a more efficient, concurrent-friendly mechanism. The optimal solution is to use a **`tokio::sync::mpsc` channel** as the read buffer for each stream. This provides a highly optimized, single-producer, single-consumer (SPSC) queue that is perfect for our use case.

### 3.1. New Data Structures

**`Stream` (The User-Facing Handle)**

The `Stream` struct will now hold the receiving end of the channel.

```rust
// In src/stream.rs
pub struct Stream {
    id: u32,
    // Sender for outgoing frames to the session's send_loop
    frame_tx: mpsc::Sender<Frame>,
    // Receiver for incoming data chunks from the session
    data_rx: mpsc::Receiver<Bytes>,
    // A temporary buffer for when a user reads only part of a data chunk
    current_chunk: Option<Bytes>,
    // Atomic flags for state management
    is_read_closed: Arc<AtomicBool>,
    is_write_closed: Arc<AtomicBool>,
}
```

**`Session` (The State Manager)**

The `Session` will hold the sending end of the channel for each stream.

```rust
// In src/session.rs
struct SessionInner<T> {
    // The DashMap will now store the sender for data chunks
    streams: DashMap<u32, mpsc::Sender<Bytes>>,
    // ... other fields
}
```

### 3.2. Refactoring `poll_read`

The `poll_read` logic becomes dramatically simpler and more robust:

1.  If there's a partially read `Bytes` chunk in `self.current_chunk`, try to fulfill the read from it.
2.  If not, call `poll_recv()` on the `data_rx` channel to get a new `Bytes` chunk.
3.  If a new chunk is received, fulfill the read from it and store any remainder in `self.current_chunk`.
4.  If the channel is empty, `poll_recv` will return `Poll::Pending` and automatically register the waker.
5.  If the channel is closed, it signifies the read side of the stream is closed, so we return EOF.

This design eliminates the need for `Mutex`, `VecDeque`, and `AtomicWaker` for the read path.

### 3.3. Channel Configuration

To prevent uncontrolled memory growth, we will use a **bounded `mpsc` channel**. The channel's capacity will be determined by the `Config::max_stream_buffer` setting. This provides a crucial backpressure mechanism, ensuring that a slow stream consumer cannot cause the session to run out of memory.

### 3.3. Refactoring `poll_write`

The `poll_write` logic remains largely the same, as it communicates with the session's `send_loop` via a channel (`frame_tx`), which is already a lock-free mechanism. However, we will integrate configuration properly.

### 3.4. Session-Side Changes

The `SessionInner` in `src/session.rs` will be modified to hold the `StreamState` directly, likely in the `DashMap`.

```rust
// In src/session.rs
struct SessionInner<T> {
    // The DashMap will now store the StreamState and a sender for UPD frames
    streams: DashMap<u32, (Arc<StreamState>, mpsc::Sender<Frame>)>,
    // ... other fields
}
```

When a frame arrives in `recv_loop`, the session will:
1.  Look up the `StreamState` in the `DashMap`.
2.  Push the data into the `read_buffer`.
3.  Wake the `read_waker`.

## 4. Implementation Steps

1.  **Introduce `AtomicWaker`:** Add a dependency on the `futures-util` crate for `AtomicWaker` or implement a simple version.
2.  **Refactor `Stream` and `StreamState`:** Split the `Stream` struct as described above.
3.  **Update `Session`:** Modify `SessionInner` to manage `StreamState` objects.
4.  **Rewrite `poll_read`:** Implement the simplified, lock-free `poll_read` logic.
5.  **Update `recv_loop`:** Modify the frame handling logic in `session.rs` to interact with the new `StreamState`.
6.  **Integrate Configuration:** Plumb the `Config` object into the `Stream` and use it for values like `max_frame_size` and protocol version.
7.  **Verify Tests:** Ensure all existing tests pass and add new tests for the refactored logic.

## 5. Benefits of Refactoring

*   **Improved Performance:** Eliminating lock contention will increase throughput, especially in highly concurrent scenarios.
*   **Simplified Code:** The `poll_read` logic will be more straightforward and easier to reason about.
*   **Reduced Risk of Deadlocks:** Removing mutexes eliminates a common source of deadlocks.
*   **Better Scalability:** The lock-free design will scale better with the number of cores and streams.