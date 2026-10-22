//! Fire2 frame-header conformance tests.
//!
//! FIFA 22 speaks EA's "Fire2" generation: a 16-byte frame header followed by
//! an optional metadata section and then the Heat2-encoded payload.
//!
//! The layout below is not a guess. It is pinned by three independent pieces
//! of evidence:
//!
//! 1. **The real FIFA 22 client.** Its very first request, captured off the
//!    wire, is `00 00 00 00 00 00 00 09 00 02 00 00 00 00 00 00` — a
//!    `Util::ping` (component `0x0009`, command `0x0002`).
//!
//! 2. **Authentic EA traffic.** Two complete `Util::fetchClientConfig` reply
//!    frames captured from a live EA client, which decode byte-exactly under
//!    this layout and consume their bytes exactly.
//!
//! 3. **The reference decoder** in `mxmstr/hp-server`, whose Fire2 header
//!    agrees byte-for-byte with (1) and (2).
//!
//! ```text
//! [0..4)   payloadSize    u32 big-endian   -- payload bytes only
//! [4..6)   metadataSize   u16 big-endian   -- 0 when there is no metadata
//! [6..8)   componentID    u16 big-endian
//! [8..10)  commandID      u16 big-endian
//! [10..13) messageNumber  u24 big-endian
//! [13]     messageType << 5 | userIndex & 0x1F
//! [14]     options        (OPTION_IMMEDIATE = 0x01)
//! [15]     reserved       (0)
//! ```
//!
//! Note that **there is no error code in the Fire2 header**. The error code
//! lives in the metadata section, as the TDF field `ERRC`.

use blaze_core::{BlazeHeader, BlazePacket, MessageType};

/// The header of the first request the real FIFA 22 client ever sends.
const CLIENT_FIRST_REQUEST: [u8; 16] = [
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x09, 0x00, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
];

/// The header of an authentic EA `Util::preAuth` reply, taken from a live
/// client capture. The payload behind it is 1390 bytes.
///
/// This frame was previously labelled a `Util::fetchClientConfig` reply.
/// That was wrong: `0x0007` is `preAuth`. The full frames are kept as
/// fixtures in `crates/services-blaze/tests/fixtures/`, and
/// `crates/services-blaze/tests/preauth_matches_ea.rs` decodes them.
const AUTHENTIC_EA_REPLY_HEADER: [u8; 16] = [
    0x00, 0x00, 0x05, 0x6E, 0x00, 0x00, 0x00, 0x09, 0x00, 0x07, 0x00, 0x00, 0x01, 0x20, 0x00, 0x00,
];

#[test]
fn fire2_header_decodes_the_captured_client_ping_request() {
    let header = BlazeHeader::from_bytes_16(&CLIENT_FIRST_REQUEST).expect("must parse");

    assert_eq!(header.component, 0x0009, "componentID must be Util");
    assert_eq!(header.command, 0x0002, "commandID must be Util::ping");
    assert_eq!(header.length, 0, "ping has no request payload");
    assert_eq!(header.metadata_size, 0, "ping carries no metadata");
    assert_eq!(header.message_number, 0, "first message in the session");
    assert_eq!(header.msg_type, MessageType::SubmitRequest, "MESSAGE");
    assert_eq!(header.user_index, 0);
    assert_eq!(header.options, 0);
}

#[test]
fn fire2_header_decodes_the_authentic_ea_reply_header() {
    let header = BlazeHeader::from_bytes_16(&AUTHENTIC_EA_REPLY_HEADER).expect("must parse");

    assert_eq!(header.length, 1390, "payloadSize of the captured reply");
    assert_eq!(header.metadata_size, 0);
    assert_eq!(header.component, 0x0009, "Util");
    assert_eq!(header.command, 0x0007, "preAuth");
    assert_eq!(header.message_number, 1, "message number is a u24 at bytes 10..13");
    assert_eq!(header.msg_type, MessageType::RecordResponse, "REPLY");
    assert_eq!(header.user_index, 0);
    assert_eq!(header.options, 0);
}

#[test]
fn fire2_header_encodes_the_authentic_ea_reply_header_byte_for_byte() {
    // Rebuild the captured header from its fields. Every byte must match.
    let header = BlazeHeader::new_fire2(
        1390,
        0x0009,
        0x0007,
        0,
        MessageType::RecordResponse,
        1,
        0,
    );

    assert_eq!(
        header.to_bytes_16(),
        AUTHENTIC_EA_REPLY_HEADER,
        "Fire2 header must encode byte-for-byte like the authentic EA capture"
    );
}

#[test]
fn fire2_header_puts_the_message_number_in_bytes_10_to_13() {
    let header = BlazeHeader::new_fire2(0, 0x0009, 0x0002, 0, MessageType::SubmitRequest, 0, 0);
    let bytes = header.to_bytes_16();

    // A u24 big-endian message number.
    assert_eq!(&bytes[10..13], &[0x00, 0x00, 0x00]);
    assert_eq!(bytes[13], 0x00, "MESSAGE (0) shifted left 5");
}

#[test]
fn fire2_header_carries_a_24_bit_message_number() {
    let header = BlazeHeader::new_fire2(0, 0x0009, 0x0002, 0, MessageType::SubmitRequest, 0, 0);
    let mut bytes = header.to_bytes_16();
    bytes[10] = 0x00;
    bytes[11] = 0x00;
    bytes[12] = 0x05;

    let parsed = BlazeHeader::from_bytes_16(&bytes).expect("must parse");
    assert_eq!(parsed.message_number, 5);

    // The top bit of the 24-bit field must not be confused with the type byte.
    bytes[10] = 0xFF;
    bytes[11] = 0xFF;
    bytes[12] = 0xFF;
    let parsed = BlazeHeader::from_bytes_16(&bytes).expect("must parse");
    assert_eq!(parsed.message_number, 0x00FF_FFFF);
}

#[test]
fn fire2_header_packs_message_type_and_user_index_into_byte_13() {
    // (type << 5) | (userIndex & 0x1F)
    let header = BlazeHeader::new_fire2(0, 0x0009, 0x0002, 0, MessageType::RecordResponse, 0, 3);
    let bytes = header.to_bytes_16();
    assert_eq!(bytes[13], (1 << 5) | 3, "REPLY with user index 3");

    let parsed = BlazeHeader::from_bytes_16(&bytes).expect("must parse");
    assert_eq!(parsed.msg_type, MessageType::RecordResponse);
    assert_eq!(parsed.user_index, 3);
}

#[test]
fn fire2_header_roundtrips_every_field() {
    let header = BlazeHeader::new_fire2(
        0x1234,
        0x0004,
        0x00AB,
        0,
        MessageType::Notification,
        0x00BEEF,
        0x1F,
    );

    let parsed = BlazeHeader::from_bytes_16(&header.to_bytes_16()).expect("must parse");

    assert_eq!(parsed.length, 0x1234);
    assert_eq!(parsed.metadata_size, 0);
    assert_eq!(parsed.component, 0x0004);
    assert_eq!(parsed.command, 0x00AB);
    assert_eq!(parsed.message_number, 0x00BEEF);
    assert_eq!(parsed.msg_type, MessageType::Notification);
    assert_eq!(parsed.user_index, 0x1F);
}

#[test]
fn fire2_header_uses_bytes_4_to_6_for_the_metadata_size_not_an_error_code() {
    // The legacy layout put an error code at bytes 4..6. In Fire2 those two
    // bytes are the metadata size, and the error code lives in the metadata
    // section as the TDF field `ERRC`.
    let mut bytes = BlazeHeader::new_fire2(0, 0x0009, 0x0002, 0, MessageType::SubmitRequest, 0, 0)
        .to_bytes_16();
    bytes[4] = 0x00;
    bytes[5] = 0x1C; // metadataSize = 28

    let parsed = BlazeHeader::from_bytes_16(&bytes).expect("must parse");
    assert_eq!(parsed.metadata_size, 28, "bytes 4..6 are the metadata size");
    assert_eq!(parsed.error_code, 0, "the header carries no error code");
}

#[test]
fn fire2_error_replies_carry_the_error_code_in_the_metadata_section() {
    let packet = BlazePacket::new(
        0x0009,
        0x0002,
        0x4001,
        MessageType::ErrorResponse,
        0,
        7,
        blaze_tdf::TdfStruct::new(),
    )
    .expect("packet");

    let encoded = packet.encode_16().expect("encode_16");

    // The header must NOT contain the error code. Bytes 4..6 hold the metadata
    // size, so they must not be the error code 0x4001.
    let metadata_size = u16::from_be_bytes([encoded[4], encoded[5]]) as usize;
    assert_ne!(
        &encoded[4..6],
        &[0x40, 0x01],
        "the error code must not be smuggled into the header"
    );
    assert!(
        metadata_size > 0,
        "an error reply must carry metadata, got size {}",
        metadata_size
    );
    assert_eq!(
        encoded[13],
        (MessageType::ErrorResponse as u8) << 5,
        "byte 13 carries the message type"
    );

    // The error code must be in the metadata, as the TDF field `ERRC`.
    let metadata = &encoded[16..16 + metadata_size];
    assert!(
        metadata.windows(3).any(|w| w == [0x97, 0x2C, 0xA3]),
        "metadata must contain the ERRC tag, got {:02X?}",
        metadata
    );
    // ...followed by the INTEGER type byte and the varsize-encoded 0x4001.
    assert!(
        metadata.windows(5).any(|w| w == [0x97, 0x2C, 0xA3, 0x00, 0x81]),
        "metadata must contain the ERRC field carrying 0x4001, got {:02X?}",
        metadata
    );
}

#[test]
fn fire2_success_replies_carry_no_metadata() {
    let packet = BlazePacket::new(
        0x0009,
        0x0002,
        0,
        MessageType::RecordResponse,
        0,
        7,
        blaze_tdf::TdfStruct::new(),
    )
    .expect("packet");

    let encoded = packet.encode_16().expect("encode_16");
    assert_eq!(&encoded[4..6], &[0x00, 0x00], "no metadata on a success reply");
    assert_eq!(encoded.len(), 16, "an empty payload produces a bare header");
}

#[test]
fn fire2_payload_size_counts_the_payload_only() {
    // The captured EA reply has payloadSize == 1390 and no metadata, and the
    // frame is 16 + 1390 bytes long.
    let packet = BlazePacket::new(
        0x0009,
        0x0007,
        0,
        MessageType::RecordResponse,
        0,
        1,
        {
            let mut s = blaze_tdf::TdfStruct::new();
            s.set("ASRC", blaze_tdf::TdfValue::String("318307".to_string()));
            s
        },
    )
    .expect("packet");

    let encoded = packet.encode_16().expect("encode_16");
    let payload_size = u32::from_be_bytes([encoded[0], encoded[1], encoded[2], encoded[3]]) as usize;
    let metadata_size = u16::from_be_bytes([encoded[4], encoded[5]]) as usize;

    assert_eq!(metadata_size, 0, "a success reply carries no metadata");
    assert_eq!(
        payload_size,
        encoded.len() - 16,
        "payloadSize is the payload length"
    );
    assert_eq!(
        encoded.len(),
        16 + metadata_size + payload_size,
        "the frame is header + metadata + payload"
    );
}

#[test]
fn legacy_twelve_byte_header_is_unchanged() {
    // The 12-byte "legacy FIRE" generation is a different protocol. It must
    // keep its exact layout: the redirector, the telemetry sink and the
    // integration harness all depend on it.
    let header = BlazeHeader::new(1024, 0x0004, 0x0001, 0x0000, MessageType::SubmitRequest, 0, 42);
    let bytes = header.to_bytes();

    assert_eq!(bytes.len(), 12);
    assert_eq!(&bytes[0..3], &[0x00, 0x04, 0x00], "24-bit length");
    assert_eq!(&bytes[3..5], &[0x00, 0x04], "component");
    assert_eq!(&bytes[5..7], &[0x00, 0x01], "command");
    assert_eq!(&bytes[7..9], &[0x00, 0x00], "error code");
    assert_eq!(bytes[9], 0x00, "message type in the high nibble");
    assert_eq!(&bytes[10..12], &[0x00, 0x2A], "sequence number");
}
