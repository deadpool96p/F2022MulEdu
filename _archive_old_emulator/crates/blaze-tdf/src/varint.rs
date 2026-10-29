//! Variable-length integer (VarInt) encoding and decoding for the Heat2 TDF codec.
//!
//! FIFA 22 speaks BlazeSDK 15, whose payload codec is "Heat2". Heat2 integers are
//! *varsize* encoded, which is **not** the plain LSB-first 7-bit (LEB128) scheme used
//! by the older 12-byte FIRE generation:
//!
//! * the first byte carries only 6 value bits (`value & 0x3F`),
//! * bit `0x40` of the first byte is the **negative** flag,
//! * bit `0x80` of the first byte means "more bytes follow",
//! * every continuation byte carries 7 value bits at shifts 6, 13, 20, 27, ...
//!
//! The two schemes disagree on every value >= 0x40, and Heat2 silently decodes LEB128
//! bytes as a *different* number rather than failing. Encoding the `Util::ping`
//! server-time stamp with LEB128 makes the client read a 1998 timestamp instead of
//! the current one, which trips its clock-drift check and its 10-second watchdog.
//!
//! The scheme below is the "varsize" implementation used by `mxmstr/hp-server`
//! (`EncodeVarsizeInteger`) and `Khysnik/BlazeSDK` (`DecodeVarsizeInteger`), two
//! independent clean-room implementations of the same EA codec, and is corroborated by
//! the commented-out `Tdf::CompressInteger` in `openBlase`.

use std::io::{self, Read, Write};

/// Sign flag carried in bit 6 of the first varsize byte.
const SIGN_BIT: u8 = 0x40;
/// Continuation flag carried in bit 7 of every varsize byte.
const CONT_BIT: u8 = 0x80;
/// Number of value bits carried in the first varsize byte.
const FIRST_BITS: u32 = 6;
/// Number of value bits carried in each continuation byte.
const REST_BITS: u32 = 7;

/// Reads a Heat2 varsize integer, returning its magnitude and the sign flag.
fn read_varsize<R: Read>(reader: &mut R) -> io::Result<(u64, bool)> {
    let mut first = [0u8; 1];
    reader.read_exact(&mut first)?;
    let first = first[0];

    let negative = first & SIGN_BIT != 0;
    let mut magnitude = (first & 0x3F) as u64;

    if first & CONT_BIT != 0 {
        let mut shift = FIRST_BITS;
        loop {
            let mut next = [0u8; 1];
            reader.read_exact(&mut next)?;
            let next = next[0];

            magnitude |= ((next & 0x7F) as u64).wrapping_shl(shift);

            if next & CONT_BIT == 0 {
                break;
            }

            shift += REST_BITS;
            if shift >= 64 {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "Heat2 varsize integer overflows 64 bits",
                ));
            }
        }
    }

    Ok((magnitude, negative))
}

/// Writes a Heat2 varsize integer from a magnitude plus an explicit sign flag.
fn write_varsize<W: Write>(writer: &mut W, magnitude: u64, negative: bool) -> io::Result<usize> {
    let mut remaining = magnitude;

    let mut first = (remaining & 0x3F) as u8;
    remaining >>= FIRST_BITS;
    if negative {
        first |= SIGN_BIT;
    }
    if remaining > 0 {
        first |= CONT_BIT;
    }

    writer.write_all(&[first])?;
    let mut written = 1;

    while remaining > 0 {
        let mut next = (remaining & 0x7F) as u8;
        remaining >>= REST_BITS;
        if remaining > 0 {
            next |= CONT_BIT;
        }
        writer.write_all(&[next])?;
        written += 1;
    }

    Ok(written)
}

/// Reads an unsigned Heat2 varsize integer.
///
/// A negative varsize integer is returned in two's complement, mirroring how Heat2
/// stores a negative INTEGER in an unsigned field.
pub fn read_varint_u64<R: Read>(reader: &mut R) -> io::Result<u64> {
    let (magnitude, negative) = read_varsize(reader)?;
    if negative {
        Ok(0u64.wrapping_sub(magnitude))
    } else {
        Ok(magnitude)
    }
}

/// Writes an unsigned Heat2 varsize integer.
pub fn write_varint_u64<W: Write>(writer: &mut W, value: u64) -> io::Result<usize> {
    write_varsize(writer, value, false)
}

/// Reads a signed Heat2 varsize integer.
pub fn read_varint_i64<R: Read>(reader: &mut R) -> io::Result<i64> {
    let (magnitude, negative) = read_varsize(reader)?;
    if negative {
        Ok((magnitude as i64).wrapping_neg())
    } else {
        Ok(magnitude as i64)
    }
}

/// Writes a signed Heat2 varsize integer.
pub fn write_varint_i64<W: Write>(writer: &mut W, value: i64) -> io::Result<usize> {
    if value < 0 {
        write_varsize(writer, value.unsigned_abs(), true)
    } else {
        write_varsize(writer, value as u64, false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_varint_roundtrip_unsigned() {
        let test_cases = [
            0u64,
            1,
            63,
            64,
            127,
            128,
            255,
            16383,
            16384,
            u32::MAX as u64,
            u64::MAX - 1,
            u64::MAX,
        ];
        for val in test_cases {
            let mut buf = Vec::new();
            write_varint_u64(&mut buf, val).expect("write failed");
            let mut cursor = std::io::Cursor::new(buf);
            let read_val = read_varint_u64(&mut cursor).expect("read failed");
            assert_eq!(val, read_val, "Roundtrip failed for {}", val);
        }
    }

    #[test]
    fn test_varint_roundtrip_signed() {
        let test_cases = [0i64, 1, -1, 63, -63, 127, -128, 16384, -16384, i64::MIN, i64::MAX];
        for val in test_cases {
            let mut buf = Vec::new();
            write_varint_i64(&mut buf, val).expect("write failed");
            let mut cursor = std::io::Cursor::new(buf);
            let read_val = read_varint_i64(&mut cursor).expect("read failed");
            assert_eq!(val, read_val, "Signed roundtrip failed for {}", val);
        }
    }

    #[test]
    fn test_heat2_first_byte_holds_only_six_value_bits() {
        // LEB128 would leak bit 6 into the first byte; Heat2 reserves it for the sign.
        let mut buf = Vec::new();
        write_varint_u64(&mut buf, 0x40).expect("write failed");
        assert_eq!(buf, vec![0x80, 0x01]);
        assert_eq!(buf[0] & 0x3F, 0x00);
    }

    #[test]
    fn test_heat2_negative_flag_is_bit_0x40() {
        let mut buf = Vec::new();
        write_varint_i64(&mut buf, -1).expect("write failed");
        assert_eq!(buf, vec![0x41]);
        assert_eq!(buf[0] & 0x40, 0x40);
    }
}
