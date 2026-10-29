//! 12-Byte Blaze Packet Header and Wire Framing.

use std::fmt::Write as FmtWrite;
use std::io::{self, Read, Write};
use blaze_tdf::{deserialize_tdf, serialize_tdf, TdfStruct};

/// Blaze message types.
///
/// The 12-byte "legacy FIRE" generation only ever used 0x0..=0x3, but Fire2
/// (the generation FIFA 22 speaks) defines six. The two extra values are
/// reserved for the transport-level keep-alive that never reaches the RPC
/// layer, so they are modelled here for completeness.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum MessageType {
    /// 0x0: Client-to-server RPC request (Fire2 calls this MESSAGE).
    SubmitRequest = 0x0,
    /// 0x1: Server-to-client RPC response (Fire2 calls this REPLY).
    RecordResponse = 0x1,
    /// 0x2: Server-to-client unsolicited notification.
    Notification = 0x2,
    /// 0x3: Server-to-client RPC error response (Fire2 calls this ERROR_REPLY).
    ErrorResponse = 0x3,
    /// 0x4: Transport-level ping (Fire2 only).
    Ping = 0x4,
    /// 0x5: Transport-level ping reply (Fire2 only).
    PingReply = 0x5,
}

impl MessageType {
    pub fn from_u8(val: u8) -> Option<Self> {
        match val {
            0x0 => Some(Self::SubmitRequest),
            0x1 => Some(Self::RecordResponse),
            0x2 => Some(Self::Notification),
            0x3 => Some(Self::ErrorResponse),
            0x4 => Some(Self::Ping),
            0x5 => Some(Self::PingReply),
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

/// The Blaze frame header.
///
/// Two *incompatible* generations share this struct:
///
/// * the **12-byte legacy FIRE** header, used by the redirector, the telemetry
///   sink and our own integration harness, where bytes 4..6 are the error code
///   and byte 9 packs `msgType << 4 | qos`;
/// * the **16-byte Fire2** header, used by FIFA 22, where bytes 4..6 are the
///   *metadata size*, the error code lives in the metadata section as the TDF
///   field `ERRC`, and byte 13 packs `msgType << 5 | userIndex`.
///
/// The two paths therefore have separate encoders (`to_bytes` / `to_bytes_16`)
/// and separate decoders (`from_bytes` / `from_bytes_16`). The Fire2 fields
/// that have no legacy counterpart (`metadata_size`, `message_number`,
/// `user_index`, `options`) are simply unused on the legacy path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BlazeHeader {
    /// Payload length in bytes, excluding the header (and, on Fire2, excluding
    /// the metadata section).
    pub length: u32,
    /// 16-bit component service identifier.
    pub component: u16,
    /// 16-bit command identifier within the component.
    pub command: u16,
    /// 16-bit error code (0x0000 = ERR_OK).
    ///
    /// On Fire2 this is *not* transmitted in the header: it is carried in the
    /// metadata section as the TDF field `ERRC`. It is still tracked here so
    /// the router can reason about failures uniformly.
    pub error_code: u16,
    /// Message type.
    pub msg_type: MessageType,
    /// 4-bit Quality of Service / Context flags (legacy header only).
    pub qos: u8,
    /// 16-bit sequence / transaction correlation identifier (legacy header).
    pub seq_no: u16,
    /// Fire2: size in bytes of the metadata section that follows the header.
    pub metadata_size: u16,
    /// Fire2: 24-bit message number at bytes 10..13.
    pub message_number: u32,
    /// Fire2: 5-bit user index, packed into byte 13 alongside the message type.
    pub user_index: u8,
    /// Fire2: options byte (bit 0 = OPTION_IMMEDIATE).
    pub options: u8,
    /// Fire2: the `CNTX` context value carried in the metadata section.
    pub context: u64,
}

impl BlazeHeader {
    /// Size of the 12-byte legacy FIRE header.
    pub const HEADER_SIZE: usize = 12;
    /// Size of the 16-byte Fire2 header.
    pub const HEADER_SIZE_16: usize = 16;

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
            metadata_size: 0,
            message_number: 0,
            user_index: 0,
            options: 0,
            context: 0,
        }
    }

    /// Builds a Fire2 header.
    ///
    /// `length` is the payload size in bytes (metadata excluded), `message_number`
    /// is the 24-bit transaction number, and `user_index` identifies the
    /// connection within a multiplexed session.
    pub fn new_fire2(
        length: u32,
        component: u16,
        command: u16,
        error_code: u16,
        msg_type: MessageType,
        message_number: u32,
        user_index: u8,
    ) -> Self {
        Self {
            length,
            component,
            command,
            error_code,
            msg_type,
            qos: 0,
            seq_no: 0,
            metadata_size: 0,
            message_number: message_number & 0x00FF_FFFF,
            user_index: user_index & 0x1F,
            options: 0,
            context: 0,
        }
    }

    /// Serializes the 12-byte legacy header into a fixed array.
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

    /// Serializes the 16-byte Fire2 header.
    ///
    /// ```text
    /// [0..4)   payloadSize    u32 big-endian  -- payload bytes only
    /// [4..6)   metadataSize   u16 big-endian  -- 0 when there is no metadata
    /// [6..8)   componentID    u16 big-endian
    /// [8..10)  commandID      u16 big-endian
    /// [10..13) messageNumber  u24 big-endian
    /// [13]     messageType << 5 | userIndex & 0x1F
    /// [14]     options        (OPTION_IMMEDIATE = 0x01)
    /// [15]     reserved       (0)
    /// ```
    pub fn to_bytes_16(&self) -> [u8; Self::HEADER_SIZE_16] {
        let mut buf = [0u8; Self::HEADER_SIZE_16];
        buf[0..4].copy_from_slice(&self.length.to_be_bytes());
        buf[4..6].copy_from_slice(&self.metadata_size.to_be_bytes());
        buf[6..8].copy_from_slice(&self.component.to_be_bytes());
        buf[8..10].copy_from_slice(&self.command.to_be_bytes());
        buf[10] = ((self.message_number >> 16) & 0xFF) as u8;
        buf[11] = ((self.message_number >> 8) & 0xFF) as u8;
        buf[12] = (self.message_number & 0xFF) as u8;
        buf[13] = ((self.msg_type as u8) << 5) | (self.user_index & 0x1F);
        buf[14] = self.options;
        // buf[15] stays 0 (reserved)
        buf
    }

    /// Deserializes a 16-byte Fire2 header from a byte slice.
    pub fn from_bytes_16(buf: &[u8; Self::HEADER_SIZE_16]) -> io::Result<Self> {
        let length = u32::from_be_bytes([buf[0], buf[1], buf[2], buf[3]]);
        let metadata_size = u16::from_be_bytes([buf[4], buf[5]]);
        let component = u16::from_be_bytes([buf[6], buf[7]]);
        let command = u16::from_be_bytes([buf[8], buf[9]]);
        let message_number = ((buf[10] as u32) << 16) | ((buf[11] as u32) << 8) | (buf[12] as u32);

        // Byte 13 packs the message type in the top 3 bits and the user index
        // in the low 5.
        let type_raw = buf[13] >> 5;
        let user_index = buf[13] & 0x1F;

        let msg_type = MessageType::from_u8(type_raw).ok_or_else(|| {
            tracing::warn!(
                "Invalid Fire2 message type: 0x{:X}\nRaw Header Dump:\n{}",
                type_raw,
                hex_dump(buf)
            );
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Invalid Fire2 message type: 0x{:X}", type_raw),
            )
        })?;

        Ok(Self {
            length,
            component,
            command,
            // Fire2 carries no error code in the header; it lives in the
            // metadata section as `ERRC`.
            error_code: 0,
            msg_type,
            qos: 0,
            seq_no: 0,
            metadata_size,
            message_number,
            user_index,
            options: buf[14],
            context: 0,
        })
    }

    /// Deserializes a 12-byte legacy header from a byte slice.
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
            metadata_size: 0,
            message_number: 0,
            user_index: 0,
            options: 0,
            context: 0,
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
        let mut packet = Self::new(
            req_header.component,
            req_header.command,
            error_code,
            MessageType::ErrorResponse,
            0,
            req_header.seq_no,
            TdfStruct::new(),
        )?;
        packet.header.message_number = req_header.message_number;
        packet.header.user_index = req_header.user_index;
        Ok(packet)
    }

    /// Constructs a success response packet matching a request header.
    pub fn response(req_header: &BlazeHeader, payload: TdfStruct) -> io::Result<Self> {
        let mut packet = Self::new(
            req_header.component,
            req_header.command,
            0x0000,
            MessageType::RecordResponse,
            0,
            req_header.seq_no,
            payload,
        )?;
        packet.header.message_number = req_header.message_number;
        packet.header.user_index = req_header.user_index;
        Ok(packet)
    }

    /// Constructs the transport-level reply to a Fire2 `PING` frame.
    ///
    /// A Fire2 `PING` is message type 4 and is answered with a `PING_REPLY`
    /// (message type 5) -- **not** with an ordinary RPC response. The reply
    /// carries no payload, and echoes the request's 24-bit message number
    /// and 5-bit user index.
    ///
    /// Answering a `PING` with a `RECORD_RESPONSE` leaves the client's
    /// transport keep-alive unsatisfied, and it reconnects.
    pub fn ping_reply(req_header: &BlazeHeader) -> io::Result<Self> {
        let mut packet = Self::new(
            req_header.component,
            req_header.command,
            0x0000,
            MessageType::PingReply,
            0,
            req_header.seq_no,
            TdfStruct::new(),
        )?;
        packet.header.message_number = req_header.message_number;
        packet.header.user_index = req_header.user_index;
        Ok(packet)
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

    /// Serializes the entire packet using the 16-byte Fire2 header format.
    ///
    /// The frame is laid out as
    ///
    /// ```text
    /// [0..16)            header
    /// [16..16+metaSize)  metadata section   (only when there is metadata)
    /// [..+payloadSize)   payload
    /// ```
    ///
    /// and the header's `payloadSize` counts the **payload only** — the
    /// metadata section is accounted for separately by `metadataSize`. This
    /// matches the authentic EA `Util::fetchClientConfig` captures, whose
    /// `payloadSize` equals the number of bytes after the 16-byte header.
    ///
    /// The error code is **not** in the header. On an error reply it travels in
    /// the metadata section as the TDF field `ERRC`, alongside the `CNTX`
    /// context value.
    pub fn encode_16(&self) -> io::Result<Vec<u8>> {
        let payload_bytes = if self.payload.fields.is_empty() {
            Vec::new()
        } else {
            serialize_tdf(&self.payload)?
        };

        // The metadata section is only present when there is something to say
        // in it: an error code, or a non-zero context value.
        let metadata_bytes = if self.header.error_code != 0 || self.header.context != 0 {
            let mut metadata = TdfStruct::new();
            metadata.set("CNTX", blaze_tdf::TdfValue::Integer(self.header.context));
            metadata.set("ERRC", blaze_tdf::TdfValue::Integer(self.header.error_code as u64));
            serialize_tdf(&metadata)?
        } else {
            Vec::new()
        };

        let mut header = self.header;
        header.length = payload_bytes.len() as u32;
        header.metadata_size = metadata_bytes.len() as u16;

        let mut buf = Vec::with_capacity(16 + metadata_bytes.len() + payload_bytes.len());
        buf.extend_from_slice(&header.to_bytes_16());
        buf.extend_from_slice(&metadata_bytes);
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
            }        };

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

    #[test]
    fn test_blaze_packet_16_roundtrip() {
        let empty_pkt = BlazePacket::new(
            0x0009,
            0x0002,
            0x0000,
            MessageType::RecordResponse,
            0,
            3,
            TdfStruct::new(),
        )
        .expect("Empty packet failed");

        let encoded = empty_pkt.encode_16().expect("encode_16 failed");
        assert_eq!(encoded.len(), 16);
        assert_eq!(&encoded[0..4], &[0, 0, 0, 0]); // payloadSize 0
        assert_eq!(&encoded[4..6], &[0x00, 0x00]); // metadataSize 0
        assert_eq!(&encoded[6..8], &[0x00, 0x09]); // comp 9
        assert_eq!(&encoded[8..10], &[0x00, 0x02]); // cmd 2
        assert_eq!(encoded[13], 0x20); // RecordResponse (1) << 5

        let header = BlazeHeader::from_bytes_16((&encoded[0..16]).try_into().unwrap())
            .expect("from_bytes_16 failed");
        assert_eq!(header.component, 0x0009);
        assert_eq!(header.command, 0x0002);
        assert_eq!(header.length, 0);
        assert_eq!(header.metadata_size, 0);
        assert_eq!(header.msg_type, MessageType::RecordResponse);
    }

    #[test]
    fn test_blaze_header_16_byte_packing() {
        // Fire2: byte 13 carries `messageType << 5 | userIndex`.
        let header = BlazeHeader::new_fire2(
            0,
            0x0009,
            0x0002,
            0x0000,
            MessageType::RecordResponse,
            42,
            0,
        );
        let bytes = header.to_bytes_16();
        assert_eq!(bytes.len(), 16);
        assert_eq!(&bytes[4..6], &[0x00, 0x00], "bytes 4..6 are the metadata size");
        assert_eq!(bytes[13], 0x20, "byte 13 must be RecordResponse (1) << 5");
        assert_eq!(&bytes[10..13], &[0x00, 0x00, 42], "bytes 10..13 are the u24 message number");

        let parsed = BlazeHeader::from_bytes_16(&bytes).expect("Failed to parse 16-byte header");
        assert_eq!(parsed.msg_type, MessageType::RecordResponse);
        assert_eq!(parsed.message_number, 42);
        assert_eq!(parsed.user_index, 0);
    }
}
