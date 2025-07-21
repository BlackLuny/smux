use smux::{Config, Session};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::process::Command;

/// Helper macro to add timeout to tests
macro_rules! test_with_timeout {
    ($test_name:ident, $timeout_secs:expr, $test_body:block) => {
        #[tokio::test]
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

/// Find an available port for testing with verification
async fn find_available_port() -> Result<u16, Box<dyn std::error::Error>> {
    for _ in 0..10 {
        // Try up to 10 times
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let port = listener.local_addr()?.port();
        drop(listener);

        // Wait a moment to ensure port is released
        tokio::time::sleep(Duration::from_millis(10)).await;

        // Verify port is actually available by trying to bind again
        match TcpListener::bind(format!("127.0.0.1:{port}")).await {
            Ok(test_listener) => {
                drop(test_listener);
                return Ok(port);
            }
            Err(_) => continue, // Port still in use, try again
        }
    }
    Err("Could not find available port after 10 attempts".into())
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
