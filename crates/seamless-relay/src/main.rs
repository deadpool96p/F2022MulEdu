//! Main entry point for Seamless FIFA 22 Relay Server.

mod protocol;
mod session;

use clap::Parser;
use std::sync::Arc;
use tokio::net::UdpSocket;
use tracing::{error, info, warn};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

use protocol::{hash_password, MessageType, SeamlessPacket};
use session::RoomManager;

#[derive(Parser, Debug)]
#[command(name = "seamless-relay", about = "Seamless FIFA 22 UDP Relay Server")]
struct Args {
    /// Port to listen on for UDP game traffic
    #[arg(short, long, default_value_t = 27015)]
    port: u16,

    /// Bind address
    #[arg(short, long, default_value = "0.0.0.0")]
    bind: String,

    /// Optional session password for diagnostics/verification
    #[arg(long)]
    password: Option<String>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    tracing_subscriber::registry()
        .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .with(tracing_subscriber::fmt::layer())
        .init();

    let args = Args::parse();
    let bind_addr = format!("{}:{}", args.bind, args.port);
    let socket = Arc::new(UdpSocket::bind(&bind_addr).await?);

    info!("============================================================");
    info!("   FIFA 22 Seamless Co-op Relay Server");
    info!("============================================================");
    info!(" * UDP Listening on: {}", bind_addr);

    if let Some(ref pass) = args.password {
        let hash = hash_password(pass);
        info!(" * Diagnostic Session Password: '{}'", pass);
        info!(" * Pre-computed Room Hash: {:02x?}", &hash[..8]);
    }
    info!("============================================================");

    let room_manager = Arc::new(RoomManager::new());

    // Spawn stale room cleanup task
    let cleaner_rooms = Arc::clone(&room_manager);
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(tokio::time::Duration::from_secs(30)).await;
            let pruned = cleaner_rooms.remove_stale(60);
            if pruned > 0 {
                info!("[Cleaner] Pruned {} inactive rooms", pruned);
            }
        }
    });

    let mut recv_buf = [0u8; 2048];
    let mut resp_buf = Vec::with_capacity(2048);

    loop {
        let (len, sender_addr) = match socket.recv_from(&mut recv_buf).await {
            Ok(res) => res,
            Err(e) => {
                error!("[Socket] Error receiving UDP packet: {}", e);
                continue;
            }
        };

        let raw_data = &recv_buf[..len];
        let packet = match SeamlessPacket::parse(raw_data) {
            Ok(pkt) => pkt,
            Err(e) => {
                warn!("[Relay] Dropped malformed packet from {}: {}", sender_addr, e);
                continue;
            }
        };

        match packet.msg_type {
            MessageType::HandshakeInit => {
                let mut room = room_manager.get_or_create(packet.room_hash);
                let assigned_peer = room.register_peer(sender_addr, Some(packet.peer_id));
                info!(
                    "[Room {:02x?}] Peer {} registered from {}",
                    &packet.room_hash[..4],
                    assigned_peer,
                    sender_addr
                );

                // Send HandshakeAck to client
                let status_byte = if room.host.is_some() && room.guest.is_some() {
                    0x02 // Connected / Ready
                } else {
                    0x01 // Waiting for peer
                };

                SeamlessPacket::serialize(
                    MessageType::HandshakeAck,
                    assigned_peer,
                    0,
                    &packet.room_hash,
                    packet.seq_num + 1,
                    &[status_byte],
                    &mut resp_buf,
                );

                let _ = socket.send_to(&resp_buf, sender_addr).await;

                // If both peers are now present, notify the other peer as well
                if status_byte == 0x02 {
                    if let Some((target_addr, _)) = room.get_target_for(sender_addr) {
                        SeamlessPacket::serialize(
                            MessageType::PeerStatusNotify,
                            assigned_peer,
                            0,
                            &packet.room_hash,
                            0,
                            &[0x02],
                            &mut resp_buf,
                        );
                        let _ = socket.send_to(&resp_buf, target_addr).await;
                        info!(
                            "[Room {:02x?}] Both peers connected! Notified {}",
                            &packet.room_hash[..4],
                            target_addr
                        );
                    }
                }
            }

            MessageType::GameData => {
                let mut room = room_manager.get_or_create(packet.room_hash);
                if let Some((target_addr, target_peer)) = room.get_target_for(sender_addr) {
                    // Forward game data packet to the opposing peer
                    SeamlessPacket::serialize(
                        MessageType::GameData,
                        packet.peer_id,
                        packet.flags,
                        &packet.room_hash,
                        packet.seq_num,
                        packet.payload,
                        &mut resp_buf,
                    );

                    if let Err(e) = socket.send_to(&resp_buf, target_addr).await {
                        error!(
                            "[Relay] Failed to forward packet to peer {} ({}): {}",
                            target_peer, target_addr, e
                        );
                    }
                }
            }

            MessageType::HeartbeatPing => {
                SeamlessPacket::serialize(
                    MessageType::HeartbeatPong,
                    packet.peer_id,
                    0,
                    &packet.room_hash,
                    packet.seq_num + 1,
                    packet.payload,
                    &mut resp_buf,
                );
                let _ = socket.send_to(&resp_buf, sender_addr).await;
            }

            MessageType::Disconnect => {
                let mut room = room_manager.get_or_create(packet.room_hash);
                info!(
                    "[Room {:02x?}] Peer {} disconnected ({})",
                    &packet.room_hash[..4],
                    packet.peer_id,
                    sender_addr
                );
                if let Some((target_addr, _)) = room.get_target_for(sender_addr) {
                    SeamlessPacket::serialize(
                        MessageType::PeerStatusNotify,
                        packet.peer_id,
                        0,
                        &packet.room_hash,
                        0,
                        &[0x00], // Disconnected
                        &mut resp_buf,
                    );
                    let _ = socket.send_to(&resp_buf, target_addr).await;
                }
            }

            _ => {}
        }
    }
}
