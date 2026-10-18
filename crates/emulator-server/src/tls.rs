//! TLS Certificate Generation and TlsAcceptor Setup.

use std::sync::Arc;
use std::path::Path;
use rcgen::{BasicConstraints, CertificateParams, DnType, IsCa, KeyPair, SanType};
use rustls_pki_types::{CertificateDer, PrivateKeyDer};
use tokio_rustls::rustls::ServerConfig;
use tokio_rustls::TlsAcceptor;

/// Generates or loads a persistent Certificate Authority and Server certificate covering EA domains,
/// returning an active tokio-rustls TlsAcceptor.
pub fn create_self_signed_tls_acceptor() -> Result<TlsAcceptor, Box<dyn std::error::Error + Send + Sync>> {
    let cert_path = Path::new("f22_ca.crt");
    let key_path = Path::new("f22_ca.key");

    let (cert_der_bytes, key_der_bytes) = if cert_path.exists() && key_path.exists() {
        tracing::info!("[TLS] Loading existing CA certificate from f22_ca.crt");
        (std::fs::read(cert_path)?, std::fs::read(key_path)?)
    } else {
        tracing::info!("[TLS] Generating new persistent Root CA and Server certificate");
        let mut params = CertificateParams::default();
        params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
        params.distinguished_name.push(DnType::CommonName, "gosredirector.ea.com");
        params.distinguished_name.push(DnType::OrganizationName, "Electronic Arts Clean-Room Emulator");

        params.subject_alt_names = vec![
            SanType::DnsName("spring18.gosredirector.ea.com".try_into()?),
            SanType::DnsName("*.gosredirector.ea.com".try_into()?),
            SanType::DnsName("gosredirector.ea.com".try_into()?),
            SanType::DnsName("*.ea.com".try_into()?),
            SanType::DnsName("*.ea.net".try_into()?),
            SanType::DnsName("fifa22.pc.ea.com".try_into()?),
            SanType::DnsName("river.data.ea.com".try_into()?),
            SanType::DnsName("pin-river.data.ea.com".try_into()?),
            SanType::DnsName("freeform-river.data.ea.com".try_into()?),
            SanType::DnsName("rl.data.ea.com".try_into()?),
            SanType::DnsName("localhost".try_into()?),
            SanType::IpAddress("127.0.0.1".parse()?),
            SanType::IpAddress("26.178.140.32".parse()?),
        ];

        let key_pair = KeyPair::generate()?;
        let cert = params.self_signed(&key_pair)?;
        let cert_der = cert.der().to_vec();
        let key_der = key_pair.serialize_der();

        let _ = std::fs::write(cert_path, &cert_der);
        let _ = std::fs::write(key_path, &key_der);

        // Also write PEM format for easy inspection or import
        let _ = std::fs::write("f22_ca.pem", cert.pem());

        // Automatically install into Windows CurrentUser\Root store
        install_cert_into_windows_root(cert_path);

        (cert_der, key_der)
    };

    let cert_der = CertificateDer::from(cert_der_bytes);
    let key_der = PrivateKeyDer::Pkcs8(key_der_bytes.into());

    let server_config = ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(vec![cert_der], key_der)?;

    Ok(TlsAcceptor::from(Arc::new(server_config)))
}

fn install_cert_into_windows_root(cert_path: &Path) {
    if let Some(path_str) = cert_path.to_str() {
        let _ = std::process::Command::new("certutil")
            .args(["-user", "-f", "-addstore", "CA", path_str])
            .output();
        let _ = std::process::Command::new("certutil")
            .args(["-user", "-f", "-addstore", "TrustedPublisher", path_str])
            .output();
    }
}
