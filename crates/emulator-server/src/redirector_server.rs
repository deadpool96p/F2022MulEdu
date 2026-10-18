//! Redirector Service TCP Server (:42127).

use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio_rustls::TlsAcceptor;
use blaze_core::frame::BlazeHeader;
use blaze_core::BlazePacket;
use blaze_tdf::deserialize_tdf;
use services_redirector::{handle_redirector_packet, RedirectorConfig};
use crate::listener::{accept_dual_stream, DualStream};

/// Runs the Redirector service listener loop.
pub async fn run_redirector_server(
    listener: TcpListener,
    tls_acceptor: TlsAcceptor,
    config: RedirectorConfig,
) {
    let config = Arc::new(config);

    while let Ok((tcp_stream, peer_addr)) = listener.accept().await {
        let acceptor = tls_acceptor.clone();
        let mut cfg = (*config).clone();
        if let Ok(local_addr) = tcp_stream.local_addr() {
            let ip_str = local_addr.ip().to_string();
            if ip_str != "0.0.0.0" {
                cfg.host = ip_str;
            }
        }
        let cfg = Arc::new(cfg);

        tokio::spawn(async move {
            tracing::info!("[Redirector] Connection from {} (redirecting to {})", peer_addr, cfg.host);
            match accept_dual_stream(tcp_stream, &acceptor).await {
                Ok(mut stream) => {
                    if let Err(e) = handle_redirector_connection(&mut stream, &cfg).await {
                        tracing::debug!("[Redirector] Session ended with {}: {:?}", peer_addr, e);
                    }
                }
                Err(e) => {
                    tracing::warn!("[Redirector] Failed to accept stream from {}: {:?}", peer_addr, e);
                }
            }
        });
    }
}

async fn handle_redirector_connection(
    stream: &mut DualStream,
    config: &RedirectorConfig,
) -> std::io::Result<()> {
    // Read the first 4 bytes to distinguish HTTP (POST/GET) from binary Blaze header
    let mut prefix = [0u8; 4];
    stream.read_exact(&mut prefix).await?;

    if &prefix == b"POST" || &prefix == b"GET " {
        handle_http_redirector(stream, &prefix, config).await
    } else {
        // Read remaining 8 bytes of 12-byte Blaze header
        let mut header_buf = [0u8; BlazeHeader::HEADER_SIZE];
        header_buf[..4].copy_from_slice(&prefix);
        stream.read_exact(&mut header_buf[4..]).await?;
        let header = BlazeHeader::from_bytes(&header_buf)?;

        // Read TDF payload
        let mut payload_buf = vec![0u8; header.length as usize];
        stream.read_exact(&mut payload_buf).await?;
        let payload = deserialize_tdf(&payload_buf)?;

        let req_packet = BlazePacket { header, payload };
        let resp_packet = handle_redirector_packet(&req_packet, config)?;

        let encoded_resp = resp_packet.encode()?;
        stream.write_all(&encoded_resp).await?;
        stream.flush().await?;

        Ok(())
    }
}

async fn handle_http_redirector(
    stream: &mut DualStream,
    prefix: &[u8; 4],
    config: &RedirectorConfig,
) -> std::io::Result<()> {
    let mut req_bytes = Vec::with_capacity(4096);
    req_bytes.extend_from_slice(prefix);

    let mut buf = [0u8; 1024];
    let mut body_start = 0;
    let mut content_length = 0usize;

    // Read until header boundary "\r\n\r\n" or 16KB limit
    loop {
        if let Some(pos) = req_bytes.windows(4).position(|w| w == b"\r\n\r\n") {
            body_start = pos + 4;
            let headers_str = String::from_utf8_lossy(&req_bytes[..pos]);
            for line in headers_str.lines() {
                if line.to_ascii_lowercase().starts_with("content-length:") {
                    if let Some(val) = line.split(':').nth(1) {
                        content_length = val.trim().parse::<usize>().unwrap_or(0);
                    }
                }
            }
            break;
        }

        let n = stream.read(&mut buf).await?;
        if n == 0 {
            break;
        }
        req_bytes.extend_from_slice(&buf[..n]);
        if req_bytes.len() > 16384 {
            break;
        }
    }

    // Read remaining body bytes if Content-Length was specified
    let total_needed = body_start + content_length;
    while req_bytes.len() < total_needed {
        let n = stream.read(&mut buf).await?;
        if n == 0 {
            break;
        }
        req_bytes.extend_from_slice(&buf[..n]);
    }

    let req_str = String::from_utf8_lossy(&req_bytes);
    tracing::info!("[Redirector HTTP] Request received:\n{}", req_str);

    let is_json = req_str.to_ascii_lowercase().contains("application/json");

    let response_body = if is_json {
        format!(
            r#"{{"address":{{"ipAddress":{{"hostname":"{}","ip":2130706433,"port":{}}}}},"secure":0,"msiz":131072}}"#,
            config.host, config.port
        )
    } else {
        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<serverinstanceinfo>
  <address>
    <ipaddress>
      <hostname>{}</hostname>
      <ip>2130706433</ip>
      <port>{}</port>
    </ipaddress>
  </address>
  <secure>0</secure>
  <msiz>131072</msiz>
  <trialService>0</trialService>
  <defaultDnsAddress>0</defaultDnsAddress>
</serverinstanceinfo>"#,
            config.host, config.port
        )
    };

    let content_type = if is_json {
        "application/json; charset=utf-8"
    } else {
        "application/xml; charset=utf-8"
    };

    let http_response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        content_type,
        response_body.len(),
        response_body
    );

    tracing::info!(
        "[Redirector HTTP] Responded 200 OK: redirecting client to {}:{} (Format={})",
        config.host,
        config.port,
        if is_json { "JSON" } else { "XML" }
    );

    stream.write_all(http_response.as_bytes()).await?;
    stream.flush().await?;

    Ok(())
}
