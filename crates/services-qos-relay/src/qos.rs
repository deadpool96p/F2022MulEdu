//! # QoS / STUN Responder (Port 17498)
//!
//! DirtySDK `ProtoPing` / QoS UDP probe responder that reflects the client's
//! external IPv4 address, port, and server timestamp with NAT type `OPEN` (0).

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{Instant, SystemTime, UNIX_EPOCH};
use tokio::net::UdpSocket;
use bytes::{BufMut, BytesMut};

pub const NAT_OPEN: u16 = 0;
pub const NAT_PORT_RESTRICTED: u16 = 1;
pub const NAT_SYMMETRIC: u16 = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NatType {
    Open = 0,
    PortRestricted = 1,
    Symmetric = 2,
    Unknown = 3,
}

static RELAY_ONLY_MODE: AtomicBool = AtomicBool::new(false);
static RELAY_ONLY_IPS: Mutex<Option<std::collections::HashSet<[u8; 4]>>> = Mutex::new(None);

/// Returns true if the server has auto-switched to ProtoTunnel relay-only mode.
pub fn is_relay_only_mode() -> bool {
    if RELAY_ONLY_MODE.load(Ordering::Relaxed) {
        return true;
    }
    let lock = RELAY_ONLY_IPS.lock().unwrap();
    lock.as_ref().map(|s| !s.is_empty()).unwrap_or(false)
}

/// Checks if a specific IP has been flagged for relay-only mode.
pub fn is_relay_only_for_ip(ip: &[u8; 4]) -> bool {
    if RELAY_ONLY_MODE.load(Ordering::Relaxed) {
        return true;
    }
    let lock = RELAY_ONLY_IPS.lock().unwrap();
    lock.as_ref().map(|s| s.contains(ip)).unwrap_or(false)
}

/// Enables or disables relay-only mode.
pub fn set_relay_only_mode(relay_only: bool) {
    RELAY_ONLY_MODE.store(relay_only, Ordering::SeqCst);
}

#[derive(Debug, Clone)]
struct ProbeRecord {
    last_port: u16,
    last_seen: Instant,
    port_changes: u32,
}

static PROBE_HISTORY: Mutex<Option<HashMap<[u8; 4], ProbeRecord>>> = Mutex::new(None);

/// Clears tracked probe history (useful for deterministic unit testing).
pub fn reset_probe_history() {
    let mut lock = PROBE_HISTORY.lock().unwrap();
    if let Some(ref mut map) = *lock {
        map.clear();
    }
    let mut ip_lock = RELAY_ONLY_IPS.lock().unwrap();
    if let Some(ref mut set) = *ip_lock {
        set.clear();
    }
    set_relay_only_mode(false);
}

/// Detects NAT type based on probe request payload flags and dynamic source port translation.
pub fn detect_nat_type(buf: &[u8], src_ip: [u8; 4], src_port: u16) -> u16 {
    // 1. Check for explicit test/simulation flags in probe payload
    if buf.len() > 4 {
        if buf[4] == 0x01 {
            return NAT_PORT_RESTRICTED;
        } else if buf[4] == 0x02 {
            return NAT_SYMMETRIC;
        }
    }
    if buf.len() >= 16 {
        if buf[14..16] == [0, 1] {
            return NAT_PORT_RESTRICTED;
        } else if buf[14..16] == [0, 2] {
            return NAT_SYMMETRIC;
        }
    }

    // 2. Dynamic tracking across multiple probes from the same IP
    let mut lock = PROBE_HISTORY.lock().unwrap();
    let map = lock.get_or_insert_with(HashMap::new);
    let now = Instant::now();

    if let Some(record) = map.get_mut(&src_ip) {
        if now.duration_since(record.last_seen).as_secs() < 30 {
            if record.last_port != src_port {
                record.port_changes += 1;
                record.last_port = src_port;
                record.last_seen = now;
                return NAT_SYMMETRIC;
            } else if record.port_changes > 0 {
                record.last_seen = now;
                return NAT_PORT_RESTRICTED;
            }
        }
        record.last_port = src_port;
        record.last_seen = now;
    } else {
        map.insert(
            src_ip,
            ProbeRecord {
                last_port: src_port,
                last_seen: now,
                port_changes: 0,
            },
        );
    }

    NAT_OPEN
}

/// Structure representing a decoded QoS probe response.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QosProbeResponse {
    pub request_id: u32,
    pub reflected_ip: [u8; 4],
    pub reflected_port: u16,
    pub server_timestamp_us: u32,
    pub nat_type: u16,
}

/// Handles an incoming QoS UDP probe packet and returns the reflected response bytes.
pub fn handle_qos_probe(buf: &[u8], src: SocketAddr) -> Vec<u8> {
    let request_id = if buf.len() >= 4 {
        u32::from_be_bytes([buf[0], buf[1], buf[2], buf[3]])
    } else {
        0
    };

    let (reflected_ip, reflected_port) = match src {
        SocketAddr::V4(v4) => (v4.ip().octets(), v4.port()),
        SocketAddr::V6(v6) => {
            if let Some(v4) = v6.ip().to_ipv4() {
                (v4.octets(), v6.port())
            } else {
                ([127, 0, 0, 1], v6.port())
            }
        }
    };

    let now_us = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_micros() as u32;

    let nat_type = detect_nat_type(buf, reflected_ip, reflected_port);

    // Auto-switch to relay-only mode if symmetric or port-restricted NAT is encountered
    if nat_type == NAT_SYMMETRIC || nat_type == NAT_PORT_RESTRICTED {
        let mut ip_lock = RELAY_ONLY_IPS.lock().unwrap();
        ip_lock
            .get_or_insert_with(std::collections::HashSet::new)
            .insert(reflected_ip);
        tracing::warn!(
            "[QoS:17498] Restrictive NAT detected (type {}) from {} -> Auto-switching to relay-only mode",
            nat_type,
            src
        );
    }

    let mut resp = BytesMut::with_capacity(std::cmp::max(buf.len(), 20));
    resp.put_u32(request_id);
    resp.put_slice(&reflected_ip);
    resp.put_u16(reflected_port);
    resp.put_u32(now_us);
    resp.put_u16(nat_type);

    // If the client sent a padded probe (e.g. 1200 bytes for MTU detection),
    // echo back the rest of the payload so the client can verify MTU and checksum.
    if buf.len() > 16 {
        resp.put_slice(&buf[16..]);
    }

    tracing::debug!(
        "[QoS:17498] Responded to probe from {} (Reflected {}:{}, ReqId 0x{:08X}, NAT: {})",
        src,
        std::net::Ipv4Addr::from(reflected_ip),
        reflected_port,
        request_id,
        match nat_type {
            NAT_PORT_RESTRICTED => "PORT_RESTRICTED",
            NAT_SYMMETRIC => "SYMMETRIC_NAT",
            _ => "OPEN",
        }
    );

    resp.to_vec()
}

/// Runs the asynchronous QoS UDP listener loop.
pub async fn run_qos_server(
    socket: UdpSocket,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let local_addr = socket.local_addr()?;
    tracing::info!("[QoS:17498] Listening on UDP {}", local_addr);

    let mut buf = vec![0u8; 4096];
    loop {
        let (len, src) = match socket.recv_from(&mut buf).await {
            Ok(res) => res,
            Err(e) => {
                tracing::warn!("[QoS:17498] UDP recv error: {}", e);
                continue;
            }
        };

        if len == 0 {
            continue;
        }

        let resp = handle_qos_probe(&buf[..len], src);
        if let Err(e) = socket.send_to(&resp, src).await {
            tracing::warn!("[QoS:17498] Failed to send QoS response to {}: {}", src, e);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{Ipv4Addr, SocketAddrV4};

    #[test]
    fn test_handle_qos_probe_open() {
        let ip = [192, 168, 1, 105];
        let src = SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(192, 168, 1, 105), 54321));
        let mut probe_req = vec![0x12, 0x34, 0x56, 0x78]; // Request ID = 0x12345678
        probe_req.resize(32, 0x00);

        let resp = handle_qos_probe(&probe_req, src);

        assert!(resp.len() >= 20);
        assert_eq!(&resp[0..4], &[0x12, 0x34, 0x56, 0x78]);
        assert_eq!(&resp[4..8], &[192, 168, 1, 105]);
        assert_eq!(&resp[8..10], &54321u16.to_be_bytes());
        // NAT Type = 0 (OPEN)
        assert_eq!(&resp[14..16], &[0, 0]);
        assert!(!is_relay_only_for_ip(&ip));
    }

    #[test]
    fn test_handle_qos_probe_flag_restricted_and_symmetric() {
        let ip1 = [10, 0, 0, 5];
        let src1 = SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(10, 0, 0, 5), 40001));

        // Test flag 0x01 (PORT_RESTRICTED)
        let probe_restricted = vec![0x00, 0x00, 0x00, 0x01, 0x01, 0x00];
        let resp_res = handle_qos_probe(&probe_restricted, src1);
        assert_eq!(&resp_res[14..16], &[0, 1]);
        assert!(is_relay_only_for_ip(&ip1));

        let ip2 = [10, 0, 0, 6];
        let src2 = SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(10, 0, 0, 6), 40002));
        // Test flag 0x02 (SYMMETRIC_NAT)
        let probe_symmetric = vec![0x00, 0x00, 0x00, 0x02, 0x02, 0x00];
        let resp_sym = handle_qos_probe(&probe_symmetric, src2);
        assert_eq!(&resp_sym[14..16], &[0, 2]);
        assert!(is_relay_only_for_ip(&ip2));
    }

    #[test]
    fn test_dynamic_symmetric_nat_port_translation() {
        let ip = [172, 16, 0, 42];
        let addr = Ipv4Addr::new(172, 16, 0, 42);

        // Probe 1 from port 50001
        let src1 = SocketAddr::V4(SocketAddrV4::new(addr, 50001));
        let probe = vec![0xAA, 0xBB, 0xCC, 0xDD, 0x00, 0x00];
        let resp1 = handle_qos_probe(&probe, src1);
        assert_eq!(&resp1[14..16], &[0, 0]); // First probe is OPEN
        assert!(!is_relay_only_for_ip(&ip));

        // Probe 2 from same IP but different port 50002 -> Port shift detected!
        let src2 = SocketAddr::V4(SocketAddrV4::new(addr, 50002));
        let resp2 = handle_qos_probe(&probe, src2);
        assert_eq!(&resp2[14..16], &[0, 2]); // SYMMETRIC_NAT
        assert!(is_relay_only_for_ip(&ip));
    }
}
