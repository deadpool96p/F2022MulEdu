//! Blaze RPC Packet Router.

use std::io;
use std::sync::Arc;
use blaze_core::BlazePacket;
use blaze_tdf::TdfStruct;
use crate::associations::*;
use crate::auth::*;
use crate::gamemanager::*;
use crate::session::SessionState;
use crate::session_store::SessionStore;
use crate::stats::*;
use crate::usersessions::*;
use crate::util::*;

/// Central router dispatching Blaze RPC requests for a connected client session.
pub struct BlazeRouter {
    pub session: SessionState,
    pub store: Arc<SessionStore>,
    pub games: Arc<GameRegistry>,
    pub qos_host: String,
    pub qos_port: u16,
}

impl BlazeRouter {
    pub fn new(
        qos_host: String,
        qos_port: u16,
        store: Arc<SessionStore>,
        games: Arc<GameRegistry>,
    ) -> Self {
        Self {
            session: SessionState::default(),
            store,
            games,
            qos_host,
            qos_port,
        }
    }

    /// Dispatches an incoming request packet and returns the response plus an optional async notification.
    pub fn handle_packet(
        &mut self,
        req: &BlazePacket,
    ) -> io::Result<(BlazePacket, Option<BlazePacket>)> {
        match (req.header.component, req.header.command) {
            // Util Component (0x0009)
            (COMPONENT_UTIL, CMD_PRE_AUTH) => {
                let resp = handle_pre_auth(req, &self.qos_host, self.qos_port)?;
                Ok((resp, None))
            }
            (COMPONENT_UTIL, CMD_POST_AUTH) => {
                let resp = handle_post_auth(req)?;
                Ok((resp, None))
            }
            (COMPONENT_UTIL, CMD_FETCH_CLIENT_CONFIG) => {
                let resp = handle_fetch_client_config(req)?;
                Ok((resp, None))
            }
            (COMPONENT_UTIL, CMD_PING) => {
                let resp = handle_ping(req)?;
                Ok((resp, None))
            }
            (COMPONENT_UTIL, CMD_SET_CLIENT_DATA) => {
                let resp = handle_set_client_data(req)?;
                Ok((resp, None))
            }
            (COMPONENT_UTIL, CMD_USER_SETTINGS_LOAD) => {
                let resp = handle_user_settings_load(req)?;
                Ok((resp, None))
            }
            (COMPONENT_UTIL, CMD_USER_SETTINGS_SAVE) => {
                let resp = handle_user_settings_save(req)?;
                Ok((resp, None))
            }
            (COMPONENT_UTIL, CMD_USER_SETTINGS_LOAD_ALL) => {
                let resp = handle_user_settings_load_all(req)?;
                Ok((resp, None))
            }

            // Authentication Component (0x0001)
            (COMPONENT_AUTH, CMD_LIST_USER_ENTITLEMENTS)
            | (COMPONENT_AUTH, 0x0032)
            | (COMPONENT_AUTH, 0x0047)
            | (COMPONENT_AUTH, 0x005A) => {
                let notif = if !self.session.is_logged_in {
                    let _ = handle_login(req, &mut self.session, &self.store)?;
                    let notif_pkt = build_notify_user_added(&self.session)?;
                    Some(notif_pkt)
                } else {
                    None
                };
                let resp = handle_list_user_entitlements(req, &self.session)?;
                Ok((resp, notif))
            }
            (COMPONENT_AUTH, _) => {
                let resp = handle_login(req, &mut self.session, &self.store)?;
                let notif_pkt = build_notify_user_added(&self.session)?;
                Ok((resp, Some(notif_pkt)))
            }

            // UserSessions Component (0x7802 / 0x0019)
            (COMPONENT_USER_SESSIONS, CMD_UPDATE_NETWORK_INFO)
            | (COMPONENT_USERS, CMD_UPDATE_NETWORK_INFO) => {
                handle_update_network_info(req, &mut self.session, &self.store)
            }
            (COMPONENT_USER_SESSIONS, CMD_LOOKUP_USERS) => {
                let resp = handle_lookup_users(req, &self.store)?;
                Ok((resp, None))
            }
            (COMPONENT_USER_SESSIONS, CMD_LOOKUP_USER) => {
                let resp = handle_lookup_user(req, &self.session, &self.store)?;
                Ok((resp, None))
            }
            (COMPONENT_USER_SESSIONS, CMD_UPDATE_CLIENT_DATA)
            | (COMPONENT_USERS, CMD_UPDATE_CLIENT_DATA) => {
                let resp = handle_update_client_data(req)?;
                Ok((resp, None))
            }

            // AssociationLists Component (0x0019)
            (COMPONENT_ASSOCIATION_LISTS, CMD_GET_LISTS)
            | (COMPONENT_ASSOCIATION_LISTS, CMD_SUBSCRIBE_TO_LISTS)
            | (COMPONENT_ASSOCIATION_LISTS, CMD_GET_MEMBERS) => {
                let resp = handle_get_lists(req, self.session.user_id, &self.store)?;
                Ok((resp, None))
            }

            // GameManager Component (0x0004)
            (COMPONENT_GAME_MANAGER, CMD_CREATE_GAME) => {
                let resp = handle_create_game(req, &self.session, &self.store, &self.games)?;
                Ok((resp, None))
            }
            (COMPONENT_GAME_MANAGER, CMD_JOIN_GAME) => {
                let resp = handle_join_game(req, &self.session, &self.store, &self.games)?;
                Ok((resp, None))
            }
            (COMPONENT_GAME_MANAGER, CMD_START_MATCHMAKING) => {
                let resp = handle_start_matchmaking(req, &self.session, &self.store, &self.games)?;
                Ok((resp, None))
            }
            (COMPONENT_GAME_MANAGER, CMD_CANCEL_MATCHMAKING) => {
                let resp = handle_cancel_matchmaking(req)?;
                Ok((resp, None))
            }
            (COMPONENT_GAME_MANAGER, CMD_FINALIZE_GAME_CREATION)
            | (COMPONENT_GAME_MANAGER, CMD_SET_GAME_SETTINGS) => {
                let resp =
                    handle_finalize_game_creation(req, &self.session, &self.store, &self.games)?;
                Ok((resp, None))
            }
            (COMPONENT_GAME_MANAGER, CMD_UPDATE_GAME_ATTRIBUTES) => {
                let resp =
                    handle_update_game_attributes(req, &self.session, &self.store, &self.games)?;
                Ok((resp, None))
            }
            (COMPONENT_GAME_MANAGER, CMD_UPDATE_PLAYER_STATE) => {
                let resp =
                    handle_update_player_state(req, &self.session, &self.store, &self.games)?;
                Ok((resp, None))
            }
            (COMPONENT_GAME_MANAGER, CMD_DESTROY_GAME) => {
                let resp = handle_destroy_game(req, &self.session, &self.store, &self.games)?;
                Ok((resp, None))
            }

            // Stats Component (0x0007)
            (COMPONENT_STATS, CMD_GET_STATS) => {
                let resp = handle_get_stats(req, &self.session)?;
                Ok((resp, None))
            }
            (COMPONENT_STATS, CMD_UPDATE_STATS) => {
                let resp = handle_update_stats(req)?;
                Ok((resp, None))
            }
            (COMPONENT_STATS, CMD_GET_LEADERBOARD)
            | (COMPONENT_STATS, CMD_GET_CENTERED_LEADERBOARD)
            | (COMPONENT_STATS, CMD_GET_FILTERED_LEADERBOARD) => {
                let resp = handle_get_leaderboard(req)?;
                Ok((resp, None))
            }
            (COMPONENT_STATS, CMD_SUBMIT_GAME_REPORT) => {
                let resp = handle_submit_game_report(req, &self.session, &self.store)?;
                Ok((resp, None))
            }
            (COMPONENT_STATS, _) => {
                let resp = handle_update_stats(req)?;
                Ok((resp, None))
            }

            // CensusData Component (0x000A)
            (COMPONENT_CENSUS_DATA, _) => {
                let resp = handle_census_data(req, self.store.count())?;
                Ok((resp, None))
            }

            // PerfComponent (0x08BA) — FifaOnline::PerfComponentClient.
            //
            // The client batches 0x08BA/0x0005 together with Util::ping (0x0009/0x0002)
            // inside a single 32-byte segment (two 16-byte Fire2 frames), which marks it
            // as a synchronous perf probe rather than an independent RPC. Following the
            // same Heat2 schema invariant as Util::PingResponse (rule #2), we respond with
            // an EMPTY struct. STIM is NOT a valid tag here — emitting it triggers
            // BLAZE_ERR_TDF_TAG_NOT_FOUND and the 10s watchdog EOF.
            //
            // HYPOTHESIS (unverified — no Ghidra-confirmed 0x08BA/0x0005 schema exists yet;
            // see docs/disassembly_and_engine_multiplayer_plan.md Phase 3 gap). If the real
            // schema requires fields, capture the wire bytes with tools/dissector/blaze.lua
            // in Wireshark and replace this empty response with the exact tags.
            (0x08BA, _) => {
                tracing::info!(
                    "[Blaze Router] Handled PerfComponent 0x08BA Command 0x{:04X} (Seq={}) \
                     -> empty PerfResponse (STIM hypothesis: not a valid Heat2 tag here)",
                    req.header.command,
                    req.header.seq_no
                );
                let resp_pkt = BlazePacket::response(&req.header, TdfStruct::new())?;
                Ok((resp_pkt, None))
            }

            // Wildcard Unknown-RPC Fallback: log payload and return ERR_OK
            (comp, cmd) => {
                tracing::info!(
                    "[Blaze Router] Wildcard Auto-OK -> Component 0x{:04X}, Command 0x{:04X}, Seq={}, PayloadKeys={:?}",
                    comp,
                    cmd,
                    req.header.seq_no,
                    req.payload.fields.keys()
                );
                let resp = BlazePacket::response(&req.header, TdfStruct::new())?;
                Ok((resp, None))
            }
        }
    }
}
