# 002: Public Interface

This document specifies the public-facing API for the `smux` library. The primary entry points for users are the `Session` and `Stream` structs.

## 1. `Session` Interface

A `Session` manages the multiplexed connection. It is responsible for creating and accepting streams.

### 1.1. Creation

A `Session` is created from an existing asynchronous I/O object (like a TCP stream) and a `Config`.

```rust
use smux::{Session, Config};
use tokio::io::{AsyncRead, AsyncWrite};

impl<T: AsyncRead + AsyncWrite + Unpin> Session<T> {
    /// Creates a new smux session.
    ///
    /// - `conn`: The underlying transport, e.g., a TCP stream.
    /// - `config`: Session configuration.
    /// - `is_client`: Specifies whether this is a client or server session,
    ///   which determines the starting stream ID (odd for client, even for server).
    pub fn new(conn: T, config: Config, is_client: bool) -> Self;
}
```

### 1.2. Stream Management

```rust
impl<T: AsyncRead + AsyncWrite + Unpin> Session<T> {
    /// Opens a new logical stream.
    ///
    /// This is how a client typically creates a new stream.
    pub async fn open_stream(&self) -> Result<Stream, SmuxError>;

    /// Accepts a new logical stream from the peer.
    ///
    /// This is how a server typically accepts a new stream.
    /// Returns `Err(SmuxError::SessionClosed)` if the session is closed gracefully.
    pub async fn accept_stream(&mut self) -> Result<Stream, SmuxError>;

    /// Closes the session and all associated streams.
    pub async fn close(&self) -> Result<(), SmuxError>;
}
```

### 1.3. Properties

```rust
impl<T: AsyncRead + AsyncWrite + Unpin> Session<T> {
    /// Returns the number of active streams.
    pub fn num_streams(&self) -> usize;

    /// Returns true if the session is closed.
    pub fn is_closed(&self) -> bool;
}
```

## 2. `Stream` Interface

A `Stream` represents a single logical, bidirectional stream within a `Session`. It implements `tokio::io::AsyncRead` and `tokio::io::AsyncWrite`.

### 2.1. I/O Operations

The primary way to interact with a `Stream` is through the `AsyncRead` and `AsyncWrite` traits.

```rust
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

// Reading from a stream
let mut stream = session.accept_stream().await?;
let mut buf = [0; 1024];
let n = stream.read(&mut buf).await?;

// Writing to a stream
let mut stream = session.open_stream().await?;
stream.write_all(b"some data").await?;
```

### 2.2. Properties

```rust
impl Stream {
    /// Returns the unique ID of the stream.
    pub fn id(&self) -> u32;
}
```

### 2.3. Closing a Stream

A `Stream` can be closed explicitly by calling `shutdown()` from `AsyncWriteExt`, or implicitly when it is dropped.

```rust
use tokio::io::AsyncWriteExt;

let mut stream = session.open_stream().await?;
// ... write data ...
stream.shutdown().await?; // Gracefully closes the write half
```

## 3. `Config` Interface

The `Config` struct allows for tuning session parameters.

```rust
pub struct Config {
    pub version: u8,
    pub keep_alive_interval: Duration,
    pub keep_alive_timeout: Duration,
    pub max_frame_size: usize,
    pub max_receive_buffer: usize,
    pub max_stream_buffer: usize,
}

impl Default for Config {
    // Provides sensible defaults
    fn default() -> Self;
}
```

## 4. Error Handling

The library will expose a single error enum, `SmuxError`, which will be used for all fallible operations.

```rust
#[derive(Debug, thiserror::Error)]
pub enum SmuxError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Session is closed")]
    SessionClosed,

    #[error("Stream ID overflow")]
    StreamIdOverflow,

    // ... other variants
}
