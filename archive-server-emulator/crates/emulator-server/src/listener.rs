//! Dual-Mode TLS / Plain-TCP / BlazeSSL Stream Listener.

use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::net::TcpStream;
use tokio_rustls::server::TlsStream;
use tokio_rustls::TlsAcceptor;
use blaze_ssl_async::stream::BlazeStream;

/// A unified stream that is either a raw plain TCP socket, standard TLS, or EA BlazeSSL session.
pub enum DualStream {
    Plain(TcpStream),
    Tls(TlsStream<TcpStream>),
    Blaze(BlazeStream),
}

impl AsyncRead for DualStream {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        match self.get_mut() {
            Self::Plain(s) => Pin::new(s).poll_read(cx, buf),
            Self::Tls(s) => Pin::new(s).poll_read(cx, buf),
            Self::Blaze(s) => Pin::new(s).poll_read(cx, buf),
        }
    }
}

impl AsyncWrite for DualStream {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        match self.get_mut() {
            Self::Plain(s) => Pin::new(s).poll_write(cx, buf),
            Self::Tls(s) => Pin::new(s).poll_write(cx, buf),
            Self::Blaze(s) => Pin::new(s).poll_write(cx, buf),
        }
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        match self.get_mut() {
            Self::Plain(s) => Pin::new(s).poll_flush(cx),
            Self::Tls(s) => Pin::new(s).poll_flush(cx),
            Self::Blaze(s) => Pin::new(s).poll_flush(cx),
        }
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        match self.get_mut() {
            Self::Plain(s) => Pin::new(s).poll_shutdown(cx),
            Self::Tls(s) => Pin::new(s).poll_shutdown(cx),
            Self::Blaze(s) => Pin::new(s).poll_shutdown(cx),
        }
    }
}

/// Peeks at the incoming TCP stream to detect TLS/SSL ClientHello (0x16) or plain Blaze TCP frames.
/// Initiates standard TLS 1.2/1.3 handshake via the trusted TlsAcceptor.
pub async fn accept_dual_stream(
    tcp_stream: TcpStream,
    tls_acceptor: &TlsAcceptor,
) -> io::Result<DualStream> {
    let peer_addr = tcp_stream.peer_addr().map(|a| a.to_string()).unwrap_or_else(|_| "unknown".to_string());
    let local_addr = tcp_stream.local_addr().map(|a| a.to_string()).unwrap_or_else(|_| "unknown".to_string());
    tracing::info!("[DualListener] TCP connection accepted: {} -> {}", peer_addr, local_addr);

    let mut peek_buf = [0u8; 8];
    let n = tcp_stream.peek(&mut peek_buf).await?;
    if n == 0 {
        tracing::warn!("[DualListener] Connection from {} closed immediately by client (peek=0)", peer_addr);
        return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "Client disconnected immediately"));
    }

    if peek_buf[0] == 0x16 {
        tracing::info!("[DualListener] Detected TLS Handshake (0x16) from {} (peeked {} bytes: {:02X?}) -> Initiating TLS Handshake", peer_addr, n, &peek_buf[..n]);

        // Capture the ClientHello before handing the socket to rustls.
        // `peek` never consumes, so the acceptor still sees every byte.
        //
        // The bytes are NOT rewritten. A TLS 1.3 handshake transcript is
        // computed over the raw ClientHello the client sent, so stripping
        // the `server_name` extension makes the client's Finished fail to
        // verify (`DecryptError`). The server has to parse the client's
        // bytes exactly as they arrived.
        let hello = peek_client_hello(&tcp_stream).await;
        if let Some(hello) = hello.as_ref() {
            tracing::info!(
                "[DualListener] ClientHello from {}: {} bytes, SNI={:?}, ALPN={:?}, ciphers={:04X?}",
                peer_addr,
                hello.raw.len(),
                hello.server_name.as_deref().unwrap_or("<none>"),
                hello.alpn,
                hello.cipher_suites
            );
            if empty_sni_extension(hello).is_some() {
                tracing::warn!(
                    "[DualListener] {} sent a zero-length SNI hostname (it dialled a bare IP). rustls rejects that; advertise a hostname instead of an IP literal.",
                    peer_addr
                );
            }
        }

        match tls_acceptor.accept(tcp_stream).await {
            Ok(tls_stream) => {
                tracing::info!("[DualListener] TLS handshake SUCCEEDED with {}!", peer_addr);
                Ok(DualStream::Tls(tls_stream))
            }
            Err(e) => {
                tracing::warn!("[DualListener] TLS handshake FAILED with {}: {:?}", peer_addr, e);
                if let Some(hello) = hello.as_ref() {
                    tracing::warn!(
                        "[DualListener] Failed ClientHello from {} ({} bytes) raw: {}",
                        peer_addr,
                        hello.raw.len(),
                        hex_dump(&hello.raw)
                    );
                }
                Err(io::Error::new(io::ErrorKind::Other, e))
            }
        }
    } else {
        tracing::info!(
            "[DualListener] Detected Plain TCP stream from {} (peeked {} bytes: {:02X?} / {:?})",
            peer_addr,
            n,
            &peek_buf[..n],
            String::from_utf8_lossy(&peek_buf[..n])
        );
        Ok(DualStream::Plain(tcp_stream))
    }
}

/// The parts of a TLS ClientHello worth logging, plus the offsets needed to
/// rewrite it.
struct ClientHello {
    raw: Vec<u8>,
    server_name: Option<String>,
    alpn: Vec<String>,
    /// The cipher suites the client offered, in wire order.
    ///
    /// The fingerprint of this list is what tells the two client TLS stacks
    /// apart: the DirtySDK ProtoSSL stack and the HTTP/2 stack do not offer
    /// the same suites, so a mismatch here explains why one handshake is
    /// trusted and the other is not.
    cipher_suites: Vec<u16>,
    /// Each extension as (offset within `raw`, extension type, total length).
    extensions: Vec<(usize, u16, usize)>,
}

/// Peeks the complete first TLS record without consuming it.
///
/// `peek` returns whatever is already buffered, so this loops until the whole
/// record is visible. The socket is left untouched for the TLS acceptor.
async fn peek_client_hello(stream: &TcpStream) -> Option<ClientHello> {
    // The record header is: type(1) legacy_version(2) length(2).
    let mut head = [0u8; 5];
    let mut got = 0usize;
    while got < head.len() {
        match stream.peek(&mut head[got..]).await {
            Ok(0) => break,
            Ok(n) => got += n,
            Err(_) => return None,
        }
    }
    if got < head.len() {
        return None;
    }
    let record_len = u16::from_be_bytes([head[3], head[4]]) as usize;
    let total = head.len() + record_len;

    let mut raw = vec![0u8; total];
    let mut seen = 0usize;
    while seen < total {
        match stream.peek(&mut raw[seen..]).await {
            Ok(0) => break,
            Ok(n) => seen += n,
            Err(_) => return None,
        }
    }
    if seen < total {
        return None;
    }

    parse_client_hello(&raw)
}

/// Parses a ClientHello record.
///
/// The layout is the standard one: record header, handshake header,
/// client_version(2), random(32), session_id, cipher_suites,
/// compression_methods, then the extensions.
fn parse_client_hello(record: &[u8]) -> Option<ClientHello> {
    // Skip the record header (5) and the handshake header (4).
    let mut p = 9usize;
    if record.len() < p + 2 + 32 {
        return None;
    }
    p += 2 + 32; // client_version + random

    // session_id
    if p >= record.len() {
        return None;
    }
    let session_id_len = record[p] as usize;
    p += 1 + session_id_len;

    // cipher_suites
    if p + 2 > record.len() {
        return None;
    }
    let cipher_len = u16::from_be_bytes([record[p], record[p + 1]]) as usize;
    let cipher_end = (p + 2 + cipher_len).min(record.len());
    let mut cipher_suites = Vec::with_capacity(cipher_len / 2);
    let mut q = p + 2;
    while q + 1 < cipher_end {
        cipher_suites.push(u16::from_be_bytes([record[q], record[q + 1]]));
        q += 2;
    }
    p += 2 + cipher_len;

    // compression_methods
    if p >= record.len() {
        return None;
    }
    let compression_len = record[p] as usize;
    p += 1 + compression_len;

    // extensions
    if p + 2 > record.len() {
        return None;
    }
    let extensions_len = u16::from_be_bytes([record[p], record[p + 1]]) as usize;
    p += 2;
    let end = (p + extensions_len).min(record.len());

    let mut server_name = None;
    let mut alpn = Vec::new();
    let mut extensions: Vec<(usize, u16, usize)> = Vec::new();

    while p + 4 <= end {
        let ext_offset = p;
        let ext_type = u16::from_be_bytes([record[p], record[p + 1]]);
        let ext_len = u16::from_be_bytes([record[p + 2], record[p + 3]]) as usize;
        p += 4;
        let body_end = (p + ext_len).min(end);
        extensions.push((ext_offset, ext_type, body_end - ext_offset));

        match ext_type {
            // server_name
            0 => {
                if p + 2 <= body_end {
                    let list_len = u16::from_be_bytes([record[p], record[p + 1]]) as usize;
                    let mut q = p + 2;
                    let list_end = (q + list_len).min(body_end);
                    while q + 3 <= list_end {
                        let name_type = record[q];
                        let name_len = u16::from_be_bytes([record[q + 1], record[q + 2]]) as usize;
                        q += 3;
                        if q + name_len > list_end {
                            break;
                        }
                        if name_type == 0 {
                            server_name =
                                Some(String::from_utf8_lossy(&record[q..q + name_len]).to_string());
                        }
                        q += name_len;
                    }
                }
            }
            // application_layer_protocol_negotiation
            16 => {
                if p + 2 <= body_end {
                    let list_len = u16::from_be_bytes([record[p], record[p + 1]]) as usize;
                    let mut q = p + 2;
                    let list_end = (q + list_len).min(body_end);
                    while q < list_end {
                        let proto_len = record[q] as usize;
                        q += 1;
                        if q + proto_len > list_end {
                            break;
                        }
                        alpn.push(String::from_utf8_lossy(&record[q..q + proto_len]).to_string());
                        q += proto_len;
                    }
                }
            }
            _ => {}
        }
        p = body_end;
    }

    Some(ClientHello {
        raw: record.to_vec(),
        server_name,
        alpn,
        cipher_suites,
        extensions,
    })
}

/// Finds a `server_name` extension whose hostname is empty.
///
/// Returns the extension's offset within the record and its total length.
///
/// A zero-length SNI hostname is what a client sends when the URL host is
/// an IP literal. rustls 0.23.46 rejects it with
/// `InvalidMessage(IllegalEmptyValue)`, so this is reported rather than
/// silently tolerated -- the fix belongs in the addresses we advertise
/// (EA advertises hostnames), never in a rewrite of the client's bytes:
/// the TLS 1.3 handshake transcript covers the ClientHello exactly as it
/// arrived, so any byte-level change makes the client's Finished fail to
/// verify.
fn empty_sni_extension(hello: &ClientHello) -> Option<(usize, usize)> {
    for (offset, ext_type, total_len) in &hello.extensions {
        if *ext_type != 0 || *total_len < 9 {
            continue;
        }
        // The extension body is: list_len(2) then name entries of
        // name_type(1) + name_len(2) + name.
        let body = &hello.raw[offset + 4..offset + total_len];
        let list_len = u16::from_be_bytes([body[0], body[1]]) as usize;
        if list_len == 0 || body.len() < 5 {
            continue;
        }
        if body[2] == 0 && u16::from_be_bytes([body[3], body[4]]) == 0 {
            return Some((*offset, *total_len));
        }
    }
    None
}

/// Renders bytes as a classic hex dump, 16 bytes per line.
fn hex_dump(bytes: &[u8]) -> String {
    let mut out = String::new();
    for (i, chunk) in bytes.chunks(16).enumerate() {
        out.push_str(&format!("{:04X}  ", i * 16));
        for b in chunk {
            out.push_str(&format!("{:02X} ", b));
        }
        for _ in chunk.len()..16 {
            out.push_str("   ");
        }
        out.push(' ');
        for b in chunk {
            out.push(if (0x20..0x7F).contains(b) { *b as char } else { '.' });
        }
        out.push('\n');
    }
    out
}

/// Reads one TLS frame and then the rest of the stream.
pub async fn accept_blaze_stream(
    tcp_stream: TcpStream,
    tls_acceptor: &TlsAcceptor,
) -> io::Result<DualStream> {
    accept_dual_stream(tcp_stream, tls_acceptor).await
}

pub async fn accept_telemetry_stream(
    tcp_stream: TcpStream,
    tls_acceptor: &TlsAcceptor,
) -> io::Result<DualStream> {
    accept_dual_stream(tcp_stream, tls_acceptor).await
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The ClientHello EA's client sends when it connects to a bare IP
    /// address, captured live. Its `server_name` extension carries a
    /// zero-length hostname, which rustls rejects.
    const EA_EMPTY_SNI_CLIENT_HELLO: &[u8] = &[
        0x16, 0x03, 0x03, 0x01, 0x22, 0x01, 0x00, 0x01, 0x1E, 0x03, 0x03, 0x1C, 0xCC, 0xF4, 0xE0,
        0x5C, 0x50, 0x84, 0x70, 0xC7, 0x5D, 0xA3, 0xBA, 0x20, 0xB3, 0xFB, 0x9E, 0x43, 0x0F, 0x2E,
        0x47, 0x52, 0xAF, 0xD1, 0x67, 0x9B, 0x43, 0x03, 0xDC, 0xFE, 0x57, 0xA7, 0x2B, 0x20, 0xF8,
        0x68, 0xBC, 0x0B, 0xB2, 0x4F, 0x7E, 0x5A, 0xA9, 0x3F, 0xD7, 0xA7, 0xA7, 0x35, 0xA8, 0x83,
        0x49, 0xF0, 0xA1, 0x4E, 0x60, 0x44, 0x67, 0x87, 0xB3, 0x6C, 0x7A, 0x68, 0xE3, 0xFD, 0x14,
        0xA2, 0x00, 0x2E, 0x13, 0x01, 0x13, 0x02, 0x13, 0x03, 0xC0, 0x2B, 0xC0, 0x2C, 0xCC, 0xA9,
        0xC0, 0x2F, 0xC0, 0x30, 0xCC, 0xA8, 0x00, 0x9C, 0x00, 0x9D, 0xC0, 0x23, 0xC0, 0x24, 0xC0,
        0x27, 0xC0, 0x28, 0x00, 0x3C, 0x00, 0x3D, 0xC0, 0x09, 0xC0, 0x0A, 0xC0, 0x13, 0xC0, 0x14,
        0x00, 0x2F, 0x00, 0x35, 0x01, 0x00, 0x00, 0xA7, 0x00, 0x00, 0x00, 0x05, 0x00, 0x03, 0x00,
        0x00, 0x00, 0x00, 0x0A, 0x00, 0x0A, 0x00, 0x08, 0x00, 0x17, 0x00, 0x1D, 0x00, 0x18, 0x00,
        0x1E, 0x00, 0x0B, 0x00, 0x02, 0x01, 0x00, 0x00, 0x10, 0x00, 0x05, 0x00, 0x03, 0x02, 0x68,
        0x32, 0xFF, 0x01, 0x00, 0x01, 0x00, 0x00, 0x33, 0x00, 0x47, 0x00, 0x45, 0x00, 0x17, 0x00,
        0x41, 0x04, 0x09, 0x1A, 0x3C, 0xB0, 0xE6, 0x36, 0x8E, 0xDA, 0x91, 0x27, 0x95, 0x6C, 0x11,
        0x95, 0x7C, 0x5A, 0x42, 0x51, 0x85, 0x16, 0x9E, 0x9E, 0x3A, 0x08, 0xA1, 0x0E, 0xA7, 0xAD,
        0x4B, 0x11, 0xBA, 0x04, 0x72, 0xF3, 0xC8, 0xF7, 0xB5, 0xB2, 0xB9, 0x15, 0x3A, 0x49, 0xA0,
        0x11, 0xAB, 0xB6, 0x91, 0xAF, 0x1C, 0x60, 0x2A, 0x0F, 0x4F, 0x57, 0x2B, 0x35, 0x03, 0x66,
        0xDF, 0xC2, 0x1D, 0xB3, 0x92, 0xB9, 0x00, 0x2B, 0x00, 0x05, 0x04, 0x03, 0x04, 0x03, 0x03,
        0x00, 0x0D, 0x00, 0x1E, 0x00, 0x1C, 0x04, 0x03, 0x08, 0x07, 0x08, 0x09, 0x08, 0x04, 0x04,
        0x01, 0x05, 0x03, 0x08, 0x0A, 0x08, 0x05, 0x05, 0x01, 0x08, 0x0B, 0x08, 0x06, 0x06, 0x01,
        0x02, 0x03, 0x02, 0x01, 0x00, 0x2D, 0x00, 0x02, 0x01, 0x01,
    ];

    #[test]
    fn the_ea_client_hello_has_an_empty_server_name() {
        let hello = parse_client_hello(EA_EMPTY_SNI_CLIENT_HELLO).expect("must parse");
        assert_eq!(hello.server_name.as_deref(), Some(""), "the SNI hostname is empty");
        assert_eq!(hello.alpn, vec!["h2".to_string()]);
        let (offset, len) = empty_sni_extension(&hello).expect("the empty SNI must be found");
        assert_eq!(len, 9, "type(2) + length(2) + list(5)");
        assert_eq!(offset, 0x80);
    }

    #[test]
    fn a_client_hello_with_a_real_server_name_is_not_flagged() {
        // A `server_name` extension carrying an actual hostname:
        // type(2) + length(2) + list_len(2) + name_type(1) + name_len(2) + name(3)
        let mut raw = Vec::new();
        raw.extend_from_slice(&0u16.to_be_bytes()); // extension type: server_name
        raw.extend_from_slice(&12u16.to_be_bytes()); // extension length
        raw.extend_from_slice(&3u16.to_be_bytes()); // server_name_list length
        raw.push(0); // name_type: host_name
        raw.extend_from_slice(&3u16.to_be_bytes()); // hostname length
        raw.extend_from_slice(b"qos");

        let hello = ClientHello {
            raw: raw.clone(),
            server_name: Some("qos".to_string()),
            alpn: Vec::new(),
            cipher_suites: Vec::new(),
            extensions: vec![(0, 0, raw.len())],
        };

        assert!(empty_sni_extension(&hello).is_none());
    }
}
