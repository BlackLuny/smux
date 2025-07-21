use crate::error::{Result, SmuxError};
use std::sync::atomic::{AtomicU32, Ordering};

#[derive(Debug)]
pub struct StreamIdGenerator {
    next_id: AtomicU32,
    is_client: bool,
}

impl StreamIdGenerator {
    pub fn new(is_client: bool) -> Self {
        let initial_id = if is_client { 1 } else { 2 };
        Self {
            next_id: AtomicU32::new(initial_id),
            is_client,
        }
    }

    pub fn next(&self) -> Result<u32> {
        let current = self.next_id.load(Ordering::Relaxed);

        // Check for overflow before incrementing
        if current > u32::MAX - 2 {
            return Err(SmuxError::ProtocolViolation(
                "Stream ID overflow - session should be restarted".to_string(),
            ));
        }

        let next = current + 2;

        // Store the next ID for the following call
        self.next_id.store(next, Ordering::Relaxed);

        Ok(current)
    }

    pub fn validate_peer_stream_id(&self, stream_id: u32) -> Result<()> {
        if stream_id == 0 {
            return Err(SmuxError::InvalidStreamId(stream_id));
        }

        let expected_parity = if self.is_client { 0 } else { 1 };
        let actual_parity = stream_id % 2;

        if actual_parity != expected_parity {
            return Err(SmuxError::InvalidStreamId(stream_id));
        }

        Ok(())
    }

    pub fn validate_own_stream_id(&self, stream_id: u32) -> Result<()> {
        if stream_id == 0 {
            return Err(SmuxError::InvalidStreamId(stream_id));
        }

        let expected_parity = if self.is_client { 1 } else { 0 };
        let actual_parity = stream_id % 2;

        if actual_parity != expected_parity {
            return Err(SmuxError::InvalidStreamId(stream_id));
        }

        Ok(())
    }

    pub fn is_client_initiated(&self, stream_id: u32) -> bool {
        stream_id % 2 == 1
    }

    pub fn is_server_initiated(&self, stream_id: u32) -> bool {
        stream_id % 2 == 0
    }

    pub fn reset(&self) {
        let initial_id = if self.is_client { 1 } else { 2 };
        self.next_id.store(initial_id, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_client_stream_id_generation() {
        let generator = StreamIdGenerator::new(true);

        // Client should generate odd IDs starting from 1
        assert_eq!(generator.next().unwrap(), 1);
        assert_eq!(generator.next().unwrap(), 3);
        assert_eq!(generator.next().unwrap(), 5);
        assert_eq!(generator.next().unwrap(), 7);
    }

    #[test]
    fn test_server_stream_id_generation() {
        let generator = StreamIdGenerator::new(false);

        // Server should generate even IDs starting from 2
        assert_eq!(generator.next().unwrap(), 2);
        assert_eq!(generator.next().unwrap(), 4);
        assert_eq!(generator.next().unwrap(), 6);
        assert_eq!(generator.next().unwrap(), 8);
    }

    #[test]
    fn test_stream_id_overflow() {
        let generator = StreamIdGenerator::new(true);

        // Set to near overflow
        generator.next_id.store(u32::MAX - 1, Ordering::Relaxed);

        // This should fail due to overflow
        assert!(generator.next().is_err());
    }

    #[test]
    fn test_peer_stream_id_validation() {
        let client_generator = StreamIdGenerator::new(true);
        let server_generator = StreamIdGenerator::new(false);

        // Client should accept server-initiated (even) IDs
        assert!(client_generator.validate_peer_stream_id(2).is_ok());
        assert!(client_generator.validate_peer_stream_id(4).is_ok());
        assert!(client_generator.validate_peer_stream_id(100).is_ok());

        // Client should reject client-initiated (odd) peer IDs
        assert!(client_generator.validate_peer_stream_id(1).is_err());
        assert!(client_generator.validate_peer_stream_id(3).is_err());
        assert!(client_generator.validate_peer_stream_id(99).is_err());

        // Server should accept client-initiated (odd) IDs
        assert!(server_generator.validate_peer_stream_id(1).is_ok());
        assert!(server_generator.validate_peer_stream_id(3).is_ok());
        assert!(server_generator.validate_peer_stream_id(99).is_ok());

        // Server should reject server-initiated (even) peer IDs
        assert!(server_generator.validate_peer_stream_id(2).is_err());
        assert!(server_generator.validate_peer_stream_id(4).is_err());
        assert!(server_generator.validate_peer_stream_id(100).is_err());

        // Both should reject stream ID 0
        assert!(client_generator.validate_peer_stream_id(0).is_err());
        assert!(server_generator.validate_peer_stream_id(0).is_err());
    }

    #[test]
    fn test_own_stream_id_validation() {
        let client_generator = StreamIdGenerator::new(true);
        let server_generator = StreamIdGenerator::new(false);

        // Client should accept own (odd) IDs
        assert!(client_generator.validate_own_stream_id(1).is_ok());
        assert!(client_generator.validate_own_stream_id(3).is_ok());
        assert!(client_generator.validate_own_stream_id(99).is_ok());

        // Client should reject server (even) IDs as own
        assert!(client_generator.validate_own_stream_id(2).is_err());
        assert!(client_generator.validate_own_stream_id(4).is_err());
        assert!(client_generator.validate_own_stream_id(100).is_err());

        // Server should accept own (even) IDs
        assert!(server_generator.validate_own_stream_id(2).is_ok());
        assert!(server_generator.validate_own_stream_id(4).is_ok());
        assert!(server_generator.validate_own_stream_id(100).is_ok());

        // Server should reject client (odd) IDs as own
        assert!(server_generator.validate_own_stream_id(1).is_err());
        assert!(server_generator.validate_own_stream_id(3).is_err());
        assert!(server_generator.validate_own_stream_id(99).is_err());

        // Both should reject stream ID 0
        assert!(client_generator.validate_own_stream_id(0).is_err());
        assert!(server_generator.validate_own_stream_id(0).is_err());
    }

    #[test]
    fn test_stream_id_classification() {
        let generator = StreamIdGenerator::new(true);

        // Test client-initiated detection
        assert!(generator.is_client_initiated(1));
        assert!(generator.is_client_initiated(3));
        assert!(generator.is_client_initiated(99));
        assert!(!generator.is_client_initiated(2));
        assert!(!generator.is_client_initiated(4));
        assert!(!generator.is_client_initiated(100));

        // Test server-initiated detection
        assert!(generator.is_server_initiated(2));
        assert!(generator.is_server_initiated(4));
        assert!(generator.is_server_initiated(100));
        assert!(!generator.is_server_initiated(1));
        assert!(!generator.is_server_initiated(3));
        assert!(!generator.is_server_initiated(99));
    }

    #[test]
    fn test_generator_reset() {
        let generator = StreamIdGenerator::new(true);

        // Generate some IDs
        assert_eq!(generator.next().unwrap(), 1);
        assert_eq!(generator.next().unwrap(), 3);

        // Reset and verify it starts over
        generator.reset();
        assert_eq!(generator.next().unwrap(), 1);
        assert_eq!(generator.next().unwrap(), 3);
    }

    #[test]
    fn test_concurrent_id_generation() {
        use std::sync::Arc;
        use std::thread;

        let generator = Arc::new(StreamIdGenerator::new(true));
        let mut handles = vec![];

        // Spawn multiple threads to generate IDs concurrently
        for _ in 0..10 {
            let generator_clone = Arc::clone(&generator);
            let handle = thread::spawn(move || {
                let mut ids = Vec::new();
                for _ in 0..10 {
                    if let Ok(id) = generator_clone.next() {
                        ids.push(id);
                    }
                }
                ids
            });
            handles.push(handle);
        }

        // Collect all generated IDs
        let mut all_ids = Vec::new();
        for handle in handles {
            let ids = handle.join().unwrap();
            all_ids.extend(ids);
        }

        // Verify all IDs are unique and odd (client-initiated)
        all_ids.sort();
        all_ids.dedup();

        for id in &all_ids {
            assert_eq!(id % 2, 1, "All client IDs should be odd");
        }

        // Should have generated unique IDs
        assert!(all_ids.len() > 50, "Should generate many unique IDs");
    }
}
