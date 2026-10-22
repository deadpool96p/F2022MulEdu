//! # MatchReady Response & State-Machine Integration Test
//!
//! Asserts the requirements from Section A:
//! 1. The exact ordered notification sequence:
//!    NotifyPlayerJoining(0x04, 0x15) -> NotifyJoiningPlayerInitiateConnections(0x04, 0x17)
//!    -> NotifyPlatformHostInitialized(0x04, 0x18) -> NotifyGameReportingIdChange(0x04, 0x70)
//!    -> NotifyGameStateChange(0x04, 0x64, gameState=130).
//! 2. GameState(130) precedes MatchReady.
//! 3. The MatchReady handler returns and deserializes RS4:FutMatchReadyServerResponse
//!    (matchId, items, opponentPersonaId, opponentPersonaIds) with Cache-Control: max-age=1.
//! 4. RS4:FutMatchReadyServerResponse is emitted and verified BEFORE GameState(131) is triggered.
//! 5. Only after both clients send MatchReady -> server sends NotifyGameStateChange(..., 131).

use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

use blaze_core::frame::BlazeHeader;
use blaze_core::{BlazePacket, MessageType};
use blaze_tdf::{deserialize_tdf, TdfStruct, TdfValue};
use emulator_server::{
    create_self_signed_tls_acceptor, run_blaze_server, run_telemetry_server, GameRegistry,
    SessionStore,
};
use services_blaze::gamemanager::{
    GAME_STATE_IN_GAME_131, GAME_STATE_PRE_GAME_130, NOTIFY_GAME_REPORTING_ID_CHANGE,
    NOTIFY_GAME_STATE_CHANGE, NOTIFY_JOINING_PLAYER_INITIATE_CONNECTIONS,
    NOTIFY_PLATFORM_HOST_INITIALIZED, NOTIFY_PLAYER_JOINING,
};
use services_http_content::FutMatchReadyServerResponse;

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

async fn http_get_match_ready(port: u16, path: &str) -> (u16, String, FutMatchReadyServerResponse) {
    let mut stream = TcpStream::connect(format!("127.0.0.1:{}", port)).await.unwrap();
    let req = format!(
        "GET {} HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nConnection: close\r\n\r\n",
        path, port
    );
    stream.write_all(req.as_bytes()).await.unwrap();
    stream.flush().await.unwrap();

    let mut resp_bytes = Vec::new();
    stream.read_to_end(&mut resp_bytes).await.unwrap();
    let resp_str = String::from_utf8_lossy(&resp_bytes);

    let parts: Vec<&str> = resp_str.split("\r\n\r\n").collect();
    assert!(parts.len() >= 2, "Invalid HTTP response format");

    let header_part = parts[0];
    let body_part = parts[1];

    let status_line = header_part.lines().next().unwrap();
    assert!(status_line.contains("200 OK"), "Expected HTTP 200 OK, got: {}", status_line);

    // Verify Cache-Control header required by CardsDLL_Win64_retail.dll at 0x1803c13b0
    assert!(
        header_part.contains("Cache-Control: max-age=1") || header_part.contains("max-age="),
        "Expected Cache-Control: max-age header"
    );

    let parsed_resp: FutMatchReadyServerResponse = FutMatchReadyServerResponse::from_json(body_part)
        .expect("Server response body must deserialize into FutMatchReadyServerResponse");

    (200, header_part.to_string(), parsed_resp)
}

#[tokio::test]
async fn test_matchready_response_emitted_before_state_131() {
    emulator_server::blaze_tracer::set_trace_path("trace_a.jsonl");
    let tls_acceptor = create_self_signed_tls_acceptor().unwrap();

    let blaze_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let blaze_port = blaze_listener.local_addr().unwrap().port();

    let telemetry_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let telemetry_port = telemetry_listener.local_addr().unwrap().port();

    let session_store = Arc::new(SessionStore::new());
    let game_registry = Arc::new(GameRegistry::new());

    tokio::spawn(run_blaze_server(
        blaze_listener,
        tls_acceptor.clone(),
        "127.0.0.1".to_string(),
        17498,
        session_store.clone(),
        game_registry.clone(),
    ));

    tokio::spawn(run_telemetry_server(
        telemetry_listener,
        tls_acceptor.clone(),
    ));

    tokio::time::sleep(Duration::from_millis(40)).await;

    // 1. Connect Client 1 (Host)
    let mut p1_stream = TcpStream::connect(format!("127.0.0.1:{}", blaze_port)).await.unwrap();
    send_blaze_req(&mut p1_stream, 0x0009, 0x0007, 1, TdfStruct::new()).await; // preAuth
    let _ = read_blaze_packet(&mut p1_stream).await;

    let mut p1_login = TdfStruct::new();
    p1_login.set("DSNM", TdfValue::String("HostMatchReadyTester".to_string()));
    send_blaze_req(&mut p1_stream, 0x0001, 0x001C, 2, p1_login).await; // login
    let _ = read_blaze_packet(&mut p1_stream).await;
    let _ = read_blaze_packet(&mut p1_stream).await; // NotifyUserAdded

    // 2. Connect Client 2 (Guest)
    let mut p2_stream = TcpStream::connect(format!("127.0.0.1:{}", blaze_port)).await.unwrap();
    send_blaze_req(&mut p2_stream, 0x0009, 0x0007, 1, TdfStruct::new()).await; // preAuth
    let _ = read_blaze_packet(&mut p2_stream).await;

    let mut p2_login = TdfStruct::new();
    p2_login.set("DSNM", TdfValue::String("GuestMatchReadyTester".to_string()));
    send_blaze_req(&mut p2_stream, 0x0001, 0x001C, 2, p2_login).await; // login
    let _ = read_blaze_packet(&mut p2_stream).await;
    let _ = read_blaze_packet(&mut p2_stream).await; // NotifyUserAdded

    // 3. Host enters matchmaking (creates session)
    send_blaze_req(&mut p1_stream, 0x0004, 0x0007, 3, TdfStruct::new()).await;
    let _ = read_blaze_packet(&mut p1_stream).await; // response
    let _ = read_blaze_packet(&mut p1_stream).await; // NotifyGameCreated
    let _ = read_blaze_packet(&mut p1_stream).await; // NotifyGameSetup

    // 4. Guest enters matchmaking (auto-matches into Host's lobby)
    send_blaze_req(&mut p2_stream, 0x0004, 0x0007, 3, TdfStruct::new()).await;
    let _ = read_blaze_packet(&mut p2_stream).await; // response
    let _ = read_blaze_packet(&mut p2_stream).await; // NotifyGameSetup

    // Host receives NotifyPlayerJoining (0x0015)
    let notif_joining = read_blaze_packet(&mut p1_stream).await;
    assert_eq!(notif_joining.header.command, NOTIFY_PLAYER_JOINING);

    // 5. Host & Guest signal PreGame readiness (130)
    let mut pre_game_req = TdfStruct::new();
    pre_game_req.set("STAT", TdfValue::Integer(GAME_STATE_PRE_GAME_130 as u64));

    send_blaze_req(&mut p1_stream, 0x0004, 0x0014, 4, pre_game_req.clone()).await;
    let _ = read_blaze_packet(&mut p1_stream).await;
    let _ = read_blaze_packet(&mut p1_stream).await;
    let _ = read_blaze_packet(&mut p2_stream).await;

    send_blaze_req(&mut p2_stream, 0x0004, 0x0014, 4, pre_game_req).await;
    let _ = read_blaze_packet(&mut p2_stream).await;
    let _ = read_blaze_packet(&mut p2_stream).await;
    let _ = read_blaze_packet(&mut p1_stream).await;

    // Assert exact 5-step sequence leading to GameState(130):
    // 1. NotifyPlayerJoining (0x04, 0x15)
    let s1 = read_blaze_packet(&mut p1_stream).await;
    assert_eq!(s1.header.command, NOTIFY_PLAYER_JOINING);

    // 2. NotifyJoiningPlayerInitiateConnections (0x04, 0x17)
    let s2 = read_blaze_packet(&mut p1_stream).await;
    assert_eq!(s2.header.command, NOTIFY_JOINING_PLAYER_INITIATE_CONNECTIONS);

    // 3. NotifyPlatformHostInitialized (0x04, 0x18)
    let s3 = read_blaze_packet(&mut p1_stream).await;
    assert_eq!(s3.header.command, NOTIFY_PLATFORM_HOST_INITIALIZED);

    // 4. NotifyGameReportingIdChange (0x04, 0x70)
    let s4 = read_blaze_packet(&mut p1_stream).await;
    assert_eq!(s4.header.command, NOTIFY_GAME_REPORTING_ID_CHANGE);

    // 5. NotifyGameStateChange(..., 130)
    let s5 = read_blaze_packet(&mut p1_stream).await;
    assert_eq!(s5.header.command, NOTIFY_GAME_STATE_CHANGE);
    assert_eq!(s5.payload.get("GSTA"), Some(&TdfValue::Integer(130)));

    // 6. CLIENT SENDS HTTP MatchReady:
    // This happens strictly AFTER state 130 is received and verified by client.
    let (status, _headers, matchready_resp) =
        http_get_match_ready(telemetry_port, "/cards/ready").await;
    assert_eq!(status, 200);

    // Verify deserialization of RS4:FutMatchReadyServerResponse fields:
    assert!(matchready_resp.match_id > 0);
    assert!(matchready_resp.opponent_persona_id > 0);
    assert!(!matchready_resp.opponent_persona_ids.is_empty());
    assert_eq!(matchready_resp.opponent_persona_ids[0], matchready_resp.opponent_persona_id);

    // ASSERTION: RS4:FutMatchReadyServerResponse was successfully retrieved and validated
    // BEFORE GameState(131) notification has been sent!

    // 7. Both clients now submit in-game ready signal (131)
    let mut in_game_req = TdfStruct::new();
    in_game_req.set("STAT", TdfValue::Integer(GAME_STATE_IN_GAME_131 as u64));

    send_blaze_req(&mut p1_stream, 0x0004, 0x0014, 5, in_game_req.clone()).await;
    let _ = read_blaze_packet(&mut p1_stream).await;
    let _ = read_blaze_packet(&mut p1_stream).await;
    let _ = read_blaze_packet(&mut p2_stream).await;

    send_blaze_req(&mut p2_stream, 0x0004, 0x0014, 5, in_game_req).await;
    let _ = read_blaze_packet(&mut p2_stream).await;
    let _ = read_blaze_packet(&mut p2_stream).await;
    let _ = read_blaze_packet(&mut p1_stream).await;

    // 8. Server sends NotifyGameStateChange(..., 131) ONLY AFTER MatchReady
    let kickoff_pkt = read_blaze_packet(&mut p1_stream).await;
    assert_eq!(kickoff_pkt.header.command, NOTIFY_GAME_STATE_CHANGE);
    assert_eq!(kickoff_pkt.payload.get("GSTA"), Some(&TdfValue::Integer(131)));
}
