//! # Authentication Component (0x0001) Handlers.

use std::io;
use blaze_core::BlazePacket;
use blaze_tdf::{TdfList, TdfStruct, TdfType, TdfValue};
use crate::session::SessionState;
use crate::session_store::SessionStore;

pub const COMPONENT_AUTH: u16 = 0x0001;

pub const CMD_EXPRESS_LOGIN: u16 = 0x001C;
pub const CMD_LOGIN: u16 = 0x0028;
pub const CMD_SILENT_LOGIN: u16 = 0x0029;
pub const CMD_LIST_USER_ENTITLEMENTS: u16 = 0x0046;

/// Handles login, expressLogin, and silentLogin backed by the dynamic SessionStore.
pub fn handle_login(
    req: &BlazePacket,
    session: &mut SessionState,
    store: &SessionStore,
) -> io::Result<BlazePacket> {
    // Extract requested persona name if present in login payload
    let requested_name = match req.payload.get("DSNM") {
        Some(TdfValue::String(s)) => Some(s.as_str()),
        _ => match req.payload.get("NAME") {
            Some(TdfValue::String(s)) => Some(s.as_str()),
            _ => None,
        },
    };

    let session_data = store.create_session(requested_name);
    session.user_id = session_data.user_id;
    session.persona_id = session_data.persona_id;
    session.display_name = session_data.persona_name.clone();
    session.session_key = session_data.session_key.clone();
    session.is_logged_in = true;

    let mut resp = TdfStruct::new();
    resp.set("SKEY", TdfValue::String(session.session_key.clone()));
    resp.set("UID", TdfValue::Integer(session.user_id));
    resp.set("PID", TdfValue::Integer(session.persona_id));
    resp.set("DSNM", TdfValue::String(session.display_name.clone()));
    resp.set("MAIL", TdfValue::String(format!("{}@f22emu.local", session.display_name)));
    resp.set("PVER", TdfValue::Integer(1));
    resp.set("LTIM", TdfValue::Integer(1700000000));

    // Nested SessionInfo struct
    let mut sess_info = TdfStruct::new();
    sess_info.set("BUID", TdfValue::Integer(session.user_id));
    sess_info.set("FRST", TdfValue::Integer(0));
    sess_info.set("KEY", TdfValue::String(session.session_key.clone()));
    sess_info.set("LLOG", TdfValue::Integer(1700000000));
    sess_info.set("MAIL", TdfValue::String(format!("{}@f22emu.local", session.display_name)));
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
/// Conforms precisely to FIFA 22 Frostbite 3 Heat2 schema for Blaze::Authentication::Entitlements.
pub fn handle_list_user_entitlements(
    req: &BlazePacket,
    session: &SessionState,
) -> io::Result<BlazePacket> {
    let mut resp = TdfStruct::new();

    // Standard online entitlement item exactly matching FIFA 22 Blaze 15 schema
    let mut entitlement = TdfStruct::new();
    entitlement.set("DEVI", TdfValue::String(String::new()));
    entitlement.set("GDAY", TdfValue::String("2021-10-01T00:00:00.000Z".to_string()));
    entitlement.set("GNAM", TdfValue::String("FIFA22".to_string()));
    entitlement.set("ID", TdfValue::Integer(session.user_id));
    entitlement.set("ISCO", TdfValue::Integer(0));
    entitlement.set("PID", TdfValue::Integer(session.persona_id));
    entitlement.set("PJID", TdfValue::Integer(0));
    entitlement.set("PRCA", TdfValue::String("ONLINE_ACCESS".to_string()));
    entitlement.set("PRID", TdfValue::String("ONLINE_ACCESS".to_string()));
    entitlement.set("STAT", TdfValue::Integer(1));
    entitlement.set("STRC", TdfValue::String(String::new()));
    entitlement.set("TAG", TdfValue::String("ONLINE_ACCESS".to_string()));
    entitlement.set("TDAY", TdfValue::String(String::new()));
    entitlement.set("TYPE", TdfValue::Integer(1));
    entitlement.set("UCNT", TdfValue::Integer(0));
    entitlement.set("VER", TdfValue::Integer(0));

    let items = vec![TdfValue::Struct(entitlement)];
    resp.set("NLST", TdfValue::List(TdfList::new(TdfType::Struct, items)));

    tracing::info!(
        "[Blaze Auth] Handled listUserEntitlements for '{}' (UID={}, PID={})",
        session.display_name,
        session.user_id,
        session.persona_id
    );
    BlazePacket::response(&req.header, resp)
}

