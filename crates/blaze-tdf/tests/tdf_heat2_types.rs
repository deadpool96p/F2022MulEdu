//! Heat2 TDF type conformance: every wire type, byte for byte.
//!
//! The vectors below are the reference repo's twelve type fixtures. Each one
//! is a complete Blaze frame whose payload holds exactly one field, so the
//! expected bytes are the whole payload.
//!
//! The fixtures are the ground truth for the Heat2 codec. They disagree with
//! the older BlazeSDK documentation in two places, and both disagreements
//! used to be bugs here:
//!
//! * `ObjectType` is wire type **0x08** and `ObjectId` is **0x09** (not the
//!   other way round), and
//! * `Float` is **0x0A** and `TimeValue` is **0x0B** (not the other way
//!   round).
//!
//! Object ids and types are *varsize*-encoded in Heat2, not written as two
//! raw big-endian u16s.

use blaze_tdf::{serialize_tdf, TdfStruct, TdfType, TdfUnion, TdfValue, TdfVariable};

/// Builds a single-field payload.
fn payload(tag: &str, value: TdfValue) -> TdfStruct {
    let mut s = TdfStruct::new();
    s.set(tag, value);
    s
}

/// The tag bytes of a 1-to-4 character tag, as they appear on the wire.
fn tag_bytes(tag: &str) -> [u8; 3] {
    blaze_tdf::tag_to_bytes(blaze_tdf::encode_tdf_tag(tag))
}

#[test]
fn integer_uses_the_varsize_scheme() {
    let encoded = serialize_tdf(&payload("VALU", TdfValue::Integer(42))).expect("encode");
    assert_eq!(
        encoded,
        vec![
            0xDA, 0x1B, 0x35, // tag "VALU"
            0x00, // type INTEGER
            0x2A, // 42
        ]
    );
}

#[test]
fn string_length_includes_the_nul_terminator() {
    let encoded = serialize_tdf(&payload("NAME", TdfValue::String("hello".to_string()))).expect("encode");
    assert_eq!(
        encoded,
        vec![
            0xBA, 0x1B, 0x65, // tag "NAME"
            0x01, // type STRING
            0x06, // length, including the NUL
            b'h', b'e', b'l', b'l', b'o',
            0x00, // NUL
        ]
    );
}

#[test]
fn blob_is_a_length_prefixed_byte_string() {
    let encoded = serialize_tdf(&payload("BLOB", TdfValue::Blob(vec![0xDE, 0xAD, 0xBE, 0xEF])))
        .expect("encode");
    assert_eq!(
        encoded,
        vec![
            0x8A, 0xCB, 0xE2, // tag "BLOB"
            0x02, // type BLOB
            0x04, // length
            0xDE, 0xAD, 0xBE, 0xEF,
        ]
    );
}

#[test]
fn float_is_type_0x0a_and_big_endian_ieee754() {
    let encoded = serialize_tdf(&payload("PI__", TdfValue::Float(3.14))).expect("encode");
    assert_eq!(
        encoded,
        vec![
            0xC2, 0x9F, 0xFF, // tag "PI__"
            0x0A, // type FLOAT
            0x40, 0x48, 0xF5, 0xC3, // 3.14f big-endian
        ]
    );
}

#[test]
fn timevalue_is_type_0x0b_and_varsize_encoded() {
    let encoded = serialize_tdf(&payload("TIME", TdfValue::TimeValue(5_000_000))).expect("encode");
    assert_eq!(
        encoded,
        vec![
            0xD2, 0x9B, 0x65, // tag "TIME"
            0x0B, // type TIMEVALUE
            0x80, 0xAD, 0xE2, 0x04, // 5000000
        ]
    );
}

#[test]
fn object_type_is_type_0x08_and_varsize_encoded() {
    let encoded = serialize_tdf(&payload(
        "OTYP",
        TdfValue::ObjectType(blaze_tdf::BlazeObjectType::new(4, 1)),
    ))
    .expect("encode");
    assert_eq!(
        encoded,
        vec![
            0xBF, 0x4E, 0x70, // tag "OTYP"
            0x08, // type OBJECT_TYPE
            0x04, // component 4
            0x01, // type 1
        ]
    );
}

#[test]
fn object_id_is_type_0x09_and_varsize_encoded() {
    let encoded = serialize_tdf(&payload(
        "OBID",
        TdfValue::ObjectId(blaze_tdf::BlazeObjectId::new(4, 1, 1000)),
    ))
    .expect("encode");
    assert_eq!(
        encoded,
        vec![
            0xBE, 0x2A, 0x64, // tag "OBID"
            0x09, // type OBJECT_ID
            0x04, // component 4
            0x01, // type 1
            0xA8, 0x0F, // id 1000
        ]
    );
}

#[test]
fn list_is_a_typed_homogeneous_sequence() {
    let encoded = serialize_tdf(&payload(
        "LIST",
        TdfValue::List(blaze_tdf::TdfList::new(
            TdfType::Integer,
            vec![TdfValue::Integer(1), TdfValue::Integer(2), TdfValue::Integer(3)],
        )),
    ))
    .expect("encode");
    assert_eq!(
        encoded,
        vec![
            0xB2, 0x9C, 0xF4, // tag "LIST"
            0x04, // type LIST
            0x00, // element type INTEGER
            0x03, // count
            0x01, 0x02, 0x03,
        ]
    );
}

#[test]
fn map_is_a_typed_key_value_collection() {
    let encoded = serialize_tdf(&payload(
        "MAP_",
        TdfValue::Map(blaze_tdf::TdfMap::new(
            TdfType::String,
            TdfType::String,
            vec![
                (TdfValue::String("a".to_string()), TdfValue::String("b".to_string())),
                (TdfValue::String("c".to_string()), TdfValue::String("d".to_string())),
            ],
        )),
    ))
    .expect("encode");
    assert_eq!(
        encoded,
        vec![
            0xB6, 0x1C, 0x3F, // tag "MAP_"
            0x05, // type MAP
            0x01, // key type STRING
            0x01, // value type STRING
            0x02, // count
            0x02, b'a', 0x00, // key "a"
            0x02, b'b', 0x00, // value "b"
            0x02, b'c', 0x00, // key "c"
            0x02, b'd', 0x00, // value "d"
        ]
    );
}

#[test]
fn nested_struct_ends_with_a_terminator() {
    let mut inner = TdfStruct::new();
    inner.set("GNAM", TdfValue::String("Room_XY79K2".to_string()));
    inner.set("GTYP", TdfValue::Integer(2));

    let encoded = serialize_tdf(&payload("QOSS", TdfValue::Struct(inner))).expect("encode");

    // The nested struct's fields, then its own 0x00 terminator, and no
    // terminator for the root struct.
    //
    // Fields inside a struct are written in tag order, so "GNAM" comes
    // before "GTYP".
    assert_eq!(
        encoded,
        vec![
            0xC6, 0xFC, 0xF3, // tag "QOSS"
            0x03, // type TDF
            0x9E, 0xE8, 0x6D, // tag "GNAM"
            0x01, // type STRING
            0x0C, // length incl. NUL
            b'R', b'o', b'o', b'm', b'_', b'X', b'Y', b'7', b'9', b'K', b'2',
            0x00, // NUL
            0x9F, 0x4E, 0x70, // tag "GTYP"
            0x00, // type INTEGER
            0x02, // 2
            0x00, // nested struct terminator
        ]
    );
}

#[test]
fn union_carries_its_case_index_and_a_three_byte_member_tag() {
    let encoded = serialize_tdf(&payload(
        "UNIN",
        TdfValue::Union(TdfUnion::with_member(
            2,
            "MBR_",
            TdfValue::Integer(7),
        )),
    ))
    .expect("encode");

    assert_eq!(
        encoded,
        vec![
            0xD6, 0xEA, 0x6E, // tag "UNIN"
            0x06, // type UNION
            0x02, // active case index
            0xB6, 0x2C, 0xBF, // member tag "MBR_"
            0x00, // member type INTEGER
            0x07, // 7
        ]
    );
}

#[test]
fn union_defaults_to_the_valu_member_tag() {
    let encoded = serialize_tdf(&payload(
        "ADDR",
        TdfValue::Union(TdfUnion::with_value(2, TdfValue::Integer(1))),
    ))
    .expect("encode");

    // The member tag is "VALU" -- the tag every anonymous EA union case uses.
    assert_eq!(&encoded[5..8], &tag_bytes("VALU"));
    assert_eq!(encoded[4], 0x02, "the case index comes first");
    assert_eq!(encoded[8], 0x00, "then the member type");
}

#[test]
fn variable_carries_its_tdf_id_member_tag_and_terminator() {
    let encoded = serialize_tdf(&payload(
        "VARI",
        TdfValue::Variable(TdfVariable::new(1234, "VVAL", TdfValue::Integer(99))),
    ))
    .expect("encode");

    assert_eq!(
        encoded,
        vec![
            0xDA, 0x1C, 0xA9, // tag "VARI"
            0x07, // type VARIABLE
            0x01, // present
            0x92, 0x13, // TDF id 1234
            0xDB, 0x68, 0x6C, // member tag "VVAL"
            0x00, // member type INTEGER
            0xA3, 0x01, // 99
            0x00, // the variable's own terminator
        ]
    );
}

#[test]
fn an_absent_variable_is_a_single_zero_byte() {
    let encoded = serialize_tdf(&payload(
        "PRVT",
        TdfValue::Variable(TdfVariable::absent()),
    ))
    .expect("encode");

    assert_eq!(
        encoded,
        vec![
            0xC3, 0x2D, 0xB4, // tag "PRVT"
            0x07, // type VARIABLE
            0x00, // not present
        ]
    );
}

/// The wire type identifiers themselves. A client picks its decoder off these
/// bytes, so a swapped pair is fatal.
#[test]
fn the_heat2_wire_type_identifiers_are_what_we_think_they_are() {
    assert_eq!(TdfType::Integer as u8, 0x00);
    assert_eq!(TdfType::String as u8, 0x01);
    assert_eq!(TdfType::Blob as u8, 0x02);
    assert_eq!(TdfType::Struct as u8, 0x03);
    assert_eq!(TdfType::List as u8, 0x04);
    assert_eq!(TdfType::Map as u8, 0x05);
    assert_eq!(TdfType::Union as u8, 0x06);
    assert_eq!(TdfType::Variable as u8, 0x07);
    assert_eq!(TdfType::ObjectType as u8, 0x08);
    assert_eq!(TdfType::ObjectId as u8, 0x09);
    assert_eq!(TdfType::Float as u8, 0x0A);
    assert_eq!(TdfType::TimeValue as u8, 0x0B);
    assert_eq!(TdfType::Generic as u8, 0x0C);
}

/// The binary's out-of-range / unknown type code is `0x0D`, not the Go
/// reference's `0xFF` sentinel.
///
/// `FUN_14496BB30` at `0x14496BB30` maps the internal TDF type id to the
/// wire type byte. An internal id of `0x00`, or anything above `0x17`,
/// produces wire byte `0x0D`:
///
/// ```text
/// cmp  ecx, 0x17
/// ja   <default>
/// ```
///
/// and the default case writes `0x0D`. A decoder that rejects `0x0D` outright
/// turns a malformed field into a hard parse failure instead of a
/// recoverable one.
#[test]
fn the_unknown_type_code_is_zero_d_zero_d() {
    assert_eq!(TdfType::from_u8(0x0D), Some(TdfType::Unknown));
    assert_eq!(TdfType::Unknown as u8, 0x0D);
    assert_eq!(
        TdfType::from_u8(0x0C),
        Some(TdfType::Generic),
        "0x0C is GENERIC in FIFA 22's Heat2 codec"
    );
}

/// Every type must round-trip through the decoder as well as the encoder.
#[test]
fn every_type_round_trips() {
    let mut inner = TdfStruct::new();
    inner.set("GNAM", TdfValue::String("Room_XY79K2".to_string()));

    let mut root = TdfStruct::new();
    root.set("UINT", TdfValue::Integer(123_456_789));
    root.set("STR", TdfValue::String("FIFA22_ONLINE_LOBBY".to_string()));
    root.set("BLOB", TdfValue::Blob(vec![0xDE, 0xAD, 0xBE, 0xEF]));
    root.set("NEST", TdfValue::Struct(inner));
    root.set(
        "LIST",
        TdfValue::List(blaze_tdf::TdfList::new(
            TdfType::String,
            vec![
                TdfValue::String("Player1".to_string()),
                TdfValue::String("Player2".to_string()),
            ],
        )),
    );
    root.set(
        "ATTR",
        TdfValue::Map(blaze_tdf::TdfMap::new(
            TdfType::String,
            TdfType::Integer,
            vec![(TdfValue::String("HOME".to_string()), TdfValue::Integer(243))],
        )),
    );
    root.set(
        "UNIO",
        TdfValue::Union(TdfUnion::with_member(
            2,
            "MBR_",
            TdfValue::Integer(1),
        )),
    );
    root.set(
        "VARI",
        TdfValue::Variable(TdfVariable::new(1234, "VVAL", TdfValue::Integer(99))),
    );
    root.set(
        "VABS",
        TdfValue::Variable(TdfVariable::absent()),
    );
    root.set(
        "OBID",
        TdfValue::ObjectId(blaze_tdf::BlazeObjectId::new(4, 1, 9999)),
    );
    root.set(
        "OBTY",
        TdfValue::ObjectType(blaze_tdf::BlazeObjectType::new(4, 1)),
    );
    root.set("TIME", TdfValue::TimeValue(1_700_000_000_000_000));
    root.set("FLOT", TdfValue::Float(3.14159));

    let encoded = serialize_tdf(&root).expect("encode");
    let decoded = blaze_tdf::deserialize_tdf(&encoded).expect("decode");

    assert_eq!(decoded.get("UINT"), Some(&TdfValue::Integer(123_456_789)));
    assert_eq!(
        decoded.get("STR"),
        Some(&TdfValue::String("FIFA22_ONLINE_LOBBY".to_string()))
    );
    assert_eq!(
        decoded.get("BLOB"),
        Some(&TdfValue::Blob(vec![0xDE, 0xAD, 0xBE, 0xEF]))
    );
    assert_eq!(
        decoded.get("UNIO"),
        Some(&TdfValue::Union(TdfUnion::with_member(
            2,
            "MBR_",
            TdfValue::Integer(1)
        )))
    );
    assert_eq!(
        decoded.get("VARI"),
        Some(&TdfValue::Variable(TdfVariable::new(
            1234,
            "VVAL",
            TdfValue::Integer(99)
        )))
    );
    assert_eq!(
        decoded.get("VABS"),
        Some(&TdfValue::Variable(TdfVariable::absent()))
    );
    assert_eq!(
        decoded.get("OBID"),
        Some(&TdfValue::ObjectId(blaze_tdf::BlazeObjectId::new(4, 1, 9999)))
    );
    assert_eq!(
        decoded.get("OBTY"),
        Some(&TdfValue::ObjectType(blaze_tdf::BlazeObjectType::new(4, 1)))
    );
    assert_eq!(
        decoded.get("TIME"),
        Some(&TdfValue::TimeValue(1_700_000_000_000_000))
    );

    match decoded.get("FLOT") {
        Some(TdfValue::Float(f)) => assert!((f - 3.14159).abs() < 1e-5),
        other => panic!("expected a float, got {:?}", other),
    }
}
