//! 12-Byte Blaze Packet Header and Wire Framing.

use std::fmt::Write as FmtWrite;
use std::io::{self, Read, Write};
use blaze_tdf::{deserialize_tdf, serialize_tdf, TdfStruct};

/// Blaze message types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum MessageType {
    /// 0x0: Client-to-server RPC request.
    SubmitRequest = 0x0,
    /// 0x1: Server-to-client RPC response.
    RecordResponse = 0x1,
    /// 0x2: Server-to-client unsolicited notification.
    Notification = 0x2,
    /// 0x3: Server-to-client RPC error response.
    ErrorResponse = 0x3,
}

impl MessageType {
    pub fn from_u8(val: u8) -> Option<Self> {
        match val {
            0x0 => Some(Self::SubmitRequest),
            0x1 => Some(Self::RecordResponse),
            0x2 => Some(Self::Notification),
            0x3 => Some(Self::ErrorResponse),
            _ => None,
        }
    }
}

/// Formats a byte slice into a standard canonical hex dump string.
pub fn hex_dump(bytes: &[u8]) -> String {
    let mut out = String::new();
    for (chunk_idx, chunk) in bytes.chunks(16).enumerate() {
        let _ = write!(out, "{:04X}  ", chunk_idx * 16);
        for i in 0..16 {
            if i < chunk.len() {
                let _ = write!(out, "{:02X} ", chunk[i]);
            } else {
                out.push_str("   ");
            }
            if i == 7 {
                out.push(' ');
            }
        }
        out.push_str(" |");
        for b in chunk {
            if b.is_ascii_graphic() || *b == b' ' {
                out.push(*b as char);
            } else {
                out.push('.');
            }
        }
        out.push_str("|\n");
    }
    out
}

/// The 12-byte fixed-size header preceding every Blaze message payload.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BlazeHeader {
    /// 24-bit payload length (excluding this 12-byte header).
    pub length: u32,
    /// 16-bit component service identifier.
    pub component: u16,
    /// 16-bit command identifier within the component.
    pub command: u16,
    /// 16-bit error code (0x0000 = ERR_OK).
    pub error_code: u16,
    /// 4-bit message type (Request, Response, Notification, Error).
    pub msg_type: MessageType,
    /// 4-bit Quality of Service / Context flags.
    pub qos: u8,
    /// 16-bit sequence / transaction correlation identifier.
    pub seq_no: u16,
}

impl BlazeHeader {
    pub const HEADER_SIZE: usize = 12;

    pub fn new(
        length: u32,
        component: u16,
        command: u16,
        error_code: u16,
        msg_type: MessageType,
        qos: u8,
        seq_no: u16,
    ) -> Self {
        Self {
            length: length & 0x00FF_FFFF,
            component,
            command,
            error_code,
            msg_type,
            qos: qos & 0x0F,
            seq_no,
        }
    }

    /// Serializes the 12-byte header into a fixed array.
    pub fn to_bytes(&self) -> [u8; Self::HEADER_SIZE] {
        let mut buf = [0u8; Self::HEADER_SIZE];

        // Bytes 0..=2: 24-bit Length (BE)
        buf[0] = ((self.length >> 16) & 0xFF) as u8;
        buf[1] = ((self.length >> 8) & 0xFF) as u8;
        buf[2] = (self.length & 0xFF) as u8;

        // Bytes 3..=4: 16-bit Component (BE)
        buf[3] = ((self.component >> 8) & 0xFF) as u8;
        buf[4] = (self.component & 0xFF) as u8;

        // Bytes 5..=6: 16-bit Command (BE)
        buf[5] = ((self.command >> 8) & 0xFF) as u8;
        buf[6] = (self.command & 0xFF) as u8;

        // Bytes 7..=8: 16-bit Error Code (BE)
        buf[7] = ((self.error_code >> 8) & 0xFF) as u8;
        buf[8] = (self.error_code & 0xFF) as u8;

        // Byte 9: MsgType (high 4b) | QoS (low 4b)
        buf[9] = ((self.msg_type as u8) << 4) | (self.qos & 0x0F);

        // Bytes 10..=11: 16-bit Sequence / Transaction ID (BE)
        buf[10] = ((self.seq_no >> 8) & 0xFF) as u8;
        buf[11] = (self.seq_no & 0xFF) as u8;

        buf
    }

    /// Serializes the 16-byte modern Blaze header (used by FIFA 22 Frostbite 3).
    pub fn to_bytes_16(&self) -> [u8; 16] {
        let mut buf = [0u8; 16];
        buf[0..4].copy_from_slice(&self.length.to_be_bytes());
        buf[4..6].copy_from_slice(&self.error_code.to_be_bytes());
        buf[6..8].copy_from_slice(&self.component.to_be_bytes());
        buf[8..10].copy_from_slice(&self.command.to_be_bytes());
        buf[10] = ((self.msg_type as u8) << 4) | (self.qos & 0x0F);
        buf[11] = 0;
        buf[12..16].copy_from_slice(&(self.seq_no as u32).to_le_bytes());
        buf
    }

    /// Deserializes a 16-byte modern Blaze header from a byte slice.
    pub fn from_bytes_16(buf: &[u8; 16]) -> io::Result<Self> {
        let length = u32::from_be_bytes([buf[0], buf[1], buf[2], buf[3]]);
        let error_code = u16::from_be_bytes([buf[4], buf[5]]);
        let component = u16::from_be_bytes([buf[6], buf[7]]);
        let command = u16::from_be_bytes([buf[8], buf[9]]);
        let type_raw = (buf[10] >> 4) & 0x0F;
        let qos = buf[10] & 0x0F;
        let seq_no = u32::from_le_bytes([buf[12], buf[13], buf[14], buf[15]]) as u16;

        let msg_type = MessageType::from_u8(type_raw).unwrap_or(MessageType::SubmitRequest);

        Ok(Self {
            length,
            component,
            command,
            error_code,
            msg_type,
            qos,
            seq_no,
        })
    }

    /// Deserializes a 12-byte header from a byte slice.
    pub fn from_bytes(buf: &[u8; Self::HEADER_SIZE]) -> io::Result<Self> {
        let length = ((buf[0] as u32) << 16) | ((buf[1] as u32) << 8) | (buf[2] as u32);
        let component = ((buf[3] as u16) << 8) | (buf[4] as u16);
        let command = ((buf[5] as u16) << 8) | (buf[6] as u16);
        let error_code = ((buf[7] as u16) << 8) | (buf[8] as u16);

        let type_raw = (buf[9] >> 4) & 0x0F;
        let qos = buf[9] & 0x0F;

        let msg_type = MessageType::from_u8(type_raw).ok_or_else(|| {
            tracing::warn!("Invalid Blaze message type: 0x{:X}\nRaw Header Dump:\n{}", type_raw, hex_dump(buf));
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Invalid Blaze message type: 0x{:X}", type_raw),
            )
        })?;

        let seq_no = ((buf[10] as u16) << 8) | (buf[11] as u16);

        Ok(Self {
            length,
            component,
            command,
            error_code,
            msg_type,
            qos,
            seq_no,
        })
    }

    /// Writes the header to an I/O writer.
    pub fn write_to<W: Write>(&self, writer: &mut W) -> io::Result<()> {
        writer.write_all(&self.to_bytes())
    }

    /// Reads the header from an I/O reader.
    pub fn read_from<R: Read>(reader: &mut R) -> io::Result<Self> {
        let mut buf = [0u8; Self::HEADER_SIZE];
        reader.read_exact(&mut buf)?;
        Self::from_bytes(&buf)
    }
}

/// A complete framed Blaze message containing a 12-byte header and TDF payload.
#[derive(Debug, Clone, PartialEq)]
pub struct BlazePacket {
    pub header: BlazeHeader,
    pub payload: TdfStruct,
}

impl BlazePacket {
    /// Constructs a new packet and computes the payload length automatically.
    pub fn new(
        component: u16,
        command: u16,
        error_code: u16,
        msg_type: MessageType,
        qos: u8,
        seq_no: u16,
        payload: TdfStruct,
    ) -> io::Result<Self> {
        let encoded_payload = serialize_tdf(&payload)?;
        let length = encoded_payload.len() as u32;

        let header = BlazeHeader::new(
            length, component, command, error_code, msg_type, qos, seq_no,
        );

        Ok(Self { header, payload })
    }

    /// Constructs an error response packet matching a request header.
    pub fn error_response(req_header: &BlazeHeader, error_code: u16) -> io::Result<Self> {
        Self::new(
            req_header.component,
            req_header.command,
            error_code,
            MessageType::ErrorResponse,
            0,
            req_header.seq_no,
            TdfStruct::new(),
        )
    }

    /// Constructs a success response packet matching a request header.
    pub fn response(req_header: &BlazeHeader, payload: TdfStruct) -> io::Result<Self> {
        Self::new(
            req_header.component,
            req_header.command,
            0x0000,
            MessageType::RecordResponse,
            0,
            req_header.seq_no,
            payload,
        )
    }

    /// Constructs an asynchronous notification packet.
    pub fn notification(component: u16, command: u16, payload: TdfStruct) -> io::Result<Self> {
        Self::new(
            component,
            command,
            0x0000,
            MessageType::Notification,
            0,
            0,
            payload,
        )
    }

    /// Serializes the entire packet (12-byte header + TDF body) into a byte buffer.
    pub fn encode(&self) -> io::Result<Vec<u8>> {
        let payload_bytes = serialize_tdf(&self.payload)?;
        let mut header = self.header;
        header.length = payload_bytes.len() as u32;

        let mut buf = Vec::with_capacity(BlazeHeader::HEADER_SIZE + payload_bytes.len());
        buf.extend_from_slice(&header.to_bytes());
        buf.extend_from_slice(&payload_bytes);
        Ok(buf)
    }

    /// Serializes the entire packet using the 16-byte header format (FIFA 22 Frostbite 3).
    pub fn encode_16(&self) -> io::Result<Vec<u8>> {
        let payload_bytes = serialize_tdf(&self.payload)?;
        let mut header = self.header;
        header.length = payload_bytes.len() as u32;

        let mut buf = Vec::with_capacity(16 + payload_bytes.len());
        buf.extend_from_slice(&header.to_bytes_16());
        buf.extend_from_slice(&payload_bytes);
        Ok(buf)
    }

    /// Deserializes a packet from an I/O reader with diagnostic hex dump logging.
    pub fn decode<R: Read>(reader: &mut R) -> io::Result<Self> {
        let header = BlazeHeader::read_from(reader)?;

        let mut payload_bytes = vec![0u8; header.length as usize];
        if let Err(e) = reader.read_exact(&mut payload_bytes) {
            tracing::error!(
                "Failed to read payload of {} bytes for component 0x{:04X} command 0x{:04X}: {:?}",
                header.length,
                header.component,
                header.command,
                e
            );
            return Err(e);
        }

        let payload = match deserialize_tdf(&payload_bytes) {
            Ok(p) => p,
            Err(e) => {
                tracing::error!(
                    "TDF Deserialization failure on Component 0x{:04X} Command 0x{:04X}:\nRaw Payload Hex Dump:\n{}",
                    header.component,
                    header.command,
                    hex_dump(&payload_bytes)
                );
                return Err(e);
            }
        };

        Ok(Self { header, payload })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use blaze_tdf::TdfValue;

    #[test]
    fn test_hex_dump_output() {
        let data = b"Hello, EA Blaze!";
        let dump = hex_dump(data);
        assert!(dump.contains("Hello, EA Blaze!"));
        assert!(dump.contains("0000"));
    }

    #[test]
    fn test_blaze_header_roundtrip() {
        let header = BlazeHeader::new(
            1024,
            0x0004, // GameManager
            0x0001, // createGame
            0x0000,
            MessageType::SubmitRequest,
            0x0,
            42,
        );

        let bytes = header.to_bytes();
        assert_eq!(bytes.len(), 12);

        let recovered = BlazeHeader::from_bytes(&bytes).expect("Failed to parse header");
        assert_eq!(header, recovered);
        assert_eq!(recovered.component, 0x0004);
        assert_eq!(recovered.command, 0x0001);
        assert_eq!(recovered.msg_type, MessageType::SubmitRequest);
        assert_eq!(recovered.seq_no, 42);
        assert_eq!(recovered.length, 1024);
    }

    #[test]
    fn test_blaze_packet_roundtrip() {
        let mut struct_payload = TdfStruct::new();
        struct_payload.set("GNAM", TdfValue::String("TestLobby".to_string()));
        struct_payload.set("GTYP", TdfValue::Integer(1));

        let packet = BlazePacket::new(
            0x0004,
            0x0001,
            0x0000,
            MessageType::SubmitRequest,
            0x0,
            100,
            struct_payload,
        )
        .expect("Packet creation failed");

        let encoded = packet.encode().expect("Packet encoding failed");
        assert!(encoded.len() > 12);

        let mut cursor = std::io::Cursor::new(encoded);
        let decoded = BlazePacket::decode(&mut cursor).expect("Packet decoding failed");

        assert_eq!(packet.header.component, decoded.header.component);
        assert_eq!(packet.header.command, decoded.header.command);
        assert_eq!(packet.header.seq_no, decoded.header.seq_no);
        assert_eq!(
            decoded.payload.get("GNAM"),
            Some(&TdfValue::String("TestLobby".to_string()))
        );
        assert_eq!(decoded.payload.get("GTYP"), Some(&TdfValue::Integer(1)));
    }
}
