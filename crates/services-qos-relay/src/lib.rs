//! # services-qos-relay
//!
//! DirtySDK `ProtoPing` / QoS responder (port 17498) and ProtoTunnel UDP Relay (port 3659).

pub mod qos;
pub mod relay;

pub use qos::{handle_qos_probe, run_qos_server, QosProbeResponse};
pub use relay::{
    handle_relay_packet, run_relay_server, PeerTable, ProtoTunnelHeader, FLAG_ACK, FLAG_DATA,
    FLAG_HEARTBEAT, FLAG_REGISTER, PROTOTUNNEL_HEADER_LEN,
};

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use tokio::net::UdpSocket;

    #[tokio::test]
    async fn test_async_qos_udp_loopback() {
        // Bind server on random ephemeral port
        let server_socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let server_addr = server_socket.local_addr().unwrap();

        // Spawn QoS server task
        let server_handle = tokio::spawn(async move {
            let mut buf = vec![0u8; 1024];
            let (len, src) = server_socket.recv_from(&mut buf).await.unwrap();
            let resp = handle_qos_probe(&buf[..len], src);
            server_socket.send_to(&resp, src).await.unwrap();
        });

        // Client socket
        let client_socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let probe_req = vec![0xDE, 0xAD, 0xBE, 0xEF, 0x01, 0x02, 0x03, 0x04];
        client_socket.send_to(&probe_req, server_addr).await.unwrap();

        let mut recv_buf = vec![0u8; 1024];
        let (recv_len, from) = client_socket.recv_from(&mut recv_buf).await.unwrap();

        assert_eq!(from, server_addr);
        assert!(recv_len >= 16);
        assert_eq!(&recv_buf[0..4], &[0xDE, 0xAD, 0xBE, 0xEF]); // Request ID echoed
        assert_eq!(&recv_buf[14..16], &[0, 0]); // NAT OPEN

        server_handle.await.unwrap();
    }

    #[tokio::test]
    async fn test_async_relay_udp_loopback() {
        let server_socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let server_addr = server_socket.local_addr().unwrap();
        let peer_table = Arc::new(PeerTable::new());

        let table_clone = peer_table.clone();
        let server_handle = tokio::spawn(async move {
            let mut buf = vec![0u8; 1024];
            // Process 3 packets: Peer 1 register, Peer 2 register, Peer 1 -> Peer 2 data
            for _ in 0..3 {
                let (len, src) = server_socket.recv_from(&mut buf).await.unwrap();
                if let Some((target, packet)) = handle_relay_packet(&buf[..len], src, &table_clone) {
                    server_socket.send_to(&packet, target).await.unwrap();
                }
            }
        });

        let peer1_sock = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let peer2_sock = UdpSocket::bind("127.0.0.1:0").await.unwrap();

        let session_id = 99;
        let peer1_id = 1;
        let peer2_id = 2;

        // 1. Peer 1 registers
        let mut reg1 = bytes::BytesMut::new();
        ProtoTunnelHeader {
            session_id,
            peer_id: peer1_id,
            payload_len: 0,
            flags: FLAG_REGISTER,
        }
        .write_to(&mut reg1);
        peer1_sock.send_to(&reg1, server_addr).await.unwrap();

        let mut ack1 = vec![0u8; 32];
        let (ack_len, _) = peer1_sock.recv_from(&mut ack1).await.unwrap();
        assert_eq!(ack_len, 8);
        let ack1_hdr = ProtoTunnelHeader::parse(&ack1[..ack_len]).unwrap();
        assert_eq!(ack1_hdr.flags, FLAG_ACK | FLAG_REGISTER);

        // 2. Peer 2 registers
        let mut reg2 = bytes::BytesMut::new();
        ProtoTunnelHeader {
            session_id,
            peer_id: peer2_id,
            payload_len: 0,
            flags: FLAG_REGISTER,
        }
        .write_to(&mut reg2);
        peer2_sock.send_to(&reg2, server_addr).await.unwrap();

        let mut ack2 = vec![0u8; 32];
        let (ack_len, _) = peer2_sock.recv_from(&mut ack2).await.unwrap();
        assert_eq!(ack_len, 8);

        // 3. Peer 1 sends frame to Peer 2
        let frame_payload = b"ROLLBACK_INPUT_TICK_120";
        let mut data_pkt = bytes::BytesMut::new();
        ProtoTunnelHeader {
            session_id,
            peer_id: peer2_id,
            payload_len: frame_payload.len() as u16,
            flags: FLAG_DATA,
        }
        .write_to(&mut data_pkt);
        data_pkt.extend_from_slice(frame_payload);

        peer1_sock.send_to(&data_pkt, server_addr).await.unwrap();

        // Peer 2 receives forwarded frame!
        let mut peer2_recv = vec![0u8; 1024];
        let (p2_len, from_srv) = peer2_sock.recv_from(&mut peer2_recv).await.unwrap();
        assert_eq!(from_srv, server_addr);
        assert_eq!(p2_len, 8 + frame_payload.len());

        let p2_hdr = ProtoTunnelHeader::parse(&peer2_recv[..8]).unwrap();
        assert_eq!(p2_hdr.peer_id, peer1_id); // Source is Peer 1
        assert_eq!(&peer2_recv[8..p2_len], frame_payload);

        server_handle.await.unwrap();
    }
}
