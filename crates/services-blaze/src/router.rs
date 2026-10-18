//! Blaze RPC Packet Router.

use std::io;
use std::sync::Arc;
use blaze_core::BlazePacket;
use blaze_tdf::TdfStruct;
use crate::associations::*;
use crate::auth::*;
use crate::session::SessionState;
use crate::session_store::SessionStore;
use crate::usersessions::*;
use crate::util::*;

/// Central router dispatching Blaze RPC requests for a connected client session.
pub struct BlazeRouter {
    pub session: SessionState,
    pub store: Arc<SessionStore>,
    pub qos_host: String,
    pub qos_port: u16,
}

impl BlazeRouter {
    pub fn new(qos_host: String, qos_port: u16, store: Arc<SessionStore>) -> Self {
        Self {
            session: SessionState::default(),
            store,
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
            (COMPONENT_AUTH, CMD_EXPRESS_LOGIN)
            | (COMPONENT_AUTH, CMD_LOGIN)
            | (COMPONENT_AUTH, CMD_SILENT_LOGIN) => {
                let resp = handle_login(req, &mut self.session, &self.store)?;
                Ok((resp, None))
            }
            (COMPONENT_AUTH, CMD_LIST_USER_ENTITLEMENTS) => {
                let resp = handle_list_user_entitlements(req)?;
                Ok((resp, None))
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

            // Stats Component (0x0007)
            (COMPONENT_STATS, _) => {
                let resp = handle_stats(req)?;
                Ok((resp, None))
            }

            // CensusData Component (0x000A)
            (COMPONENT_CENSUS_DATA, _) => {
                let resp = handle_census_data(req, self.store.count())?;
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
        }
    }
}
