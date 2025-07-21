use crate::{error::Result, frame::Frame};
use tokio::sync::mpsc;

/// A multiplexed stream within a smux session
///
/// This is a placeholder implementation for Task 04.
/// The full stream implementation will be completed in Task 05.
#[derive(Debug)]
#[allow(dead_code)] // Fields will be used in Task 05
pub struct Stream {
    pub(crate) stream_id: u32,
    pub(crate) frame_tx: mpsc::Sender<Frame>,
    pub(crate) frame_rx: mpsc::Receiver<Frame>,
}

impl Stream {
    pub(crate) fn new(
        stream_id: u32,
        frame_tx: mpsc::Sender<Frame>,
        frame_rx: mpsc::Receiver<Frame>,
    ) -> Self {
        Self {
            stream_id,
            frame_tx,
            frame_rx,
        }
    }

    /// Get the stream ID
    pub fn stream_id(&self) -> u32 {
        self.stream_id
    }

    /// Close the stream (placeholder)
    pub async fn close(&mut self) -> Result<()> {
        // TODO: Implement in Task 05
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stream_creation() {
        let (frame_tx, _) = mpsc::channel(1);
        let (_, frame_rx) = mpsc::channel(1);

        let stream = Stream::new(123, frame_tx, frame_rx);
        assert_eq!(stream.stream_id(), 123);
    }
}
