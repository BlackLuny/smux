# Task: Benchmarking

## Overview

Implement the performance benchmarks as specified in `specs/007-benchmarking.md`. These benchmarks are essential for measuring the performance of the library, identifying bottlenecks, and preventing performance regressions in the future.

## Tooling

*   Use the `criterion` crate for all benchmarks.
*   Use `tokio::runtime` to execute asynchronous code within the benchmarks.

## Benchmark Plan

1.  **Setup (`benches/` directory):**
    *   Create a helper function to quickly set up a client and server `Session` pair connected over an in-memory transport like `tokio::io::duplex`.

2.  **Throughput Benchmarks:**
    *   **Single Stream:** Measure the time it takes to send a large volume of data (e.g., 1GB) over a single stream. The result should be reported in bytes per second.
    *   **Multi-Stream:** Measure the aggregate throughput of sending data over many concurrent streams (e.g., 100 streams sending 10MB each).

3.  **Latency Benchmarks:**
    *   **Ping-Pong:** Measure the round-trip time for sending a small message on a stream and receiving a reply. This should be measured in microseconds or nanoseconds.
    *   **Stream Creation:** Measure the time it takes to open a new stream (`open_stream` and `accept_stream`).

4.  **Concurrency Benchmarks:**
    *   **Stream Creation Rate:** Measure how many new streams can be opened per second.

## Execution

*   The benchmarks should be runnable via `cargo bench`.
*   `criterion` will automatically save the results and compare them against previous runs, which will be checked into version control to track performance over time.
