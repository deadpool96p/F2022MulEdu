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
    // Read 12-byte Blaze header
    let mut header_buf = [0u8; BlazeHeader::HEADER_SIZE];
    stream.read_exact(&mut header_buf).await?;
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
