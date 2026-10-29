//! # Full Match Kickoff & 60 Hz Input Sync Integration Test
//!
//! Validates Milestones 4 & 5 end-to-end:
//! 1. Canonical Squad Hash (`ATTR_SQDH`) synchronization via HTTP `/content/squads/hash`.
//! 2. Deterministic 32-bit random seed (`ATTR_SEED`) distribution.
//! 3. Team selection (`ATTR_TEAM_HOME`, `ATTR_TEAM_AWAY`, `ATTR_KITS`) attribute broadcast.
//! 4. Pre-Kickoff state machine: `PRE_GAME -> IN_GAME` symmetric transitions.
//! 5. ProtoTunnel UDP Relay (:3659) registration and 60 Hz `NetGameDist` input frame replication.

use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream, UdpSocket};

use blaze_core::frame::BlazeHeader;
use blaze_core::{BlazePacket, MessageType};
use blaze_tdf::{deserialize_tdf, TdfMap, TdfStruct, TdfType, TdfValue};
use emulator_server::{
    create_self_signed_tls_acceptor, run_blaze_server, run_relay_server, run_telemetry_server,
    GameRegistry, PeerTable, SessionStore,
};
use services_http_content::SquadStore;

const FLAG_DATA: u16 = 0x0000;
const FLAG_REGISTER: u16 = 0x0001;
const FLAG_ACK: u16 = 0x0004;
const PROTOTUNNEL_HEADER_LEN: usize = 8;

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
async fn test_full_match_kickoff_and_60hz_sync() {
    let tls_acceptor = create_self_signed_tls_acceptor().unwrap();

    // 1. Bind Dynamic Test Sockets
    let blaze_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let blaze_port = blaze_listener.local_addr().unwrap().port();

    let telemetry_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let telemetry_port = telemetry_listener.local_addr().unwrap().port();

    let relay_socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let relay_addr = relay_socket.local_addr().unwrap();

    let session_store = Arc::new(SessionStore::new());
    let game_registry = Arc::new(GameRegistry::new());
    let peer_table = Arc::new(PeerTable::new());

    // Spawn Blaze Server
    tokio::spawn(run_blaze_server(
        blaze_listener,
        tls_acceptor.clone(),
        "127.0.0.1".to_string(),
        17498,
        session_store.clone(),
        game_registry.clone(),
    ));

    // Spawn Telemetry / HTTP Content Server (Serving /content/squads/hash)
    tokio::spawn(run_telemetry_server(
        telemetry_listener,
        tls_acceptor.clone(),
    ));

    // Spawn ProtoTunnel UDP Relay Server (:3659)
    let pt_table = peer_table.clone();
    tokio::spawn(async move {
        let _ = run_relay_server(relay_socket, pt_table).await;
    });

    tokio::time::sleep(Duration::from_millis(50)).await;

    // =========================================================================
    // Phase 1: HTTP Canonical Squad Hash Verification
    // =========================================================================
    let mut http_client = TcpStream::connect(format!("127.0.0.1:{}", telemetry_port))
        .await
        .unwrap();
    let http_req = format!(
        "GET /content/squads/hash HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nConnection: close\r\n\r\n",
        telemetry_port
    );
    http_client.write_all(http_req.as_bytes()).await.unwrap();

    let mut http_resp_buf = vec![0u8; 1024];
    let n = http_client.read(&mut http_resp_buf).await.unwrap();
    let http_resp = String::from_utf8_lossy(&http_resp_buf[..n]);
    assert!(http_resp.contains("HTTP/1.1 200 OK"));
    assert!(http_resp.contains("sha256"));

    let canonical_hash = SquadStore::global().get_canonical_hash();
    assert!(http_resp.contains(&canonical_hash));

    // =========================================================================
    // Phase 2: Player Authentication & Lobby Setup
    // =========================================================================
    // Connect Player 1 (Host)
    let mut p1_stream = TcpStream::connect(format!("127.0.0.1:{}", blaze_port))
        .await
        .unwrap();
    send_blaze_req(&mut p1_stream, 0x0009, 0x0007, 1, TdfStruct::new()).await; // preAuth
    let _ = read_blaze_packet(&mut p1_stream).await;

    let mut p1_login = TdfStruct::new();
    p1_login.set("DSNM", TdfValue::String("RealMadridHost".to_string()));
    send_blaze_req(&mut p1_stream, 0x0001, 0x00E6, 2, p1_login).await; // login
    let _ = read_blaze_packet(&mut p1_stream).await;
    let _ = read_blaze_packet(&mut p1_stream).await; // NotifyUserAdded (0x7802)

    // Connect Player 2 (Guest)
    let mut p2_stream = TcpStream::connect(format!("127.0.0.1:{}", blaze_port))
        .await
        .unwrap();
    send_blaze_req(&mut p2_stream, 0x0009, 0x0007, 1, TdfStruct::new()).await; // preAuth
    let _ = read_blaze_packet(&mut p2_stream).await;

    let mut p2_login = TdfStruct::new();
    p2_login.set("DSNM", TdfValue::String("LiverpoolGuest".to_string()));
    send_blaze_req(&mut p2_stream, 0x0001, 0x00E6, 2, p2_login).await; // login
    let _ = read_blaze_packet(&mut p2_stream).await;
    let _ = read_blaze_packet(&mut p2_stream).await; // NotifyUserAdded (0x7802)

    // Player 1 enters matchmaking (creates session)
    send_blaze_req(&mut p1_stream, 0x0004, 0x0007, 3, TdfStruct::new()).await;
    let _ = read_blaze_packet(&mut p1_stream).await; // response
    let _ = read_blaze_packet(&mut p1_stream).await; // NotifyGameCreated (0x000F)
    let p1_setup = read_blaze_packet(&mut p1_stream).await; // NotifyGameSetup (0x0014)

    // Inspect Player 1's SEED & Squad Hash in NotifyGameSetup
    let p1_game_struct = match p1_setup.payload.get("GAME") {
        Some(TdfValue::Struct(s)) => s,
        _ => panic!("Expected GAME struct in NotifyGameSetup"),
    };
    let p1_seed = match p1_game_struct.get("SEED") {
        Some(TdfValue::Integer(sd)) => *sd as u32,
        _ => panic!("Expected SEED in GAME struct"),
    };
    assert!(p1_seed > 0, "Deterministic RNG seed must be non-zero");

    // Player 2 enters matchmaking (joins Player 1's lobby)
    send_blaze_req(&mut p2_stream, 0x0004, 0x0007, 3, TdfStruct::new()).await;
    let _ = read_blaze_packet(&mut p2_stream).await; // response
    let p2_setup = read_blaze_packet(&mut p2_stream).await; // NotifyGameSetup (0x0014)

    let _ = read_blaze_packet(&mut p1_stream).await; // Player 1 receives NotifyPlayerJoining (0x0015)

    let p2_game_struct = match p2_setup.payload.get("GAME") {
        Some(TdfValue::Struct(s)) => s,
        _ => panic!("Expected GAME struct in Player 2 NotifyGameSetup"),
    };
    let p2_seed = match p2_game_struct.get("SEED") {
        Some(TdfValue::Integer(sd)) => *sd as u32,
        _ => panic!("Expected SEED in Player 2 GAME struct"),
    };

    // Verify both peers received the identical deterministic seed
    assert_eq!(p1_seed, p2_seed, "Both players must share identical ATTR_SEED");

    // =========================================================================
    // Phase 3: Team Selection & Attribute Broadcasts
    // =========================================================================
    let mut team_attrs = Vec::new();
    team_attrs.push((TdfValue::String("ATTR_TEAM_HOME".into()), TdfValue::String("243".into()))); // Real Madrid
    team_attrs.push((TdfValue::String("ATTR_TEAM_AWAY".into()), TdfValue::String("9".into())));   // Liverpool
    team_attrs.push((TdfValue::String("ATTR_KITS".into()), TdfValue::String("1".into())));
    team_attrs.push((TdfValue::String("ATTR_SQDH".into()), TdfValue::String(canonical_hash.clone())));

    let mut update_attr_req = TdfStruct::new();
    update_attr_req.set("ATTR", TdfValue::Map(TdfMap::new(TdfType::String, TdfType::String, team_attrs)));

    send_blaze_req(&mut p1_stream, 0x0004, 0x0016, 4, update_attr_req).await;
    let _ = read_blaze_packet(&mut p1_stream).await; // Response

    // Both players receive NotifyGameAttribChange (0x0050 & 0x0018)
    let p1_attr_notif = read_blaze_packet(&mut p1_stream).await;
    assert_eq!(p1_attr_notif.header.command, 0x0050);
    let _ = read_blaze_packet(&mut p1_stream).await; // 0x0018 alt

    let p2_attr_notif = read_blaze_packet(&mut p2_stream).await;
    assert_eq!(p2_attr_notif.header.command, 0x0050);
    let _ = read_blaze_packet(&mut p2_stream).await; // 0x0018 alt

    // =========================================================================
    // Phase 4: Pre-Game Transition
    // =========================================================================
    let mut pre_game_req = TdfStruct::new();
    pre_game_req.set("STAT", TdfValue::Integer(3)); // GAME_STATE_PRE_GAME

    // Player 1 signals PRE_GAME
    send_blaze_req(&mut p1_stream, 0x0004, 0x0014, 5, pre_game_req.clone()).await;
    let _ = read_blaze_packet(&mut p1_stream).await; // Response
    let _ = read_blaze_packet(&mut p1_stream).await; // NotifyPlayerStateChange
    let _ = read_blaze_packet(&mut p2_stream).await; // P2 receives NotifyPlayerStateChange

    // Player 2 signals PRE_GAME -> triggers symmetric transition into PRE_GAME
    send_blaze_req(&mut p2_stream, 0x0004, 0x0014, 4, pre_game_req).await;
    let _ = read_blaze_packet(&mut p2_stream).await; // Response
    let _ = read_blaze_packet(&mut p2_stream).await; // NotifyPlayerStateChange
    let _ = read_blaze_packet(&mut p1_stream).await; // P1 receives NotifyPlayerStateChange

    let mut p1_state_change = read_blaze_packet(&mut p1_stream).await;
    while p1_state_change.header.command != 0x0064 {
        p1_state_change = read_blaze_packet(&mut p1_stream).await;
    }
    assert_eq!(p1_state_change.header.command, 0x0064);

    let mut p2_state_change = read_blaze_packet(&mut p2_stream).await;
    while p2_state_change.header.command != 0x0064 {
        p2_state_change = read_blaze_packet(&mut p2_stream).await;
    }
    assert_eq!(p2_state_change.header.command, 0x0064);

    // =========================================================================
    // Phase 5: ProtoTunnel UDP Relay Registration (:3659)
    // =========================================================================
    let p1_udp = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let p2_udp = UdpSocket::bind("127.0.0.1:0").await.unwrap();

    // Register Player 1 as Peer 1 in Session 1
    let mut p1_reg_pkt = Vec::with_capacity(PROTOTUNNEL_HEADER_LEN);
    p1_reg_pkt.extend_from_slice(&1u16.to_be_bytes()); // session_id
    p1_reg_pkt.extend_from_slice(&1u16.to_be_bytes()); // peer_id
    p1_reg_pkt.extend_from_slice(&0u16.to_be_bytes()); // payload_len
    p1_reg_pkt.extend_from_slice(&FLAG_REGISTER.to_be_bytes());
    p1_udp.send_to(&p1_reg_pkt, relay_addr).await.unwrap();

    let mut ack_buf = [0u8; 32];
    let (n1, _) = p1_udp.recv_from(&mut ack_buf).await.unwrap();
    assert_eq!(n1, PROTOTUNNEL_HEADER_LEN);
    let p1_ack_flags = u16::from_be_bytes([ack_buf[6], ack_buf[7]]);
    assert!(p1_ack_flags & FLAG_ACK != 0, "Player 1 must receive registration ACK");

    // Register Player 2 as Peer 2 in Session 1
    let mut p2_reg_pkt = Vec::with_capacity(PROTOTUNNEL_HEADER_LEN);
    p2_reg_pkt.extend_from_slice(&1u16.to_be_bytes()); // session_id
    p2_reg_pkt.extend_from_slice(&2u16.to_be_bytes()); // peer_id
    p2_reg_pkt.extend_from_slice(&0u16.to_be_bytes()); // payload_len
    p2_reg_pkt.extend_from_slice(&FLAG_REGISTER.to_be_bytes());
    p2_udp.send_to(&p2_reg_pkt, relay_addr).await.unwrap();

    let (n2, _) = p2_udp.recv_from(&mut ack_buf).await.unwrap();
    assert_eq!(n2, PROTOTUNNEL_HEADER_LEN);
    let p2_ack_flags = u16::from_be_bytes([ack_buf[6], ack_buf[7]]);
    assert!(p2_ack_flags & FLAG_ACK != 0, "Player 2 must receive registration ACK");

    // =========================================================================
    // Phase 6: Symmetric Kickoff Trigger (IN_GAME)
    // =========================================================================
    let mut in_game_req = TdfStruct::new();
    in_game_req.set("STAT", TdfValue::Integer(4)); // GAME_STATE_IN_GAME

    send_blaze_req(&mut p1_stream, 0x0004, 0x0014, 6, in_game_req.clone()).await;
    let _ = read_blaze_packet(&mut p1_stream).await; // Response
    let _ = read_blaze_packet(&mut p1_stream).await; // NotifyPlayerStateChange
    let _ = read_blaze_packet(&mut p2_stream).await; // P2 receives NotifyPlayerStateChange

    send_blaze_req(&mut p2_stream, 0x0004, 0x0014, 5, in_game_req).await;
    let _ = read_blaze_packet(&mut p2_stream).await; // Response
    let _ = read_blaze_packet(&mut p2_stream).await; // NotifyPlayerStateChange
    let _ = read_blaze_packet(&mut p1_stream).await; // P1 receives NotifyPlayerStateChange

    // Opening Whistle: Both players receive symmetric Kickoff notification (GSTA = 4 or 131)
    let mut p1_kickoff = read_blaze_packet(&mut p1_stream).await;
    while p1_kickoff.header.command != 0x0064 {
        p1_kickoff = read_blaze_packet(&mut p1_stream).await;
    }
    assert_eq!(p1_kickoff.header.command, 0x0064);

    let mut p2_kickoff = read_blaze_packet(&mut p2_stream).await;
    while p2_kickoff.header.command != 0x0064 {
        p2_kickoff = read_blaze_packet(&mut p2_stream).await;
    }
    assert_eq!(p2_kickoff.header.command, 0x0064);

    // =========================================================================
    // Phase 7: 60 Hz NetGameDist Input Replication (60 Frames)
    // =========================================================================
    let mut p1_recv_buf = [0u8; 128];
    let mut p2_recv_buf = [0u8; 128];

    for frame in 0..60u32 {
        let frame_start = Instant::now();

        // P1 sends controller input frame to P2 via Relay (Session 1, Target Peer 2)
        let mut p1_input_payload = vec![0xDE, 0xAD];
        p1_input_payload.extend_from_slice(&frame.to_be_bytes()); // Frame sequence
        p1_input_payload.extend_from_slice(&[0x10, 0x20, 0x30, 0x40]); // Buttons & Analogs

        let mut p1_pkt = Vec::with_capacity(PROTOTUNNEL_HEADER_LEN + p1_input_payload.len());
        p1_pkt.extend_from_slice(&1u16.to_be_bytes()); // session_id
        p1_pkt.extend_from_slice(&2u16.to_be_bytes()); // target peer_id
        p1_pkt.extend_from_slice(&(p1_input_payload.len() as u16).to_be_bytes());
        p1_pkt.extend_from_slice(&FLAG_DATA.to_be_bytes());
        p1_pkt.extend_from_slice(&p1_input_payload);

        p1_udp.send_to(&p1_pkt, relay_addr).await.unwrap();

        // P2 receives P1's frame
        let (p2_bytes, _) = p2_udp.recv_from(&mut p2_recv_buf).await.unwrap();
        assert_eq!(p2_bytes, PROTOTUNNEL_HEADER_LEN + p1_input_payload.len());
        assert_eq!(&p2_recv_buf[PROTOTUNNEL_HEADER_LEN..p2_bytes], &p1_input_payload[..]);

        // P2 sends controller input frame to P1 via Relay (Session 1, Target Peer 1)
        let mut p2_input_payload = vec![0xBE, 0xEF];
        p2_input_payload.extend_from_slice(&frame.to_be_bytes());
        p2_input_payload.extend_from_slice(&[0x01, 0x02, 0x03, 0x04]);

        let mut p2_pkt = Vec::with_capacity(PROTOTUNNEL_HEADER_LEN + p2_input_payload.len());
        p2_pkt.extend_from_slice(&1u16.to_be_bytes()); // session_id
        p2_pkt.extend_from_slice(&1u16.to_be_bytes()); // target peer_id
        p2_pkt.extend_from_slice(&(p2_input_payload.len() as u16).to_be_bytes());
        p2_pkt.extend_from_slice(&FLAG_DATA.to_be_bytes());
        p2_pkt.extend_from_slice(&p2_input_payload);

        p2_udp.send_to(&p2_pkt, relay_addr).await.unwrap();

        // P1 receives P2's frame
        let (p1_bytes, _) = p1_udp.recv_from(&mut p1_recv_buf).await.unwrap();
        assert_eq!(p1_bytes, PROTOTUNNEL_HEADER_LEN + p2_input_payload.len());
        assert_eq!(&p1_recv_buf[PROTOTUNNEL_HEADER_LEN..p1_bytes], &p2_input_payload[..]);

        let latency = frame_start.elapsed();
        assert!(
            latency < Duration::from_millis(15),
            "Frame {} latency was {:?}, expected < 15ms loopback",
            frame,
            latency
        );
    }
}
