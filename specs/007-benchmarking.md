# 007: Benchmarking Specification

This document outlines the strategy for performance benchmarking the `smux` Rust library, focusing on throughput testing for data send/receive operations.

## 1. Tooling

We will use **Criterion.rs** for benchmarking, which works with stable Rust and provides statistical analysis, plotting, and detailed performance reports. This approach is more robust than unstable nightly benchmarks.

Add to `Cargo.toml`:
```toml
[dev-dependencies]
criterion = { version = "0.5", features = ["html_reports"] }

[[bench]]
name = "throughput"
harness = false
```

## 2. Benchmark Scenarios

The benchmarks will measure data transfer rates for different stream configurations to evaluate multiplexing performance.

### 2.1. Throughput Tests

**Goal**: Measure data send/receive speeds under various concurrent stream loads.

**Test Scenarios**:
1. **Single Stream**: Baseline throughput with one stream
2. **Multiple Streams**: Concurrent streams (2, 4, 8, 16, 32) to test multiplexing efficiency
3. **Large Data Transfer**: Send large payloads (1MB, 10MB) to test sustained throughput
4. **Small Message Burst**: Many small messages to test frame overhead impact

**Data Patterns**:
- **Sequential Write/Read**: Continuous data streaming
- **Ping-Pong**: Request-response pattern measuring round-trip efficiency
- **Bidirectional**: Simultaneous send/receive on same stream

## 3. Methodology

**Setup**: 
- Use `tokio` TCP client/server pair over loopback interface
- Sessions established with default configuration
- Benchmarks run for statistically significant duration (Criterion handles this)

**Execution**: 
- Run with `cargo bench`
- Criterion generates HTML reports in `target/criterion/`
- Results include throughput metrics (bytes/sec), latency percentiles, and regression analysis

**Environment**:
- Benchmark on dedicated test data to ensure consistent measurements
- Test with different chunk sizes (1KB, 8KB, 64KB) to find optimal transfer size

## 4. Example Benchmark Implementation

```rust
// benches/throughput.rs
use criterion::{black_box, criterion_group, criterion_main, Criterion, Throughput};
use smux::{Config, Session};
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::runtime::Runtime;

const CHUNK_SIZE: usize = 8192; // 8KB chunks
const TEST_DATA_SIZE: usize = 1024 * 1024; // 1MB total

async fn setup_session_pair() -> (Session<TcpStream>, Session<TcpStream>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    
    let client_stream = TcpStream::connect(addr).await.unwrap();
    let (server_stream, _) = listener.accept().await.unwrap();
    
    let config = Config::default();
    let client_session = Session::new_client(client_stream, config.clone()).await.unwrap();
    let server_session = Session::new_server(server_stream, config).await.unwrap();
    
    (client_session, server_session)
}

async fn throughput_test(num_streams: usize, data_size: usize) -> u64 {
    let (client_session, server_session) = setup_session_pair().await;
    let test_data = vec![0u8; CHUNK_SIZE];
    
    // Server task: accept streams and echo data
    let server_handle = tokio::spawn(async move {
        let mut total_bytes = 0u64;
        for _ in 0..num_streams {
            let mut stream = server_session.accept_stream().await.unwrap();
            let test_data = test_data.clone();
            tokio::spawn(async move {
                let mut buffer = vec![0u8; CHUNK_SIZE];
                let mut bytes_received = 0;
                while bytes_received < data_size {
                    let n = stream.read(&mut buffer).await.unwrap();
                    if n == 0 { break; }
                    bytes_received += n;
                    total_bytes += n as u64;
                }
            });
        }
        total_bytes
    });
    
    // Client task: open streams and send data
    let mut client_handles = Vec::new();
    for _ in 0..num_streams {
        let mut stream = client_session.open_stream().await.unwrap();
        let test_data = test_data.clone();
        let handle = tokio::spawn(async move {
            let mut bytes_sent = 0;
            while bytes_sent < data_size {
                let chunk_size = std::cmp::min(CHUNK_SIZE, data_size - bytes_sent);
                stream.write_all(&test_data[..chunk_size]).await.unwrap();
                bytes_sent += chunk_size;
            }
            stream.flush().await.unwrap();
        });
        client_handles.push(handle);
    }
    
    // Wait for completion
    for handle in client_handles {
        handle.await.unwrap();
    }
    
    server_handle.await.unwrap()
}

fn bench_single_stream_throughput(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let mut group = c.benchmark_group("single_stream");
    group.throughput(Throughput::Bytes(TEST_DATA_SIZE as u64));
    
    group.bench_function("1MB", |b| {
        b.to_async(&rt).iter(|| async {
            black_box(throughput_test(1, TEST_DATA_SIZE).await)
        })
    });
    
    group.finish();
}

fn bench_multiple_streams_throughput(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let mut group = c.benchmark_group("multiple_streams");
    
    for num_streams in [2, 4, 8, 16].iter() {
        group.throughput(Throughput::Bytes((TEST_DATA_SIZE * num_streams) as u64));
        group.bench_with_input(
            format!("{}_streams", num_streams),
            num_streams,
            |b, &num_streams| {
                b.to_async(&rt).iter(|| async {
                    black_box(throughput_test(num_streams, TEST_DATA_SIZE).await)
                })
            },
        );
    }
    
    group.finish();
}

criterion_group!(benches, bench_single_stream_throughput, bench_multiple_streams_throughput);
criterion_main!(benches);
```

## 5. Performance Goals & Usage

**Primary Goals**:
- Achieve competitive throughput with Go smux implementation
- Identify optimal chunk sizes and stream concurrency levels
- Detect performance regressions in future changes
- Establish baseline metrics for multiplexing efficiency

**Running Benchmarks**:
```bash
# Run all benchmarks
cargo bench

# Run specific benchmark group
cargo bench single_stream

# Generate HTML reports (saved to target/criterion/)
cargo bench -- --output-format html

# Run with specific test duration
cargo bench -- --measurement-time 10
```

**Interpreting Results**:
- **Throughput**: Bytes/second for data transfer rate
- **Latency**: Time per operation (lower is better)  
- **Regression Analysis**: Criterion detects performance changes over time
- **Statistical Significance**: Confidence intervals and variance analysis

**Expected Performance Targets**:
- Single stream: >100 MB/s on loopback
- Multiple streams: Linear scaling up to CPU/network limits
- Low latency: <1ms for small message round-trips
- Memory efficiency: Minimal allocation overhead per stream
