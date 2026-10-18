//! HTTP/HTTPS Telemetry Sink and Squad Distribution Server.

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio_rustls::TlsAcceptor;
use services_http_content::{parse_http_request, route_http_request, HttpResponse};
use crate::listener::accept_dual_stream;

/// Runs the HTTP/HTTPS Telemetry server listener loop.
pub async fn run_telemetry_server(listener: TcpListener, tls_acceptor: TlsAcceptor) {
    while let Ok((tcp_stream, peer_addr)) = listener.accept().await {
        let acceptor = tls_acceptor.clone();

        tokio::spawn(async move {
            match accept_dual_stream(tcp_stream, &acceptor).await {
                Ok(mut stream) => {
                    let mut buffer = vec![0u8; 8192];
                    if let Ok(n) = stream.read(&mut buffer).await {
                        if n > 0 {
                            let resp = match parse_http_request(&buffer[..n]) {
                                Some(req) => route_http_request(&req),
                                None => HttpResponse::ok_json(r#"{"status":"ok"}"#),
                            };
                            let _ = stream.write_all(&resp.to_bytes()).await;
                            let _ = stream.flush().await;
                        }
                    }
                }
                Err(e) => {
                    tracing::debug!("[Telemetry Sink] Stream read error from {}: {:?}", peer_addr, e);
                }
            }
        });
    }
}
