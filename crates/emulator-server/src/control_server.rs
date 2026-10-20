//! # Control Channel HTTP Server (:42129)
//!
//! Provides lightweight JSON/HTTP REST endpoints for the in-game C++ minimal overlay,
//! facilitating Room Code hosting, joining, and real-time netcode health polling.

use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use services_blaze::{build_notify_game_setup, build_notify_game_state_change, build_notify_player_joining, GameRegistry, SessionStore};
use services_http_content::{parse_http_request, HttpResponse};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RoomMember {
    pub slot: usize,
    pub name: String,
    pub steam_id: u64,
    pub role: String, // "host" or "guest"
    pub ping_ms: u32,
    pub connected: bool,
}

#[derive(Debug, Clone)]
pub struct ControlRoom {
    pub code: String,
    pub members: Vec<RoomMember>,
}

#[derive(Debug, Default)]
pub struct RoomManager {
    active_room: RwLock<Option<String>>,
    rooms: RwLock<HashMap<String, ControlRoom>>,
}

impl RoomManager {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn host_room(&self, code: String, persona_name: String, steam_id: u64) -> ControlRoom {
        let member = RoomMember {
            slot: 0,
            name: persona_name,
            steam_id,
            role: "host".to_string(),
            ping_ms: 0,
            connected: true,
        };
        let room = ControlRoom {
            code: code.clone(),
            members: vec![member],
        };
        self.rooms.write().unwrap().insert(code.clone(), room.clone());
        *self.active_room.write().unwrap() = Some(code);
        room
    }

    pub fn join_room(
        &self,
        code: &str,
        persona_name: String,
        steam_id: u64,
    ) -> Result<(usize, ControlRoom), &'static str> {
        let mut rooms = self.rooms.write().unwrap();
        let room = rooms.entry(code.to_string()).or_insert_with(|| ControlRoom {
            code: code.to_string(),
            members: vec![RoomMember {
                slot: 0,
                name: "Host".to_string(),
                steam_id: 0,
                role: "host".to_string(),
                ping_ms: 0,
                connected: true,
            }],
        });

        // Check if player is already in room
        let existing = room.members.iter_mut().find(|m| {
            (steam_id != 0 && m.steam_id == steam_id)
                || (!persona_name.is_empty() && m.name == persona_name)
        });

        if let Some(existing_member) = existing {
            existing_member.connected = true;
            if !persona_name.is_empty() {
                existing_member.name = persona_name;
            }
            if steam_id != 0 {
                existing_member.steam_id = steam_id;
            }
            let slot = existing_member.slot;
            let room_clone = room.clone();
            *self.active_room.write().unwrap() = Some(code.to_string());
            return Ok((slot, room_clone));
        }

        // Find first free slot in 1..4 (slots 1, 2, 3 for guests)
        let used_slots: Vec<usize> = room.members.iter().map(|m| m.slot).collect();
        let mut free_slot = None;
        for s in 1..4 {
            if !used_slots.contains(&s) {
                free_slot = Some(s);
                break;
            }
        }

        let assigned_slot = free_slot.ok_or("Room lobby is full (max 4 players)")?;
        let new_member = RoomMember {
            slot: assigned_slot,
            name: if persona_name.is_empty() {
                format!("Player {}", assigned_slot + 1)
            } else {
                persona_name
            },
            steam_id,
            role: "guest".to_string(),
            ping_ms: 18,
            connected: true,
        };

        room.members.push(new_member);
        room.members.sort_by_key(|m| m.slot);
        let room_clone = room.clone();
        *self.active_room.write().unwrap() = Some(code.to_string());
        Ok((assigned_slot, room_clone))
    }

    pub fn get_active_room(&self) -> Option<ControlRoom> {
        let code_guard = self.active_room.read().unwrap();
        let code = code_guard.as_ref()?;
        self.rooms.read().unwrap().get(code).cloned()
    }

    pub fn update_steam_identity(&self, persona_name: &str, steam_id: u64) {
        let mut rooms = self.rooms.write().unwrap();
        for room in rooms.values_mut() {
            if let Some(host) = room.members.iter_mut().find(|m| m.slot == 0) {
                if host.steam_id == 0 || host.steam_id == steam_id {
                    host.name = persona_name.to_string();
                    host.steam_id = steam_id;
                }
            }
            for m in room.members.iter_mut() {
                if m.steam_id == steam_id {
                    m.name = persona_name.to_string();
                }
            }
        }
    }
}

/// Runs the Control API listener loop on port 42129.
pub async fn run_control_server(
    listener: TcpListener,
    store: Arc<SessionStore>,
    games: Arc<GameRegistry>,
) {
    let local_addr = listener.local_addr().ok();
    tracing::info!("[Control Server] REST API listening on HTTP {:?}", local_addr);

    let room_manager = Arc::new(RoomManager::new());

    while let Ok((mut stream, _peer_addr)) = listener.accept().await {
        let session_store = store.clone();
        let game_registry = games.clone();
        let room_mgr = room_manager.clone();

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

                    let (active_room_code, room_members) = match room_mgr.get_active_room() {
                        Some(room) => (room.code, room.members),
                        None => {
                            let mut players = session_store.list_active_sessions();
                            players.sort_by_key(|p| p.user_id);
                            let code = players
                                .first()
                                .and_then(|s| session_store.get_target_room_code(s.user_id))
                                .unwrap_or_default();
                            let mut mems = Vec::new();
                            for (idx, p) in players.iter().enumerate() {
                                mems.push(RoomMember {
                                    slot: idx,
                                    name: p.steam_name.clone().unwrap_or_else(|| p.persona_name.clone()),
                                    steam_id: p.steam_id.unwrap_or(0),
                                    role: if idx == 0 { "host".into() } else { "guest".into() },
                                    ping_ms: if idx == 0 { 0 } else { 18 },
                                    connected: true,
                                });
                            }
                            (code, mems)
                        }
                    };

                    let total_online = std::cmp::max(online_count, room_members.len());
                    let members_json = serde_json::to_string(&room_members).unwrap_or_else(|_| "[]".to_string());
                    let cipher_reuses = services_http_content::NetcodeHttpMetrics::global().cipher_reuse_count.load(std::sync::atomic::Ordering::Relaxed);

                    let json = format!(
                        r#"{{"status":"online","online":true,"players_online":{},"active_players":{},"active_games":{},"current_room":"{}","room_code":"{}","transport_mode":"P2P","ping_ms":18,"peer_ping_ms":18,"cipher_reuse_count":{},"members":{}}}"#,
                        total_online, total_online, games_count, active_room_code, active_room_code, cipher_reuses, members_json
                    );
                    HttpResponse::ok_json(&json)
                }

                ("GET", "/metrics") | ("GET", "/api/v1/metrics") => {
                    HttpResponse::ok_json(&services_http_content::NetcodeHttpMetrics::global().to_json())
                }

                ("POST", "/api/v1/rooms/host") => {
                    let body_str = String::from_utf8_lossy(&req.body);
                    let code = game_registry.generate_room_code();

                    let host_name = extract_json_string(&body_str, "persona_name")
                        .or_else(|| {
                            session_store
                                .get_registered_steam_profile()
                                .map(|p| p.persona_name)
                        })
                        .unwrap_or_else(|| "Host".to_string());
                    let host_steam_id = extract_json_u64(&body_str, "steam_id")
                        .or_else(|| {
                            session_store
                                .get_registered_steam_profile()
                                .map(|p| p.steam_id)
                        })
                        .unwrap_or(0);

                    let room = room_mgr.host_room(code.clone(), host_name, host_steam_id);

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
                    let members_json = serde_json::to_string(&room.members).unwrap_or_else(|_| "[]".to_string());
                    let json = format!(r#"{{"status":"ok","room_code":"{}","slot":0,"members":{}}}"#, code, members_json);
                    HttpResponse::ok_json(&json)
                }

                ("POST", "/api/v1/rooms/join") => {
                    let body_str = String::from_utf8_lossy(&req.body);
                    let room_code = extract_json_string(&body_str, "room_code")
                        .unwrap_or_else(|| "DEFAULT".to_string());

                    let guest_name = extract_json_string(&body_str, "persona_name")
                        .unwrap_or_else(|| "Guest".to_string());
                    let guest_steam_id = extract_json_u64(&body_str, "steam_id").unwrap_or(0);

                    let (slot, room) = match room_mgr.join_room(&room_code, guest_name.clone(), guest_steam_id) {
                        Ok(res) => res,
                        Err(e) => {
                            let err_json = format!(r#"{{"status":"error","joined":false,"message":"{}","room_code":"{}"}}"#, e, room_code);
                            let resp = HttpResponse::bad_request(&err_json);
                            let bytes = resp.to_bytes();
                            let _ = stream.write_all(&bytes).await;
                            let _ = stream.flush().await;
                            return;
                        }
                    };

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
                    } else {
                        // Even without a Blaze session yet, room membership is confirmed!
                        joined = true;
                    }

                    tracing::info!(
                        "[Control Server] Room join request for '{}' by '{}' -> Slot {} (Joined: {})",
                        room_code,
                        guest_name,
                        slot,
                        joined
                    );

                    let members_json = serde_json::to_string(&room.members).unwrap_or_else(|_| "[]".to_string());
                    let json = format!(
                        r#"{{"status":"ok","joined":{},"room_code":"{}","slot":{},"members":{}}}"#,
                        joined, room_code, slot, members_json
                    );
                    HttpResponse::ok_json(&json)
                }

                ("POST", "/api/v1/steam/identity") => {
                    match serde_json::from_slice::<services_blaze::SteamProfile>(&req.body) {
                        Ok(profile) => {
                            let steam_id = profile.steam_id;
                            let name = profile.persona_name.clone();
                            session_store.register_steam_identity(profile);
                            room_mgr.update_steam_identity(&name, steam_id);
                            let json = format!(
                                r#"{{"status":"ok","steam_id":{},"persona_name":"{}"}}"#,
                                steam_id, name
                            );
                            HttpResponse::ok_json(&json)
                        }
                        Err(e) => {
                            tracing::warn!("[Control Server] Invalid SteamProfile JSON: {}", e);
                            HttpResponse::bad_request(&format!(
                                r#"{{"status":"error","message":"{}"}}"#,
                                e
                            ))
                        }
                    }
                }

                ("POST", "/api/v1/steam/lobby-sync") => {
                    let body_str = String::from_utf8_lossy(&req.body);
                    let room_code = extract_json_string(&body_str, "room_code").unwrap_or_default();
                    let lobby_id = extract_json_u64(&body_str, "steam_lobby_id").unwrap_or(0);
                    session_store.bind_steam_lobby(&room_code, lobby_id);

                    let json = format!(
                        r#"{{"status":"ok","room_code":"{}","steam_lobby_id":{}}}"#,
                        room_code, lobby_id
                    );
                    HttpResponse::ok_json(&json)
                }

                ("POST", "/api/v1/steam/invite-accepted") => {
                    let body_str = String::from_utf8_lossy(&req.body);
                    let room_code = extract_json_string(&body_str, "room_code").unwrap_or_default();

                    let mut players = session_store.list_active_sessions();
                    players.sort_by_key(|p| p.user_id);
                    let guest_sess = players.last().cloned();

                    let mut joined = false;
                    if let Some(guest) = guest_sess {
                        session_store.set_target_room_code(guest.user_id, Some(room_code.clone()));

                        if let Some(game) = game_registry.find_by_room_code(&room_code) {
                            if let Ok(joined_game) = game_registry.join_game(game.game_id, &guest) {
                                joined = true;
                                if let Some(host) = session_store.get_session(joined_game.host_user_id) {
                                    if let Ok(setup_notif) =
                                        build_notify_game_setup(&joined_game, &host, Some(&guest))
                                    {
                                        session_store.send_notification(guest.user_id, setup_notif);
                                    }
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
                        "[Control Server] Steam invite accepted for room '{}' (Joined: {})",
                        room_code,
                        joined
                    );

                    let json = format!(
                        r#"{{"status":"ok","joined":{},"room_code":"{}"}}"#,
                        joined, room_code
                    );
                    HttpResponse::ok_json(&json)
                }

                ("POST", "/api/v1/game/kickoff") => {
                    let mut notified = 0;
                    let all_games = game_registry.list_all_games();
                    if all_games.is_empty() {
                        let players = session_store.list_active_sessions();
                        if let Some(player) = players.first() {
                            let game = game_registry.create_game(player, None);
                            let _ = game_registry.update_state(game.game_id, 131);
                            if let Ok(setup_notif) = build_notify_game_setup(&game, player, players.get(1)) {
                                session_store.send_notification(player.user_id, setup_notif.clone());
                                if let Some(p2) = players.get(1) {
                                    session_store.send_notification(p2.user_id, setup_notif);
                                }
                            }
                            if let Ok(state_notif) = build_notify_game_state_change(game.game_id, 131) {
                                for p in &players {
                                    session_store.send_notification(p.user_id, state_notif.clone());
                                    notified += 1;
                                }
                            }
                        }
                    } else {
                        for game in all_games {
                            let _ = game_registry.update_state(game.game_id, 131); // 131 = GAME_STATE_IN_GAME
                            if let Ok(notif) = build_notify_game_state_change(game.game_id, 131) {
                                session_store.send_notification(game.host_user_id, notif.clone());
                                notified += 1;
                                if let Some(guest_id) = game.guest_user_id {
                                    session_store.send_notification(guest_id, notif.clone());
                                    notified += 1;
                                }
                            }
                        }
                    }

                    tracing::info!("[Control Server] Kickoff triggered: sent state change to {} players", notified);
                    let json = format!(r#"{{"status":"ok","kickoff":true,"notified_players":{}}}"#, notified);
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

/// Helper function to extract an unsigned integer field from simple JSON (quoted or unquoted).
fn extract_json_u64(json: &str, field: &str) -> Option<u64> {
    let key = format!("\"{}\"", field);
    let key_pos = json.find(&key)?;
    let colon_pos = json[key_pos..].find(':')? + key_pos;
    let after_colon = json[colon_pos + 1..].trim_start().trim_start_matches('"');
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
