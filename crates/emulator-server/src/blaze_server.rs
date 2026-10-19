//! Core Blaze Protocol TCP Server (:42128).

use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio_rustls::TlsAcceptor;
use blaze_core::frame::BlazeHeader;
use blaze_core::BlazePacket;
use blaze_tdf::deserialize_tdf;
use services_blaze::{BlazeRouter, GameRegistry, SessionStore};
use crate::listener::{accept_dual_stream, DualStream};

/// Runs the Core Blaze server listener loop.
pub async fn run_blaze_server(
    listener: TcpListener,
    tls_acceptor: TlsAcceptor,
    qos_host: String,
    qos_port: u16,
    store: Arc<SessionStore>,
    games: Arc<GameRegistry>,
) {
    while let Ok((tcp_stream, peer_addr)) = listener.accept().await {
        let acceptor = tls_acceptor.clone();
        let q_host = qos_host.clone();
        let session_store = store.clone();
        let game_registry = games.clone();

        tokio::spawn(async move {
            tracing::info!("[Blaze Server] New client connection from {}", peer_addr);
            match accept_dual_stream(tcp_stream, &acceptor).await {
                Ok(mut stream) => {
                    let mut router = BlazeRouter::new(q_host, qos_port, session_store, game_registry);
                    if let Err(e) = handle_blaze_session(&mut stream, &mut router).await {
                        tracing::debug!("[Blaze Server] Session closed for {}: {:?}", peer_addr, e);
                    }
                    // Clean up on disconnect
                    router.store.remove_notification_channel(router.session.user_id);
                }
                Err(e) => {
                    tracing::warn!("[Blaze Server] Stream accept error from {}: {:?}", peer_addr, e);
                }
            }
        });
    }
}

async fn read_blaze_packet(stream: &mut DualStream) -> std::io::Result<Option<BlazePacket>> {
    let mut raw_buf = [0u8; 4096];
    let n = stream.read(&mut raw_buf).await?;
    if n == 0 {
        return Ok(None);
    }

    tracing::info!(
        "[Blaze Server] Wire data received ({} bytes):\n{}",
        n,
        blaze_core::hex_dump(&raw_buf[..n])
    );

    // Check if packet matches the 12-byte modern Fire2 / Frostbite header format:
    // [00 00 00 00] (4-byte length = 0)
    // [00 00]       (2-byte error code / flags = 0)
    // [00 09]       (2-byte component = 0x0009 Util)
    // [00 02]       (2-byte command = 0x0002 Ping)
    // [00 00]       (2-byte seq = 0)
    let (header, payload_bytes) = if n >= 12 && raw_buf[0..4] == [0, 0, 0, 0] && raw_buf[6..8] == [0, 9] {
        let length = u32::from_be_bytes([raw_buf[0], raw_buf[1], raw_buf[2], raw_buf[3]]);
        let error_code = u16::from_be_bytes([raw_buf[4], raw_buf[5]]);
        let component = u16::from_be_bytes([raw_buf[6], raw_buf[7]]);
        let command = u16::from_be_bytes([raw_buf[8], raw_buf[9]]);
        let seq_no = u16::from_be_bytes([raw_buf[10], raw_buf[11]]);
        let header = BlazeHeader::new(
            length,
            component,
            command,
            error_code,
            blaze_core::MessageType::SubmitRequest,
            0,
            seq_no,
        );
        let payload = if n > 12 { &raw_buf[12..n] } else { &[] };
        (header, payload)
    } else if n >= BlazeHeader::HEADER_SIZE {
        let mut header_buf = [0u8; BlazeHeader::HEADER_SIZE];
        header_buf.copy_from_slice(&raw_buf[..BlazeHeader::HEADER_SIZE]);
        let header = BlazeHeader::from_bytes(&header_buf)?;
        let payload_end = (BlazeHeader::HEADER_SIZE + (header.length as usize)).min(n);
        let payload = &raw_buf[BlazeHeader::HEADER_SIZE..payload_end];
        (header, payload)
    } else {
        return Err(std::io::Error::new(
            std::io::ErrorKind::UnexpectedEof,
            format!("Received incomplete frame: {} bytes", n),
        ));
    };

    tracing::info!(
        "[Blaze Server] Header parsed: Comp=0x{:04X}, Cmd=0x{:04X}, Seq={}, Len={}, MsgType={:?}",
        header.component,
        header.command,
        header.seq_no,
        header.length,
        header.msg_type
    );

    let payload = if payload_bytes.is_empty() {
        blaze_tdf::TdfStruct::new()
    } else {
        match deserialize_tdf(payload_bytes) {
            Ok(p) => p,
            Err(e) => {
                tracing::warn!("[Blaze Server] TDF deserialize error (ignoring for request): {:?}", e);
                blaze_tdf::TdfStruct::new()
            }
        }
    };

    Ok(Some(BlazePacket { header, payload }))
}

async fn handle_blaze_session(
    stream: &mut DualStream,
    router: &mut BlazeRouter,
) -> std::io::Result<()> {
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<BlazePacket>();
    let mut registered_uid: u64 = 0;
    let timeout_duration = std::time::Duration::from_secs(45);

    loop {
        tokio::select! {
            // Full-duplex unsolicited async notification from server to client
            Some(notif_pkt) = rx.recv() => {
                let notif_bytes = notif_pkt.encode()?;
                stream.write_all(&notif_bytes).await?;
                stream.flush().await?;
                tracing::debug!(
                    "[Blaze Server] Pushed async notification 0x{:04X}:0x{:04X} to UID {}",
                    notif_pkt.header.component,
                    notif_pkt.header.command,
                    router.session.user_id
                );
            }
            // Client RPC request monitored by 45s keep-alive timeout
            timeout_res = tokio::time::timeout(timeout_duration, read_blaze_packet(stream)) => {
                let req_packet = match timeout_res {
                    Ok(Ok(Some(pkt))) => pkt,
                    Ok(Ok(None)) => {
                        tracing::info!(
                            "[Blaze Server] Client UID {} closed connection (EOF)",
                            router.session.user_id
                        );
                        break;
                    }
                    Ok(Err(e)) => {
                        tracing::warn!(
                            "[Blaze Server] Socket read error for UID {}: {}",
                            router.session.user_id, e
                        );
                        break;
                    }
                    Err(_) => {
                        tracing::warn!(
                            "[Blaze Server] Keep-alive ping timeout (45s) expired for UID {}",
                            router.session.user_id
                        );
                        break;
                    }
                };

                // Touch session to record keep-alive activity
                if router.session.user_id != 0 {
                    router.store.touch_session(router.session.user_id);
                }

                let (resp_packet, notif_opt) = router.handle_packet(&req_packet)?;

                // If user logged in and received a valid UID, register/update notification channel
                if router.session.user_id != 0 && router.session.user_id != registered_uid {
                    if registered_uid != 0 {
                        router.store.remove_notification_channel(registered_uid);
                    }
                    registered_uid = router.session.user_id;
                    router.store.register_notification_channel(registered_uid, tx.clone());
                }

                // Send RPC response
                let resp_bytes = resp_packet.encode()?;
                stream.write_all(&resp_bytes).await?;

                // Send immediate inline notification if generated
                if let Some(notif) = notif_opt {
                    let notif_bytes = notif.encode()?;
                    stream.write_all(&notif_bytes).await?;
                }

                stream.flush().await?;
            }
        }
    }

    // Disconnect cleanup: unregister channel, remove session, and cleanup active games
    let uid_to_clean = if registered_uid != 0 {
        registered_uid
    } else {
        router.session.user_id
    };

    if uid_to_clean != 0 {
        router.store.remove_notification_channel(uid_to_clean);
        router.store.remove_session(uid_to_clean);

        if let Some((removed_game, remaining_uids)) = router.games.handle_player_disconnect(uid_to_clean) {
            tracing::info!(
                "[Blaze Server] Cleaned up Game {} on disconnect of UID {}. Notifying {} peers.",
                removed_game.game_id,
                uid_to_clean,
                remaining_uids.len()
            );
            for peer_uid in remaining_uids {
                if let Ok(notif_player) = services_blaze::gamemanager::build_notify_player_removed(
                    removed_game.game_id,
                    router.session.persona_id,
                    4, // Reason: 4 = DISCONNECTED / PLAYER_LEFT
                ) {
                    let _ = router.store.send_notification(peer_uid, notif_player);
                }
                if let Ok(notif_game) = services_blaze::gamemanager::build_notify_game_removed(removed_game.game_id) {
                    let _ = router.store.send_notification(peer_uid, notif_game);
                }
                if let Ok(notif_game_alt) = services_blaze::gamemanager::build_notify_game_removed_alt(removed_game.game_id) {
                    let _ = router.store.send_notification(peer_uid, notif_game_alt);
                }
            }
        }
    }

    Ok(())
}
