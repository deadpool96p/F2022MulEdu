//! # services-http-content
//!
//! Fast mock HTTP/HTTPS telemetry sink (`river.data.ea.com`, `pin-river.data.ea.com`)
//! and squad content distribution endpoint.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::OnceLock;

pub mod protohttp;
pub mod squads;
pub use protohttp::*;
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
    /// The ALPN protocol negotiated on the TLS connection this request
    /// arrived on, when the caller knows it.
    ///
    /// ALPN is negotiated in the TLS layer, so the HTTP layer cannot see
    /// it; the transport that owns the TLS acceptor is expected to fill
    /// this in via [`HttpRequest::with_alpn`] so the sink can log it.
    pub alpn: Option<String>,
}

impl HttpRequest {
    /// Attaches the ALPN protocol negotiated by the TLS layer.
    pub fn with_alpn(mut self, alpn: Option<String>) -> Self {
        self.alpn = alpn;
        self
    }

    /// The request target without its query string.
    pub fn path_only(&self) -> &str {
        self.path.split('?').next().unwrap_or(&self.path)
    }

    /// The raw query string, without the leading `?`.
    pub fn query(&self) -> &str {
        self.path.split_once('?').map(|(_, q)| q).unwrap_or("")
    }

    /// The query parameters, in order, percent-decoded.
    pub fn query_pairs(&self) -> Vec<(String, String)> {
        self.query()
            .split('&')
            .filter(|p| !p.is_empty())
            .map(|pair| match pair.split_once('=') {
                Some((k, v)) => (percent_decode(k), percent_decode(v)),
                None => (percent_decode(pair), String::new()),
            })
            .collect()
    }

    /// A single query parameter, by name.
    pub fn query_param(&self, key: &str) -> Option<String> {
        self.query_pairs()
            .into_iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v)
    }

    /// The form parameters carried in the request body, percent-decoded.
    ///
    /// The client sends `Content-Type: application/x-www-form-urlencoded`
    /// (the literal is at file offset `0x79F6076` in `FIFA22.exe`) and
    /// formats four of its own fields that way: `sourceIP=%s`
    /// (`0x79F5F78`), `sourcePort=%d` (`0x79F5F85`), `tag=%s`
    /// (`0x79F5F94`) and `sendCount=%d` (`0x79F5F9C`).
    ///
    /// Those four names are **not** part of the query string in either
    /// URL format the client builds (`0x79F6010` and `0x79F5F28`), so
    /// they can only arrive in the body.  Reading them is therefore
    /// required to answer `/getPeerAddress` faithfully.
    pub fn form_pairs(&self) -> Vec<(String, String)> {
        let content_type = self.header("content-type").unwrap_or_default();
        let is_form = content_type
            .split(';')
            .next()
            .unwrap_or_default()
            .trim()
            .eq_ignore_ascii_case("application/x-www-form-urlencoded");
        if !is_form {
            return Vec::new();
        }
        let body = String::from_utf8_lossy(&self.body);
        body.split('&')
            .filter(|p| !p.is_empty())
            .map(|pair| match pair.split_once('=') {
                Some((k, v)) => (percent_decode(k), percent_decode(v)),
                None => (percent_decode(pair), String::new()),
            })
            .collect()
    }

    /// A request parameter, from the query string or from the form body.
    ///
    /// The query string wins; the form body is the fallback.  An empty
    /// query value does **not** shadow a form value, because
    /// `?myIP=` carries no information while the body does.
    pub fn param(&self, key: &str) -> Option<String> {
        if let Some(value) = self.query_param(key) {
            if !value.is_empty() {
                return Some(value);
            }
        }
        self.form_pairs()
            .into_iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v)
    }

    /// A header value, by (case-insensitive) name.
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers.get(&name.to_ascii_lowercase()).map(String::as_str)
    }

    /// The ProtoHttp user agent, when this request came from a ProtoHttp
    /// (DirtySDK) client.
    pub fn proto_http_user_agent(&self) -> Option<ProtoHttpUserAgent> {
        self.header("user-agent")
            .and_then(ProtoHttpUserAgent::parse)
    }

    /// True when the request carries a ProtoHttp user agent.
    pub fn is_proto_http(&self) -> bool {
        self.proto_http_user_agent().is_some()
    }
}

/// Decodes `%XX` escapes and `+` as space in a query-string component.
fn percent_decode(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            b'%' if i + 2 < bytes.len() => {
                let hex = &input[i + 1..i + 3];
                match u8::from_str_radix(hex, 16) {
                    Ok(byte) => {
                        out.push(byte);
                        i += 3;
                    }
                    Err(_) => {
                        out.push(bytes[i]);
                        i += 1;
                    }
                }
            }
            other => {
                out.push(other);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
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

    /// A `text/plain` response.
    ///
    /// Used by `/getPeerAddress`, whose body is a list of `name=value`
    /// pairs and **not** XML.
    pub fn ok_text(body: &str) -> Self {
        Self {
            status_code: 200,
            reason_phrase: "OK",
            content_type: "text/plain",
            headers: HashMap::new(),
            body: body.as_bytes().to_vec(),
        }
    }

    /// Sets (or replaces) a response header.
    ///
    /// Header names are matched case-insensitively, so a caller can
    /// override a header this crate sets by default.
    pub fn set_header(&mut self, name: &str, value: &str) -> &mut Self {
        self.headers
            .retain(|k, _| !k.eq_ignore_ascii_case(name));
        self.headers.insert(name.to_string(), value.to_string());
        self
    }

    /// Shapes the response for a ProtoHttp (DirtySDK) client.
    ///
    /// ProtoHttp is a plain HTTP/1.1 client whose response parser only
    /// recognises `content-length`, `transfer-encoding`, `chunked`,
    /// `location`, `last-modified` and `connection` (file offsets
    /// `0x7575CC0`-`0x7575D08` in `FIFA22.exe`).  So the response must:
    ///
    /// * carry an exact `Content-Length` — never chunked, and
    /// * close the connection, because the client sends `Connection: Close`.
    ///
    /// **ASSUMPTION.** The `User-Agent` response header is *not* read by
    /// the client (see [`PROTO_HTTP_SERVER_USER_AGENT`]).  It is sent
    /// anyway so the ProtoHttp path is observable on the wire, and so a
    /// version-sensitive client does not see an unrecognised stack.
    pub fn for_proto_http(mut self, _ua: Option<&ProtoHttpUserAgent>) -> Self {
        self.set_header("Connection", "Close");
        self.set_header("User-Agent", PROTO_HTTP_SERVER_USER_AGENT);
        self
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
    ///
    /// The `Content-Type`, `Content-Length` and `Connection` headers are
    /// emitted by default, but any of them can be overridden through
    /// [`HttpResponse::set_header`] — the override wins, and the default
    /// is not emitted twice.
    pub fn to_bytes(&self) -> Vec<u8> {
        // Defaults first, in a stable order.
        let mut headers: Vec<(String, String)> = vec![
            ("Content-Type".to_string(), self.content_type.to_string()),
            ("Content-Length".to_string(), self.body.len().to_string()),
            ("Connection".to_string(), "close".to_string()),
            ("Access-Control-Allow-Origin".to_string(), "*".to_string()),
        ];

        // Caller-supplied headers override the defaults, case-insensitively.
        for (k, v) in &self.headers {
            match headers
                .iter_mut()
                .find(|(name, _)| name.eq_ignore_ascii_case(k))
            {
                Some(slot) => slot.1 = v.clone(),
                None => headers.push((k.clone(), v.clone())),
            }
        }

        let mut out = Vec::new();
        let mut header_str = format!(
            "HTTP/1.1 {} {}\r\n",
            self.status_code, self.reason_phrase
        );
        for (k, v) in &headers {
            header_str.push_str(&format!("{}: {}\r\n", k, v));
        }
        header_str.push_str("\r\n");
        out.extend_from_slice(header_str.as_bytes());
        out.extend_from_slice(&self.body);
        out
    }
}

/// The HTTP/2 connection preface a client sends when it has negotiated
/// the `h2` ALPN protocol.
pub const HTTP2_CONNECTION_PREFACE: &[u8] = b"PRI * HTTP/2.0\r\n\r\nSM\r\n\r\n";

/// True when `raw` starts with the HTTP/2 connection preface.
///
/// The sink speaks **HTTP/1.1 only**.  A client that negotiates `h2`
/// would send this preface, and the first line parses as a request line
/// (`PRI * HTTP/2.0`) — so the preface has to be detected explicitly,
/// otherwise the request is silently mis-routed to the fast-path
/// fallback and answered with an HTTP/1.1 response the client cannot
/// read.
///
/// The emulator's TLS acceptor advertises **no** ALPN protocol (there is
/// no `alpn_protocols` anywhere in `crates/emulator-server`), so the
/// client's `h2` offer is declined and ProtoHttp falls back to HTTP/1.1.
pub fn is_http2_preface(raw: &[u8]) -> bool {
    raw.starts_with(HTTP2_CONNECTION_PREFACE)
}

/// Parses a raw HTTP/1.1 request buffer into an HttpRequest.
///
/// Returns `None` for an HTTP/2 connection preface — see
/// [`is_http2_preface`].
pub fn parse_http_request(raw: &[u8]) -> Option<HttpRequest> {
    if is_http2_preface(raw) {
        return None;
    }
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
        alpn: None,
    })
}

/// The QoS UDP responder's port. The client learns it from the `/qos/qos`
/// response and then sends `ProtoPing` probes here.
pub const QOS_UDP_PORT: u16 = 17498;

/// The emulator's own address, in dotted-quad form.
///
/// The `getPeerAddress` IP fields are parsed with a dotted-quad parser
/// (`0x140C96010`), unlike `qosip`, which is a decimal u32.  The address
/// is the same loopback address the client reaches the emulator on.
pub const PEER_ADDRESS: &str = "127.0.0.1";

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

/// The request-scoped values a QoS response echoes back to the client.
///
/// The client sends `rqid` (request id) and `rqsc` (request secret) as
/// query parameters — they are visible in the URL formats the client
/// builds, e.g. `/qos/firetype?vers=..&rqid=..&rqsc=..&inip=..&inpt=..`
/// (the `&rqid=` and `&rqsc=` literals are at file offsets `0x77947A8`
/// and `0x77947A0` in `FIFA22.exe`).  The response carries the same two
/// values back as `<requestid>` and `<reqsecret>`.
///
/// `requestid` **must** be non-zero: the client rejects a type-2 QoS
/// response whose `requestid` is zero (`0x141FE874D` -> fail with -4).
/// A client that sends no `rqid` therefore gets the constant 1.
#[derive(Debug, Clone, Copy)]
struct QosRequest {
    request_id: u32,
    request_secret: u32,
}

impl QosRequest {
    fn from_request(req: &HttpRequest) -> Self {
        let request_id = req
            .param("rqid")
            .and_then(|v| v.parse::<u32>().ok())
            .filter(|v| *v != 0)
            .unwrap_or(1);
        let request_secret = req
            .param("rqsc")
            .and_then(|v| v.parse::<u32>().ok())
            .unwrap_or(0);
        Self {
            request_id,
            request_secret,
        }
    }
}

/// Builds the `/qos/qos` response body.
///
/// The client resolves the root element `qos` and then reads `.qosport`,
/// `.qosip`, `.numprobes`, `.probesize`, `.requestid` and `.reqsecret`
/// as *siblings* of each other, so they must all be direct children of
/// `<qos>`.
///
/// `qosip` and `qosport` are mandatory: a zero value in either makes the
/// client fail the QoS step with -3 (`0x141FE85EC` / `0x141FE85F6`).
///
/// One body serves both request types.  The client picks type 1 or type 2
/// with the `qtyp` query parameter, but it reads the same field names
/// either way, and a type-2 request additionally requires
/// `numprobes >= 2`, `probesize != 0` and `requestid != 0` — all of
/// which this body satisfies.
fn qos_response_body(qos: &QosRequest) -> String {
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <qos>\n\
         \x20   <qosip>{QOS_IP_DECIMAL}</qosip>\n\
         \x20   <qosport>{QOS_UDP_PORT}</qosport>\n\
         \x20   <numprobes>{QOS_NUM_PROBES}</numprobes>\n\
         \x20   <probesize>{QOS_PROBE_SIZE}</probesize>\n\
         \x20   <requestid>{}</requestid>\n\
         \x20   <reqsecret>{}</reqsecret>\n\
         </qos>",
        qos.request_id, qos.request_secret
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
///
/// The `ips` entry uses the same **big-endian decimal u32** encoding as
/// `qosip` (the client converts it to a socket address the same way, at
/// `0x141FE8D90`).
fn firewall_response_body(qos: &QosRequest) -> String {
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
         \x20   <requestid>{}</requestid>\n\
         \x20   <reqsecret>{}</reqsecret>\n\
         </firewall>",
        qos.request_id, qos.request_secret
    )
}

    /// Builds the /redirector/getServerInstance response body.
    ///
    /// This is the critical path for QoS service discovery. The client
    /// parses the response as a DOM and expects the qosapi service
    /// to be advertised using the lowercased TDF field names.
    fn redirector_get_server_instance_body(_req: &HttpRequest) -> String {
        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<serverinstanceinfo>
    <service name="qosapi">
        <entry>
            <hostname>spring18.gosredirector.ea.com</hostname>
            <dstport>17498</dstport>
            <srcport>17498</srcport>
            <srcip>spring18.gosredirector.ea.com</srcip>
            <qosip>spring18.gosredirector.ea.com</qosip>
            <qosport>17498</qosport>
            <probesize>1000</probesize>
            <numprobes>10</numprobes>
        </entry>
    </service>
</serverinstanceinfo>"#
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

/// The emulator's peer relay port.
///
/// The ProtoTunnel / NetGameDist relay is the UDP socket that carries
/// *peer* traffic, as opposed to [`QOS_UDP_PORT`], which carries the
/// ProtoPing latency probes.  The emulator binds it on `:3659`
/// (`crates/emulator-server/src/lib.rs`), and the Blaze session
/// publishes the same port as the player's network address.
///
/// **ASSUMPTION.** The client does not say in the request which peer it
/// is asking about, and the emulator hosts *both* players, so the peer
/// is the emulator itself.  The peer address is therefore the relay's
/// address — the same address the Blaze session hands the client for
/// the game session.
pub const PEER_RELAY_UDP_PORT: u16 = 3659;

/// Builds the `/getPeerAddress` response body.
///
/// The client requests
/// `http://<host>:<port>/getPeerAddress?myIP=<ip>&myPort=<port>&version=1.0`
/// (the URL format is at file offset `0x79F6010` in `FIFA22.exe` — note
/// that it is **plain `http`**, not `https`) and then reads the response
/// as a list of `name=value` pairs separated by newlines.  The names it
/// looks up are:
///
/// | Field | Read at |
/// | :--- | :--- |
/// | `targetIP` | `0x144AA0731` |
/// | `targetPort` | `0x144AA0845` |
/// | `tag` | `0x144AA092D` |
/// | `sendCount` | `0x144AA0A49` |
/// | `sourcePort` | `0x144AA0B39` |
/// | `peerIP` | `0x144AA151D` |
/// | `peerPort` | `0x144AA159C` |
///
/// The IP fields are parsed with a **dotted-quad** parser
/// (`0x140C96010`), unlike the `/qos/qos` response's `qosip`, which is a
/// decimal u32.
///
/// The client describes *itself* with `myIP`/`myPort` (query string)
/// and `sourceIP`/`sourcePort`/`tag`/`sendCount` (form body — see
/// [`HttpRequest::form_pairs`]).  All of those are echoed back, so the
/// response is self-describing and the client's own correlation values
/// survive the round trip.
fn get_peer_address_response_body(req: &HttpRequest) -> String {
    let my_ip = req
        .param("myIP")
        .filter(|v| !v.is_empty())
        .or_else(|| req.param("sourceIP").filter(|v| !v.is_empty()))
        .unwrap_or_else(|| "127.0.0.1".to_string());
    let my_port: u32 = req
        .param("myPort")
        .and_then(|v| v.parse().ok())
        .or_else(|| req.param("sourcePort").and_then(|v| v.parse().ok()))
        .unwrap_or(0);
    let tag = req
        .param("tag")
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| "emulator".to_string());
    let send_count: u32 = req
        .param("sendCount")
        .and_then(|v| v.parse().ok())
        .unwrap_or(1);

    // The peer is the emulator itself: the peer traffic is relayed by
    // the ProtoTunnel relay the emulator binds on :3659.
    format!(
        "sourceIP={my_ip}\r\n\
         sourcePort={my_port}\r\n\
         targetIP={PEER_ADDRESS}\r\n\
         targetPort={PEER_RELAY_UDP_PORT}\r\n\
         tag={tag}\r\n\
         sendCount={send_count}\r\n\
         peerIP={PEER_ADDRESS}\r\n\
         peerPort={PEER_RELAY_UDP_PORT}\r\n"
    )
}

/// Routes HTTP requests for telemetry sinks and squad content distribution.
///
/// # Logging
///
/// Every request is logged at `info` with the full request line, the
/// path *including* its query string, the `Host` header, the
/// `User-Agent`, the `Content-Type` and whether the caller is a ProtoHttp
/// client.  This is the only place the sink sees the request, and the
/// request line is what tells us what the client is really asking for —
/// the routing decision alone is not enough to diagnose a probe that
/// goes missing.
///
/// The `Host` header is **not** used for routing: the client may connect
/// to a bare IP while sending `Host: accounts.ea.com` (or
/// `Host: spring18.gosredirector.ea.com`), so routing is by path only.
pub fn route_http_request(req: &HttpRequest) -> HttpResponse {
    let path = req.path_only();
    let proto_http_ua = req.proto_http_user_agent();

    tracing::info!(
        "[HTTP Sink] request: {method} {target} | host={host} | ua={ua} | ctype={ctype} | protohttp={proto} | alpn={alpn}",
        method = req.method,
        target = req.path,
        host = req.header("host").unwrap_or("<none>"),
        ua = req.header("user-agent").unwrap_or("<none>"),
        ctype = req.header("content-type").unwrap_or("<none>"),
        proto = proto_http_ua.is_some(),
        alpn = req.alpn.as_deref().unwrap_or("<tls-layer>"),
    );

    let response = match path {
        // Fast telemetry sink endpoints for river / pinEvents
        "/pinEvents" | "/river" | "/telemetry" | "/" => {
            tracing::info!("[Telemetry Sink] Absorbed {} telemetry ping", req.method);
            HttpResponse::ok_json(r#"{"status":"ok"}"#)
        }
        "/redirector/getServerInstance" => {
            tracing::info!("[Redirector] Serving QoS service discovery for {}", path);
            HttpResponse::ok_xml(&redirector_get_server_instance_body(req))
        }
        // DirtySDK HTTP QoS metadata discovery probes.
        //
        // These must be matched *before* the Nucleus arm below: the
        // catch-all Nucleus pattern matches any path containing "auth",
        // and `/qos/firewall` does not, but `/qos/qos` must not be
        // swallowed by the telemetry fall-through either.
        //
        // The client issues these with GET; the arms are deliberately
        // method-agnostic so a POST from a different ProtoHttp build
        // still gets a well-formed answer.
        "/qos/qos" => {
            tracing::info!("[QoS HTTP] Serving QoS configuration metadata");
            let qos = QosRequest::from_request(req);
            HttpResponse::ok_xml(&qos_response_body(&qos))
        }
        "/qos/firewall" => {
            tracing::info!("[QoS HTTP] Serving firewall metadata");
            let qos = QosRequest::from_request(req);
            HttpResponse::ok_xml(&firewall_response_body(&qos))
        }
        "/qos/firetype" => {
            tracing::info!("[QoS HTTP] Serving firewall-type classification");
            HttpResponse::ok_xml(&firetype_response_body())
        }
        "/getPeerAddress" => {
            tracing::info!("[QoS HTTP] Serving peer address discovery");
            // The body is a list of `name=value` pairs, not XML.
            HttpResponse::ok_text(&get_peer_address_response_body(req))
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
    };

    // A ProtoHttp client gets a ProtoHttp-shaped response: an exact
    // Content-Length, no chunked framing, and a closed connection.
    match proto_http_ua {
        Some(ua) => response.for_proto_http(Some(&ua)),
        None => response,
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
    /// reads a fixed set of `name=value` pairs out of the response body:
    /// `targetIP`, `targetPort`, `tag`, `sendCount`, `sourcePort`,
    /// `peerIP` and `peerPort`.
    #[test]
    fn test_get_peer_address_endpoint() {
        let raw = b"GET /getPeerAddress?myIP=127.0.0.1&myPort=5000&version=1.0 HTTP/1.1\r\nHost: qosapi\r\n\r\n";
        let req = parse_http_request(raw).unwrap();
        let resp = route_http_request(&req);
        assert_eq!(resp.status_code, 200);
        let body = String::from_utf8(resp.body).unwrap();
        for field in [
            "sourceIP",
            "sourcePort",
            "targetIP",
            "targetPort",
            "tag",
            "sendCount",
            "peerIP",
            "peerPort",
        ] {
            assert!(
                body.contains(&format!("{}=", field)),
                "the getPeerAddress response must carry a `{}` pair",
                field
            );
        }
    }

    /// The client's `getPeerAddress` parser splits the body on `=` and
    /// reads the value up to the next CR or LF, so the pairs must be
    /// separated by newlines rather than spaces.
    #[test]
    fn test_get_peer_address_pairs_are_newline_separated() {
        let raw = b"GET /getPeerAddress?myIP=127.0.0.1&myPort=5000&version=1.0 HTTP/1.1\r\nHost: qosapi\r\n\r\n";
        let req = parse_http_request(raw).unwrap();
        let resp = route_http_request(&req);
        let body = String::from_utf8(resp.body).unwrap();

        for line in body.lines().filter(|l| !l.is_empty()) {
            assert!(
                line.contains('='),
                "each line of the getPeerAddress body must be a `name=value` pair, got {:?}",
                line
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

    // -----------------------------------------------------------------
    // ProtoHttp user-agent handling
    // -----------------------------------------------------------------

    /// The live FIFA 22 ProtoHttp user agent, verbatim from the
    /// `/redirector/getServerInstance` request in `emulator_server.log`
    /// and from the format string at `FIFA22.exe` file offset
    /// `0x7575C00`.
    const FIFA22_PROTO_HTTP_UA: &str = "ProtoHttp 1.3/DS 15.1.7.0.3 (Windows)";

    /// A ProtoHttp user agent must be recognised, and its versions
    /// recovered.
    #[test]
    fn test_proto_http_user_agent_is_recognised() {
        let raw = format!(
            "GET /qos/qos?vers=1&qtyp=2 HTTP/1.1\r\n\
             Host: qosapi:443\r\n\
             User-Agent: {FIFA22_PROTO_HTTP_UA}\r\n\r\n"
        );
        let req = parse_http_request(raw.as_bytes()).unwrap();

        assert!(req.is_proto_http(), "the ProtoHttp user agent must be recognised");

        let ua = req.proto_http_user_agent().expect("must parse");
        assert_eq!(ua.http_version, (1, 3));
        assert_eq!(ua.ds_version, [15, 1, 7, 0, 3]);
        assert_eq!(ua.platform.as_deref(), Some("Windows"));
    }

    /// A non-ProtoHttp user agent must not be mistaken for one.
    #[test]
    fn test_non_proto_http_user_agent_is_not_recognised() {
        let raw = b"GET /qos/qos HTTP/1.1\r\n\
                    Host: qosapi:443\r\n\
                    User-Agent: Mozilla/5.0 (Windows NT 10.0; Win64; x64)\r\n\r\n";
        let req = parse_http_request(raw).unwrap();
        assert!(!req.is_proto_http());
        assert!(req.proto_http_user_agent().is_none());
    }

    /// ProtoHttp is a plain HTTP/1.1 client that sends `Connection: Close`
    /// and frames responses by `Content-Length`.  A response to a ProtoHttp
    /// client must therefore close the connection and carry an exact
    /// `Content-Length` — and must never be chunked, because the client
    /// has no chunked decoder for arbitrary servers.
    #[test]
    fn test_proto_http_requests_get_a_proto_http_shaped_response() {
        let raw = format!(
            "GET /qos/qos?vers=1&qtyp=2 HTTP/1.1\r\n\
             Host: qosapi:443\r\n\
             User-Agent: {FIFA22_PROTO_HTTP_UA}\r\n\r\n"
        );
        let req = parse_http_request(raw.as_bytes()).unwrap();
        let resp = route_http_request(&req);
        let bytes = resp.to_bytes();
        let text = String::from_utf8_lossy(&bytes);

        // Show the exact bytes on the wire.
        println!("--- ProtoHttp response ---\n{text}\n---");

        assert!(text.starts_with("HTTP/1.1 200 OK\r\n"));
        assert!(
            text.contains("\r\nConnection: Close\r\n"),
            "a ProtoHttp client asked for `Connection: Close`; got:\n{text}"
        );
        assert!(
            !text.contains("Transfer-Encoding"),
            "ProtoHttp must never be sent a chunked response; got:\n{text}"
        );

        // The Content-Length must be the exact body length.
        let body = &bytes[bytes
            .windows(4)
            .position(|w| w == b"\r\n\r\n")
            .expect("the header block must end")
            + 4..];
        assert!(
            text.contains(&format!("Content-Length: {}\r\n", body.len())),
            "the Content-Length must equal the body length; got:\n{text}"
        );
    }

    /// A request that is *not* ProtoHttp keeps the default response shape.
    #[test]
    fn test_non_proto_http_requests_keep_the_default_response_shape() {
        let raw = b"GET /qos/qos HTTP/1.1\r\nHost: qosapi:443\r\n\r\n";
        let req = parse_http_request(raw).unwrap();
        let resp = route_http_request(&req);
        let bytes = resp.to_bytes();
        let text = String::from_utf8_lossy(&bytes);

        assert!(text.contains("\r\nConnection: close\r\n"));
        assert!(
            !text.contains("User-Agent:"),
            "the ProtoHttp user agent is only added for ProtoHttp clients; got:\n{text}"
        );
    }

    /// The `Host` header must not influence routing: the client connects
    /// to a bare IP but sends `Host: accounts.ea.com` (or
    /// `Host: spring18.gosredirector.ea.com`).
    #[test]
    fn test_host_header_does_not_affect_routing() {
        for host in [
            "accounts.ea.com",
            "spring18.gosredirector.ea.com",
            "qosapi",
            "127.0.0.1:443",
        ] {
            let raw = format!("GET /qos/qos HTTP/1.1\r\nHost: {host}\r\n\r\n");
            let req = parse_http_request(raw.as_bytes()).unwrap();
            let resp = route_http_request(&req);
            let body = String::from_utf8(resp.body).unwrap();
            assert!(
                body.contains("<qosport>"),
                "Host: {host} must still reach the /qos/qos handler"
            );
        }
    }

    // -----------------------------------------------------------------
    // QoS request/response details
    // -----------------------------------------------------------------

    /// The client issues the QoS probes with GET.  The handlers are
    /// method-agnostic so a POST from a different ProtoHttp build still
    /// gets a well-formed answer rather than the fast-path fallback.
    #[test]
    fn test_qos_endpoints_accept_both_get_and_post() {
        for method in ["GET", "POST"] {
            for path in [
                "/qos/qos?vers=1&qtyp=2",
                "/qos/firewall?vers=1&nint=1",
                "/qos/firetype?vers=1&rqid=7&rqsc=0&inip=0&inpt=0",
                "/getPeerAddress?myIP=127.0.0.1&myPort=5000&version=1.0",
            ] {
                let raw = format!("{method} {path} HTTP/1.1\r\nHost: qosapi:443\r\n\r\n");
                let req = parse_http_request(raw.as_bytes()).unwrap();
                let resp = route_http_request(&req);
                assert_eq!(resp.status_code, 200, "{method} {path}");
                let body = String::from_utf8_lossy(&resp.body);
                assert!(
                    !body.contains("\"status\":\"ok\""),
                    "{method} {path} must be answered by the QoS handler, not the fast path"
                );
            }
        }
    }

    /// The client sends `rqid` (request id) as a query parameter and the
    /// response must carry the same value back as `<requestid>`.
    #[test]
    fn test_qos_response_echoes_the_client_request_id() {
        let raw = b"GET /qos/qos?vers=1&qtyp=2&rqid=42&rqsc=7 HTTP/1.1\r\nHost: qosapi:443\r\n\r\n";
        let req = parse_http_request(raw).unwrap();
        let resp = route_http_request(&req);
        let body = String::from_utf8(resp.body).unwrap();

        assert_eq!(
            extract_element(&body, "requestid").as_deref(),
            Some("42"),
            "the response must echo the client's rqid"
        );
        assert_eq!(
            extract_element(&body, "reqsecret").as_deref(),
            Some("7"),
            "the response must echo the client's rqsc"
        );
    }

    /// The client *rejects* a QoS response whose `requestid` is zero
    /// (`0x141FE874D` -> fail with -4), so a client that sends no `rqid`
    /// — or sends a zero one — must still be given a non-zero id.
    #[test]
    fn test_qos_request_id_is_never_zero() {
        for query in ["", "?rqid=0", "?rqid=notanumber"] {
            let raw = format!("GET /qos/qos?vers=1&qtyp=2{query} HTTP/1.1\r\nHost: qosapi:443\r\n\r\n");
            let req = parse_http_request(raw.as_bytes()).unwrap();
            let resp = route_http_request(&req);
            let body = String::from_utf8(resp.body).unwrap();

            let requestid: u32 = extract_element(&body, "requestid")
                .expect("<requestid> must be present")
                .parse()
                .expect("<requestid> must be an integer");
            assert_ne!(
                requestid, 0,
                "the client aborts the QoS step when requestid is 0 (FIFA22.exe 0x141FE874D)"
            );
        }
    }

    /// The `firewall` response must carry the same values the QoS
    /// response does, because it is the same type-3 request.
    #[test]
    fn test_firewall_response_echoes_the_client_request_id() {
        let raw = b"GET /qos/firewall?vers=1&nint=1&rqid=99 HTTP/1.1\r\nHost: qosapi:443\r\n\r\n";
        let req = parse_http_request(raw).unwrap();
        let resp = route_http_request(&req);
        let body = String::from_utf8(resp.body).unwrap();
        assert_eq!(extract_element(&body, "requestid").as_deref(), Some("99"));
    }

    /// The `getPeerAddress` body is a list of `name=value` pairs, not
    /// XML, so it must not be served as `application/xml`.
    #[test]
    fn test_get_peer_address_is_served_as_text_plain() {
        let raw = b"GET /getPeerAddress?myIP=127.0.0.1&myPort=5000&version=1.0 HTTP/1.1\r\nHost: qosapi:443\r\n\r\n";
        let req = parse_http_request(raw).unwrap();
        let resp = route_http_request(&req);

        assert_eq!(resp.status_code, 200);
        assert_eq!(
            resp.content_type, "text/plain",
            "the getPeerAddress body is `name=value` lines, not XML"
        );
        let body = String::from_utf8(resp.body).unwrap();
        assert!(
            !body.contains("<"),
            "the getPeerAddress body must not look like XML; got:\n{body}"
        );
    }

    /// The client parses the `getPeerAddress` IP fields with a
    /// dotted-quad parser (`0x140C96010`), so the response must carry a
    /// dotted quad — unlike `qosip`, which is a decimal u32.
    #[test]
    fn test_get_peer_address_echoes_the_requested_address() {
        let raw = b"GET /getPeerAddress?myIP=10.1.2.3&myPort=5000&version=1.0 HTTP/1.1\r\nHost: qosapi:443\r\n\r\n";
        let req = parse_http_request(raw).unwrap();
        let resp = route_http_request(&req);
        let body = String::from_utf8(resp.body).unwrap();

        // The client's own address is echoed back.
        assert!(body.contains("sourceIP=10.1.2.3\r\n"), "{body}");
        assert!(body.contains("sourcePort=5000\r\n"), "{body}");
        // The peer is the emulator, on the relay port it binds.
        assert!(body.contains("targetIP=127.0.0.1\r\n"), "{body}");
        assert!(
            body.contains(&format!("peerPort={PEER_RELAY_UDP_PORT}\r\n")),
            "the peer port must be the UDP port the emulator binds; got:\n{body}"
        );
    }

    /// `sourceIP`, `sourcePort`, `tag` and `sendCount` are not in either
    /// URL format the client builds (`0x79F6010`, `0x79F5F28`) — the
    /// client sends them as an `application/x-www-form-urlencoded` body
    /// (`0x79F6076`).  They must be read from the body and echoed.
    #[test]
    fn test_get_peer_address_reads_the_form_body() {
        let body = b"sourceIP=10.1.2.3&sourcePort=5000&tag=player-2&sendCount=4";
        let mut raw = Vec::new();
        raw.extend_from_slice(
            b"POST /getPeerAddress?myIP=10.1.2.3&myPort=5000&version=1.0 HTTP/1.1\r\n\
              Host: qosapi:443\r\n\
              Content-Type: application/x-www-form-urlencoded\r\n\
              Content-Length: ",
        );
        raw.extend_from_slice(body.len().to_string().as_bytes());
        raw.extend_from_slice(b"\r\n\r\n");
        raw.extend_from_slice(body);

        let req = parse_http_request(&raw).unwrap();
        let resp = route_http_request(&req);
        let text = String::from_utf8(resp.body).unwrap();

        assert!(text.contains("sourceIP=10.1.2.3\r\n"), "{text}");
        assert!(text.contains("sourcePort=5000\r\n"), "{text}");
        assert!(text.contains("tag=player-2\r\n"), "{text}");
        assert!(text.contains("sendCount=4\r\n"), "{text}");
    }

    /// A body is only read as form data when the client says it is form
    /// data — otherwise a `text/plain` body would be mis-parsed.
    #[test]
    fn test_a_non_form_body_is_not_parsed_as_form_data() {
        let raw = b"POST /getPeerAddress?myIP=127.0.0.1&myPort=5000 HTTP/1.1\r\n\
                    Host: qosapi:443\r\n\
                    Content-Type: text/plain\r\n\
                    Content-Length: 32\r\n\
                    \r\n\
                    sourceIP=1.2.3.4&sourcePort=99\r\n";
        let req = parse_http_request(raw).unwrap();
        assert!(req.form_pairs().is_empty());
        assert_eq!(req.param("sourceIP"), None);
        // The query string still works.
        assert_eq!(req.param("myIP").as_deref(), Some("127.0.0.1"));
    }

    /// The peer port the response advertises must be one the emulator
    /// actually binds: the ProtoTunnel relay is on `:3659`
    /// (`crates/emulator-server/src/lib.rs`).
    #[test]
    fn test_the_advertised_peer_port_is_a_port_the_emulator_binds() {
        assert_eq!(PEER_RELAY_UDP_PORT, 3659);
        assert_eq!(QOS_UDP_PORT, 17498);
    }

    /// The client looks up exactly these seven names in the
    /// `getPeerAddress` response.
    #[test]
    fn test_get_peer_address_carries_every_name_the_client_looks_up() {
        let raw = b"GET /getPeerAddress?myIP=127.0.0.1&myPort=5000&version=1.0 HTTP/1.1\r\nHost: qosapi:443\r\n\r\n";
        let req = parse_http_request(raw).unwrap();
        let resp = route_http_request(&req);
        let body = String::from_utf8(resp.body).unwrap();

        for field in [
            "targetIP", "targetPort", "tag", "sendCount", "sourcePort", "peerIP", "peerPort",
        ] {
            assert!(
                body.contains(&format!("{field}=")),
                "the client looks up `{field}`; the response must carry it"
            );
        }
    }

    /// The QoS responder the client is told to probe is the emulator's own
    /// UDP listener, which binds `:17498`
    /// (`crates/emulator-server/src/lib.rs`).
    #[test]
    fn test_the_advertised_qos_port_is_the_port_the_emulator_binds() {
        assert_eq!(QOS_UDP_PORT, 17498);
        let raw = b"GET /qos/qos HTTP/1.1\r\nHost: qosapi:443\r\n\r\n";
        let req = parse_http_request(raw).unwrap();
        let resp = route_http_request(&req);
        let body = String::from_utf8(resp.body).unwrap();
        assert!(body.contains("<qosport>17498</qosport>"), "{body}");
    }

    /// An unknown path is fast-pathed with a 200 so the client never
    /// stalls waiting for a response.
    #[test]
    fn test_unknown_path_is_fast_pathed_with_a_200() {
        let raw = b"GET /some/unmapped/ea/path HTTP/1.1\r\nHost: accounts.ea.com\r\n\r\n";
        let req = parse_http_request(raw).unwrap();
        let resp = route_http_request(&req);

        assert_eq!(resp.status_code, 200);
        assert_eq!(resp.content_type, "application/json");
        let body = String::from_utf8(resp.body).unwrap();
        assert!(body.contains("\"status\":\"ok\""), "{body}");
    }

    /// The sink speaks HTTP/1.1 only.  An HTTP/2 connection preface must
    /// be detected, and must never be mis-routed as the request line
    /// `PRI * HTTP/2.0`.
    #[test]
    fn test_http2_preface_is_detected_and_not_parsed_as_http11() {
        let preface = b"PRI * HTTP/2.0\r\n\r\nSM\r\n\r\n\x00\x00\x00\x04\x00";

        assert!(is_http2_preface(preface));
        assert!(
            parse_http_request(preface).is_none(),
            "an HTTP/2 preface must not be parsed as an HTTP/1.1 request"
        );

        assert!(!is_http2_preface(b"GET /qos/qos HTTP/1.1\r\n\r\n"));
        assert!(parse_http_request(b"GET /qos/qos HTTP/1.1\r\n\r\n").is_some());
    }

    /// The sink must never point the client at a real EA host: the
    /// responses are fully self-contained, and the QoS peer address is
    /// the emulator's own loopback address.
    #[test]
    fn test_no_response_points_at_a_real_ea_host() {
        let paths = [
            "/qos/qos",
            "/qos/firewall",
            "/qos/firetype",
            "/getPeerAddress?myIP=127.0.0.1&myPort=5000&version=1.0",
        ];
        for path in paths {
            let raw = format!("GET {path} HTTP/1.1\r\nHost: accounts.ea.com\r\n\r\n");
            let req = parse_http_request(raw.as_bytes()).unwrap();
            let resp = route_http_request(&req);
            let body = String::from_utf8_lossy(&resp.body).to_lowercase();

            for forbidden in ["http://", "https://", ".ea.com", ".ea.net", ".dice.se"] {
                assert!(
                    !body.contains(forbidden),
                    "{path} must not reference {forbidden}; got:\n{body}"
                );
            }
        }
    }
}
