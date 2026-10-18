//! # GameManager Component (0x0004) & Game Session Engine
//!
//! Orchestrates P2P lobbies, Room Code reservations, native auto-matchmaking,
//! game attributes, and cross-peer async notification broadcasts.

use std::collections::HashMap;
use std::io;
use std::sync::atomic::{AtomicU64, Ordering};
use parking_lot::RwLock;

use blaze_core::BlazePacket;
use blaze_tdf::{TdfList, TdfMap, TdfStruct, TdfType, TdfUnion, TdfValue};
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

/// Game Session State Lifecycle
pub const GAME_STATE_INITIALIZING: u16 = 1;
pub const GAME_STATE_IN_LOBBY: u16 = 2;
pub const GAME_STATE_PRE_GAME: u16 = 3;
pub const GAME_STATE_IN_GAME: u16 = 4;
pub const GAME_STATE_POST_GAME: u16 = 5;

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
}

/// Global registry managing all active multiplayer game sessions.
pub struct GameRegistry {
    next_game_id: AtomicU64,
    games: RwLock<HashMap<u64, GameSession>>,
    room_map: RwLock<HashMap<String, u64>>,
    player_game_map: RwLock<HashMap<u64, u64>>,
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
        attributes.insert("ATTR_TEAM_HOME".to_string(), "0".to_string());
        attributes.insert("ATTR_TEAM_AWAY".to_string(), "0".to_string());
        attributes.insert("ATTR_KITS".to_string(), "0".to_string());
        attributes.insert("ATTR_HALF_LEN".to_string(), "6".to_string());

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
        } else if Some(user_id) == game.guest_user_id {
            game.guest_state = new_state;
        }

        let mut state_transition = None;

        if game.guest_user_id.is_some() {
            if game.host_state >= GAME_STATE_IN_GAME && game.guest_state >= GAME_STATE_IN_GAME {
                if game.game_state != GAME_STATE_IN_GAME {
                    game.game_state = GAME_STATE_IN_GAME;
                    state_transition = Some(GAME_STATE_IN_GAME);
                }
            } else if game.host_state >= GAME_STATE_PRE_GAME && game.guest_state >= GAME_STATE_PRE_GAME {
                if game.game_state != GAME_STATE_PRE_GAME && game.game_state < GAME_STATE_IN_GAME {
                    game.game_state = GAME_STATE_PRE_GAME;
                    state_transition = Some(GAME_STATE_PRE_GAME);
                }
            }
        } else {
            if game.host_state >= GAME_STATE_IN_GAME && game.game_state != GAME_STATE_IN_GAME {
                game.game_state = GAME_STATE_IN_GAME;
                state_transition = Some(GAME_STATE_IN_GAME);
            } else if game.host_state >= GAME_STATE_PRE_GAME && game.game_state < GAME_STATE_PRE_GAME {
                game.game_state = GAME_STATE_PRE_GAME;
                state_transition = Some(GAME_STATE_PRE_GAME);
            }
        }

        Some((game.clone(), state_transition))
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
    game_struct.set("GNAM", TdfValue::String("FIFA22 Match".to_string()));
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

    // Host network address inside PRES
    let mut host_addr = TdfStruct::new();
    host_addr.set("HOST", TdfValue::String(host.external_ip.clone()));
    host_addr.set("PORT", TdfValue::Integer(host.port as u64));
    game_struct.set(
        "PRES",
        TdfValue::Union(TdfUnion::with_value(0, TdfValue::Struct(host_addr))),
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

    let mut addr = TdfStruct::new();
    addr.set("HOST", TdfValue::String(joining.external_ip.clone()));
    addr.set("PORT", TdfValue::Integer(joining.port as u64));
    player.set(
        "ADDR",
        TdfValue::Union(TdfUnion::with_value(0, TdfValue::Struct(addr))),
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
        let updated = registry.update_state(game.game_id, GAME_STATE_IN_LOBBY);
        if let Some(g) = updated {
            let notif = build_notify_game_state_change(g.game_id, GAME_STATE_IN_LOBBY)?;
            let mut participants = vec![g.host_user_id];
            if let Some(gid) = g.guest_user_id {
                participants.push(gid);
            }
            store.broadcast_notification(&participants, notif);
        }
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

        // 2. Symmetric Kickoff state machine check
        if let Some((g, state_transition)) = registry.update_player_state(game.game_id, session.user_id, state) {
            if let Some(new_state) = state_transition {
                tracing::info!(
                    "[GameManager] Kickoff State Machine: Game {} reached state {} (HostState={}, GuestState={})",
                    g.game_id,
                    new_state,
                    g.host_state,
                    g.guest_state
                );
                let notif_game = build_notify_game_state_change(g.game_id, new_state)?;
                store.broadcast_notification(&participants, notif_game);
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
}
