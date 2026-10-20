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
        // Synthetic EA Nucleus / Gateway / Identity Auth Token endpoint
        p if p.contains("token") || p.contains("auth") || p.contains("login") || p.contains("pid") => {
            tracing::info!("[Nucleus Sink] Serving synthetic Nucleus Auth token for {}", path);
            let json = r#"{"access_token":"f22_mock_nucleus_token_deadpool","token_type":"Bearer","expires_in":31536000,"id_token":"mock_id_token","pid_id":76561198956360709,"persona_id":76561198956360709,"status":"SUCCESS"}"#;
            HttpResponse::ok_json(json)
        }
        // DirtySDK HTTP QoS metadata discovery probes
        "/qos/qos" | "/qos/fire" => {
            tracing::info!("[QoS HTTP] Serving QoS XML configuration metadata");
            let xml = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<qos>\n    <numprobes>10</numprobes>\n    <qoshost>127.0.0.1</qoshost>\n    <qosport>17498</qosport>\n    <probesize>1200</probesize>\n    <serviceid>1</serviceid>\n</qos>";
            HttpResponse::ok_xml(xml)
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
        assert!(body.contains("<qoshost>127.0.0.1</qoshost>"));
        assert!(body.contains("<qosport>17498</qosport>"));
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
