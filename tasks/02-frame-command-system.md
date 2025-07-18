# Task: Frame Structure & Command System

## Overview

Implement the core frame structure and command system that defines the smux protocol. This includes the binary frame representation, command types, and frame manipulation utilities.

## Code Design Plan

### Frame Structure (`src/frame.rs`)

*   Define a `Frame` struct to represent a smux protocol frame. It will contain `version`, `cmd`, `stream_id`, and `data` (using `bytes::Bytes`).
*   Define a `HEADER_SIZE` constant.
*   Implement methods for creating new frames, calculating sizes, and validating the frame against protocol rules (e.g., max size, valid data for a given command).

### Command System (`src/command.rs`)

*   Create a `Command` enum to represent the different frame types (`SYN`, `FIN`, `PSH`, `NOP`, `UPD`).
*   The `UPD` variant will hold `consumed` and `window` values for v2 flow control.
*   Implement methods to convert the `Command` to and from its byte representation.
*   Add helper methods to check command properties (e.g., `is_control`, `can_carry_data`).

### Stream ID Management

*   Create a `StreamIdGenerator` to produce unique stream IDs.
*   It should differentiate between client-initiated (odd numbers) and server-initiated (even numbers) streams.
*   It needs to handle potential ID overflow for long-lived sessions.

## Test Plan

### Frame Tests

*   Test `Frame` creation and property accessors.
*   Unit test the `validate` method with both valid and invalid frames (e.g., oversized, control frames with data, v1 `UPD` frames).

### Command Tests

*   Test the conversion between `Command` enum variants and their byte representations.
*   Verify the round-trip conversion for all command types.
*   Test the property helper methods (`is_control`, etc.).

### Stream ID Tests

*   Verify that the `StreamIdGenerator` produces the correct sequence of odd IDs for a client and even IDs for a server.
*   Test the overflow detection logic.
*   Test the validation of peer-initiated stream IDs.
