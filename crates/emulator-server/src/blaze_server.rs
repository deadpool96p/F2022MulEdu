//! Core Blaze Protocol TCP Server (:42128).

use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio_rustls::TlsAcceptor;
use blaze_core::framing::{detect_framing, try_parse_packet as try_parse_packet_for, Framing};
use blaze_core::BlazePacket;
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

/// Parses the next frame out of `buffer`.
///
/// The framing generation is latched from the first frame of the
/// connection: a client's first frame is always a request, and for a
/// request the two generations are mutually exclusive. See
/// `blaze_core::framing` for the derivation.
fn try_parse_packet(
    buffer: &mut Vec<u8>,
    framing: &mut Option<Framing>,
) -> Option<BlazePacket> {
    let framing = match *framing {
        Some(f) => f,
        None => {
            let detected = detect_framing(buffer)?;
            *framing = Some(detected);
            tracing::info!("[Blaze Server] Detected {:?} framing", detected);
            detected
        }
    };

    let packet = try_parse_packet_for(buffer, framing)?;

    tracing::info!(
        "[Blaze Server] Parsed {:?} frame: Comp=0x{:04X}, Cmd=0x{:04X}, Seq={}, Len={}, MsgType={:?}",
        framing,
        packet.header.component,
        packet.header.command,
        packet.header.seq_no,
        packet.header.length,
        packet.header.msg_type
    );
    Some(packet)
}

async fn process_packet(
    stream: &mut DualStream,
    router: &mut BlazeRouter,
    req_packet: &BlazePacket,
    tx: &tokio::sync::mpsc::UnboundedSender<BlazePacket>,
    registered_uid: &mut u64,
    framing: Framing,
) -> std::io::Result<()> {
    if router.session.user_id != 0 {
        router.store.touch_session(router.session.user_id);
    }

    let req_bytes = encode_for(req_packet, framing).unwrap_or_default();
    router.games.deadlock_guard.record_message("IN", req_packet.header.component, req_packet.header.command, &req_bytes);
    crate::blaze_tracer::record_trace("IN", req_packet.header.component, req_packet.header.command, &req_bytes);

    let (resp_packet, notif_opt) = router.handle_packet(req_packet)?;

    if router.session.user_id != 0 && router.session.user_id != *registered_uid {
        if *registered_uid != 0 {
            router.store.remove_notification_channel(*registered_uid);
        }
        *registered_uid = router.session.user_id;
        router.store.register_notification_channel(*registered_uid, tx.clone());
    }

    let resp_bytes = encode_for(&resp_packet, framing)?;
    router.games.deadlock_guard.record_message("OUT", resp_packet.header.component, resp_packet.header.command, &resp_bytes);
    crate::blaze_tracer::record_trace("OUT", resp_packet.header.component, resp_packet.header.command, &resp_bytes);

    tracing::info!(
        "[Blaze Server] Sending response for Comp=0x{:04X}, Cmd=0x{:04X}, Seq={} ({} bytes):\n{}",
        resp_packet.header.component,
        resp_packet.header.command,
        resp_packet.header.seq_no,
        resp_bytes.len(),
        blaze_core::hex_dump(&resp_bytes)
    );
    stream.write_all(&resp_bytes).await?;

    if let Some(notif) = notif_opt {
        let notif_bytes = encode_for(&notif, framing)?;
        router.games.deadlock_guard.record_message("OUT", notif.header.component, notif.header.command, &notif_bytes);
        crate::blaze_tracer::record_trace("OUT", notif.header.component, notif.header.command, &notif_bytes);

        tracing::info!(
            "[Blaze Server] Sending paired notification for Comp=0x{:04X}, Cmd=0x{:04X} ({} bytes):\n{}",
            notif.header.component,
            notif.header.command,
            notif_bytes.len(),
            blaze_core::hex_dump(&notif_bytes)
        );
        stream.write_all(&notif_bytes).await?;
    }

    stream.flush().await?;
    tracing::info!("[Blaze Server] Flushed response to client socket successfully");
    Ok(())
}

/// Encodes a packet in the framing generation the client is using.
fn encode_for(packet: &BlazePacket, framing: Framing) -> std::io::Result<Vec<u8>> {
    match framing {
        Framing::Fire2 => packet.encode_16(),
        Framing::Legacy => packet.encode(),
    }
}

async fn handle_blaze_session(
    stream: &mut DualStream,
    router: &mut BlazeRouter,
) -> std::io::Result<()> {
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<BlazePacket>();

    let mut registered_uid: u64 = 0;

    let timeout_duration = std::time::Duration::from_secs(45);
    let mut rx_buf = Vec::with_capacity(8192);
    let mut read_chunk = [0u8; 4096];
    // Latched from the client's first frame; see `try_parse_packet`.
    let mut framing: Option<Framing> = None;

    loop {
        // Drain any complete packet already present in buffer
        if let Some(req_packet) = try_parse_packet(&mut rx_buf, &mut framing) {
            let f = framing.expect("framing is latched once a frame parses");
            process_packet(stream, router, &req_packet, &tx, &mut registered_uid, f).await?;
            continue;
        }

        tokio::select! {
            // Full-duplex unsolicited async notification from server to client
            Some(notif_pkt) = rx.recv() => {
                let notif_bytes = encode_for(&notif_pkt, framing.unwrap_or(Framing::Legacy))?;
                router.games.deadlock_guard.record_message("OUT", notif_pkt.header.component, notif_pkt.header.command, &notif_bytes);
                crate::blaze_tracer::record_trace("OUT", notif_pkt.header.component, notif_pkt.header.command, &notif_bytes);
                stream.write_all(&notif_bytes).await?;
                stream.flush().await?;
                tracing::debug!(
                    "[Blaze Server] Pushed async notification 0x{:04X}:0x{:04X} to UID {}",
                    notif_pkt.header.component,
                    notif_pkt.header.command,
                    router.session.user_id
                );
            }
            // Read incoming bytes from client
            timeout_res = tokio::time::timeout(timeout_duration, stream.read(&mut read_chunk)) => {
                match timeout_res {
                    Ok(Ok(0)) => {
                        tracing::info!(
                            "[Blaze Server] Client UID {} closed connection (EOF)",
                            router.session.user_id
                        );
                        break;
                    }
                    Ok(Ok(n)) => {
                        tracing::info!(
                            "[Blaze Server] Wire data received ({} bytes):\n{}",
                            n,
                            blaze_core::hex_dump(&read_chunk[..n])
                        );
                        rx_buf.extend_from_slice(&read_chunk[..n]);

                        // Drain and process all packets available in rx_buf
                        while let Some(req_packet) = try_parse_packet(&mut rx_buf, &mut framing) {
                            let f = framing.expect("framing is latched once a frame parses");
                            process_packet(stream, router, &req_packet, &tx, &mut registered_uid, f).await?;
                        }
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
                }
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
