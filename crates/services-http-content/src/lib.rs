//! # services-http-content
//!
//! Fast mock HTTP/HTTPS telemetry sink (`river.data.ea.com`, `pin-river.data.ea.com`)
//! and squad content distribution endpoint.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::OnceLock;

pub mod squads;
pub use squads::*;

/// Real-time UDP Netcode Metrics exposed to HTTP Netcode HUD & monitoring
#[derive(Debug)]
pub struct NetcodeHttpMetrics {
    pub rtt_ms: AtomicU32,
    pub jitter_ms: AtomicU32,
    pub dropped_frames: AtomicU32,
    pub cipher_reuse_count: AtomicU64,
}

static GLOBAL_METRICS: OnceLock<NetcodeHttpMetrics> = OnceLock::new();

impl NetcodeHttpMetrics {
    pub fn global() -> &'static NetcodeHttpMetrics {
        GLOBAL_METRICS.get_or_init(|| NetcodeHttpMetrics {
            rtt_ms: AtomicU32::new(18),
            jitter_ms: AtomicU32::new(2),
            dropped_frames: AtomicU32::new(0),
            cipher_reuse_count: AtomicU64::new(0),
        })
    }

    pub fn update(&self, rtt: u32, jitter: u32, dropped: u32, cipher_reuse: u64) {
        self.rtt_ms.store(rtt, Ordering::Relaxed);
        self.jitter_ms.store(jitter, Ordering::Relaxed);
        self.dropped_frames.store(dropped, Ordering::Relaxed);
        self.cipher_reuse_count.store(cipher_reuse, Ordering::Relaxed);
    }

    pub fn increment_cipher_reuse(&self) -> u64 {
        self.cipher_reuse_count.fetch_add(1, Ordering::Relaxed) + 1
    }

    pub fn set_cipher_reuse_count(&self, count: u64) {
        self.cipher_reuse_count.store(count, Ordering::Relaxed);
    }

    pub fn to_json(&self) -> String {
        format!(
            r#"{{"rtt_ms":{},"jitter_ms":{},"dropped_frames":{},"cipher_reuse_count":{}}}"#,
            self.rtt_ms.load(Ordering::Relaxed),
            self.jitter_ms.load(Ordering::Relaxed),
            self.dropped_frames.load(Ordering::Relaxed),
            self.cipher_reuse_count.load(Ordering::Relaxed),
        )
    }
}

/// Parsed HTTP request structure.
#[derive(Debug, Clone)]
pub struct HttpRequest {
    pub method: String,
    pub path: String,
    pub headers: HashMap<String, String>,
    pub body: Vec<u8>,
}

/// HTTP response structure.
/// RS4:FutMatchReadyServerResponse returned upon receiving MatchReady.
/// Deserialized by CardsDLL_Win64_retail.dll at 0x1803c13b0 and serialized at 0x1803c16b0.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FutMatchReadyServerResponse {
    pub match_id: u64,
    pub items: Vec<String>,
    pub opponent_persona_id: u64,
    pub opponent_persona_ids: Vec<u64>,
}

impl FutMatchReadyServerResponse {
    pub fn new(match_id: u64, opponent_persona_id: u64) -> Self {
        Self {
            match_id,
            items: Vec::new(),
            opponent_persona_id,
            opponent_persona_ids: vec![opponent_persona_id],
        }
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| "{}".to_string())
    }

    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }
}

/// HTTP response structure.
#[derive(Debug, Clone)]
pub struct HttpResponse {
    pub status_code: u16,
    pub reason_phrase: &'static str,
    pub content_type: &'static str,
    pub headers: HashMap<String, String>,
    pub body: Vec<u8>,
}

impl HttpResponse {
    pub fn ok_json(body: &str) -> Self {
        Self {
            status_code: 200,
            reason_phrase: "OK",
            content_type: "application/json",
            headers: HashMap::new(),
            body: body.as_bytes().to_vec(),
        }
    }

    pub fn ok_xml(body: &str) -> Self {
        Self {
            status_code: 200,
            reason_phrase: "OK",
            content_type: "application/xml",
            headers: HashMap::new(),
            body: body.as_bytes().to_vec(),
        }
    }

    pub fn ok_binary(body: Vec<u8>) -> Self {
        Self {
            status_code: 200,
            reason_phrase: "OK",
            content_type: "application/octet-stream",
            headers: HashMap::new(),
            body,
        }
    }

    pub fn not_found() -> Self {
        Self {
            status_code: 404,
            reason_phrase: "Not Found",
            content_type: "text/plain",
            headers: HashMap::new(),
            body: b"404 Not Found".to_vec(),
        }
    }

    pub fn bad_request(body: &str) -> Self {
        Self {
            status_code: 400,
            reason_phrase: "Bad Request",
            content_type: "application/json",
            headers: HashMap::new(),
            body: body.as_bytes().to_vec(),
        }
    }

    pub fn match_ready_response(match_id: u64, opponent_persona_id: u64, max_age_secs: u32) -> Self {
        let resp_struct = FutMatchReadyServerResponse::new(match_id, opponent_persona_id);
        let json = resp_struct.to_json();
        let mut headers = HashMap::new();
        headers.insert("Cache-Control".to_string(), format!("max-age={}", max_age_secs));
        headers.insert("X-Cache-Control".to_string(), format!("max-age={}", max_age_secs));
        Self {
            status_code: 200,
            reason_phrase: "OK",
            content_type: "application/json",
            headers,
            body: json.into_bytes(),
        }
    }

    /// Serializes the response into raw HTTP wire bytes.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = Vec::new();
        let mut header_str = format!(
            "HTTP/1.1 {} {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\nAccess-Control-Allow-Origin: *\r\n",
            self.status_code, self.reason_phrase, self.content_type, self.body.len()
        );
        for (k, v) in &self.headers {
            header_str.push_str(&format!("{}: {}\r\n", k, v));
        }
        header_str.push_str("\r\n");
        out.extend_from_slice(header_str.as_bytes());
        out.extend_from_slice(&self.body);
        out
    }
}

/// Parses a raw HTTP/1.1 request buffer into an HttpRequest.
pub fn parse_http_request(raw: &[u8]) -> Option<HttpRequest> {
    let header_end = raw.windows(4).position(|w| w == b"\r\n\r\n")?;
    let header_str = String::from_utf8_lossy(&raw[..header_end]);
    let body = raw[header_end + 4..].to_vec();

    let mut lines = header_str.lines();
    let request_line = lines.next()?;
    let mut parts = request_line.split_whitespace();

    let method = parts.next()?.to_string();
    let path = parts.next()?.to_string();

    let mut headers = HashMap::new();
    for line in lines {
        if let Some((k, v)) = line.split_once(':') {
            headers.insert(k.trim().to_lowercase(), v.trim().to_string());
        }
    }

    Some(HttpRequest {
        method,
        path,
        headers,
        body,
    })
}

/// The QoS UDP responder's port. The client learns it from the `/qos/qos`
/// response and then sends `ProtoPing` probes here.
pub const QOS_UDP_PORT: u16 = 17498;

/// The QoS responder's address, encoded the way FIFA 22 expects it.
///
/// `qosip` is read by the client with a *decimal* integer parser
/// (`0x140CA2280` with `edx = 0`), not with `inet_pton`. The resulting
/// u32 is then split into its four bytes most-significant-first
/// (`0x141FE8B6A`: `al` -> byte 0, `shr eax, 8` -> byte 1, ...), so the
/// value is a **big-endian** IPv4 address.
///
/// 127.0.0.1 == 0x7F000001 == 2130706433.
const QOS_IP_DECIMAL: u32 = 2_130_706_433;

/// The number of latency probes the client should send. The client rejects
/// a type-2 QoS response whose `numprobes` is below 2
/// (`0x141FE8753`: `cmp dword ptr [rbx + 0x44], 2` / `jb` -> fail).
const QOS_NUM_PROBES: u32 = 2;

/// The size of each latency probe. The client rejects a type-2 QoS
/// response whose `probesize` is zero (`0x141FE8759`: `cmp dword ptr
/// [rbx + 0x40], 0` / `jne` -> success, fall through to the -4 failure).
const QOS_PROBE_SIZE: u32 = 1200;

/// The `firetype` classification. `0` means "no firewall detected", which
/// is correct for a private server where the client reaches the QoS
/// responder directly.
const QOS_FIRETYPE: u32 = 0;

/// Builds the `/qos/qos` response body.
///
/// The client resolves the root element `qos` and then reads `.qosport`,
/// `.qosip`, `.numprobes`, `.probesize`, `.requestid` and `.reqsecret`
/// as *siblings* of each other, so they must all be direct children of
/// `<qos>`.
///
/// `qosip` and `qosport` are mandatory: a zero value in either makes the
/// client fail the QoS step with -3 (`0x141FE85EC` / `0x141FE85F6`).
fn qos_response_body() -> String {
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <qos>\n\
         \x20   <qosip>{QOS_IP_DECIMAL}</qosip>\n\
         \x20   <qosport>{QOS_UDP_PORT}</qosport>\n\
         \x20   <numprobes>{QOS_NUM_PROBES}</numprobes>\n\
         \x20   <probesize>{QOS_PROBE_SIZE}</probesize>\n\
         \x20   <requestid>1</requestid>\n\
         \x20   <reqsecret>0</reqsecret>\n\
         </qos>"
    )
}

/// Builds the `/qos/firewall` response body.
///
/// The client reads `.numinterfaces`, `.ips.ips` and `.ports.ports` from
/// the root element `firewall`. Both `.ips.ips` and `.ports.ports` must
/// resolve, otherwise the whole request fails with -4 (`0x141FE87D9` /
/// `0x141FE87F0`).
///
/// The two list elements hold one child element per interface: the
/// client walks the children of `.ips.ips` and `.ports.ports` in lockstep
/// (`0x141FE8822`) and reads each pair as an address/port tuple.
fn firewall_response_body() -> String {
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <firewall>\n\
         \x20   <numinterfaces>1</numinterfaces>\n\
         \x20   <ips>\n\
         \x20       <ips>{QOS_IP_DECIMAL}</ips>\n\
         \x20   </ips>\n\
         \x20   <ports>\n\
         \x20       <ports>{QOS_UDP_PORT}</ports>\n\
         \x20   </ports>\n\
         \x20   <requestid>1</requestid>\n\
         \x20   <reqsecret>0</reqsecret>\n\
         </firewall>"
    )
}

/// Builds the `/qos/firetype` response body.
///
/// The client reads the root element `firetype` and then its `.firetype`
/// child (`0x141FE88DE` / `0x141FE88FA`).
fn firetype_response_body() -> String {
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <firetype>\n\
         \x20   <firetype>{QOS_FIRETYPE}</firetype>\n\
         </firetype>"
    )
}

/// Builds the `/getPeerAddress` response body.
///
/// The client sends
/// `getPeerAddress?myIP=<ip>&myPort=<port>&version=1.0` and then reads
/// `peerIP` and `peerPort` out of the response body (`0x144AA151D` /
/// `0x144AA159C`). Both are looked up as `name=value` pairs separated by
/// newlines, and the IP is parsed with a dotted-quad parser
/// (`0x140C96010`).
///
/// `sourceIP` / `sourcePort` / `tag` / `sendCount` are the fields the
/// client writes into its own request, so they are echoed back in the
/// same `name=value` shape.
fn get_peer_address_response_body(req: &HttpRequest) -> String {
    let query = req.path.split_once('?').map(|(_, q)| q).unwrap_or("");
    let mut my_ip = "127.0.0.1".to_string();
    let mut my_port: u32 = 0;
    for pair in query.split('&') {
        if let Some((k, v)) = pair.split_once('=') {
            match k {
                "myIP" => my_ip = v.to_string(),
                "myPort" => my_port = v.parse().unwrap_or(0),
                _ => {}
            }
        }
    }
    if my_ip.is_empty() {
        my_ip = "127.0.0.1".to_string();
    }

    format!(
        "sourceIP={my_ip}\r\n\
         sourcePort={my_port}\r\n\
         tag=emulator\r\n\
         sendCount=1\r\n\
         peerIP={my_ip}\r\n\
         peerPort={QOS_UDP_PORT}\r\n"
    )
}

/// Routes HTTP requests for telemetry sinks and squad content distribution.
pub fn route_http_request(req: &HttpRequest) -> HttpResponse {
    let path = req.path.split('?').next().unwrap_or(&req.path);

    tracing::debug!("[HTTP Sink] {} {}", req.method, path);

    match path {
        // Fast telemetry sink endpoints for river / pinEvents
        "/pinEvents" | "/river" | "/telemetry" | "/" => {
            tracing::info!("[Telemetry Sink] Absorbed {} telemetry ping", req.method);
            HttpResponse::ok_json(r#"{"status":"ok"}"#)
        }
        // DirtySDK HTTP QoS metadata discovery probes.
        //
        // These must be matched *before* the Nucleus arm below: the
        // catch-all Nucleus pattern matches any path containing "auth",
        // and `/qos/firewall` does not, but `/qos/qos` must not be
        // swallowed by the telemetry fall-through either.
        "/qos/qos" => {
            tracing::info!("[QoS HTTP] Serving QoS configuration metadata");
            HttpResponse::ok_xml(&qos_response_body())
        }
        "/qos/firewall" => {
            tracing::info!("[QoS HTTP] Serving firewall metadata");
            HttpResponse::ok_xml(&firewall_response_body())
        }
        "/qos/firetype" => {
            tracing::info!("[QoS HTTP] Serving firewall-type classification");
            HttpResponse::ok_xml(&firetype_response_body())
        }
        "/getPeerAddress" => {
            tracing::info!("[QoS HTTP] Serving peer address discovery");
            HttpResponse::ok_xml(&get_peer_address_response_body(req))
        }
        // Synthetic EA Nucleus / Gateway / Identity Auth Token endpoint
        p if p.contains("token") || p.contains("auth") || p.contains("login") || p.contains("pid") => {
            tracing::info!("[Nucleus Sink] Serving synthetic Nucleus Auth token for {}", path);
            let json = r#"{"access_token":"f22_mock_nucleus_token_deadpool","token_type":"Bearer","expires_in":31536000,"id_token":"mock_id_token","pid_id":76561198956360709,"persona_id":76561198956360709,"status":"SUCCESS"}"#;
            HttpResponse::ok_json(json)
        }
        // Canonical Squad Hash query
        "/content/squads/hash" => {
            let (hash, size) = SquadStore::global().get_hash_and_size();
            let json = format!(r#"{{"sha256":"{}","size":{}}}"#, hash, size);
            HttpResponse::ok_json(&json)
        }
        // Canonical Squad Database upload
        "/content/squads/upload" => {
            let (hash, size) = SquadStore::global().update_squad(req.body.clone());
            let json = format!(r#"{{"status":"ok","sha256":"{}","size":{}}}"#, hash, size);
            HttpResponse::ok_json(&json)
        }
        // Canonical Squad Database file distribution
        "/content/squads" | "/content/squads/canonical_squads.db" => {
            tracing::info!("[Squad Content] Serving canonical squad database");
            let bytes = SquadStore::global().get_squad_bytes();
            HttpResponse::ok_binary(bytes)
        }
        // Real-time UDP Netcode Metrics endpoint
        "/metrics" => {
            tracing::info!("[HTTP Content] Serving live /metrics for Netcode HUD");
            let json = NetcodeHttpMetrics::global().to_json();
            HttpResponse::ok_json(&json)
        }
        // MatchReady endpoint (/ready, /match/ready, /cards/ready, /matchready)
        p if p.contains("ready") => {
            tracing::info!("[HTTP Sink] Serving RS4:FutMatchReadyServerResponse for {}", path);
            HttpResponse::match_ready_response(1, 2, 1)
        }
        // EA / Cards Server Status & Availability Probe (EOS bypass)
        p if p.contains("status") || p.contains("server_status") || p.contains("serverStatus") || p.contains("health") || p.contains("dime") => {
            tracing::info!("[Server Status] Serving active ONLINE server status for {}", path);
            let json = r#"{"status":"online","code":0,"server_status":0,"serverStatus":0,"messages":[],"protocolVersion":1,"data":{"status":"online","code":0,"available":true}}"#;
            HttpResponse::ok_json(json)
        }
        _ => {
            // Fallback 200 OK for any unmapped EA telemetry/ad path to prevent game freeze
            tracing::info!("[HTTP Sink] Fast-pathing unmapped endpoint: {}", path);
            HttpResponse::ok_json(r#"{"status":"ok","code":0,"server_status":0}"#)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fut_match_ready_response_serialization_and_deserialization() {
        let resp = FutMatchReadyServerResponse::new(42, 1002);
        let json = resp.to_json();

        assert!(json.contains("\"matchId\":42"));
        assert!(json.contains("\"opponentPersonaId\":1002"));
        assert!(json.contains("\"opponentPersonaIds\":[1002]"));
        assert!(json.contains("\"items\":[]"));

        let deserialized = FutMatchReadyServerResponse::from_json(&json)
            .expect("Must successfully deserialize RS4:FutMatchReadyServerResponse");
        assert_eq!(deserialized.match_id, 42);
        assert_eq!(deserialized.opponent_persona_id, 1002);
        assert_eq!(deserialized.opponent_persona_ids, vec![1002]);
        assert!(deserialized.items.is_empty());
    }

    #[test]
    fn test_match_ready_http_route_and_headers() {
        let raw_req = b"GET /cards/ready HTTP/1.1\r\nHost: easw.easports.com:8099\r\n\r\n";
        let req = parse_http_request(raw_req).expect("Failed to parse request");
        let resp = route_http_request(&req);

        assert_eq!(resp.status_code, 200);
        let resp_bytes = resp.to_bytes();
        let resp_str = String::from_utf8_lossy(&resp_bytes);

        assert!(resp_str.contains("Cache-Control: max-age=1"));
        assert!(resp_str.contains("X-Cache-Control: max-age=1"));

        let deserialized = FutMatchReadyServerResponse::from_json(&String::from_utf8_lossy(&resp.body))
            .expect("HTTP body must deserialize to FutMatchReadyServerResponse");
        assert_eq!(deserialized.match_id, 1);
        assert_eq!(deserialized.opponent_persona_id, 2);
    }

    #[test]
    fn test_telemetry_pin_events_returns_200_ok() {
        let raw_req = b"POST /pinEvents HTTP/1.1\r\nHost: river.data.ea.com\r\nContent-Length: 18\r\n\r\n{\"event\":\"startup\"}";
        let req = parse_http_request(raw_req).expect("Failed to parse request");

        assert_eq!(req.method, "POST");
        assert_eq!(req.path, "/pinEvents");

        let resp = route_http_request(&req);
        assert_eq!(resp.status_code, 200);
        let resp_bytes = resp.to_bytes();
        let resp_str = String::from_utf8_lossy(&resp_bytes);
        assert!(resp_str.contains("HTTP/1.1 200 OK"));
        assert!(resp_str.contains(r#"{"status":"ok"}"#));
    }

    #[test]
    fn test_squad_endpoints() {
        let raw_hash = b"GET /content/squads/hash HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n";
        let req_hash = parse_http_request(raw_hash).unwrap();
        let resp_hash = route_http_request(&req_hash);
        assert_eq!(resp_hash.status_code, 200);
        let hash_str = String::from_utf8_lossy(&resp_hash.body);
        assert!(hash_str.contains("sha256"));

        let raw_upload = b"POST /content/squads/upload HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Length: 10\r\n\r\nNEWSQUADS1";
        let req_upload = parse_http_request(raw_upload).unwrap();
        let resp_upload = route_http_request(&req_upload);
        assert_eq!(resp_upload.status_code, 200);

        let raw_squad = b"GET /content/squads/canonical_squads.db HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n";
        let req_squad = parse_http_request(raw_squad).unwrap();
        let resp_squad = route_http_request(&req_squad);
        assert_eq!(resp_squad.status_code, 200);
        assert_eq!(resp_squad.content_type, "application/octet-stream");
        assert_eq!(resp_squad.body, b"NEWSQUADS1");
    }

    #[test]
    fn test_qos_xml_endpoints() {
        let raw_qos = b"GET /qos/qos HTTP/1.1\r\nHost: gosqos.ea.com\r\n\r\n";
        let req_qos = parse_http_request(raw_qos).unwrap();
        let resp_qos = route_http_request(&req_qos);
        assert_eq!(resp_qos.status_code, 200);
        assert_eq!(resp_qos.content_type, "application/xml");
        let body = String::from_utf8(resp_qos.body).unwrap();
        assert!(body.contains("<qosport>17498</qosport>"));
    }

    /// The QoS response is read by the client with a path-based DOM lookup:
    /// `qos` -> `qosport` / `qosip` / `numprobes` / ...
    ///
    /// Verified against FIFA22.exe: the parser at `0x141FE8580` looks up the
    /// root element `qos` and then reads `.qosport` and `.qosip` as direct
    /// children of it. It then *requires* both to be non-zero
    /// (`0x141FE85EC` / `0x141FE85F6`), otherwise the QoS step fails.
    #[test]
    fn test_qos_response_has_the_fields_the_client_reads() {
        let raw_qos = b"GET /qos/qos HTTP/1.1\r\nHost: qosapi\r\n\r\n";
        let req = parse_http_request(raw_qos).unwrap();
        let resp = route_http_request(&req);
        let body = String::from_utf8(resp.body).unwrap();

        // The client reads these names; they must exist in the document.
        for field in ["qosport", "qosip", "numprobes", "probesize"] {
            assert!(
                body.contains(&format!("<{}>", field)),
                "the QoS response must contain a <{}> element; the client reads it by name",
                field
            );
        }

        // `qosip` must be a non-zero IPv4 integer, `qosport` a non-zero port.
        let qosip = extract_element(&body, "qosip").expect("<qosip> must be present");
        let qosip: u32 = qosip.parse().expect("<qosip> must be an integer");
        assert!(
            qosip != 0,
            "the client aborts the QoS step when qosip is 0 (FIFA22.exe 0x141FE85EC)"
        );

        let qosport = extract_element(&body, "qosport").expect("<qosport> must be present");
        let qosport: u32 = qosport.parse().expect("<qosport> must be an integer");
        assert!(
            qosport != 0,
            "the client aborts the QoS step when qosport is 0 (FIFA22.exe 0x141FE85F6)"
        );

        // `probesize` must be non-zero and `numprobes` must be >= 2,
        // otherwise the type-2 (latency-probe) QoS request fails with -4.
        //
        //   0x141FE8753  cmp dword ptr [rbx + 0x44], 2   ; .numprobes
        //   0x141FE8757  jb  0x141FE875F                 ; < 2  -> fail
        //   0x141FE8759  cmp dword ptr [rbx + 0x40], 0   ; .probesize
        //   0x141FE875D  jne 0x141FE8769                 ; != 0 -> success
        //   0x141FE875F  mov eax, 0xfffffffc             ; fail: return -4
        let probesize: u32 = extract_element(&body, "probesize")
            .expect("<probesize> must be present")
            .parse()
            .expect("<probesize> must be an integer");
        assert_ne!(
            probesize, 0,
            "the client requires probesize != 0 (FIFA22.exe 0x141FE8759)"
        );

        let numprobes: u32 = extract_element(&body, "numprobes")
            .expect("<numprobes> must be present")
            .parse()
            .expect("<numprobes> must be an integer");
        assert!(
            numprobes >= 2,
            "the client requires numprobes >= 2 (FIFA22.exe 0x141FE8753)"
        );
    }

    /// The `firewall` response must carry `ips.ips` and `ports.ports`
    /// sub-documents; the client fails the request when either is missing
    /// (FIFA22.exe `0x141FE87D9` / `0x141FE87F0`).
    #[test]
    fn test_firewall_response_has_ips_and_ports_documents() {
        let raw = b"GET /qos/firewall HTTP/1.1\r\nHost: qosapi\r\n\r\n";
        let req = parse_http_request(raw).unwrap();
        let resp = route_http_request(&req);
        assert_eq!(resp.status_code, 200);
        let body = String::from_utf8(resp.body).unwrap();

        assert!(
            body.contains("<ips>") && body.contains("<ports>"),
            "the firewall response must carry nested <ips> and <ports> documents"
        );
        assert!(
            body.contains("<numinterfaces>"),
            "the client reads .numinterfaces from the firewall response"
        );
    }

    /// The client's firewall parser walks the children of `.ips.ips` and
    /// `.ports.ports` in lockstep and reads each pair as an address/port
    /// tuple, so the two lists must contain the *same* number of entries.
    #[test]
    fn test_firewall_ip_and_port_lists_have_matching_lengths() {
        let raw = b"GET /qos/firewall HTTP/1.1\r\nHost: qosapi\r\n\r\n";
        let req = parse_http_request(raw).unwrap();
        let resp = route_http_request(&req);
        let body = String::from_utf8(resp.body).unwrap();

        let numinterfaces: u32 = extract_element(&body, "numinterfaces")
            .expect("<numinterfaces> must be present")
            .parse()
            .expect("<numinterfaces> must be an integer");

        let ips = extract_list(&body, "ips");
        let ports = extract_list(&body, "ports");
        assert_eq!(
            ips.len(),
            ports.len(),
            "the client pairs each <ips> entry with a <ports> entry"
        );
        assert_eq!(
            ips.len() as u32,
            numinterfaces,
            "the number of listed interfaces must match <numinterfaces>"
        );
    }

    /// The `firetype` response is read as `firetype` -> `.firetype`.
    #[test]
    fn test_firetype_response_carries_the_firetype_value() {
        let raw = b"GET /qos/firetype HTTP/1.1\r\nHost: qosapi\r\n\r\n";
        let req = parse_http_request(raw).unwrap();
        let resp = route_http_request(&req);
        assert_eq!(resp.status_code, 200);
        let body = String::from_utf8(resp.body).unwrap();
        assert!(
            body.contains("<firetype>"),
            "the firetype response must carry a <firetype> element"
        );
    }

    /// `getPeerAddress` answers the NAT-traversal peer probe. The client
    /// reads `sourceIP`, `sourcePort`, `peerIP` and `peerPort` from the
    /// response body.
    #[test]
    fn test_get_peer_address_endpoint() {
        let raw = b"GET /getPeerAddress?myIP=127.0.0.1&myPort=5000&version=1.0 HTTP/1.1\r\nHost: qosapi\r\n\r\n";
        let req = parse_http_request(raw).unwrap();
        let resp = route_http_request(&req);
        assert_eq!(resp.status_code, 200);
        let body = String::from_utf8(resp.body).unwrap();
        for field in ["sourceIP", "sourcePort", "peerIP", "peerPort"] {
            assert!(
                body.contains(field),
                "the getPeerAddress response must carry `{}`",
                field
            );
        }
    }

    /// Extracts the text content of the first `<name>...</name>` element.
    fn extract_element(xml: &str, name: &str) -> Option<String> {
        let open = format!("<{}>", name);
        let close = format!("</{}>", name);
        let start = xml.find(&open)? + open.len();
        let end = xml[start..].find(&close)? + start;
        Some(xml[start..end].trim().to_string())
    }

    /// Extracts the text content of every child element of the first
    /// `<name>...</name>` element.
    fn extract_list(xml: &str, name: &str) -> Vec<String> {
        let open = format!("<{}>", name);
        let close = format!("</{}>", name);
        let start = match xml.find(&open) {
            Some(i) => i + open.len(),
            None => return Vec::new(),
        };
        let end = match xml[start..].find(&close) {
            Some(i) => i + start,
            None => return Vec::new(),
        };
        let inner = &xml[start..end];
        let mut out = Vec::new();
        let mut cursor = 0usize;
        while let Some(rel) = inner[cursor..].find('<') {
            let abs = cursor + rel;
            let name_end = match inner[abs + 1..].find('>') {
                Some(i) => abs + 1 + i,
                None => break,
            };
            let tag = &inner[abs + 1..name_end];
            if !tag.starts_with('/') {
                let text_start = name_end + 1;
                let text_end = match inner[text_start..].find('<') {
                    Some(i) => text_start + i,
                    None => inner.len(),
                };
                out.push(inner[text_start..text_end].trim().to_string());
                cursor = text_end;
            } else {
                cursor = name_end + 1;
            }
        }
        out
    }

    /// `qosip` is read by the client with a *decimal* integer parser, and
    /// the resulting u32 is split into four bytes most-significant-first
    /// (`0x141FE8B6A`: `al` -> byte 0, then `shr eax, 8` per byte). The
    /// value is therefore a big-endian IPv4 address, not a dotted quad.
    #[test]
    fn test_qosip_is_a_big_endian_decimal_ipv4_address() {
        let raw = b"GET /qos/qos HTTP/1.1\r\nHost: qosapi\r\n\r\n";
        let req = parse_http_request(raw).unwrap();
        let resp = route_http_request(&req);
        let body = String::from_utf8(resp.body).unwrap();

        let qosip: u32 = extract_element(&body, "qosip")
            .expect("<qosip> must be present")
            .parse()
            .expect("<qosip> must be a decimal integer, not a dotted quad");

        // The client reassembles the address as
        //   byte0 = (v >> 24), byte1 = (v >> 16), byte2 = (v >> 8), byte3 = v
        let octets = [
            (qosip >> 24) as u8,
            (qosip >> 16) as u8,
            (qosip >> 8) as u8,
            qosip as u8,
        ];
        assert_eq!(
            octets,
            [127, 0, 0, 1],
            "qosip must decode to 127.0.0.1 in the client's byte order"
        );
    }

    /// The client never reads `qoshost` or `serviceid`; those names do not
    /// appear anywhere in FIFA22.exe. Shipping them is harmless, but
    /// shipping them *instead of* `qosip` is fatal.
    #[test]
    fn test_qos_response_does_not_leave_out_qosip() {
        let raw = b"GET /qos/qos HTTP/1.1\r\nHost: qosapi\r\n\r\n";
        let req = parse_http_request(raw).unwrap();
        let resp = route_http_request(&req);
        let body = String::from_utf8(resp.body).unwrap();
        assert!(
            !body.contains("qoshost"),
            "the client never reads <qoshost>; it is noise"
        );
        assert!(
            !body.contains("serviceid"),
            "the client never reads <serviceid>; it is noise"
        );
    }

    #[test]
    fn test_metrics_endpoint() {
        NetcodeHttpMetrics::global().update(24, 3, 0, 42);
        let raw_req = b"GET /metrics HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n";
        let req = parse_http_request(raw_req).unwrap();
        let resp = route_http_request(&req);
        assert_eq!(resp.status_code, 200);
        let body = String::from_utf8(resp.body).unwrap();
        assert!(body.contains("\"rtt_ms\":24"));
        assert!(body.contains("\"jitter_ms\":3"));
        assert!(body.contains("\"dropped_frames\":0"));
        assert!(body.contains("\"cipher_reuse_count\":42"));
    }
}
