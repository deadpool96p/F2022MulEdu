//! HTTP/HTTPS Telemetry Sink, Squad Distribution, and Dual-Port Redirector Server (:443).

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio_rustls::TlsAcceptor;
use services_http_content::{
    is_http2_preface, parse_http_request, route_http_request, HttpResponse,
};
use services_redirector::{handle_redirector_packet, RedirectorConfig};
use blaze_core::frame::BlazeHeader;
use blaze_core::BlazePacket;
use blaze_tdf::deserialize_tdf;
use crate::listener::accept_telemetry_stream;

/// Runs the HTTP/HTTPS Telemetry and dual-port redirector server loop (:443).
pub async fn run_telemetry_server(listener: TcpListener, tls_acceptor: TlsAcceptor) {
    let redir_cfg = RedirectorConfig {
        host: "127.0.0.1".to_string(),
        port: 42128,
        secure: false,
        ..Default::default()
    };

    while let Ok((tcp_stream, peer_addr)) = listener.accept().await {
        let acceptor = tls_acceptor.clone();
        let cfg = redir_cfg.clone();

        tokio::spawn(async move {
            match accept_telemetry_stream(tcp_stream, &acceptor).await {
                Ok(mut stream) => {
                    let mut buffer = vec![0u8; 8192];
                    if let Ok(n) = stream.read(&mut buffer).await {
                        if n >= BlazeHeader::HEADER_SIZE && buffer[0] == 0x00 && buffer[1] < 0x80 {
                            // Possible Blaze Packet (Redirector request on port 443)
                            let header_arr: &[u8; BlazeHeader::HEADER_SIZE] = match (&buffer[..BlazeHeader::HEADER_SIZE]).try_into() {
                                Ok(arr) => arr,
                                Err(_) => &[0u8; BlazeHeader::HEADER_SIZE],
                            };
                            if let Ok(header) = BlazeHeader::from_bytes(header_arr) {
                                if header.component == 0x0005 {
                                    tracing::info!("[Telemetry:443] Handled Blaze Redirector request from {}", peer_addr);
                                    let payload_end = BlazeHeader::HEADER_SIZE + (header.length as usize);
                                    if n >= payload_end {
                                        if let Ok(payload) = deserialize_tdf(&buffer[BlazeHeader::HEADER_SIZE..payload_end]) {
                                            let req_packet = BlazePacket { header, payload };
                                            if let Ok(resp_packet) = handle_redirector_packet(&req_packet, &cfg) {
                                                if let Ok(encoded) = resp_packet.encode() {
                                                    let _ = stream.write_all(&encoded).await;
                                                    let _ = stream.flush().await;
                                                    return;
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }

                        // Standard HTTP Telemetry / River request
                        let resp = match parse_http_request(&buffer[..n]) {
                            Some(req) => {
                                tracing::debug!("[Telemetry:443] HTTP {} {} from {}", req.method, req.path, peer_addr);
                                route_http_request(&req)
                            }
                            None => {
                                // The sink speaks HTTP/1.1 only.  An HTTP/2
                                // connection preface, or a request that
                                // arrived split across TCP segments, must be
                                // visible in the log rather than silently
                                // fast-pathed.
                                let first_line = String::from_utf8_lossy(&buffer[..n])
                                    .lines()
                                    .next()
                                    .unwrap_or("<empty>")
                                    .to_string();
                                if is_http2_preface(&buffer[..n]) {
                                    tracing::warn!(
                                        "[Telemetry:443] HTTP/2 connection preface from {} ({:?}) — the sink is HTTP/1.1 only; \
                                         the TLS acceptor must not select the `h2` ALPN protocol",
                                        peer_addr,
                                        first_line
                                    );
                                } else {
                                    tracing::warn!(
                                        "[Telemetry:443] Unparsed HTTP request from {}: {:?}",
                                        peer_addr,
                                        first_line
                                    );
                                }
                                HttpResponse::ok_json(r#"{"status":"ok"}"#)
                            }
                        };
                        let _ = stream.write_all(&resp.to_bytes()).await;
                        let _ = stream.flush().await;
                    }
                }
                Err(e) => {
                    tracing::debug!("[Telemetry Sink] Stream read error from {}: {:?}", peer_addr, e);
                }
            }
        });
    }
}
