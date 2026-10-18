//! # Concurrent Player Session Store
//!
//! Maintains active player sessions (User IDs, Persona IDs, names, network addresses,
//! target room codes, and async notification channels) across all connected Blaze sessions.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;
use parking_lot::RwLock;
use tokio::sync::mpsc::UnboundedSender;
use blaze_core::BlazePacket;

/// Active player session information.
#[derive(Debug, Clone)]
pub struct SessionData {
    pub user_id: u64,
    pub persona_id: u64,
    pub persona_name: String,
    pub session_key: String,
    pub external_ip: String,
    pub internal_ip: String,
    pub port: u16,
    pub nat_type: u8,
    pub target_room_code: Option<String>,
    pub last_seen: Instant,
}

/// Global thread-safe session registry.
pub struct SessionStore {
    next_id: AtomicU64,
    sessions: RwLock<HashMap<u64, SessionData>>,
    persona_map: RwLock<HashMap<String, u64>>,
    notif_channels: RwLock<HashMap<u64, UnboundedSender<BlazePacket>>>,
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
        }
    }

    /// Allocates and registers a new active session for a player.
    pub fn create_session(&self, requested_name: Option<&str>) -> SessionData {
        let user_id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let persona_id = user_id;

        let persona_name = match requested_name {
            Some(name) if !name.trim().is_empty() => name.trim().to_string(),
            _ => {
                let player_idx = user_id - 10000;
                format!("F22_Player{}", player_idx)
            }
        };

        let session_key = format!("F22_SESS_{:08X}_{:04X}", user_id, user_id ^ 0xBEEF);

        let data = SessionData {
            user_id,
            persona_id,
            persona_name: persona_name.clone(),
            session_key,
            external_ip: "127.0.0.1".to_string(),
            internal_ip: "127.0.0.1".to_string(),
            port: 3659,
            nat_type: 0, // 0 = OPEN
            target_room_code: None,
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
}
