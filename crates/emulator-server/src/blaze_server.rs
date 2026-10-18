//! Core Blaze Protocol TCP Server (:42128).

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio_rustls::TlsAcceptor;
use blaze_core::frame::BlazeHeader;
use blaze_core::BlazePacket;
use blaze_tdf::deserialize_tdf;
use services_blaze::BlazeRouter;
use crate::listener::{accept_dual_stream, DualStream};

/// Runs the Core Blaze server listener loop.
pub async fn run_blaze_server(
    listener: TcpListener,
    tls_acceptor: TlsAcceptor,
    qos_host: String,
    qos_port: u16,
) {
    while let Ok((tcp_stream, peer_addr)) = listener.accept().await {
        let acceptor = tls_acceptor.clone();
        let q_host = qos_host.clone();

        tokio::spawn(async move {
            tracing::info!("[Blaze Server] New client connection from {}", peer_addr);
            match accept_dual_stream(tcp_stream, &acceptor).await {
                Ok(mut stream) => {
                    let mut router = BlazeRouter::new(q_host, qos_port);
                    if let Err(e) = handle_blaze_session(&mut stream, &mut router).await {
                        tracing::debug!("[Blaze Server] Session closed for {}: {:?}", peer_addr, e);
                    }
                }
                Err(e) => {
                    tracing::warn!("[Blaze Server] Stream accept error from {}: {:?}", peer_addr, e);
                }
            }
        });
    }
}

async fn handle_blaze_session(
    stream: &mut DualStream,
    router: &mut BlazeRouter,
) -> std::io::Result<()> {
    loop {
        // Read 12-byte Blaze header
        let mut header_buf = [0u8; BlazeHeader::HEADER_SIZE];
        if let Err(e) = stream.read_exact(&mut header_buf).await {
            if e.kind() == std::io::ErrorKind::UnexpectedEof {
                break; // Clean disconnection
            }
            return Err(e);
        }
        let header = BlazeHeader::from_bytes(&header_buf)?;

        // Read TDF payload
        let mut payload_buf = vec![0u8; header.length as usize];
        stream.read_exact(&mut payload_buf).await?;
        let payload = deserialize_tdf(&payload_buf)?;

        let req_packet = BlazePacket { header, payload };
        let (resp_packet, notif_opt) = router.handle_packet(&req_packet)?;

        // Send RPC response
        let resp_bytes = resp_packet.encode()?;
        stream.write_all(&resp_bytes).await?;

        // Send async notification if triggered (e.g. UserSessionExtendedDataUpdate)
        if let Some(notif) = notif_opt {
            let notif_bytes = notif.encode()?;
            stream.write_all(&notif_bytes).await?;
        }

        stream.flush().await?;
    }

    Ok(())
}
