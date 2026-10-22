//! TDF Type definitions and Value enumerations for EA Blaze Protocol.

use std::collections::BTreeMap;

/// Tagged Data Format (TDF) Wire Type Identifiers.
///
/// The identifiers below are the **Heat2** numbering, pinned by the twelve
/// reference fixtures in the reference repo's test suite and corroborated by
/// live EA traffic. They are *not* the numbers in the older BlazeSDK
/// documentation, where `ObjectId` and `ObjectType` are swapped relative to
/// Heat2 and `TimeValue` and `Float` are swapped as well.
///
/// | id  | type       | wire form                                     |
/// | --- | ---------- | --------------------------------------------- |
/// | 0x0 | Integer    | varsize integer                               |
/// | 0x1 | String     | varsize length (incl. NUL) + bytes + NUL      |
/// | 0x2 | Blob       | varsize length + bytes                        |
/// | 0x3 | Struct     | tagged fields + `0x00` terminator             |
/// | 0x4 | List       | element type + varsize count + elements       |
/// | 0x5 | Map        | key type + value type + varsize count + pairs |
/// | 0x6 | Union      | case index + member tag + type + value        |
/// | 0x7 | Variable   | present + TDF id + member tag + type + value  |
/// | 0x8 | ObjectType | varsize component + varsize type              |
/// | 0x9 | ObjectId   | varsize component + varsize type + varsize id |
/// | 0xA | Float      | 4 bytes, IEEE 754 big-endian                  |
/// | 0xB | TimeValue  | varsize integer (microseconds)                |
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
    /// 0x06: Tagged union (1-byte case index + 3-byte member tag + inner field).
    Union = 0x06,
    /// 0x07: Variable (present flag + TDF id + 3-byte member tag + inner field).
    Variable = 0x07,
    /// 0x08: Blaze Object Type (varsize component + varsize type).
    ObjectType = 0x08,
    /// 0x09: Blaze Object ID (varsize component + varsize type + varsize id).
    ObjectId = 0x09,
    /// 0x0A: 32-bit floating point number (IEEE 754 big-endian).
    Float = 0x0A,
    /// 0x0B: Time value in microseconds (varsize integer).
    TimeValue = 0x0B,
    /// 0x0C: Generic value (two varsize u16s).
    ///
    /// FIFA 22's Heat2 codec produces this byte from internal type id
    /// `0x01` (see `FUN_14496BB30` at `0x14496BB30`). The Go reference
    /// lists it in its type table but never emits it, and this crate was
    /// missing it entirely.
    Generic = 0x0C,
    /// 0x0D: Unknown / out-of-range type.
    ///
    /// The binary's default case for an internal type id it does not
    /// recognise (id `0x00`, or anything above `0x17`) writes this byte.
    /// The Go reference uses `0xFF` as its sentinel instead, which the
    /// client never sends.
    Unknown = 0x0D,
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
            0x07 => Some(Self::Variable),
            0x08 => Some(Self::ObjectType),
            0x09 => Some(Self::ObjectId),
            0x0A => Some(Self::Float),
            0x0B => Some(Self::TimeValue),
            0x0C => Some(Self::Generic),
            0x0D => Some(Self::Unknown),
            _ => None,
        }
    }
}

/// Converts a dotted-quad IPv4 string into the 32-bit value EA stores.
///
/// EA's `IpAddress` struct carries the address as a single unsigned integer
/// in network byte order, not as four separate octets. The authentic EA
/// `UserSessions` frame stores `192.168.1.7` as `3232236743`, which is
/// `0xC0A80107`.
///
/// Returns `None` for anything that is not a valid dotted quad.
pub fn ipv4_to_u32(addr: &str) -> Option<u32> {
    let mut octets = [0u8; 4];
    let mut count = 0;
    for part in addr.split('.') {
        if count == 4 {
            return None;
        }
        octets[count] = part.parse::<u8>().ok()?;
        count += 1;
    }
    if count != 4 {
        return None;
    }
    Some(u32::from_be_bytes(octets))
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
    Variable(TdfVariable),
    ObjectId(BlazeObjectId),
    ObjectType(BlazeObjectType),
    TimeValue(i64),
    Float(f32),
    /// A `GENERIC` (wire type `0x0C`) value.
    ///
    /// The wire form is a presence byte followed, when present, by a varsize
    /// integer:
    ///
    /// ```text
    /// [ presence (0x00 | 0x01) ] [ varsize integer, only when present ]
    /// ```
    ///
    /// Confirmed from the retail binary's encoder case for internal type id
    /// `0x01` (`FUN_14496AC20` at `0x14496B31A`): it writes the member header
    /// with `r8d = 0x0C`, then the presence byte, then the varsize integer
    /// when the value is present.
    Generic { present: bool, value: u64 },
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
            Self::Variable(_) => TdfType::Variable,
            Self::ObjectId(_) => TdfType::ObjectId,
            Self::ObjectType(_) => TdfType::ObjectType,
            Self::TimeValue(_) => TdfType::TimeValue,
            Self::Float(_) => TdfType::Float,
            Self::Generic { .. } => TdfType::Generic,
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
///
/// The Heat2 wire form is
///
/// ```text
/// [case index]  1 byte
/// [member tag]  3 bytes
/// [member type] 1 byte
/// [value]       depends on the member type
/// ```
///
/// The *case index* selects which member of the union is active, and the
/// *member tag* names the field that carries the value. For the anonymous
/// unions EA uses in practice (a union whose cases are anonymous structs)
/// the member tag is always `VALU`.
///
/// Both facts are pinned by authentic EA bytes: the captured
/// `UserSessions::updateNetworkInfo` reply carries
/// `ADDR` as case `2` with member tag `VALU` holding an `IpPairAddress`
/// struct, and the reference repo's union fixture carries case `2` with
/// member tag `MBR_` holding an integer.
#[derive(Debug, Clone, PartialEq)]
pub struct TdfUnion {
    /// Index of the active union case.
    pub active_tag: u8,
    /// The 3-byte tag that names the member value.
    pub member_tag: String,
    /// The active member's value, or `None` for an empty union.
    pub value: Option<Box<TdfValue>>,
}

impl TdfUnion {
    /// Marks an empty union.
    pub const EMPTY_TAG: u8 = 0xFF;
    /// The other value EA treats as "no active member".
    pub const INVALID_TAG: u8 = 0x7F;
    /// The member tag EA uses for anonymous union cases.
    pub const DEFAULT_MEMBER_TAG: &'static str = "VALU";

    /// Builds an empty union.
    pub fn empty() -> Self {
        Self {
            active_tag: Self::EMPTY_TAG,
            member_tag: String::new(),
            value: None,
        }
    }

    /// Builds a union with the given case index and value, using EA's default
    /// `VALU` member tag.
    pub fn with_value(active_tag: u8, value: TdfValue) -> Self {
        Self::with_member(active_tag, Self::DEFAULT_MEMBER_TAG, value)
    }

    /// Builds a union with an explicit member tag.
    pub fn with_member(active_tag: u8, member_tag: &str, value: TdfValue) -> Self {
        Self {
            active_tag,
            member_tag: member_tag.to_string(),
            value: Some(Box::new(value)),
        }
    }
}

/// A TDF variable: a value whose concrete type is only known at runtime.
///
/// The Heat2 wire form is
///
/// ```text
/// [present]     0x01 when set, 0x00 when absent
/// [TDF id]      varsize integer
/// [member tag]  3 bytes
/// [member type] 1 byte
/// [value]       depends on the member type
/// [terminator]  0x00
/// ```
///
/// An absent variable is just the single `0x00` byte.
#[derive(Debug, Clone, PartialEq)]
pub struct TdfVariable {
    /// The TDF type definition id this variable refers to.
    pub tdf_id: u32,
    /// The 3-byte tag that names the member value.
    pub member_tag: String,
    /// The variable's value, or `None` when the variable is absent.
    pub value: Option<Box<TdfValue>>,
}

impl TdfVariable {
    /// Builds an absent variable.
    pub fn absent() -> Self {
        Self {
            tdf_id: 0,
            member_tag: String::new(),
            value: None,
        }
    }

    /// Builds a present variable.
    pub fn new(tdf_id: u32, member_tag: &str, value: TdfValue) -> Self {
        Self {
            tdf_id,
            member_tag: member_tag.to_string(),
            value: Some(Box::new(value)),
        }
    }
}
