//! # GameManager Component (0x0004) & Game Session Engine
//!
//! Orchestrates P2P lobbies, Room Code reservations, native auto-matchmaking,
//! game attributes, and cross-peer async notification broadcasts.

use std::collections::HashMap;
use std::io;
use std::sync::atomic::{AtomicU64, Ordering};
use parking_lot::RwLock;

use blaze_core::BlazePacket;
use blaze_tdf::{TdfList, TdfMap, TdfStruct, TdfType, TdfValue};
use crate::session::SessionState;
use crate::session_store::{SessionData, SessionStore};

pub const COMPONENT_GAME_MANAGER: u16 = 0x0004;

pub const CMD_CREATE_GAME: u16 = 0x0001;
pub const CMD_DESTROY_GAME: u16 = 0x0002;
pub const CMD_JOIN_GAME: u16 = 0x0004;
pub const CMD_START_MATCHMAKING: u16 = 0x0007;
pub const CMD_CANCEL_MATCHMAKING: u16 = 0x0008;
pub const CMD_FINALIZE_GAME_CREATION: u16 = 0x000B;
pub const CMD_SET_GAME_SETTINGS: u16 = 0x000C;
pub const CMD_UPDATE_PLAYER_STATE: u16 = 0x0014;
pub const CMD_UPDATE_GAME_ATTRIBUTES: u16 = 0x0016;

pub const NOTIFY_GAME_REMOVED: u16 = 0x000A;
pub const NOTIFY_GAME_REMOVED_ALT: u16 = 0x0010;
pub const NOTIFY_GAME_CREATED: u16 = 0x000F;
pub const NOTIFY_GAME_SETUP: u16 = 0x0014;
pub const NOTIFY_PLAYER_JOINING: u16 = 0x0015;
pub const NOTIFY_PLAYER_LEFT: u16 = 0x0016;
pub const NOTIFY_GAME_ATTRIB_CHANGE_ALT: u16 = 0x0018;
pub const NOTIFY_PLAYER_REMOVED: u16 = 0x0020;
pub const NOTIFY_GAME_ATTRIB_CHANGE: u16 = 0x0050;
pub const NOTIFY_GAME_STATE_CHANGE: u16 = 0x0064;
pub const NOTIFY_PLAYER_STATE_CHANGE: u16 = 0x0074;
pub const NOTIFY_MATCHMAKING_ASYNC_STATUS: u16 = 0x000C;

pub const NOTIFY_JOINING_PLAYER_INITIATE_CONNECTIONS: u16 = 0x0017;
pub const NOTIFY_PLATFORM_HOST_INITIALIZED: u16 = 0x0018;
pub const NOTIFY_GAME_REPORTING_ID_CHANGE: u16 = 0x0070;

pub const GAME_STATE_INITIALIZING: u16 = 1;
pub const GAME_STATE_IN_LOBBY: u16 = 2;
pub const GAME_STATE_PRE_GAME: u16 = 3;
pub const GAME_STATE_IN_GAME: u16 = 4;
pub const GAME_STATE_POST_GAME: u16 = 5;
pub const GAME_STATE_PRE_GAME_130: u16 = 130;
pub const GAME_STATE_IN_GAME_131: u16 = 131;

/// Strict 6-step GameManager notification sequence
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum SequenceStep {
    NotifyPlayerJoining = 0,
    NotifyJoiningPlayerInitiateConnections = 1,
    NotifyPlatformHostInitialized = 2,
    NotifyGameReportingIdChange = 3,
    GameState130 = 4,
    GameState131 = 5,
}

/// Tracks and validates strict ordering for a game session's notifications.
#[derive(Debug, Clone)]
pub struct NotificationSequence {
    pub game_id: u64,
    pub current_step: Option<SequenceStep>,
}

impl NotificationSequence {
    pub fn new(game_id: u64) -> Self {
        Self {
            game_id,
            current_step: None,
        }
    }

    pub fn advance_to(&mut self, next: SequenceStep) {
        let expected = match self.current_step {
            None => SequenceStep::NotifyPlayerJoining,
            Some(SequenceStep::NotifyPlayerJoining) => SequenceStep::NotifyJoiningPlayerInitiateConnections,
            Some(SequenceStep::NotifyJoiningPlayerInitiateConnections) => SequenceStep::NotifyPlatformHostInitialized,
            Some(SequenceStep::NotifyPlatformHostInitialized) => SequenceStep::NotifyGameReportingIdChange,
            Some(SequenceStep::NotifyGameReportingIdChange) => SequenceStep::GameState130,
            Some(SequenceStep::GameState130) => SequenceStep::GameState131,
            Some(SequenceStep::GameState131) => SequenceStep::GameState131,
        };

        if cfg!(debug_assertions) && next != expected {
            panic!(
                "Strict NotificationSequence violation for Game {}: expected step {:?}, attempted send {:?}",
                self.game_id, expected, next
            );
        }

        self.current_step = Some(next);
    }
}

/// Builder enforcing strict ordering of match initialization notifications.
/// Panics in debug builds on out-of-order sends.
#[derive(Debug)]
pub struct NotificationSequenceBuilder {
    pub sequence: NotificationSequence,
    packets: Vec<(SequenceStep, BlazePacket)>,
}

impl NotificationSequenceBuilder {
    pub fn new(game_id: u64) -> Self {
        Self {
            sequence: NotificationSequence::new(game_id),
            packets: Vec::new(),
        }
    }

    pub fn notify_player_joining(mut self, pkt: BlazePacket) -> Self {
        self.sequence.advance_to(SequenceStep::NotifyPlayerJoining);
        self.packets.push((SequenceStep::NotifyPlayerJoining, pkt));
        self
    }

    pub fn notify_joining_player_initiate_connections(mut self, pkt: BlazePacket) -> Self {
        self.sequence.advance_to(SequenceStep::NotifyJoiningPlayerInitiateConnections);
        self.packets.push((SequenceStep::NotifyJoiningPlayerInitiateConnections, pkt));
        self
    }

    pub fn notify_platform_host_initialized(mut self, pkt: BlazePacket) -> Self {
        self.sequence.advance_to(SequenceStep::NotifyPlatformHostInitialized);
        self.packets.push((SequenceStep::NotifyPlatformHostInitialized, pkt));
        self
    }

    pub fn notify_game_reporting_id_change(mut self, pkt: BlazePacket) -> Self {
        self.sequence.advance_to(SequenceStep::NotifyGameReportingIdChange);
        self.packets.push((SequenceStep::NotifyGameReportingIdChange, pkt));
        self
    }

    pub fn game_state_130(mut self, pkt: BlazePacket) -> Self {
        self.sequence.advance_to(SequenceStep::GameState130);
        self.packets.push((SequenceStep::GameState130, pkt));
        self
    }

    pub fn game_state_131(mut self, pkt: BlazePacket) -> Self {
        self.sequence.advance_to(SequenceStep::GameState131);
        self.packets.push((SequenceStep::GameState131, pkt));
        self
    }

    pub fn build(self) -> Vec<BlazePacket> {
        self.packets.into_iter().map(|(_, p)| p).collect()
    }
}

/// Traced Blaze message recorded for deadlock diagnosis.
#[derive(Debug, Clone)]
pub struct TracedMessage {
    pub direction: &'static str,
    pub component: u16,
    pub command: u16,
    pub timestamp_us: u64,
    pub tdf_hex: String,
}

/// Guard detecting server hangs waiting >5s for MatchReady, dumping the last 20 TDF hex messages.
pub struct DeadlockGuard {
    recent_messages: RwLock<std::collections::VecDeque<TracedMessage>>,
    wait_start: RwLock<Option<(u64, std::time::Instant)>>,
    warned: std::sync::atomic::AtomicBool,
}

impl Default for DeadlockGuard {
    fn default() -> Self {
        Self::new()
    }
}

impl DeadlockGuard {
    pub fn new() -> Self {
        Self {
            recent_messages: RwLock::new(std::collections::VecDeque::with_capacity(20)),
            wait_start: RwLock::new(None),
            warned: std::sync::atomic::AtomicBool::new(false),
        }
    }

    pub fn record_message(&self, direction: &'static str, comp: u16, cmd: u16, tdf_bytes: &[u8]) {
        let now_us = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_micros() as u64;

        let hex = blaze_core::hex_dump(tdf_bytes);
        let mut msgs = self.recent_messages.write();
        if msgs.len() >= 20 {
            msgs.pop_front();
        }
        msgs.push_back(TracedMessage {
            direction,
            component: comp,
            command: cmd,
            timestamp_us: now_us,
            tdf_hex: hex,
        });
    }

    pub fn start_waiting_for_match_ready(&self, game_id: u64) {
        *self.wait_start.write() = Some((game_id, std::time::Instant::now()));
        self.warned.store(false, Ordering::SeqCst);
    }

    pub fn match_ready_achieved(&self, _game_id: u64) {
        *self.wait_start.write() = None;
    }

    pub fn check_deadlock(&self, game_id: u64) -> bool {
        let lock = self.wait_start.read();
        if let Some((waiting_gid, start)) = *lock {
            if waiting_gid == game_id && start.elapsed() > std::time::Duration::from_secs(5) {
                if !self.warned.swap(true, Ordering::SeqCst) {
                    let msgs = self.recent_messages.read();
                    let mut dump = String::new();
                    dump.push_str(&format!(
                        "\n==================== [DeadlockGuard WARN: Game {} waited >5s for MatchReady] ====================\n",
                        game_id
                    ));
                    for (i, msg) in msgs.iter().enumerate() {
                        dump.push_str(&format!(
                            "[{:02}] {} Comp=0x{:04X} Cmd=0x{:04X} t={}us:\n{}\n",
                            i + 1, msg.direction, msg.component, msg.command, msg.timestamp_us, msg.tdf_hex
                        ));
                    }
                    dump.push_str("============================================================================================\n");
                    tracing::warn!("{}", dump);
                }
                return true;
            }
        }
        false
    }

    pub fn dump_recent(&self) -> Vec<TracedMessage> {
        self.recent_messages.read().iter().cloned().collect()
    }
}

/// Represents an active 1v1 multiplayer game session.
#[derive(Debug, Clone)]
pub struct GameSession {
    pub game_id: u64,
    pub room_code: String,
    pub host_user_id: u64,
    pub host_persona_id: u64,
    pub guest_user_id: Option<u64>,
    pub guest_persona_id: Option<u64>,
    pub game_state: u16,
    pub settings: u32,
    pub seed: u32,
    pub attributes: HashMap<String, String>,
    pub host_state: u16,
    pub guest_state: u16,
    pub squad_hash_synced: bool,
    pub host_match_ready: bool,
    pub guest_match_ready: bool,
}

/// Global registry managing all active multiplayer game sessions.
pub struct GameRegistry {
    next_game_id: AtomicU64,
    games: RwLock<HashMap<u64, GameSession>>,
    room_map: RwLock<HashMap<String, u64>>,
    player_game_map: RwLock<HashMap<u64, u64>>,
    pub deadlock_guard: std::sync::Arc<DeadlockGuard>,
}

impl Default for GameRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl GameRegistry {
    pub fn new() -> Self {
        Self {
            next_game_id: AtomicU64::new(1),
            games: RwLock::new(HashMap::new()),
            room_map: RwLock::new(HashMap::new()),
            player_game_map: RwLock::new(HashMap::new()),
            deadlock_guard: std::sync::Arc::new(DeadlockGuard::new()),
        }
    }

    /// Generates a deterministic 32-bit random seed for physics and gameplay synchronization.
    pub fn generate_deterministic_seed() -> u32 {
        use std::time::SystemTime;
        let nanos = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .subsec_nanos();
        let s = (nanos ^ 0x5F3759DF) & 0x7FFFFFFF;
        if s == 0 {
            13374269
        } else {
            s
        }
    }

    /// Generates a random 6-character uppercase alphanumeric Room Code.
    pub fn generate_room_code(&self) -> String {
        use std::time::SystemTime;
        let nanos = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .subsec_nanos();
        let chars = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789";
        let mut code = String::with_capacity(6);
        let mut seed = nanos;
        for _ in 0..6 {
            let idx = (seed % (chars.len() as u32)) as usize;
            code.push(chars[idx] as char);
            seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
        }
        code
    }

    /// Allocates and creates a new game session.
    pub fn create_game(
        &self,
        host: &SessionData,
        custom_room_code: Option<String>,
    ) -> GameSession {
        let game_id = self.next_game_id.fetch_add(1, Ordering::Relaxed);
        let room_code = custom_room_code.unwrap_or_else(|| self.generate_room_code());
        let seed = Self::generate_deterministic_seed();
        let canonical_hash = services_http_content::SquadStore::global().get_canonical_hash();

        let mut attributes = HashMap::new();
        attributes.insert("ATTR_SEED".to_string(), seed.to_string());
        attributes.insert("ATTR_SQDH".to_string(), canonical_hash);
        attributes.insert("ATTR_MATCH_TYPE".to_string(), "0".to_string()); // Standard Head-to-Head Friendly
        attributes.insert("ATTR_GAME_MODE".to_string(), "1".to_string());  // Online Friendly (not UCL tournament)
        attributes.insert("ATTR_CUSTOM_GAME_MODE".to_string(), "0".to_string());
        attributes.insert("ATTR_TEAM_HOME".to_string(), "243".to_string()); // Real Madrid default
        attributes.insert("ATTR_TEAM_AWAY".to_string(), "9".to_string());   // Liverpool default
        attributes.insert("ATTR_KITS".to_string(), "1".to_string());
        attributes.insert("ATTR_HALF_LEN".to_string(), "6".to_string());
        attributes.insert("ATTR_DIFF".to_string(), "3".to_string());
        attributes.insert("ATTR_CONTROL".to_string(), "0".to_string());

        let session = GameSession {
            game_id,
            room_code: room_code.clone(),
            host_user_id: host.user_id,
            host_persona_id: host.persona_id,
            guest_user_id: None,
            guest_persona_id: None,
            game_state: GAME_STATE_INITIALIZING,
            settings: 0x00000001,
            seed,
            attributes,
            host_state: GAME_STATE_INITIALIZING,
            guest_state: GAME_STATE_INITIALIZING,
            squad_hash_synced: true,
            host_match_ready: false,
            guest_match_ready: false,
        };

        self.games.write().insert(game_id, session.clone());
        self.room_map.write().insert(room_code.clone(), game_id);
        self.player_game_map.write().insert(host.user_id, game_id);

        tracing::info!(
            "[GameManager] Created Game {} (Room Code: '{}', Seed: {}, SquadHash: OK) for Host '{}' (UID={})",
            game_id,
            room_code,
            seed,
            host.persona_name,
            host.user_id
        );

        session
    }

    /// Joins a player into an existing game session.
    pub fn join_game(
        &self,
        game_id: u64,
        guest: &SessionData,
    ) -> Result<GameSession, &'static str> {
        let mut games = self.games.write();
        let game = games.get_mut(&game_id).ok_or("Game not found")?;

        if game.guest_user_id.is_some() && game.guest_user_id != Some(guest.user_id) {
            return Err("Game lobby is full");
        }

        game.guest_user_id = Some(guest.user_id);
        game.guest_persona_id = Some(guest.persona_id);
        game.game_state = GAME_STATE_IN_LOBBY;

        self.player_game_map.write().insert(guest.user_id, game_id);

        tracing::info!(
            "[GameManager] Player '{}' (UID={}) joined Game {} (Room '{}')",
            guest.persona_name,
            guest.user_id,
            game_id,
            game.room_code
        );

        Ok(game.clone())
    }

    /// Finds an open waiting game or creates one (Native Auto-Matchmaking).
    pub fn find_or_create_auto_match(
        &self,
        player: &SessionData,
    ) -> (GameSession, bool) {
        // Look for waiting open game where player is not already host
        let open_game_id = {
            let games = self.games.read();
            games
                .values()
                .find(|g| g.guest_user_id.is_none() && g.host_user_id != player.user_id)
                .map(|g| g.game_id)
        };

        if let Some(gid) = open_game_id {
            if let Ok(joined) = self.join_game(gid, player) {
                return (joined, true); // Joined existing game
            }
        }

        // None available, create new waiting lobby
        let new_game = self.create_game(player, None);
        (new_game, false) // Created new game
    }

    /// Looks up a game by room code.
    pub fn find_by_room_code(&self, code: &str) -> Option<GameSession> {
        let gid = self.room_map.read().get(code).copied()?;
        self.games.read().get(&gid).cloned()
    }

    /// Looks up a game by game ID.
    pub fn get_game(&self, game_id: u64) -> Option<GameSession> {
        self.games.read().get(&game_id).cloned()
    }

    /// Looks up the active game a player is currently in.
    pub fn find_by_player(&self, user_id: u64) -> Option<GameSession> {
        let gid = self.player_game_map.read().get(&user_id).copied()?;
        self.games.read().get(&gid).cloned()
    }

    /// Updates arbitrary game attributes (e.g. squad hash, team choices).
    pub fn update_attributes(
        &self,
        game_id: u64,
        new_attrs: HashMap<String, String>,
    ) -> Option<GameSession> {
        let mut games = self.games.write();
        let game = games.get_mut(&game_id)?;
        let canonical_hash = services_http_content::SquadStore::global().get_canonical_hash();

        for (k, v) in new_attrs {
            if k == "ATTR_SQDH" {
                if v != canonical_hash {
                    tracing::warn!(
                        "[GameManager] Game {}: Squad Hash mismatch! Client sent '{}', canonical is '{}'",
                        game_id, v, canonical_hash
                    );
                    game.squad_hash_synced = false;
                } else {
                    game.squad_hash_synced = true;
                }
            }
            game.attributes.insert(k, v);
        }
        Some(game.clone())
    }

    /// Updates player readiness state and triggers symmetric kickoff when both peers are ready.
    pub fn update_player_state(
        &self,
        game_id: u64,
        user_id: u64,
        new_state: u16,
    ) -> Option<(GameSession, Option<u16>)> {
        let mut games = self.games.write();
        let game = games.get_mut(&game_id)?;

        if user_id == game.host_user_id {
            game.host_state = new_state;
            if new_state == GAME_STATE_IN_GAME_131 || new_state == GAME_STATE_IN_GAME {
                game.host_match_ready = true;
            }
        } else if Some(user_id) == game.guest_user_id {
            game.guest_state = new_state;
            if new_state == GAME_STATE_IN_GAME_131 || new_state == GAME_STATE_IN_GAME {
                game.guest_match_ready = true;
            }
        }

        let mut state_transition = None;

        let host_in_game = game.host_match_ready || game.host_state == GAME_STATE_IN_GAME || game.host_state == GAME_STATE_IN_GAME_131;
        let guest_in_game = game.guest_match_ready || game.guest_state == GAME_STATE_IN_GAME || game.guest_state == GAME_STATE_IN_GAME_131;
        let host_pre_game = game.host_state == GAME_STATE_PRE_GAME_130 || game.host_state >= GAME_STATE_PRE_GAME;
        let guest_pre_game = game.guest_state == GAME_STATE_PRE_GAME_130 || game.guest_state >= GAME_STATE_PRE_GAME;

        if game.guest_user_id.is_some() {
            // Both clients must send MatchReady (131 or IN_GAME) before advancing to 131 / InGame
            if host_in_game && guest_in_game {
                if game.game_state != GAME_STATE_IN_GAME_131 && game.game_state != GAME_STATE_IN_GAME {
                    game.game_state = new_state;
                    state_transition = Some(new_state);
                }
            } else if host_pre_game && guest_pre_game {
                if game.game_state != GAME_STATE_PRE_GAME_130 && game.game_state != GAME_STATE_PRE_GAME && game.game_state < GAME_STATE_IN_GAME {
                    game.game_state = new_state;
                    state_transition = Some(new_state);
                }
            }
        } else {
            if host_in_game && game.game_state != GAME_STATE_IN_GAME_131 && game.game_state != GAME_STATE_IN_GAME {
                game.game_state = new_state;
                state_transition = Some(new_state);
            } else if host_pre_game && game.game_state < GAME_STATE_PRE_GAME {
                game.game_state = new_state;
                state_transition = Some(new_state);
            }
        }

        Some((game.clone(), state_transition))
    }

    /// Sets player MatchReady and advances to state 131 if both players are ready.
    pub fn set_player_match_ready(
        &self,
        game_id: u64,
        user_id: u64,
        store: &SessionStore,
    ) -> Option<(GameSession, bool)> {
        let mut games = self.games.write();
        let game = games.get_mut(&game_id)?;

        if user_id == game.host_user_id {
            game.host_match_ready = true;
            game.host_state = GAME_STATE_IN_GAME_131;
        } else if Some(user_id) == game.guest_user_id {
            game.guest_match_ready = true;
            game.guest_state = GAME_STATE_IN_GAME_131;
        }

        let both_ready = if game.guest_user_id.is_some() {
            game.host_match_ready && game.guest_match_ready
        } else {
            game.host_match_ready
        };

        let game_clone = game.clone();
        drop(games);

        if both_ready && game_clone.game_state != GAME_STATE_IN_GAME_131 {
            self.advance_state(game_id, GAME_STATE_IN_GAME_131, store);
        }

        Some((game_clone, both_ready))
    }

    /// Centralized state machine transition for GameManager.
    /// Enforces the strict notification sequence:
    /// NotifyPlayerJoining (0x0015) ->
    /// NotifyJoiningPlayerInitiateConnections (0x0017) ->
    /// NotifyPlatformHostInitialized (0x0018) ->
    /// NotifyGameReportingIdChange (0x0070) ->
    /// GameState(130) ->
    /// GameState(131)
    /// Also arms and triggers DeadlockGuard if waiting >5s for MatchReady.
    pub fn advance_state(
        &self,
        game_id: u64,
        target_state: u16,
        store: &SessionStore,
    ) -> Option<GameSession> {
        let mut games = self.games.write();
        let game = games.get_mut(&game_id)?;

        let prev_state = game.game_state;
        game.game_state = target_state;
        let game_clone = game.clone();
        drop(games);

        tracing::info!(
            "[GameManager] advance_state: Game {} transitioning from {} to {}",
            game_id,
            prev_state,
            target_state
        );

        let mut participants = vec![game_clone.host_user_id];
        if let Some(gid) = game_clone.guest_user_id {
            participants.push(gid);
        }

        match target_state {
            GAME_STATE_IN_LOBBY => {
                if let Ok(notif) = build_notify_game_state_change(game_id, GAME_STATE_IN_LOBBY) {
                    store.broadcast_notification(&participants, notif);
                }
            }
            GAME_STATE_PRE_GAME | GAME_STATE_PRE_GAME_130 => {
                let host_sess = store.get_session(game_clone.host_user_id);
                let guest_sess = game_clone.guest_user_id.and_then(|uid| store.get_session(uid));

                let mut builder = NotificationSequenceBuilder::new(game_id);

                // Step 1: NotifyPlayerJoining (0x0015)
                if let Some(ref guest) = guest_sess {
                    if let Ok(pkt) = build_notify_player_joining(&game_clone, guest) {
                        builder = builder.notify_player_joining(pkt.clone());
                        store.send_notification(game_clone.host_user_id, pkt);
                    }
                } else if let Some(ref host) = host_sess {
                    if let Ok(pkt) = build_notify_player_joining(&game_clone, host) {
                        builder = builder.notify_player_joining(pkt);
                    }
                }

                // Step 2: NotifyJoiningPlayerInitiateConnections (0x0017)
                let guest_pid = guest_sess.as_ref().map(|g| g.persona_id).unwrap_or(0);
                if let Ok(pkt) = build_notify_joining_player_initiate_connections(game_id, guest_pid) {
                    builder = builder.notify_joining_player_initiate_connections(pkt.clone());
                    store.broadcast_notification(&participants, pkt);
                }

                // Step 3: NotifyPlatformHostInitialized (0x0018)
                let host_pid = host_sess.as_ref().map(|h| h.persona_id).unwrap_or(game_clone.host_persona_id);
                if let Ok(pkt) = build_notify_platform_host_initialized(game_id, host_pid) {
                    builder = builder.notify_platform_host_initialized(pkt.clone());
                    store.broadcast_notification(&participants, pkt);
                }

                // Step 4: NotifyGameReportingIdChange (0x0070)
                if let Ok(pkt) = build_notify_game_reporting_id_change(game_id, game_id) {
                    builder = builder.notify_game_reporting_id_change(pkt.clone());
                    store.broadcast_notification(&participants, pkt);
                }

                // Step 5: GameState(130) (0x0064)
                if let Ok(pkt) = build_notify_game_state_change(game_id, GAME_STATE_PRE_GAME_130) {
                    builder = builder.game_state_130(pkt.clone());
                    store.broadcast_notification(&participants, pkt);
                }
                let _ = builder.build();

                if target_state == GAME_STATE_PRE_GAME {
                    if let Ok(pkt3) = build_notify_game_state_change(game_id, GAME_STATE_PRE_GAME) {
                        store.broadcast_notification(&participants, pkt3);
                    }
                }

                // Arm deadlock guard (>5s)
                self.deadlock_guard.start_waiting_for_match_ready(game_id);

                let dg = self.deadlock_guard.clone();
                tokio::spawn(async move {
                    tokio::time::sleep(std::time::Duration::from_millis(5050)).await;
                    dg.check_deadlock(game_id);
                });
            }
            GAME_STATE_IN_GAME | GAME_STATE_IN_GAME_131 => {
                // MatchReady achieved - disarm deadlock guard!
                self.deadlock_guard.match_ready_achieved(game_id);

                // Step 6: GameState(131)
                if let Ok(pkt) = build_notify_game_state_change(game_id, GAME_STATE_IN_GAME_131) {
                    store.broadcast_notification(&participants, pkt);
                }

                if target_state == GAME_STATE_IN_GAME {
                    if let Ok(pkt4) = build_notify_game_state_change(game_id, GAME_STATE_IN_GAME) {
                        store.broadcast_notification(&participants, pkt4);
                    }
                }
            }
            _ => {
                if let Ok(pkt) = build_notify_game_state_change(game_id, target_state) {
                    store.broadcast_notification(&participants, pkt);
                }
            }
        }

        Some(game_clone)
    }

    /// Updates game state (Lobby -> PreGame -> InGame).
    pub fn update_state(&self, game_id: u64, state: u16) -> Option<GameSession> {
        let mut games = self.games.write();
        let game = games.get_mut(&game_id)?;
        game.game_state = state;
        Some(game.clone())
    }

    /// Destroys a game session and frees all reservations.
    pub fn destroy_game(&self, game_id: u64) -> Option<GameSession> {
        let removed = self.games.write().remove(&game_id)?;
        self.room_map.write().remove(&removed.room_code);
        self.player_game_map.write().remove(&removed.host_user_id);
        if let Some(gid) = removed.guest_user_id {
            self.player_game_map.write().remove(&gid);
        }
        tracing::info!("[GameManager] Destroyed Game {}", game_id);
        Some(removed)
    }

    /// Handles abrupt player disconnection, cleanly cleans up session, and returns (removed_game, remaining_players)
    pub fn handle_player_disconnect(&self, user_id: u64) -> Option<(GameSession, Vec<u64>)> {
        let game = self.find_by_player(user_id)?;
        let mut remaining = Vec::new();
        if game.host_user_id != user_id {
            remaining.push(game.host_user_id);
        }
        if let Some(gid) = game.guest_user_id {
            if gid != user_id {
                remaining.push(gid);
            }
        }

        let removed = self.destroy_game(game.game_id)?;
        Some((removed, remaining))
    }

    /// Returns all active game sessions in the registry.
    pub fn list_all_games(&self) -> Vec<GameSession> {
        self.games.read().values().cloned().collect()
    }
}

// =============================================================================
// Blaze Notification Builders
// =============================================================================

/// Builds a NotifyGameCreated (0x000F) notification.
pub fn build_notify_game_created(game: &GameSession) -> io::Result<BlazePacket> {
    let mut payload = TdfStruct::new();
    payload.set("GID", TdfValue::Integer(game.game_id));
    BlazePacket::notification(COMPONENT_GAME_MANAGER, NOTIFY_GAME_CREATED, payload)
}

/// Builds a NotifyGameSetup (0x0014) notification describing full lobby state and peers.
pub fn build_notify_game_setup(
    game: &GameSession,
    host: &SessionData,
    guest: Option<&SessionData>,
) -> io::Result<BlazePacket> {
    let mut payload = TdfStruct::new();

    // GAME struct
    let mut game_struct = TdfStruct::new();
    game_struct.set("GID", TdfValue::Integer(game.game_id));
    game_struct.set("GNAM", TdfValue::String("OSDK_match".to_string()));
    game_struct.set("GSTA", TdfValue::Integer(game.game_state as u64));
    game_struct.set("GSET", TdfValue::Integer(game.settings as u64));
    game_struct.set("SEED", TdfValue::Integer(game.seed as u64));
    game_struct.set("UUID", TdfValue::String(game.room_code.clone()));

    // ATTR Map
    let mut attr_entries = Vec::new();
    for (k, v) in &game.attributes {
        attr_entries.push((TdfValue::String(k.clone()), TdfValue::String(v.clone())));
    }
    game_struct.set(
        "ATTR",
        TdfValue::Map(TdfMap::new(TdfType::String, TdfType::String, attr_entries)),
    );

    // Host network address inside PRES. `PRES` is the `NetworkAddress`
    // union; case 2 is `IpPairAddress`, the shape actual players use.
    game_struct.set(
        "PRES",
        crate::netaddr::network_address_ip_pair(crate::netaddr::ip_pair_address(
            crate::netaddr::ip_address(&host.external_ip, 0, 0),
            crate::netaddr::ip_address(&host.external_ip, 0x7F00_0001, host.port),
        )),
    );

    payload.set("GAME", TdfValue::Struct(game_struct));

    // PROS: Player roster list
    let mut roster = Vec::new();

    // Host player (slot 0)
    let mut p0 = TdfStruct::new();
    p0.set("PID", TdfValue::Integer(host.persona_id));
    p0.set("PNAM", TdfValue::String(host.persona_name.clone()));
    p0.set("SID", TdfValue::Integer(0));
    p0.set("STAT", TdfValue::Integer(2)); // Connected
    p0.set("UID", TdfValue::Integer(host.user_id));
    roster.push(TdfValue::Struct(p0));

    // Guest player (slot 1) if present
    if let Some(g) = guest {
        let mut p1 = TdfStruct::new();
        p1.set("PID", TdfValue::Integer(g.persona_id));
        p1.set("PNAM", TdfValue::String(g.persona_name.clone()));
        p1.set("SID", TdfValue::Integer(1));
        p1.set("STAT", TdfValue::Integer(2)); // Connected
        p1.set("UID", TdfValue::Integer(g.user_id));
        roster.push(TdfValue::Struct(p1));
    }

    payload.set("PROS", TdfValue::List(TdfList::new(TdfType::Struct, roster)));

    BlazePacket::notification(COMPONENT_GAME_MANAGER, NOTIFY_GAME_SETUP, payload)
}

/// Builds a NotifyPlayerJoining (0x0015) notification pushed to the host.
pub fn build_notify_player_joining(
    game: &GameSession,
    joining: &SessionData,
) -> io::Result<BlazePacket> {
    let mut payload = TdfStruct::new();
    payload.set("GID", TdfValue::Integer(game.game_id));

    let mut player = TdfStruct::new();
    player.set("PID", TdfValue::Integer(joining.persona_id));
    player.set("PNAM", TdfValue::String(joining.persona_name.clone()));
    player.set("SID", TdfValue::Integer(1)); // Guest slot
    player.set("STAT", TdfValue::Integer(2));
    player.set("UID", TdfValue::Integer(joining.user_id));

    // `ADDR` is the `NetworkAddress` union; case 2 is `IpPairAddress`.
    player.set(
        "ADDR",
        crate::netaddr::network_address_ip_pair(crate::netaddr::ip_pair_address(
            crate::netaddr::ip_address(&joining.external_ip, 0, 0),
            crate::netaddr::ip_address(&joining.external_ip, 0x7F00_0001, joining.port),
        )),
    );

    payload.set("PDAT", TdfValue::Struct(player));

    BlazePacket::notification(COMPONENT_GAME_MANAGER, NOTIFY_PLAYER_JOINING, payload)
}

/// Builds a NotifyGameStateChange (0x0064) notification.
pub fn build_notify_game_state_change(game_id: u64, state: u16) -> io::Result<BlazePacket> {
    let mut payload = TdfStruct::new();
    payload.set("GID", TdfValue::Integer(game_id));
    payload.set("GSTA", TdfValue::Integer(state as u64));
    BlazePacket::notification(COMPONENT_GAME_MANAGER, NOTIFY_GAME_STATE_CHANGE, payload)
}

/// Builds a NotifyGameAttribChange (0x0050) notification.
pub fn build_notify_game_attrib_change(
    game_id: u64,
    attrs: &HashMap<String, String>,
) -> io::Result<BlazePacket> {
    let mut payload = TdfStruct::new();
    payload.set("GID", TdfValue::Integer(game_id));

    let mut entries = Vec::new();
    for (k, v) in attrs {
        entries.push((TdfValue::String(k.clone()), TdfValue::String(v.clone())));
    }
    payload.set(
        "ATTR",
        TdfValue::Map(TdfMap::new(TdfType::String, TdfType::String, entries)),
    );

    BlazePacket::notification(COMPONENT_GAME_MANAGER, NOTIFY_GAME_ATTRIB_CHANGE, payload)
}

/// Builds a NotifyGameAttribChange (0x0018) legacy/alt notification.
pub fn build_notify_game_attrib_change_0018(
    game_id: u64,
    attrs: &HashMap<String, String>,
) -> io::Result<BlazePacket> {
    let mut payload = TdfStruct::new();
    payload.set("GID", TdfValue::Integer(game_id));

    let mut entries = Vec::new();
    for (k, v) in attrs {
        entries.push((TdfValue::String(k.clone()), TdfValue::String(v.clone())));
    }
    payload.set(
        "ATTR",
        TdfValue::Map(TdfMap::new(TdfType::String, TdfType::String, entries)),
    );

    BlazePacket::notification(COMPONENT_GAME_MANAGER, NOTIFY_GAME_ATTRIB_CHANGE_ALT, payload)
}

/// Builds a NotifyPlayerStateChange (0x0074) notification.
pub fn build_notify_player_state_change(
    game_id: u64,
    persona_id: u64,
    state: u16,
) -> io::Result<BlazePacket> {
    let mut payload = TdfStruct::new();
    payload.set("GID", TdfValue::Integer(game_id));
    payload.set("PID", TdfValue::Integer(persona_id));
    payload.set("STAT", TdfValue::Integer(state as u64));
    BlazePacket::notification(COMPONENT_GAME_MANAGER, NOTIFY_PLAYER_STATE_CHANGE, payload)
}

/// Builds a NotifyGameRemoved (0x000A) notification.
pub fn build_notify_game_removed(game_id: u64) -> io::Result<BlazePacket> {
    let mut payload = TdfStruct::new();
    payload.set("GID", TdfValue::Integer(game_id));
    BlazePacket::notification(COMPONENT_GAME_MANAGER, NOTIFY_GAME_REMOVED, payload)
}

/// Builds a NotifyGameRemoved (0x0010) alt notification.
pub fn build_notify_game_removed_alt(game_id: u64) -> io::Result<BlazePacket> {
    let mut payload = TdfStruct::new();
    payload.set("GID", TdfValue::Integer(game_id));
    BlazePacket::notification(COMPONENT_GAME_MANAGER, NOTIFY_GAME_REMOVED_ALT, payload)
}

/// Builds a NotifyPlayerRemoved (0x0020) notification.
pub fn build_notify_player_removed(
    game_id: u64,
    persona_id: u64,
    reason: u16,
) -> io::Result<BlazePacket> {
    let mut payload = TdfStruct::new();
    payload.set("GID", TdfValue::Integer(game_id));
    payload.set("PID", TdfValue::Integer(persona_id));
    payload.set("REAS", TdfValue::Integer(reason as u64));
    BlazePacket::notification(COMPONENT_GAME_MANAGER, NOTIFY_PLAYER_REMOVED, payload)
}

/// Builds a NotifyJoiningPlayerInitiateConnections (0x0017) notification.
pub fn build_notify_joining_player_initiate_connections(
    game_id: u64,
    joining_persona_id: u64,
) -> io::Result<BlazePacket> {
    let mut payload = TdfStruct::new();
    payload.set("GID", TdfValue::Integer(game_id));
    payload.set("PID", TdfValue::Integer(joining_persona_id));
    BlazePacket::notification(COMPONENT_GAME_MANAGER, NOTIFY_JOINING_PLAYER_INITIATE_CONNECTIONS, payload)
}

/// Builds a NotifyPlatformHostInitialized (0x0018) notification.
pub fn build_notify_platform_host_initialized(
    game_id: u64,
    host_persona_id: u64,
) -> io::Result<BlazePacket> {
    let mut payload = TdfStruct::new();
    payload.set("GID", TdfValue::Integer(game_id));
    payload.set("PHID", TdfValue::Integer(host_persona_id));
    BlazePacket::notification(COMPONENT_GAME_MANAGER, NOTIFY_PLATFORM_HOST_INITIALIZED, payload)
}

/// Builds a NotifyGameReportingIdChange (0x0070) notification.
pub fn build_notify_game_reporting_id_change(
    game_id: u64,
    reporting_id: u64,
) -> io::Result<BlazePacket> {
    let mut payload = TdfStruct::new();
    payload.set("GID", TdfValue::Integer(game_id));
    payload.set("GRID", TdfValue::Integer(reporting_id));
    BlazePacket::notification(COMPONENT_GAME_MANAGER, NOTIFY_GAME_REPORTING_ID_CHANGE, payload)
}

// =============================================================================
// GameManager RPC Handlers
// =============================================================================

/// Handles createGame (0x0001).
pub fn handle_create_game(
    req: &BlazePacket,
    session: &SessionState,
    store: &SessionStore,
    registry: &GameRegistry,
) -> io::Result<BlazePacket> {
    let host_sess = store.get_session(session.user_id).unwrap_or_else(|| {
        store.create_session(Some(&session.display_name))
    });

    let target_room = store.get_target_room_code(session.user_id);
    let game = registry.create_game(&host_sess, target_room);

    // Push async NotifyGameCreated and NotifyGameSetup to host
    let notif_created = build_notify_game_created(&game)?;
    let notif_setup = build_notify_game_setup(&game, &host_sess, None)?;
    store.send_notification(session.user_id, notif_created);
    store.send_notification(session.user_id, notif_setup);

    let mut resp = TdfStruct::new();
    resp.set("GID", TdfValue::Integer(game.game_id));
    BlazePacket::response(&req.header, resp)
}

/// Handles joinGame (0x0004).
pub fn handle_join_game(
    req: &BlazePacket,
    session: &SessionState,
    store: &SessionStore,
    registry: &GameRegistry,
) -> io::Result<BlazePacket> {
    let guest_sess = store.get_session(session.user_id).unwrap_or_else(|| {
        store.create_session(Some(&session.display_name))
    });

    let game_id = match req.payload.get("GID") {
        Some(TdfValue::Integer(gid)) => *gid,
        _ => {
            // Check if player configured a room code
            if let Some(code) = store.get_target_room_code(session.user_id) {
                registry.find_by_room_code(&code).map(|g| g.game_id).unwrap_or(1)
            } else {
                1
            }
        }
    };

    let joined_game = match registry.join_game(game_id, &guest_sess) {
        Ok(g) => g,
        Err(e) => {
            tracing::warn!("[GameManager] Failed to join game {}: {}", game_id, e);
            let mut err_resp = BlazePacket::response(&req.header, TdfStruct::new())?;
            err_resp.header.error_code = 0x0066; // GAME_ERR_FULL / NOT_FOUND
            return Ok(err_resp);
        }
    };

    let host_sess = store.get_session(joined_game.host_user_id);
    if let Some(host) = host_sess {
        // 1. Notify joining player with complete GameSetup
        let notif_setup = build_notify_game_setup(&joined_game, &host, Some(&guest_sess))?;
        store.send_notification(session.user_id, notif_setup);

        // 2. Notify host player that peer joined
        let notif_joining = build_notify_player_joining(&joined_game, &guest_sess)?;
        store.send_notification(host.user_id, notif_joining);
    }

    let mut resp = TdfStruct::new();
    resp.set("GID", TdfValue::Integer(joined_game.game_id));
    BlazePacket::response(&req.header, resp)
}

/// Handles startMatchmaking (0x0007).
pub fn handle_start_matchmaking(
    req: &BlazePacket,
    session: &SessionState,
    store: &SessionStore,
    registry: &GameRegistry,
) -> io::Result<BlazePacket> {
    let player_sess = store.get_session(session.user_id).unwrap_or_else(|| {
        store.create_session(Some(&session.display_name))
    });

    let target_room = store.get_target_room_code(session.user_id);

    let (game, is_joined) = if let Some(code) = target_room {
        // Flow A: Room-Code Target Matchmaking
        if let Some(existing) = registry.find_by_room_code(&code) {
            match registry.join_game(existing.game_id, &player_sess) {
                Ok(joined) => (joined, true),
                Err(_) => (registry.create_game(&player_sess, Some(code)), false),
            }
        } else {
            (registry.create_game(&player_sess, Some(code)), false)
        }
    } else {
        // Flow B: Native Auto-Matchmaking
        registry.find_or_create_auto_match(&player_sess)
    };

    if is_joined {
        // Broadcast setup to both
        if let Some(host_sess) = store.get_session(game.host_user_id) {
            let notif_host = build_notify_player_joining(&game, &player_sess)?;
            store.send_notification(host_sess.user_id, notif_host);

            let notif_guest = build_notify_game_setup(&game, &host_sess, Some(&player_sess))?;
            store.send_notification(session.user_id, notif_guest);
        }
    } else {
        // Push initial setup to host
        let notif_created = build_notify_game_created(&game)?;
        let notif_setup = build_notify_game_setup(&game, &player_sess, None)?;
        store.send_notification(session.user_id, notif_created);
        store.send_notification(session.user_id, notif_setup);
    }

    let mut resp = TdfStruct::new();
    resp.set("MSID", TdfValue::Integer(game.game_id));
    BlazePacket::response(&req.header, resp)
}

/// Handles cancelMatchmaking (0x0008).
pub fn handle_cancel_matchmaking(req: &BlazePacket) -> io::Result<BlazePacket> {
    BlazePacket::response(&req.header, TdfStruct::new())
}

/// Handles finalizeGameCreation (0x000B) & setGameSettings (0x000C).
pub fn handle_finalize_game_creation(
    req: &BlazePacket,
    session: &SessionState,
    store: &SessionStore,
    registry: &GameRegistry,
) -> io::Result<BlazePacket> {
    if let Some(game) = registry.find_by_player(session.user_id) {
        registry.advance_state(game.game_id, GAME_STATE_IN_LOBBY, store);
    }
    BlazePacket::response(&req.header, TdfStruct::new())
}

/// Handles updateGameAttributes (0x0016).
pub fn handle_update_game_attributes(
    req: &BlazePacket,
    session: &SessionState,
    store: &SessionStore,
    registry: &GameRegistry,
) -> io::Result<BlazePacket> {
    if let Some(game) = registry.find_by_player(session.user_id) {
        let mut new_attrs = HashMap::new();
        if let Some(TdfValue::Map(map)) = req.payload.get("ATTR") {
            for (k, v) in &map.entries {
                if let (TdfValue::String(ks), TdfValue::String(vs)) = (k, v) {
                    new_attrs.insert(ks.clone(), vs.clone());
                }
            }
        }

        if let Some(g) = registry.update_attributes(game.game_id, new_attrs.clone()) {
            let notif = build_notify_game_attrib_change(g.game_id, &new_attrs)?;
            let notif_0018 = build_notify_game_attrib_change_0018(g.game_id, &new_attrs)?;
            let mut participants = vec![g.host_user_id];
            if let Some(gid) = g.guest_user_id {
                participants.push(gid);
            }
            store.broadcast_notification(&participants, notif);
            store.broadcast_notification(&participants, notif_0018);
        }
    }
    BlazePacket::response(&req.header, TdfStruct::new())
}

/// Handles updatePlayerState (0x0014).
pub fn handle_update_player_state(
    req: &BlazePacket,
    session: &SessionState,
    store: &SessionStore,
    registry: &GameRegistry,
) -> io::Result<BlazePacket> {
    let state = match req.payload.get("STAT") {
        Some(TdfValue::Integer(st)) => *st as u16,
        _ => GAME_STATE_IN_LOBBY,
    };

    if let Some(game) = registry.find_by_player(session.user_id) {
        let mut participants = vec![game.host_user_id];
        if let Some(gid) = game.guest_user_id {
            participants.push(gid);
        }

        // 1. Notify individual player state change (0x0074)
        let notif_player = build_notify_player_state_change(game.game_id, session.persona_id, state)?;
        store.broadcast_notification(&participants, notif_player);

        // 2. Symmetric Kickoff state machine check - centralized in advance_state
        if let Some((g, state_transition)) = registry.update_player_state(game.game_id, session.user_id, state) {
            if let Some(new_state) = state_transition {
                tracing::info!(
                    "[GameManager] Kickoff State Machine: Game {} reached state {} (HostState={}, GuestState={})",
                    g.game_id,
                    new_state,
                    g.host_state,
                    g.guest_state
                );
                registry.advance_state(g.game_id, new_state, store);
            }
        }
    }

    BlazePacket::response(&req.header, TdfStruct::new())
}

/// Handles destroyGame (0x0002).
pub fn handle_destroy_game(
    req: &BlazePacket,
    session: &SessionState,
    store: &SessionStore,
    registry: &GameRegistry,
) -> io::Result<BlazePacket> {
    let target_game = match req.payload.get("GID") {
        Some(TdfValue::Integer(gid)) => registry.games.read().get(gid).cloned(),
        _ => registry.find_by_player(session.user_id),
    };

    if let Some(game) = target_game {
        if let Some(removed) = registry.destroy_game(game.game_id) {
            let notif = build_notify_game_removed(removed.game_id)?;
            let notif_alt = build_notify_game_removed_alt(removed.game_id)?;
            let mut participants = vec![removed.host_user_id];
            if let Some(gid) = removed.guest_user_id {
                participants.push(gid);
            }
            store.broadcast_notification(&participants, notif);
            store.broadcast_notification(&participants, notif_alt);
        }
    }
    BlazePacket::response(&req.header, TdfStruct::new())
}

/// Notifies both peers in a game to switch to ProtoTunnel relay-only transport.
pub fn notify_relay_mode_switch(
    game_id: u64,
    registry: &GameRegistry,
    store: &SessionStore,
) -> io::Result<()> {
    let mut attrs = HashMap::new();
    attrs.insert("ATTR_FORCE_RELAY".to_string(), "1".to_string());
    attrs.insert("ATTR_RELAY_TRANSPORT".to_string(), "ProtoTunnel".to_string());

    if let Some(g) = registry.update_attributes(game_id, attrs.clone()) {
        let notif = build_notify_game_attrib_change(g.game_id, &attrs)?;
        let notif_0018 = build_notify_game_attrib_change_0018(g.game_id, &attrs)?;
        let mut participants = vec![g.host_user_id];
        if let Some(gid) = g.guest_user_id {
            participants.push(gid);
        }
        store.broadcast_notification(&participants, notif);
        store.broadcast_notification(&participants, notif_0018);
        tracing::info!("[GameManager] Auto-switched to relay-only: Broadcast ATTR_FORCE_RELAY=1 to peers in game {}", game_id);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn test_game_registry_room_code_and_matchmaking() {
        let store = Arc::new(SessionStore::new());
        let registry = Arc::new(GameRegistry::new());

        let p1 = store.create_session(Some("Player1"));
        let p2 = store.create_session(Some("Player2"));

        // Player 1 auto-match (creates game)
        let (g1, is_joined1) = registry.find_or_create_auto_match(&p1);
        assert!(!is_joined1);
        assert_eq!(g1.host_user_id, p1.user_id);
        assert_eq!(g1.room_code.len(), 6);

        // Player 2 auto-match (matches into Player 1's game)
        let (g2, is_joined2) = registry.find_or_create_auto_match(&p2);
        assert!(is_joined2);
        assert_eq!(g2.game_id, g1.game_id);
        assert_eq!(g2.guest_user_id, Some(p2.user_id));

        // Test room code lookup
        let found = registry.find_by_room_code(&g1.room_code).unwrap();
        assert_eq!(found.game_id, g1.game_id);

        // Test attributes update
        let mut attrs = HashMap::new();
        attrs.insert("ATTR_SQDH".to_string(), "SYNCED_HASH_123".to_string());
        let updated = registry.update_attributes(g1.game_id, attrs).unwrap();
        assert_eq!(
            updated.attributes.get("ATTR_SQDH"),
            Some(&"SYNCED_HASH_123".to_string())
        );

        // Test destroy
        let destroyed = registry.destroy_game(g1.game_id).unwrap();
        assert_eq!(destroyed.game_id, g1.game_id);
        assert!(registry.find_by_room_code(&g1.room_code).is_none());
    }

    #[test]
    fn test_deterministic_seed_and_squad_hash_verification() {
        let store = Arc::new(SessionStore::new());
        let registry = Arc::new(GameRegistry::new());
        let p1 = store.create_session(Some("HostPlayer"));

        let game = registry.create_game(&p1, None);
        assert!(game.seed > 0);
        assert_eq!(
            game.attributes.get("ATTR_SEED"),
            Some(&game.seed.to_string())
        );

        let canonical_hash = services_http_content::SquadStore::global().get_canonical_hash();
        assert_eq!(
            game.attributes.get("ATTR_SQDH"),
            Some(&canonical_hash)
        );
        assert!(game.squad_hash_synced);

        // Update with matching squad hash
        let mut matching = HashMap::new();
        matching.insert("ATTR_SQDH".to_string(), canonical_hash.clone());
        let updated_ok = registry.update_attributes(game.game_id, matching).unwrap();
        assert!(updated_ok.squad_hash_synced);

        // Update with mismatched squad hash
        let mut mismatched = HashMap::new();
        mismatched.insert("ATTR_SQDH".to_string(), "corrupted_or_outdated_hash_404".to_string());
        let updated_bad = registry.update_attributes(game.game_id, mismatched).unwrap();
        assert!(!updated_bad.squad_hash_synced);
    }

    #[test]
    fn test_team_and_kit_attribute_broadcasts() {
        let store = Arc::new(SessionStore::new());
        let registry = Arc::new(GameRegistry::new());
        let p1 = store.create_session(Some("Host"));

        let game = registry.create_game(&p1, None);

        let mut team_attrs = HashMap::new();
        team_attrs.insert("ATTR_TEAM_HOME".to_string(), "243".to_string()); // Real Madrid
        team_attrs.insert("ATTR_TEAM_AWAY".to_string(), "9".to_string());   // Liverpool
        team_attrs.insert("ATTR_KITS".to_string(), "1".to_string());

        let updated = registry.update_attributes(game.game_id, team_attrs.clone()).unwrap();
        assert_eq!(updated.attributes.get("ATTR_TEAM_HOME"), Some(&"243".to_string()));
        assert_eq!(updated.attributes.get("ATTR_TEAM_AWAY"), Some(&"9".to_string()));
        assert_eq!(updated.attributes.get("ATTR_KITS"), Some(&"1".to_string()));

        // Verify both 0x0050 and 0x0018 notification packet formats build cleanly
        let notif_0050 = build_notify_game_attrib_change(game.game_id, &team_attrs).unwrap();
        assert_eq!(notif_0050.header.component, COMPONENT_GAME_MANAGER);
        assert_eq!(notif_0050.header.command, NOTIFY_GAME_ATTRIB_CHANGE);

        let notif_0018 = build_notify_game_attrib_change_0018(game.game_id, &team_attrs).unwrap();
        assert_eq!(notif_0018.header.component, COMPONENT_GAME_MANAGER);
        assert_eq!(notif_0018.header.command, NOTIFY_GAME_ATTRIB_CHANGE_ALT);
    }

    #[test]
    fn test_symmetric_kickoff_state_machine() {
        let store = Arc::new(SessionStore::new());
        let registry = Arc::new(GameRegistry::new());

        let p1 = store.create_session(Some("HostPlayer"));
        let p2 = store.create_session(Some("GuestPlayer"));

        let (game, _) = registry.find_or_create_auto_match(&p1);
        let (joined_game, is_joined) = registry.find_or_create_auto_match(&p2);
        assert!(is_joined);
        assert_eq!(joined_game.game_state, GAME_STATE_IN_LOBBY);

        // Host transitions to PRE_GAME (Guest still IN_LOBBY) -> Session state remains IN_LOBBY
        let (g, transition1) = registry
            .update_player_state(game.game_id, p1.user_id, GAME_STATE_PRE_GAME)
            .unwrap();
        assert_eq!(g.host_state, GAME_STATE_PRE_GAME);
        assert_eq!(g.guest_state, GAME_STATE_INITIALIZING);
        assert_eq!(g.game_state, GAME_STATE_IN_LOBBY);
        assert_eq!(transition1, None);

        // Guest transitions to PRE_GAME -> Session symmetrically reaches PRE_GAME
        let (g, transition2) = registry
            .update_player_state(game.game_id, p2.user_id, GAME_STATE_PRE_GAME)
            .unwrap();
        assert_eq!(g.guest_state, GAME_STATE_PRE_GAME);
        assert_eq!(g.game_state, GAME_STATE_PRE_GAME);
        assert_eq!(transition2, Some(GAME_STATE_PRE_GAME));

        // Host transitions to IN_GAME (ready for kickoff) -> Session remains PRE_GAME
        let (g, transition3) = registry
            .update_player_state(game.game_id, p1.user_id, GAME_STATE_IN_GAME)
            .unwrap();
        assert_eq!(g.host_state, GAME_STATE_IN_GAME);
        assert_eq!(g.game_state, GAME_STATE_PRE_GAME);
        assert_eq!(transition3, None);

        // Guest transitions to IN_GAME -> Symmetric Kickoff triggered!
        let (g, transition4) = registry
            .update_player_state(game.game_id, p2.user_id, GAME_STATE_IN_GAME)
            .unwrap();
        assert_eq!(g.guest_state, GAME_STATE_IN_GAME);
        assert_eq!(g.game_state, GAME_STATE_IN_GAME);
        assert_eq!(transition4, Some(GAME_STATE_IN_GAME));
    }

    #[test]
    fn test_destroy_game_and_session_deallocation() {
        let store = Arc::new(SessionStore::new());
        let registry = Arc::new(GameRegistry::new());

        let p1 = store.create_session(Some("HostDestroyer"));
        let p2 = store.create_session(Some("GuestVictim"));

        let (game, _) = registry.find_or_create_auto_match(&p1);
        let (_, is_joined) = registry.find_or_create_auto_match(&p2);
        assert!(is_joined);

        // Verify player mappings
        assert!(registry.find_by_player(p1.user_id).is_some());
        assert!(registry.find_by_player(p2.user_id).is_some());
        assert!(registry.find_by_room_code(&game.room_code).is_some());

        // Destroy Game
        let destroyed = registry.destroy_game(game.game_id).unwrap();
        assert_eq!(destroyed.game_id, game.game_id);

        // Verify total cleanup and deallocation
        assert!(registry.find_by_player(p1.user_id).is_none());
        assert!(registry.find_by_player(p2.user_id).is_none());
        assert!(registry.find_by_room_code(&game.room_code).is_none());

        // Verify notification builders for 0x000A, 0x0010, and 0x0020
        let notif_000a = build_notify_game_removed(game.game_id).unwrap();
        assert_eq!(notif_000a.header.command, NOTIFY_GAME_REMOVED);

        let notif_0010 = build_notify_game_removed_alt(game.game_id).unwrap();
        assert_eq!(notif_0010.header.command, NOTIFY_GAME_REMOVED_ALT);

        let notif_0020 = build_notify_player_removed(game.game_id, p1.persona_id, 0).unwrap();
        assert_eq!(notif_0020.header.command, NOTIFY_PLAYER_REMOVED);
    }

    #[test]
    fn test_handle_player_disconnect() {
        let store = Arc::new(SessionStore::new());
        let registry = Arc::new(GameRegistry::new());

        let p1 = store.create_session(Some("DroppingHost"));
        let p2 = store.create_session(Some("RemainingGuest"));

        let (game, _) = registry.find_or_create_auto_match(&p1);
        let _ = registry.find_or_create_auto_match(&p2);

        // Host drops connection
        let (removed, remaining) = registry.handle_player_disconnect(p1.user_id).unwrap();
        assert_eq!(removed.game_id, game.game_id);
        assert_eq!(remaining, vec![p2.user_id]);

        // Verify registry is cleanly freed
        assert!(registry.find_by_player(p1.user_id).is_none());
        assert!(registry.find_by_player(p2.user_id).is_none());
    }

    #[test]
    fn test_notification_sequence_strict_ordering() {
        let mut seq = NotificationSequence::new(42);
        seq.advance_to(SequenceStep::NotifyPlayerJoining);
        seq.advance_to(SequenceStep::NotifyJoiningPlayerInitiateConnections);
        seq.advance_to(SequenceStep::NotifyPlatformHostInitialized);
        seq.advance_to(SequenceStep::NotifyGameReportingIdChange);
        seq.advance_to(SequenceStep::GameState130);
        seq.advance_to(SequenceStep::GameState131);
        assert_eq!(seq.current_step, Some(SequenceStep::GameState131));
    }

    #[test]
    #[should_panic(expected = "Strict NotificationSequence violation")]
    fn test_notification_sequence_out_of_order_panics() {
        let mut seq = NotificationSequence::new(99);
        seq.advance_to(SequenceStep::NotifyPlayerJoining);
        // Skip steps directly to GameState130 - must panic in debug!
        seq.advance_to(SequenceStep::GameState130);
    }

    #[test]
    fn test_deadlock_guard_records_and_dumps_hex() {
        let guard = DeadlockGuard::new();
        for i in 0..25 {
            let dummy = vec![0x00, 0x01, i as u8];
            guard.record_message("IN", COMPONENT_GAME_MANAGER, 0x0014, &dummy);
        }
        let recent = guard.dump_recent();
        assert_eq!(recent.len(), 20, "DeadlockGuard ring buffer must cap at 20 messages");
        assert_eq!(recent[0].command, 0x0014);

        guard.start_waiting_for_match_ready(101);
        // Not yet 5s elapsed
        assert!(!guard.check_deadlock(101));
        guard.match_ready_achieved(101);
    }

    #[test]
    fn test_notify_relay_mode_switch_broadcasts_attributes() {
        let store = Arc::new(SessionStore::new());
        let registry = Arc::new(GameRegistry::new());

        let p1 = store.create_session(Some("HostP1"));
        let p2 = store.create_session(Some("GuestP2"));

        let (game, _) = registry.find_or_create_auto_match(&p1);
        let _ = registry.find_or_create_auto_match(&p2);

        let res = notify_relay_mode_switch(game.game_id, &registry, &store);
        assert!(res.is_ok());

        let updated = registry.find_by_player(p1.user_id).unwrap();
        assert_eq!(updated.attributes.get("ATTR_FORCE_RELAY"), Some(&"1".to_string()));
        assert_eq!(updated.attributes.get("ATTR_RELAY_TRANSPORT"), Some(&"ProtoTunnel".to_string()));
    }
}
