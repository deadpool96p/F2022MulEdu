//! Variable-length integer (VarInt) encoding and decoding for Blaze TDF.
//!
//! Blaze encodes numbers and collection lengths using a 7-bit continuation byte format.

use std::io::{self, Read, Write};

/// Reads an unsigned variable-length integer from a reader.
pub fn read_varint_u64<R: Read>(reader: &mut R) -> io::Result<u64> {
    let mut result: u64 = 0;
    let mut shift = 0;

    loop {
        let mut byte = [0u8; 1];
        reader.read_exact(&mut byte)?;
        let b = byte[0];

        result |= ((b & 0x7F) as u64) << shift;

        if (b & 0x80) == 0 {
            break;
        }

        shift += 7;
        if shift >= 64 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "VarInt overflow exceeding 64 bits",
            ));
        }
    }

    Ok(result)
}

/// Writes an unsigned variable-length integer to a writer.
pub fn write_varint_u64<W: Write>(writer: &mut W, mut value: u64) -> io::Result<usize> {
    let mut bytes_written = 0;

    loop {
        let mut byte = (value & 0x7F) as u8;
        value >>= 7;

        if value != 0 {
            byte |= 0x80;
        }

        writer.write_all(&[byte])?;
        bytes_written += 1;

        if value == 0 {
            break;
        }
    }

    Ok(bytes_written)
}

/// Reads a signed variable-length integer using zigzag decoding.
pub fn read_varint_i64<R: Read>(reader: &mut R) -> io::Result<i64> {
    let raw = read_varint_u64(reader)?;
    // Zigzag decode: (n >>> 1) ^ -(n & 1)
    let val = ((raw >> 1) as i64) ^ (-((raw & 1) as i64));
    Ok(val)
}

/// Writes a signed variable-length integer using zigzag encoding.
pub fn write_varint_i64<W: Write>(writer: &mut W, value: i64) -> io::Result<usize> {
    // Zigzag encode: (n << 1) ^ (n >> 63)
    let raw = ((value << 1) ^ (value >> 63)) as u64;
    write_varint_u64(writer, raw)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_varint_roundtrip_unsigned() {
        let test_cases = [
            0u64,
            1,
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
        let test_cases = [0i64, 1, -1, 127, -128, 16384, -16384, i64::MIN, i64::MAX];
        for val in test_cases {
            let mut buf = Vec::new();
            write_varint_i64(&mut buf, val).expect("write failed");
            let mut cursor = std::io::Cursor::new(buf);
            let read_val = read_varint_i64(&mut cursor).expect("read failed");
            assert_eq!(val, read_val, "Signed roundtrip failed for {}", val);
        }
    }
}
