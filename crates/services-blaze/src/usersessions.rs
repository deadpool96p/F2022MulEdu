//! # UserSessions Component (0x7802 / 0x0019) Handlers
//!
//! Manages network address registration, NAT type reporting, and user lookups.

use std::io;
use blaze_core::BlazePacket;
use blaze_tdf::{TdfList, TdfStruct, TdfType, TdfUnion, TdfValue};
use crate::session::SessionState;
use crate::session_store::SessionStore;

pub const COMPONENT_USER_SESSIONS: u16 = 0x7802;
pub const COMPONENT_USERS: u16 = 0x0019;

pub const CMD_LOOKUP_USER: u16 = 0x0001;
pub const CMD_LOOKUP_USERS: u16 = 0x0003;
pub const CMD_UPDATE_NETWORK_INFO: u16 = 0x0008;
pub const CMD_UPDATE_CLIENT_DATA: u16 = 0x0014;

pub const NOTIFY_USER_SESSION_EXTENDED_DATA: u16 = 0x0001;
pub const NOTIFY_USER_ADDED: u16 = 0x0002;
pub const NOTIFY_USER_REMOVED: u16 = 0x0003;
pub const NOTIFY_USER_UPDATED: u16 = 0x0005;

/// Builds the unsolicited NotifyUserAdded (0x7802:0x0002) notification.
/// This informs BlazeSDK's UserSessionManager of the authenticated user's session.
pub fn build_notify_user_added(session: &SessionState) -> io::Result<BlazePacket> {
    let mut root = TdfStruct::new();

    // 1. DATA: UserSessionExtendedData
    let mut data = TdfStruct::new();

    let mut addr_struct = TdfStruct::new();
    addr_struct.set("HOST", TdfValue::String(session.external_ip.clone()));
    addr_struct.set("IP", TdfValue::Integer(0x7F000001)); // 127.0.0.1
    addr_struct.set("PORT", TdfValue::Integer(session.port as u64));
    data.set(
        "ADDR",
        TdfValue::Union(TdfUnion::with_value(0x00, TdfValue::Struct(addr_struct))),
    );

    data.set("BPS", TdfValue::String("bio-iad".to_string()));
    data.set("CTY", TdfValue::Integer(1)); // CLIENT_TYPE_GAMEPLAY_USER
    data.set("HWFL", TdfValue::Integer(0));
    data.set("UID", TdfValue::Integer(session.user_id));

    let mut qos = TdfStruct::new();
    qos.set("BWIN", TdfValue::Integer(1000000));
    qos.set("BWOT", TdfValue::Integer(1000000));
    qos.set("DBW", TdfValue::Integer(0));
    qos.set("NATT", TdfValue::Integer(session.nat_type as u64));
    qos.set("UBW", TdfValue::Integer(0));
    data.set("QOS", TdfValue::Struct(qos));

    root.set("DATA", TdfValue::Struct(data));

    // 2. USER: UserData
    let mut user = TdfStruct::new();
    user.set("AID", TdfValue::Integer(session.user_id));
    user.set(
        "ALST",
        TdfValue::List(TdfList::new(
            TdfType::Integer,
            vec![TdfValue::Integer(session.user_id)],
        )),
    );
    user.set("EXID", TdfValue::Integer(session.persona_id));
    user.set("ID", TdfValue::Integer(session.user_id));
    user.set("NAME", TdfValue::String(session.display_name.clone()));
    user.set("PID", TdfValue::Integer(session.persona_id));
    root.set("USER", TdfValue::Struct(user));

    tracing::info!(
        "[UserSessions] Built NotifyUserAdded for '{}' (UID={}, PID={})",
        session.display_name,
        session.user_id,
        session.persona_id
    );

    BlazePacket::notification(COMPONENT_USER_SESSIONS, NOTIFY_USER_ADDED, root)
}

/// Handles updateNetworkInfo (0x0008).
/// Returns the RPC response AND an async UserSessionExtendedDataUpdate notification.
pub fn handle_update_network_info(
    req: &BlazePacket,
    session: &mut SessionState,
    store: &SessionStore,
) -> io::Result<(BlazePacket, Option<BlazePacket>)> {
    if let Some(TdfValue::Union(addr_union)) = req.payload.get("ADDR") {
        if let Some(box_val) = &addr_union.value {
            if let TdfValue::Struct(addr_struct) = box_val.as_ref() {
                if let Some(TdfValue::Integer(port)) = addr_struct.get("PORT") {
                    session.port = *port as u16;
                }
            }
        }
    }

    if let Some(TdfValue::Integer(nat)) = req.payload.get("NATT") {
        session.nat_type = *nat as u8;
    }

    // Update global store
    store.update_network_info(
        session.user_id,
        &session.external_ip,
        &session.internal_ip,
        session.port,
        session.nat_type,
    );

    tracing::info!(
        "[UserSessions] Updated Network Info for {}: Port={}, NAT={}",
        session.display_name,
        session.port,
        session.nat_type
    );

    // 1. Success response
    let response = BlazePacket::response(&req.header, TdfStruct::new())?;

    // 2. Async UserSessionExtendedDataUpdate notification
    let mut notif_tdf = TdfStruct::new();
    notif_tdf.set("BUID", TdfValue::Integer(session.user_id));
    notif_tdf.set("FRST", TdfValue::Integer(0));
    notif_tdf.set("KEY", TdfValue::String(session.session_key.clone()));
    notif_tdf.set("UID", TdfValue::Integer(session.user_id));

    // Address info inside notification
    let mut ip_struct = TdfStruct::new();
    ip_struct.set("HOST", TdfValue::String(session.external_ip.clone()));
    ip_struct.set("PORT", TdfValue::Integer(session.port as u64));
    notif_tdf.set(
        "ADDR",
        TdfValue::Union(TdfUnion::with_value(0x00, TdfValue::Struct(ip_struct))),
    );

    let notification = BlazePacket::notification(
        COMPONENT_USER_SESSIONS,
        NOTIFY_USER_SESSION_EXTENDED_DATA,
        notif_tdf,
    )?;

    Ok((response, Some(notification)))
}

/// Handles lookupUsers (0x0003).
/// Returns active players and their network info.
pub fn handle_lookup_users(
    req: &BlazePacket,
    store: &SessionStore,
) -> io::Result<BlazePacket> {
    let mut resp = TdfStruct::new();
    let all_players = store.list_active_sessions();
    let mut user_list = Vec::new();

    for player in all_players {
        let mut user_entry = TdfStruct::new();
        user_entry.set("FLGS", TdfValue::Integer(0));
        user_entry.set("ID", TdfValue::Integer(player.user_id));
        user_entry.set("NAME", TdfValue::String(player.persona_name.clone()));
        user_entry.set("STUS", TdfValue::Integer(2)); // 2 = Online

        // Extended data struct (EDAT)
        let mut edat = TdfStruct::new();
        edat.set("NATT", TdfValue::Integer(player.nat_type as u64));
        edat.set("PORT", TdfValue::Integer(player.port as u64));

        let mut addr_struct = TdfStruct::new();
        addr_struct.set("HOST", TdfValue::String(player.external_ip.clone()));
        addr_struct.set("PORT", TdfValue::Integer(player.port as u64));
        edat.set(
            "ADDR",
            TdfValue::Union(TdfUnion::with_value(0x00, TdfValue::Struct(addr_struct))),
        );

        user_entry.set("EDAT", TdfValue::Struct(edat));
        user_list.push(TdfValue::Struct(user_entry));
    }

    resp.set("ULST", TdfValue::List(TdfList::new(TdfType::Struct, user_list)));

    tracing::info!(
        "[UserSessions] Handled lookupUsers -> returned {} active players",
        store.count()
    );

    BlazePacket::response(&req.header, resp)
}

/// Handles lookupUser (0x0001).
pub fn handle_lookup_user(
    req: &BlazePacket,
    current_session: &SessionState,
    store: &SessionStore,
) -> io::Result<BlazePacket> {
    let mut resp = TdfStruct::new();

    let target_id = match req.payload.get("UID") {
        Some(TdfValue::Integer(uid)) => *uid,
        _ => current_session.user_id,
    };

    let target_sess = store.get_session(target_id).unwrap_or_else(|| {
        crate::session_store::SessionData {
            user_id: current_session.user_id,
            persona_id: current_session.persona_id,
            persona_name: current_session.display_name.clone(),
            session_key: current_session.session_key.clone(),
            external_ip: current_session.external_ip.clone(),
            internal_ip: current_session.internal_ip.clone(),
            port: current_session.port,
            nat_type: current_session.nat_type,
            steam_id: None,
            steam_name: None,
            online_friends: Vec::new(),
            target_room_code: None,
            steam_lobby_id: None,
            last_seen: std::time::Instant::now(),
        }
    });

    resp.set("FLGS", TdfValue::Integer(0));
    resp.set("ID", TdfValue::Integer(target_sess.user_id));
    resp.set("NAME", TdfValue::String(target_sess.persona_name));
    resp.set("STUS", TdfValue::Integer(2));

    BlazePacket::response(&req.header, resp)
}

/// Handles updateClientData (0x0014).
pub fn handle_update_client_data(req: &BlazePacket) -> io::Result<BlazePacket> {
    BlazePacket::response(&req.header, TdfStruct::new())
}
