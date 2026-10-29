//! TDF Binary Deserializer / Reader.

use std::io::{self, Read};

use crate::tag::{decode_tdf_tag, tag_from_bytes};
use crate::types::*;
use crate::varint::{read_varint_i64, read_varint_u64};

/// Deserializes TDF values and structs from a binary reader.
pub struct TdfReader<R> {
    reader: R,
}

impl<R: Read> TdfReader<R> {
    pub fn new(reader: R) -> Self {
        Self { reader }
    }

    pub fn into_inner(self) -> R {
        self.reader
    }

    /// Reads an entire TdfStruct, stopping when the 0x00 TAG_END byte is encountered.
    pub fn read_struct(&mut self) -> io::Result<TdfStruct> {
        let mut result = TdfStruct::new();

        loop {
            let mut first_byte = [0u8; 1];
            if self.reader.read_exact(&mut first_byte).is_err() {
                // If stream ended without tag end, return what we have
                break;
            }

            if first_byte[0] == 0x00 {
                // TAG_END: End of struct
                break;
            }

            // Read remaining 2 bytes of the 3-byte tag
            let mut remaining_tag = [0u8; 2];
            self.reader.read_exact(&mut remaining_tag)?;
            let tag_bytes = [first_byte[0], remaining_tag[0], remaining_tag[1]];
            let tag_num = tag_from_bytes(&tag_bytes);
            let tag_str = decode_tdf_tag(tag_num);

            // Read 1-byte wire type
            let mut type_byte = [0u8; 1];
            self.reader.read_exact(&mut type_byte)?;
            let wire_type = TdfType::from_u8(type_byte[0]).ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("Unknown TDF wire type: 0x{:02X}", type_byte[0]),
                )
            })?;

            let value = self.read_value(wire_type)?;
            result.fields.insert(tag_str, value);
        }

        Ok(result)
    }

    /// Reads a TDF value according to the expected wire type.
    pub fn read_value(&mut self, wire_type: TdfType) -> io::Result<TdfValue> {
        match wire_type {
            TdfType::Integer => {
                let val = read_varint_u64(&mut self.reader)?;
                Ok(TdfValue::Integer(val))
            }
            TdfType::String => {
                let len = read_varint_u64(&mut self.reader)? as usize;
                if len == 0 {
                    return Ok(TdfValue::String(String::new()));
                }
                let mut buf = vec![0u8; len];
                self.reader.read_exact(&mut buf)?;
                // Strip trailing null byte if present
                if let Some(pos) = buf.iter().position(|&b| b == 0) {
                    buf.truncate(pos);
                }
                let s = String::from_utf8_lossy(&buf).into_owned();
                Ok(TdfValue::String(s))
            }
            TdfType::Blob => {
                let len = read_varint_u64(&mut self.reader)? as usize;
                let mut buf = vec![0u8; len];
                self.reader.read_exact(&mut buf)?;
                Ok(TdfValue::Blob(buf))
            }
            TdfType::Struct => {
                let inner = self.read_struct()?;
                Ok(TdfValue::Struct(inner))
            }
            TdfType::List => {
                let mut item_type_byte = [0u8; 1];
                self.reader.read_exact(&mut item_type_byte)?;
                let item_type = TdfType::from_u8(item_type_byte[0]).ok_or_else(|| {
                    io::Error::new(
                        io::ErrorKind::InvalidData,
                        format!("Unknown List item type: 0x{:02X}", item_type_byte[0]),
                    )
                })?;

                let count = read_varint_u64(&mut self.reader)? as usize;
                let mut items = Vec::with_capacity(count);
                for _ in 0..count {
                    items.push(self.read_value(item_type)?);
                }
                Ok(TdfValue::List(TdfList::new(item_type, items)))
            }
            TdfType::Map => {
                let mut key_type_byte = [0u8; 1];
                self.reader.read_exact(&mut key_type_byte)?;
                let key_type = TdfType::from_u8(key_type_byte[0]).ok_or_else(|| {
                    io::Error::new(
                        io::ErrorKind::InvalidData,
                        format!("Unknown Map key type: 0x{:02X}", key_type_byte[0]),
                    )
                })?;

                let mut val_type_byte = [0u8; 1];
                self.reader.read_exact(&mut val_type_byte)?;
                let val_type = TdfType::from_u8(val_type_byte[0]).ok_or_else(|| {
                    io::Error::new(
                        io::ErrorKind::InvalidData,
                        format!("Unknown Map value type: 0x{:02X}", val_type_byte[0]),
                    )
                })?;

                let count = read_varint_u64(&mut self.reader)? as usize;
                let mut entries = Vec::with_capacity(count);
                for _ in 0..count {
                    let k = self.read_value(key_type)?;
                    let v = self.read_value(val_type)?;
                    entries.push((k, v));
                }
                Ok(TdfValue::Map(TdfMap::new(key_type, val_type, entries)))
            }
            TdfType::Union => {
                let mut active_tag_byte = [0u8; 1];
                self.reader.read_exact(&mut active_tag_byte)?;
                let active_tag = active_tag_byte[0];

                if active_tag == TdfUnion::EMPTY_TAG || active_tag == TdfUnion::INVALID_TAG {
                    Ok(TdfValue::Union(TdfUnion::empty()))
                } else {
                    // The 3-byte member tag follows the case index, then the
                    // member's type byte and its value.
                    let mut member_tag_bytes = [0u8; 3];
                    self.reader.read_exact(&mut member_tag_bytes)?;
                    let member_tag = decode_tdf_tag(tag_from_bytes(&member_tag_bytes));

                    let mut inner_type_byte = [0u8; 1];
                    self.reader.read_exact(&mut inner_type_byte)?;
                    let inner_type = TdfType::from_u8(inner_type_byte[0]).ok_or_else(|| {
                        io::Error::new(
                            io::ErrorKind::InvalidData,
                            format!("Unknown Union inner type: 0x{:02X}", inner_type_byte[0]),
                        )
                    })?;
                    let val = self.read_value(inner_type)?;
                    Ok(TdfValue::Union(TdfUnion::with_member(
                        active_tag,
                        &member_tag,
                        val,
                    )))
                }
            }
            TdfType::Variable => {
                let mut present_byte = [0u8; 1];
                self.reader.read_exact(&mut present_byte)?;
                if present_byte[0] == 0 {
                    return Ok(TdfValue::Variable(TdfVariable::absent()));
                }

                let tdf_id = read_varint_u64(&mut self.reader)? as u32;

                let mut member_tag_bytes = [0u8; 3];
                self.reader.read_exact(&mut member_tag_bytes)?;
                let member_tag = decode_tdf_tag(tag_from_bytes(&member_tag_bytes));

                let mut member_type_byte = [0u8; 1];
                self.reader.read_exact(&mut member_type_byte)?;
                let member_type = TdfType::from_u8(member_type_byte[0]).ok_or_else(|| {
                    io::Error::new(
                        io::ErrorKind::InvalidData,
                        format!("Unknown Variable member type: 0x{:02X}", member_type_byte[0]),
                    )
                })?;
                let val = self.read_value(member_type)?;

                // The variable's own trailing terminator.
                let mut terminator = [0u8; 1];
                self.reader.read_exact(&mut terminator)?;

                Ok(TdfValue::Variable(TdfVariable::new(
                    tdf_id,
                    &member_tag,
                    val,
                )))
            }
            TdfType::ObjectId => {
                // Heat2 varsize-encodes the component, the type and the id.
                let component = read_varint_u64(&mut self.reader)? as u16;
                let object_type = read_varint_u64(&mut self.reader)? as u16;
                let id = read_varint_u64(&mut self.reader)?;
                Ok(TdfValue::ObjectId(BlazeObjectId::new(
                    component,
                    object_type,
                    id,
                )))
            }
            TdfType::ObjectType => {
                let component = read_varint_u64(&mut self.reader)? as u16;
                let object_type = read_varint_u64(&mut self.reader)? as u16;
                Ok(TdfValue::ObjectType(BlazeObjectType::new(
                    component,
                    object_type,
                )))
            }
            TdfType::TimeValue => {
                let time_us = read_varint_i64(&mut self.reader)?;
                Ok(TdfValue::TimeValue(time_us))
            }
            TdfType::Float => {
                let mut float_bytes = [0u8; 4];
                self.reader.read_exact(&mut float_bytes)?;
                let f = f32::from_be_bytes(float_bytes);
                Ok(TdfValue::Float(f))
            }
            TdfType::Generic => {
                // [ presence byte ] [ varsize integer, only when present ]
                let mut present = [0u8; 1];
                self.reader.read_exact(&mut present)?;
                if present[0] == 0 {
                    return Ok(TdfValue::Generic {
                        present: false,
                        value: 0,
                    });
                }
                let value = read_varint_u64(&mut self.reader)?;
                Ok(TdfValue::Generic {
                    present: true,
                    value,
                })
            }
            TdfType::Unknown => Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Unknown TDF wire type 0x0D: the value's length cannot be determined",
            )),
        }
    }
}
