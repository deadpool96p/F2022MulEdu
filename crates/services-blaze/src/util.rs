//! Util Component (0x0009) Handlers.

use std::io;
use std::time::{SystemTime, UNIX_EPOCH};
use blaze_core::BlazePacket;
use blaze_tdf::{TdfMap, TdfStruct, TdfType, TdfValue};

pub const COMPONENT_UTIL: u16 = 0x0009;

// The `Util` component's command IDs, from EA's own `UtilMessage` enum
// (`references/Skate3BlazeServer/.../Blaze/MessageLists/UtilMessages.cs`).
//
// These were wrong here, and the failure was silent. A Blaze server
// dispatches on the numeric command ID alone, so a command ID that is off by
// a few does not raise an error -- it answers the wrong request with the
// wrong reply. A live capture of FIFA 22 shows the client opening the
// session with `Comp=0x0009, Cmd=0x0007` and a 230-byte client-identity
// payload, which this build answered as `userSettingsSave` with an empty
// payload. The client never received its `preAuth` response and fell into a
// three-second reconnect loop.
//
// EA's own `PreAuthHandler` builds its response with the literal header
// bytes `0, 0, 0, 9, 0, 7, 0, 0, 0, 0, 0, 0` -- component `0x0009`, command
// `0x0007` -- which is `preAuth`.
pub const CMD_FETCH_CLIENT_CONFIG: u16 = 0x0001;
pub const CMD_PING: u16 = 0x0002;
pub const CMD_SET_CLIENT_DATA: u16 = 0x0003;
pub const CMD_LOCALIZE_STRINGS: u16 = 0x0004;
pub const CMD_GET_TELEMETRY_SERVER: u16 = 0x0005;
pub const CMD_GET_TICKER_SERVER: u16 = 0x0006;
pub const CMD_PRE_AUTH: u16 = 0x0007;
pub const CMD_POST_AUTH: u16 = 0x0008;
pub const CMD_USER_SETTINGS_LOAD: u16 = 0x000A;
pub const CMD_USER_SETTINGS_SAVE: u16 = 0x000B;
pub const CMD_USER_SETTINGS_LOAD_ALL: u16 = 0x000C;
pub const CMD_USER_SETTINGS_LOAD_ALL_FOR_USER_ID: u16 = 0x000D;
pub const CMD_FILTER_FOR_PROFANITY: u16 = 0x0014;
pub const CMD_FETCH_QOS_CONFIG: u16 = 0x0015;
pub const CMD_SET_CLIENT_METRICS: u16 = 0x0016;
pub const CMD_SET_CONNECTION_STATE: u16 = 0x0017;

pub const COMPONENT_STATS: u16 = 0x0007;
pub const COMPONENT_CENSUS_DATA: u16 = 0x000A;

/// Builds a `QosPingSiteInfo` struct.
///
/// EA's schema (`QosPingSiteInfo` in
/// `references/Skate3BlazeServer/.../Util/Models/QosPingSiteInfo.cs`):
///
/// ```text
/// PSA : string   (address)
/// PSP : ushort   (port)
/// SNA : string   (site name)
/// ```
fn qos_ping_site(address: &str, port: u16, site_name: &str) -> TdfStruct {
    let mut site = TdfStruct::new();
    site.set("PSA", TdfValue::String(address.to_string()));
    site.set("PSP", TdfValue::Integer(port as u64));
    site.set("SNA", TdfValue::String(site_name.to_string()));
    site
}

/// Builds a `FetchConfigResponse`.
///
/// EA's schema (`references/Skate3BlazeServer/.../Util/Commands/
/// FetchConfigResponse.cs`) is a struct with a *single* member, `CONF`,
/// which is the configuration map itself:
///
/// ```text
/// FetchConfigResponse
///   CONF : map<string, string>
/// ```
///
/// So on the wire the configuration is a struct *wrapping* a map, not a bare
/// map. The two authentic EA `preAuth` replies confirm it -- each carries
/// `CONF (struct)` whose only member is `CONF (map)`.
fn fetch_config_response(entries: Vec<(TdfValue, TdfValue)>) -> TdfStruct {
    let mut conf = TdfStruct::new();
    conf.set(
        "CONF",
        TdfValue::Map(TdfMap::new(TdfType::String, TdfType::String, entries)),
    );
    conf
}

/// The client configuration map EA's server ships in the `preAuth` reply.
///
/// The keys and their values are taken from the two authentic EA `preAuth`
/// replies (Blaze 15.1.1.4.6 and 18.3.0), with two changes:
///
///  * every host is rewritten to `127.0.0.1`, so the client can never reach
///    a real EA service, and
///  * the keys that carry third-party credentials or third-party endpoints
///    (`voipSTT*`, `voipTTS*`, `xbl*`, `xboxOne*`) are dropped entirely --
///    the emulator speaks no Watson and no Live services.
fn client_config_entries() -> Vec<(TdfValue, TdfValue)> {
    let entry = |k: &str, v: &str| (TdfValue::String(k.to_string()), TdfValue::String(v.to_string()));

    vec![
        entry("associationListSkipInitialSet", "1"),
        entry("autoReconnectEnabled", "0"),
        entry("cachedUserRefreshInterval", "1s"),
        entry("clientUserMetricsUpdateRate", "60000"),
        entry("connIdleTimeout", "40s"),
        entry("defaultRequestTimeout", "20s"),
        entry("maxReconnectAttempts", "30"),
        entry("nonResumableTimeoutScale", "2.0"),
        entry("pingPeriod", "20s"),
        entry("userManagerMaxCachedUsers", "0"),
        entry("voipHeadsetUpdateRate", "1000"),
        // The Nucleus and Bytevault services are served locally by the
        // emulator's own HTTP sink, which already answers the synthetic
        // Nucleus auth-token request.
        entry("nucleusConnect", "https://127.0.0.1"),
        entry("nucleusConnectTrusted", "https://127.0.0.1"),
        entry("nucleusPortal", "https://127.0.0.1"),
        entry("nucleusProxy", "https://127.0.0.1"),
        entry("bytevaultHostname", "127.0.0.1"),
        entry("bytevaultPort", "42127"),
        entry("bytevaultSecure", "true"),
    ]
}

/// Handles preAuth (0x0007).
///
/// `Util` command `0x0007` is `preAuth`. Three independent sources agree:
///
///  * EA's own `UtilMessage` enum
///    (`references/Skate3BlazeServer/.../Blaze/MessageLists/UtilMessages.cs`)
///    puts `preAuth` at 7;
///  * EA's own `PreAuthHandler`
///    (`references/Skate3BlazeServer/.../Util/Handlers/PreAuthHandler.cs`)
///    builds its response with the literal header bytes
///    `0, 0, 0, 9, 0, 7, 0, 0, 0, 0, 0, 0` -- component `0x0009`, command
///    `0x0007`; and
///  * the two authentic EA `preAuth` replies, whose payloads decode to
///    exactly the struct below.
///
/// The reply's members are the ones the authentic EA replies carry:
///
/// ```text
/// ASRC : string   asset source id
/// CIDS : list<uint>  component ids the server implements
/// CLID : string   server id
/// CONF : FetchConfigResponse { CONF : map<string, string> }
/// ESRC : string
/// INST : string   product id
/// MAID : uint     master account id
/// MINR : uint
/// NASP : string   authentication namespace
/// PILD : string
/// PLAT : string   platform
/// QOSS : QosConfigInfo { BWPS, LNP, LTPS, SVID }
/// RSRC : string
/// SVER : string   server version
/// ```
///
/// Note that `ANAM` and a root-level `TIME` are *not* members: the emulator
/// used to send both, and neither exists in the schema.
pub fn handle_pre_auth(req: &BlazePacket, qos_host: &str, qos_port: u16) -> io::Result<BlazePacket> {
    let mut resp = TdfStruct::new();

    // The context members. Their values are the client identity the FIFA 22
    // client reported in its own `preAuth` request (build 5091917, product
    // `fifa-2022-pc`, platform `pc`), so the server and the client agree.
    resp.set("ASRC", TdfValue::String("5091917".to_string()));
    resp.set("CLID", TdfValue::String("FIFA22-PC-EMULATOR-BLAZE".to_string()));
    resp.set("ESRC", TdfValue::String("5091917".to_string()));
    resp.set("INST", TdfValue::String("fifa-2022-pc".to_string()));
    resp.set("MAID", TdfValue::Integer(0));
    resp.set("MINR", TdfValue::Integer(0));
    resp.set("NASP", TdfValue::String("cem_ea_id".to_string()));
    resp.set("PILD", TdfValue::String(String::new()));
    resp.set("PLAT", TdfValue::String("pc".to_string()));
    resp.set("RSRC", TdfValue::String("5091917".to_string()));

    // CIDS: List of supported component IDs
    let components = vec![
        TdfValue::Integer(0x0001), // Authentication
        TdfValue::Integer(0x0004), // GameManager
        TdfValue::Integer(0x0007), // Stats
        TdfValue::Integer(0x0009), // Util
        TdfValue::Integer(0x000A), // CensusData
        TdfValue::Integer(0x000F), // Clubs
        TdfValue::Integer(0x0010), // Cards
        TdfValue::Integer(0x0018), // Stats/Achievements
        TdfValue::Integer(0x0019), // Users / AssociationLists
        TdfValue::Integer(0x001C), // GameReporting
        TdfValue::Integer(0x0021), // Store/Catalog
        TdfValue::Integer(0x08BA), // PerfComponent
        TdfValue::Integer(0x7802), // UserSessions
    ];
    resp.set("CIDS", TdfValue::List(blaze_tdf::TdfList::new(TdfType::Integer, components)));

    // CONF: the server's client configuration. A struct wrapping a map --
    // see `fetch_config_response`.
    resp.set("CONF", TdfValue::Struct(fetch_config_response(client_config_entries())));

    // QOSS: QoS server configuration pointing at the emulator's QoS
    // responder.
    //
    // The member names are EA's own, from the code-generated
    // `QosConfigInfo` (see
    // `references/Skate3BlazeServer/.../Util/Models/QosConfigInfo.cs`):
    //
    //   BWPS : QosPingSiteInfo          (the default ping site)
    //   LNP  : ushort                   (number of latency probes)
    //   LTPS : map<string, QosPingSiteInfo>
    //   SVID : uint                     (service id)
    //
    // and `QosPingSiteInfo` carries `PSA` (address), `PSP` (port) and
    // `SNA` (site name).
    let site = qos_ping_site(qos_host, qos_port, "emulator-qos");
    let mut qos_struct = TdfStruct::new();
    qos_struct.set("BWPS", TdfValue::Struct(site.clone()));
    qos_struct.set("LNP", TdfValue::Integer(2));
    qos_struct.set(
        "LTPS",
        TdfValue::Map(TdfMap::new(
            TdfType::String,
            TdfType::Struct,
            vec![(TdfValue::String("emulator-qos".to_string()), TdfValue::Struct(site))],
        )),
    );
    qos_struct.set("SVID", TdfValue::Integer(1));
    resp.set("QOSS", TdfValue::Struct(qos_struct));

    // SVER: the server version, in EA's own format
    // ("Blaze <sdk> (CL# <build>)").
    resp.set(
        "SVER",
        TdfValue::String("Blaze 15.1.1.10.3 (CL# 5091917)".to_string()),
    );

    tracing::info!("[Blaze Util] Handled preAuth -> QoS directed to {}:{}", qos_host, qos_port);
    BlazePacket::response(&req.header, resp)
}

/// Handles postAuth (0x0008).
///
/// `PostAuthResponse` carries two nested structs, per EA's own model
/// (`references/Skate3BlazeServer/.../Util/Commands/PostAuthResponse.cs`):
/// `TELE` is a `GetTelemetryServerResponse` and `TICK` is a
/// `GetTickerServerResponse`. Both point at the emulator's own HTTP sink on
/// port 443, so the client's telemetry never leaves the machine.
pub fn handle_post_auth(req: &BlazePacket) -> io::Result<BlazePacket> {
    let mut resp = TdfStruct::new();

    resp.set("TELE", TdfValue::Struct(telemetry_server()));
    resp.set("TICK", TdfValue::Struct(ticker_server()));

    tracing::info!("[Blaze Util] Handled postAuth");
    BlazePacket::response(&req.header, resp)
}

/// The `GetTelemetryServerResponse` struct, per EA's own model
/// (`references/Skate3BlazeServer/.../Util/Commands/GetTelemetryServerResponse.cs`).
///
/// Every member EA declares is present, even the ones the emulator has no
/// opinion about: a decoder that fills defaults for absent members is fine,
/// but one that requires them is not, and there is no cost to sending them.
fn telemetry_server() -> TdfStruct {
    let mut tele = TdfStruct::new();
    tele.set("ADRS", TdfValue::String("127.0.0.1".to_string()));
    tele.set("ANON", TdfValue::Integer(1));
    tele.set("DISA", TdfValue::String(String::new()));
    tele.set("FILT", TdfValue::String(String::new()));
    tele.set("LOC", TdfValue::Integer(0));
    tele.set("NOOK", TdfValue::String(String::new()));
    tele.set("PORT", TdfValue::Integer(443));
    tele.set("SDLY", TdfValue::Integer(0));
    tele.set("SKEY", TdfValue::String(String::new()));
    tele.set("SPCT", TdfValue::Integer(100));
    tele
}

/// The `GetTickerServerResponse` struct, per EA's own model
/// (`references/Skate3BlazeServer/.../Util/Commands/GetTickerServerResponse.cs`).
fn ticker_server() -> TdfStruct {
    let mut tick = TdfStruct::new();
    tick.set("ADRS", TdfValue::String("127.0.0.1".to_string()));
    tick.set("PORT", TdfValue::Integer(443));
    tick.set("SKEY", TdfValue::String(String::new()));
    tick
}

/// Handles getTelemetryServer (0x0005).
pub fn handle_get_telemetry_server(req: &BlazePacket) -> io::Result<BlazePacket> {
    let mut resp = TdfStruct::new();
    resp.set("TELE", TdfValue::Struct(telemetry_server()));
    tracing::info!("[Blaze Util] Handled getTelemetryServer");
    BlazePacket::response(&req.header, resp)
}

/// Handles getTickerServer (0x0006).
pub fn handle_get_ticker_server(req: &BlazePacket) -> io::Result<BlazePacket> {
    let mut resp = TdfStruct::new();
    resp.set("TICK", TdfValue::Struct(ticker_server()));
    tracing::info!("[Blaze Util] Handled getTickerServer");
    BlazePacket::response(&req.header, resp)
}

/// Handles filterForProfanity (0x0014).
///
/// `FilterForProfanityResponse` carries `TLST`, a list of `FilteredUserText`
/// (`DIRT` : int, `UTXT` : string). The emulator filters nothing, so the list
/// is empty -- which is exactly the answer for an unfiltered text.
pub fn handle_filter_for_profanity(req: &BlazePacket) -> io::Result<BlazePacket> {
    let mut resp = TdfStruct::new();
    resp.set(
        "TLST",
        TdfValue::List(blaze_tdf::TdfList::new(TdfType::Struct, Vec::new())),
    );
    tracing::info!("[Blaze Util] Handled filterForProfanity");
    BlazePacket::response(&req.header, resp)
}

/// Handles fetchClientConfig (0x0001).
///
/// The reply is a `FetchConfigResponse`, which is a struct wrapping the
/// configuration map -- the same shape the `preAuth` reply's `CONF` member
/// uses. See `fetch_config_response`.
pub fn handle_fetch_client_config(req: &BlazePacket) -> io::Result<BlazePacket> {
    let section = match req.payload.get("CFG") {
        Some(TdfValue::String(s)) => s.as_str(),
        _ => "FIFA_ONLINE",
    };

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

    let resp = fetch_config_response(entries);

    tracing::info!("[Blaze Util] Handled fetchClientConfig for section '{}'", section);
    BlazePacket::response(&req.header, resp)
}

/// Handles ping (0x0002).
///
/// In Blaze protocol, PingResponse must include STIM (Server Time, Unix epoch seconds).
/// DirtySDK validates STIM to measure round-trip time and synchronize server clock.
/// Without STIM, DirtySDK treats the ping transaction as pending and drops the
/// connection after its 10-second watchdog timer expires.
pub fn handle_ping(req: &BlazePacket) -> io::Result<BlazePacket> {
    let mut resp = TdfStruct::new();
    let epoch_sec = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    resp.set("STIM", TdfValue::Integer(epoch_sec));
    tracing::info!(
        "[Blaze Util] Handled ping (Seq={}) -> PingResponse(STIM={})",
        req.header.seq_no,
        epoch_sec
    );
    BlazePacket::response(&req.header, resp)
}

/// Handles setClientData (0x0003).
pub fn handle_set_client_data(req: &BlazePacket) -> io::Result<BlazePacket> {
    tracing::debug!("[Blaze Util] setClientData acknowledged");
    BlazePacket::response(&req.header, TdfStruct::new())
}

/// Handles setClientMetrics (0x0016).
///
/// EA's own router answers this with an empty response
/// (`case UtilMessage.setClientMetrics: await ServerUtils.SendEmptyResponse(...)`).
pub fn handle_set_client_metrics(req: &BlazePacket) -> io::Result<BlazePacket> {
    tracing::debug!("[Blaze Util] setClientMetrics acknowledged");
    BlazePacket::response(&req.header, TdfStruct::new())
}

/// Handles userSettingsLoad (0x000A).
pub fn handle_user_settings_load(req: &BlazePacket) -> io::Result<BlazePacket> {
    let mut resp = TdfStruct::new();
    resp.set("DATA", TdfValue::String(String::new()));
    BlazePacket::response(&req.header, resp)
}

/// Handles userSettingsSave (0x000B).
pub fn handle_user_settings_save(req: &BlazePacket) -> io::Result<BlazePacket> {
    BlazePacket::response(&req.header, TdfStruct::new())
}

/// Handles userSettingsLoadAll (0x000C).
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
    use blaze_core::{BlazeHeader, BlazePacket, MessageType};

    #[allow(dead_code)]
    fn make_header(seq: u16) -> BlazeHeader {
        BlazeHeader::new(0, COMPONENT_UTIL, CMD_PING, 0x0000, MessageType::SubmitRequest, 0, seq)
    }

    #[test]
    fn test_ping_response_includes_stim() {
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
            resp.payload.get("STIM").is_some(),
            "PingResponse must include STIM (Server Time)"
        );

        let encoded_16 = resp.encode_16().expect("encode_16 must succeed");
        // Fire2 packs the message type into byte 13 as `type << 5 | userIndex`.
        assert_eq!(
            encoded_16[13],
            (MessageType::RecordResponse as u8) << 5,
            "byte 13 must carry RecordResponse (0x01) shifted left 5"
        );
        assert!(encoded_16.len() > 16, "Must contain STIM payload");
        println!("ping 16-byte frame ({} bytes):\n{}", encoded_16.len(), blaze_core::hex_dump(&encoded_16));
    }

    #[test]
    fn test_pre_auth_response_serialization() {
        let req = BlazePacket::new(
            COMPONENT_UTIL,
            CMD_PRE_AUTH,
            0,
            MessageType::SubmitRequest,
            0,
            1,
            TdfStruct::new(),
        )
        .unwrap();

        let resp = handle_pre_auth(&req, "127.0.0.1", 17498).expect("preAuth handler must not error");
        assert_eq!(resp.header.component, COMPONENT_UTIL);
        assert_eq!(resp.header.command, CMD_PRE_AUTH);
        assert_eq!(resp.header.msg_type, MessageType::RecordResponse);
        assert_eq!(resp.header.seq_no, 1);

        // The members the two authentic EA preAuth replies carry.
        for member in [
            "ASRC", "CIDS", "CLID", "CONF", "ESRC", "INST", "MAID", "MINR", "NASP", "PILD",
            "PLAT", "QOSS", "RSRC", "SVER",
        ] {
            assert!(
                resp.payload.get(member).is_some(),
                "the preAuth reply must carry the `{}` member",
                member
            );
        }

        // `ANAM` and a root-level `TIME` are not members of the schema.
        assert!(resp.payload.get("ANAM").is_none(), "ANAM is not a PreAuthResponse member");
        assert!(resp.payload.get("TIME").is_none(), "TIME is not a PreAuthResponse member");

        let encoded_16 = resp.encode_16().expect("encode_16 must succeed");
        // Fire2 packs the message type into byte 13 as `type << 5 | userIndex`.
        assert_eq!(
            encoded_16[13],
            (MessageType::RecordResponse as u8) << 5,
            "byte 13 must carry RecordResponse (0x01) shifted left 5"
        );
        println!("preAuth 16-byte frame ({} bytes):\n{}", encoded_16.len(), blaze_core::hex_dump(&encoded_16));
    }

    /// The `CONF` member is a `FetchConfigResponse` -- a *struct* whose only
    /// member is the `CONF` map -- not a bare map.
    ///
    /// EA's schema says so (`FetchConfigResponse` has a single `CONF` member
    /// of type `map<string, string>`), and the two authentic EA preAuth
    /// replies both decode as `CONF (struct)` containing `CONF (map)`.
    #[test]
    fn test_pre_auth_conf_is_a_struct_wrapping_the_map() {
        let req = BlazePacket::new(
            COMPONENT_UTIL,
            CMD_PRE_AUTH,
            0,
            MessageType::SubmitRequest,
            0,
            1,
            TdfStruct::new(),
        )
        .unwrap();
        let resp = handle_pre_auth(&req, "127.0.0.1", 17498).expect("preAuth handler must not error");

        let conf = match resp.payload.get("CONF") {
            Some(TdfValue::Struct(s)) => s,
            other => panic!("CONF must be a struct, got {:?}", other),
        };
        let map = match conf.get("CONF") {
            Some(TdfValue::Map(m)) => m,
            other => panic!("the CONF struct's member must be a map, got {:?}", other),
        };
        assert!(
            !map.entries.is_empty(),
            "the configuration map must carry at least one entry"
        );
    }

    /// Nothing in the configuration map may point at a real EA service.
    ///
    /// The emulator is a private server: the client must never be sent to
    /// `accounts.ea.com`, `gateway.ea.com`, `signin.ea.com`, a Bytevault
    /// host, a Live service, or a Watson speech endpoint.
    #[test]
    fn test_no_config_entry_points_at_a_real_ea_service() {
        let req = BlazePacket::new(
            COMPONENT_UTIL,
            CMD_PRE_AUTH,
            0,
            MessageType::SubmitRequest,
            0,
            1,
            TdfStruct::new(),
        )
        .unwrap();
        let resp = handle_pre_auth(&req, "127.0.0.1", 17498).expect("preAuth handler must not error");

        let conf = match resp.payload.get("CONF") {
            Some(TdfValue::Struct(s)) => s,
            other => panic!("CONF must be a struct, got {:?}", other),
        };
        let map = match conf.get("CONF") {
            Some(TdfValue::Map(m)) => m,
            other => panic!("the CONF struct's member must be a map, got {:?}", other),
        };

        for (key, value) in &map.entries {
            let (TdfValue::String(key), TdfValue::String(value)) = (key, value) else {
                panic!("configuration entries must be string -> string");
            };
            for forbidden in [
                "ea.com",
                "eac.ea.com",
                "gos.ea.com",
                "xboxlive.com",
                "watson.cloud.ibm.com",
            ] {
                assert!(
                    !value.contains(forbidden),
                    "configuration entry `{}` points at a real EA service: {}",
                    key,
                    value
                );
            }
        }
    }

    /// The `QOSS` struct must use EA's own member names.
    ///
    /// The schema is the one EA's code generator emitted, preserved in
    /// `references/Skate3BlazeServer/.../Util/Models/QosConfigInfo.cs`:
    ///
    /// ```text
    /// QosConfigInfo
    ///   BWPS : QosPingSiteInfo
    ///   LNP  : ushort
    ///   LTPS : map<string, QosPingSiteInfo>
    ///   SVID : uint
    ///
    /// QosPingSiteInfo
    ///   PSA  : string   (address)
    ///   PSP  : ushort   (port)
    ///   SNA  : string   (site name)
    /// ```
    ///
    /// The names we used to ship -- `BWHR`, `LATT`, `PSA` as a nested
    /// struct, `SKEY` -- do not exist in the schema, so a client that
    /// decodes `QOSS` by type would read none of them.
    #[test]
    fn test_qoss_uses_ea_member_names() {
        let req = BlazePacket::new(
            COMPONENT_UTIL,
            CMD_PRE_AUTH,
            0,
            MessageType::SubmitRequest,
            0,
            1,
            TdfStruct::new(),
        )
        .unwrap();
        let resp = handle_pre_auth(&req, "127.0.0.1", 17498).expect("preAuth handler must not error");

        let qoss = match resp.payload.get("QOSS") {
            Some(TdfValue::Struct(s)) => s,
            other => panic!("QOSS must be a struct, got {:?}", other),
        };

        for member in ["BWPS", "LNP", "LTPS", "SVID"] {
            assert!(
                qoss.get(member).is_some(),
                "QOSS must carry the `{}` member from EA's QosConfigInfo schema",
                member
            );
        }
        for bogus in ["BWHR", "LATT", "SKEY"] {
            assert!(
                qoss.get(bogus).is_none(),
                "QOSS must not carry `{}`; it is not part of EA's schema",
                bogus
            );
        }
    }

    /// The `LTPS` map maps a site alias to a `QosPingSiteInfo`, and each
    /// value carries `PSA` (address), `PSP` (port) and `SNA` (site name).
    /// The emulator's QoS responder must be reachable under the alias the
    /// client is told to use.
    #[test]
    fn test_qoss_ltps_map_points_at_the_emulator_qos_responder() {
        let req = BlazePacket::new(
            COMPONENT_UTIL,
            CMD_PRE_AUTH,
            0,
            MessageType::SubmitRequest,
            0,
            1,
            TdfStruct::new(),
        )
        .unwrap();
        let resp = handle_pre_auth(&req, "127.0.0.1", 17498).expect("preAuth handler must not error");

        let qoss = match resp.payload.get("QOSS") {
            Some(TdfValue::Struct(s)) => s,
            other => panic!("QOSS must be a struct, got {:?}", other),
        };

        let ltps = match qoss.get("LTPS") {
            Some(TdfValue::Map(m)) => m,
            other => panic!("LTPS must be a map, got {:?}", other),
        };
        assert!(
            !ltps.entries.is_empty(),
            "LTPS must advertise at least one QoS ping site"
        );

        let (_alias, site) = ltps
            .entries
            .first()
            .expect("LTPS must have at least one entry");
        let site = match site {
            TdfValue::Struct(s) => s,
            other => panic!("an LTPS value must be a struct, got {:?}", other),
        };

        assert_eq!(
            site.get("PSA"),
            Some(&TdfValue::String("127.0.0.1".to_string())),
            "the QoS site address must be the emulator host"
        );
        assert_eq!(
            site.get("PSP"),
            Some(&TdfValue::Integer(17498)),
            "the QoS site port must be the UDP responder's port"
        );
        assert!(
            site.get("SNA").is_some(),
            "a QoS site must carry its `SNA` site name"
        );
    }
}

/// Handles Stats component queries.
pub fn handle_stats(req: &BlazePacket) -> io::Result<BlazePacket> {
    BlazePacket::response(&req.header, TdfStruct::new())
}
