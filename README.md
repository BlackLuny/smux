# smux

[![Crates.io](https://img.shields.io/crates/v/smux.svg)](https://crates.io/crates/smux)
[![Documentation](https://docs.rs/smux/badge.svg)](https://docs.rs/smux)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

A Rust implementation of the [smux protocol](https://github.com/xtaci/smux) - a stream multiplexing library that enables multiple logical streams over a single connection.

## Features

- **Async/await support**: Built on tokio for high-performance async I/O
- **Multiple streams**: Create many logical streams over one connection  
- **Flow control**: Built-in sliding window flow control
- **Keep-alive**: Configurable keep-alive mechanism
- **Interoperability**: Compatible with the Go smux protocol
- **Zero-copy**: Efficient data handling with minimal allocations

## Quick Start

Add this to your `Cargo.toml`:

```toml
[dependencies]
smux = "0.1"
tokio = { version = "1.0", features = ["full"] }
```

### Server Example

```rust
use smux::{Config, Session};
use tokio::net::TcpListener;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let listener = TcpListener::bind("127.0.0.1:8080").await?;
    println!("Server listening on 127.0.0.1:8080");
    
    while let Ok((conn, _)) = listener.accept().await {
        tokio::spawn(async move {
            let config = Config::default();
            let mut session = Session::server(conn, config).await.unwrap();
            
            while let Ok(mut stream) = session.accept_stream().await {
                tokio::spawn(async move {
                    let mut buffer = vec![0; 1024];
                    if let Ok(n) = stream.read(&mut buffer).await {
                        // Echo back the data
                        let _ = stream.write_all(&buffer[..n]).await;
                    }
                });
            }
        });
    }
    Ok(())
}
```

### Client Example

```rust
use smux::{Config, Session};
use tokio::net::TcpStream;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let conn = TcpStream::connect("127.0.0.1:8080").await?;
    let config = Config::default();
    let mut session = Session::client(conn, config).await?;
    
    // Open multiple streams concurrently
    for i in 0..10 {
        let mut stream = session.open_stream().await?;
        tokio::spawn(async move {
            let message = format!("Hello from stream {}!", i);
            stream.write_all(message.as_bytes()).await.unwrap();
            
            let mut response = vec![0; 1024];
            let n = stream.read(&mut response).await.unwrap();
            println!("Stream {} received: {}", i, String::from_utf8_lossy(&response[..n]));
        });
    }
    
    // Keep the session alive
    tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
    session.close().await?;
    Ok(())
}
```

## Configuration

Customize the session behavior with `ConfigBuilder`:

```rust
use smux::{Config, ConfigBuilder};
use std::time::Duration;

let config = ConfigBuilder::new()
    .version(2)
    .keep_alive_interval(Duration::from_secs(30))
    .keep_alive_timeout(Duration::from_secs(90))
    .max_frame_size(64 * 1024)  // 64KB frames
    .max_receive_buffer(8 * 1024 * 1024)  // 8MB buffer
    .enable_keep_alive(true)
    .build()
    .expect("Valid configuration");
```

### Configuration Options

| Option | Default | Description |
|--------|---------|-------------|
| `version` | 1 | Protocol version |
| `keep_alive_interval` | 10s | Interval between keep-alive probes |
| `keep_alive_timeout` | 30s | Timeout before considering connection dead |
| `max_frame_size` | 32KB | Maximum size of a single frame |
| `max_receive_buffer` | 4MB | Maximum receive buffer size |
| `max_stream_buffer` | 64KB | Per-stream buffer size |
| `enable_keep_alive` | true | Enable/disable keep-alive mechanism |

## Protocol Compatibility

This implementation is compatible with the [Go smux library](https://github.com/xtaci/smux). You can use Rust clients with Go servers and vice versa.

## Performance

The library is designed for high performance with:

- Zero-copy frame processing where possible
- Efficient async I/O with tokio
- Lock-free stream management using atomic operations
- Batched frame sending to reduce syscalls

## Error Handling

The library uses the `SmuxError` enum for comprehensive error handling:

```rust
use smux::{SmuxError, Result};

match session.open_stream().await {
    Ok(stream) => {
        // Use the stream
    }
    Err(SmuxError::SessionClosed) => {
        println!("Session was closed");
    }
    Err(SmuxError::Io(io_err)) => {
        println!("I/O error: {}", io_err);
    }
    Err(e) => {
        println!("Other error: {}", e);
    }
}
```

## License

This project is distributed under the terms of MIT.

See [LICENSE](LICENSE.md) for details.
