//! # services-http-content
//!
//! Fast mock HTTP/HTTPS telemetry sink (`river.data.ea.com`, `pin-river.data.ea.com`)
//! and squad content distribution endpoint.

use std::collections::HashMap;

pub mod squads;
pub use squads::*;

/// Parsed HTTP request structure.
#[derive(Debug, Clone)]
pub struct HttpRequest {
    pub method: String,
    pub path: String,
    pub headers: HashMap<String, String>,
    pub body: Vec<u8>,
}

/// HTTP response structure.
#[derive(Debug, Clone)]
pub struct HttpResponse {
    pub status_code: u16,
    pub reason_phrase: &'static str,
    pub content_type: &'static str,
    pub body: Vec<u8>,
}

impl HttpResponse {
    pub fn ok_json(body: &str) -> Self {
        Self {
            status_code: 200,
            reason_phrase: "OK",
            content_type: "application/json",
            body: body.as_bytes().to_vec(),
        }
    }

    pub fn ok_xml(body: &str) -> Self {
        Self {
            status_code: 200,
            reason_phrase: "OK",
            content_type: "application/xml",
            body: body.as_bytes().to_vec(),
        }
    }

    pub fn ok_binary(body: Vec<u8>) -> Self {
        Self {
            status_code: 200,
            reason_phrase: "OK",
            content_type: "application/octet-stream",
            body,
        }
    }

    pub fn not_found() -> Self {
        Self {
            status_code: 404,
            reason_phrase: "Not Found",
            content_type: "text/plain",
            body: b"404 Not Found".to_vec(),
        }
    }

    pub fn bad_request(body: &str) -> Self {
        Self {
            status_code: 400,
            reason_phrase: "Bad Request",
            content_type: "application/json",
            body: body.as_bytes().to_vec(),
        }
    }

    /// Serializes the response into raw HTTP wire bytes.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = Vec::new();
        let status_line = format!(
            "HTTP/1.1 {} {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\nAccess-Control-Allow-Origin: *\r\n\r\n",
            self.status_code, self.reason_phrase, self.content_type, self.body.len()
        );
        out.extend_from_slice(status_line.as_bytes());
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
        _ => {
            // Fallback 200 OK for any unmapped EA telemetry/ad path to prevent game freeze
            tracing::info!("[HTTP Sink] Fast-pathing unmapped endpoint: {}", path);
            HttpResponse::ok_json(r#"{"status":"ok"}"#)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
