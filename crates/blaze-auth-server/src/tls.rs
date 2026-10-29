use std::sync::Arc;
use rustls_pki_types::{CertificateDer, PrivateKeyDer};
use tokio_rustls::rustls::ServerConfig;
use tokio_rustls::TlsAcceptor;

pub static SERVER_CRT: &[u8] = include_bytes!("../certs/f22_server.crt");
pub static CA_CRT: &[u8] = include_bytes!("../certs/f22_ca.crt");
pub static SERVER_KEY: &[u8] = include_bytes!("../certs/f22_server.key");
pub static CA_MODULUS: &[u8] = include_bytes!("../certs/f22_ca_modulus.bin");
pub static CA_CRL: &[u8] = include_bytes!("../certs/f22_ca.crl");

#[derive(Debug)]
struct FallbackCertResolver {
    default_cert: Arc<tokio_rustls::rustls::sign::CertifiedKey>,
}

impl tokio_rustls::rustls::server::ResolvesServerCert for FallbackCertResolver {
    fn resolve(
        &self,
        _client_hello: tokio_rustls::rustls::server::ClientHello,
    ) -> Option<Arc<tokio_rustls::rustls::sign::CertifiedKey>> {
        Some(self.default_cert.clone())
    }
}

pub fn create_tls_acceptor() -> Result<TlsAcceptor, Box<dyn std::error::Error + Send + Sync>> {
    let cert_chain = vec![
        CertificateDer::from(SERVER_CRT),
        CertificateDer::from(CA_CRT),
    ];
    let key_der = PrivateKeyDer::Pkcs8(SERVER_KEY.to_vec().into());

    let signing_key = tokio_rustls::rustls::crypto::ring::sign::any_supported_type(&key_der)?;
    let server_config = ServerConfig::builder()
        .with_no_client_auth()
        .with_cert_resolver(Arc::new(FallbackCertResolver {
            default_cert: Arc::new(tokio_rustls::rustls::sign::CertifiedKey::new(cert_chain, signing_key)),
        }));

    Ok(TlsAcceptor::from(Arc::new(server_config)))
}
