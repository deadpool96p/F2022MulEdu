//! Blaze framing: choosing between the two Blaze header generations and
//! turning a byte stream into packets.
//!
//! # The two generations
//!
//! EA has shipped two incompatible Blaze framings:
//!
//! ```text
//! Fire2 (16 bytes)                 legacy FIRE (12 bytes)
//! [0..4)   payloadSize   u32 BE    [0..3)   length      u24 BE
//! [4..6)   metadataSize  u16 BE    [3..5)   component   u16 BE
//! [6..8)   componentID   u16 BE    [5..7)   command     u16 BE
//! [8..10)  commandID     u16 BE    [7..9)   errorCode   u16 BE
//! [10..13) messageNumber u24 BE    [9]      type<<4|qos
//! [13]     type<<5|userIndex       [10..12) seqNo       u16 BE
//! [14]     options
//! [15]     reserved
//! ```
//!
//! **FIFA 22 speaks Fire2.** The redirector, the telemetry sink and our own
//! integration harness still speak the legacy generation, so both have to be
//! supported on the wire.
//!
//! # Deciding between them
//!
//! Both generations carry a component id and a command id, and neither is
//! ever zero. They sit at *different offsets*, so each reading of a buffer
//! yields a different pair — and for the frames a client sends, exactly one
//! reading is valid:
//!
//! * a **Fire2 request** has a payload size below 16 MiB and a metadata size
//!   below 64 KiB, so bytes 3..5 are zero and the legacy reading sees a
//!   component id of zero;
//! * a **legacy request** has an error code of zero and a message type of
//!   MESSAGE, so bytes 8..10 are zero and the Fire2 reading sees a command
//!   id of zero.
//!
//! Since a client's *first* frame on a connection is always a request, the
//! generation can be latched from the first 12 bytes with no ambiguity.
//!
//! Server-to-client frames (replies and notifications) can be read either
//! way, so those are separated by frame-length consistency and, failing
//! that, by whether the claimed payload decodes as a TDF struct that
//! consumes exactly the bytes it claims.

use std::io::{self, Cursor, Read};

use blaze_tdf::{deserialize_tdf, TdfReader, TdfStruct};

use crate::frame::{BlazeHeader, BlazePacket};

/// The largest frame we are willing to buffer, as a guard against a
/// corrupt length field making the server allocate an enormous buffer.
pub const MAX_FRAME_SIZE: usize = 16 * 1024 * 1024;

/// The highest Fire2 message type (PING_REPLY). Fire2 defines six, against
/// legacy's four.
const FIRE2_MAX_MESSAGE_TYPE: u8 = 5;

/// The highest legacy FIRE message type (ERROR_REPLY).
const LEGACY_MAX_MESSAGE_TYPE: u8 = 3;

/// Which Blaze header generation a byte stream uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Framing {
    /// The 16-byte Fire2 header that FIFA 22 speaks.
    Fire2,
    /// The 12-byte legacy FIRE header.
    Legacy,
}

/// The Fire2 frame layout, as read off the wire.
#[derive(Debug, Clone)]
pub struct Fire2Frame {
    pub header: BlazeHeader,
    /// The decoded metadata section, empty when the frame carries none.
    pub metadata: TdfStruct,
    /// The decoded payload.
    pub payload: TdfStruct,
    /// The total number of bytes this frame occupied on the wire.
    pub frame_len: usize,
}

// ---------------------------------------------------------------- header reads

/// The Fire2 header fields, read from at least 12 bytes of buffer.
#[derive(Debug, Clone, Copy)]
struct Fire2Header {
    payload_size: u32,
    metadata_size: u16,
    component: u16,
    command: u16,
    msg_type: u8,
}

fn read_fire2_header(buffer: &[u8]) -> Fire2Header {
    Fire2Header {
        payload_size: u32::from_be_bytes([buffer[0], buffer[1], buffer[2], buffer[3]]),
        metadata_size: u16::from_be_bytes([buffer[4], buffer[5]]),
        component: u16::from_be_bytes([buffer[6], buffer[7]]),
        command: u16::from_be_bytes([buffer[8], buffer[9]]),
        // Byte 13 is only present once 14 bytes have arrived.
        msg_type: if buffer.len() > 13 { buffer[13] >> 5 } else { 0 },
    }
}

/// The legacy FIRE header fields, read from 12 bytes of buffer.
#[derive(Debug, Clone, Copy)]
struct LegacyHeader {
    length: u32,
    component: u16,
    command: u16,
    msg_type: u8,
}

fn read_legacy_header(buffer: &[u8]) -> LegacyHeader {
    LegacyHeader {
        length: ((buffer[0] as u32) << 16) | ((buffer[1] as u32) << 8) | (buffer[2] as u32),
        component: u16::from_be_bytes([buffer[3], buffer[4]]),
        command: u16::from_be_bytes([buffer[5], buffer[6]]),
        msg_type: buffer[9] >> 4,
    }
}

/// The total number of bytes a Fire2 frame occupies.
fn fire2_frame_len_of(header: &Fire2Header) -> usize {
    16 + header.metadata_size as usize + header.payload_size as usize
}

/// The total number of bytes a legacy frame occupies.
fn legacy_frame_len_of(header: &LegacyHeader) -> usize {
    12 + header.length as usize
}

// ------------------------------------------------------------------ detection

/// A Fire2 reading is structurally sound when the component id and the
/// command id it finds are both non-zero, and the message type it finds in
/// the high three bits of byte 13 is a Fire2 type.
///
/// Fire2 defines six message types (MESSAGE .. PING_REPLY) against legacy's
/// four, so the type check is only applied once byte 13 has arrived.
fn fire2_structurally_valid(buffer: &[u8]) -> bool {
    let h = read_fire2_header(buffer);
    if h.component == 0 || h.command == 0 {
        return false;
    }
    if buffer.len() > 13 && h.msg_type > FIRE2_MAX_MESSAGE_TYPE {
        return false;
    }
    true
}

/// A legacy reading is structurally sound when the component id and the
/// command id it finds are both non-zero, and the message type it finds in
/// the high nibble of byte 9 is a legacy type.
fn legacy_structurally_valid(buffer: &[u8]) -> bool {
    let h = read_legacy_header(buffer);
    h.component != 0 && h.command != 0 && h.msg_type <= LEGACY_MAX_MESSAGE_TYPE
}

/// True when `bytes` decodes as a TDF struct that consumes exactly those
/// bytes.
///
/// An empty region is *not* considered consistent: an empty payload is
/// legitimate, but it proves nothing about which generation the frame uses,
/// so it must not be allowed to win a tiebreak.
fn tdf_consumes_exactly(bytes: &[u8]) -> bool {
    if bytes.is_empty() {
        return false;
    }
    let mut reader = TdfReader::new(Cursor::new(bytes));
    if reader.read_struct().is_err() {
        return false;
    }
    reader.into_inner().position() as usize == bytes.len()
}

/// Decides which framing generation a buffer uses.
///
/// Returns `None` when the buffer does not yet hold enough bytes to tell,
/// or when the evidence is genuinely inconclusive; the caller should read
/// more bytes and try again.
pub fn detect_framing(buffer: &[u8]) -> Option<Framing> {
    if buffer.len() < 12 {
        return None;
    }

    let f2_ok = fire2_structurally_valid(buffer);
    let lg_ok = legacy_structurally_valid(buffer);

    match (f2_ok, lg_ok) {
        (true, false) => return Some(Framing::Fire2),
        (false, true) => return Some(Framing::Legacy),
        (false, false) => return None,
        (true, true) => {}
    }

    // Both readings are structurally sound. This happens for server-to-client
    // frames, where the error code and the message type are not both zero.
    // Separate them on frame-length consistency first, then on whether the
    // claimed payload really decodes as a TDF struct.
    let f2 = read_fire2_header(buffer);
    let lg = read_legacy_header(buffer);
    let f2_len = fire2_frame_len_of(&f2);
    let lg_len = legacy_frame_len_of(&lg);
    let f2_fits = f2_len <= buffer.len();
    let lg_fits = lg_len <= buffer.len();

    if f2_fits && !lg_fits {
        return Some(Framing::Fire2);
    }
    if lg_fits && !f2_fits {
        return Some(Framing::Legacy);
    }

    if f2_fits && lg_fits {
        let f2_payload = &buffer[16 + f2.metadata_size as usize..f2_len];
        let lg_payload = &buffer[12..lg_len];
        let f2_consistent = tdf_consumes_exactly(f2_payload);
        let lg_consistent = tdf_consumes_exactly(lg_payload);
        match (f2_consistent, lg_consistent) {
            (true, false) => return Some(Framing::Fire2),
            (false, true) => return Some(Framing::Legacy),
            _ => {}
        }
    }

    None
}

/// True when the buffer starts a Fire2 frame.
///
/// This is a convenience wrapper around [`detect_framing`] for the cases
/// where only the Fire2 answer matters.
pub fn looks_like_fire2(buffer: &[u8]) -> bool {
    detect_framing(buffer) == Some(Framing::Fire2)
}

/// Reads the length fields out of a Fire2 header without consuming it.
pub fn fire2_frame_len(buffer: &[u8; 16]) -> usize {
    let payload_size = u32::from_be_bytes([buffer[0], buffer[1], buffer[2], buffer[3]]) as usize;
    let metadata_size = u16::from_be_bytes([buffer[4], buffer[5]]) as usize;
    16 + metadata_size + payload_size
}

// -------------------------------------------------------------------- parsing

/// Parses one Fire2 frame from the front of `buffer`.
///
/// Returns `None` when the buffer does not yet hold a whole frame; the
/// caller should read more bytes and try again. On success the frame is
/// removed from the buffer.
pub fn try_parse_fire2(buffer: &mut Vec<u8>) -> Option<Fire2Frame> {
    if buffer.len() < 16 {
        return None;
    }

    let header_slice: [u8; 16] = buffer[0..16].try_into().ok()?;
    let frame_len = fire2_frame_len(&header_slice);

    if frame_len > MAX_FRAME_SIZE {
        tracing::error!(
            "[Fire2] Refusing a {} byte frame (payloadSize={}, metadataSize={})",
            frame_len,
            u32::from_be_bytes([header_slice[0], header_slice[1], header_slice[2], header_slice[3]]),
            u16::from_be_bytes([header_slice[4], header_slice[5]]),
        );
        // Drop the connection rather than resynchronising blindly.
        buffer.clear();
        return None;
    }

    if buffer.len() < frame_len {
        return None;
    }

    let header = match BlazeHeader::from_bytes_16(&header_slice) {
        Ok(h) => h,
        Err(e) => {
            tracing::error!("[Fire2] Bad header: {:?}", e);
            buffer.drain(0..frame_len);
            return None;
        }
    };

    let metadata_size = header.metadata_size as usize;

    let metadata_bytes = &buffer[16..16 + metadata_size];
    let metadata = if metadata_bytes.is_empty() {
        TdfStruct::new()
    } else {
        deserialize_tdf(metadata_bytes).unwrap_or_else(|e| {
            tracing::warn!("[Fire2] Metadata section failed to decode: {:?}", e);
            TdfStruct::new()
        })
    };

    let payload_bytes = &buffer[16 + metadata_size..frame_len];
    let payload = if payload_bytes.is_empty() {
        TdfStruct::new()
    } else {
        deserialize_tdf(payload_bytes).unwrap_or_else(|e| {
            tracing::warn!("[Fire2] Payload failed to decode: {:?}", e);
            TdfStruct::new()
        })
    };

    buffer.drain(0..frame_len);

    Some(Fire2Frame {
        header,
        metadata,
        payload,
        frame_len,
    })
}

/// Parses one Fire2 frame and converts it into a `BlazePacket`.
///
/// The error code is lifted out of the metadata section (where Fire2 puts
/// it) and into the packet's header, so the rest of the server can keep
/// treating `header.error_code` as "did this call fail".
pub fn try_parse_fire2_packet(buffer: &mut Vec<u8>) -> Option<BlazePacket> {
    let frame = try_parse_fire2(buffer)?;
    let mut packet = BlazePacket {
        header: frame.header,
        payload: frame.payload,
    };

    // Fire2 carries the error code in the metadata section as `ERRC`.
    if let Some(blaze_tdf::TdfValue::Integer(errc)) = frame.metadata.get("ERRC") {
        packet.header.error_code = *errc as u16;
    }
    if let Some(blaze_tdf::TdfValue::Integer(cntx)) = frame.metadata.get("CNTX") {
        packet.header.context = *cntx;
    }

    Some(packet)
}

/// Parses one legacy 12-byte FIRE frame from the front of `buffer`.
pub fn try_parse_legacy_packet(buffer: &mut Vec<u8>) -> Option<BlazePacket> {
    if buffer.len() < 12 {
        return None;
    }

    let header_slice: [u8; 12] = buffer[0..12].try_into().ok()?;
    let header = match BlazeHeader::from_bytes(&header_slice) {
        Ok(h) => h,
        Err(e) => {
            tracing::error!("[Legacy] Bad header: {:?}", e);
            buffer.drain(0..12);
            return None;
        }
    };

    let payload_size = header.length as usize;
    let frame_len = 12 + payload_size;
    if frame_len > MAX_FRAME_SIZE {
        tracing::error!("[Legacy] Refusing a {} byte frame", frame_len);
        buffer.clear();
        return None;
    }
    if buffer.len() < frame_len {
        return None;
    }

    let payload_bytes = &buffer[12..frame_len];
    let payload = if payload_bytes.is_empty() {
        TdfStruct::new()
    } else {
        deserialize_tdf(payload_bytes).unwrap_or_else(|e| {
            tracing::warn!("[Legacy] Payload failed to decode: {:?}", e);
            TdfStruct::new()
        })
    };

    buffer.drain(0..frame_len);

    Some(BlazePacket { header, payload })
}

/// Parses one frame of the given generation from the front of `buffer`.
pub fn try_parse_packet(buffer: &mut Vec<u8>, framing: Framing) -> Option<BlazePacket> {
    match framing {
        Framing::Fire2 => try_parse_fire2_packet(buffer),
        Framing::Legacy => try_parse_legacy_packet(buffer),
    }
}

/// Reads exactly one Fire2 frame from a reader.
pub fn read_fire2_frame<R: Read>(reader: &mut R) -> io::Result<Fire2Frame> {
    let mut header_slice = [0u8; 16];
    reader.read_exact(&mut header_slice)?;

    let frame_len = fire2_frame_len(&header_slice);
    if frame_len > MAX_FRAME_SIZE {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("Fire2 frame of {} bytes is too large", frame_len),
        ));
    }

    let mut rest = vec![0u8; frame_len - 16];
    reader.read_exact(&mut rest)?;

    let mut buffer = Vec::with_capacity(frame_len);
    buffer.extend_from_slice(&header_slice);
    buffer.extend_from_slice(&rest);

    try_parse_fire2(&mut buffer).ok_or_else(|| {
        io::Error::new(io::ErrorKind::InvalidData, "malformed Fire2 frame")
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frame::MessageType;

    /// The header of an authentic EA `Util::preAuth` reply.
    ///
    /// The full frames are kept as fixtures in
    /// `crates/services-blaze/tests/fixtures/`.
    const AUTHENTIC_EA_REPLY_HEADER: [u8; 16] = [
        0x00, 0x00, 0x05, 0x6E, 0x00, 0x00, 0x00, 0x09, 0x00, 0x07, 0x00, 0x00, 0x01, 0x20, 0x00, 0x00,
    ];

    /// The authentic EA `Util::ping` request, byte for byte, as recorded in
    /// `docs/POST_PING_STALL_ROOT_CAUSE.md`.
    const AUTHENTIC_EA_PING_REQUEST: [u8; 16] = [
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x09, 0x00, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    ];

    /// Builds a 12-byte legacy FIRE frame.
    fn legacy_frame(component: u16, command: u16, msg_type: u8, payload: &[u8]) -> Vec<u8> {
        let mut buf = vec![0u8; 12];
        buf[0..3].copy_from_slice(&(payload.len() as u32).to_be_bytes()[1..]);
        buf[3..5].copy_from_slice(&component.to_be_bytes());
        buf[5..7].copy_from_slice(&command.to_be_bytes());
        buf[9] = msg_type << 4;
        buf.extend_from_slice(payload);
        buf
    }

    #[test]
    fn fire2_frame_length_is_header_plus_metadata_plus_payload() {
        // The authentic EA reply: 16-byte header + 1390 bytes of payload.
        assert_eq!(fire2_frame_len(&AUTHENTIC_EA_REPLY_HEADER), 16 + 1390);
    }

    #[test]
    fn a_frame_with_metadata_is_longer_than_its_payload_says() {
        // payloadSize = 2, metadataSize = 4.
        let mut header = [0u8; 16];
        header[0..4].copy_from_slice(&2u32.to_be_bytes());
        header[4..6].copy_from_slice(&4u16.to_be_bytes());
        assert_eq!(fire2_frame_len(&header), 16 + 4 + 2);
    }

    #[test]
    fn an_authentic_ea_reply_is_recognised_as_fire2() {
        let mut buffer = AUTHENTIC_EA_REPLY_HEADER.to_vec();
        buffer.extend_from_slice(&[0u8; 1390]);
        assert_eq!(
            detect_framing(&buffer),
            Some(Framing::Fire2),
            "an authentic EA reply is Fire2"
        );
    }

    #[test]
    fn an_authentic_ea_request_is_recognised_as_fire2() {
        assert_eq!(
            detect_framing(&AUTHENTIC_EA_PING_REQUEST),
            Some(Framing::Fire2),
            "an authentic EA Util::ping request is Fire2"
        );
    }

    #[test]
    fn a_legacy_frame_is_recognised_as_legacy() {
        for (component, command) in [
            (0x0005u16, 0x0001u16), // redirector getServerInstance
            (0x0009, 0x0002),       // util ping
            (0x0009, 0x0007),       // util preAuth
            (0x0001, 0x00E6),       // auth createWalUserSession
            (0x7802, 0x0008),       // usersessions updateNetworkInfo
            (0x00FE, 0x0022),       // wildcard unknown RPC
        ] {
            let frame = legacy_frame(component, command, 0, &[]);
            assert_eq!(
                detect_framing(&frame),
                Some(Framing::Legacy),
                "a legacy 0x{:04X}:0x{:04X} request is legacy",
                component,
                command
            );
        }
    }

    #[test]
    fn a_legacy_frame_with_a_payload_is_recognised_as_legacy() {
        let frame = legacy_frame(0x0009, 0x0002, 0, &[0xCF, 0x4A, 0x6D, 0x00]);
        assert_eq!(detect_framing(&frame), Some(Framing::Legacy));
    }

    #[test]
    fn the_old_byte_10_heuristic_would_have_misread_a_high_message_number() {
        // A Fire2 frame with message number 0x04_0000. The old heuristic
        // read byte 10 as a message type, so it would have rejected this.
        let mut header = AUTHENTIC_EA_REPLY_HEADER;
        header[10] = 0x04; // message number 0x04_0000
        let mut buffer = header.to_vec();
        buffer.extend_from_slice(&[0u8; 1390]);
        assert_eq!(
            detect_framing(&buffer),
            Some(Framing::Fire2),
            "a Fire2 frame with a large message number is still Fire2"
        );
    }

    #[test]
    fn a_partial_buffer_is_not_yet_decidable() {
        assert_eq!(detect_framing(&[]), None);
        assert_eq!(detect_framing(&[0x00, 0x00, 0x00]), None);
    }

    #[test]
    fn parsing_consumes_the_whole_frame_including_the_metadata() {
        // Build a Fire2 frame by hand: an error reply with metadata.
        let mut header = [0u8; 16];
        header[0..4].copy_from_slice(&0u32.to_be_bytes()); // payloadSize
        header[4..6].copy_from_slice(&0u16.to_be_bytes()); // metadataSize
        header[6..8].copy_from_slice(&0x0009u16.to_be_bytes());
        header[8..10].copy_from_slice(&0x0002u16.to_be_bytes());
        header[13] = (MessageType::ErrorResponse as u8) << 5;

        // The real metadata section: CNTX and ERRC.
        let mut metadata = TdfStruct::new();
        metadata.set("CNTX", blaze_tdf::TdfValue::Integer(0));
        metadata.set("ERRC", blaze_tdf::TdfValue::Integer(0x4001));
        let metadata_bytes = blaze_tdf::serialize_tdf(&metadata).expect("metadata");
        header[4..6].copy_from_slice(&(metadata_bytes.len() as u16).to_be_bytes());

        let mut buffer = Vec::new();
        buffer.extend_from_slice(&header);
        buffer.extend_from_slice(&metadata_bytes);

        let packet = try_parse_fire2_packet(&mut buffer).expect("frame must parse");
        assert_eq!(packet.header.error_code, 0x4001, "ERRC is lifted into the header");
        assert_eq!(packet.header.metadata_size, metadata_bytes.len() as u16);
        assert!(buffer.is_empty(), "the whole frame is consumed");
    }

    #[test]
    fn a_partial_frame_is_not_parsed() {
        let mut buffer = AUTHENTIC_EA_REPLY_HEADER.to_vec();
        buffer.extend_from_slice(&[0u8; 100]); // only 100 of 1390 payload bytes
        assert!(try_parse_fire2_packet(&mut buffer).is_none());
        assert_eq!(buffer.len(), 116, "the buffer is left untouched");
    }

    #[test]
    fn a_legacy_frame_round_trips_through_the_legacy_parser() {
        let frame = legacy_frame(0x0009, 0x0002, 0, &[]);
        let mut buffer = frame.clone();
        let packet = try_parse_legacy_packet(&mut buffer).expect("legacy frame must parse");
        assert_eq!(packet.header.component, 0x0009);
        assert_eq!(packet.header.command, 0x0002);
        assert_eq!(packet.header.msg_type, MessageType::SubmitRequest);
        assert!(buffer.is_empty(), "the whole frame is consumed");
    }

    #[test]
    fn a_truncated_buffer_is_never_misdetected() {
        // Whatever the buffer length, the detection must never claim the
        // wrong generation for a frame it has not fully seen.
        let fire2 = {
            let mut b = AUTHENTIC_EA_REPLY_HEADER.to_vec();
            b.extend_from_slice(&[0u8; 1390]);
            b
        };
        for n in 1..=fire2.len() {
            if let Some(got) = detect_framing(&fire2[..n]) {
                assert_eq!(got, Framing::Fire2, "misdetected at {} bytes", n);
            }
        }

        let legacy = legacy_frame(0x0009, 0x0002, 0, &[0xCF, 0x4A, 0x6D, 0x00]);
        for n in 1..=legacy.len() {
            if let Some(got) = detect_framing(&legacy[..n]) {
                assert_eq!(got, Framing::Legacy, "misdetected at {} bytes", n);
            }
        }
    }
}
