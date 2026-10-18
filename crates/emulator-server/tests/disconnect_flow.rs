//! # Disconnection Detection & Session Recovery Integration Test
//!
//! Validates Milestone 6 Step 2:
//! 1. Connects two players (Host & Guest) and forms an active IN_GAME match session.
//! 2. Abruptly drops the Host's connection (simulating network cut / process crash).
//! 3. Verifies the server detects the drop, cleans up the GameSession from GameRegistry,
//!    removes the player session from SessionStore, and broadcasts NotifyPlayerRemoved
//!    and NotifyGameRemoved to the remaining client so it returns safely to the main menu.

use std::sync::Arc;
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

use blaze_core::frame::BlazeHeader;
use blaze_core::{BlazePacket, MessageType};
use blaze_tdf::{deserialize_tdf, TdfStruct, TdfValue};
use emulator_server::{
    create_self_signed_tls_acceptor, run_blaze_server, GameRegistry, SessionStore,
};
use services_blaze::gamemanager::{
    CMD_CREATE_GAME, CMD_JOIN_GAME, CMD_UPDATE_PLAYER_STATE, COMPONENT_GAME_MANAGER,
    GAME_STATE_IN_GAME, NOTIFY_GAME_REMOVED, NOTIFY_GAME_REMOVED_ALT, NOTIFY_PLAYER_REMOVED,
};
use services_blaze::util::{CMD_PRE_AUTH, COMPONENT_UTIL};

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
async fn test_abrupt_disconnect_recovery_and_peer_notification() {
    let tls_acceptor = create_self_signed_tls_acceptor().unwrap();

    // 1. Bind Dynamic Blaze TCP Port
    let blaze_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let blaze_port = blaze_listener.local_addr().unwrap().port();

    let session_store = Arc::new(SessionStore::new());
    let game_registry = Arc::new(GameRegistry::new());

    // 2. Launch Blaze Server
    {
        let store = session_store.clone();
        let games = game_registry.clone();
        tokio::spawn(async move {
            run_blaze_server(
                blaze_listener,
                tls_acceptor,
                "127.0.0.1".to_string(),
                17498,
                store,
                games,
            )
            .await;
        });
    }

    tokio::time::sleep(Duration::from_millis(50)).await;

    // 3. Connect Player 1 (Host)
    let mut p1_stream = TcpStream::connect(format!("127.0.0.1:{}", blaze_port)).await.unwrap();

    // Handshake
    send_blaze_req(&mut p1_stream, COMPONENT_UTIL, CMD_PRE_AUTH, 1, TdfStruct::new()).await;
    let _ = read_blaze_packet(&mut p1_stream).await;

    // Login Host
    let mut auth_req = TdfStruct::new();
    auth_req.set("MAIL", TdfValue::String("host_player@fifa22.com".to_string()));
    send_blaze_req(&mut p1_stream, 0x0001, 0x0028, 2, auth_req).await;
    let p1_auth_resp = read_blaze_packet(&mut p1_stream).await;
    let p1_uid = match p1_auth_resp.payload.get("UID") {
        Some(TdfValue::Integer(uid)) => *uid,
        _ => panic!("Expected UID in login response"),
    };

    // Create Game
    let mut create_req = TdfStruct::new();
    create_req.set("GNAM", TdfValue::String("Host Match".to_string()));
    send_blaze_req(&mut p1_stream, COMPONENT_GAME_MANAGER, CMD_CREATE_GAME, 3, create_req).await;
    let p1_create_resp = read_blaze_packet(&mut p1_stream).await;
    let game_id = match p1_create_resp.payload.get("GID") {
        Some(TdfValue::Integer(gid)) => *gid,
        _ => panic!("Expected GID in createGame response"),
    };
    let _notif_created = read_blaze_packet(&mut p1_stream).await; // 0x000F

    // 4. Connect Player 2 (Guest)
    let mut p2_stream = TcpStream::connect(format!("127.0.0.1:{}", blaze_port)).await.unwrap();

    // Handshake
    send_blaze_req(&mut p2_stream, COMPONENT_UTIL, CMD_PRE_AUTH, 10, TdfStruct::new()).await;
    let _ = read_blaze_packet(&mut p2_stream).await;

    // Login Guest
    let mut auth_req2 = TdfStruct::new();
    auth_req2.set("MAIL", TdfValue::String("guest_player@fifa22.com".to_string()));
    send_blaze_req(&mut p2_stream, 0x0001, 0x0028, 11, auth_req2).await;
    let p2_auth_resp = read_blaze_packet(&mut p2_stream).await;
    let p2_uid = match p2_auth_resp.payload.get("UID") {
        Some(TdfValue::Integer(uid)) => *uid,
        _ => panic!("Expected UID in login response"),
    };

    // Join Game
    let mut join_req = TdfStruct::new();
    join_req.set("GID", TdfValue::Integer(game_id));
    send_blaze_req(&mut p2_stream, COMPONENT_GAME_MANAGER, CMD_JOIN_GAME, 12, join_req).await;
    let _p2_join_resp = read_blaze_packet(&mut p2_stream).await;
    let _p2_notif_setup = read_blaze_packet(&mut p2_stream).await;

    // Player 1 receives NotifyPlayerJoining
    let _p1_notif_joining = read_blaze_packet(&mut p1_stream).await;

    // 5. Transition to IN_GAME
    let mut state_req = TdfStruct::new();
    state_req.set("STAT", TdfValue::Integer(GAME_STATE_IN_GAME as u64));

    // Player 1 signals IN_GAME
    send_blaze_req(&mut p1_stream, COMPONENT_GAME_MANAGER, CMD_UPDATE_PLAYER_STATE, 4, state_req.clone()).await;
    let _ = read_blaze_packet(&mut p1_stream).await; // Response
    let _ = read_blaze_packet(&mut p1_stream).await; // NotifyPlayerStateChange
    let _ = read_blaze_packet(&mut p2_stream).await; // P2 receives NotifyPlayerStateChange

    // Player 2 signals IN_GAME
    send_blaze_req(&mut p2_stream, COMPONENT_GAME_MANAGER, CMD_UPDATE_PLAYER_STATE, 13, state_req).await;
    let _ = read_blaze_packet(&mut p2_stream).await; // Response
    let _ = read_blaze_packet(&mut p2_stream).await; // NotifyPlayerStateChange
    let _ = read_blaze_packet(&mut p1_stream).await; // P1 receives NotifyPlayerStateChange

    let _p1_state_change = read_blaze_packet(&mut p1_stream).await; // NotifyGameStateChange (0x0064)
    let _p2_state_change = read_blaze_packet(&mut p2_stream).await; // NotifyGameStateChange (0x0064)

    // Verify session is active in registry
    assert!(game_registry.find_by_player(p1_uid).is_some());
    assert!(game_registry.find_by_player(p2_uid).is_some());
    assert_eq!(game_registry.get_game(game_id).unwrap().game_state, GAME_STATE_IN_GAME);

    // 6. Abruptly drop Player 1 (Host) connection
    drop(p1_stream);

    // 7. Player 2 (Guest) should receive NotifyPlayerRemoved and/or NotifyGameRemoved
    let mut received_player_removed = false;
    let mut received_game_removed = false;

    // Read up to 3 notifications pushed to Player 2
    for _ in 0..3 {
        let pkt = tokio::time::timeout(Duration::from_millis(1500), read_blaze_packet(&mut p2_stream)).await;
        if let Ok(pkt) = pkt {
            if pkt.header.component == COMPONENT_GAME_MANAGER {
                if pkt.header.command == NOTIFY_PLAYER_REMOVED {
                    received_player_removed = true;
                } else if pkt.header.command == NOTIFY_GAME_REMOVED
                    || pkt.header.command == NOTIFY_GAME_REMOVED_ALT
                {
                    received_game_removed = true;
                }
            }
        }
    }

    assert!(
        received_player_removed,
        "Remaining player must receive NotifyPlayerRemoved (0x0020) on peer disconnect"
    );
    assert!(
        received_game_removed,
        "Remaining player must receive NotifyGameRemoved (0x000A / 0x0010) on match cleanup"
    );

    // 8. Verify Registry and SessionStore clean state
    assert!(
        game_registry.find_by_player(p1_uid).is_none(),
        "Host must be cleared from GameRegistry"
    );
    assert!(
        game_registry.find_by_player(p2_uid).is_none(),
        "Guest must be cleared from GameRegistry"
    );
    assert!(
        game_registry.get_game(game_id).is_none(),
        "GameSession must be completely deallocated"
    );
    assert!(
        session_store.get_session(p1_uid).is_none(),
        "Disconnected host session must be removed from SessionStore"
    );
}
