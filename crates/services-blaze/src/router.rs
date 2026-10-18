//! Blaze RPC Packet Router.

use std::io;
use blaze_core::BlazePacket;
use blaze_tdf::TdfStruct;
use crate::auth::*;
use crate::session::SessionState;
use crate::usersessions::*;
use crate::util::*;

/// Central router dispatching Blaze RPC requests for a connected client session.
pub struct BlazeRouter {
    pub session: SessionState,
    pub qos_host: String,
    pub qos_port: u16,
}

impl BlazeRouter {
    pub fn new(qos_host: String, qos_port: u16) -> Self {
        Self {
            session: SessionState::default(),
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

            // Authentication Component (0x0001)
            (COMPONENT_AUTH, CMD_EXPRESS_LOGIN)
            | (COMPONENT_AUTH, CMD_LOGIN)
            | (COMPONENT_AUTH, CMD_SILENT_LOGIN) => {
                let resp = handle_login(req, &mut self.session)?;
                Ok((resp, None))
            }
            (COMPONENT_AUTH, CMD_LIST_USER_ENTITLEMENTS) => {
                let resp = handle_list_user_entitlements(req)?;
                Ok((resp, None))
            }

            // UserSessions Component (0x7802 / 0x0019)
            (COMPONENT_USER_SESSIONS, CMD_UPDATE_NETWORK_INFO)
            | (COMPONENT_USERS, CMD_UPDATE_NETWORK_INFO) => {
                handle_update_network_info(req, &mut self.session)
            }
            (COMPONENT_USER_SESSIONS, CMD_UPDATE_CLIENT_DATA)
            | (COMPONENT_USERS, CMD_UPDATE_CLIENT_DATA) => {
                let resp = handle_update_client_data(req)?;
                Ok((resp, None))
            }

            // Fallback for unhandled commands (respond ERR_OK to avoid blocking game initialization)
            (comp, cmd) => {
                tracing::warn!(
                    "[Blaze Router] Unhandled RPC -> Component 0x{:04X}, Command 0x{:04X}",
                    comp,
                    cmd
                );
                let resp = BlazePacket::response(&req.header, TdfStruct::new())?;
                Ok((resp, None))
            }
        }
    }
}
