use smux::{Config, Session};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

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

/// Find an available port for testing
async fn find_available_port() -> Result<u16, Box<dyn std::error::Error>> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let port = listener.local_addr()?.port();
    drop(listener); // Close the listener to free the port
    Ok(port)
}

test_with_timeout!(test_e2e_basic_tcp_communication, 30, {
    let port = find_available_port().await?;
    let addr = format!("127.0.0.1:{port}");

    // Start server in background task
    let server_handle = tokio::spawn({
        let addr = addr.clone();
        async move {
            let listener = TcpListener::bind(&addr).await.unwrap();
            let (socket, _) = listener.accept().await.unwrap();

            let config = Config::default();
            let server_session = Session::server(socket, config).await.unwrap();

            // Accept a stream
            let mut server_stream = server_session.accept_stream().await.unwrap().unwrap();

            // Read "hello"
            let mut buffer = [0u8; 5];
            server_stream.read_exact(&mut buffer).await.unwrap();
            assert_eq!(&buffer, b"hello");

            // Send "world" back
            server_stream.write_all(b"world").await.unwrap();
            server_stream.shutdown().await.unwrap();
        }
    });

    // Give server time to start
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Connect client
    let socket = TcpStream::connect(&addr).await?;
    let config = Config::default();
    let client_session = Session::client(socket, config).await?;

    // Open stream and send data
    let mut client_stream = client_session.open_stream().await?;
    client_stream.write_all(b"hello").await?;

    // Read response
    let mut buffer = [0u8; 5];
    client_stream.read_exact(&mut buffer).await?;
    assert_eq!(&buffer, b"world");

    client_stream.shutdown().await?;

    // Wait for server to complete
    server_handle.await.unwrap();

    Ok::<(), Box<dyn std::error::Error>>(())
});

test_with_timeout!(test_e2e_multiple_streams_tcp, 30, {
    let port = find_available_port().await?;
    let addr = format!("127.0.0.1:{port}");

    const NUM_STREAMS: usize = 3;

    // Start server in background task
    let server_handle = tokio::spawn({
        let addr = addr.clone();
        async move {
            let listener = TcpListener::bind(&addr).await.unwrap();
            let (socket, _) = listener.accept().await.unwrap();

            let config = Config::default();
            let server_session = Session::server(socket, config).await.unwrap();

            // Handle multiple streams
            let mut tasks = Vec::new();
            for i in 0..NUM_STREAMS {
                let server_stream = server_session.accept_stream().await.unwrap().unwrap();
                let task = tokio::spawn(async move {
                    let mut stream = server_stream;

                    // Read data
                    let mut buffer = [0u8; 64];
                    let n = stream.read(&mut buffer).await.unwrap();
                    let received = String::from_utf8_lossy(&buffer[..n]);

                    // Verify it's the expected stream data
                    let expected = format!("stream {i}");
                    assert_eq!(received, expected);

                    // Echo it back with prefix
                    let response = format!("echo: {received}");
                    stream.write_all(response.as_bytes()).await.unwrap();
                    stream.shutdown().await.unwrap();
                });
                tasks.push(task);
            }

            // Wait for all streams to complete
            for task in tasks {
                task.await.unwrap();
            }
        }
    });

    // Give server time to start
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Connect client
    let socket = TcpStream::connect(&addr).await?;
    let config = Config::default();
    let client_session = Session::client(socket, config).await?;

    // Create multiple streams
    let mut client_tasks = Vec::new();
    for i in 0..NUM_STREAMS {
        let client_stream = client_session.open_stream().await?;
        let task = tokio::spawn(async move {
            let mut stream = client_stream;

            // Send stream-specific data
            let data = format!("stream {i}");
            stream.write_all(data.as_bytes()).await.unwrap();

            // Read echo response
            let expected_response = format!("echo: {data}");
            let mut buffer = vec![0u8; expected_response.len()];
            stream.read_exact(&mut buffer).await.unwrap();
            assert_eq!(buffer, expected_response.as_bytes());

            stream.shutdown().await.unwrap();
        });
        client_tasks.push(task);
    }

    // Wait for all client streams to complete
    for task in client_tasks {
        task.await.unwrap();
    }

    // Wait for server to complete
    server_handle.await.unwrap();

    Ok::<(), Box<dyn std::error::Error>>(())
});

test_with_timeout!(test_e2e_large_data_transfer, 30, {
    let port = find_available_port().await?;
    let addr = format!("127.0.0.1:{port}");

    // Create large test data (32KB to avoid overwhelming the test)
    let test_data: Vec<u8> = (0..32768).map(|i| (i % 256) as u8).collect();
    let test_data_clone = test_data.clone();

    // Start server in background task
    let server_handle = tokio::spawn({
        let addr = addr.clone();
        async move {
            let listener = TcpListener::bind(&addr).await.unwrap();
            let (socket, _) = listener.accept().await.unwrap();

            let config = Config::default();
            let server_session = Session::server(socket, config).await.unwrap();

            // Accept stream
            let mut server_stream = server_session.accept_stream().await.unwrap().unwrap();

            // Read all data
            let mut received_data = Vec::new();
            let mut buffer = [0u8; 4096];
            loop {
                let n = server_stream.read(&mut buffer).await.unwrap();
                if n == 0 {
                    break; // EOF
                }
                received_data.extend_from_slice(&buffer[..n]);

                // Prevent infinite loop in case of issues
                if received_data.len() >= test_data_clone.len() {
                    break;
                }
            }

            // Verify we received all data correctly
            assert_eq!(received_data.len(), test_data_clone.len());
            assert_eq!(received_data, test_data_clone);

            // Send acknowledgment
            server_stream.write_all(b"received_all").await.unwrap();
            server_stream.shutdown().await.unwrap();
        }
    });

    // Give server time to start
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Connect client
    let socket = TcpStream::connect(&addr).await?;
    let config = Config::default();
    let client_session = Session::client(socket, config).await?;

    // Open stream and send large data
    let mut client_stream = client_session.open_stream().await?;
    client_stream.write_all(&test_data).await?;
    client_stream.shutdown().await?;

    // Read acknowledgment
    let mut ack_buffer = [0u8; 12];
    client_stream.read_exact(&mut ack_buffer).await?;
    assert_eq!(&ack_buffer, b"received_all");

    // Wait for server to complete
    server_handle.await.unwrap();

    Ok::<(), Box<dyn std::error::Error>>(())
});

test_with_timeout!(test_e2e_connection_handling, 30, {
    let port = find_available_port().await?;
    let addr = format!("127.0.0.1:{port}");

    // Start server in background task
    let server_handle = tokio::spawn({
        let addr = addr.clone();
        async move {
            let listener = TcpListener::bind(&addr).await.unwrap();
            let (socket, _) = listener.accept().await.unwrap();

            let config = Config::default();
            let server_session = Session::server(socket, config).await.unwrap();

            // Accept stream but don't do anything - let client disconnect
            let _server_stream = server_session.accept_stream().await.unwrap().unwrap();

            // Wait a bit then try to accept another stream (should fail due to disconnect)
            tokio::time::sleep(Duration::from_millis(200)).await;

            let result =
                tokio::time::timeout(Duration::from_millis(500), server_session.accept_stream())
                    .await;

            // Should either timeout or return None due to session close
            match result {
                Ok(Ok(None)) => {} // Session closed gracefully
                Ok(Ok(Some(_))) => panic!("Unexpected stream after client disconnect"),
                Ok(Err(_)) => {} // Error is acceptable
                Err(_) => {}     // Timeout is acceptable
            }
        }
    });

    // Give server time to start
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Connect client, open stream, then disconnect abruptly
    {
        let socket = TcpStream::connect(&addr).await?;
        let config = Config::default();
        let client_session = Session::client(socket, config).await?;

        let _client_stream = client_session.open_stream().await?;

        // Give time for stream to be established
        tokio::time::sleep(Duration::from_millis(50)).await;

        // Abrupt disconnect - drop everything
    }

    // Wait for server to handle disconnect
    server_handle.await.unwrap();

    Ok::<(), Box<dyn std::error::Error>>(())
});
