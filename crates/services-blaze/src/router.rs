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
        let mut res = match (req.header.component, req.header.command) {
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
            (COMPONENT_UTIL, CMD_SET_CLIENT_METRICS) => {
                let resp = handle_set_client_metrics(req)?;
                Ok((resp, None))
            }
            (COMPONENT_UTIL, CMD_GET_TELEMETRY_SERVER) => {
                let resp = handle_get_telemetry_server(req)?;
                Ok((resp, None))
            }
            (COMPONENT_UTIL, CMD_GET_TICKER_SERVER) => {
                let resp = handle_get_ticker_server(req)?;
                Ok((resp, None))
            }
            (COMPONENT_UTIL, CMD_FILTER_FOR_PROFANITY) => {
                let resp = handle_filter_for_profanity(req)?;
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
            //
            // The routing mirrors EA's own `AuthenticationRouter`
            // (`references/Skate3BlazeServer/.../Blaze/Routers/AuthenticationRouter.cs`):
            // `acceptTos` and `getEntitlements` get an empty response,
            // `createWalUserSession` gets EA's own response model, and every
            // other command gets `ERR_COMMAND_NOT_FOUND`.
            //
            // The emulator used to answer *any* `Authentication` command with
            // a fabricated login response and an entitlement list. That is
            // what the live FIFA 22 capture shows going wrong: the client
            // sent `Comp=0x0001, Cmd=0x0046` -- `logout`, with an empty
            // payload -- and the reply it got back was a login response full
            // of members the client's `logout` schema does not have, so the
            // client could not decode it and retried every ~10 s.
            //
            // `logout` itself is answered with an empty **success**, not with
            // `ERR_COMMAND_NOT_FOUND`. The reference router does not
            // implement it, but the retail server does, and the live client
            // answers an errored `logout` by sending `logout` again -- the
            // capture shows a fresh Fire2 message number on every attempt
            // (0x29, 0x2A, 0x2B, ... 0x2F), so these are new requests, not
            // retransmissions.
            (COMPONENT_AUTH, CMD_ACCEPT_TOS) => {
                ensure_session(req, &mut self.session, &self.store);
                let resp = handle_accept_tos(req)?;
                let notif_pkt = build_notify_user_added(&self.session)?;
                Ok((resp, Some(notif_pkt)))
            }
            (COMPONENT_AUTH, CMD_GET_ENTITLEMENTS) => {
                let resp = handle_get_entitlements(req)?;
                Ok((resp, None))
            }
            (COMPONENT_AUTH, CMD_LOGOUT) => {
                let resp = handle_logout(req)?;
                // The client considers itself logged out now, so the local
                // session must go with it.  The identity fields are kept so
                // that a command arriving before the next `ensure_session`
                // still has a usable user to book against.
                self.session.is_logged_in = false;
                Ok((resp, None))
            }
            (COMPONENT_AUTH, CMD_CREATE_WAL_USER_SESSION) => {
                ensure_session(req, &mut self.session, &self.store);
                let resp = handle_create_wal_user_session(req, &self.session)?;
                let notif_pkt = build_notify_user_added(&self.session)?;
                Ok((resp, Some(notif_pkt)))
            }
            (COMPONENT_AUTH, _) => {
                let resp = handle_command_not_found(req)?;
                Ok((resp, None))
            }

            // UserSessions Component (0x7802 / 0x0019)
            //
            // `updateNetworkInfo` is where a client registers itself with
            // the server, so it is also where the emulator creates its local
            // user if the client has not established one through
            // `acceptTos` or `createWalUserSession`.
            (COMPONENT_USER_SESSIONS, CMD_UPDATE_NETWORK_INFO)
            | (COMPONENT_USERS, CMD_UPDATE_NETWORK_INFO) => {
                ensure_session(req, &mut self.session, &self.store);
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
            // PerfComponent (0x08BA) — FifaOnline::PerfComponentClient.
            (0x08BA, _) => {
                tracing::info!(
                    "[Blaze Router] Handled PerfComponent 0x08BA Command 0x{:04X} (Seq={}) \
                     -> empty PerfResponse",
                    req.header.command,
                    req.header.seq_no
                );
                let resp_pkt = BlazePacket::response(&req.header, TdfStruct::new())?;
                Ok((resp_pkt, None))
            }

            // Cards / Store / FUT Components: guarantee error_code = 0 (Success)
            (0x000F, _) | (0x0010, _) | (0x0018, _) | (0x0021, _) => {
                tracing::info!(
                    "[Blaze Router] Intercepted Cards/FUT/Store Component 0x{:04X} Command 0x{:04X} (Seq={}) -> Force Success (0)",
                    req.header.component,
                    req.header.command,
                    req.header.seq_no
                );
                let resp = BlazePacket::response(&req.header, TdfStruct::new())?;
                Ok((resp, None))
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
        };

        // Post-processing EOS Safety Guard:
        // Under no circumstances allow an error_code of 0x006F (111 / CARDS_CB_ERR_SERVICE_UNAVAILABLE)
        // or 0x0197 (407 / CARDS_CB_ERR_PLAYER_OFFLINE) to be emitted to the client.
        if let Ok((ref mut resp, _)) = res {
            if resp.header.error_code == 0x006F || resp.header.error_code == 0x0197 {
                tracing::warn!(
                    "[Blaze Router] Intercepted fatal error code 0x{:04X} on Component 0x{:04X} Command 0x{:04X}! Overriding to 0 (Success)",
                    resp.header.error_code,
                    req.header.component,
                    req.header.command
                );
                resp.header.error_code = 0;
            }
        }

        res
    }
}

