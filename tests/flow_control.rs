use smux::{ConfigBuilder, Session};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[tokio::test]
async fn test_session_flow_control() {
    // Create a small buffer size to trigger flow control
    let config = ConfigBuilder::new()
        .max_frame_size(100)
        .max_receive_buffer(512)
        .build()
        .unwrap();

    let (client_transport, server_transport) = tokio::io::duplex(8192);

    let client_session = Session::client(client_transport, config.clone())
        .await
        .unwrap();
    let server_session = Session::server(server_transport, config).await.unwrap();

    // Open a stream from client
    let mut client_stream = client_session.open_stream().await.unwrap();

    // Give time for SYN to propagate
    tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

    // Server accepts the stream
    let mut server_stream = server_session.accept_stream().await.unwrap();

    // Send data smaller than the buffer first
    let test_data = vec![42u8; 256];
    let expected_len = test_data.len();

    // Write data
    client_stream.write_all(&test_data).await.unwrap();

    // Give time for data to propagate
    tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

    // Read data
    let mut total_read = 0;
    let mut buffer = [0u8; 100];

    while total_read < expected_len {
        let n = server_stream.read(&mut buffer).await.unwrap();
        if n == 0 {
            break;
        }
        total_read += n;

        // Verify data
        for &byte in &buffer[..n] {
            assert_eq!(byte, 42);
        }
    }

    assert_eq!(total_read, expected_len);
}

#[tokio::test]
async fn test_token_exhaustion_and_recovery() {
    let config = ConfigBuilder::new()
        .max_frame_size(100)
        .max_receive_buffer(200)
        .build()
        .unwrap();

    let (client_transport, server_transport) = tokio::io::duplex(8192);

    let client_session = Session::client(client_transport, config.clone())
        .await
        .unwrap();
    let server_session = Session::server(server_transport, config).await.unwrap();

    // Open multiple streams
    let mut client_streams = Vec::new();
    let mut server_streams = Vec::new();

    for _ in 0..3 {
        let client_stream = client_session.open_stream().await.unwrap();
        client_streams.push(client_stream);

        tokio::time::sleep(tokio::time::Duration::from_millis(5)).await;

        let server_stream = server_session.accept_stream().await.unwrap();
        server_streams.push(server_stream);
    }

    let bytes_sent = Arc::new(AtomicUsize::new(0));
    let bytes_received = Arc::new(AtomicUsize::new(0));

    // Spawn writers
    let mut write_handles = Vec::new();
    for (i, mut stream) in client_streams.into_iter().enumerate() {
        let bytes_sent = Arc::clone(&bytes_sent);
        let handle = tokio::spawn(async move {
            let data = vec![i as u8; 2000];
            stream.write_all(&data).await.unwrap();
            bytes_sent.fetch_add(data.len(), Ordering::Relaxed);
        });
        write_handles.push(handle);
    }

    // Spawn readers
    let mut read_handles = Vec::new();
    for mut stream in server_streams.into_iter() {
        let bytes_received = Arc::clone(&bytes_received);
        let handle = tokio::spawn(async move {
            let mut buffer = [0u8; 100];
            let mut total = 0;
            loop {
                match stream.read(&mut buffer).await {
                    Ok(0) => break, // EOF
                    Ok(n) => {
                        total += n;
                        bytes_received.fetch_add(n, Ordering::Relaxed);
                        // Simulate slow processing to test flow control
                        tokio::time::sleep(tokio::time::Duration::from_millis(2)).await;
                    }
                    Err(_) => break,
                }
            }
            total
        });
        read_handles.push(handle);
    }

    // Wait for all operations to complete
    for handle in write_handles {
        handle.await.unwrap();
    }

    for handle in read_handles {
        handle.await.unwrap();
    }

    let total_sent = bytes_sent.load(Ordering::Relaxed);
    let total_received = bytes_received.load(Ordering::Relaxed);

    assert_eq!(total_sent, total_received);
}
