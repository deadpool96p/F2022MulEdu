//! # blaze-tdf
//!
//! Production-grade Tagged Data Format (TDF) parser and serializer for the EA Blaze Protocol.
//! Supports all 12 wire types, 6-bit compressed ASCII tag encoding, and VarInt encoding.

pub mod reader;
pub mod tag;
pub mod types;
pub mod varint;
pub mod writer;

pub use reader::TdfReader;
pub use tag::{decode_tdf_tag, encode_tdf_tag, tag_from_bytes, tag_to_bytes};
pub use types::*;
pub use varint::{read_varint_i64, read_varint_u64, write_varint_i64, write_varint_u64};
pub use writer::TdfWriter;

use std::io::{self, Cursor};

/// Serializes a TdfStruct into a byte buffer.
///
/// This writes a **root** struct: no trailing TAG_END byte. The root struct of
/// a Blaze frame is delimited by the frame's own length field, so EA does not
/// emit a terminator for it. Nested structs still carry their own terminator.
pub fn serialize_tdf(s: &TdfStruct) -> io::Result<Vec<u8>> {
    let mut writer = TdfWriter::new(Vec::new());
    writer.write_root_struct(s)?;
    Ok(writer.into_inner())
}

/// Deserializes a TdfStruct from a byte slice.
pub fn deserialize_tdf(bytes: &[u8]) -> io::Result<TdfStruct> {
    let mut reader = TdfReader::new(Cursor::new(bytes));
    reader.read_struct()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_all_12_wire_types_roundtrip() {
        let mut root = TdfStruct::new();

        // 0x00: Integer
        root.set("UINT", TdfValue::Integer(123456789));

        // 0x01: String
        root.set("STR", TdfValue::String("FIFA22_ONLINE_LOBBY".to_string()));

        // 0x02: Blob
        root.set("BLOB", TdfValue::Blob(vec![0xDE, 0xAD, 0xBE, 0xEF]));

        // 0x03: Nested Struct
        let mut inner = TdfStruct::new();
        inner.set("GNAM", TdfValue::String("Room_XY79K2".to_string()));
        inner.set("GTYP", TdfValue::Integer(2));
        root.set("NEST", TdfValue::Struct(inner));

        // 0x04: List
        let list_items = vec![
            TdfValue::String("Player1".to_string()),
            TdfValue::String("Player2".to_string()),
        ];
        root.set("LIST", TdfValue::List(TdfList::new(TdfType::String, list_items)));

        // 0x05: Map
        let map_entries = vec![
            (
                TdfValue::String("HOME".to_string()),
                TdfValue::Integer(243), // Real Madrid ID
            ),
            (
                TdfValue::String("AWAY".to_string()),
                TdfValue::Integer(9), // Liverpool ID
            ),
        ];
        root.set(
            "ATTR",
            TdfValue::Map(TdfMap::new(TdfType::String, TdfType::Integer, map_entries)),
        );

        // 0x06: Union
        root.set(
            "UNIO",
            TdfValue::Union(TdfUnion::with_value(
                1,
                TdfValue::String("ActiveUnionVal".to_string()),
            )),
        );

        // 0x07: VariableInteger
        root.set("VINT", TdfValue::VariableInteger(-987654321));

        // 0x08: ObjectId
        root.set(
            "OBID",
            TdfValue::ObjectId(BlazeObjectId::new(0x0004, 0x0001, 9999)),
        );

        // 0x09: ObjectType
        root.set(
            "OBTY",
            TdfValue::ObjectType(BlazeObjectType::new(0x0004, 0x0001)),
        );

        // 0x0A: TimeValue
        root.set("TIME", TdfValue::TimeValue(1700000000000000));

        // 0x0B: Float
        root.set("FLOT", TdfValue::Float(3.14159));

        // Serialize
        let encoded = serialize_tdf(&root).expect("Serialization failed");
        assert!(!encoded.is_empty());

        // Deserialize
        let decoded = deserialize_tdf(&encoded).expect("Deserialization failed");

        // Verify fields
        assert_eq!(decoded.get("UINT"), Some(&TdfValue::Integer(123456789)));
        assert_eq!(
            decoded.get("STR"),
            Some(&TdfValue::String("FIFA22_ONLINE_LOBBY".to_string()))
        );
        assert_eq!(
            decoded.get("BLOB"),
            Some(&TdfValue::Blob(vec![0xDE, 0xAD, 0xBE, 0xEF]))
        );
        assert_eq!(
            decoded.get("VINT"),
            Some(&TdfValue::VariableInteger(-987654321))
        );
        assert_eq!(
            decoded.get("OBID"),
            Some(&TdfValue::ObjectId(BlazeObjectId::new(0x0004, 0x0001, 9999)))
        );
        assert_eq!(
            decoded.get("OBTY"),
            Some(&TdfValue::ObjectType(BlazeObjectType::new(0x0004, 0x0001)))
        );
        assert_eq!(
            decoded.get("TIME"),
            Some(&TdfValue::TimeValue(1700000000000000))
        );

        // Verify Float with epsilon
        if let Some(TdfValue::Float(f)) = decoded.get("FLOT") {
            assert!((f - 3.14159).abs() < 1e-5);
        } else {
            panic!("Expected Float value");
        }

        // Verify Nested Struct
        if let Some(TdfValue::Struct(s)) = decoded.get("NEST") {
            assert_eq!(
                s.get("GNAM"),
                Some(&TdfValue::String("Room_XY79K2".to_string()))
            );
            assert_eq!(s.get("GTYP"), Some(&TdfValue::Integer(2)));
        } else {
            panic!("Expected Nested Struct");
        }

        // Verify List
        if let Some(TdfValue::List(l)) = decoded.get("LIST") {
            assert_eq!(l.item_type, TdfType::String);
            assert_eq!(l.items.len(), 2);
            assert_eq!(l.items[0], TdfValue::String("Player1".to_string()));
            assert_eq!(l.items[1], TdfValue::String("Player2".to_string()));
        } else {
            panic!("Expected List");
        }

        // Verify Map
        if let Some(TdfValue::Map(m)) = decoded.get("ATTR") {
            assert_eq!(m.key_type, TdfType::String);
            assert_eq!(m.val_type, TdfType::Integer);
            assert_eq!(m.entries.len(), 2);
            assert_eq!(
                m.entries[0],
                (TdfValue::String("HOME".to_string()), TdfValue::Integer(243))
            );
            assert_eq!(
                m.entries[1],
                (TdfValue::String("AWAY".to_string()), TdfValue::Integer(9))
            );
        } else {
            panic!("Expected Map");
        }
    }
}
