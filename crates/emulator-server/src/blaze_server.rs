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
    let mut header_buf = [0u8; BlazeHeader::HEADER_SIZE];
    if let Err(e) = stream.read_exact(&mut header_buf).await {
        if e.kind() == std::io::ErrorKind::UnexpectedEof {
            return Ok(None);
        }
        return Err(e);
    }
    let header = BlazeHeader::from_bytes(&header_buf)?;

    let mut payload_buf = vec![0u8; header.length as usize];
    stream.read_exact(&mut payload_buf).await?;
    let payload = deserialize_tdf(&payload_buf)?;

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
