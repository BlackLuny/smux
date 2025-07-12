# 003: Frame and Command Specification

This document details the structure of a `smux` frame and the commands it can carry.

## 1. Frame Structure

A `smux` frame is the atomic unit of data transfer. Every frame consists of a fixed-size header followed by a variable-length data payload.

### 1.1. Binary Layout

```
 0                   1                   2                   3
 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|    Version    |      Cmd      |            Length             |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|                           Stream ID                           |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|                                                               |
+                          Data (optional)                      +
|                                                               |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
```

*   **Version (1 byte)**: The protocol version. The library will support versions 1 and 2.
*   **Cmd (1 byte)**: The command of the frame. See section 2 for details.
*   **Length (2 bytes)**: The length of the `Data` field, in little-endian format.
*   **Stream ID (4 bytes)**: The ID of the stream this frame belongs to, in little-endian format.
*   **Data (variable)**: The payload of the frame. Its length is defined by the `Length` field.

### 1.2. Rust Representation

The `Frame` will be represented in Rust as follows:

```rust
pub struct Frame {
    pub version: u8,
    pub cmd: Command,
    pub stream_id: u32,
    pub data: Bytes, // Using Bytes for efficient slicing
}
```

## 2. Commands

The `Cmd` field in the frame header determines the frame's purpose.

### 2.1. Rust Representation

The `Command` enum will represent the possible commands:

```rust
pub enum Command {
    /// (0x00) Synchronize: Sent to initiate a new stream.
    Syn,
    /// (0x01) Finish: Sent to close a stream.
    Fin,
    /// (0x02) Push: Sent to transmit data.
    Psh,
    /// (0x03) No-op: Sent for keep-alive messages.
    Nop,
    /// (0x04) Update: Sent to update the stream's receive window (v2 only).
    Upd {
        /// Number of bytes the peer has consumed.
        consumed: u32,
        /// The new window size.
        window: u32,
    },
}
```

### 2.2. Command Details

*   **`SYN` (Synchronize)**
    *   **Purpose**: To open a new stream.
    *   **Direction**: Client -> Server or Server -> Client.
    *   **Data**: The `Data` field is empty.
    *   **Behavior**: Upon receiving a `SYN`, the peer creates a new `Stream` with the given `Stream ID` and places it in the accept queue.

*   **`FIN` (Finish)**
    *   **Purpose**: To close one direction of a stream (half-close).
    *   **Direction**: Client -> Server or Server -> Client.
    *   **Data**: The `Data` field is empty.
    *   **Behavior**: The receiving `Stream` is marked as "read-closed". No more data will be received on this stream. The peer can still send data until it also sends a `FIN`.

*   **`PSH` (Push)**
    *   **Purpose**: To transmit data over a stream.
    *   **Direction**: Client -> Server or Server -> Client.
    *   **Data**: The `Data` field contains the application data.
    *   **Behavior**: The received data is pushed into the `Stream`'s read buffer.

*   **`NOP` (No Operation)**
    *   **Purpose**: Used for keep-alive messages to prevent the underlying connection from timing out.
    *   **Direction**: Client -> Server or Server -> Client.
    *   **Data**: The `Data` field is empty.
    *   **Behavior**: The receiver simply ignores this frame. It serves to confirm the connection is still active.

*   **`UPD` (Update)**
    *   **Purpose**: To implement stream-level flow control (only in protocol v2).
    *   **Direction**: Client -> Server or Server -> Client.
    *   **Data**: An 8-byte payload:
        *   **Consumed (4 bytes)**: The total number of bytes consumed by the receiver on this stream.
        *   **Window (4 bytes)**: The receiver's current buffer capacity for this stream.
    *   **Behavior**: The sender updates its send window for the stream, allowing it to send more data.
