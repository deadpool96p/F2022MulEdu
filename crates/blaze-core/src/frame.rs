//! 12-Byte Blaze Packet Header and Wire Framing.

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

    /// Deserializes a 12-byte header from a byte slice.
    pub fn from_bytes(buf: &[u8; Self::HEADER_SIZE]) -> io::Result<Self> {
        let length = ((buf[0] as u32) << 16) | ((buf[1] as u32) << 8) | (buf[2] as u32);
        let component = ((buf[3] as u16) << 8) | (buf[4] as u16);
        let command = ((buf[5] as u16) << 8) | (buf[6] as u16);
        let error_code = ((buf[7] as u16) << 8) | (buf[8] as u16);

        let type_raw = (buf[9] >> 4) & 0x0F;
        let qos = buf[9] & 0x0F;

        let msg_type = MessageType::from_u8(type_raw).ok_or_else(|| {
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

    /// Deserializes a packet from an I/O reader.
    pub fn decode<R: Read>(reader: &mut R) -> io::Result<Self> {
        let header = BlazeHeader::read_from(reader)?;

        let mut payload_bytes = vec![0u8; header.length as usize];
        reader.read_exact(&mut payload_bytes)?;

        let payload = deserialize_tdf(&payload_bytes)?;

        Ok(Self { header, payload })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use blaze_tdf::TdfValue;

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
