use smux::{Config, Session};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

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

test_with_timeout!(test_basic_data_transfer, 30, {
    let (client_transport, server_transport) = tokio::io::duplex(1024);
    let config = Config::default();

    let client_session = Session::client(client_transport, config.clone()).await?;
    let server_session = Session::server(server_transport, config).await?;

    // Client opens a stream
    let mut client_stream = client_session.open_stream().await?;

    // Give time for SYN frame to propagate
    tokio::time::sleep(Duration::from_millis(10)).await;

    // Server accepts the stream
    let mut server_stream = server_session.accept_stream().await?.unwrap();

    // Verify stream IDs match
    assert_eq!(client_stream.stream_id(), server_stream.stream_id());

    // Transfer data from client to server
    let test_data = b"Hello, smux integration test!";
    client_stream.write_all(test_data).await?;

    // Give time for data to propagate
    tokio::time::sleep(Duration::from_millis(10)).await;

    // Server reads the data
    let mut buffer = vec![0u8; test_data.len()];
    server_stream.read_exact(&mut buffer).await?;
    assert_eq!(buffer, test_data);

    // Clean shutdown
    client_stream.shutdown().await?;
    server_stream.shutdown().await?;

    Ok::<(), Box<dyn std::error::Error>>(())
});

test_with_timeout!(test_bidirectional_transfer, 30, {
    let (client_transport, server_transport) = tokio::io::duplex(1024);
    let config = Config::default();

    let client_session = Session::client(client_transport, config.clone()).await?;
    let server_session = Session::server(server_transport, config).await?;

    // Establish stream connection
    let client_stream = client_session.open_stream().await?;
    tokio::time::sleep(Duration::from_millis(10)).await;
    let server_stream = server_session.accept_stream().await?.unwrap();

    // Spawn tasks for concurrent bidirectional communication
    let client_data = b"Client -> Server";
    let server_data = b"Server -> Client";

    let client_task = {
        let client_stream = client_stream;
        let client_data = *client_data;
        let server_data = *server_data;
        tokio::spawn(async move {
            let mut client_stream = client_stream;
            // Send data to server
            client_stream.write_all(&client_data).await.unwrap();

            // Read response from server
            let mut buffer = vec![0u8; server_data.len()];
            client_stream.read_exact(&mut buffer).await.unwrap();
            assert_eq!(buffer, server_data);

            client_stream.shutdown().await.unwrap();
        })
    };

    let server_task = {
        let server_stream = server_stream;
        let client_data = *client_data;
        let server_data = *server_data;
        tokio::spawn(async move {
            let mut server_stream = server_stream;
            // Read data from client
            let mut buffer = vec![0u8; client_data.len()];
            server_stream.read_exact(&mut buffer).await.unwrap();
            assert_eq!(buffer, client_data);

            // Send response to client
            server_stream.write_all(&server_data).await.unwrap();

            server_stream.shutdown().await.unwrap();
        })
    };

    // Wait for both tasks to complete
    tokio::try_join!(client_task, server_task)?;

    Ok::<(), Box<dyn std::error::Error>>(())
});

test_with_timeout!(test_multiple_concurrent_streams, 30, {
    let (client_transport, server_transport) = tokio::io::duplex(2048);
    let config = Config::default();

    let client_session = Session::client(client_transport, config.clone()).await?;
    let server_session = Session::server(server_transport, config).await?;

    const NUM_STREAMS: usize = 5;
    let mut client_tasks = Vec::new();
    let mut server_tasks = Vec::new();

    // Create multiple streams
    for i in 0..NUM_STREAMS {
        let client_stream = client_session.open_stream().await?;
        client_tasks.push(tokio::spawn({
            let mut stream = client_stream;
            async move {
                let data = format!("Data from client stream {i}");
                stream.write_all(data.as_bytes()).await.unwrap();

                // Read echo response
                let mut buffer = vec![0u8; data.len()];
                stream.read_exact(&mut buffer).await.unwrap();
                assert_eq!(buffer, data.as_bytes());

                stream.shutdown().await.unwrap();
                i // Return stream index for verification
            }
        }));
    }

    // Give time for all SYN frames to propagate
    tokio::time::sleep(Duration::from_millis(50)).await;

    // Accept and handle streams on server side
    for _ in 0..NUM_STREAMS {
        let server_stream = server_session.accept_stream().await?.unwrap();
        server_tasks.push(tokio::spawn({
            let mut stream = server_stream;
            async move {
                // Read data from client
                let mut buffer = [0u8; 64];
                let n = stream.read(&mut buffer).await.unwrap();
                let received_data = &buffer[..n];

                // Echo it back
                stream.write_all(received_data).await.unwrap();

                stream.shutdown().await.unwrap();
            }
        }));
    }

    // Wait for all tasks to complete
    let client_results = futures::future::try_join_all(client_tasks).await?;
    futures::future::try_join_all(server_tasks).await?;

    // Verify all streams completed
    assert_eq!(client_results.len(), NUM_STREAMS);
    for (i, result) in client_results.into_iter().enumerate() {
        assert_eq!(result, i);
    }

    Ok::<(), Box<dyn std::error::Error>>(())
});

test_with_timeout!(test_session_close, 30, {
    let (client_transport, server_transport) = tokio::io::duplex(1024);
    let config = Config::default();

    let client_session = Session::client(client_transport, config.clone()).await?;
    let server_session = Session::server(server_transport, config).await?;

    // Open a stream
    let mut client_stream = client_session.open_stream().await?;
    tokio::time::sleep(Duration::from_millis(10)).await;
    let mut server_stream = server_session.accept_stream().await?.unwrap();

    // Write some data
    client_stream.write_all(b"test data").await?;
    tokio::time::sleep(Duration::from_millis(10)).await;

    // Verify we can read the data first
    let mut buffer = [0u8; 64];
    let n = server_stream.read(&mut buffer).await?;
    assert_eq!(&buffer[..n], b"test data");

    // Close the client session
    client_session.close().await?;

    // Give time for close to propagate
    tokio::time::sleep(Duration::from_millis(50)).await;

    // Verify sessions are marked as closed
    assert!(client_session.is_closed());

    // Close server session too for clean shutdown
    server_session.close().await?;

    Ok::<(), Box<dyn std::error::Error>>(())
});

test_with_timeout!(test_stream_close, 30, {
    let (client_transport, server_transport) = tokio::io::duplex(1024);
    let config = Config::default();

    let client_session = Session::client(client_transport, config.clone()).await?;
    let server_session = Session::server(server_transport, config).await?;

    // Establish stream
    let mut client_stream = client_session.open_stream().await?;
    tokio::time::sleep(Duration::from_millis(10)).await;
    let mut server_stream = server_session.accept_stream().await?.unwrap();

    // Client sends data then closes write side
    client_stream.write_all(b"closing after this").await?;
    client_stream.shutdown().await?;

    // Give time for data and FIN to propagate
    tokio::time::sleep(Duration::from_millis(10)).await;

    // Server should be able to read the data
    let mut buffer = [0u8; 64];
    let n = server_stream.read(&mut buffer).await?;
    assert_eq!(&buffer[..n], b"closing after this");

    // Next read should return EOF due to client closing write side
    let n = server_stream.read(&mut buffer).await?;
    assert_eq!(n, 0, "Expected EOF after client closed write side");

    // Server can still write back (stream is half-closed)
    server_stream.write_all(b"acknowledgment").await?;

    // But then server closes too
    server_stream.shutdown().await?;

    Ok::<(), Box<dyn std::error::Error>>(())
});

test_with_timeout!(test_peer_disconnect, 30, {
    let (client_transport, server_transport) = tokio::io::duplex(1024);
    let config = Config::default();

    let client_session = Session::client(client_transport, config.clone()).await?;
    let server_session = Session::server(server_transport, config).await?;

    // Establish stream
    let mut client_stream = client_session.open_stream().await?;
    tokio::time::sleep(Duration::from_millis(10)).await;
    let mut server_stream = server_session.accept_stream().await?.unwrap();

    // Send some data
    client_stream.write_all(b"test").await?;
    tokio::time::sleep(Duration::from_millis(10)).await;

    // Verify server receives it
    let mut buffer = [0u8; 4];
    server_stream.read_exact(&mut buffer).await?;
    assert_eq!(&buffer, b"test");

    // Abruptly close client session (simulating network disconnect)
    drop(client_session);
    drop(client_stream);

    // Give time for close to propagate
    tokio::time::sleep(Duration::from_millis(20)).await;

    // Server operations should eventually fail or return EOF
    let result = tokio::time::timeout(
        Duration::from_millis(100),
        server_stream.write_all(b"this should fail"),
    )
    .await;

    // We expect either a timeout or an error
    match result {
        Err(_) => {
            // Timeout is acceptable
        }
        Ok(Err(_)) => {
            // I/O error is expected when peer disconnects
        }
        Ok(Ok(())) => {
            // If write succeeded, subsequent read should fail or return EOF
            let read_result =
                tokio::time::timeout(Duration::from_millis(100), server_stream.read(&mut buffer))
                    .await;

            match read_result {
                Ok(Ok(0)) => {
                    // EOF is acceptable
                }
                Ok(Err(_)) => {
                    // I/O error is expected
                }
                Err(_) => {
                    // Timeout is also acceptable
                }
                Ok(Ok(_)) => {
                    return Err("Unexpected successful read after peer disconnect".into());
                }
            }
        }
    }

    Ok::<(), Box<dyn std::error::Error>>(())
});
