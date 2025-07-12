# 007: Benchmarking Specification

This document outlines the strategy for performance benchmarking the `smux` Rust library to ensure it meets high-performance standards and to track regressions over time.

## 1. Tooling

We will use the [`criterion`](https://crates.io/crates/criterion) crate as our primary benchmarking framework. It provides statistically rigorous analysis and is the de facto standard for benchmarking in the Rust ecosystem.

## 2. Benchmark Scenarios

The benchmarks will be organized in the `benches` directory and will cover the following key scenarios:

### 2.1. Throughput

*   **Goal**: Measure the maximum data transfer rate.
*   **Scenarios**:
    1.  **Single-Stream Throughput**: Measure the speed of sending a large amount of data (e.g., 1 GB) over a single stream. This will test the raw performance of the data path.
    2.  **Multi-Stream Throughput**: Measure the aggregate throughput when sending data over many concurrent streams (e.g., 100 streams sending 10 MB each). This will test the session's ability to handle concurrent load.

### 2.2. Latency

*   **Goal**: Measure the overhead of stream creation and communication.
*   **Scenarios**:
    1.  **Stream Open Latency**: Measure the time it takes to open a new stream and receive a confirmation from the peer (round-trip time for `SYN`).
    2.  **Ping-Pong Latency**: Measure the round-trip time of sending a small message on a stream and receiving a reply. This will test the latency of the entire data path for small payloads.

### 2.3. Concurrency and Scalability

*   **Goal**: Measure how the library performs as the number of streams increases.
*   **Scenarios**:
    1.  **Stream Creation Rate**: Measure how many new streams can be opened per second.
    2.  **High-Concurrency Ping-Pong**: Run the ping-pong test across a large number of concurrent streams (e.g., 1,000 or 10,000) to identify any contention points in the session management logic.

## 3. Methodology

*   **Setup**: Benchmarks will be run locally, using a `tokio` TCP client/server pair communicating over the loopback interface. This minimizes network variability.
*   **Implementation**: Each benchmark will be defined in its own file within the `benches` directory (e.g., `benches/throughput.rs`).
*   **Execution**: Benchmarks will be run using `cargo bench`. The results will be stored, and `criterion` will automatically compare performance against previous runs, highlighting any regressions.

## 4. Example Benchmark Structure

Here is a conceptual example of what a throughput benchmark might look like:

```rust
// benches/throughput_bench.rs
use criterion::{criterion_group, criterion_main, Criterion, Throughput, Bencher};
use tokio::runtime::Runtime;
use smux::{Session, Config};
// ... other necessary imports

fn setup_session_pair() -> (Session, Session) {
    // Helper to create a client and server session connected over an in-memory pipe
    // ...
}

fn single_stream_throughput(c: &mut Criterion) {
    let mut group = c.benchmark_group("single_stream_throughput");
    let data_size = 1024 * 1024; // 1 MB

    group.throughput(Throughput::Bytes(data_size as u64));
    group.bench_function("1mb_transfer", |b: &mut Bencher| {
        let rt = Runtime::new().unwrap();
        b.to_async(rt).iter_batched(
            setup_session_pair,
            |(mut client_session, mut server_session)| async move {
                // ... benchmark logic ...
            },
            criterion::BatchSize::SmallInput,
        );
    });
    group.finish();
}

criterion_group!(benches, single_stream_throughput);
criterion_main!(benches);
```

## 5. Performance Goals

*   **Primary Goal**: To be competitive with, or exceed, the performance of the original Go implementation under similar conditions.
*   **Secondary Goal**: To ensure that no code changes introduce significant performance regressions without justification. `criterion`'s regression analysis will be key to enforcing this.

By implementing this benchmarking strategy, we can be confident in the library's performance and make data-driven decisions during development and optimization.
