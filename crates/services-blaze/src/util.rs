//! Util Component (0x0009) Handlers.

use std::io;
use std::time::{SystemTime, UNIX_EPOCH};
use blaze_core::{BlazeHeader, BlazePacket, MessageType};
use blaze_tdf::{TdfMap, TdfStruct, TdfType, TdfValue};

pub const COMPONENT_UTIL: u16 = 0x0009;

pub const CMD_FETCH_CLIENT_CONFIG: u16 = 0x0001;
pub const CMD_PING: u16 = 0x0002;
pub const CMD_SET_CLIENT_DATA: u16 = 0x0003;
pub const CMD_USER_SETTINGS_SAVE: u16 = 0x0007;
pub const CMD_USER_SETTINGS_LOAD: u16 = 0x0008;
pub const CMD_USER_SETTINGS_LOAD_ALL: u16 = 0x000A;
pub const CMD_PRE_AUTH: u16 = 0x000B;
pub const CMD_POST_AUTH: u16 = 0x000C;

pub const COMPONENT_STATS: u16 = 0x0007;
pub const COMPONENT_CENSUS_DATA: u16 = 0x000A;

/// Handles preAuth (0x000B).
pub fn handle_pre_auth(req: &BlazePacket, qos_host: &str, qos_port: u16) -> io::Result<BlazePacket> {
    let mut resp = TdfStruct::new();

    resp.set("ANAM", TdfValue::String("fifa-2022-pc".to_string()));

    // CIDS: List of supported component IDs
    let components = vec![
        TdfValue::Integer(0x0001), // Authentication
        TdfValue::Integer(0x0004), // GameManager
        TdfValue::Integer(0x0007), // Stats
        TdfValue::Integer(0x0009), // Util
        TdfValue::Integer(0x000A), // CensusData
        TdfValue::Integer(0x0019), // Users
        TdfValue::Integer(0x001C), // GameReporting
        TdfValue::Integer(0x08BA), // PerfComponent
        TdfValue::Integer(0x7802), // UserSessions
    ];
    resp.set("CIDS", TdfValue::List(blaze_tdf::TdfList::new(TdfType::Integer, components)));

    // QOSS: QoS server configuration pointing to emulator QoS/STUN responder
    let mut qos_struct = TdfStruct::new();
    qos_struct.set("BWHR", TdfValue::Integer(1000));
    qos_struct.set("LATT", TdfValue::Integer(20));

    let mut psa_struct = TdfStruct::new();
    psa_struct.set("HOST", TdfValue::String(qos_host.to_string()));
    psa_struct.set("PORT", TdfValue::Integer(qos_port as u64));
    qos_struct.set("PSA", TdfValue::Struct(psa_struct));
    qos_struct.set("SKEY", TdfValue::String("QOS_SESSION_KEY".to_string()));
    qos_struct.set("VERS", TdfValue::Integer(1));
    resp.set("QOSS", TdfValue::Struct(qos_struct));

    resp.set("SVER", TdfValue::String("Blaze 15.1.1.2.0 (F22-Emulator)".to_string()));

    let epoch_sec = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    resp.set("TIME", TdfValue::Integer(epoch_sec));

    tracing::info!("[Blaze Util] Handled preAuth -> QoS directed to {}:{}", qos_host, qos_port);
    BlazePacket::response(&req.header, resp)
}

/// Handles postAuth (0x000C).
pub fn handle_post_auth(req: &BlazePacket) -> io::Result<BlazePacket> {
    let mut resp = TdfStruct::new();

    // Telemetry server info
    let mut tele = TdfStruct::new();
    tele.set("ADRS", TdfValue::String("127.0.0.1".to_string()));
    tele.set("PORT", TdfValue::Integer(80));
    tele.set("SKEY", TdfValue::String("TELE_KEY".to_string()));
    resp.set("TELE", TdfValue::Struct(tele));

    // Ticker server info
    let mut tick = TdfStruct::new();
    tick.set("ADRS", TdfValue::String("127.0.0.1".to_string()));
    tick.set("PORT", TdfValue::Integer(80));
    tick.set("SKEY", TdfValue::String("TICK_KEY".to_string()));
    resp.set("TICK", TdfValue::Struct(tick));

    tracing::info!("[Blaze Util] Handled postAuth");
    BlazePacket::response(&req.header, resp)
}

/// Handles fetchClientConfig (0x0001).
pub fn handle_fetch_client_config(req: &BlazePacket) -> io::Result<BlazePacket> {
    let section = match req.payload.get("CFG") {
        Some(TdfValue::String(s)) => s.as_str(),
        _ => "FIFA_ONLINE",
    };

    let mut resp = TdfStruct::new();
    let mut entries = Vec::new();

    match section {
        "FIFA_ONLINE" => {
            entries.push((TdfValue::String("online_enabled".to_string()), TdfValue::String("1".to_string())));
            entries.push((TdfValue::String("matchmaking_enabled".to_string()), TdfValue::String("1".to_string())));
            entries.push((TdfValue::String("crossplay_enabled".to_string()), TdfValue::String("0".to_string())));
        }
        "PIN" => {
            entries.push((TdfValue::String("pin_enabled".to_string()), TdfValue::String("0".to_string())));
        }
        _ => {
            entries.push((TdfValue::String("status".to_string()), TdfValue::String("ok".to_string())));
        }
    }

    resp.set("CONF", TdfValue::Map(TdfMap::new(TdfType::String, TdfType::String, entries)));

    tracing::info!("[Blaze Util] Handled fetchClientConfig for section '{}'", section);
    BlazePacket::response(&req.header, resp)
}

/// Handles ping (0x0002).
///
/// Per .agents/rules/blaze_protocol_invariants.md rule #2: in FIFA 22 BlazeSDK
/// 15.1.1.10.3, Blaze::Util::PingResponse has 0 member fields in its Heat2 schema.
/// Sending unexpected tags (e.g. STIM) triggers BLAZE_ERR_TDF_TAG_NOT_FOUND in the
/// Heat2Decoder, which is what caused the 10s watchdog EOF. Response MUST be an
/// empty struct (length 0, RecordResponse).
pub fn handle_ping(req: &BlazePacket) -> io::Result<BlazePacket> {
    tracing::info!(
        "[Blaze Util] Handled ping (Seq={}) -> empty PingResponse (0 fields per Heat2 schema)",
        req.header.seq_no
    );
    BlazePacket::response(&req.header, TdfStruct::new())
}

/// Handles setClientData (0x0003).
pub fn handle_set_client_data(req: &BlazePacket) -> io::Result<BlazePacket> {
    tracing::debug!("[Blaze Util] setClientData acknowledged");
    BlazePacket::response(&req.header, TdfStruct::new())
}

/// Handles userSettingsLoad (0x0008).
pub fn handle_user_settings_load(req: &BlazePacket) -> io::Result<BlazePacket> {
    let mut resp = TdfStruct::new();
    resp.set("DATA", TdfValue::String(String::new()));
    BlazePacket::response(&req.header, resp)
}

/// Handles userSettingsSave (0x0007).
pub fn handle_user_settings_save(req: &BlazePacket) -> io::Result<BlazePacket> {
    BlazePacket::response(&req.header, TdfStruct::new())
}

/// Handles userSettingsLoadAll (0x000A).
pub fn handle_user_settings_load_all(req: &BlazePacket) -> io::Result<BlazePacket> {
    let mut resp = TdfStruct::new();
    let entries: Vec<(TdfValue, TdfValue)> = Vec::new();
    resp.set("SMAP", TdfValue::Map(TdfMap::new(TdfType::String, TdfType::String, entries)));
    BlazePacket::response(&req.header, resp)
}

/// Handles CensusData component queries.
pub fn handle_census_data(req: &BlazePacket, active_users: usize) -> io::Result<BlazePacket> {
    let mut resp = TdfStruct::new();
    resp.set("CENS", TdfValue::Integer(std::cmp::max(1, active_users as u64)));
    BlazePacket::response(&req.header, resp)
}

#[cfg(test)]
mod tests {
    use super::*;
    use blaze_core::{BlazePacket, MessageType};

    fn make_header(seq: u16) -> BlazeHeader {
        BlazeHeader::new(0, COMPONENT_UTIL, CMD_PING, 0x0000, MessageType::SubmitRequest, 0, seq)
    }

    /// Regression test for the 10s watchdog EOF.
    ///
    /// In FIFA 22 BlazeSDK 15.1.1.10.3, Blaze::Util::PingResponse has 0 member
    /// fields in its Heat2 schema. Any unexpected tag (e.g. STIM) is rejected by
    /// Heat2Decoder with BLAZE_ERR_TDF_TAG_NOT_FOUND, which drops the connection
    /// ~10s later. The response MUST encode as an empty struct.
    #[test]
    fn test_ping_response_is_empty_struct_no_stim() {
        let req = BlazePacket::new(
            COMPONENT_UTIL,
            CMD_PING,
            0,
            MessageType::SubmitRequest,
            0,
            42,
            TdfStruct::new(),
        )
        .unwrap();

        let resp = handle_ping(&req).expect("ping handler must not error");
        assert_eq!(resp.header.component, COMPONENT_UTIL);
        assert_eq!(resp.header.command, CMD_PING);
        assert_eq!(resp.header.error_code, 0x0000);
        assert_eq!(resp.header.msg_type, MessageType::RecordResponse);
        assert_eq!(resp.header.seq_no, 42);
        assert!(
            resp.payload.fields.is_empty(),
            "PingResponse must be an empty TDF struct (0 fields); got keys {:?}",
            resp.payload.fields.keys().collect::<Vec<_>>()
        );
    }
}

/// Handles Stats component queries.
pub fn handle_stats(req: &BlazePacket) -> io::Result<BlazePacket> {
    BlazePacket::response(&req.header, TdfStruct::new())
}
