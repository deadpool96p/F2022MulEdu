//! Redirector Service TCP Server (:42127).

use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio_rustls::TlsAcceptor;
use blaze_core::frame::BlazeHeader;
use blaze_core::framing::{detect_framing, Framing};
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
            let ip = local_addr.ip();
            match ip {
                std::net::IpAddr::V4(ipv4) => {
                    if !ipv4.is_unspecified() {
                        cfg.host = ipv4.to_string();
                    }
                }
                std::net::IpAddr::V6(ipv6) => {
                    if let Some(ipv4) = ipv6.to_ipv4_mapped() {
                        cfg.host = ipv4.to_string();
                    } else {
                        // Frostbite DirtySDK cannot parse raw IPv6 strings ("::1") in Blaze ADDR union.
                        // Always fallback to standard IPv4 loopback "127.0.0.1" for local loopback connections.
                        cfg.host = "127.0.0.1".to_string();
                    }
                }
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
        // Read initial chunk into buffer
        let mut buffer = Vec::with_capacity(1024);
        buffer.extend_from_slice(&prefix);

        // Read at least 12 more bytes to determine 12-byte vs 16-byte header
        let mut rest_hdr = [0u8; 12];
        stream.read_exact(&mut rest_hdr).await?;
        buffer.extend_from_slice(&rest_hdr);

        // Decide which header generation the client is speaking. A client's
        // first frame is always a request, and for a request the two
        // generations are mutually exclusive. See `blaze_core::framing`.
        let framing = detect_framing(&buffer).ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "could not determine the Blaze framing generation",
            )
        })?;

        let (header, payload_len, header_size, metadata_len) = match framing {
            Framing::Fire2 => {
                // The Fire2 header is 16 bytes; read the rest of it.
                let mut extra = [0u8; 4];
                stream.read_exact(&mut extra).await?;
                buffer.extend_from_slice(&extra);

                let header_slice: [u8; 16] = buffer[0..16].try_into().unwrap();
                let header = BlazeHeader::from_bytes_16(&header_slice)?;
                let metadata_len = header.metadata_size as usize;
                (header, header.length as usize, 16usize, metadata_len)
            }
            Framing::Legacy => {
                let header_slice: [u8; 12] = buffer[0..12].try_into().unwrap();
                let header = BlazeHeader::from_bytes(&header_slice)?;
                (header, header.length as usize, 12usize, 0usize)
            }
        };

        tracing::info!(
            "[Redirector] Parsed header ({:?}): Comp=0x{:04X}, Cmd=0x{:04X}, Seq={}, Len={}, MsgType={:?}",
            framing,
            header.component,
            header.command,
            header.seq_no,
            header.length,
            header.msg_type
        );

        // Read the metadata section (Fire2 only) and then the payload.
        let mut payload_buf = Vec::with_capacity(payload_len);
        if metadata_len > 0 {
            let mut meta = vec![0u8; metadata_len];
            stream.read_exact(&mut meta).await?;
        }

        let already_read = buffer.len() - header_size;
        if already_read > 0 {
            let take = already_read.min(payload_len);
            payload_buf.extend_from_slice(&buffer[header_size..header_size + take]);
        }
        while payload_buf.len() < payload_len {
            let mut chunk = vec![0u8; payload_len - payload_buf.len()];
            stream.read_exact(&mut chunk).await?;
            payload_buf.extend_from_slice(&chunk);
        }

        let payload = if payload_buf.is_empty() {
            blaze_tdf::TdfStruct::new()
        } else {
            deserialize_tdf(&payload_buf).unwrap_or_else(|_| blaze_tdf::TdfStruct::new())
        };

        let req_packet = BlazePacket { header, payload };
        let resp_packet = handle_redirector_packet(&req_packet, config)?;

        let encoded_resp = match framing {
            Framing::Fire2 => resp_packet.encode_16()?,
            Framing::Legacy => resp_packet.encode()?,
        };

        tracing::info!(
            "[Redirector] Sending ServerInstance response to client ({:?}, {} bytes):\n{}",
            framing,
            encoded_resp.len(),
            blaze_core::hex_dump(&encoded_resp)
        );

        stream.write_all(&encoded_resp).await?;
        stream.flush().await?;
        tracing::info!("[Redirector] ServerInstance response sent and flushed successfully.");

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

    // When config.secure is false, downgrade to Plain TCP (0) to allow clean certificate-free socket flow
    let secure_val = if config.secure { 1 } else { 0 };

    let response_body = if is_json {
        format!(
            r#"{{"address":{{"ipAddress":{{"hostname":"{}","ip":2130706433,"port":{}}}}},"secure":{},"msiz":131072}}"#,
            config.host, config.port, secure_val
        )
    } else {
        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<serverinstanceinfo>
  <address member="0">
    <valu>
      <hostname>{}</hostname>
      <ip>2130706433</ip>
      <port>{}</port>
    </valu>
    <ipaddress>
      <hostname>{}</hostname>
      <ip>2130706433</ip>
      <port>{}</port>
    </ipaddress>
  </address>
  <secure>{}</secure>
  <msiz>131072</msiz>
  <trialService>0</trialService>
  <defaultDnsAddress>0</defaultDnsAddress>
</serverinstanceinfo>"#,
            config.host, config.port, config.host, config.port, secure_val
        )
    };

    let content_type = if is_json {
        "application/json; charset=utf-8"
    } else {
        "application/xml; charset=utf-8"
    };

    let http_response = format!(
        "HTTP/1.1 200 OK\r\n\
Content-Type: {}\r\n\
Content-Length: {}\r\n\
Connection: close\r\n\
X-BLAZE-ERRORCODE: 0\r\n\
X-BLAZE-COMPONENT: redirector\r\n\
X-BLAZE-COMMAND: getServerInstance\r\n\
X-BLAZE-SEQNO: 0\r\n\r\n{}",
        content_type,
        response_body.len(),
        response_body
    );

    tracing::info!(
        "[Redirector HTTP] Responded 200 OK: redirecting client to {}:{} (Format={}, Secure={})",
        config.host,
        config.port,
        if is_json { "JSON" } else { "XML" },
        secure_val
    );

    stream.write_all(http_response.as_bytes()).await?;
    stream.flush().await?;

    Ok(())
}
