//! Publisher for the emulator's certificate revocation list.
//!
//! The leaf certificate the emulator serves carries a
//! `crlDistributionPoints` extension pointing at
//! [`crate::tls::CRL_DISTRIBUTION_POINT_URL`]. Windows' revocation engine
//! (crypt32) fetches that URL whenever it validates the chain, and it
//! refuses the handshake with `CRYPT_E_NO_REVOCATION_CHECK` (0x80092012)
//! when it cannot obtain the list.
//!
//! The list is served over **plain HTTP** on purpose. A `https://`
//! distribution point would make the CRL fetch itself a TLS handshake that
//! needs a revocation check -- the very check that is failing -- and the
//! fetch would never complete.

use std::io;
use std::sync::Arc;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

/// The `Content-Type` a CRL is served with (RFC 5280, section 6.1).
const CRL_CONTENT_TYPE: &str = "application/pkix-crl";

/// Runs the CRL publisher until the listener fails.
pub async fn run_crl_server(listener: TcpListener, crl: Arc<Vec<u8>>) {
    tracing::info!(
        "[CRL] Publishing a {} byte revocation list on port {}",
        crl.len(),
        listener.local_addr().map(|a| a.port()).unwrap_or(0)
    );

    loop {
        let (mut stream, peer) = match listener.accept().await {
            Ok(pair) => pair,
            Err(e) => {
                tracing::warn!("[CRL] Accept failed: {:?}", e);
                continue;
            }
        };

        let crl = crl.clone();
        tokio::spawn(async move {
            if let Err(e) = serve_crl(&mut stream, &crl).await {
                tracing::debug!("[CRL] Session from {} ended: {:?}", peer, e);
            }
        });
    }
}

/// Answers a single CRL request.
async fn serve_crl(stream: &mut TcpStream, crl: &[u8]) -> io::Result<()> {
    let head = read_request_head(stream).await?;
    let request_line = head.lines().next().unwrap_or("");
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or("");
    let path = parts.next().unwrap_or("/");

    tracing::info!("[CRL] {} {}", method, path);

    let (status, content_type, body): (&str, &str, &[u8]) = match method {
        "GET" | "HEAD" => ("200 OK", CRL_CONTENT_TYPE, crl),
        _ => ("405 Method Not Allowed", "text/plain", b"method not allowed"),
    };

    let mut response = format!(
        "HTTP/1.1 {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n",
        status,
        content_type,
        body.len()
    )
    .into_bytes();

    // A HEAD response carries the headers of the GET response but no body.
    if method != "HEAD" {
        response.extend_from_slice(body);
    }

    stream.write_all(&response).await?;
    stream.flush().await?;
    Ok(())
}

/// Reads up to the end of the request head (or the whole request, if the
/// client half-closes).
async fn read_request_head(stream: &mut TcpStream) -> io::Result<String> {
    let mut buf = Vec::with_capacity(1024);
    let mut chunk = [0u8; 1024];

    loop {
        let n = stream.read(&mut chunk).await?;
        if n == 0 {
            break;
        }
        buf.extend_from_slice(&chunk[..n]);

        if buf.windows(4).any(|w| w == b"\r\n\r\n") {
            break;
        }
        if buf.len() > 8192 {
            break;
        }
    }

    Ok(String::from_utf8_lossy(&buf).into_owned())
}
