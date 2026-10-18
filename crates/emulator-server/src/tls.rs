//! TLS Certificate Generation and TlsAcceptor Setup.
//! Compatible with EA Frostbite 3 / FIFA 22 DirtySDK ProtoSSL (RSA 2048 with SHA-256).

use std::sync::Arc;
use std::path::Path;
use rcgen::{BasicConstraints, CertificateParams, DnType, IsCa, KeyPair, SanType};
use rustls_pki_types::{CertificateDer, PrivateKeyDer};
use tokio_rustls::rustls::ServerConfig;
use tokio_rustls::TlsAcceptor;

/// Helper to extract the 256-byte RSA-2048 modulus from an X.509 certificate DER.
pub fn extract_rsa_2048_modulus(cert_der: &[u8]) -> Option<[u8; 256]> {
    // Look for ASN.1 INTEGER header: 0x02, 0x82, 0x01, 0x01, 0x00 (257 bytes with leading 00)
    let p1 = [0x02, 0x82, 0x01, 0x01, 0x00];
    if let Some(pos) = cert_der.windows(p1.len()).position(|w| w == p1) {
        let start = pos + p1.len();
        if start + 256 <= cert_der.len() {
            let mut out = [0u8; 256];
            out.copy_from_slice(&cert_der[start..start + 256]);
            return Some(out);
        }
    }

    // Alternative: 0x02, 0x82, 0x01, 0x00 (256 bytes without leading 00)
    let p2 = [0x02, 0x82, 0x01, 0x00];
    if let Some(pos) = cert_der.windows(p2.len()).position(|w| w == p2) {
        let start = pos + p2.len();
        if start + 256 <= cert_der.len() {
            let mut out = [0u8; 256];
            out.copy_from_slice(&cert_der[start..start + 256]);
            return Some(out);
        }
    }

    None
}

/// Generates or loads a persistent RSA-2048 Certificate Authority and Server certificate covering EA domains,
/// returning an active tokio-rustls TlsAcceptor.
pub fn create_self_signed_tls_acceptor() -> Result<TlsAcceptor, Box<dyn std::error::Error + Send + Sync>> {
    let ca_cert_path = Path::new("f22_ca.crt");
    let ca_key_path = Path::new("f22_ca.key");
    let leaf_cert_path = Path::new("f22_server.crt");
    let leaf_key_path = Path::new("f22_server.key");
    let modulus_path = Path::new("f22_ca_modulus.bin");

    let (leaf_der_bytes, ca_der_bytes, leaf_key_bytes) = if ca_cert_path.exists()
        && ca_key_path.exists()
        && leaf_cert_path.exists()
        && leaf_key_path.exists()
        && modulus_path.exists()
    {
        tracing::info!("[TLS] Loading existing RSA 2048 certificate chain from disk");
        (
            std::fs::read(leaf_cert_path)?,
            std::fs::read(ca_cert_path)?,
            std::fs::read(leaf_key_path)?,
        )
    } else {
        tracing::info!("[TLS] Generating new persistent RSA 2048 DigiCert Root CA and Server certificate");

        use rsa::pkcs8::EncodePrivateKey;
        use rsa::traits::PublicKeyParts;
        use rsa::RsaPrivateKey;
        use rsa::rand_core::OsRng;

        let mut rng = OsRng;

        // 1. Root CA matching DirtySDK's embedded DigiCert Global Root CA
        let mut ca_params = CertificateParams::default();
        ca_params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
        ca_params.distinguished_name.push(DnType::CommonName, "DigiCert Global Root CA");
        ca_params.distinguished_name.push(DnType::OrganizationName, "DigiCert Inc");
        ca_params.distinguished_name.push(DnType::OrganizationalUnitName, "www.digicert.com");
        ca_params.distinguished_name.push(DnType::CountryName, "US");

        let ca_rsa = RsaPrivateKey::new(&mut rng, 2048)?;
        let ca_rsa_der = ca_rsa.to_pkcs8_der()?;
        let ca_key_pair = KeyPair::try_from(ca_rsa_der.as_bytes())?;
        let ca_cert = ca_params.self_signed(&ca_key_pair)?;
        let ca_der = ca_cert.der().to_vec();
        let ca_key_der = ca_key_pair.serialize_der();

        // Extract and export the 256-byte RSA modulus
        let modulus_be = ca_rsa.n().to_bytes_be();
        let mut modulus = [0u8; 256];
        if modulus_be.len() == 256 {
            modulus.copy_from_slice(&modulus_be);
        } else if modulus_be.len() > 256 {
            modulus.copy_from_slice(&modulus_be[modulus_be.len() - 256..]);
        } else {
            modulus[256 - modulus_be.len()..].copy_from_slice(&modulus_be);
        }

        let _ = std::fs::write(modulus_path, &modulus);
        tracing::info!("[TLS] Exported 256-byte Root CA RSA modulus to f22_ca_modulus.bin");

        // Also copy to game directory if present
        let game_modulus_path = Path::new(r"G:\Games\FIFA 22\f22_ca_modulus.bin");
        if game_modulus_path.parent().map(|p| p.exists()).unwrap_or(false) {
            let _ = std::fs::write(game_modulus_path, &modulus);
            tracing::info!("[TLS] Deployed f22_ca_modulus.bin directly to G:\\Games\\FIFA 22\\");
        }

        // 2. Leaf certificate signed by the DigiCert Root CA
        let mut leaf_params = CertificateParams::default();
        leaf_params.distinguished_name.push(DnType::CommonName, "gosredirector.ea.com");
        leaf_params.distinguished_name.push(DnType::OrganizationName, "Electronic Arts Inc");
        leaf_params.distinguished_name.push(DnType::CountryName, "US");

        leaf_params.subject_alt_names = vec![
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

        let leaf_rsa = RsaPrivateKey::new(&mut rng, 2048)?;
        let leaf_rsa_der = leaf_rsa.to_pkcs8_der()?;
        let leaf_key_pair = KeyPair::try_from(leaf_rsa_der.as_bytes())?;
        let leaf_cert = leaf_params.signed_by(&leaf_key_pair, &ca_cert, &ca_key_pair)?;
        let leaf_der = leaf_cert.der().to_vec();
        let leaf_key_der = leaf_key_pair.serialize_der();

        let _ = std::fs::write(ca_cert_path, &ca_der);
        let _ = std::fs::write(ca_key_path, &ca_key_der);
        let _ = std::fs::write(leaf_cert_path, &leaf_der);
        let _ = std::fs::write(leaf_key_path, &leaf_key_der);


        // Also write PEM formats
        let _ = std::fs::write("f22_ca.pem", ca_cert.pem());
        let _ = std::fs::write("f22_server.pem", leaf_cert.pem());

        // Install Root CA into Windows Certificate Store
        install_cert_into_windows_root(ca_cert_path);

        (leaf_der, ca_der, leaf_key_der)
    };

    let cert_chain = vec![
        CertificateDer::from(leaf_der_bytes),
        CertificateDer::from(ca_der_bytes),
    ];
    let key_der = PrivateKeyDer::Pkcs8(leaf_key_bytes.into());

    let server_config = ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(cert_chain, key_der)?;

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


