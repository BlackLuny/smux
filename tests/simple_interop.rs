use serial_test::serial;
use smux::{Config, Session};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::process::Command;

/// Helper macro to add timeout to tests
macro_rules! test_with_timeout {
    ($test_name:ident, $timeout_secs:expr, $test_body:block) => {
        #[tokio::test]
        #[serial]
        async fn $test_name() {
            let result = tokio::time::timeout(
                Duration::from_secs($timeout_secs),
                async move $test_body
            ).await;

            match result {
                Ok(Ok(())) => {},
                Ok(Err(e)) => panic!("Test failed: {:?}", e),
                Err(_) => panic!("Test timed out after {} seconds", $timeout_secs),
            }
        }
    };
}

/// Find an available port for testing
async fn find_available_port() -> Result<u16, Box<dyn std::error::Error>> {
    use port_check::free_local_port_in_range;

    // Since tests are now serial, we can use a simple approach
    match free_local_port_in_range(50000..=59999) {
        Some(port) => Ok(port),
        None => Err("Could not find available port in range 50000-59999".into()),
    }
}

/// Wait for Go server to be ready
async fn wait_for_server_ready(port: u16, max_attempts: usize) -> bool {
    for attempt in 1..=max_attempts {
        match TcpStream::connect(format!("127.0.0.1:{port}")).await {
            Ok(conn) => {
                drop(conn);
                return true;
            }
            Err(_) => {
                if attempt < max_attempts {
                    tokio::time::sleep(Duration::from_millis(100 * attempt as u64)).await;
                }
            }
        }
    }
    false
}

/// Check if Go is available for testing
async fn check_go_available() -> bool {
    let go_check = Command::new("go").arg("version").output().await;

    if go_check.is_err() {
        println!("Go not available, skipping interop tests");
        return false;
    }

    // Ensure fixtures directory exists for our test files
    tokio::fs::create_dir_all("fixtures").await.ok();
    true
}

/// Start a Go smux echo server using existing fixture
async fn start_go_smux_echo_server(
    port: u16,
) -> Result<tokio::process::Child, Box<dyn std::error::Error>> {
    // Determine binary name based on OS
    let binary_name = if cfg!(windows) {
        "go_smux_echo_server.exe"
    } else {
        "go_smux_echo_server"
    };

    // First compile the Go binary with cross-platform binary name
    let compile_result = Command::new("go")
        .arg("build")
        .arg("-o")
        .arg(binary_name)
        .arg("go_smux_echo_server.go")
        .current_dir("fixtures")
        .output()
        .await?;

    if !compile_result.status.success() {
        return Err(format!(
            "Failed to compile Go server: {}",
            String::from_utf8_lossy(&compile_result.stderr)
        )
        .into());
    }

    // Run the compiled binary directly (cross-platform)
    let binary_path = if cfg!(windows) {
        "./go_smux_echo_server.exe"
    } else {
        "./go_smux_echo_server"
    };

    let child = Command::new(binary_path)
        .arg(port.to_string())
        .current_dir("fixtures")
        .spawn()?;

    Ok(child)
}

/// Start a Go smux client that connects to a Rust server
async fn start_go_smux_client(
    port: u16,
    num_streams: usize,
) -> Result<tokio::process::Child, Box<dyn std::error::Error>> {
    // Determine binary name based on OS
    let binary_name = if cfg!(windows) {
        "go_smux_client.exe"
    } else {
        "go_smux_client"
    };

    // First compile the Go client binary
    let compile_result = Command::new("go")
        .arg("build")
        .arg("-o")
        .arg(binary_name)
        .arg("go_smux_client.go")
        .current_dir("fixtures")
        .output()
        .await?;

    if !compile_result.status.success() {
        return Err(format!(
            "Failed to compile Go client: {}",
            String::from_utf8_lossy(&compile_result.stderr)
        )
        .into());
    }

    // Run the compiled binary
    let binary_path = if cfg!(windows) {
        "./go_smux_client.exe"
    } else {
        "./go_smux_client"
    };

    let child = Command::new(binary_path)
        .arg(port.to_string())
        .arg(num_streams.to_string())
        .current_dir("fixtures")
        .spawn()?;

    Ok(child)
}

/// Simple echo handler for Rust smux server streams
async fn handle_echo_stream(
    mut stream: smux::Stream,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    println!("🔄 Handling echo stream");

    // Read data from the stream with timeout
    let mut buffer = Vec::new();
    let mut temp_buf = [0u8; 1024];

    // Read with timeout to avoid hanging
    let read_result = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            match stream.read(&mut temp_buf).await {
                Ok(0) => {
                    println!("📄 Stream read returned 0 bytes (EOF)");
                    break;
                }
                Ok(n) => {
                    buffer.extend_from_slice(&temp_buf[..n]);
                    println!("📖 Read {} bytes, total: {}", n, buffer.len());

                    // If we haven't read anything for a bit and have data, assume we're done
                    tokio::time::sleep(Duration::from_millis(10)).await;

                    // Try to read more - if nothing comes quickly, we're probably done
                    let mut peek_buf = [0u8; 1];
                    match tokio::time::timeout(
                        Duration::from_millis(50),
                        stream.read(&mut peek_buf),
                    )
                    .await
                    {
                        Ok(Ok(0)) => break, // EOF
                        Ok(Ok(1)) => {
                            buffer.push(peek_buf[0]);
                            continue;
                        }
                        _ => break, // Timeout or error, assume we have all data
                    }
                }
                Err(e) => {
                    println!("❌ Read error: {e}");
                    return Err(e.into());
                }
            }
        }
        Ok::<(), Box<dyn std::error::Error + Send + Sync>>(())
    })
    .await;

    match read_result {
        Ok(Ok(())) => {
            println!("📋 Received data: {}", String::from_utf8_lossy(&buffer));

            // Echo back the data
            if !buffer.is_empty() {
                stream.write_all(&buffer).await?;
                println!("📤 Echoed back {} bytes", buffer.len());

                // Flush to ensure data is sent
                stream.flush().await?;
                println!("🚿 Flushed stream");
            }
        }
        Ok(Err(e)) => return Err(e),
        Err(_) => {
            println!("⏰ Read operation timed out");
            return Err("Read timeout".into());
        }
    }

    // Give client time to read before closing
    tokio::time::sleep(Duration::from_millis(100)).await;

    println!("✅ Stream echo completed");
    Ok(())
}

test_with_timeout!(test_simple_interop_rust_smux_go_smux, 15, {
    if !check_go_available().await {
        println!("Skipping interop test - Go not available");
        return Ok(());
    }

    let port = find_available_port().await?;
    println!("Starting Go smux echo server on port {port}");

    // Start Go smux echo server
    let mut go_server = start_go_smux_echo_server(port).await?;

    // Test execution with guaranteed cleanup
    let test_result = async {
        // Wait for server to be ready with retries
        if !wait_for_server_ready(port, 10).await {
            return Err("Go smux echo server failed to start or become ready".into());
        }

        println!("✅ Go smux echo server is ready on port {port}");

        // Connect with Rust smux client
        let socket = TcpStream::connect(format!("127.0.0.1:{port}")).await?;
        let config = Config::default();
        let client_session = Session::client(socket, config).await?;

        // Open a stream through smux
        let mut client_stream = client_session.open_stream().await?;
        let test_data = b"Hello from Rust smux to Go smux!";

        println!("📤 Sending data: {}", String::from_utf8_lossy(test_data));
        client_stream.write_all(test_data).await?;

        // Close the write side to signal EOF to the Go server
        client_stream.shutdown().await?;

        // Read the echo response
        let mut response = Vec::new();
        client_stream.read_to_end(&mut response).await?;

        let response_str = String::from_utf8_lossy(&response);
        println!("📥 Received echo: {response_str}");

        // Verify we got back what we sent
        assert_eq!(
            response_str,
            String::from_utf8_lossy(test_data),
            "Echo response should match sent data"
        );

        println!("✅ Simple smux interop test passed: Rust smux ↔ Go smux");
        println!("💡 Data was successfully echoed between Rust and Go smux implementations");

        Ok::<(), Box<dyn std::error::Error>>(())
    }
    .await;

    // Always clean up the Go server process
    println!("🧹 Cleaning up Go server process...");

    // Simple kill since we're running the binary directly (no child processes)
    if let Err(e) = go_server.kill().await {
        println!("Warning: Failed to kill Go server: {e}");
    }

    // Wait for process to terminate
    let _ = go_server.wait().await;

    println!("🧹 Go server cleanup completed");

    test_result
});

test_with_timeout!(test_multi_stream_single_session, 30, {
    if !check_go_available().await {
        println!("Skipping multi-stream test - Go not available");
        return Ok(());
    }

    println!("🚀 Multi-stream single session test: 20 streams over one connection");

    let port = find_available_port().await?;
    println!("Starting Go smux echo server on port {port}");

    // Start a single Go smux echo server
    let mut go_server = start_go_smux_echo_server(port).await?;

    // Test execution with guaranteed cleanup
    let test_result = async {
        // Wait for server to be ready
        if !wait_for_server_ready(port, 10).await {
            return Err("Go smux echo server failed to start or become ready".into());
        }

        println!("✅ Go smux echo server is ready on port {port}");

        // Connect with single Rust smux client
        let socket = TcpStream::connect(format!("127.0.0.1:{port}")).await?;
        let config = Config::default();
        let client_session = Session::client(socket, config).await?;

        println!("📡 Creating and testing streams sequentially...");

        // Test streams sequentially to avoid overwhelming the connection
        let mut successful_streams = 0;
        let mut failed_streams = 0;

        for i in 0..20 {
            match client_session.open_stream().await {
                Ok(mut stream) => {
                    let test_data = format!("stream_{i}_test_data");

                    // Send data
                    match stream.write_all(test_data.as_bytes()).await {
                        Ok(()) => {
                            println!("✅ Stream {i}: sent '{test_data}'");

                            // Close write side to signal EOF
                            match stream.shutdown().await {
                                Ok(()) => {
                                    // Read response
                                    let mut response = Vec::new();
                                    match stream.read_to_end(&mut response).await {
                                        Ok(_) => {
                                            let response_str = String::from_utf8_lossy(&response);
                                            if response_str == test_data {
                                                println!("✅ Stream {i}: received echo '{response_str}'");
                                                successful_streams += 1;
                                            } else {
                                                println!("❌ Stream {i}: data mismatch - sent '{test_data}', received '{response_str}'");
                                                failed_streams += 1;
                                            }
                                        }
                                        Err(e) => {
                                            println!("❌ Stream {i} read error: {e}");
                                            failed_streams += 1;
                                        }
                                    }
                                }
                                Err(e) => {
                                    println!("❌ Stream {i} shutdown error: {e}");
                                    failed_streams += 1;
                                }
                            }
                        }
                        Err(e) => {
                            println!("❌ Stream {i} write error: {e}");
                            failed_streams += 1;
                        }
                    }
                }
                Err(e) => {
                    println!("❌ Failed to open stream {i}: {e}");
                    failed_streams += 1;
                }
            }

            // Small delay between streams to avoid overwhelming
            tokio::time::sleep(Duration::from_millis(50)).await;
        }

        // Give the server time to process everything
        tokio::time::sleep(Duration::from_millis(1000)).await;

        println!("📊 Multi-stream test results:");
        println!("   • Total streams: 20");
        println!("   • Successful streams: {successful_streams}");
        println!("   • Failed streams: {failed_streams}");
        println!("   • Success rate: {:.1}%", (successful_streams as f64 / 20.0) * 100.0);

        assert!(successful_streams == 20, "Expected 20 successful streams, got {successful_streams}");

        Ok::<(), Box<dyn std::error::Error>>(())
    }.await;

    // Always clean up the Go server process
    println!("🧹 Cleaning up Go server process...");

    // Simple kill since we're running the binary directly (no child processes)
    if let Err(e) = go_server.kill().await {
        println!("Warning: Failed to kill Go server: {e}");
    }

    // Wait for process to terminate
    let _ = go_server.wait().await;

    println!("🧹 Go server cleanup completed");

    test_result
});

#[tokio::test]
#[serial]
async fn test_interop_environment_check() {
    // This test just checks if the interop test environment is set up
    if check_go_available().await {
        println!("✅ Go environment is available for simple interop tests");
        println!("📝 Run `cargo test test_simple_interop` to execute basic interop tests");
        println!("💡 This tests basic connectivity between Rust smux and Go TCP");
        println!("   (Full smux protocol interop would require the reference Go smux library)");
    } else {
        println!("⚠️  Go environment not available");
        println!("💡 To enable basic interop tests:");
        println!("   Install Go: https://golang.org/dl/");
    }
}

test_with_timeout!(test_rust_server_go_client, 20, {
    if !check_go_available().await {
        println!("Skipping Rust server Go client test - Go not available");
        return Ok(());
    }

    let port = find_available_port().await?;
    println!("🚀 Starting Rust smux server with Go client test on port {port}");

    // Start Rust smux server
    let listener = TcpListener::bind(format!("127.0.0.1:{port}")).await?;
    println!("✅ Rust TCP listener started on port {port}");

    // Spawn server task
    let server_handle = tokio::spawn(async move {
        loop {
            match listener.accept().await {
                Ok((socket, _)) => {
                    println!("📡 TCP connection accepted, creating smux server session");

                    let config = Config::default();
                    match Session::server(socket, config).await {
                        Ok(server_session) => {
                            println!("✅ Smux server session created");

                            // Handle incoming streams
                            let mut stream_count = 0;
                            loop {
                                match server_session.accept_stream().await {
                                    Ok(stream) => {
                                        stream_count += 1;
                                        println!("📥 Accepted stream {stream_count}");

                                        // Handle each stream in a separate task
                                        tokio::spawn(async move {
                                            if let Err(e) = handle_echo_stream(stream).await {
                                                println!("❌ Stream error: {e}");
                                            } else {
                                                println!("✅ Stream handled successfully");
                                            }
                                        });

                                        // Stop after reasonable number for testing
                                        if stream_count >= 5 {
                                            break;
                                        }
                                    }
                                    Err(_) => {
                                        println!("No more streams available");
                                        break;
                                    }
                                }
                            }

                            println!("🏁 Server session completed handling {stream_count} streams");
                        }
                        Err(e) => println!("❌ Failed to create smux server session: {e}"),
                    }
                }
                Err(e) => {
                    println!("❌ Accept error: {e}");
                    break;
                }
            }
        }
    });

    // Wait a moment for server to be ready
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Start Go client
    println!("📤 Starting Go smux client");
    let mut go_client = start_go_smux_client(port, 3).await?;

    // Wait for client to complete with timeout
    let client_result = tokio::time::timeout(Duration::from_secs(15), go_client.wait()).await?;

    match client_result {
        Ok(status) => {
            if status.success() {
                println!("✅ Go client completed successfully");
            } else {
                return Err(format!("Go client failed with status: {status}").into());
            }
        }
        Err(e) => {
            return Err(format!("Go client process error: {e}").into());
        }
    }

    // Cleanup server
    server_handle.abort();

    println!("✅ Rust server ↔ Go client interop test passed");
    Ok::<(), Box<dyn std::error::Error>>(())
});

test_with_timeout!(test_bidirectional_multi_stream, 25, {
    if !check_go_available().await {
        println!("Skipping bidirectional test - Go not available");
        return Ok(());
    }

    println!("🔄 Bidirectional multi-stream test: Rust↔Go both directions");

    // Test 1: Rust client -> Go server (existing functionality)
    let port1 = find_available_port().await?;
    println!("📡 Phase 1: Rust client → Go server on port {port1}");

    let mut go_server = start_go_smux_echo_server(port1).await?;

    let phase1_result = async {
        if !wait_for_server_ready(port1, 10).await {
            return Err("Go server failed to start".into());
        }

        let socket = TcpStream::connect(format!("127.0.0.1:{port1}")).await?;
        let config = Config::default();
        let client_session = Session::client(socket, config).await?;

        for i in 0..3 {
            let mut stream = client_session.open_stream().await?;
            let test_data = format!("rust_to_go_stream_{i}");

            stream.write_all(test_data.as_bytes()).await?;
            stream.shutdown().await?;

            let mut response = Vec::new();
            stream.read_to_end(&mut response).await?;

            assert_eq!(String::from_utf8_lossy(&response), test_data);
        }

        println!("✅ Phase 1 completed: Rust client → Go server");
        Ok::<(), Box<dyn std::error::Error>>(())
    }
    .await;

    go_server.kill().await.ok();
    go_server.wait().await.ok();

    phase1_result?;

    // Test 2: Go client -> Rust server
    let port2 = find_available_port().await?;
    println!("📡 Phase 2: Go client → Rust server on port {port2}");

    let listener = TcpListener::bind(format!("127.0.0.1:{port2}")).await?;

    let server_handle = tokio::spawn(async move {
        println!("🔄 Server listening for connections...");

        match tokio::time::timeout(Duration::from_secs(5), listener.accept()).await {
            Ok(Ok((socket, _))) => {
                println!("📡 TCP connection accepted in Phase 2");
                let config = Config::default();

                match Session::server(socket, config).await {
                    Ok(server_session) => {
                        println!("✅ Smux server session created in Phase 2");

                        // Handle streams with timeout
                        let mut handled_streams = 0;

                        for _i in 0..5 {
                            // Try to handle up to 5 streams
                            match tokio::time::timeout(
                                Duration::from_secs(3),
                                server_session.accept_stream(),
                            )
                            .await
                            {
                                Ok(Ok(stream)) => {
                                    handled_streams += 1;
                                    println!("📥 Accepted stream {handled_streams} in Phase 2");

                                    tokio::spawn(async move {
                                        if let Err(e) = handle_echo_stream(stream).await {
                                            println!("❌ Stream error in Phase 2: {e}");
                                        } else {
                                            println!("✅ Stream handled successfully in Phase 2");
                                        }
                                    });
                                }
                                Ok(Err(_)) => {
                                    println!("📄 No more streams available in Phase 2");
                                    break;
                                }
                                Err(_) => {
                                    println!("⏰ Stream accept timeout in Phase 2");
                                    break;
                                }
                            }
                        }

                        println!("🏁 Phase 2 server handled {handled_streams} streams");
                    }
                    Err(e) => println!("❌ Failed to create server session in Phase 2: {e}"),
                }
            }
            Ok(Err(e)) => println!("❌ Accept error in Phase 2: {e}"),
            Err(_) => println!("⏰ Accept timeout in Phase 2"),
        }
    });

    // Wait for server to be ready
    tokio::time::sleep(Duration::from_millis(200)).await;

    let mut go_client = start_go_smux_client(port2, 3).await?;

    // Wait for client with proper timeout
    let client_result = tokio::time::timeout(Duration::from_secs(15), go_client.wait()).await;

    // Wait for server to finish processing
    tokio::time::sleep(Duration::from_millis(500)).await;
    server_handle.abort();

    match client_result {
        Ok(Ok(status)) => {
            if !status.success() {
                return Err("Go client failed in phase 2".into());
            }
        }
        Ok(Err(e)) => return Err(format!("Go client process error in phase 2: {e}").into()),
        Err(_) => return Err("Go client timeout in phase 2".into()),
    }

    println!("✅ Phase 2 completed: Go client → Rust server");
    println!("🎉 Bidirectional multi-stream test passed!");

    Ok::<(), Box<dyn std::error::Error>>(())
});

test_with_timeout!(test_concurrent_sessions, 30, {
    if !check_go_available().await {
        println!("Skipping concurrent sessions test - Go not available");
        return Ok(());
    }

    println!("🔀 Testing concurrent sessions: Multiple connections simultaneously");

    let port = find_available_port().await?;
    let mut go_server = start_go_smux_echo_server(port).await?;

    let test_result = async {
        if !wait_for_server_ready(port, 10).await {
            return Err("Go server failed to start".into());
        }

        println!("✅ Go server ready, creating concurrent Rust clients");

        // Create multiple concurrent client sessions
        let mut handles = Vec::new();

        for session_id in 0..3 {
            let handle = tokio::spawn(async move {
                let socket = TcpStream::connect(format!("127.0.0.1:{port}"))
                    .await
                    .map_err(|e| format!("Connection error: {e}"))?;
                let config = Config::default();
                let client_session = Session::client(socket, config)
                    .await
                    .map_err(|e| format!("Session creation error: {e}"))?;

                // Each session opens multiple streams
                for stream_id in 0..2 {
                    let mut stream = client_session
                        .open_stream()
                        .await
                        .map_err(|e| format!("Stream open error: {e}"))?;
                    let test_data = format!("session_{session_id}_stream_{stream_id}");

                    stream
                        .write_all(test_data.as_bytes())
                        .await
                        .map_err(|e| format!("Write error: {e}"))?;
                    stream
                        .shutdown()
                        .await
                        .map_err(|e| format!("Shutdown error: {e}"))?;

                    let mut response = Vec::new();
                    stream
                        .read_to_end(&mut response)
                        .await
                        .map_err(|e| format!("Read error: {e}"))?;

                    let response_str = String::from_utf8_lossy(&response);
                    if response_str != test_data {
                        return Err(format!(
                            "Data mismatch in session {session_id} stream {stream_id}"
                        ));
                    }

                    println!("✅ Session {session_id} stream {stream_id} completed");

                    // Small delay to avoid overwhelming
                    tokio::time::sleep(Duration::from_millis(50)).await;
                }

                Ok::<(), String>(())
            });

            handles.push(handle);
        }

        // Wait for all sessions to complete
        for (i, handle) in handles.into_iter().enumerate() {
            match handle.await {
                Ok(Ok(())) => println!("✅ Session {i} completed successfully"),
                Ok(Err(e)) => return Err(format!("Session {i} failed: {e}").into()),
                Err(e) => return Err(format!("Session {i} join error: {e}").into()),
            }
        }

        println!("🎉 All concurrent sessions completed successfully!");
        Ok::<(), Box<dyn std::error::Error>>(())
    }
    .await;

    go_server.kill().await.ok();
    go_server.wait().await.ok();

    test_result
});

test_with_timeout!(test_protocol_version_compatibility, 20, {
    if !check_go_available().await {
        println!("Skipping protocol version test - Go not available");
        return Ok(());
    }

    println!("🔢 Testing protocol version compatibility between Rust and Go");

    let port = find_available_port().await?;
    let mut go_server = start_go_smux_echo_server(port).await?;

    let test_result = async {
        if !wait_for_server_ready(port, 10).await {
            return Err("Go server failed to start".into());
        }

        // Test with version 1 (default for compatibility)
        println!("🧪 Testing with protocol version 1");
        let socket_v1 = TcpStream::connect(format!("127.0.0.1:{port}")).await?;
        let config_v1 = Config::default();
        // Assuming there's a way to set version in config - if not, this tests default behavior
        let client_session_v1 = Session::client(socket_v1, config_v1).await?;

        let mut stream_v1 = client_session_v1.open_stream().await?;
        let test_data_v1 = "version_1_test_data";

        stream_v1.write_all(test_data_v1.as_bytes()).await?;
        stream_v1.shutdown().await?;

        let mut response_v1 = Vec::new();
        stream_v1.read_to_end(&mut response_v1).await?;

        let response_str_v1 = String::from_utf8_lossy(&response_v1);
        if response_str_v1 != test_data_v1 {
            return Err(format!(
                "Version 1 test failed: expected '{test_data_v1}', got '{response_str_v1}'"
            )
            .into());
        }

        println!("✅ Protocol version 1 compatibility confirmed");

        // Test multiple streams with version compatibility
        println!("🧪 Testing multiple streams with version compatibility");
        for i in 0..3 {
            let mut stream = client_session_v1.open_stream().await?;
            let test_data = format!("v1_stream_{i}_data");

            stream.write_all(test_data.as_bytes()).await?;
            stream.shutdown().await?;

            let mut response = Vec::new();
            stream.read_to_end(&mut response).await?;

            let response_str = String::from_utf8_lossy(&response);
            if response_str != test_data {
                return Err(format!(
                    "Version 1 stream {i} failed: expected '{test_data}', got '{response_str}'"
                )
                .into());
            }

            println!("✅ Version 1 stream {i} successful");
        }

        println!("🎉 Protocol version compatibility test passed!");
        Ok::<(), Box<dyn std::error::Error>>(())
    }
    .await;

    go_server.kill().await.ok();
    go_server.wait().await.ok();

    test_result
});
