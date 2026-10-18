//! # Concurrent Player Session Store
//!
//! Maintains active player sessions (User IDs, Persona IDs, names, network addresses,
//! target room codes, Steamworks identity, and async notification channels) across all connected Blaze sessions.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;
use parking_lot::RwLock;
use serde::{Deserialize, Deserializer, Serialize};
use tokio::sync::mpsc::UnboundedSender;
use blaze_core::BlazePacket;

/// Deserializes a u64 from either a JSON integer or a JSON string.
pub fn deserialize_u64_lenient<'de, D>(deserializer: D) -> Result<u64, D::Error>
where
    D: Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum U64OrString {
        Int(u64),
        Str(String),
    }

    match U64OrString::deserialize(deserializer)? {
        U64OrString::Int(n) => Ok(n),
        U64OrString::Str(s) => s.trim().parse::<u64>().map_err(serde::de::Error::custom),
    }
}

/// A synced Steam friend entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SteamFriend {
    #[serde(deserialize_with = "deserialize_u64_lenient")]
    pub steam_id: u64,
    pub name: String,
}

/// Synced local Steam profile from the in-game shim.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SteamProfile {
    #[serde(deserialize_with = "deserialize_u64_lenient")]
    pub steam_id: u64,
    pub persona_name: String,
    #[serde(default)]
    pub friends: Vec<SteamFriend>,
}

/// Active player session information.
#[derive(Debug, Clone)]
pub struct SessionData {
    pub user_id: u64,
    pub persona_id: u64,
    pub persona_name: String,
    pub steam_id: Option<u64>,
    pub steam_name: Option<String>,
    pub online_friends: Vec<(u64, String)>,
    pub session_key: String,
    pub external_ip: String,
    pub internal_ip: String,
    pub port: u16,
    pub nat_type: u8,
    pub target_room_code: Option<String>,
    pub steam_lobby_id: Option<u64>,
    pub last_seen: Instant,
}

/// Global thread-safe session registry.
pub struct SessionStore {
    next_id: AtomicU64,
    sessions: RwLock<HashMap<u64, SessionData>>,
    persona_map: RwLock<HashMap<String, u64>>,
    notif_channels: RwLock<HashMap<u64, UnboundedSender<BlazePacket>>>,
    pending_steam_profile: RwLock<Option<SteamProfile>>,
    steam_lobby_map: RwLock<HashMap<String, u64>>,
}

impl Default for SessionStore {
    fn default() -> Self {
        Self::new()
    }
}

impl SessionStore {
    pub fn new() -> Self {
        Self {
            next_id: AtomicU64::new(10001),
            sessions: RwLock::new(HashMap::new()),
            persona_map: RwLock::new(HashMap::new()),
            notif_channels: RwLock::new(HashMap::new()),
            pending_steam_profile: RwLock::new(None),
            steam_lobby_map: RwLock::new(HashMap::new()),
        }
    }

    /// Registers or updates a local Steam identity received via the control channel.
    pub fn register_steam_identity(&self, profile: SteamProfile) {
        tracing::info!(
            "[SessionStore] Registered Steam identity: '{}' (SteamID={}) with {} friends",
            profile.persona_name,
            profile.steam_id,
            profile.friends.len()
        );

        // Update any already-active session(s)
        let mut sessions = self.sessions.write();
        for sess in sessions.values_mut() {
            sess.steam_id = Some(profile.steam_id);
            sess.steam_name = Some(profile.persona_name.clone());
            sess.online_friends = profile
                .friends
                .iter()
                .map(|f| (f.steam_id, f.name.clone()))
                .collect();
            sess.persona_name = profile.persona_name.clone();
        }

        *self.pending_steam_profile.write() = Some(profile);
    }

    /// Retrieves the registered Steam profile if present.
    pub fn get_registered_steam_profile(&self) -> Option<SteamProfile> {
        self.pending_steam_profile.read().clone()
    }

    /// Binds a Steam Lobby ID to an active Room Code.
    pub fn bind_steam_lobby(&self, room_code: &str, steam_lobby_id: u64) {
        self.steam_lobby_map
            .write()
            .insert(room_code.to_uppercase(), steam_lobby_id);
        tracing::info!(
            "[SessionStore] Bound Steam Lobby ID {} to Room Code '{}'",
            steam_lobby_id,
            room_code
        );
    }

    /// Gets the Steam Lobby ID associated with a Room Code.
    pub fn get_steam_lobby(&self, room_code: &str) -> Option<u64> {
        self.steam_lobby_map
            .read()
            .get(&room_code.to_uppercase())
            .copied()
    }

    /// Allocates and registers a new active session for a player.
    pub fn create_session(&self, requested_name: Option<&str>) -> SessionData {
        let user_id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let steam_profile = self.get_registered_steam_profile();

        let (persona_id, persona_name, steam_id, steam_name, online_friends) =
            if let Some(ref prof) = steam_profile {
                // Map SteamID directly into Persona ID bit-space (positive 64-bit integer)
                let pid = (prof.steam_id & 0x7FFF_FFFF).max(user_id);
                let name = match requested_name {
                    Some(n) if !n.trim().is_empty() => n.trim().to_string(),
                    _ => prof.persona_name.clone(),
                };
                let friends = prof
                    .friends
                    .iter()
                    .map(|f| (f.steam_id, f.name.clone()))
                    .collect();
                (pid, name, Some(prof.steam_id), Some(prof.persona_name.clone()), friends)
            } else {
                let name = match requested_name {
                    Some(n) if !n.trim().is_empty() => n.trim().to_string(),
                    _ => {
                        let player_idx = user_id - 10000;
                        format!("F22_Player{}", player_idx)
                    }
                };
                (user_id, name, None, None, Vec::new())
            };

        let session_key = format!("F22_SESS_{:08X}_{:04X}", user_id, user_id ^ 0xBEEF);

        let data = SessionData {
            user_id,
            persona_id,
            persona_name: persona_name.clone(),
            steam_id,
            steam_name,
            online_friends,
            session_key,
            external_ip: "127.0.0.1".to_string(),
            internal_ip: "127.0.0.1".to_string(),
            port: 3659,
            nat_type: 0, // 0 = OPEN
            target_room_code: None,
            steam_lobby_id: None,
            last_seen: Instant::now(),
        };

        self.sessions.write().insert(user_id, data.clone());
        self.persona_map.write().insert(persona_name, user_id);

        data
    }

    /// Registers the async notification channel for a connected user.
    pub fn register_notification_channel(&self, user_id: u64, tx: UnboundedSender<BlazePacket>) {
        self.notif_channels.write().insert(user_id, tx);
        tracing::debug!("[SessionStore] Registered notification channel for user {}", user_id);
    }

    /// Removes the async notification channel for a disconnected user.
    pub fn remove_notification_channel(&self, user_id: u64) {
        self.notif_channels.write().remove(&user_id);
        tracing::debug!("[SessionStore] Removed notification channel for user {}", user_id);
    }

    /// Sends an async unsolicited Blaze notification packet directly to a specific user.
    pub fn send_notification(&self, user_id: u64, packet: BlazePacket) -> bool {
        if let Some(tx) = self.notif_channels.read().get(&user_id) {
            tx.send(packet).is_ok()
        } else {
            false
        }
    }

    /// Broadcasts an async Blaze notification packet to multiple users.
    pub fn broadcast_notification(&self, user_ids: &[u64], packet: BlazePacket) {
        let channels = self.notif_channels.read();
        for &uid in user_ids {
            if let Some(tx) = channels.get(&uid) {
                let _ = tx.send(packet.clone());
            }
        }
    }

    /// Retrieves session data by user ID.
    pub fn get_session(&self, user_id: u64) -> Option<SessionData> {
        self.sessions.read().get(&user_id).cloned()
    }

    /// Retrieves session data by persona name.
    pub fn get_by_persona(&self, name: &str) -> Option<SessionData> {
        let uid = self.persona_map.read().get(name).copied()?;
        self.get_session(uid)
    }

    /// Sets or clears a target room code for a player session.
    pub fn set_target_room_code(&self, user_id: u64, code: Option<String>) {
        if let Some(sess) = self.sessions.write().get_mut(&user_id) {
            sess.target_room_code = code;
        }
    }

    /// Gets the target room code configured for a player session.
    pub fn get_target_room_code(&self, user_id: u64) -> Option<String> {
        self.sessions.read().get(&user_id).and_then(|s| s.target_room_code.clone())
    }

    /// Updates network information for an active player.
    pub fn update_network_info(
        &self,
        user_id: u64,
        external_ip: &str,
        internal_ip: &str,
        port: u16,
        nat_type: u8,
    ) {
        if let Some(sess) = self.sessions.write().get_mut(&user_id) {
            sess.external_ip = external_ip.to_string();
            sess.internal_ip = internal_ip.to_string();
            sess.port = port;
            sess.nat_type = nat_type;
            sess.last_seen = Instant::now();
        }
    }

    /// Returns a list of all active player sessions.
    pub fn list_active_sessions(&self) -> Vec<SessionData> {
        self.sessions.read().values().cloned().collect()
    }

    /// Returns the number of currently registered players.
    pub fn count(&self) -> usize {
        self.sessions.read().len()
    }

    /// Removes an ended session.
    pub fn remove_session(&self, user_id: u64) {
        self.remove_notification_channel(user_id);
        if let Some(removed) = self.sessions.write().remove(&user_id) {
            self.persona_map.write().remove(&removed.persona_name);
            tracing::info!(
                "[SessionStore] Removed session for user {} ({})",
                user_id,
                removed.persona_name
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_store_multi_client_allocation() {
        let store = SessionStore::new();

        let s1 = store.create_session(None);
        assert_eq!(s1.user_id, 10001);
        assert_eq!(s1.persona_name, "F22_Player1");

        let s2 = store.create_session(Some("Striker99"));
        assert_eq!(s2.user_id, 10002);
        assert_eq!(s2.persona_name, "Striker99");

        assert_eq!(store.count(), 2);
        assert!(store.get_session(10001).is_some());
        assert_eq!(store.get_by_persona("Striker99").unwrap().user_id, 10002);

        store.update_network_info(10002, "192.168.1.50", "192.168.1.50", 3659, 0);
        let updated = store.get_session(10002).unwrap();
        assert_eq!(updated.external_ip, "192.168.1.50");
        assert_eq!(updated.port, 3659);

        // Test room code setting
        store.set_target_room_code(10001, Some("XY79K2".to_string()));
        assert_eq!(store.get_target_room_code(10001), Some("XY79K2".to_string()));
    }

    #[test]
    fn test_steam_identity_and_lobby_binding() {
        let store = SessionStore::new();

        let profile = SteamProfile {
            steam_id: 76561198012345678,
            persona_name: "ProGamer_Steam".to_string(),
            friends: vec![
                SteamFriend {
                    steam_id: 76561198087654321,
                    name: "SteamBuddy".to_string(),
                },
            ],
        };

        store.register_steam_identity(profile);

        let sess = store.create_session(None);
        assert_eq!(sess.persona_name, "ProGamer_Steam");
        assert_eq!(sess.steam_id, Some(76561198012345678));
        assert_eq!(sess.online_friends.len(), 1);
        assert_eq!(sess.online_friends[0].1, "SteamBuddy");

        store.bind_steam_lobby("XY79K2", 109775241000000000);
        assert_eq!(store.get_steam_lobby("xy79k2"), Some(109775241000000000));
    }
}
