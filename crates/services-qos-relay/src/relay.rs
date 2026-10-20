//! # ProtoTunnel UDP Relay Engine (Port 3659)
//!
//! Sub-millisecond UDP packet forwarding for FIFA 22 players restricted by
//! Carrier-Grade NAT (CGNAT) or Symmetric NAT using the 8-byte multiplex header.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};
use bytes::{Buf, BufMut, BytesMut};
use parking_lot::RwLock;
use tokio::net::UdpSocket;

pub const FLAG_DATA: u16 = 0x0000;
pub const FLAG_REGISTER: u16 = 0x0001;
pub const FLAG_HEARTBEAT: u16 = 0x0002;
pub const FLAG_ACK: u16 = 0x0004;

pub const PROTOTUNNEL_HEADER_LEN: usize = 8;
pub const INPUT_FRAME_LEN: usize = 22;

/// 8-byte ProtoTunnel Multiplex Header
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProtoTunnelHeader {
    pub session_id: u16,
    pub peer_id: u16,
    pub payload_len: u16,
    pub flags: u16,
}

impl ProtoTunnelHeader {
    pub fn parse(buf: &[u8]) -> Option<Self> {
        if buf.len() < PROTOTUNNEL_HEADER_LEN {
            return None;
        }
        let mut cur = &buf[..PROTOTUNNEL_HEADER_LEN];
        let session_id = cur.get_u16();
        let peer_id = cur.get_u16();
        let payload_len = cur.get_u16();
        let flags = cur.get_u16();
        Some(Self {
            session_id,
            peer_id,
            payload_len,
            flags,
        })
    }

    pub fn write_to(&self, out: &mut BytesMut) {
        out.put_u16(self.session_id);
        out.put_u16(self.peer_id);
        out.put_u16(self.payload_len);
        out.put_u16(self.flags);
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bm = BytesMut::with_capacity(PROTOTUNNEL_HEADER_LEN);
        self.write_to(&mut bm);
        bm.to_vec()
    }
}

/// NetGameDist 22-byte Input Frame containing 4-byte packetid, 2-byte length, and 16-byte XINPUT_GAMEPAD
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InputFrame {
    pub packet_id: u32,
    pub length: u16,
    pub gamepad: [u8; 16],
}

impl InputFrame {
    pub fn parse(buf: &[u8]) -> Option<Self> {
        if buf.len() < INPUT_FRAME_LEN {
            return None;
        }
        let mut cur = &buf[..INPUT_FRAME_LEN];
        let packet_id = cur.get_u32();
        let length = cur.get_u16();
        let mut gamepad = [0u8; 16];
        gamepad.copy_from_slice(&buf[6..22]);
        Some(Self {
            packet_id,
            length,
            gamepad,
        })
    }

    pub fn write_to(&self, out: &mut BytesMut) {
        out.put_u32(self.packet_id);
        out.put_u16(self.length);
        out.put_slice(&self.gamepad);
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bm = BytesMut::with_capacity(INPUT_FRAME_LEN);
        self.write_to(&mut bm);
        bm.to_vec()
    }
}

/// ARC4 stream cipher state for ProtoTunnel encryption/decryption.
#[derive(Clone)]
pub struct Arc4State {
    s: [u8; 256],
    i: u8,
    j: u8,
}

impl Arc4State {
    pub fn new(key: &[u8]) -> Self {
        let mut s = [0u8; 256];
        for k in 0..=255 {
            s[k] = k as u8;
        }
        let mut j: u8 = 0;
        let default_key = [0x5A, 0xA5, 0x12, 0x34];
        let k_bytes = if key.is_empty() { &default_key[..] } else { key };

        for i in 0..=255 {
            j = j.wrapping_add(s[i]).wrapping_add(k_bytes[i % k_bytes.len()]);
            s.swap(i, j as usize);
        }
        Self { s, i: 0, j: 0 }
    }

    #[inline(always)]
    pub fn apply_keystream(&mut self, data: &mut [u8]) {
        for byte in data.iter_mut() {
            self.i = self.i.wrapping_add(1);
            self.j = self.j.wrapping_add(self.s[self.i as usize]);
            self.s.swap(self.i as usize, self.j as usize);
            let t = self.s[self.i as usize].wrapping_add(self.s[self.j as usize]);
            let k = self.s[t as usize];
            *byte ^= k;
        }
    }
}

/// A long-lived session cipher keyed on session_id, created on the 0x0017 notification
/// and destroyed on 0x000A, enforcing stream continuity (no per-packet re-keying).
pub struct SessionCipher {
    pub session_id: u16,
    pub peer_id: u16,
    state: parking_lot::Mutex<Arc4State>,
    pub reuse_count: std::sync::atomic::AtomicU64,
}

impl SessionCipher {
    pub fn new(session_id: u16, peer_id: u16, key: &[u8]) -> Self {
        Self {
            session_id,
            peer_id,
            state: parking_lot::Mutex::new(Arc4State::new(key)),
            reuse_count: std::sync::atomic::AtomicU64::new(0),
        }
    }

    #[inline(always)]
    pub fn process_datagram(&self, data: &mut [u8]) {
        self.reuse_count.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let mut lock = self.state.lock();
        lock.apply_keystream(data);
    }

    pub fn get_reuse_count(&self) -> u64 {
        self.reuse_count.load(std::sync::atomic::Ordering::Relaxed)
    }
}

/// Thread-safe peer address registry mapping (session_id, peer_id) <-> SocketAddr.
#[derive(Default)]
pub struct PeerTable {
    peers: RwLock<HashMap<(u16, u16), (SocketAddr, Instant)>>,
    addr_to_peer: RwLock<HashMap<SocketAddr, (u16, u16)>>,
    ciphers: RwLock<HashMap<u16, Arc<SessionCipher>>>,
    pub cipher_reuse_total: std::sync::atomic::AtomicU64,
}

impl PeerTable {
    pub fn new() -> Self {
        Self {
            peers: RwLock::new(HashMap::new()),
            addr_to_peer: RwLock::new(HashMap::new()),
            ciphers: RwLock::new(HashMap::new()),
            cipher_reuse_total: std::sync::atomic::AtomicU64::new(0),
        }
    }

    /// Gets or creates a SessionCipher once per match keyed on session_id.
    pub fn get_or_create_cipher(&self, session_id: u16, key: &[u8]) -> Arc<SessionCipher> {
        {
            let c = self.ciphers.read();
            if let Some(sc) = c.get(&session_id) {
                return sc.clone();
            }
        }
        let mut c = self.ciphers.write();
        c.entry(session_id)
            .or_insert_with(|| Arc::new(SessionCipher::new(session_id, 0, key)))
            .clone()
    }

    /// Gets existing cipher for session_id.
    pub fn get_cipher(&self, session_id: u16) -> Option<Arc<SessionCipher>> {
        self.ciphers.read().get(&session_id).cloned()
    }

    /// Explicitly initializes SessionCipher on 0x0017 (NotifyJoiningPlayerInitiateConnections).
    pub fn on_notify_initiate_connections(&self, session_id: u16) -> Arc<SessionCipher> {
        let default_key = [0x46, 0x49, 0x46, 0x41];
        self.get_or_create_cipher(session_id, &default_key)
    }

    /// Explicitly destroys SessionCipher on 0x000A (NotifyGameRemoved).
    pub fn destroy_session_cipher(&self, session_id: u16) -> Option<Arc<SessionCipher>> {
        let mut c = self.ciphers.write();
        let removed = c.remove(&session_id);
        if removed.is_some() {
            tracing::info!(
                "[ProtoTunnel:3659] Destroyed SessionCipher for session {} (0x000A NotifyGameRemoved)",
                session_id
            );
        }
        removed
    }

    pub fn get_total_cipher_reuses(&self) -> u64 {
        self.cipher_reuse_total.load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Registers or updates a peer's network endpoint.
    pub fn register(&self, session_id: u16, peer_id: u16, addr: SocketAddr) {
        let now = Instant::now();
        self.peers.write().insert((session_id, peer_id), (addr, now));
        self.addr_to_peer.write().insert(addr, (session_id, peer_id));
        tracing::info!(
            "[ProtoTunnel:3659] Registered peer {} in session {} at {}",
            peer_id,
            session_id,
            addr
        );
    }

    /// Looks up a peer's destination SocketAddr.
    pub fn get_peer_addr(&self, session_id: u16, peer_id: u16) -> Option<SocketAddr> {
        self.peers.read().get(&(session_id, peer_id)).map(|(addr, _)| *addr)
    }

    /// Looks up which (session_id, peer_id) sent a packet from the given SocketAddr.
    pub fn get_peer_by_addr(&self, addr: &SocketAddr) -> Option<(u16, u16)> {
        self.addr_to_peer.read().get(addr).copied()
    }

    /// Prunes peers inactive for longer than `timeout`.
    pub fn prune_stale(&self, timeout: Duration) {
        let now = Instant::now();
        let mut peers = self.peers.write();
        let mut addr_map = self.addr_to_peer.write();

        peers.retain(|&(sid, pid), (addr, last_seen)| {
            if now.duration_since(*last_seen) > timeout {
                tracing::info!(
                    "[ProtoTunnel:3659] Pruning timed-out peer {} in session {}",
                    pid,
                    sid
                );
                addr_map.remove(addr);
                false
            } else {
                true
            }
        });
    }
}

/// Handles an incoming UDP packet, returning an optional direct response or forwarding tuple (target_addr, packet_bytes).
pub fn handle_relay_packet(
    buf: &[u8],
    src: SocketAddr,
    table: &PeerTable,
) -> Option<(SocketAddr, Vec<u8>)> {
    let header = ProtoTunnelHeader::parse(buf)?;
    let payload = if buf.len() > PROTOTUNNEL_HEADER_LEN {
        &buf[PROTOTUNNEL_HEADER_LEN..]
    } else {
        &[]
    };

    // Registration packet
    if header.flags & FLAG_REGISTER != 0 {
        table.register(header.session_id, header.peer_id, src);

        // Echo registration ACK back to sender
        let ack_header = ProtoTunnelHeader {
            session_id: header.session_id,
            peer_id: header.peer_id,
            payload_len: 0,
            flags: FLAG_ACK | FLAG_REGISTER,
        };
        let mut ack = BytesMut::with_capacity(PROTOTUNNEL_HEADER_LEN);
        ack_header.write_to(&mut ack);
        return Some((src, ack.to_vec()));
    }

    // Heartbeat packet
    if header.flags & FLAG_HEARTBEAT != 0 {
        if let Some((sid, pid)) = table.get_peer_by_addr(&src) {
            table.register(sid, pid, src);
        }
        return None;
    }

    // Identify sender peer ID so the recipient knows who transmitted this frame
    let source_peer_id = table
        .get_peer_by_addr(&src)
        .map(|(_, pid)| pid)
        .unwrap_or(0);

    // Reuse long-lived session cipher created once per match keyed on session_id (0x0017)
    let default_key = [0x46, 0x49, 0x46, 0x41];
    let cipher = table.get_or_create_cipher(header.session_id, &default_key);
    cipher.reuse_count.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    table.cipher_reuse_total.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    services_http_content::NetcodeHttpMetrics::global().increment_cipher_reuse();

    // If target peer_id is 0 (broadcast), route to opposite player in session
    let target_peer_id = if header.peer_id == 0 {
        if source_peer_id == 1 { 2 } else { 1 }
    } else {
        header.peer_id
    };
    let target_addr = table.get_peer_addr(header.session_id, target_peer_id)?;

    // Build forwarded packet with sender peer ID in the header
    let forward_header = ProtoTunnelHeader {
        session_id: header.session_id,
        peer_id: source_peer_id,
        payload_len: payload.len() as u16,
        flags: header.flags,
    };

    let mut out = BytesMut::with_capacity(PROTOTUNNEL_HEADER_LEN + payload.len());
    forward_header.write_to(&mut out);
    out.put_slice(payload);

    Some((target_addr, out.to_vec()))
}

/// Runs the asynchronous ProtoTunnel UDP relay listener loop.
pub async fn run_relay_server(
    socket: UdpSocket,
    table: Arc<PeerTable>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let local_addr = socket.local_addr()?;
    tracing::info!("[ProtoTunnel:3659] Relay engine listening on UDP {}", local_addr);

    let mut buf = vec![0u8; 4096];
    loop {
        let (len, src) = match socket.recv_from(&mut buf).await {
            Ok(res) => res,
            Err(e) => {
                tracing::warn!("[ProtoTunnel:3659] Recv error: {}", e);
                continue;
            }
        };

        if len == 0 {
            continue;
        }

        if let Some((target, packet)) = handle_relay_packet(&buf[..len], src, &table) {
            if let Err(e) = socket.send_to(&packet, target).await {
                tracing::warn!(
                    "[ProtoTunnel:3659] Failed to forward packet to {}: {}",
                    target,
                    e
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{Ipv4Addr, SocketAddrV4};

    #[test]
    fn test_prototunnel_header_roundtrip() {
        let header = ProtoTunnelHeader {
            session_id: 1001,
            peer_id: 2,
            payload_len: 64,
            flags: FLAG_DATA,
        };
        let mut buf = BytesMut::new();
        header.write_to(&mut buf);
        assert_eq!(buf.len(), 8);

        let parsed = ProtoTunnelHeader::parse(&buf).expect("Failed to parse header");
        assert_eq!(header, parsed);
    }

    #[test]
    fn test_relay_registration_and_packet_forwarding() {
        let table = PeerTable::new();
        let peer1_addr = SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(127, 0, 0, 1), 10001));
        let peer2_addr = SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(127, 0, 0, 1), 10002));

        let session_id = 42;
        let peer1_id = 1;
        let peer2_id = 2;

        // Peer 1 registers
        let mut reg1 = BytesMut::new();
        ProtoTunnelHeader {
            session_id,
            peer_id: peer1_id,
            payload_len: 0,
            flags: FLAG_REGISTER,
        }
        .write_to(&mut reg1);

        let resp1 = handle_relay_packet(&reg1, peer1_addr, &table).expect("Expected ACK response");
        assert_eq!(resp1.0, peer1_addr);
        let ack1_hdr = ProtoTunnelHeader::parse(&resp1.1).unwrap();
        assert_eq!(ack1_hdr.flags, FLAG_ACK | FLAG_REGISTER);

        // Peer 2 registers
        let mut reg2 = BytesMut::new();
        ProtoTunnelHeader {
            session_id,
            peer_id: peer2_id,
            payload_len: 0,
            flags: FLAG_REGISTER,
        }
        .write_to(&mut reg2);
        let _ = handle_relay_packet(&reg2, peer2_addr, &table);

        // Peer 1 sends input data to Peer 2
        let input_payload = b"NETGAMEDIST_INPUT_FRAME_TICK_60";
        let mut data_pkt = BytesMut::new();
        ProtoTunnelHeader {
            session_id,
            peer_id: peer2_id, // Target is Peer 2
            payload_len: input_payload.len() as u16,
            flags: FLAG_DATA,
        }
        .write_to(&mut data_pkt);
        data_pkt.put_slice(input_payload);

        let fwd = handle_relay_packet(&data_pkt, peer1_addr, &table).expect("Expected packet forwarded");
        // Forwarded to Peer 2's address!
        assert_eq!(fwd.0, peer2_addr);

        let fwd_hdr = ProtoTunnelHeader::parse(&fwd.1).unwrap();
        // The peer ID in the forwarded packet is rewritten to Peer 1 (the sender)
        assert_eq!(fwd_hdr.peer_id, peer1_id);
        assert_eq!(&fwd.1[8..], input_payload);
    }

    #[test]
    fn test_prototunnel_header_byte_level_layout() {
        let session_id: u16 = 0x1234;
        let peer_id: u16 = 0x5678;
        let payload_len: u16 = 0x0016; // 22 bytes
        let proto_flags: u16 = 0x0001; // Encrypted

        let header = ProtoTunnelHeader {
            session_id,
            peer_id,
            payload_len,
            flags: proto_flags,
        };

        let raw = header.to_bytes();
        assert_eq!(raw.len(), 8, "ProtoTunnel header must be exactly 8 bytes");

        // Byte-level offset assertions (Big Endian)
        assert_eq!(&raw[0..2], &0x1234u16.to_be_bytes(), "session_id at offset 0..2");
        assert_eq!(&raw[2..4], &0x5678u16.to_be_bytes(), "peer_id at offset 2..4");
        assert_eq!(&raw[4..6], &0x0016u16.to_be_bytes(), "payload_len at offset 4..6");
        assert_eq!(&raw[6..8], &0x0001u16.to_be_bytes(), "proto_flags at offset 6..8");

        let parsed = ProtoTunnelHeader::parse(&raw).expect("Must parse valid header bytes");
        assert_eq!(parsed.session_id, session_id);
        assert_eq!(parsed.peer_id, peer_id);
        assert_eq!(parsed.payload_len, payload_len);
        assert_eq!(parsed.flags, proto_flags);
    }

    #[test]
    fn test_input_frame_byte_level_layout() {
        let packet_id: u32 = 0x0000003C; // Frame 60
        let length: u16 = 16;
        let gamepad: [u8; 16] = [
            0x10, 0x00, // wButtons (e.g. START)
            0xFF,       // bLeftTrigger
            0x00,       // bRightTrigger
            0x00, 0x20, // sThumbLX
            0x00, 0x00, // sThumbLY
            0x00, 0x00, // sThumbRX
            0x00, 0x00, // sThumbRY
            0x00, 0x00, 0x00, 0x00, // padding/reserved
        ];

        let frame = InputFrame {
            packet_id,
            length,
            gamepad,
        };

        let raw = frame.to_bytes();
        assert_eq!(raw.len(), 22, "InputFrame must be exactly 22 bytes (4 + 2 + 16)");

        // Byte-level offset assertions
        assert_eq!(&raw[0..4], &0x0000003Cu32.to_be_bytes(), "packet_id at offset 0..4");
        assert_eq!(&raw[4..6], &16u16.to_be_bytes(), "length at offset 4..6");
        assert_eq!(&raw[6..22], &gamepad[..], "16-byte XINPUT_GAMEPAD at offset 6..22");

        let parsed = InputFrame::parse(&raw).expect("Must parse valid input frame");
        assert_eq!(parsed.packet_id, packet_id);
        assert_eq!(parsed.length, length);
        assert_eq!(parsed.gamepad, gamepad);
    }

    #[test]
    fn test_session_cipher_1000_frames_keystream_continuity() {
        let session_id: u16 = 77;
        let key = b"F2022_STREAM_CIPHER_CONTINUITY_KEY";
        let cipher = SessionCipher::new(session_id, 1, key);

        // Reference: single continuous ARC4 state encrypting 1000 * 22 bytes
        let mut reference_state = Arc4State::new(key);

        let mut consecutive_frames_output = Vec::with_capacity(1000 * INPUT_FRAME_LEN);
        let mut reference_stream_output = Vec::with_capacity(1000 * INPUT_FRAME_LEN);

        for tick in 1..=1000u32 {
            let mut gamepad = [0u8; 16];
            gamepad[0] = (tick & 0xFF) as u8;
            gamepad[1] = ((tick >> 8) & 0xFF) as u8;

            let frame = InputFrame {
                packet_id: tick,
                length: 16,
                gamepad,
            };
            let mut frame_bytes = frame.to_bytes();
            let mut ref_bytes = frame_bytes.clone();

            // 1. Process via long-lived SessionCipher (1 frame per datagram)
            cipher.process_datagram(&mut frame_bytes);
            consecutive_frames_output.extend_from_slice(&frame_bytes);

            // 2. Process via reference continuous stream
            reference_state.apply_keystream(&mut ref_bytes);
            reference_stream_output.extend_from_slice(&ref_bytes);
        }

        // Keystream continuity assertion: 1000 sequential frames encrypted datagram-by-datagram
        // MUST EXACTLY equal a single continuous 22,000-byte stream!
        assert_eq!(
            consecutive_frames_output, reference_stream_output,
            "SessionCipher must maintain keystream continuity across 1000 consecutive datagrams without re-keying"
        );
        assert_eq!(cipher.get_reuse_count(), 1000);

        // Negative assertion: verify that re-keying per frame would fail stream continuity
        let mut rekey_output = Vec::with_capacity(1000 * INPUT_FRAME_LEN);
        for tick in 1..=1000u32 {
            let mut gamepad = [0u8; 16];
            gamepad[0] = (tick & 0xFF) as u8;
            gamepad[1] = ((tick >> 8) & 0xFF) as u8;

            let frame = InputFrame {
                packet_id: tick,
                length: 16,
                gamepad,
            };
            let mut frame_bytes = frame.to_bytes();
            // Simulating legacy bug: re-keying per packet
            let mut bad_cipher = Arc4State::new(key);
            bad_cipher.apply_keystream(&mut frame_bytes);
            rekey_output.extend_from_slice(&frame_bytes);
        }
        assert_ne!(
            consecutive_frames_output, rekey_output,
            "Re-keying per packet must not match continuous keystream"
        );
    }

    #[test]
    fn test_session_cipher_lifecycle_0x0017_and_0x000a() {
        let table = PeerTable::new();
        let session_id: u16 = 505;

        assert!(table.get_cipher(session_id).is_none());

        // 1. Created on 0x0017 (NotifyJoiningPlayerInitiateConnections)
        let cipher1 = table.on_notify_initiate_connections(session_id);
        assert_eq!(cipher1.session_id, session_id);
        assert!(table.get_cipher(session_id).is_some());

        // 2. Subsequent lookups return the EXACT same Arc<SessionCipher> instance
        let cipher2 = table.get_or_create_cipher(session_id, b"dummy_key");
        assert!(Arc::ptr_eq(&cipher1, &cipher2), "Must share identical session cipher instance");

        // 3. Destroyed on 0x000A (NotifyGameRemoved)
        let destroyed = table.destroy_session_cipher(session_id);
        assert!(destroyed.is_some());
        assert!(table.get_cipher(session_id).is_none());
    }
}
