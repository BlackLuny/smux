use crate::{
    error::{Result, SmuxError},
    frame::Frame,
};
use bytes::Bytes;
use std::{
    pin::Pin,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    task::{Context, Poll},
};
use tokio::{
    io::{AsyncRead, AsyncWrite, ReadBuf},
    sync::mpsc,
};

/// A multiplexed stream within a smux session
///
/// Implements AsyncRead and AsyncWrite for seamless integration with tokio.
#[derive(Debug)]
pub struct Stream {
    /// Stream ID
    stream_id: u32,
    /// Sends frames to the session's send_loop
    frame_tx: mpsc::Sender<Frame>,
    /// Receives incoming data chunks from the session
    data_rx: mpsc::UnboundedReceiver<Bytes>,
    /// A temporary buffer for when a user reads only part of a data chunk
    current_chunk: Option<Bytes>,
    /// Set when a FIN frame is received (no more data from peer)
    is_read_closed: Arc<AtomicBool>,
    /// Set when the stream is closed for writing
    is_write_closed: Arc<AtomicBool>,
}

impl Stream {
    pub(crate) fn new(
        stream_id: u32,
        frame_tx: mpsc::Sender<Frame>,
        data_rx: mpsc::UnboundedReceiver<Bytes>,
    ) -> Self {
        Self {
            stream_id,
            frame_tx,
            data_rx,
            current_chunk: None,
            is_read_closed: Arc::new(AtomicBool::new(false)),
            is_write_closed: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Get the stream ID
    pub fn stream_id(&self) -> u32 {
        self.stream_id
    }

    /// Check if the stream is closed for reading
    pub fn is_read_closed(&self) -> bool {
        self.is_read_closed.load(Ordering::Relaxed)
    }

    /// Check if the stream is closed for writing
    pub fn is_write_closed(&self) -> bool {
        self.is_write_closed.load(Ordering::Relaxed)
    }

    /// Check if the stream is fully closed
    pub fn is_closed(&self) -> bool {
        self.is_read_closed() && self.is_write_closed()
    }

    /// Close the stream for writing (sends FIN frame)
    pub async fn close(&mut self) -> Result<()> {
        if !self.is_write_closed.swap(true, Ordering::Relaxed) {
            let fin_frame = Frame::new_fin(1, self.stream_id); // TODO: Use actual version
            self.frame_tx
                .send(fin_frame)
                .await
                .map_err(|_| SmuxError::SessionClosed)?;
        }
        Ok(())
    }
}

impl AsyncRead for Stream {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        let this = self.get_mut();

        // First, try to fulfill the read from any partially consumed chunk
        if let Some(ref mut chunk) = this.current_chunk {
            let to_copy = std::cmp::min(chunk.len(), buf.remaining());
            if to_copy > 0 {
                let data = chunk.split_to(to_copy);
                buf.put_slice(&data);

                // If chunk is now empty, remove it
                if chunk.is_empty() {
                    this.current_chunk = None;
                }

                return Poll::Ready(Ok(()));
            }
        }

        // No current chunk or remaining buffer space, try to get a new chunk
        match this.data_rx.poll_recv(cx) {
            Poll::Ready(Some(mut chunk)) => {
                let to_copy = std::cmp::min(chunk.len(), buf.remaining());
                if to_copy > 0 {
                    let data = chunk.split_to(to_copy);
                    buf.put_slice(&data);

                    // Store remainder if any
                    if !chunk.is_empty() {
                        this.current_chunk = Some(chunk);
                    }

                    Poll::Ready(Ok(()))
                } else {
                    // No buffer space, store the chunk for later
                    this.current_chunk = Some(chunk);
                    Poll::Ready(Ok(()))
                }
            }
            Poll::Ready(None) => {
                // Channel closed, return EOF
                Poll::Ready(Ok(()))
            }
            Poll::Pending => {
                // No data available, check if read is closed
                if this.is_read_closed.load(Ordering::Relaxed) {
                    Poll::Ready(Ok(()))
                } else {
                    Poll::Pending
                }
            }
        }
    }
}

impl AsyncWrite for Stream {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        let this = self.get_mut();

        if this.is_write_closed.load(Ordering::Relaxed) {
            return Poll::Ready(Err(std::io::Error::new(
                std::io::ErrorKind::BrokenPipe,
                "Stream is closed for writing",
            )));
        }

        if buf.is_empty() {
            return Poll::Ready(Ok(0));
        }

        // For now, we'll use a simple approach and send all data in one frame
        // TODO: Implement proper fragmentation and flow control
        let max_frame_size = 4096; // TODO: Get from config
        let chunk_size = std::cmp::min(buf.len(), max_frame_size);
        let data = Bytes::copy_from_slice(&buf[..chunk_size]);

        let psh_frame = Frame::new_psh(1, this.stream_id, data); // TODO: Use actual version

        // Try to send the frame
        match this.frame_tx.try_send(psh_frame) {
            Ok(_) => Poll::Ready(Ok(chunk_size)),
            Err(mpsc::error::TrySendError::Full(_)) => {
                // Channel is full, return pending and will be woken when ready
                cx.waker().wake_by_ref();
                Poll::Pending
            }
            Err(mpsc::error::TrySendError::Closed(_)) => {
                // Channel closed
                Poll::Ready(Err(std::io::Error::new(
                    std::io::ErrorKind::BrokenPipe,
                    "Session is closed",
                )))
            }
        }
    }

    fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        // For now, we don't buffer writes, so flush is a no-op
        Poll::Ready(Ok(()))
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        let this = self.get_mut();

        if this.is_write_closed.load(Ordering::Relaxed) {
            return Poll::Ready(Ok(()));
        }

        let fin_frame = Frame::new_fin(1, this.stream_id); // TODO: Use actual version

        match this.frame_tx.try_send(fin_frame) {
            Ok(_) => {
                this.is_write_closed.store(true, Ordering::Relaxed);
                Poll::Ready(Ok(()))
            }
            Err(mpsc::error::TrySendError::Full(_)) => {
                // Channel is full, wake and try again
                cx.waker().wake_by_ref();
                Poll::Pending
            }
            Err(mpsc::error::TrySendError::Closed(_)) => {
                // Channel closed, consider it shutdown
                this.is_write_closed.store(true, Ordering::Relaxed);
                Poll::Ready(Ok(()))
            }
        }
    }
}

impl Drop for Stream {
    fn drop(&mut self) {
        // If the stream is not closed for writing, try to send a FIN frame
        if !self.is_write_closed.load(Ordering::Relaxed) {
            self.is_write_closed.store(true, Ordering::Relaxed);
            let fin_frame = Frame::new_fin(1, self.stream_id); // TODO: Use actual version

            // Try to send FIN frame (best effort, ignore errors)
            let _ = self.frame_tx.try_send(fin_frame);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Command;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    #[tokio::test]
    async fn test_stream_creation() {
        let (frame_tx, _) = mpsc::channel(1);
        let (_, data_rx) = mpsc::unbounded_channel();

        let stream = Stream::new(123, frame_tx, data_rx);
        assert_eq!(stream.stream_id(), 123);
        assert!(!stream.is_read_closed());
        assert!(!stream.is_write_closed());
        assert!(!stream.is_closed());
    }

    #[tokio::test]
    async fn test_stream_read_with_data() {
        let (frame_tx, _) = mpsc::channel(1);
        let (data_tx, data_rx) = mpsc::unbounded_channel();

        let mut stream = Stream::new(123, frame_tx, data_rx);

        // Send data directly to the data channel
        let test_data = Bytes::from("hello world");
        data_tx.send(test_data.clone()).unwrap();

        // Read from stream
        let mut buf = [0u8; 20];
        let n = stream.read(&mut buf).await.unwrap();
        assert_eq!(n, test_data.len());
        assert_eq!(&buf[..n], test_data.as_ref());
    }

    #[tokio::test]
    async fn test_stream_read_eof() {
        let (frame_tx, _) = mpsc::channel(1);
        let (data_tx, data_rx) = mpsc::unbounded_channel();

        let mut stream = Stream::new(123, frame_tx, data_rx);

        // Close the data channel to simulate EOF
        drop(data_tx);

        // Read should return 0 (EOF)
        let mut buf = [0u8; 20];
        let n = stream.read(&mut buf).await.unwrap();
        assert_eq!(n, 0);
    }

    #[tokio::test]
    async fn test_stream_write() {
        let (frame_tx, mut frame_rx) = mpsc::channel(1);
        let (_, data_rx) = mpsc::unbounded_channel();

        let mut stream = Stream::new(123, frame_tx, data_rx);

        // Write some data
        let test_data = b"hello world";
        let n = stream.write(test_data).await.unwrap();
        assert_eq!(n, test_data.len());

        // Verify PSH frame was sent
        let frame = frame_rx.recv().await.unwrap();
        assert_eq!(frame.cmd, Command::Psh);
        assert_eq!(frame.stream_id, 123);
        assert_eq!(frame.data.as_ref(), test_data);
    }

    #[tokio::test]
    async fn test_stream_shutdown() {
        let (frame_tx, mut frame_rx) = mpsc::channel(1);
        let (_, data_rx) = mpsc::unbounded_channel();

        let mut stream = Stream::new(123, frame_tx, data_rx);

        // Shutdown the stream
        stream.shutdown().await.unwrap();
        assert!(stream.is_write_closed());

        // Verify FIN frame was sent
        let frame = frame_rx.recv().await.unwrap();
        assert_eq!(frame.cmd, Command::Fin);
        assert_eq!(frame.stream_id, 123);
    }

    #[tokio::test]
    async fn test_stream_close() {
        let (frame_tx, mut frame_rx) = mpsc::channel(1);
        let (_, data_rx) = mpsc::unbounded_channel();

        let mut stream = Stream::new(123, frame_tx, data_rx);

        // Close the stream
        stream.close().await.unwrap();
        assert!(stream.is_write_closed());

        // Verify FIN frame was sent
        let frame = frame_rx.recv().await.unwrap();
        assert_eq!(frame.cmd, Command::Fin);
        assert_eq!(frame.stream_id, 123);
    }

    #[tokio::test]
    async fn test_stream_multiple_reads() {
        let (frame_tx, _) = mpsc::channel(1);
        let (data_tx, data_rx) = mpsc::unbounded_channel();

        let mut stream = Stream::new(123, frame_tx, data_rx);

        // Send multiple data chunks
        let data1 = Bytes::from("hello ");
        let data2 = Bytes::from("world");

        data_tx.send(data1).unwrap();
        data_tx.send(data2).unwrap();

        // Read all data
        let mut buf = [0u8; 20];
        let n1 = stream.read(&mut buf).await.unwrap();
        let n2 = stream.read(&mut buf[n1..]).await.unwrap();

        let total_len = n1 + n2;
        let expected = b"hello world";
        assert_eq!(total_len, expected.len());
        assert_eq!(&buf[..total_len], expected);
    }

    #[tokio::test]
    async fn test_stream_drop_sends_fin() {
        let (frame_tx, mut frame_rx) = mpsc::channel(1);
        let (_, data_rx) = mpsc::unbounded_channel();

        {
            let _stream = Stream::new(123, frame_tx, data_rx);
            // Stream is dropped here
        }

        // Verify FIN frame was sent on drop
        let frame = frame_rx.try_recv().unwrap();
        assert_eq!(frame.cmd, Command::Fin);
        assert_eq!(frame.stream_id, 123);
    }

    #[tokio::test]
    async fn test_stream_integration_with_session() {
        use crate::{Config, Session};
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        // Create a duplex connection for client-server communication
        let (client_transport, server_transport) = tokio::io::duplex(1024);
        let config = Config::default();

        // Create client and server sessions
        let client_session = Session::client(client_transport, config.clone())
            .await
            .unwrap();
        let server_session = Session::server(server_transport, config).await.unwrap();

        // Client opens a stream
        let mut client_stream = client_session.open_stream().await.unwrap();

        // Give some time for the SYN frame to propagate
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

        // Server accepts the stream
        let mut server_stream = server_session.accept_stream().await.unwrap().unwrap();

        // Verify stream IDs match
        assert_eq!(client_stream.stream_id(), server_stream.stream_id());

        // Client writes data
        let test_data = b"Hello from client!";
        client_stream.write_all(test_data).await.unwrap();

        // Give some time for the data to propagate
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

        // Server reads data
        let mut buffer = [0u8; 64];
        let n = server_stream.read(&mut buffer).await.unwrap();
        assert_eq!(n, test_data.len());
        assert_eq!(&buffer[..n], test_data);

        // Server writes response
        let response_data = b"Hello from server!";
        server_stream.write_all(response_data).await.unwrap();

        // Give some time for the data to propagate
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

        // Client reads response
        let mut response_buffer = [0u8; 64];
        let n = client_stream.read(&mut response_buffer).await.unwrap();
        assert_eq!(n, response_data.len());
        assert_eq!(&response_buffer[..n], response_data);

        // Close streams
        client_stream.shutdown().await.unwrap();
        server_stream.shutdown().await.unwrap();
    }
}
