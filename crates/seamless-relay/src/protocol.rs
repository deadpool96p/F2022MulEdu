//! Binary envelope protocol for Seamless FIFA 22.

pub const PROTOCOL_MAGIC: [u8; 4] = [0x53, 0x46, 0x32, 0x32]; // "SF22"
pub const PROTOCOL_VERSION: u8 = 0x01;
pub const HEADER_LEN: usize = 46;

pub const PEER_ID_HOST: u8 = 0;
pub const PEER_ID_GUEST: u8 = 1;
pub const PEER_ID_UNASSIGNED: u8 = 0xFF;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum MessageType {
    HandshakeInit = 0x01,
    HandshakeAck = 0x02,
    GameData = 0x03,
    HeartbeatPing = 0x04,
    HeartbeatPong = 0x05,
    PeerStatusNotify = 0x06,
    Disconnect = 0x07,
}

impl MessageType {
    pub fn from_u8(val: u8) -> Option<Self> {
        match val {
            0x01 => Some(Self::HandshakeInit),
            0x02 => Some(Self::HandshakeAck),
            0x03 => Some(Self::GameData),
            0x04 => Some(Self::HeartbeatPing),
            0x05 => Some(Self::HeartbeatPong),
            0x06 => Some(Self::PeerStatusNotify),
            0x07 => Some(Self::Disconnect),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct SeamlessPacket<'a> {
    pub version: u8,
    pub msg_type: MessageType,
    pub peer_id: u8,
    pub flags: u8,
    pub room_hash: [u8; 32],
    pub seq_num: u32,
    pub payload: &'a [u8],
}

impl<'a> SeamlessPacket<'a> {
    pub fn parse(buf: &'a [u8]) -> Result<Self, &'static str> {
        if buf.len() < HEADER_LEN {
            return Err("Packet too short for SF22 header");
        }

        if buf[0..4] != PROTOCOL_MAGIC {
            return Err("Invalid magic bytes, expected SF22");
        }

        let version = buf[4];
        if version != PROTOCOL_VERSION {
            return Err("Unsupported protocol version");
        }

        let msg_type = MessageType::from_u8(buf[5]).ok_or("Invalid message opcode")?;
        let peer_id = buf[6];
        let flags = buf[7];

        let mut room_hash = [0u8; 32];
        room_hash.copy_from_slice(&buf[8..40]);

        let seq_num = u32::from_be_bytes([buf[40], buf[41], buf[42], buf[43]]);
        let payload_len = u16::from_be_bytes([buf[44], buf[45]]) as usize;

        if buf.len() < HEADER_LEN + payload_len {
            return Err("Packet payload length mismatch");
        }

        let payload = &buf[HEADER_LEN..HEADER_LEN + payload_len];

        Ok(Self {
            version,
            msg_type,
            peer_id,
            flags,
            room_hash,
            seq_num,
            payload,
        })
    }

    pub fn serialize(
        msg_type: MessageType,
        peer_id: u8,
        flags: u8,
        room_hash: &[u8; 32],
        seq_num: u32,
        payload: &[u8],
        out: &mut Vec<u8>,
    ) {
        out.clear();
        out.reserve(HEADER_LEN + payload.len());

        out.extend_from_slice(&PROTOCOL_MAGIC);
        out.push(PROTOCOL_VERSION);
        out.push(msg_type as u8);
        out.push(peer_id);
        out.push(flags);
        out.extend_from_slice(room_hash);
        out.extend_from_slice(&seq_num.to_be_bytes());
        out.extend_from_slice(&(payload.len() as u16).to_be_bytes());
        out.extend_from_slice(payload);
    }
}

pub fn hash_password(password: &str) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"FIFA22_SEAMLESS_SALT:");
    hasher.update(password.as_bytes());
    *hasher.finalize().as_bytes()
}
