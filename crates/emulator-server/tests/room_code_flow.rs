//! # Steam-Independent Room-Code Matchmaking Flow Test
//!
//! Validates the Steam-independent room code matchmaking path:
//! 1. Host and Guest discover reflected endpoints via QoS/STUN responder (Port 17498).
//! 2. Host creates a room code via Control REST API (POST /api/v1/rooms/host).
//! 3. Guest joins via room code (POST /api/v1/rooms/join) with zero Steam API dependence (steam_id = 0).
//! 4. Control server verifies 2 members in room with slots 0 and 1.
//! 5. Both local processes connect to Blaze, and verify lobby setup and notification delivery.

use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream, UdpSocket};

use blaze_core::frame::BlazeHeader;
use blaze_core::{BlazePacket, MessageType};
use blaze_tdf::{deserialize_tdf, TdfStruct};
use emulator_server::{
    create_self_signed_tls_acceptor, run_blaze_server, run_control_server, GameRegistry,
    SessionStore,
};
use services_qos_relay::handle_qos_probe;

async fn send_blaze_req(
    stream: &mut TcpStream,
    comp: u16,
    cmd: u16,
    seq: u16,
    payload: TdfStruct,
) {
    let req = BlazePacket::new(comp, cmd, 0, MessageType::SubmitRequest, 0, seq, payload).unwrap();
    let req_bytes = req.encode().unwrap();
    stream.write_all(&req_bytes).await.unwrap();
    stream.flush().await.unwrap();
}

async fn read_blaze_packet(stream: &mut TcpStream) -> BlazePacket {
    let mut hdr_buf = [0u8; BlazeHeader::HEADER_SIZE];
    stream.read_exact(&mut hdr_buf).await.unwrap();
    let header = BlazeHeader::from_bytes(&hdr_buf).unwrap();

    let mut body_buf = vec![0u8; header.length as usize];
    stream.read_exact(&mut body_buf).await.unwrap();
    let payload = deserialize_tdf(&body_buf).unwrap();
    BlazePacket { header, payload }
}

#[tokio::test]
async fn test_steam_independent_room_code_flow() {
    // 1. Start QoS UDP Responder
    let qos_socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let qos_addr = qos_socket.local_addr().unwrap();
    let qos_port = qos_addr.port();

    tokio::spawn(async move {
        let mut buf = vec![0u8; 1024];
        while let Ok((len, src)) = qos_socket.recv_from(&mut buf).await {
            let resp = handle_qos_probe(&buf[..len], src);
            let _ = qos_socket.send_to(&resp, src).await;
        }
    });

    // 2. Start Blaze and Control Servers
    let tls_acceptor = create_self_signed_tls_acceptor().unwrap();
    let blaze_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let blaze_port = blaze_listener.local_addr().unwrap().port();

    let control_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let control_port = control_listener.local_addr().unwrap().port();

    let session_store = Arc::new(SessionStore::new());
    let game_registry = Arc::new(GameRegistry::new());

    tokio::spawn(run_blaze_server(
        blaze_listener,
        tls_acceptor,
        "127.0.0.1".to_string(),
        qos_port,
        session_store.clone(),
        game_registry.clone(),
    ));

    tokio::spawn(run_control_server(
        control_listener,
        session_store.clone(),
        game_registry.clone(),
    ));

    tokio::time::sleep(Duration::from_millis(50)).await;

    // 3. Process 1 (Host) probes QoS/STUN to verify NAT punch reflection
    let host_udp = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let host_probe = vec![0x01, 0x02, 0x03, 0x04, 0x00, 0x00];
    host_udp.send_to(&host_probe, qos_addr).await.unwrap();
    let mut host_udp_resp = vec![0u8; 1024];
    let (n_host, _) = host_udp.recv_from(&mut host_udp_resp).await.unwrap();
    assert!(n_host >= 16);
    assert_eq!(&host_udp_resp[0..4], &[0x01, 0x02, 0x03, 0x04]);

    // 4. Process 2 (Guest) probes QoS/STUN to verify NAT punch reflection
    let guest_udp = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let guest_probe = vec![0x05, 0x06, 0x07, 0x08, 0x00, 0x00];
    guest_udp.send_to(&guest_probe, qos_addr).await.unwrap();
    let mut guest_udp_resp = vec![0u8; 1024];
    let (n_guest, _) = guest_udp.recv_from(&mut guest_udp_resp).await.unwrap();
    assert!(n_guest >= 16);
    assert_eq!(&guest_udp_resp[0..4], &[0x05, 0x06, 0x07, 0x08]);

    // 5. Host establishes Blaze session (without Steam)
    let mut host_stream = TcpStream::connect(format!("127.0.0.1:{}", blaze_port))
        .await
        .unwrap();
    send_blaze_req(&mut host_stream, 0x0009, 0x0007, 1, TdfStruct::new()).await;
    let _ = read_blaze_packet(&mut host_stream).await;

    send_blaze_req(&mut host_stream, 0x0001, 0x001C, 2, TdfStruct::new()).await;
    let _ = read_blaze_packet(&mut host_stream).await; // Login response
    let _ = read_blaze_packet(&mut host_stream).await; // NotifyUserAdded (0x7802)

    // 6. Host hosts room code via POST /api/v1/rooms/host (Steam-independent)
    let mut host_http = TcpStream::connect(format!("127.0.0.1:{}", control_port))
        .await
        .unwrap();
    let host_body = r#"{"persona_name":"LAN_Host","steam_id":0}"#;
    let host_req = format!(
        "POST /api/v1/rooms/host HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Length: {}\r\nContent-Type: application/json\r\n\r\n{}",
        host_body.len(),
        host_body
    );
    host_http.write_all(host_req.as_bytes()).await.unwrap();
    let mut host_http_resp = vec![0u8; 1024];
    let n = host_http.read(&mut host_http_resp).await.unwrap();
    let host_str = String::from_utf8_lossy(&host_http_resp[..n]);
    assert!(host_str.contains("\"status\":\"ok\""));

    let code_idx = host_str.find("\"room_code\":\"").unwrap() + 13;
    let room_code = host_str[code_idx..code_idx + 6].to_string();
    assert_eq!(room_code.len(), 6);

    let conn_key = "\"connection_string\":\"";
    let conn_idx = host_str.find(conn_key).unwrap() + conn_key.len();
    let conn_end = host_str[conn_idx..].find('"').unwrap();
    let conn_string = host_str[conn_idx..conn_idx + conn_end].to_string();
    assert!(conn_string.starts_with("127.0.0.1:"));
    assert!(conn_string.ends_with(&room_code));

    // Host receives NotifyGameSetup (0x0014) on Blaze
    let setup_p1 = read_blaze_packet(&mut host_stream).await;
    assert_eq!(setup_p1.header.component, 0x0004);
    assert_eq!(setup_p1.header.command, 0x0014);

    // 7. Guest establishes Blaze session (without Steam)
    let mut guest_stream = TcpStream::connect(format!("127.0.0.1:{}", blaze_port))
        .await
        .unwrap();
    send_blaze_req(&mut guest_stream, 0x0009, 0x0007, 1, TdfStruct::new()).await;
    let _ = read_blaze_packet(&mut guest_stream).await;

    send_blaze_req(&mut guest_stream, 0x0001, 0x001C, 2, TdfStruct::new()).await;
    let _ = read_blaze_packet(&mut guest_stream).await;
    let _ = read_blaze_packet(&mut guest_stream).await;

    // 8. Guest joins room code via POST /api/v1/rooms/join using connection_string (HOST:PORT:TOKEN)
    let mut guest_http = TcpStream::connect(format!("127.0.0.1:{}", control_port))
        .await
        .unwrap();
    let join_body = format!(
        r#"{{"connection_string":"{}","persona_name":"LAN_Guest","steam_id":0}}"#,
        conn_string
    );
    let join_req = format!(
        "POST /api/v1/rooms/join HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Length: {}\r\nContent-Type: application/json\r\n\r\n{}",
        join_body.len(),
        join_body
    );
    guest_http.write_all(join_req.as_bytes()).await.unwrap();
    let mut guest_http_resp = vec![0u8; 1024];
    let n2 = guest_http.read(&mut guest_http_resp).await.unwrap();
    let guest_str = String::from_utf8_lossy(&guest_http_resp[..n2]);
    assert!(guest_str.contains("\"status\":\"ok\""));
    assert!(guest_str.contains("\"slot\":1"));

    // 9. Host receives NotifyPlayerJoining (0x0015) with Guest information
    let join_notif = read_blaze_packet(&mut host_stream).await;
    assert_eq!(join_notif.header.component, 0x0004);
    assert_eq!(join_notif.header.command, 0x0015);

    // Guest receives NotifyGameSetup (0x0014)
    let guest_setup = read_blaze_packet(&mut guest_stream).await;
    assert_eq!(guest_setup.header.component, 0x0004);
    assert_eq!(guest_setup.header.command, 0x0014);

    // 10. Test Symmetric NAT detection & fallback: Probe with SYMMETRIC flag (0x02)
    let sym_probe = vec![0x09, 0x0A, 0x0B, 0x0C, 0x02, 0x00];
    guest_udp.send_to(&sym_probe, qos_addr).await.unwrap();
    let mut sym_resp = vec![0u8; 1024];
    let (n_sym, _) = guest_udp.recv_from(&mut sym_resp).await.unwrap();
    assert!(n_sym >= 16);
    // NAT Type = 2 (SYMMETRIC)
    assert_eq!(&sym_resp[14..16], &[0, 2]);
    assert!(services_qos_relay::is_relay_only_mode());

    // Trigger relay notification over Blaze to both peers
    let active_game = game_registry.find_by_room_code(&room_code).unwrap();
    let notify_res = services_blaze::notify_relay_mode_switch(active_game.game_id, &game_registry, &session_store);
    assert!(notify_res.is_ok());

    // Host and guest receive NotifyGameAttribChange with ATTR_FORCE_RELAY
    let host_relay_notif = read_blaze_packet(&mut host_stream).await;
    assert_eq!(host_relay_notif.header.command, 0x0050); // NOTIFY_GAME_ATTRIB_CHANGE
    let _ = read_blaze_packet(&mut host_stream).await; // 0x0018 alt
    let guest_relay_notif = read_blaze_packet(&mut guest_stream).await;
    assert_eq!(guest_relay_notif.header.command, 0x0050);

    // 11. Query Control API Status: verify both members are joined with active room code
    let mut status_http = TcpStream::connect(format!("127.0.0.1:{}", control_port))
        .await
        .unwrap();
    let status_req = "GET /api/v1/status HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n";
    status_http.write_all(status_req.as_bytes()).await.unwrap();
    let mut status_resp = vec![0u8; 2048];
    let n_st = status_http.read(&mut status_resp).await.unwrap();
    let status_str = String::from_utf8_lossy(&status_resp[..n_st]);

    assert!(status_str.contains(&format!(r#""room_code":"{}""#, room_code)));
    assert!(status_str.contains("LAN_Host"));
    assert!(status_str.contains("LAN_Guest"));
}
