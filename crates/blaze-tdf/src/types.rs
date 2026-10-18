//! TDF Type definitions and Value enumerations for EA Blaze Protocol.

use std::collections::BTreeMap;

/// Tagged Data Format (TDF) Wire Type Identifiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum TdfType {
    /// 0x00: Unsigned variable-length integer (1 to 8 bytes).
    Integer = 0x00,
    /// 0x01: Variable-length string (VarInt length + UTF-8 payload).
    String = 0x01,
    /// 0x02: Binary blob (VarInt length + byte buffer).
    Blob = 0x02,
    /// 0x03: Nested struct of tagged fields, terminated by 0x00 tag end.
    Struct = 0x03,
    /// 0x04: Homogeneous list (1-byte item type + VarInt count + items).
    List = 0x04,
    /// 0x05: Key-Value map (1-byte key type + 1-byte value type + VarInt count + pairs).
    Map = 0x05,
    /// 0x06: Tagged union (1-byte active index + inner field).
    Union = 0x06,
    /// 0x07: Signed variable-length 64-bit integer.
    VariableInteger = 0x07,
    /// 0x08: Blaze Object ID (Component u16 + Type u16 + ID u64).
    ObjectId = 0x08,
    /// 0x09: Blaze Object Type (Component u16 + Type u16).
    ObjectType = 0x09,
    /// 0x0A: Time value in microseconds (signed 64-bit integer).
    TimeValue = 0x0A,
    /// 0x0B: 32-bit floating point number (IEEE 754 big-endian).
    Float = 0x0B,
}

impl TdfType {
    /// Attempts to parse a 1-byte wire type identifier into a TdfType.
    pub fn from_u8(val: u8) -> Option<Self> {
        match val {
            0x00 => Some(Self::Integer),
            0x01 => Some(Self::String),
            0x02 => Some(Self::Blob),
            0x03 => Some(Self::Struct),
            0x04 => Some(Self::List),
            0x05 => Some(Self::Map),
            0x06 => Some(Self::Union),
            0x07 => Some(Self::VariableInteger),
            0x08 => Some(Self::ObjectId),
            0x09 => Some(Self::ObjectType),
            0x0A => Some(Self::TimeValue),
            0x0B => Some(Self::Float),
            _ => None,
        }
    }
}

/// Identifies an object category within the Blaze component hierarchy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BlazeObjectType {
    pub component: u16,
    pub object_type: u16,
}

impl BlazeObjectType {
    pub fn new(component: u16, object_type: u16) -> Self {
        Self {
            component,
            object_type,
        }
    }
}

/// Fully qualified identifier for a Blaze domain object (e.g. User, Game, Club).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BlazeObjectId {
    pub object_type: BlazeObjectType,
    pub id: u64,
}

impl BlazeObjectId {
    pub fn new(component: u16, object_type: u16, id: u64) -> Self {
        Self {
            object_type: BlazeObjectType::new(component, object_type),
            id,
        }
    }
}

/// Generic representation of any TDF value.
#[derive(Debug, Clone, PartialEq)]
pub enum TdfValue {
    Integer(u64),
    String(String),
    Blob(Vec<u8>),
    Struct(TdfStruct),
    List(TdfList),
    Map(TdfMap),
    Union(TdfUnion),
    VariableInteger(i64),
    ObjectId(BlazeObjectId),
    ObjectType(BlazeObjectType),
    TimeValue(i64),
    Float(f32),
}

impl TdfValue {
    /// Returns the corresponding wire type for this value.
    pub fn wire_type(&self) -> TdfType {
        match self {
            Self::Integer(_) => TdfType::Integer,
            Self::String(_) => TdfType::String,
            Self::Blob(_) => TdfType::Blob,
            Self::Struct(_) => TdfType::Struct,
            Self::List(_) => TdfType::List,
            Self::Map(_) => TdfType::Map,
            Self::Union(_) => TdfType::Union,
            Self::VariableInteger(_) => TdfType::VariableInteger,
            Self::ObjectId(_) => TdfType::ObjectId,
            Self::ObjectType(_) => TdfType::ObjectType,
            Self::TimeValue(_) => TdfType::TimeValue,
            Self::Float(_) => TdfType::Float,
        }
    }
}

/// Ordered map of tagged values forming a TDF Struct.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TdfStruct {
    pub fields: BTreeMap<String, TdfValue>,
}

impl TdfStruct {
    pub fn new() -> Self {
        Self {
            fields: BTreeMap::new(),
        }
    }

    pub fn set<S: Into<String>>(&mut self, tag: S, val: TdfValue) {
        self.fields.insert(tag.into(), val);
    }

    pub fn get(&self, tag: &str) -> Option<&TdfValue> {
        self.fields.get(tag)
    }
}

/// Homogeneous sequence of TDF values.
#[derive(Debug, Clone, PartialEq)]
pub struct TdfList {
    pub item_type: TdfType,
    pub items: Vec<TdfValue>,
}

impl TdfList {
    pub fn new(item_type: TdfType, items: Vec<TdfValue>) -> Self {
        Self { item_type, items }
    }
}

/// Key-Value collection of TDF values.
#[derive(Debug, Clone, PartialEq)]
pub struct TdfMap {
    pub key_type: TdfType,
    pub val_type: TdfType,
    pub entries: Vec<(TdfValue, TdfValue)>,
}

impl TdfMap {
    pub fn new(key_type: TdfType, val_type: TdfType, entries: Vec<(TdfValue, TdfValue)>) -> Self {
        Self {
            key_type,
            val_type,
            entries,
        }
    }
}

/// Tagged union with an active member or None (0xFF).
#[derive(Debug, Clone, PartialEq)]
pub struct TdfUnion {
    pub active_tag: u8,
    pub value: Option<Box<TdfValue>>,
}

impl TdfUnion {
    pub const EMPTY_TAG: u8 = 0xFF;

    pub fn empty() -> Self {
        Self {
            active_tag: Self::EMPTY_TAG,
            value: None,
        }
    }

    pub fn with_value(active_tag: u8, value: TdfValue) -> Self {
        Self {
            active_tag,
            value: Some(Box::new(value)),
        }
    }
}
