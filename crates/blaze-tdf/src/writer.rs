//! TDF Binary Serializer / Writer.

use std::io::{self, Write};

use crate::tag::{encode_tdf_tag, tag_to_bytes};
use crate::types::*;
use crate::varint::{write_varint_i64, write_varint_u64};

/// Serializes TDF values and structs into a binary writer.
pub struct TdfWriter<W> {
    writer: W,
}

impl<W: Write> TdfWriter<W> {
    pub fn new(writer: W) -> Self {
        Self { writer }
    }

    pub fn into_inner(self) -> W {
        self.writer
    }

    /// Serializes an entire TdfStruct, terminating with the 0x00 TAG_END byte.
    pub fn write_struct(&mut self, s: &TdfStruct) -> io::Result<()> {
        for (tag, value) in &s.fields {
            self.write_tagged_field(tag, value)?;
        }
        // Write struct terminator (0x00 byte)
        self.writer.write_all(&[0x00])?;
        Ok(())
    }

    /// Writes a single field with its 3-byte tag, 1-byte type, and payload.
    pub fn write_tagged_field(&mut self, tag_str: &str, value: &TdfValue) -> io::Result<()> {
        let tag_num = encode_tdf_tag(tag_str);
        let tag_bytes = tag_to_bytes(tag_num);
        self.writer.write_all(&tag_bytes)?;

        let wire_type = value.wire_type() as u8;
        self.writer.write_all(&[wire_type])?;

        self.write_value(value)?;
        Ok(())
    }

    /// Writes the payload of a TDF value according to its type.
    pub fn write_value(&mut self, value: &TdfValue) -> io::Result<()> {
        match value {
            TdfValue::Integer(val) => {
                write_varint_u64(&mut self.writer, *val)?;
            }
            TdfValue::String(s) => {
                let bytes = s.as_bytes();
                // Length includes the null terminator (+1)
                write_varint_u64(&mut self.writer, (bytes.len() + 1) as u64)?;
                self.writer.write_all(bytes)?;
                self.writer.write_all(&[0x00])?;
            }
            TdfValue::Blob(blob) => {
                write_varint_u64(&mut self.writer, blob.len() as u64)?;
                self.writer.write_all(blob)?;
            }
            TdfValue::Struct(s) => {
                self.write_struct(s)?;
            }
            TdfValue::List(list) => {
                self.writer.write_all(&[list.item_type as u8])?;
                write_varint_u64(&mut self.writer, list.items.len() as u64)?;
                for item in &list.items {
                    self.write_value(item)?;
                }
            }
            TdfValue::Map(map) => {
                self.writer.write_all(&[map.key_type as u8])?;
                self.writer.write_all(&[map.val_type as u8])?;
                write_varint_u64(&mut self.writer, map.entries.len() as u64)?;
                for (k, v) in &map.entries {
                    self.write_value(k)?;
                    self.write_value(v)?;
                }
            }
            TdfValue::Union(u) => {
                self.writer.write_all(&[u.active_tag])?;
                if u.active_tag != TdfUnion::EMPTY_TAG {
                    if let Some(inner) = &u.value {
                        self.writer.write_all(&[inner.wire_type() as u8])?;
                        self.write_value(inner)?;
                    }
                }
            }
            TdfValue::VariableInteger(val) => {
                write_varint_i64(&mut self.writer, *val)?;
            }
            TdfValue::ObjectId(obj_id) => {
                self.writer
                    .write_all(&obj_id.object_type.component.to_be_bytes())?;
                self.writer
                    .write_all(&obj_id.object_type.object_type.to_be_bytes())?;
                write_varint_u64(&mut self.writer, obj_id.id)?;
            }
            TdfValue::ObjectType(obj_type) => {
                self.writer
                    .write_all(&obj_type.component.to_be_bytes())?;
                self.writer
                    .write_all(&obj_type.object_type.to_be_bytes())?;
            }
            TdfValue::TimeValue(time_us) => {
                write_varint_i64(&mut self.writer, *time_us)?;
            }
            TdfValue::Float(f) => {
                self.writer.write_all(&f.to_be_bytes())?;
            }
        }
        Ok(())
    }
}
