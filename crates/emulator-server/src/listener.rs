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
    let mut peek_buf = [0u8; 1];
    let n = tcp_stream.peek(&mut peek_buf).await?;

    if n > 0 && peek_buf[0] == 0x16 {
        tracing::debug!("[DualListener] Detected TLS Handshake (0x16) -> Initiating TLS 1.2 Handshake");
        match tls_acceptor.accept(tcp_stream).await {
            Ok(tls_stream) => {
                tracing::info!("[DualListener] TLS handshake SUCCEEDED!");
                Ok(DualStream::Tls(tls_stream))
            }
            Err(e) => {
                tracing::warn!("[DualListener] TLS handshake failed: {:?}", e);
                Err(io::Error::new(io::ErrorKind::Other, e))
            }
        }
    } else {
        tracing::debug!("[DualListener] Detected Plain TCP stream (First byte: 0x{:02X})", peek_buf[0]);
        Ok(DualStream::Plain(tcp_stream))
    }
}

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
