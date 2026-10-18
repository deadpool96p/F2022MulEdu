//! # Control Channel HTTP Server (:42129)
//!
//! Provides lightweight JSON/HTTP REST endpoints for the in-game C++ minimal overlay,
//! facilitating Room Code hosting, joining, and real-time netcode health polling.

use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use services_blaze::{build_notify_game_setup, build_notify_player_joining, GameRegistry, SessionStore};
use services_http_content::{parse_http_request, HttpResponse};

/// Runs the Control API listener loop on port 42129.
pub async fn run_control_server(
    listener: TcpListener,
    store: Arc<SessionStore>,
    games: Arc<GameRegistry>,
) {
    let local_addr = listener.local_addr().ok();
    tracing::info!("[Control Server] REST API listening on HTTP {:?}", local_addr);

    while let Ok((mut stream, _peer_addr)) = listener.accept().await {
        let session_store = store.clone();
        let game_registry = games.clone();

        tokio::spawn(async move {
            let mut buf = vec![0u8; 4096];
            let len = match stream.read(&mut buf).await {
                Ok(n) if n > 0 => n,
                _ => return,
            };

            let req = match parse_http_request(&buf[..len]) {
                Some(r) => r,
                None => return,
            };

            let path = req.path.split('?').next().unwrap_or(&req.path);
            tracing::debug!("[Control Server] {} {}", req.method, path);

            let resp = match (req.method.as_str(), path) {
                ("GET", "/api/v1/status") => {
                    let online_count = session_store.count();
                    let games_count = session_store
                        .list_active_sessions()
                        .iter()
                        .filter_map(|s| game_registry.find_by_player(s.user_id))
                        .count();

                    let mut players = session_store.list_active_sessions();
                    players.sort_by_key(|p| p.user_id);
                    let active_room = players
                        .first()
                        .and_then(|s| session_store.get_target_room_code(s.user_id))
                        .unwrap_or_default();

                    let json = format!(
                        r#"{{"status":"online","players_online":{},"active_games":{},"current_room":"{}","transport_mode":"P2P","ping_ms":18}}"#,
                        online_count, games_count, active_room
                    );
                    HttpResponse::ok_json(&json)
                }

                ("POST", "/api/v1/rooms/host") => {
                    let body_str = String::from_utf8_lossy(&req.body);
                    let code = game_registry.generate_room_code();

                    let mut players = session_store.list_active_sessions();
                    players.sort_by_key(|p| p.user_id);
                    let host_sess = if let Some(uid) = extract_json_u64(&body_str, "user_id") {
                        session_store.get_session(uid)
                    } else {
                        players.first().cloned()
                    };

                    if let Some(host) = host_sess {
                        session_store.set_target_room_code(host.user_id, Some(code.clone()));
                        let game = game_registry.create_game(&host, Some(code.clone()));
                        if let Ok(setup_notif) = build_notify_game_setup(&game, &host, None) {
                            session_store.send_notification(host.user_id, setup_notif);
                        }
                    }

                    tracing::info!("[Control Server] Allocated Room Code '{}'", code);
                    let json = format!(r#"{{"status":"ok","room_code":"{}"}}"#, code);
                    HttpResponse::ok_json(&json)
                }

                ("POST", "/api/v1/rooms/join") => {
                    let body_str = String::from_utf8_lossy(&req.body);
                    let room_code = extract_json_string(&body_str, "room_code")
                        .unwrap_or_else(|| "DEFAULT".to_string());

                    let mut players = session_store.list_active_sessions();
                    players.sort_by_key(|p| p.user_id);
                    let guest_sess = if let Some(uid) = extract_json_u64(&body_str, "user_id") {
                        session_store.get_session(uid)
                    } else {
                        players.last().cloned()
                    };

                    let mut joined = false;
                    if let Some(guest) = guest_sess {
                        session_store.set_target_room_code(guest.user_id, Some(room_code.clone()));

                        if let Some(game) = game_registry.find_by_room_code(&room_code) {
                            if let Ok(joined_game) = game_registry.join_game(game.game_id, &guest) {
                                joined = true;
                                if let Some(host) = session_store.get_session(joined_game.host_user_id) {
                                    // Notify guest with setup
                                    if let Ok(setup_notif) =
                                        build_notify_game_setup(&joined_game, &host, Some(&guest))
                                    {
                                        session_store.send_notification(guest.user_id, setup_notif);
                                    }
                                    // Notify host with joiner info
                                    if let Ok(joining_notif) =
                                        build_notify_player_joining(&joined_game, &guest)
                                    {
                                        session_store.send_notification(host.user_id, joining_notif);
                                    }
                                }
                            }
                        }
                    }

                    tracing::info!(
                        "[Control Server] Room join request for '{}' (Joined: {})",
                        room_code,
                        joined
                    );

                    let json = format!(
                        r#"{{"status":"ok","joined":{},"room_code":"{}"}}"#,
                        joined, room_code
                    );
                    HttpResponse::ok_json(&json)
                }

                _ => HttpResponse::not_found(),
            };

            let bytes = resp.to_bytes();
            let _ = stream.write_all(&bytes).await;
            let _ = stream.flush().await;
        });
    }
}

/// Helper function to extract a string field from simple JSON without heavy dependencies.
fn extract_json_string(json: &str, field: &str) -> Option<String> {
    let key = format!("\"{}\"", field);
    let key_pos = json.find(&key)?;
    let colon_pos = json[key_pos..].find(':')? + key_pos;
    let quote_start = json[colon_pos..].find('"')? + colon_pos + 1;
    let quote_end = json[quote_start..].find('"')? + quote_start;
    Some(json[quote_start..quote_end].trim().to_string())
}

/// Helper function to extract an unsigned integer field from simple JSON.
fn extract_json_u64(json: &str, field: &str) -> Option<u64> {
    let key = format!("\"{}\"", field);
    let key_pos = json.find(&key)?;
    let colon_pos = json[key_pos..].find(':')? + key_pos;
    let after_colon = json[colon_pos + 1..].trim_start();
    let num_str: String = after_colon
        .chars()
        .take_while(|c| c.is_ascii_digit())
        .collect();
    num_str.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_json_string_and_u64() {
        let json = r#"{"room_code": "XY79K2", "user_id": 10001, "active": true}"#;
        assert_eq!(
            extract_json_string(json, "room_code"),
            Some("XY79K2".to_string())
        );
        assert_eq!(extract_json_u64(json, "user_id"), Some(10001));
    }
}
