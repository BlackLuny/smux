# Task: Codec Implementation

## Overview

Implement the binary codec for encoding and decoding smux frames. This component will translate between the in-memory `Frame` struct and the byte stream format used on the wire, using `tokio-util`.

## Code Design Plan

### Core Codec (`src/codec.rs`)

*   Create a `Codec` struct.
*   Implement the `tokio_util::codec::Decoder` trait for the `Codec`.
    *   The `decode` method will read the header, determine the full frame length, and wait for enough bytes to be buffered.
    *   It will parse the header fields, including version, command, and stream ID.
    *   It will handle parsing the data payload, with special logic for the `UPD` command's embedded values.
*   Implement the `tokio_util::codec::Encoder` trait for the `Codec`.
    *   The `encode` method will write the frame's header and data into a `BytesMut` buffer.
    *   It will ensure correct byte order (Little Endian).
    *   It will handle serializing the `UPD` command's `consumed` and `window` values into the data payload for v2 frames.

## Test Plan

### Core Codec Tests

*   Test the encode/decode round trip for all frame types (`SYN`, `FIN`, `PSH`, `UPD`).
*   Test decoding with partial data to ensure the codec waits for a full frame (e.g., incomplete header, incomplete data payload).
*   Test decoding a buffer that contains multiple complete frames.
*   Test error conditions, such as invalid protocol versions or oversized frames.
*   Verify the correct handling of Little Endian byte order.

### Integration Tests

*   Use a mock I/O source (like `tokio_test::io::Builder`) to test the codec with a `FramedRead` transport.
*   Test partial reads at the I/O level to ensure the codec behaves correctly.
*   Verify that codec errors are propagated correctly through the `Framed` transport.

### Property-Based Tests

*   Use a library like `proptest` to generate a wide variety of frames.
*   For each generated frame, verify that the encode-then-decode process results in the original frame, ensuring robustness against unexpected inputs.
