//! Authentication Component (0x0001) Handlers.

use std::io;
use blaze_core::BlazePacket;
use blaze_tdf::{TdfList, TdfStruct, TdfType, TdfValue};
use crate::session::SessionState;

pub const COMPONENT_AUTH: u16 = 0x0001;

pub const CMD_EXPRESS_LOGIN: u16 = 0x001C;
pub const CMD_LOGIN: u16 = 0x0028;
pub const CMD_SILENT_LOGIN: u16 = 0x0029;
pub const CMD_LIST_USER_ENTITLEMENTS: u16 = 0x0046;

/// Handles login, expressLogin, and silentLogin.
pub fn handle_login(req: &BlazePacket, session: &mut SessionState) -> io::Result<BlazePacket> {
    session.is_logged_in = true;

    let mut resp = TdfStruct::new();
    resp.set("SKEY", TdfValue::String(session.session_key.clone()));
    resp.set("UID", TdfValue::Integer(session.user_id));
    resp.set("PID", TdfValue::Integer(session.persona_id));
    resp.set("DSNM", TdfValue::String(session.display_name.clone()));
    resp.set("MAIL", TdfValue::String("player@f22emu.local".to_string()));
    resp.set("PVER", TdfValue::Integer(1));
    resp.set("LTIM", TdfValue::Integer(1700000000));

    // Nested SessionInfo struct
    let mut sess_info = TdfStruct::new();
    sess_info.set("BUID", TdfValue::Integer(session.user_id));
    sess_info.set("FRST", TdfValue::Integer(0));
    sess_info.set("KEY", TdfValue::String(session.session_key.clone()));
    sess_info.set("LLOG", TdfValue::Integer(1700000000));
    sess_info.set("MAIL", TdfValue::String("player@f22emu.local".to_string()));
    sess_info.set("PERSONA", TdfValue::String(session.display_name.clone()));
    sess_info.set("UID", TdfValue::Integer(session.user_id));
    resp.set("SESS", TdfValue::Struct(sess_info));

    tracing::info!(
        "[Blaze Auth] User '{}' logged in successfully (UID={}, PID={}, SessionKey={})",
        session.display_name,
        session.user_id,
        session.persona_id,
        session.session_key
    );

    BlazePacket::response(&req.header, resp)
}

/// Handles listUserEntitlements (0x0046).
pub fn handle_list_user_entitlements(req: &BlazePacket) -> io::Result<BlazePacket> {
    let mut resp = TdfStruct::new();

    // Standard online entitlement item
    let mut entitlement = TdfStruct::new();
    entitlement.set("DEAT", TdfValue::Integer(0));
    entitlement.set("GNAM", TdfValue::String("FIFA22".to_string()));
    entitlement.set("ISCO", TdfValue::Integer(0));
    entitlement.set("ISST", TdfValue::Integer(0));
    entitlement.set("NAME", TdfValue::String("ONLINE_ACCESS".to_string()));
    entitlement.set("STAT", TdfValue::Integer(1));
    entitlement.set("TAG", TdfValue::String("ONLINE_ACCESS".to_string()));
    entitlement.set("TYPE", TdfValue::Integer(1));
    entitlement.set("VERS", TdfValue::Integer(0));

    let items = vec![TdfValue::Struct(entitlement)];
    resp.set("NLST", TdfValue::List(TdfList::new(TdfType::Struct, items)));

    tracing::info!("[Blaze Auth] Handled listUserEntitlements");
    BlazePacket::response(&req.header, resp)
}
