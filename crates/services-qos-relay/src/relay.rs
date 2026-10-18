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
}

/// Thread-safe peer address registry mapping (session_id, peer_id) <-> SocketAddr.
#[derive(Default)]
pub struct PeerTable {
    peers: RwLock<HashMap<(u16, u16), (SocketAddr, Instant)>>,
    addr_to_peer: RwLock<HashMap<SocketAddr, (u16, u16)>>,
}

impl PeerTable {
    pub fn new() -> Self {
        Self::default()
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

    // Data forwarding:
    // Sender wants to send data to target peer `header.peer_id`.
    let target_peer_id = header.peer_id;
    let target_addr = table.get_peer_addr(header.session_id, target_peer_id)?;

    // Identify sender peer ID so the recipient knows who transmitted this frame
    let source_peer_id = table
        .get_peer_by_addr(&src)
        .map(|(_, pid)| pid)
        .unwrap_or(0);

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
}
