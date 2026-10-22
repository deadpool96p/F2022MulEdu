//! HTTP/2 server for the QoS / ProtoPing API on `:443`.
//!
//! FIFA 22 has two different HTTP stacks in `FIFA22.exe`:
//!
//! * the DirtySDK **ProtoHttp** stack, which is HTTP/1.1 over TLS and
//!   offers **no** ALPN protocol at all (it is the one that posts to
//!   `rl.data.ea.com` and `freeform-river.data.ea.com`), and
//! * an **HTTP/2** stack, which negotiates ALPN `h2` and is the one that
//!   dials `26.178.140.32:443` (the QoS endpoint).
//!
//! The `:443` sink used to answer HTTP/1.1 only, so a request that arrived
//! over the `h2` ALPN was answered with a `{"status":"ok"}` JSON blob -- or
//! not answered at all. This module is the HTTP/2 half of the sink.
//!
//! The QoS request/response *bodies* are not defined here: they live in
//! `services-http-content` and are shared with the HTTP/1.1 path, so the two
//! transports cannot drift apart. Only the framing differs.
//!
//! `★ Insight ─────────────────────────────────────`
//! `h2` 0.4 does **not** use `http_body::Body`. Its server side is
//! frame-oriented: `send_response` takes a `Response<()>` (headers only)
//! and returns a `SendStream<Bytes>`; the body then goes out with
//! `send_data(bytes, end_of_stream)`. That is why the `h2` crate has no
//! `http-body` dependency at all -- it is a *frame* library, not a body
//! library. Sizing the response to a single DATA frame keeps the QoS
//! replies inside the initial 64 KiB flow-control window, so the client
//! never has to wait for a WINDOW_UPDATE.
//! `─────────────────────────────────────────────────`

use std::collections::HashMap;

use bytes::Bytes;
use http::Response;
use services_http_content::{route_http_request, HttpRequest};
use tokio::io::{AsyncRead, AsyncWrite};

/// The ALPN protocol name the client's HTTP/2 stack offers.
pub const ALPN_H2: &[u8] = b"h2";

/// The ALPN protocol name a plain HTTP/1.1 client would use.
pub const ALPN_HTTP11: &[u8] = b"http/1.1";

/// True when `alpn` is the HTTP/2 protocol name.
pub fn is_http2_alpn(alpn: Option<&[u8]>) -> bool {
    alpn == Some(ALPN_H2)
}

/// Serves HTTP/2 requests on `stream` until the peer goes away.
///
/// The `h2` crate's server handshake consumes the client's connection
/// preface and the `SETTINGS` frame; from there every request is handed to
/// [`route_http_request`], exactly like an HTTP/1.1 request.
pub async fn serve_http2<S>(stream: S, peer: &str)
where
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    let mut conn = match h2::server::handshake(stream).await {
        Ok(conn) => conn,
        Err(e) => {
            tracing::warn!("[HTTP/2] Connection preface failed with {}: {:?}", peer, e);
            return;
        }
    };
    tracing::info!("[HTTP/2] Connection established with {}", peer);

    while let Some(request) = conn.accept().await {
        let (request, mut respond) = match request {
            Ok(pair) => pair,
            Err(e) => {
                tracing::debug!("[HTTP/2] Connection error with {}: {:?}", peer, e);
                break;
            }
        };

        tokio::spawn(async move {
            let (parts, mut body) = request.into_parts();

            // Collect the request body. `RecvStream::data` yields the DATA
            // frames and then `None` at end-of-stream.
            let mut payload = Vec::new();
            while let Some(chunk) = body.data().await {
                match chunk {
                    Ok(data) => {
                        payload.extend_from_slice(&data);
                        // Give the flow-control window back, or the client
                        // stalls once 64 KiB has been sent.
                        let len = data.len();
                        if let Err(e) = body.flow_control().release_capacity(len) {
                            tracing::debug!("[HTTP/2] Could not release capacity: {:?}", e);
                        }
                    }
                    Err(e) => {
                        tracing::warn!("[HTTP/2] Error reading request body: {:?}", e);
                        break;
                    }
                }
            }

            let mut headers = HashMap::new();
            for (name, value) in parts.headers.iter() {
                if let Ok(value) = value.to_str() {
                    headers.insert(name.as_str().to_ascii_lowercase(), value.to_string());
                }
            }

            let req = HttpRequest {
                method: parts.method.to_string(),
                // The request target carries the query string, which the QoS
                // handlers read (`rqid`, `rqsc`, `qtyp`, `vers`, ...).
                path: parts.uri.to_string(),
                headers,
                body: payload,
                alpn: Some(String::from_utf8_lossy(ALPN_H2).to_string()),
            };

            let resp = route_http_request(&req);

            let mut builder = Response::builder().status(resp.status_code);
            for (name, value) in &resp.headers {
                // `Connection` and `Transfer-Encoding` are forbidden in
                // HTTP/2 (RFC 9113 §8.2.2) -- sending them is a protocol
                // error, not a warning.
                if name.eq_ignore_ascii_case("connection")
                    || name.eq_ignore_ascii_case("transfer-encoding")
                    || name.eq_ignore_ascii_case("keep-alive")
                    || name.eq_ignore_ascii_case("upgrade")
                {
                    continue;
                }
                builder = builder.header(name.as_str(), value.as_str());
            }
            builder = builder.header("content-type", resp.content_type);

            let response = match builder.body(()) {
                Ok(response) => response,
                Err(e) => {
                    tracing::warn!("[HTTP/2] Could not build response: {:?}", e);
                    return;
                }
            };

            // `send_response` carries the headers only; the body follows as
            // DATA frames.
            let mut send = match respond.send_response(response, false) {
                Ok(send) => send,
                Err(e) => {
                    tracing::warn!("[HTTP/2] Failed to send response headers: {:?}", e);
                    return;
                }
            };

            if let Err(e) = send.send_data(Bytes::from(resp.body), true) {
                tracing::warn!("[HTTP/2] Failed to send response body: {:?}", e);
            }
        });
    }

    tracing::info!("[HTTP/2] Connection closed with {}", peer);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_h2_alpn_name_is_recognised() {
        assert!(is_http2_alpn(Some(ALPN_H2)));
        assert!(!is_http2_alpn(Some(ALPN_HTTP11)));
        assert!(!is_http2_alpn(None));
    }

    /// The QoS replies are small enough to fit in the initial flow-control
    /// window, so they go out as a single DATA frame.
    #[test]
    fn a_qos_reply_fits_in_one_data_frame() {
        let req = HttpRequest {
            method: "GET".to_string(),
            path: "/qos/qos?vers=1&qtyp=2".to_string(),
            headers: HashMap::new(),
            body: Vec::new(),
            alpn: Some("h2".to_string()),
        };
        let resp = route_http_request(&req);
        assert_eq!(resp.status_code, 200);
        assert!(
            resp.body.len() < 64 * 1024,
            "the reply must fit in the initial flow-control window"
        );
    }
}
