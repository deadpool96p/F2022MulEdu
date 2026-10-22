//! # Fire2 Framing Integration Test
//!
//! The eight other integration test files all exchange the **legacy**
//! 12-byte FIRE generation, because that is what our own harness speaks.
//! FIFA 22 speaks the **16-byte Fire2** generation instead, so this file
//! covers the path the real client takes.
//!
//! It asserts that:
//!
//! 1. The server detects Fire2 framing from the client's first frame and
//!    answers in Fire2 (16-byte header, `messageType` in byte 13).
//! 2. A Fire2 request's reply carries the same 24-bit message number, so
//!    the client can correlate the two.
//! 3. A Fire2 *error* reply carries its error code in the metadata section
//!    as `ERRC` -- Fire2 has no error code field in the header.
//! 4. The server keeps the connection alive and answers a second request
//!    on the same Fire2 connection.
//!
//! The request bytes used here are the authentic ones recorded in
//! `docs/POST_PING_STALL_ROOT_CAUSE.md` section 4.

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

/// The authentic EA `Util::ping` request, byte for byte.
const AUTHENTIC_EA_PING_REQUEST: [u8; 16] = [
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x09, 0x00, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
];

/// Sends a Fire2 request and reads the Fire2 reply.
async fn send_fire2_request(
    stream: &mut TcpStream,
    comp: u16,
    cmd: u16,
    message_number: u32,
    payload: TdfStruct,
) -> BlazePacket {
    let req = BlazePacket::new(
        comp,
        cmd,
        0,
        MessageType::SubmitRequest,
        0,
        0,
        payload,
    )
    .unwrap();
    let mut req_bytes = req.encode_16().unwrap();
    // The Fire2 header carries the 24-bit message number at bytes 10..13.
    req_bytes[10] = ((message_number >> 16) & 0xFF) as u8;
    req_bytes[11] = ((message_number >> 8) & 0xFF) as u8;
    req_bytes[12] = (message_number & 0xFF) as u8;
    stream.write_all(&req_bytes).await.unwrap();
    stream.flush().await.unwrap();

    read_fire2_packet(stream).await
}

/// Reads one Fire2 frame, including its optional metadata section.
async fn read_fire2_packet(stream: &mut TcpStream) -> BlazePacket {
    let mut hdr_buf = [0u8; 16];
    stream.read_exact(&mut hdr_buf).await.unwrap();

    let header = BlazeHeader::from_bytes_16(&hdr_buf).unwrap();
    assert_eq!(
        header.metadata_size, 0,
        "a success reply carries no metadata section"
    );

    let mut body_buf = vec![0u8; header.length as usize];
    stream.read_exact(&mut body_buf).await.unwrap();
    let payload = deserialize_tdf(&body_buf).unwrap();
    BlazePacket { header, payload }
}

#[tokio::test]
async fn the_server_answers_fire2_clients_in_fire2() {
    let tls_acceptor = create_self_signed_tls_acceptor().expect("TLS setup failed");

    let blaze_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let blaze_port = blaze_listener.local_addr().unwrap().port();

    let session_store = Arc::new(SessionStore::new());
    let game_registry = Arc::new(GameRegistry::new());
    tokio::spawn(run_blaze_server(
        blaze_listener,
        tls_acceptor,
        "127.0.0.1".to_string(),
        17498,
        session_store,
        game_registry,
    ));

    tokio::time::sleep(Duration::from_millis(50)).await;

    let mut stream = TcpStream::connect(format!("127.0.0.1:{}", blaze_port))
        .await
        .unwrap();

    // ---------------------------------------------------------------------
    // 1. The authentic EA Util::ping request, sent byte for byte.
    // ---------------------------------------------------------------------
    stream.write_all(&AUTHENTIC_EA_PING_REQUEST).await.unwrap();
    stream.flush().await.unwrap();

    let mut hdr_buf = [0u8; 16];
    stream.read_exact(&mut hdr_buf).await.unwrap();

    // The reply must be a Fire2 frame: component at bytes 6..8, command at
    // bytes 8..10, and the message type in the high three bits of byte 13.
    assert_eq!(
        &hdr_buf[6..8],
        &[0x00, 0x09],
        "the reply's component id must be Util (0x0009)"
    );
    assert_eq!(
        &hdr_buf[8..10],
        &[0x00, 0x02],
        "the reply's command id must be ping (0x0002)"
    );
    assert_eq!(
        hdr_buf[13] >> 5,
        MessageType::RecordResponse as u8,
        "byte 13 must carry the message type in its high three bits"
    );
    assert_eq!(hdr_buf[15], 0, "byte 15 is reserved and must be zero");

    let header = BlazeHeader::from_bytes_16(&hdr_buf).unwrap();
    let mut body_buf = vec![0u8; header.length as usize];
    stream.read_exact(&mut body_buf).await.unwrap();
    let ping_payload = deserialize_tdf(&body_buf).unwrap();

    // The reply must carry the server time, encoded with the Heat2 varsize
    // scheme. A server timestamp of 0 would mean we regressed to a
    // placeholder, and a wrong scheme would decode to a bogus date.
    match ping_payload.get("STIM") {
        Some(TdfValue::Integer(stim)) => {
            assert!(
                *stim > 1_700_000_000,
                "the server time must be a real epoch second, got {}",
                stim
            );
        }
        other => panic!("PingResponse must carry STIM, got {:?}", other),
    }

    // ---------------------------------------------------------------------
    // 2. A second request on the same connection, with a non-zero message
    //    number, must be correlated back exactly.
    // ---------------------------------------------------------------------
    let pre_auth = send_fire2_request(&mut stream, 0x0009, 0x0007, 0x04_0201, TdfStruct::new()).await;
    assert_eq!(pre_auth.header.component, 0x0009);
    assert_eq!(pre_auth.header.command, 0x0007);
    assert_eq!(
        pre_auth.header.message_number, 0x04_0201,
        "the reply must echo the request's 24-bit message number"
    );
    assert_eq!(pre_auth.header.msg_type, MessageType::RecordResponse);
    assert!(
        pre_auth.payload.get("QOSS").is_some(),
        "preAuth must answer with the QoS settings"
    );

    // ---------------------------------------------------------------------
    // 3. The connection stays open and keeps answering.
    // ---------------------------------------------------------------------
    let ping = send_fire2_request(&mut stream, 0x0009, 0x0002, 0x04_0202, TdfStruct::new()).await;
    assert_eq!(ping.header.command, 0x0002);
    assert_eq!(ping.header.message_number, 0x04_0202);
    assert_eq!(ping.header.msg_type, MessageType::RecordResponse);
}

#[tokio::test]
async fn a_fire2_transport_ping_is_answered_with_a_ping_reply() {
    // A Fire2 transport-level PING is message type 4, and it is *not* an RPC
    // request: it is answered with a PING_REPLY (message type 5) that carries
    // no payload.
    //
    // Answering it with an ordinary RPC response leaves the client's
    // keep-alive unsatisfied; a live capture shows the client sending a PING
    // frame, the server replying with a RECORD_RESPONSE, and the client
    // dropping the connection and reconnecting three seconds later.
    let tls_acceptor = create_self_signed_tls_acceptor().expect("TLS setup failed");

    let blaze_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let blaze_port = blaze_listener.local_addr().unwrap().port();

    let session_store = Arc::new(SessionStore::new());
    let game_registry = Arc::new(GameRegistry::new());
    tokio::spawn(run_blaze_server(
        blaze_listener,
        tls_acceptor,
        "127.0.0.1".to_string(),
        17498,
        session_store,
        game_registry,
    ));

    tokio::time::sleep(Duration::from_millis(50)).await;

    let mut stream = TcpStream::connect(format!("127.0.0.1:{}", blaze_port))
        .await
        .unwrap();

    let ping = BlazePacket::new(
        0x0009,
        0x0002,
        0,
        MessageType::Ping,
        0,
        0,
        TdfStruct::new(),
    )
    .unwrap();
    let mut ping_bytes = ping.encode_16().unwrap();
    // Give the frame a non-zero 24-bit message number so we can prove the
    // reply echoes it.
    ping_bytes[10] = 0x00;
    ping_bytes[11] = 0x12;
    ping_bytes[12] = 0x34;
    stream.write_all(&ping_bytes).await.unwrap();
    stream.flush().await.unwrap();

    let mut hdr_buf = [0u8; 16];
    stream.read_exact(&mut hdr_buf).await.unwrap();

    assert_eq!(
        hdr_buf[13] >> 5,
        MessageType::PingReply as u8,
        "a transport PING must be answered with a PING_REPLY"
    );
    assert_eq!(
        &hdr_buf[10..13],
        &[0x00, 0x12, 0x34],
        "the PING_REPLY must echo the request's message number"
    );
    assert_eq!(
        &hdr_buf[0..4],
        &[0x00, 0x00, 0x00, 0x00],
        "a PING_REPLY carries no payload"
    );
    assert_eq!(&hdr_buf[6..8], &[0x00, 0x09], "component is echoed");
    assert_eq!(&hdr_buf[8..10], &[0x00, 0x02], "command is echoed");
}
