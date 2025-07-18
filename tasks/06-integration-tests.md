# Task: Integration and E2E Tests

## Overview

Implement the integration and end-to-end (E2E) tests as defined in `specs/008-test-strategy.md`. These tests are crucial for verifying that all the individual components of the `smux` library work together correctly in realistic scenarios.

## Test Plan

### Integration Tests (`tests/` directory)

1.  **Transport Mocking:**
    *   Use `tokio::io::duplex` to create in-memory, connected pairs of I/O streams. This will serve as the transport for client and server sessions, allowing tests to run without real networking.

2.  **Core Scenarios:**
    *   **Data Transfer:** Test that data written by a client stream is correctly received by the corresponding server stream.
    *   **Bidirectional Transfer:** Ensure data can be sent and received in both directions simultaneously on the same stream.
    *   **Multiple Streams:** Test the session with a high number of concurrent streams to check for interference or state corruption.
    *   **Session Close:** Verify that closing the session gracefully terminates all streams and background tasks.
    *   **Stream Close:** Test that closing an individual stream (`FIN` frame) works correctly and that the other direction of the stream remains open.

### End-to-End (E2E) Tests

1.  **Real Networking:**
    *   Use `tokio::net::TcpListener` and `tokio::net::TcpStream` to run tests over the actual network loopback interface.

2.  **Scenarios:**
    *   **Basic Connect:** Write a test where a client connects to a server, they open a stream, exchange a "hello world" message, and close everything cleanly.
    *   **Keep-Alive:** Enable keep-alive with a short interval and timeout. Verify that `NOP` frames are being sent and that the session is correctly terminated if no frames are received from the peer.

3.  **Interop Testing (Stretch Goal):**
    *   If a Go `smux` client/server is available, write tests to verify that the Rust implementation can correctly communicate with it, proving protocol compliance.
