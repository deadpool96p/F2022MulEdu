//! 6-Bit Character Tag Encoding and Decoding for EA Blaze Protocol TDF.
//!
//! Blaze struct tags are 24-bit integers holding 1 to 4 ASCII characters.
//! Each character is compressed into exactly 6 bits (values 0..=63),
//! allowing 4 characters to fit into 24 bits (3 bytes) without bit overlap.

/// Encodes an ASCII byte into a 6-bit Blaze character chunk (0..=63).
#[inline]
pub fn char_to_6bit(c: u8) -> u32 {
    if c == 0 {
        0
    } else if (c & 0x20) == 0 {
        // Uppercase ASCII letter or symbol ('A'-'Z', '@', etc.)
        // ASCII bit 5 is 0. In 6-bit encoding, bit 5 is set: 0x20 | (c & 0x1F)
        0x20 | ((c as u32) & 0x1F)
    } else {
        // Digit or symbol ('0'-'9', '_', etc.)
        // ASCII bit 5 is 1. In 6-bit encoding, bit 5 is cleared: (c & 0x1F)
        (c as u32) & 0x1F
    }
}

/// Decodes a 6-bit Blaze character chunk back into an ASCII char.
#[inline]
pub fn sixbit_to_char(p: u32) -> Option<char> {
    let p = p & 0x3F;
    if p == 0 {
        None
    } else if (p & 0x20) != 0 {
        // Bit 5 set: reconstruct uppercase letter (ASCII bit 6=1, bit 5=0)
        Some((((p & 0x1F) | 0x40) as u8) as char)
    } else {
        // Bit 5 clear: reconstruct digit/symbol (ASCII bit 6=0, bit 5=1)
        Some((((p & 0x1F) | 0x20) as u8) as char)
    }
}

/// Encodes a 1-to-4 character ASCII tag into a 24-bit Blaze tag integer.
pub fn encode_tdf_tag(tag_str: &str) -> u32 {
    let mut bytes = [0u8; 4];
    for (i, b) in tag_str.as_bytes().iter().take(4).enumerate() {
        bytes[i] = *b;
    }

    let p0 = char_to_6bit(bytes[0]);
    let p1 = char_to_6bit(bytes[1]);
    let p2 = char_to_6bit(bytes[2]);
    let p3 = char_to_6bit(bytes[3]);

    // Pack 4 x 6-bit values into exactly 24 bits
    (p0 << 18) | (p1 << 12) | (p2 << 6) | p3
}

/// Decodes a 24-bit Blaze tag integer back into an ASCII string.
pub fn decode_tdf_tag(tag: u32) -> String {
    let p0 = (tag >> 18) & 0x3F;
    let p1 = (tag >> 12) & 0x3F;
    let p2 = (tag >> 6) & 0x3F;
    let p3 = tag & 0x3F;

    let mut result = String::with_capacity(4);
    for p in [p0, p1, p2, p3] {
        if let Some(ch) = sixbit_to_char(p) {
            result.push(ch);
        }
    }
    result
}

/// Writes a 24-bit tag to a 3-byte big-endian array.
pub fn tag_to_bytes(tag: u32) -> [u8; 3] {
    [
        ((tag >> 16) & 0xFF) as u8,
        ((tag >> 8) & 0xFF) as u8,
        (tag & 0xFF) as u8,
    ]
}

/// Reads a 24-bit tag from a 3-byte big-endian slice.
pub fn tag_from_bytes(bytes: &[u8]) -> u32 {
    ((bytes[0] as u32) << 16) | ((bytes[1] as u32) << 8) | (bytes[2] as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tag_roundtrip_standard() {
        let test_tags = [
            "GNAM", "GTYP", "GSET", "ATTR", "DATA", "SEED", "ID", "A", "0001", "PORT", "NATT",
            "EXIP", "INIP", "SQDH",
        ];
        for tag in test_tags {
            let encoded = encode_tdf_tag(tag);
            assert!(encoded <= 0x00FF_FFFF, "Tag {} exceeded 24 bits: 0x{:08X}", tag, encoded);
            let decoded = decode_tdf_tag(encoded);
            assert_eq!(tag, decoded, "Tag round-trip failed for {}", tag);
        }
    }

    #[test]
    fn test_tag_byte_serialization() {
        let tag = encode_tdf_tag("GNAM");
        let bytes = tag_to_bytes(tag);
        assert_eq!(bytes.len(), 3);
        let recovered = tag_from_bytes(&bytes);
        assert_eq!(tag, recovered);
        assert_eq!("GNAM", decode_tdf_tag(recovered));
    }
}
