//! End-to-End Lobby & Matchmaking Integration Test.
//! Validates:
//! - Flow A: Native Auto-Matchmaking (two players using native menus matchmake into a 1v1 lobby)
//! - Flow B: Room Code Control API (hosting via REST API and joining via room code)

use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use blaze_core::frame::BlazeHeader;
use blaze_core::{BlazePacket, MessageType};
use blaze_tdf::{deserialize_tdf, TdfStruct, TdfValue};
use emulator_server::{
    create_self_signed_tls_acceptor, run_blaze_server, run_control_server, GameRegistry,
    SessionStore,
};

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
async fn test_flow_a_native_menu_matchmaking() {
    let tls_acceptor = create_self_signed_tls_acceptor().unwrap();

    let blaze_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let blaze_port = blaze_listener.local_addr().unwrap().port();

    let session_store = Arc::new(SessionStore::new());
    let game_registry = Arc::new(GameRegistry::new());

    tokio::spawn(run_blaze_server(
        blaze_listener,
        tls_acceptor,
        "127.0.0.1".to_string(),
        17498,
        session_store.clone(),
        game_registry.clone(),
    ));

    tokio::time::sleep(Duration::from_millis(50)).await;

    // Connect Player 1
    let mut p1_stream = TcpStream::connect(format!("127.0.0.1:{}", blaze_port))
        .await
        .unwrap();
    send_blaze_req(&mut p1_stream, 0x0009, 0x0007, 1, TdfStruct::new()).await;
    let _ = read_blaze_packet(&mut p1_stream).await;

    let mut p1_login = TdfStruct::new();
    p1_login.set("DSNM", TdfValue::String("NativePlayer1".to_string()));
    send_blaze_req(&mut p1_stream, 0x0001, 0x00E6, 2, p1_login).await;
    let _ = read_blaze_packet(&mut p1_stream).await;
    let _ = read_blaze_packet(&mut p1_stream).await; // NotifyUserAdded (0x7802)

    // Connect Player 2
    let mut p2_stream = TcpStream::connect(format!("127.0.0.1:{}", blaze_port))
        .await
        .unwrap();
    send_blaze_req(&mut p2_stream, 0x0009, 0x0007, 1, TdfStruct::new()).await;
    let _ = read_blaze_packet(&mut p2_stream).await;

    let mut p2_login = TdfStruct::new();
    p2_login.set("DSNM", TdfValue::String("NativePlayer2".to_string()));
    send_blaze_req(&mut p2_stream, 0x0001, 0x00E6, 2, p2_login).await;
    let _ = read_blaze_packet(&mut p2_stream).await;
    let _ = read_blaze_packet(&mut p2_stream).await; // NotifyUserAdded (0x7802)

    // Player 1 clicks "Search Match" (startMatchmaking)
    send_blaze_req(&mut p1_stream, 0x0004, 0x0007, 3, TdfStruct::new()).await;
    let p1_resp = read_blaze_packet(&mut p1_stream).await;
    assert_eq!(p1_resp.header.command, 0x0007);
    assert!(p1_resp.payload.get("MSID").is_some());

    // Player 1 receives NotifyGameCreated and NotifyGameSetup
    let p1_notif1 = read_blaze_packet(&mut p1_stream).await;
    assert_eq!(p1_notif1.header.command, 0x000F); // NotifyGameCreated
    let p1_notif2 = read_blaze_packet(&mut p1_stream).await;
    assert_eq!(p1_notif2.header.command, 0x0014); // NotifyGameSetup

    // Player 2 clicks "Search Match" (startMatchmaking) -> auto-matched into Player 1's lobby!
    send_blaze_req(&mut p2_stream, 0x0004, 0x0007, 3, TdfStruct::new()).await;
    let p2_resp = read_blaze_packet(&mut p2_stream).await;
    assert_eq!(p2_resp.header.command, 0x0007);

    // Player 2 receives NotifyGameSetup with host information
    let p2_setup = read_blaze_packet(&mut p2_stream).await;
    assert_eq!(p2_setup.header.command, 0x0014);

    // Player 1 asynchronously receives NotifyPlayerJoining!
    let p1_joined_notif = read_blaze_packet(&mut p1_stream).await;
    assert_eq!(p1_joined_notif.header.command, 0x0015); // NotifyPlayerJoining
}

#[tokio::test]
async fn test_flow_b_room_code_control_api() {
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
        17498,
        session_store.clone(),
        game_registry.clone(),
    ));

    tokio::spawn(run_control_server(
        control_listener,
        session_store.clone(),
        game_registry.clone(),
    ));

    tokio::time::sleep(Duration::from_millis(50)).await;

    // Connect Player 1 & 2 to Blaze
    let mut p1_stream = TcpStream::connect(format!("127.0.0.1:{}", blaze_port))
        .await
        .unwrap();
    send_blaze_req(&mut p1_stream, 0x0001, 0x00E6, 1, TdfStruct::new()).await;
    let _ = read_blaze_packet(&mut p1_stream).await;
    let _ = read_blaze_packet(&mut p1_stream).await; // NotifyUserAdded (0x7802)

    let mut p2_stream = TcpStream::connect(format!("127.0.0.1:{}", blaze_port))
        .await
        .unwrap();
    send_blaze_req(&mut p2_stream, 0x0001, 0x00E6, 1, TdfStruct::new()).await;
    let _ = read_blaze_packet(&mut p2_stream).await;
    let _ = read_blaze_packet(&mut p2_stream).await; // NotifyUserAdded (0x7802)

    // 1. Query GET /api/v1/status
    let mut http_client = TcpStream::connect(format!("127.0.0.1:{}", control_port))
        .await
        .unwrap();
    http_client
        .write_all(b"GET /api/v1/status HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n")
        .await
        .unwrap();
    let mut status_resp = vec![0u8; 1024];
    let n = http_client.read(&mut status_resp).await.unwrap();
    let status_str = String::from_utf8_lossy(&status_resp[..n]);
    assert!(status_str.contains("\"status\":\"online\""));
    assert!(status_str.contains("\"players_online\":2"));

    // 2. Player 1 hosts via POST /api/v1/rooms/host
    let mut host_client = TcpStream::connect(format!("127.0.0.1:{}", control_port))
        .await
        .unwrap();
    host_client
        .write_all(b"POST /api/v1/rooms/host HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n")
        .await
        .unwrap();
    let mut host_resp = vec![0u8; 1024];
    let n = host_client.read(&mut host_resp).await.unwrap();
    let host_str = String::from_utf8_lossy(&host_resp[..n]);
    assert!(host_str.contains("\"status\":\"ok\""));
    assert!(host_str.contains("\"room_code\":\""));

    // Extract room code
    let code_idx = host_str.find("\"room_code\":\"").unwrap() + 13;
    let room_code = &host_str[code_idx..code_idx + 6];

    // Player 1 receives NotifyGameSetup
    let p1_setup = read_blaze_packet(&mut p1_stream).await;
    assert_eq!(p1_setup.header.command, 0x0014);

    // 3. Player 2 joins via POST /api/v1/rooms/join
    let mut join_client = TcpStream::connect(format!("127.0.0.1:{}", control_port))
        .await
        .unwrap();
    let join_body = format!("{{\"room_code\":\"{}\"}}", room_code);
    let join_req = format!(
        "POST /api/v1/rooms/join HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Length: {}\r\n\r\n{}",
        join_body.len(),
        join_body
    );
    join_client.write_all(join_req.as_bytes()).await.unwrap();
    let mut join_resp = vec![0u8; 1024];
    let n = join_client.read(&mut join_resp).await.unwrap();
    let join_str = String::from_utf8_lossy(&join_resp[..n]);
    assert!(join_str.contains("\"joined\":true"));

    // Player 2 receives NotifyGameSetup
    let p2_setup = read_blaze_packet(&mut p2_stream).await;
    assert_eq!(p2_setup.header.command, 0x0014);

    // Player 1 receives NotifyPlayerJoining!
    let p1_joining = read_blaze_packet(&mut p1_stream).await;
    assert_eq!(p1_joining.header.command, 0x0015);
}

#[tokio::test]
async fn test_flow_c_steam_identity_and_invite_sync() {
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
        17498,
        session_store.clone(),
        game_registry.clone(),
    ));

    tokio::spawn(run_control_server(
        control_listener,
        session_store.clone(),
        game_registry.clone(),
    ));

    tokio::time::sleep(Duration::from_millis(50)).await;

    // 1. Register Steam Identity via POST /api/v1/steam/identity
    let mut steam_client = TcpStream::connect(format!("127.0.0.1:{}", control_port))
        .await
        .unwrap();
    let steam_body = r#"{"steam_id":"76561198012345678","persona_name":"SteamStriker","friends":[{"steam_id":"76561198087654321","name":"SteamDefender"}]}"#;
    let steam_req = format!(
        "POST /api/v1/steam/identity HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Length: {}\r\n\r\n{}",
        steam_body.len(),
        steam_body
    );
    steam_client.write_all(steam_req.as_bytes()).await.unwrap();
    let mut steam_resp = vec![0u8; 1024];
    let n = steam_client.read(&mut steam_resp).await.unwrap();
    let steam_str = String::from_utf8_lossy(&steam_resp[..n]);
    assert!(steam_str.contains("\"status\":\"ok\""));
    assert!(steam_str.contains("SteamStriker"));

    // 2. Connect Player 1 (should automatically adopt Steam Identity)
    let mut p1_stream = TcpStream::connect(format!("127.0.0.1:{}", blaze_port))
        .await
        .unwrap();
    send_blaze_req(&mut p1_stream, 0x0009, 0x0007, 1, TdfStruct::new()).await;
    let _ = read_blaze_packet(&mut p1_stream).await;

    send_blaze_req(&mut p1_stream, 0x0001, 0x00E6, 2, TdfStruct::new()).await;
    let p1_login_resp = read_blaze_packet(&mut p1_stream).await;
    let _ = read_blaze_packet(&mut p1_stream).await; // NotifyUserAdded (0x7802)
    // `createWalUserSession` carries the display name in `PDTL.DSNM`.
    let p1_persona = match p1_login_resp.payload.get("PDTL") {
        Some(TdfValue::Struct(s)) => s,
        other => panic!("createWalUserSession must carry PDTL, got {:?}", other),
    };
    assert_eq!(
        p1_persona.get("DSNM").unwrap(),
        &TdfValue::String("SteamStriker".to_string())
    );

    // 3. Verify Steam Defender friend appears in AssociationLists
    send_blaze_req(&mut p1_stream, 0x0019, 0x0001, 3, TdfStruct::new()).await;
    let p1_lists_resp = read_blaze_packet(&mut p1_stream).await;
    let lists_val = p1_lists_resp.payload.get("LST").unwrap();
    if let TdfValue::List(outer) = lists_val {
        if let TdfValue::Struct(flist) = &outer.items[0] {
            if let Some(TdfValue::List(members)) = flist.get("LMEM") {
                let mut names = Vec::new();
                for m in &members.items {
                    if let TdfValue::Struct(st) = m {
                        if let Some(TdfValue::String(name)) = st.get("NAME") {
                            names.push(name.clone());
                        }
                    }
                }
                assert!(names.contains(&"SteamDefender".to_string()));
            }
        }
    }

    // 4. Player 1 hosts room via POST /api/v1/rooms/host
    let mut host_client = TcpStream::connect(format!("127.0.0.1:{}", control_port))
        .await
        .unwrap();
    let host_req = "POST /api/v1/rooms/host HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Length: 0\r\n\r\n";
    host_client.write_all(host_req.as_bytes()).await.unwrap();
    let mut host_resp = vec![0u8; 1024];
    let n = host_client.read(&mut host_resp).await.unwrap();
    let host_str = String::from_utf8_lossy(&host_resp[..n]);
    let code_idx = host_str.find("\"room_code\":\"").unwrap() + 13;
    let room_code = &host_str[code_idx..code_idx + 6];

    // P1 receives NotifyGameSetup
    let p1_setup = read_blaze_packet(&mut p1_stream).await;
    assert_eq!(p1_setup.header.command, 0x0014);

    // 5. Sync Steam Lobby via POST /api/v1/steam/lobby-sync
    let mut sync_client = TcpStream::connect(format!("127.0.0.1:{}", control_port))
        .await
        .unwrap();
    let sync_body = format!("{{\"room_code\":\"{}\",\"steam_lobby_id\":\"109775241000000000\"}}", room_code);
    let sync_req = format!(
        "POST /api/v1/steam/lobby-sync HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Length: {}\r\n\r\n{}",
        sync_body.len(),
        sync_body
    );
    sync_client.write_all(sync_req.as_bytes()).await.unwrap();
    let mut sync_resp = vec![0u8; 1024];
    let n = sync_client.read(&mut sync_resp).await.unwrap();
    let sync_str = String::from_utf8_lossy(&sync_resp[..n]);
    assert!(sync_str.contains("\"status\":\"ok\""));
    assert_eq!(session_store.get_steam_lobby(room_code), Some(109775241000000000));

    // 6. Connect Player 2
    let mut p2_stream = TcpStream::connect(format!("127.0.0.1:{}", blaze_port))
        .await
        .unwrap();
    send_blaze_req(&mut p2_stream, 0x0009, 0x0007, 1, TdfStruct::new()).await;
    let _ = read_blaze_packet(&mut p2_stream).await;

    let mut p2_login = TdfStruct::new();
    p2_login.set("DSNM", TdfValue::String("InvitedFriend".to_string()));
    send_blaze_req(&mut p2_stream, 0x0001, 0x00E6, 2, p2_login).await;
    let _ = read_blaze_packet(&mut p2_stream).await;
    let _ = read_blaze_packet(&mut p2_stream).await; // NotifyUserAdded (0x7802)

    // 7. Player 2 accepts Steam invite via POST /api/v1/steam/invite-accepted
    let mut accept_client = TcpStream::connect(format!("127.0.0.1:{}", control_port))
        .await
        .unwrap();
    let accept_body = format!("{{\"room_code\":\"{}\"}}", room_code);
    let accept_req = format!(
        "POST /api/v1/steam/invite-accepted HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Length: {}\r\n\r\n{}",
        accept_body.len(),
        accept_body
    );
    accept_client.write_all(accept_req.as_bytes()).await.unwrap();
    let mut accept_resp = vec![0u8; 1024];
    let n = accept_client.read(&mut accept_resp).await.unwrap();
    let accept_str = String::from_utf8_lossy(&accept_resp[..n]);
    assert!(accept_str.contains("\"joined\":true"));

    // P2 receives NotifyGameSetup
    let p2_setup = read_blaze_packet(&mut p2_stream).await;
    assert_eq!(p2_setup.header.command, 0x0014);

    // P1 receives NotifyPlayerJoining
    let p1_joining = read_blaze_packet(&mut p1_stream).await;
    assert_eq!(p1_joining.header.command, 0x0015);
}
