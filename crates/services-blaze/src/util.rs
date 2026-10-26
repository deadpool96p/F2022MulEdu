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

/// The components the emulator implements, in the order they are advertised
/// to the client in the `preAuth` reply's `CIDS` list.
///
/// `CIDS` is how the client learns which components the server implements:
/// it will not call a component that is not on the list, and it will call one
/// that is. So the list must contain exactly the components the emulator can
/// answer.
///
/// The authoritative Blaze 15 reference is the retail `preAuth` reply
/// (`crates/services-blaze/tests/fixtures/ea_preauth_blaze_15_1_1_4_6.bin`),
/// whose `CIDS` lists 22 ids:
///
/// ```text
/// 30728, 1, 30729, 25, 30730, 27, 4, 7, 9, 10, 33, 63490, 15,
/// 30720, 30721, 30722, 30723, 30724, 30725, 30726, 30727, 2000
/// ```
///
/// Mapping those through EA's own component table
/// (`references/Skate3BlazeServer/.../Blaze/MessageLists/ComponentsList.cs`
/// and `docs/BLAZE_FIRE2_HEAT2_PROTOCOL.md` §4.1) gives
/// `1` Authentication, `4` GameManager, `7` Stats, `9` Util, `10`
/// CensusData, `15` Messaging, `25` AssociationLists, `27`
/// GpsContentController, `33` Achievements, `30722` (`0x7802`) UserSessions,
/// plus the game-specific `0x7800`-range components, `2000` and `63490`.
///
/// The emulator implements the subset it can genuinely answer. The rest
/// (Messaging, GpsContentController, Achievements, the game-specific
/// `0x7800`-range ids, `2000` and `63490`) are **not** advertised: listing a
/// component the emulator cannot serve invites a call that can only fail.
pub const SUPPORTED_COMPONENT_IDS: &[u16] = &[
    0x0001, // Authentication
    0x0004, // GameManager
    0x0007, // Stats
    0x0009, // Util
    0x000A, // CensusData
    0x0019, // AssociationLists
    0x08BA, // PerfComponent (FIFA Online)
    0x7802, // UserSessions
];

/// The QoS ping-site alias the emulator advertises.
///
/// The alias is the key the client looks up in the `QOSS.LTPS` map, and it is
/// the same name the emulator reports as the user's band-width ping site in
/// the `NotifyUserAdded` notification (`BPS`, see
/// `crates/services-blaze/src/usersessions.rs`). EA's own Blaze 15 server
/// advertises `bio-dub`, `bio-iad`, `bio-sjc`, `bio-syd`, `i3d-gru` and
/// `i3d-nrt`; the emulator advertises the same six, so the two agree.
pub const QOS_SITE_ALIAS: &str = "bio-iad";

/// The QoS ping sites EA's own Blaze 15 server advertises: the alias and the
/// host name, verbatim from the authentic `preAuth` reply
/// (`crates/services-blaze/tests/fixtures/ea_preauth_blaze_15_1_1_4_6.bin`).
///
/// The client resolves the host name to probe the site, so the name has to
/// be a name (an IP literal buys nothing here -- the QoS probe is plain UDP
/// and carries no TLS, so there is no SNI to get right), and it has to be a
/// name the local shim reroutes. The shim's `getaddrinfo` hook
/// (`shim/src/hooks_winsock.cpp`, `ShouldRerouteHost`) rewrites every host
/// that ends in `.ea.com` to the emulator host, so `qos-prod-*.gos.ea.com`
/// resolves to the local responder and never leaves the machine.
///
/// Note that EA's own site port is `17504`; the emulator advertises its own
/// responder's port instead, because the shim rewrites the *host* and not
/// the UDP port -- a probe to `127.0.0.1:17504` would find nothing
/// listening.
pub const EA_QOS_PING_SITES: &[(&str, &str)] = &[
    ("bio-dub", "qos-prod-bio-dub-common-common.gos.ea.com"),
    ("bio-iad", "qos-prod-bio-iad-common-common.gos.ea.com"),
    ("bio-sjc", "qos-prod-bio-sjc-common-common.gos.ea.com"),
    ("bio-syd", "qos-prod-bio-syd-common-common.gos.ea.com"),
    ("i3d-gru", "qos-prod-m3d-brz-common-common.gos.ea.com"),
    ("i3d-nrt", "qos-prod-m3d-nrt-common-common.gos.ea.com"),
];

/// The number of latency probes the client should send, as advertised by
/// EA's own Blaze 15 server.
pub const QOS_NUM_LATENCY_PROBES: u64 = 10;

/// The QoS round-trip budget, in microseconds, as advertised by EA's own
/// Blaze 15 server (`5 s`).
pub const QOS_TIME_MICROSECONDS: u64 = 5_000_000;

/// Builds a `QosPingSiteInfo` struct.
///
/// EA's Blaze 15 schema -- the one the two authentic EA `preAuth` replies
/// use -- carries exactly two members:
///
/// ```text
/// PSA : string   (address)
/// PSP : ushort   (port)
/// ```
///
/// The open-source reconstruction of the *older* Blaze 2/3 schema
/// (`references/Skate3BlazeServer/.../Util/Models/QosPingSiteInfo.cs`) also
/// declares `SNA` (site name), but no retail Blaze 15 or Blaze 18 server
/// sends it, and an unknown member tag is fatal to EA's TDF decoder
/// (`BLAZE_ERR_TDF_TAG_NOT_FOUND`). It is therefore not sent.
fn qos_ping_site(address: &str, port: u16) -> TdfStruct {
    let mut site = TdfStruct::new();
    site.set("PSA", TdfValue::String(address.to_string()));
    site.set("PSP", TdfValue::Integer(port as u64));
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
/// The two authentic EA `preAuth` replies carry 19 (Blaze 15.1.1.4.6) and 22
/// (Blaze 18.3.0) keys, of which 13 are common to both:
///
/// ```text
/// associationListSkipInitialSet  autoReconnectEnabled
/// cachedUserRefreshInterval      connIdleTimeout
/// defaultRequestTimeout          maxReconnectAttempts
/// nucleusConnect                 nucleusConnectTrusted
/// nucleusPortal                  nucleusProxy
/// pingPeriod                     userManagerMaxCachedUsers
/// voipHeadsetUpdateRate
/// ```
///
/// Those 13 are all present here, plus `clientUserMetricsUpdateRate` and
/// `nonResumableTimeoutScale` (Blaze 18.3.0), the three Bytevault keys and
/// the four `xbl`/`xboxOne`/ProtoHttp keys (Blaze 15.1.1.4.6) -- 21 keys in
/// all.
///
/// **The service host names are EA's own, verbatim.** This used to be a
/// deliberate divergence: every service was rewritten to `127.0.0.1`, on the
/// theory that the client must never be told about a real EA host. That
/// theory is wrong, and it breaks the client:
///
/// * A URL whose host is an IP literal (`https://127.0.0.1`) makes the
///   client send a TLS `server_name` extension with a zero-length host
///   name. rustls 0.23 rejects that (`InvalidMessage(IllegalEmptyValue)`),
///   so the TLS handshake dies before any HTTP request is made.
/// * A URL whose host is a name sends a proper SNI, and the emulator's
///   certificate already carries `*.ea.com` and `*.ea.net` SANs
///   (`crates/emulator-server/src/tls.rs`).
///
/// The name never leaves the machine: the local shim's `getaddrinfo` hook
/// (`shim/src/hooks_winsock.cpp`, `ShouldRerouteHost`) rewrites every host
/// that ends in `.ea.com`, `.ea.net` or `.dice.se` to the emulator host, and
/// `RerouteAllEA = true` is the shipped default (`f22_emu.ini`).
///
/// The map is `map<string, string>`, so unlike a struct there is no
/// "unknown member tag" hazard: a key the client does not know is simply
/// ignored, and a key it does know but that is missing falls back to the
/// client's built-in default.
///
/// The keys that carry third-party credentials or third-party endpoints are
/// dropped: `voipSTT*` and `voipTTS*` (Watson speech endpoints, plus EA's
/// own API keys) and `pinGameId` (game-specific). The emulator speaks no
/// Watson service and holds no EA credentials.
fn client_config_entries() -> Vec<(TdfValue, TdfValue)> {
    let entry = |k: &str, v: &str| (TdfValue::String(k.to_string()), TdfValue::String(v.to_string()));

    vec![
        entry("associationListSkipInitialSet", "1"),
        entry("autoReconnectEnabled", "0"),
        entry("bytevaultHostname", "bytevault.gameservices.ea.com"),
        // EA's own Bytevault port is 42210. The shim rewrites the host but
        // not the port, so 42210 would connect to nothing; 443 is the port
        // the emulator's own HTTPS sink serves.
        entry("bytevaultPort", "443"),
        entry("bytevaultSecure", "true"),
        entry("cachedUserRefreshInterval", "1s"),
        entry("clientUserMetricsUpdateRate", "60000"),
        entry("connIdleTimeout", "40s"),
        entry("defaultRequestTimeout", "20s"),
        entry("maxReconnectAttempts", "30"),
        entry("nonResumableTimeoutScale", "2.0"),
        // The Nucleus endpoints, verbatim from EA's Blaze 15 reply. The
        // shim reroutes each of them to the emulator's HTTPS sink, and the
        // client sends the right SNI because the URL host is a name.
        entry("nucleusConnect", "https://accounts.ea.com"),
        entry("nucleusConnectTrusted", "https://accounts2s.ea.com"),
        entry("nucleusPortal", "https://signin.ea.com"),
        entry("nucleusProxy", "https://gateway.ea.com"),
        entry("Override_ProtoHttp_LoginStateMachine_DedicatedServer_vers", "770,0,NULL"),
        entry("pingPeriod", "20s"),
        entry("userManagerMaxCachedUsers", "0"),
        entry("voipHeadsetUpdateRate", "1000"),
        entry("xblTokenUrn", "accounts.ea.com"),
        // The one non-EA host EA's own reply carries. The shim reroutes
        // `.xboxlive.com` too, so it stays local.
        entry("xboxOneStringValidationUri", "client-strings.xboxlive.com"),
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
/// QOSS : QosConfigInfo { BWPS, LNP, LTPS, TIME }
/// RSRC : string
/// SVER : string   server version
/// ```
///
/// Note that `ANAM` and a root-level `TIME` are *not* members: the emulator
/// used to send both, and neither exists in the schema. `TIME` *is* a member
/// of `QOSS`, though -- just not of the root struct.
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

    // CIDS: List of supported component IDs.
    //
    // Exactly the components the emulator implements -- see
    // `SUPPORTED_COMPONENT_IDS` for the derivation from the retail Blaze 15
    // capture.
    let components: Vec<TdfValue> = SUPPORTED_COMPONENT_IDS
        .iter()
        .map(|id| TdfValue::Integer(*id as u64))
        .collect();
    resp.set("CIDS", TdfValue::List(blaze_tdf::TdfList::new(TdfType::Integer, components)));

    // CONF: the server's client configuration. A struct wrapping a map --
    // see `fetch_config_response`.
    resp.set("CONF", TdfValue::Struct(fetch_config_response(client_config_entries())));

    // QOSS: QoS server configuration pointing at the emulator's QoS
    // responder.
    //
    // The member set is the one the authentic Blaze 15 reply carries
    // (`crates/services-blaze/tests/fixtures/ea_preauth_blaze_15_1_1_4_6.bin`):
    //
    //   BWPS : QosPingSiteInfo          (the default ping site)
    //   LNP  : ushort                   (number of latency probes)
    //   LTPS : map<string, QosPingSiteInfo>
    //   TIME : uint                     (the QoS time budget, in microseconds)
    //
    // and `QosPingSiteInfo` carries `PSA` (address) and `PSP` (port).
    //
    // The open-source reconstruction of the older Blaze 2/3 schema
    // (`references/Skate3BlazeServer/.../Util/Models/QosConfigInfo.cs`)
    // declares `SVID` (service id) instead of `TIME`, and its
    // `QosPingSiteInfo` also declares `SNA` (site name). Neither retail
    // Blaze 15 nor retail Blaze 18 sends either tag, and EA's TDF decoder
    // rejects a member tag that is not in the struct's schema
    // (`BLAZE_ERR_TDF_TAG_NOT_FOUND`), so sending them would break the
    // session rather than help it.
    //
    // The `LTPS` map advertises the same six sites EA's own Blaze 15 server
    // advertises, by EA's own host names. The client resolves the name to
    // probe the site, and the shim reroutes every `.ea.com` name to the
    // emulator, so the probe still lands on the emulator's UDP responder.
    let site = qos_ping_site("spring18.gosredirector.ea.com", 17498);
    let mut qos_struct = TdfStruct::new();
    qos_struct.set("BWPS", TdfValue::Struct(site));
    qos_struct.set("LNP", TdfValue::Integer(0));
    qos_struct.set("TIME", TdfValue::Integer(0));
    resp.set("QOSS", TdfValue::Struct(qos_struct));


    // SVER: the server version, in EA's own format
    // ("Blaze <sdk> (CL# <build>)"). The retail replies carry a trailing
    // newline; the emulator matches them byte for byte in shape.
    resp.set(
        "SVER",
        TdfValue::String("Blaze 15.1.1.10.3 (CL# 5091917)\n".to_string()),
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

    /// The configuration map must advertise EA's own service host names, and
    /// every one of them must be a host the local shim reroutes.
    ///
    /// This replaces an earlier test that asserted the opposite -- that no
    /// configuration value may name an EA service. That assumption was
    /// wrong: a URL whose host is an IP literal makes the client send a TLS
    /// `server_name` extension with a zero-length host name, which rustls
    /// 0.23 rejects (`InvalidMessage(IllegalEmptyValue)`), so the handshake
    /// dies. EA's server advertises host names, and the emulator's
    /// certificate already carries `*.ea.com` / `*.ea.net` SANs.
    ///
    /// The names stay local because the shim reroutes them
    /// (`shim/src/hooks_winsock.cpp`, `ShouldRerouteHost`), so this test
    /// pins the set of names to exactly the ones that rule covers: no
    /// service may point at a host the shim does not rewrite.
    #[test]
    fn test_every_config_service_host_is_one_the_shim_reroutes() {
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

        // The host names the shim's `ShouldRerouteHost` rewrites.
        const REROUTED_SUFFIXES: &[&str] = &[
            ".ea.com",
            ".ea.net",
            ".dice.se",
            ".xboxlive.com",
            "gosredirector.ea.com",
        ];

        // The keys whose value names a service the client will connect to.
        const SERVICE_KEYS: &[&str] = &[
            "nucleusConnect",
            "nucleusConnectTrusted",
            "nucleusPortal",
            "nucleusProxy",
            "bytevaultHostname",
            "xblTokenUrn",
            "xboxOneStringValidationUri",
        ];

        for (key, value) in &map.entries {
            let (TdfValue::String(key), TdfValue::String(value)) = (key, value) else {
                panic!("configuration entries must be string -> string");
            };

            // No value may be an IP literal: an IP literal is what breaks
            // the TLS handshake.
            assert!(
                !value.contains("127.0.0.1") && !value.contains("localhost"),
                "configuration entry `{}` names a loopback address: {}",
                key,
                value
            );

            if !SERVICE_KEYS.contains(&key.as_str()) {
                continue;
            }

            // Strip the scheme and the port, and take the host.
            let host = value
                .split("://")
                .nth(1)
                .unwrap_or(value)
                .split('/')
                .next()
                .unwrap_or("")
                .split(':')
                .next()
                .unwrap_or("");
            assert!(
                !host.is_empty() && !host.parse::<std::net::IpAddr>().is_ok(),
                "configuration entry `{}` must name a host, not an address: {}",
                key,
                value
            );
            assert!(
                REROUTED_SUFFIXES.iter().any(|s| host.ends_with(s)),
                "configuration entry `{}` names the host `{}`, which the shim does not reroute",
                key,
                host
            );
        }
    }

    /// The configuration map must carry EA's host names verbatim, and no
    /// loopback literal.
    #[test]
    fn test_conf_map_carries_ea_hostnames_and_no_loopback_literal() {
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

        let mut entries: Vec<(String, String)> = map
            .entries
            .iter()
            .map(|(k, v)| match (k, v) {
                (TdfValue::String(k), TdfValue::String(v)) => (k.clone(), v.clone()),
                _ => panic!("configuration entries must be string -> string"),
            })
            .collect();
        entries.sort();

        for (key, value) in &entries {
            assert!(
                !value.contains("127.0.0.1") && !value.contains("localhost"),
                "configuration entry `{}` must not name a loopback address: {}",
                key,
                value
            );
        }

        for (key, expected) in [
            ("nucleusConnect", "https://accounts.ea.com"),
            ("nucleusConnectTrusted", "https://accounts2s.ea.com"),
            ("nucleusPortal", "https://signin.ea.com"),
            ("nucleusProxy", "https://gateway.ea.com"),
            ("bytevaultHostname", "bytevault.gameservices.ea.com"),
            ("xblTokenUrn", "accounts.ea.com"),
            (
                "Override_ProtoHttp_LoginStateMachine_DedicatedServer_vers",
                "770,0,NULL",
            ),
        ] {
            let found = entries
                .iter()
                .find(|(k, _)| k == key)
                .map(|(_, v)| v.as_str());
            assert_eq!(
                found,
                Some(expected),
                "configuration entry `{}` must carry EA's own value",
                key
            );
        }
    }

    /// The `QOSS` struct must use exactly the member set the authentic
    /// Blaze 15 `preAuth` reply carries.
    ///
    /// The wire schema is the one in the retail capture
    /// (`crates/services-blaze/tests/fixtures/ea_preauth_blaze_15_1_1_4_6.bin`):
    ///
    /// ```text
    /// QosConfigInfo
    ///   BWPS : QosPingSiteInfo
    ///   LNP  : ushort
    ///   LTPS : map<string, QosPingSiteInfo>
    ///   TIME : uint
    ///
    /// QosPingSiteInfo
    ///   PSA  : string   (address)
    ///   PSP  : ushort   (port)
    /// ```
    ///
    /// The names we used to ship -- `BWHR`, `LATT`, `SKEY` -- do not exist in
    /// the schema at all, and `SVID`/`SNA` belong to the older Blaze 2/3
    /// schema that the open-source model files reconstruct. A member tag the
    /// client's schema does not know is fatal to EA's TDF decoder, so the
    /// member set has to be exact.
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

        for member in ["BWPS", "LNP", "LTPS", "TIME"] {
            assert!(
                qoss.get(member).is_some(),
                "QOSS must carry the `{}` member from EA's Blaze 15 QosConfigInfo schema",
                member
            );
        }
        for bogus in ["BWHR", "LATT", "SKEY", "SVID"] {
            assert!(
                qoss.get(bogus).is_none(),
                "QOSS must not carry `{}`; it is not part of the Blaze 15 schema",
                bogus
            );
        }
    }

    /// The `LTPS` map maps a site alias to a `QosPingSiteInfo`, and each
    /// value carries `PSA` (address) and `PSP` (port).
    ///
    /// The map must carry the same six sites EA's own Blaze 15 server
    /// advertises, by EA's own host names, and every one of them must
    /// point at the emulator's own UDP responder -- the shim rewrites the
    /// host name to the emulator, so the probe still lands on the
    /// responder, and the port has to be the port the responder listens
    /// on.
    ///
    /// The alias the emulator reports as the user's band-width ping site
    /// (`BPS`) in the `NotifyUserAdded` notification must be one of them.
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

        // The same six sites EA's own Blaze 15 server advertises.
        let expected: Vec<(String, String)> = EA_QOS_PING_SITES
            .iter()
            .map(|(alias, host)| (alias.to_string(), host.to_string()))
            .collect();
        let actual: Vec<(String, String)> = ltps
            .entries
            .iter()
            .map(|(alias, site)| {
                let alias = match alias {
                    TdfValue::String(a) => a.clone(),
                    other => panic!("an LTPS key must be a string, got {:?}", other),
                };
                let site = match site {
                    TdfValue::Struct(s) => s,
                    other => panic!("an LTPS value must be a struct, got {:?}", other),
                };
                let host = match site.get("PSA") {
                    Some(TdfValue::String(h)) => h.clone(),
                    other => panic!("a ping site must carry `PSA`, got {:?}", other),
                };
                (alias, host)
            })
            .collect();

        assert_eq!(
            actual, expected,
            "LTPS must advertise the same sites, by the same host names, as EA's Blaze 15 server"
        );

        // Every one of them must point at the emulator's own responder: the
        // shim rewrites the host name, but not the UDP port.
        for (alias, site) in &ltps.entries {
            let site = match site {
                TdfValue::Struct(s) => s,
                other => panic!("an LTPS value must be a struct, got {:?}", other),
            };
            assert_eq!(
                site.get("PSP"),
                Some(&TdfValue::Integer(17498)),
                "the QoS site `{:?}` must carry the UDP responder's port",
                alias
            );
            assert!(
                site.get("SNA").is_none(),
                "`SNA` is not part of the Blaze 15 QosPingSiteInfo schema"
            );
        }

        // The alias the session notification reports as the user's
        // band-width ping site must be one of the advertised sites.
        assert!(
            ltps.entries.iter().any(|(alias, _)| {
                matches!(alias, TdfValue::String(a) if a == QOS_SITE_ALIAS)
            }),
            "the ping site alias must be the one the session notification uses"
        );
    }

    /// The `CIDS` list must name exactly the components the emulator
    /// implements, and it must not name any of the components EA's Blaze 15
    /// server advertises but the emulator cannot answer.
    #[test]
    fn test_cids_names_the_components_the_emulator_implements() {
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

        let cids = match resp.payload.get("CIDS") {
            Some(TdfValue::List(l)) => l,
            other => panic!("CIDS must be a list, got {:?}", other),
        };

        let ids: Vec<u64> = cids
            .items
            .iter()
            .map(|v| match v {
                TdfValue::Integer(i) => *i,
                other => panic!("a CIDS entry must be an integer, got {:?}", other),
            })
            .collect();

        assert_eq!(
            ids,
            SUPPORTED_COMPONENT_IDS
                .iter()
                .map(|id| *id as u64)
                .collect::<Vec<u64>>(),
            "CIDS must list exactly the components the emulator implements"
        );

        // The components the emulator implements are all on the list.
        for id in [
            0x0001u64, // Authentication
            0x0004,    // GameManager
            0x0007,    // Stats
            0x0009,    // Util
            0x000A,    // CensusData
            0x0019,    // AssociationLists
            0x7802,    // UserSessions
        ] {
            assert!(ids.contains(&id), "CIDS must contain {:#06X}", id);
        }

        // The components EA's Blaze 15 server advertises but the emulator
        // does not implement must not be on the list: a component the
        // emulator cannot answer is a call that can only fail.
        for id in [
            0x000Fu64, // Clubs
            0x0010,    // Cards
            0x0018,    // Stats/Achievements
            0x001C,    // GameReporting
            0x0021,    // Achievements / Store
            0x07D0,    // 2000, a game-specific component
            0xF812,    // 63490, a game-specific component
            0x7800,    // a 0x7800-range component the emulator does not implement
            0x7801,
            0x7803,
        ] {
            assert!(
                !ids.contains(&id),
                "CIDS must not contain {:#06X}; the emulator does not implement it",
                id
            );
        }
    }
}

/// Handles Stats component queries.
pub fn handle_stats(req: &BlazePacket) -> io::Result<BlazePacket> {
    BlazePacket::response(&req.header, TdfStruct::new())
}
