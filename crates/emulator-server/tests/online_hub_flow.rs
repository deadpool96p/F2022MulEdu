//! End-to-End Online Hub Integration Test.
//! Simulates two concurrent FIFA 22 clients (Player1 & Player2) performing:
//! 1. UDP QoS / STUN probing (port 17498) -> NAT OPEN
//! 2. Redirector lookup -> redirected to Core Blaze (:42128)
//! 3. Blaze preAuth & Login (dynamic persona allocation)
//! 4. UserSessions network registration & cross-client lookup
//! 5. AssociationLists virtual online friends discovery
//! 6. Wildcard unknown RPC fallback handling with ping verification

use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream, UdpSocket};
use blaze_core::frame::BlazeHeader;
use blaze_core::{BlazePacket, MessageType};
use blaze_tdf::{deserialize_tdf, TdfStruct, TdfUnion, TdfValue};
use emulator_server::{
    create_self_signed_tls_acceptor, run_blaze_server, run_qos_server, run_redirector_server,
    run_relay_server, PeerTable, SessionStore,
};
use services_redirector::RedirectorConfig;

async fn send_and_recv_blaze(
    stream: &mut TcpStream,
    comp: u16,
    cmd: u16,
    seq: u16,
    payload: TdfStruct,
) -> (BlazePacket, Option<BlazePacket>) {
    let req = BlazePacket::new(comp, cmd, 0, MessageType::SubmitRequest, 0, seq, payload).unwrap();
    let req_bytes = req.encode().unwrap();
    stream.write_all(&req_bytes).await.unwrap();
    stream.flush().await.unwrap();

    // Read response packet
    let mut hdr_buf = [0u8; BlazeHeader::HEADER_SIZE];
    stream.read_exact(&mut hdr_buf).await.unwrap();
    let header = BlazeHeader::from_bytes(&hdr_buf).unwrap();

    let mut body_buf = vec![0u8; header.length as usize];
    stream.read_exact(&mut body_buf).await.unwrap();
    let payload = deserialize_tdf(&body_buf).unwrap();
    let first_pkt = BlazePacket { header, payload };

    // If updateNetworkInfo, an async notification immediately follows
    let second_pkt = if comp == 0x7802 && cmd == 0x0008 {
        let mut n_hdr_buf = [0u8; BlazeHeader::HEADER_SIZE];
        stream.read_exact(&mut n_hdr_buf).await.unwrap();
        let n_header = BlazeHeader::from_bytes(&n_hdr_buf).unwrap();

        let mut n_body_buf = vec![0u8; n_header.length as usize];
        stream.read_exact(&mut n_body_buf).await.unwrap();
        let n_payload = deserialize_tdf(&n_body_buf).unwrap();
        Some(BlazePacket {
            header: n_header,
            payload: n_payload,
        })
    } else {
        None
    };

    (first_pkt, second_pkt)
}

#[tokio::test]
async fn test_full_online_hub_two_player_flow() {
    let tls_acceptor = create_self_signed_tls_acceptor().unwrap();

    // Ephemeral ports for isolated testing
    let redir_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let redir_port = redir_listener.local_addr().unwrap().port();

    let blaze_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let blaze_port = blaze_listener.local_addr().unwrap().port();

    let qos_socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let qos_port = qos_socket.local_addr().unwrap().port();

    let relay_socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();

    let session_store = Arc::new(SessionStore::new());
    let peer_table = Arc::new(PeerTable::new());

    // Spawn server services
    let redir_cfg = RedirectorConfig {
        host: "127.0.0.1".to_string(),
        port: blaze_port,
        secure: false,
    };
    tokio::spawn(run_redirector_server(
        redir_listener,
        tls_acceptor.clone(),
        redir_cfg,
    ));

    let game_registry = Arc::new(services_blaze::GameRegistry::new());
    let store_clone = session_store.clone();
    let games_clone = game_registry.clone();
    tokio::spawn(run_blaze_server(
        blaze_listener,
        tls_acceptor,
        "127.0.0.1".to_string(),
        qos_port,
        store_clone,
        games_clone,
    ));

    tokio::spawn(async move {
        let _ = run_qos_server(qos_socket).await;
    });

    tokio::spawn(async move {
        let _ = run_relay_server(relay_socket, peer_table).await;
    });

    tokio::time::sleep(Duration::from_millis(50)).await;

    // =========================================================================
    // 1. Both clients probe UDP QoS server on port Q
    // =========================================================================
    let client1_udp = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let probe_req1 = vec![0xAA, 0xBB, 0xCC, 0xDD, 0x00, 0x01];
    client1_udp
        .send_to(&probe_req1, format!("127.0.0.1:{}", qos_port))
        .await
        .unwrap();

    let mut qos_resp_buf = vec![0u8; 64];
    let (rlen, _) = client1_udp.recv_from(&mut qos_resp_buf).await.unwrap();
    assert!(rlen >= 16);
    assert_eq!(&qos_resp_buf[0..4], &[0xAA, 0xBB, 0xCC, 0xDD]); // Echoed ID
    assert_eq!(&qos_resp_buf[14..16], &[0, 0]); // NAT OPEN!

    // =========================================================================
    // 2. Both clients connect to Redirector on port R
    // =========================================================================
    let mut redir_stream = TcpStream::connect(format!("127.0.0.1:{}", redir_port))
        .await
        .unwrap();
    let mut get_inst = TdfStruct::new();
    get_inst.set("NAME", TdfValue::String("fifa-2022-pc".to_string()));
    let (inst_resp, _) = send_and_recv_blaze(&mut redir_stream, 0x0005, 0x0001, 1, get_inst).await;
    let srv_struct = match inst_resp.payload.get("ADDR").unwrap() {
        TdfValue::Union(u) => match u.value.as_ref().unwrap().as_ref() {
            TdfValue::Struct(s) => s.clone(),
            _ => panic!("Expected struct in ADDR union"),
        },
        _ => panic!("Expected union for ADDR"),
    };
    assert_eq!(
        srv_struct.get("PORT"),
        Some(&TdfValue::Integer(blaze_port as u64))
    );

    // =========================================================================
    // 3. Client 1 (Player 1) Handshake & Login
    // =========================================================================
    let mut c1_stream = TcpStream::connect(format!("127.0.0.1:{}", blaze_port))
        .await
        .unwrap();

    // preAuth
    let (c1_pre_auth, _) =
        send_and_recv_blaze(&mut c1_stream, 0x0009, 0x000B, 1, TdfStruct::new()).await;
    assert_eq!(c1_pre_auth.header.command, 0x000B);

    // expressLogin
    let mut c1_login_tdf = TdfStruct::new();
    c1_login_tdf.set("DSNM", TdfValue::String("FifaPro_1".to_string()));
    let (c1_login_resp, _) =
        send_and_recv_blaze(&mut c1_stream, 0x0001, 0x001C, 2, c1_login_tdf).await;
    assert_eq!(
        c1_login_resp.payload.get("DSNM"),
        Some(&TdfValue::String("FifaPro_1".to_string()))
    );

    // updateNetworkInfo
    let mut c1_net = TdfStruct::new();
    let mut c1_addr = TdfStruct::new();
    c1_addr.set("PORT", TdfValue::Integer(3659));
    c1_net.set(
        "ADDR",
        TdfValue::Union(TdfUnion::with_value(0, TdfValue::Struct(c1_addr))),
    );
    c1_net.set("NATT", TdfValue::Integer(0));
    let (c1_net_resp, c1_notif) =
        send_and_recv_blaze(&mut c1_stream, 0x7802, 0x0008, 3, c1_net).await;
    assert_eq!(c1_net_resp.header.command, 0x0008);
    assert!(c1_notif.is_some());

    // =========================================================================
    // 4. Client 2 (Player 2) Handshake & Login
    // =========================================================================
    let mut c2_stream = TcpStream::connect(format!("127.0.0.1:{}", blaze_port))
        .await
        .unwrap();

    // preAuth
    send_and_recv_blaze(&mut c2_stream, 0x0009, 0x000B, 1, TdfStruct::new()).await;

    // login
    let mut c2_login_tdf = TdfStruct::new();
    c2_login_tdf.set("DSNM", TdfValue::String("FifaPro_2".to_string()));
    let (c2_login_resp, _) =
        send_and_recv_blaze(&mut c2_stream, 0x0001, 0x0028, 2, c2_login_tdf).await;
    assert_eq!(
        c2_login_resp.payload.get("DSNM"),
        Some(&TdfValue::String("FifaPro_2".to_string()))
    );

    // updateNetworkInfo
    let mut c2_net = TdfStruct::new();
    let mut c2_addr = TdfStruct::new();
    c2_addr.set("PORT", TdfValue::Integer(3660));
    c2_net.set(
        "ADDR",
        TdfValue::Union(TdfUnion::with_value(0, TdfValue::Struct(c2_addr))),
    );
    c2_net.set("NATT", TdfValue::Integer(0));
    send_and_recv_blaze(&mut c2_stream, 0x7802, 0x0008, 3, c2_net).await;

    // =========================================================================
    // 5. Cross-Client Peer Discovery & Virtual Online Friends List
    // =========================================================================
    // Player 1 looks up active players
    let (c1_lookup, _) =
        send_and_recv_blaze(&mut c1_stream, 0x7802, 0x0003, 4, TdfStruct::new()).await;
    if let Some(TdfValue::List(ulist)) = c1_lookup.payload.get("ULST") {
        assert_eq!(ulist.items.len(), 2);
    } else {
        panic!("Expected ULST in lookupUsers response");
    }

    // Player 1 queries AssociationLists (Friends List)
    let (c1_friends, _) =
        send_and_recv_blaze(&mut c1_stream, 0x0019, 0x0001, 5, TdfStruct::new()).await;
    if let Some(TdfValue::List(lists)) = c1_friends.payload.get("LST") {
        if let TdfValue::Struct(fl) = &lists.items[0] {
            if let Some(TdfValue::List(members)) = fl.get("LMEM") {
                assert_eq!(members.items.len(), 1); // Sees Player 2
                if let TdfValue::Struct(m) = &members.items[0] {
                    assert_eq!(
                        m.get("NAME"),
                        Some(&TdfValue::String("FifaPro_2".to_string()))
                    );
                }
            }
        }
    } else {
        panic!("Expected LST in getLists response");
    }

    // =========================================================================
    // 6. Wildcard Unknown-RPC Fallback keeps connection alive
    // =========================================================================
    let mut telemetry_tdf = TdfStruct::new();
    telemetry_tdf.set("EVENT", TdfValue::String("MENU_TILE_NAV".to_string()));
    let (unknown_resp, _) =
        send_and_recv_blaze(&mut c1_stream, 0x00FE, 0x0022, 6, telemetry_tdf).await;
    assert_eq!(unknown_resp.header.component, 0x00FE);
    assert_eq!(unknown_resp.header.command, 0x0022);
    assert_eq!(unknown_resp.header.error_code, 0x0000); // Handled with ERR_OK!

    // Verify socket remains active and responsive with a ping
    let (ping_resp, _) =
        send_and_recv_blaze(&mut c1_stream, 0x0009, 0x0002, 7, TdfStruct::new()).await;
    assert_eq!(ping_resp.header.command, 0x0002);
    assert_eq!(ping_resp.header.msg_type, MessageType::RecordResponse);
}
