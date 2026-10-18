//! # QoS / STUN Responder (Port 17498)
//!
//! DirtySDK `ProtoPing` / QoS UDP probe responder that reflects the client's
//! external IPv4 address, port, and server timestamp with NAT type `OPEN` (0).

use std::net::SocketAddr;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::net::UdpSocket;
use bytes::{BufMut, BytesMut};

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

    let nat_type: u16 = 0; // 0 = OPEN

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
        "[QoS:17498] Responded to probe from {} (Reflected {}:{}, ReqId 0x{:08X}, NAT: OPEN)",
        src,
        std::net::Ipv4Addr::from(reflected_ip),
        reflected_port,
        request_id
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
    fn test_handle_qos_probe() {
        let src = SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(192, 168, 1, 105), 54321));
        let mut probe_req = vec![0x12, 0x34, 0x56, 0x78]; // Request ID = 0x12345678
        probe_req.resize(32, 0xAA); // Additional probe padding

        let resp = handle_qos_probe(&probe_req, src);

        assert!(resp.len() >= 20);
        // Request ID echoed
        assert_eq!(&resp[0..4], &[0x12, 0x34, 0x56, 0x78]);
        // Reflected IP
        assert_eq!(&resp[4..8], &[192, 168, 1, 105]);
        // Reflected Port
        assert_eq!(&resp[8..10], &54321u16.to_be_bytes());
        // NAT Type (bytes 14..16) = 0 (OPEN)
        assert_eq!(&resp[14..16], &[0, 0]);
        // Echoed trailing payload
        assert_eq!(&resp[16..], &probe_req[16..]);
    }
}
