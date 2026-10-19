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

fn try_parse_packet(buffer: &mut Vec<u8>, is_16_byte: &mut bool) -> Option<BlazePacket> {
    if buffer.len() < 12 {
        return None;
    }

    // Modern 16-byte header format (FIFA 22 Frostbite 3):
    // [00 00 00 00] (length)
    // [00 00]       (error code / flags)
    // [00 XX]       (component)
    // [00 YY]       (command)
    // [ZZ 00]       (msg_type / flags)
    // [00 00 00 00] (seq_no LE)
    if *is_16_byte || (buffer.len() >= 16 && buffer[0..4] == [0, 0, 0, 0] && buffer[4] == 0 && buffer[5] == 0 && (buffer[6] == 0 || buffer[6] == 0x78)) {
        *is_16_byte = true;
        if buffer.len() < 16 {
            return None;
        }
        let length = u32::from_be_bytes([buffer[0], buffer[1], buffer[2], buffer[3]]) as usize;
        let total_packet_len = 16 + length;
        if buffer.len() < total_packet_len {
            return None;
        }

        let header_slice: [u8; 16] = buffer[0..16].try_into().unwrap();
        let header = match BlazeHeader::from_bytes_16(&header_slice) {
            Ok(h) => h,
            Err(_) => return None,
        };

        let payload_slice = &buffer[16..total_packet_len];
        let payload = if payload_slice.is_empty() {
            blaze_tdf::TdfStruct::new()
        } else {
            deserialize_tdf(payload_slice).unwrap_or_else(|_| blaze_tdf::TdfStruct::new())
        };

        buffer.drain(0..total_packet_len);
        tracing::info!(
            "[Blaze Server] Parsed 16-byte frame: Comp=0x{:04X}, Cmd=0x{:04X}, Seq={}, Len={}, MsgType={:?}",
            header.component,
            header.command,
            header.seq_no,
            header.length,
            header.msg_type
        );
        return Some(BlazePacket { header, payload });
    }

    // Legacy 12-byte header format
    let length = (((buffer[0] as u32) << 16) | ((buffer[1] as u32) << 8) | (buffer[2] as u32)) as usize;
    let total_packet_len = 12 + length;
    if buffer.len() < total_packet_len {
        return None;
    }

    let header_slice: [u8; 12] = buffer[0..12].try_into().unwrap();
    let header = match BlazeHeader::from_bytes(&header_slice) {
        Ok(h) => h,
        Err(_) => return None,
    };

    let payload_slice = &buffer[12..total_packet_len];
    let payload = if payload_slice.is_empty() {
        blaze_tdf::TdfStruct::new()
    } else {
        deserialize_tdf(payload_slice).unwrap_or_else(|_| blaze_tdf::TdfStruct::new())
    };

    buffer.drain(0..total_packet_len);
    tracing::info!(
        "[Blaze Server] Parsed 12-byte frame: Comp=0x{:04X}, Cmd=0x{:04X}, Seq={}, Len={}, MsgType={:?}",
        header.component,
        header.command,
        header.seq_no,
        header.length,
        header.msg_type
    );
    Some(BlazePacket { header, payload })
}

async fn process_packet(
    stream: &mut DualStream,
    router: &mut BlazeRouter,
    req_packet: &BlazePacket,
    tx: &tokio::sync::mpsc::UnboundedSender<BlazePacket>,
    registered_uid: &mut u64,
    is_16_byte: bool,
) -> std::io::Result<()> {
    if router.session.user_id != 0 {
        router.store.touch_session(router.session.user_id);
    }

    let (resp_packet, notif_opt) = router.handle_packet(req_packet)?;

    if router.session.user_id != 0 && router.session.user_id != *registered_uid {
        if *registered_uid != 0 {
            router.store.remove_notification_channel(*registered_uid);
        }
        *registered_uid = router.session.user_id;
        router.store.register_notification_channel(*registered_uid, tx.clone());
    }

    let resp_bytes = if is_16_byte {
        resp_packet.encode_16()?
    } else {
        resp_packet.encode()?
    };
    stream.write_all(&resp_bytes).await?;

    if let Some(notif) = notif_opt {
        let notif_bytes = if is_16_byte {
            notif.encode_16()?
        } else {
            notif.encode()?
        };
        stream.write_all(&notif_bytes).await?;
    }

    stream.flush().await?;
    Ok(())
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
    let mut is_16_byte = false;

    loop {
        // Drain any complete packet already present in buffer
        if let Some(req_packet) = try_parse_packet(&mut rx_buf, &mut is_16_byte) {
            process_packet(stream, router, &req_packet, &tx, &mut registered_uid, is_16_byte).await?;
            continue;
        }

        tokio::select! {
            // Full-duplex unsolicited async notification from server to client
            Some(notif_pkt) = rx.recv() => {
                let notif_bytes = if is_16_byte {
                    notif_pkt.encode_16()?
                } else {
                    notif_pkt.encode()?
                };
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
                        while let Some(req_packet) = try_parse_packet(&mut rx_buf, &mut is_16_byte) {
                            process_packet(stream, router, &req_packet, &tx, &mut registered_uid, is_16_byte).await?;
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
