//! Heat2 TDF wire-format conformance tests.
//!
//! FIFA 22 speaks BlazeSDK 15, whose payload codec is "Heat2". Heat2 integers are
//! *varsize* encoded and are NOT plain LEB128:
//!
//! * the first byte carries only 6 value bits (`value & 0x3F`),
//! * bit `0x40` of the first byte is the **negative** flag,
//! * bit `0x80` of the first byte means "more bytes follow",
//! * every continuation byte carries 7 value bits at shifts 6, 13, 20, 27, ...
//!
//! A plain LSB-first 7-bit varint (the scheme used by the older 12-byte FIRE
//! generation) is decoded by Heat2 as a *different* number, silently.
//
//! The reference vectors below are the "varsize" scheme implemented by
//! `mxmstr/hp-server` (`EncodeVarsizeInteger`) and by `Khysnik/BlazeSDK`
//! (`DecodeVarsizeInteger`), both of which are independent clean-room
//! implementations of the same EA codec.

use blaze_tdf::{
    read_varint_i64, read_varint_u64, serialize_tdf, write_varint_i64, write_varint_u64,
    TdfStruct, TdfValue,
};

/// Encodes a value with the Heat2 varsize scheme (reference implementation,
/// transcribed from `mxmstr/hp-server`'s `EncodeVarsizeInteger`).
fn reference_varsize(value: i64) -> Vec<u8> {
    let negative = value < 0;
    let mut remaining = value.unsigned_abs();
    let mut first_byte = (remaining & 0x3F) as u8;
    remaining >>= 6;
    if negative {
        first_byte |= 0x40;
    }
    if remaining > 0 {
        first_byte |= 0x80;
    }
    let mut out = vec![first_byte];
    while remaining > 0 {
        let mut next_byte = (remaining & 0x7F) as u8;
        remaining >>= 7;
        if remaining > 0 {
            next_byte |= 0x80;
        }
        out.push(next_byte);
    }
    out
}

#[test]
fn varsize_matches_reference_vectors() {
    let cases: &[(i64, &[u8])] = &[
        (0, &[0x00]),
        (1, &[0x01]),
        (2, &[0x02]),
        (62, &[0x3E]),
        (63, &[0x3F]),
        // First byte is exhausted -> continuation bit set, next byte holds bit 6.
        (64, &[0x80, 0x01]),
        (127, &[0xBF, 0x01]),
        (128, &[0x80, 0x02]),
        (255, &[0xBF, 0x03]),
        (256, &[0x80, 0x04]),
        (16383, &[0xBF, 0xFF, 0x01]),
        (16384, &[0x80, 0x80, 0x02]),
        (1_792_499_378, &[0xB2, 0xFA, 0xBA, 0xAD, 0x0D]),
        (4_294_967_296, &[0x80, 0x80, 0x80, 0x80, 0x20]),
        (1_099_511_627_776, &[0x80, 0x80, 0x80, 0x80, 0x80, 0x40]),
        // Negative flag lives in bit 0x40 of the first byte.
        (-1, &[0x41]),
        (-2, &[0x42]),
        (-63, &[0x7F]),
        (-64, &[0xC0, 0x01]),
        (-65, &[0xC1, 0x01]),
        (-1000, &[0xE8, 0x0F]),
    ];

    for (value, expected) in cases {
        let mut buf = Vec::new();
        write_varint_i64(&mut buf, *value).expect("write failed");
        assert_eq!(
            buf, *expected,
            "Heat2 encoding mismatch for {} (reference says {:02X?})",
            value, expected
        );

        let mut cursor = std::io::Cursor::new(buf.clone());
        let decoded = read_varint_i64(&mut cursor).expect("read failed");
        assert_eq!(decoded, *value, "round-trip failed for {}", value);
    }
}

#[test]
fn varsize_agrees_with_reference_across_a_wide_range() {
    for value in -100_000i64..=100_000 {
        let mut buf = Vec::new();
        write_varint_i64(&mut buf, value).expect("write failed");
        assert_eq!(
            buf,
            reference_varsize(value),
            "encoding diverged from reference at {}",
            value
        );
    }
}

#[test]
fn values_below_0x40_encode_to_a_single_byte() {
    for value in 0u64..0x40 {
        let mut buf = Vec::new();
        write_varint_u64(&mut buf, value).expect("write failed");
        assert_eq!(buf.len(), 1, "value {} should encode to one byte", value);
        assert_eq!(buf[0] & 0x3F, value as u8, "value {} lost bits", value);
        assert_eq!(buf[0] & 0x80, 0, "value {} must not set continuation", value);
    }
}

#[test]
fn first_byte_never_carries_more_than_six_value_bits() {
    // A 7-bit-first-byte (LEB128) encoder would leak bit 6 into the first byte.
    // Heat2 reserves bit 6 for the sign flag and moves it to the next byte.
    for value in [0x40u64, 0x41, 0x1000, 0x1234, 1_792_499_378, u32::MAX as u64] {
        let mut buf = Vec::new();
        write_varint_u64(&mut buf, value).expect("write failed");
        assert_eq!(
            buf[0] & 0x3F,
            (value & 0x3F) as u8,
            "first byte of {} must hold only the low 6 bits",
            value
        );
    }
}

#[test]
fn negative_values_set_the_sign_flag_in_the_first_byte() {
    for value in [-1i64, -2, -63, -64, -1000, -1_000_000] {
        let mut buf = Vec::new();
        write_varint_i64(&mut buf, value).expect("write failed");
        assert_ne!(
            buf[0] & 0x40,
            0,
            "negative value {} must set the 0x40 sign flag",
            value
        );
    }
}

#[test]
fn unsigned_roundtrip_preserves_the_full_u64_range() {
    let cases = [0u64, 1, 63, 64, 127, 128, 16383, 16384, u32::MAX as u64, u64::MAX - 1, u64::MAX];
    for value in cases {
        let mut buf = Vec::new();
        write_varint_u64(&mut buf, value).expect("write failed");
        let mut cursor = std::io::Cursor::new(buf);
        let decoded = read_varint_u64(&mut cursor).expect("read failed");
        assert_eq!(decoded, value, "round-trip failed for {}", value);
    }
}

#[test]
fn signed_roundtrip_preserves_sign() {
    let cases = [
        0i64, 1, -1, 63, -63, 64, -64, 127, -128, 16384, -16384, i64::MIN, i64::MAX,
    ];
    for value in cases {
        let mut buf = Vec::new();
        write_varint_i64(&mut buf, value).expect("write failed");
        let mut cursor = std::io::Cursor::new(buf);
        let decoded = read_varint_i64(&mut cursor).expect("read failed");
        assert_eq!(decoded, value, "signed round-trip failed for {}", value);
    }
}

/// The exact payload the real FIFA 22 client received for `Util::ping`.
///
/// `STIM` (server time) is tagged `CF 4A 6D`, typed as INTEGER (`0x00`), and the
/// timestamp 1792499378 (2026-10-20 12:29:38Z) encodes as `B2 FA BA AD 0D`.
///
/// There is **no** trailing `00`: the root TDF struct of a Blaze frame is
/// delimited by the frame's length field, not by a terminator. See
/// `root_struct_has_no_trailing_terminator` below.
#[test]
fn ping_response_stim_uses_the_heat2_integer_encoding() {
    let mut payload = TdfStruct::new();
    payload.set("STIM", TdfValue::Integer(1_792_499_378));

    let encoded = serialize_tdf(&payload).expect("serialization failed");

    assert_eq!(
        encoded,
        vec![
            0xCF, 0x4A, 0x6D, // tag "STIM"
            0x00, // type INTEGER
            0xB2, 0xFA, 0xBA, 0xAD, 0x0D, // 1792499378
        ],
        "STIM must be encoded with the Heat2 varsize integer scheme"
    );
}

/// The root TDF struct of a Blaze frame carries **no** trailing terminator.
///
/// This is not a stylistic choice. EA's own frames end exactly on the last
/// byte of their last field:
///
/// * both authentic EA `Util::fetchClientConfig` Fire2 replies (1324 and
///   1390 byte payloads) end on the payload boundary,
/// * the byte-exact authentic EA `Redirector::getServerInstance` reply payload
///   (74 bytes) ends on the payload boundary,
/// * 21 of the 22 authentic legacy-FIRE frames in the reference corpus end on
///   the payload boundary, and none of them carries a terminator.
///
/// The root struct is delimited by the frame's `payloadSize` field instead.
/// A spurious trailing `00` shifts every subsequent byte of the frame and
/// makes the client's Heat2 decoder reject the packet.
#[test]
fn root_struct_has_no_trailing_terminator() {
    let mut payload = TdfStruct::new();
    payload.set("ASRC", TdfValue::String("318307".to_string()));
    payload.set("STIM", TdfValue::Integer(1_792_499_378));

    let encoded = serialize_tdf(&payload).expect("serialization failed");

    // ASRC: tag(3) + type(1) + varsize len(1) + 6 body + NUL(1) = 12 bytes
    // STIM: tag(3) + type(1) + varsize(5)                  =  9 bytes
    // Total 21 bytes -- the struct ends exactly on the last field.
    assert_eq!(
        encoded.len(),
        12 + 9,
        "the root struct must end on its last field, with no terminator"
    );
    // The last field is an INTEGER, so the final byte is its value, not a
    // terminator. A root terminator would make this 0x00 and the length 22.
    assert_eq!(
        *encoded.last().unwrap(),
        0x0D,
        "the root struct must not end with a terminator byte"
    );
}

/// Nested structs *do* carry a terminator: they have no length prefix, so the
/// `00` is the only thing that says where the nested struct ends.
#[test]
fn nested_structs_do_carry_a_terminator() {
    let mut inner = TdfStruct::new();
    inner.set("GNAM", TdfValue::String("Room_XY79K2".to_string()));

    let mut root = TdfStruct::new();
    root.set("NEST", TdfValue::Struct(inner));

    let encoded = serialize_tdf(&root).expect("serialization failed");

    // The nested struct is the last thing in the root struct, so the trailing
    // byte is the nested terminator, not a root terminator.
    assert_eq!(
        *encoded.last().unwrap(),
        0x00,
        "a nested struct must end with a terminator"
    );

    // GNAM is 11 characters: tag(3) + type(1) + varsize len(1) + 11 body
    // + NUL(1) = 17, plus the nested struct's own terminator = 18. The root
    // struct adds tag(3) + type(1). Total 22.
    //
    // If the root struct also carried a terminator, this would be 23.
    assert_eq!(encoded.len(), 4 + 18, "only the nested struct is terminated");
}

/// String lengths on the wire include the NUL terminator and are themselves
/// varsize encoded.
#[test]
fn string_length_includes_nul_and_is_varsize_encoded() {
    // A 200-character string: length+1 = 201.
    //   Heat2 : 0x89 0x03   (low 6 bits = 9, continuation, then 3 -> 9 + 3*64 = 201)
    //   LEB128: 0xC9 0x01   (low 7 bits = 73, continuation, then 1 -> 73 + 128 = 201)
    let long = "A".repeat(200);

    let mut payload = TdfStruct::new();
    payload.set("STRS", TdfValue::String(long));

    let encoded = serialize_tdf(&payload).expect("serialization failed");

    // tag(3) + type(1) = 4 bytes, then the varsize length.
    assert_eq!(&encoded[4..6], &[0x89, 0x03], "string length must be Heat2 varsize");
    // tag(3) + type(1) + varsize length(2) + 200 body bytes + NUL(1)
    assert_eq!(
        encoded.len(),
        4 + 2 + 200 + 1,
        "string body plus NUL terminator must follow the length"
    );
}
