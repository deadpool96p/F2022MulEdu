//! UserSessions Component (0x7802 / 0x0019) Handlers.

use std::io;
use blaze_core::BlazePacket;
use blaze_tdf::{TdfStruct, TdfUnion, TdfValue};
use crate::session::SessionState;

pub const COMPONENT_USER_SESSIONS: u16 = 0x7802;
pub const COMPONENT_USERS: u16 = 0x0019;

pub const CMD_UPDATE_NETWORK_INFO: u16 = 0x0008;
pub const CMD_UPDATE_CLIENT_DATA: u16 = 0x0014;

pub const NOTIFY_USER_SESSION_EXTENDED_DATA: u16 = 0x0001;

/// Handles updateNetworkInfo (0x0008).
/// Returns the RPC response AND an async UserSessionExtendedDataUpdate notification.
pub fn handle_update_network_info(
    req: &BlazePacket,
    session: &mut SessionState,
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

/// Handles updateClientData (0x0014).
pub fn handle_update_client_data(req: &BlazePacket) -> io::Result<BlazePacket> {
    BlazePacket::response(&req.header, TdfStruct::new())
}
