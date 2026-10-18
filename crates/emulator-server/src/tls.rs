//! TLS Certificate Generation and TlsAcceptor Setup.

use std::sync::Arc;
use rcgen::{CertificateParams, DnType, KeyPair, SanType};
use rustls_pki_types::{CertificateDer, PrivateKeyDer};
use tokio_rustls::rustls::ServerConfig;
use tokio_rustls::TlsAcceptor;

/// Generates a self-signed certificate covering EA domains and localhost,
/// returning an active tokio-rustls TlsAcceptor.
pub fn create_self_signed_tls_acceptor() -> Result<TlsAcceptor, Box<dyn std::error::Error + Send + Sync>> {
    let mut params = CertificateParams::default();

    params.distinguished_name.push(DnType::CommonName, "gosredirector.ea.com");
    params.distinguished_name.push(DnType::OrganizationName, "Electronic Arts Clean-Room Emulator");

    params.subject_alt_names = vec![
        SanType::DnsName("gosredirector.ea.com".try_into()?),
        SanType::DnsName("fifa22.pc.ea.com".try_into()?),
        SanType::DnsName("river.data.ea.com".try_into()?),
        SanType::DnsName("pin-river.data.ea.com".try_into()?),
        SanType::DnsName("localhost".try_into()?),
        SanType::IpAddress("127.0.0.1".parse()?),
    ];

    let key_pair = KeyPair::generate()?;
    let cert = params.self_signed(&key_pair)?;

    let cert_der = CertificateDer::from(cert.der().to_vec());
    let key_der = PrivateKeyDer::Pkcs8(key_pair.serialize_der().into());

    let server_config = ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(vec![cert_der], key_der)?;

    Ok(TlsAcceptor::from(Arc::new(server_config)))
}
