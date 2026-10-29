//! The leaf certificate has to carry a *working* revocation path.
//!
//! FIFA 22's HTTP/2 stack is a Windows WinHTTP client, and WinHTTP treats
//! "the chain carries no revocation information" as fatal: the handshake
//! fails with `CRYPT_E_NO_REVOCATION_CHECK` (0x80092012) even when the
//! chain is valid and its root is trusted. Browsers and `certutil` only
//! warn about it; WinHTTP does not.
//!
//! The fix is to give Windows something to fetch: a `crlDistributionPoints`
//! extension on the leaf that points at a revocation list signed by the
//! same CA. These tests pin the three links in that chain:
//!
//! 1. the leaf names the distribution point,
//! 2. the revocation list is bound to the CA that issued the leaf
//!    (its `authorityKeyIdentifier` is the CA's `subjectKeyIdentifier`),
//! 3. the list is actually served over plain HTTP.

use std::path::PathBuf;
use std::sync::Arc;

use emulator_server::crl_server::run_crl_server;
use emulator_server::tls::{generate_chain, CRL_DISTRIBUTION_POINT_URL, CRL_FILE_NAME};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

/// A directory that is unique to the test process.
fn scratch_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("f22-crl-{}-{}", name, std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("could not create the scratch directory");
    dir
}

/// Extracts the 20 byte subject key identifier from a certificate.
///
/// The `subjectKeyIdentifier` extension (OID 2.5.29.14) wraps the key id in
/// an OCTET STRING, which is itself wrapped in the extension's OCTET
/// STRING, so the id sits behind two `04 14` markers.
fn subject_key_identifier(der: &[u8]) -> Vec<u8> {
    let oid = [0x55, 0x1D, 0x0E];
    let oid_pos = der
        .windows(oid.len())
        .position(|w| w == oid)
        .expect("the certificate carries no subjectKeyIdentifier extension");

    let after_oid = &der[oid_pos + oid.len()..];
    let marker = [0x04, 0x14];
    let marker_pos = after_oid
        .windows(marker.len())
        .position(|w| w == marker)
        .expect("the subjectKeyIdentifier extension holds no 20 byte key id");

    after_oid[marker_pos + marker.len()..marker_pos + marker.len() + 20].to_vec()
}

/// The leaf has to name the distribution point the revocation list is
/// published at, or Windows has nothing to fetch.
#[test]
fn the_leaf_names_the_crl_distribution_point() {
    let dir = scratch_dir("dp");
    let artifacts = generate_chain(&dir, None).expect("chain generation failed");

    let url = CRL_DISTRIBUTION_POINT_URL.as_bytes();
    assert!(
        artifacts
            .leaf_der
            .windows(url.len())
            .any(|w| w == url),
        "the leaf certificate does not name {} as a CRL distribution point",
        CRL_DISTRIBUTION_POINT_URL
    );
}

/// The revocation list has to be the one the leaf's issuer signed: Windows
/// matches a fetched CRL against the issuer by key id.
#[test]
fn the_crl_is_bound_to_the_issuing_ca() {
    let dir = scratch_dir("aki");
    let artifacts = generate_chain(&dir, None).expect("chain generation failed");

    let ski = subject_key_identifier(&artifacts.ca_der);
    assert_eq!(ski.len(), 20, "unexpected subject key identifier length");

    assert!(
        artifacts.crl_der.windows(20).any(|w| w == ski.as_slice()),
        "the CRL's authority key identifier does not match the CA's subject key identifier"
    );

    // The list also has to name the CA that issued the leaf.
    assert!(
        artifacts
            .crl_der
            .windows(b"DigiCert Global Root CA".len())
            .any(|w| w == b"DigiCert Global Root CA"),
        "the CRL does not name its issuer"
    );
}

/// The list has to be a DER SEQUENCE -- an empty `revokedCertificates`
/// list is omitted entirely, as RFC 5280 section 5.1.2.6 requires.
#[test]
fn the_crl_is_a_der_sequence() {
    let dir = scratch_dir("der");
    let artifacts = generate_chain(&dir, None).expect("chain generation failed");

    assert_eq!(artifacts.crl_der[0], 0x30, "a CRL has to be a SEQUENCE");
    assert!(
        artifacts.crl_der.len() > 100,
        "a CRL of {} bytes is suspiciously small",
        artifacts.crl_der.len()
    );
}

/// The list has to be published over plain HTTP on the port the leaf names,
/// because an `https://` distribution point would make the CRL fetch itself
/// depend on a revocation check.
#[tokio::test]
async fn the_crl_is_served_over_plain_http() {
    let dir = scratch_dir("http");
    let artifacts = generate_chain(&dir, None).expect("chain generation failed");

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let crl = Arc::new(artifacts.crl_der.clone());
    tokio::spawn(run_crl_server(listener, crl.clone()));

    let mut stream = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
    let request = format!("GET /f22_ca.crl HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n");
    stream.write_all(request.as_bytes()).await.unwrap();

    let mut response = Vec::new();
    stream.read_to_end(&mut response).await.unwrap();

    let text = String::from_utf8_lossy(&response);
    assert!(
        text.starts_with("HTTP/1.1 200 OK"),
        "the CRL endpoint answered: {}",
        text.lines().next().unwrap_or("<empty>")
    );
    assert!(
        text.contains("application/pkix-crl"),
        "the CRL endpoint does not announce the CRL content type"
    );

    let body = response
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .map(|pos| &response[pos + 4..])
        .expect("the response has no header/body separator");

    assert_eq!(body, artifacts.crl_der.as_slice());
}

/// The distribution point has to be reachable at the URL the leaf names,
/// and it has to be a loopback HTTP URL -- a loopback TLS URL would
/// recurse into the revocation check that is failing.
#[test]
fn the_distribution_point_is_a_loopback_http_url() {
    assert!(
        CRL_DISTRIBUTION_POINT_URL.starts_with("http://127.0.0.1:"),
        "the distribution point must be plain HTTP on loopback, got {}",
        CRL_DISTRIBUTION_POINT_URL
    );
    assert!(
        !CRL_DISTRIBUTION_POINT_URL.starts_with("https://"),
        "an https:// distribution point would make the CRL fetch depend on a revocation check"
    );
    assert_eq!(CRL_FILE_NAME, "f22_ca.crl");
}
