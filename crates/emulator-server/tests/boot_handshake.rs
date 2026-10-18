//! Automated End-to-End Boot Handshake Integration Test.
//! Simulates a FIFA 22 client connecting to the Redirector, resolving the Blaze server,
//! and executing preAuth -> expressLogin -> postAuth -> ping.

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use blaze_core::frame::BlazeHeader;
use blaze_core::{BlazePacket, MessageType};
use blaze_tdf::{deserialize_tdf, TdfStruct, TdfValue};
use emulator_server::{
    create_self_signed_tls_acceptor, run_blaze_server, run_redirector_server,
};
use services_redirector::RedirectorConfig;

#[tokio::test]
async fn test_full_boot_handshake_flow() {
    let tls_acceptor = create_self_signed_tls_acceptor().expect("TLS setup failed");

    // Bind on ephemeral ports to avoid port collision during CI/test runs
    let redir_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let redir_port = redir_listener.local_addr().unwrap().port();

    let blaze_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let blaze_port = blaze_listener.local_addr().unwrap().port();

    let redir_cfg = RedirectorConfig {
        host: "127.0.0.1".to_string(),
        port: blaze_port,
        secure: false, // Plain TCP downgrade test
    };

    // Spawn server tasks
    tokio::spawn(run_redirector_server(
        redir_listener,
        tls_acceptor.clone(),
        redir_cfg,
    ));

    let session_store = std::sync::Arc::new(services_blaze::SessionStore::new());
    tokio::spawn(run_blaze_server(
        blaze_listener,
        tls_acceptor,
        "127.0.0.1".to_string(),
        17498,
        session_store,
    ));

    // Wait a brief moment for listeners to start
    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    // =========================================================================
    // STEP 1: Connect to Redirector and query getServerInstance
    // =========================================================================
    let mut redir_stream = TcpStream::connect(format!("127.0.0.1:{}", redir_port))
        .await
        .expect("Failed to connect to Redirector");

    let mut redir_req_tdf = TdfStruct::new();
    redir_req_tdf.set("NAME", TdfValue::String("fifa-2022-pc".to_string()));
    redir_req_tdf.set("CLNT", TdfValue::String("FIFA22".to_string()));

    let redir_req_packet = BlazePacket::new(
        0x0005, // Component: Redirector
        0x0001, // Command: getServerInstance
        0x0000,
        MessageType::SubmitRequest,
        0,
        1,
        redir_req_tdf,
    )
    .unwrap();

    let encoded_req = redir_req_packet.encode().unwrap();
    redir_stream.write_all(&encoded_req).await.unwrap();

    // Read Redirector Response
    let mut hdr_buf = [0u8; 12];
    redir_stream.read_exact(&mut hdr_buf).await.unwrap();
    let hdr = BlazeHeader::from_bytes(&hdr_buf).unwrap();

    let mut payload_buf = vec![0u8; hdr.length as usize];
    redir_stream.read_exact(&mut payload_buf).await.unwrap();
    let redir_resp_tdf = deserialize_tdf(&payload_buf).unwrap();

    assert_eq!(hdr.component, 0x0005);
    assert_eq!(hdr.command, 0x0001);
    assert_eq!(hdr.msg_type, MessageType::RecordResponse);
    assert_eq!(hdr.seq_no, 1);
    assert_eq!(redir_resp_tdf.get("SECU"), Some(&TdfValue::Integer(0)));

    drop(redir_stream); // Clean disconnect from redirector

    // =========================================================================
    // STEP 2: Connect to Core Blaze Server
    // =========================================================================
    let mut blaze_stream = TcpStream::connect(format!("127.0.0.1:{}", blaze_port))
        .await
        .expect("Failed to connect to Blaze server");

    // Helper closure to send request and read response
    async fn send_recv(
        stream: &mut TcpStream,
        comp: u16,
        cmd: u16,
        seq: u16,
        payload: TdfStruct,
    ) -> (BlazeHeader, TdfStruct) {
        let req = BlazePacket::new(comp, cmd, 0, MessageType::SubmitRequest, 0, seq, payload).unwrap();
        let bytes = req.encode().unwrap();
        stream.write_all(&bytes).await.unwrap();

        let mut h_buf = [0u8; 12];
        stream.read_exact(&mut h_buf).await.unwrap();
        let resp_hdr = BlazeHeader::from_bytes(&h_buf).unwrap();

        let mut p_buf = vec![0u8; resp_hdr.length as usize];
        stream.read_exact(&mut p_buf).await.unwrap();
        let resp_tdf = deserialize_tdf(&p_buf).unwrap();

        (resp_hdr, resp_tdf)
    }

    // 2a. preAuth (Util 0x0009 :: 0x000B)
    let (pre_hdr, pre_tdf) = send_recv(&mut blaze_stream, 0x0009, 0x000B, 10, TdfStruct::new()).await;
    assert_eq!(pre_hdr.component, 0x0009);
    assert_eq!(pre_hdr.command, 0x000B);
    assert_eq!(pre_hdr.seq_no, 10);
    assert!(pre_tdf.get("QOSS").is_some());
    assert!(pre_tdf.get("CIDS").is_some());

    // 2b. expressLogin (Auth 0x0001 :: 0x001C)
    let mut login_payload = TdfStruct::new();
    login_payload.set("AUTH", TdfValue::String("TOKEN_123".to_string()));
    let (login_hdr, login_tdf) = send_recv(&mut blaze_stream, 0x0001, 0x001C, 11, login_payload).await;
    assert_eq!(login_hdr.component, 0x0001);
    assert_eq!(login_hdr.command, 0x001C);
    assert_eq!(login_hdr.seq_no, 11);
    assert_eq!(
        login_tdf.get("DSNM"),
        Some(&TdfValue::String("F22_Player1".to_string()))
    );
    assert!(login_tdf.get("SKEY").is_some());

    // 2c. postAuth (Util 0x0009 :: 0x000C)
    let (post_hdr, post_tdf) = send_recv(&mut blaze_stream, 0x0009, 0x000C, 12, TdfStruct::new()).await;
    assert_eq!(post_hdr.component, 0x0009);
    assert_eq!(post_hdr.command, 0x000C);
    assert!(post_tdf.get("TELE").is_some());

    // 2d. ping (Util 0x0009 :: 0x0002)
    let (ping_hdr, ping_tdf) = send_recv(&mut blaze_stream, 0x0009, 0x0002, 13, TdfStruct::new()).await;
    assert_eq!(ping_hdr.component, 0x0009);
    assert_eq!(ping_hdr.command, 0x0002);
    assert!(ping_tdf.get("STIM").is_some());

    println!(">>> Full boot handshake integration test successfully verified!");
}
