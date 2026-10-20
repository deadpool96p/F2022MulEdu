//! # State-Machine Ordering & Deadlock Guard Integration Test
//!
//! Simulates two clients (Host & Guest) with configurable ready delays.
//! Asserts the strict notification sequence:
//! NotifyPlayerJoining (0x0015) ->
//! NotifyJoiningPlayerInitiateConnections (0x0017) ->
//! NotifyPlatformHostInitialized (0x0018) ->
//! NotifyGameReportingIdChange (0x0070) ->
//! GameState(130) ->
//! GameState(131)
//!
//! Validates that no deadlock occurs below the 5-second DeadlockGuard threshold.

use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

use blaze_core::frame::BlazeHeader;
use blaze_core::{BlazePacket, MessageType};
use blaze_tdf::{deserialize_tdf, TdfStruct, TdfValue};
use emulator_server::{create_self_signed_tls_acceptor, run_blaze_server, GameRegistry, SessionStore};
use services_blaze::gamemanager::{
    GAME_STATE_IN_GAME_131, GAME_STATE_PRE_GAME_130, NOTIFY_GAME_REPORTING_ID_CHANGE,
    NOTIFY_GAME_STATE_CHANGE, NOTIFY_JOINING_PLAYER_INITIATE_CONNECTIONS,
    NOTIFY_PLATFORM_HOST_INITIALIZED, NOTIFY_PLAYER_JOINING,
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

async fn run_ordering_scenario(ready_delay: Duration) {
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

    tokio::time::sleep(Duration::from_millis(40)).await;

    // 1. Connect Client 1 (Host)
    let mut p1_stream = TcpStream::connect(format!("127.0.0.1:{}", blaze_port)).await.unwrap();
    send_blaze_req(&mut p1_stream, 0x0009, 0x000B, 1, TdfStruct::new()).await; // preAuth
    let _ = read_blaze_packet(&mut p1_stream).await;

    let mut p1_login = TdfStruct::new();
    p1_login.set("DSNM", TdfValue::String("HostOrderingTester".to_string()));
    send_blaze_req(&mut p1_stream, 0x0001, 0x001C, 2, p1_login).await; // login
    let _ = read_blaze_packet(&mut p1_stream).await;
    let _ = read_blaze_packet(&mut p1_stream).await; // NotifyUserAdded

    // 2. Connect Client 2 (Guest)
    let mut p2_stream = TcpStream::connect(format!("127.0.0.1:{}", blaze_port)).await.unwrap();
    send_blaze_req(&mut p2_stream, 0x0009, 0x000B, 1, TdfStruct::new()).await; // preAuth
    let _ = read_blaze_packet(&mut p2_stream).await;

    let mut p2_login = TdfStruct::new();
    p2_login.set("DSNM", TdfValue::String("GuestOrderingTester".to_string()));
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

    // Configurable Delay between lobby join and pre-game signal
    tokio::time::sleep(ready_delay).await;

    // 5. Host signals Pre-Game readiness (130)
    let mut pre_game_req = TdfStruct::new();
    pre_game_req.set("STAT", TdfValue::Integer(GAME_STATE_PRE_GAME_130 as u64));
    send_blaze_req(&mut p1_stream, 0x0004, 0x0014, 4, pre_game_req.clone()).await;
    let _ = read_blaze_packet(&mut p1_stream).await; // Response
    let _ = read_blaze_packet(&mut p1_stream).await; // NotifyPlayerStateChange
    let _ = read_blaze_packet(&mut p2_stream).await; // P2 receives NotifyPlayerStateChange

    // Guest delays and then signals Pre-Game readiness (130)
    tokio::time::sleep(ready_delay).await;
    send_blaze_req(&mut p2_stream, 0x0004, 0x0014, 4, pre_game_req).await;
    let _ = read_blaze_packet(&mut p2_stream).await; // Response
    let _ = read_blaze_packet(&mut p2_stream).await; // NotifyPlayerStateChange
    let _ = read_blaze_packet(&mut p1_stream).await; // P1 receives NotifyPlayerStateChange

    // Verification of Strict Sequence on symmetric trigger:
    // P1 (Host) receives:
    // Step 1: NotifyPlayerJoining (0x0015)
    let p1_s1 = read_blaze_packet(&mut p1_stream).await;
    assert_eq!(p1_s1.header.command, NOTIFY_PLAYER_JOINING);

    // Step 2: NotifyJoiningPlayerInitiateConnections (0x0017)
    let p1_s2 = read_blaze_packet(&mut p1_stream).await;
    assert_eq!(p1_s2.header.command, NOTIFY_JOINING_PLAYER_INITIATE_CONNECTIONS);

    // Step 3: NotifyPlatformHostInitialized (0x0018)
    let p1_s3 = read_blaze_packet(&mut p1_stream).await;
    assert_eq!(p1_s3.header.command, NOTIFY_PLATFORM_HOST_INITIALIZED);

    // Step 4: NotifyGameReportingIdChange (0x0070)
    let p1_s4 = read_blaze_packet(&mut p1_stream).await;
    assert_eq!(p1_s4.header.command, NOTIFY_GAME_REPORTING_ID_CHANGE);

    // Step 5: GameState(130) (0x0064)
    let p1_s5 = read_blaze_packet(&mut p1_stream).await;
    assert_eq!(p1_s5.header.command, NOTIFY_GAME_STATE_CHANGE);
    assert_eq!(p1_s5.payload.get("GSTA"), Some(&TdfValue::Integer(130)));

    // 6. Configurable Delay before MatchReady (In-Game / Kickoff)
    tokio::time::sleep(ready_delay).await;

    // Both players signal MatchReady (131)
    let mut in_game_req = TdfStruct::new();
    in_game_req.set("STAT", TdfValue::Integer(GAME_STATE_IN_GAME_131 as u64));

    send_blaze_req(&mut p1_stream, 0x0004, 0x0014, 5, in_game_req.clone()).await;
    let _ = read_blaze_packet(&mut p1_stream).await; // Response
    let _ = read_blaze_packet(&mut p1_stream).await; // NotifyPlayerStateChange
    let _ = read_blaze_packet(&mut p2_stream).await; // P2 receives NotifyPlayerStateChange

    tokio::time::sleep(ready_delay).await;

    send_blaze_req(&mut p2_stream, 0x0004, 0x0014, 5, in_game_req).await;
    let _ = read_blaze_packet(&mut p2_stream).await; // Response
    let _ = read_blaze_packet(&mut p2_stream).await; // NotifyPlayerStateChange
    let _ = read_blaze_packet(&mut p1_stream).await; // P1 receives NotifyPlayerStateChange

    // Step 6: GameState(131) (0x0064) - Kickoff whistle!
    let p1_kickoff = read_blaze_packet(&mut p1_stream).await;
    assert_eq!(p1_kickoff.header.command, NOTIFY_GAME_STATE_CHANGE);
    assert_eq!(p1_kickoff.payload.get("GSTA"), Some(&TdfValue::Integer(131)));

    // Assert no deadlock was triggered (< 5s threshold)
    assert!(
        !game_registry.deadlock_guard.check_deadlock(1),
        "Deadlock guard must not trigger when players ready up below 5-second threshold"
    );
}

#[tokio::test]
async fn test_state_machine_ordering_instant_ready() {
    run_ordering_scenario(Duration::from_millis(10)).await;
}

#[tokio::test]
async fn test_state_machine_ordering_with_staggered_delay() {
    run_ordering_scenario(Duration::from_millis(150)).await;
}
