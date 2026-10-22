//! TLS Certificate Generation and TlsAcceptor Setup.
//! Compatible with EA Frostbite 3 / FIFA 22 DirtySDK ProtoSSL (RSA 2048 with SHA-256).

use std::sync::Arc;
use std::path::Path;
use rcgen::{
    BasicConstraints, CertificateParams, CertificateRevocationListParams, CrlDistributionPoint,
    DnType, IsCa, KeyIdMethod, KeyPair, KeyUsagePurpose, SanType, SerialNumber,
};
use rustls_pki_types::{CertificateDer, PrivateKeyDer};
use time::{Duration, OffsetDateTime};
use tokio_rustls::rustls::ServerConfig;
use tokio_rustls::TlsAcceptor;

/// The port the certificate revocation list is published on.
///
/// The CRL has to be reachable over **plain HTTP**: a `https://` distribution
/// point would make Windows fetch the CRL over TLS, and that TLS fetch would
/// itself need a revocation check -- which is the very thing that is missing.
/// A loopback HTTP port breaks the recursion.
pub const CRL_PORT: u16 = 8099;

/// The CRL distribution point that is baked into the leaf certificate.
///
/// Windows' revocation engine (crypt32) reads the `crlDistributionPoints`
/// extension of the leaf and fetches the CRL from this URL. Without it the
/// chain has no revocation information at all, and a strict TLS client --
/// WinHTTP, which is what FIFA 22's HTTP/2 stack is -- refuses the
/// handshake with `CRYPT_E_NO_REVOCATION_CHECK` (0x80092012) even though
/// the chain itself is perfectly valid and trusted.
pub const CRL_DISTRIBUTION_POINT_URL: &str = "http://127.0.0.1:8099/f22_ca.crl";

/// The file the DER-encoded CRL is written to.
pub const CRL_FILE_NAME: &str = "f22_ca.crl";

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

/// Computes standard FIPS 180-4 SHA-256 digest of data.
pub fn sha256_digest(data: &[u8]) -> [u8; 32] {
    let mut h: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a,
        0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19,
    ];
    let k: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
        0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
        0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
        0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
        0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
        0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
        0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
    ];

    let bit_len = (data.len() as u64) * 8;
    let mut msg = data.to_vec();
    msg.push(0x80);
    while (msg.len() + 8) % 64 != 0 {
        msg.push(0x00);
    }
    msg.extend_from_slice(&bit_len.to_be_bytes());

    for chunk in msg.chunks_exact(64) {
        let mut w = [0u32; 64];
        for i in 0..16 {
            w[i] = u32::from_be_bytes([chunk[4 * i], chunk[4 * i + 1], chunk[4 * i + 2], chunk[4 * i + 3]]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16].wrapping_add(s0).wrapping_add(w[i - 7]).wrapping_add(s1);
        }

        let mut a = h[0];
        let mut b = h[1];
        let mut c = h[2];
        let mut d = h[3];
        let mut e = h[4];
        let mut f = h[5];
        let mut g = h[6];
        let mut hh = h[7];

        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let temp1 = hh.wrapping_add(s1).wrapping_add(ch).wrapping_add(k[i]).wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let temp2 = s0.wrapping_add(maj);

            hh = g;
            g = f;
            f = e;
            e = d.wrapping_add(temp1);
            d = c;
            c = b;
            b = a;
            a = temp1.wrapping_add(temp2);
        }

        h[0] = h[0].wrapping_add(a);
        h[1] = h[1].wrapping_add(b);
        h[2] = h[2].wrapping_add(c);
        h[3] = h[3].wrapping_add(d);
        h[4] = h[4].wrapping_add(e);
        h[5] = h[5].wrapping_add(f);
        h[6] = h[6].wrapping_add(g);
        h[7] = h[7].wrapping_add(hh);
    }

    let mut out = [0u8; 32];
    for (i, val) in h.iter().enumerate() {
        out[i * 4..i * 4 + 4].copy_from_slice(&val.to_be_bytes());
    }
    out
}

/// Formats a 32-byte SHA-256 digest into an uppercase colon-delimited string (e.g. "AA:BB:CC:...").
pub fn format_fingerprint(hash: &[u8; 32]) -> String {
    hash.iter()
        .map(|b| format!("{:02X}", b))
        .collect::<Vec<_>>()
        .join(":")
}

/// The artefacts a generated certificate chain consists of.
pub struct ChainArtifacts {
    /// The leaf (server) certificate, DER.
    pub leaf_der: Vec<u8>,
    /// The root CA certificate, DER.
    pub ca_der: Vec<u8>,
    /// The leaf's private key, PKCS#8 DER.
    pub leaf_key_der: Vec<u8>,
    /// The revocation list the leaf points at, DER.
    pub crl_der: Vec<u8>,
    /// The root CA's RSA modulus, 256 bytes big endian.
    pub ca_modulus: [u8; 256],
}

/// Generates a fresh RSA-2048 root CA, a leaf certificate signed by it and a
/// revocation list, and writes all of them into `dir`.
///
/// Nothing outside `dir` is written, so a test can generate a chain into a
/// temporary directory without disturbing the certificates the running
/// emulator serves.
pub fn generate_chain(
    dir: &Path,
    custom_cn: Option<&str>,
) -> Result<ChainArtifacts, Box<dyn std::error::Error + Send + Sync>> {
    let ca_cert_path = dir.join("f22_ca.crt");
    let ca_key_path = dir.join("f22_ca.key");
    let leaf_cert_path = dir.join("f22_server.crt");
    let leaf_key_path = dir.join("f22_server.key");
    let modulus_path = dir.join("f22_ca_modulus.bin");
    let crl_path = dir.join(CRL_FILE_NAME);

    tracing::info!("[TLS] Generating fresh persistent RSA 2048 DigiCert Root CA and Server certificate");

    use rsa::pkcs8::EncodePrivateKey;
    use rsa::traits::PublicKeyParts;
    use rsa::RsaPrivateKey;
    use rsa::rand_core::OsRng;

    let mut rng = OsRng;

    // 1. Root CA matching DirtySDK's embedded DigiCert Global Root CA
    let mut ca_params = CertificateParams::default();
    ca_params.not_before = rcgen::date_time_ymd(2006, 11, 10);
    ca_params.not_after = rcgen::date_time_ymd(2031, 11, 10);
    ca_params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
    // The real DigiCert Global Root CA carries `keyCertSign` and `cRLSign`.
    // Ours has to carry them too: `cRLSign` is what authorises this CA to
    // issue the revocation list that the leaf points at, and rcgen refuses
    // to sign a CRL with a CA whose key usages do not include it.
    ca_params.key_usages = vec![KeyUsagePurpose::KeyCertSign, KeyUsagePurpose::CrlSign];
    // Writes the `authorityKeyIdentifier` extension, which is what lets a
    // verifier bind a CRL to this CA by key id instead of by name alone.
    ca_params.use_authority_key_identifier_extension = true;
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

    let _ = std::fs::write(&modulus_path, &modulus);
    tracing::info!(
        "[TLS] Exported 256-byte Root CA RSA modulus to {}",
        modulus_path.display()
    );

    // 2. Leaf certificate signed by the DigiCert Root CA
    let cn = custom_cn.unwrap_or("gosredirector.ea.com");
    let mut leaf_params = CertificateParams::default();
    leaf_params.not_before = rcgen::date_time_ymd(2020, 1, 1);
    leaf_params.not_after = rcgen::date_time_ymd(2030, 12, 31);
    leaf_params.distinguished_name.push(DnType::CommonName, cn);
    leaf_params.distinguished_name.push(DnType::OrganizationName, "Electronic Arts Inc");
    leaf_params.distinguished_name.push(DnType::CountryName, "US");

    let mut sans = vec![
        SanType::DnsName(cn.to_string().try_into()?),
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
        // The QoS service is advertised to the client under its real EA host
        // names (see `services_blaze::util::QOSS`). The client turns that host
        // into the TLS SNI, so the leaf has to cover the whole `gos.ea.com`
        // zone or the handshake fails on name mismatch.
        SanType::DnsName("*.gos.ea.com".try_into()?),
        SanType::DnsName("qos-prod-bio-dub-common-common.gos.ea.com".try_into()?),
        SanType::DnsName("qos-prod-bio-iad-common-common.gos.ea.com".try_into()?),
        SanType::DnsName("qos-prod-bio-sjc-common-common.gos.ea.com".try_into()?),
        SanType::DnsName("qos-prod-bio-syd-common-common.gos.ea.com".try_into()?),
        SanType::DnsName("qos-prod-m3d-brz-common-common.gos.ea.com".try_into()?),
        SanType::DnsName("qos-prod-m3d-nrt-common-common.gos.ea.com".try_into()?),
        SanType::IpAddress("127.0.0.1".parse()?),
        SanType::IpAddress("26.178.140.32".parse()?),
    ];
    // Remove duplicate SANs
    sans.dedup();
    leaf_params.subject_alt_names = sans;

    // Tell the verifier where to find this certificate's revocation status.
    // Without this extension Windows has nothing to check and a strict TLS
    // client rejects the chain with `CRYPT_E_NO_REVOCATION_CHECK`.
    leaf_params.crl_distribution_points = vec![CrlDistributionPoint {
        uris: vec![CRL_DISTRIBUTION_POINT_URL.to_string()],
    }];

    let leaf_rsa = RsaPrivateKey::new(&mut rng, 2048)?;
    let leaf_rsa_der = leaf_rsa.to_pkcs8_der()?;
    let leaf_key_pair = KeyPair::try_from(leaf_rsa_der.as_bytes())?;
    let leaf_cert = leaf_params.signed_by(&leaf_key_pair, &ca_cert, &ca_key_pair)?;
    let leaf_der = leaf_cert.der().to_vec();
    let leaf_key_der = leaf_key_pair.serialize_der();

    // The revocation list the leaf points at. It revokes nothing: it exists
    // so that a strict TLS client has something to fetch and can conclude
    // that the leaf is good.
    let crl_der = build_crl(&ca_cert, &ca_key_pair)?;
    let _ = std::fs::write(&crl_path, &crl_der);
    tracing::info!(
        "[TLS] Published an empty CRL ({} bytes) at {}",
        crl_der.len(),
        CRL_DISTRIBUTION_POINT_URL
    );

    let _ = std::fs::write(&ca_cert_path, &ca_der);
    let _ = std::fs::write(&ca_key_path, &ca_key_der);
    let _ = std::fs::write(&leaf_cert_path, &leaf_der);
    let _ = std::fs::write(&leaf_key_path, &leaf_key_der);

    // Also write PEM formats
    let _ = std::fs::write(dir.join("f22_ca.pem"), ca_cert.pem());
    let _ = std::fs::write(dir.join("f22_server.pem"), leaf_cert.pem());

    Ok(ChainArtifacts {
        leaf_der,
        ca_der,
        leaf_key_der,
        crl_der,
        ca_modulus: modulus,
    })
}

/// Generates fresh persistent RSA-2048 DigiCert Root CA and Server certificates,
/// saving them to disk and returning (server_der, ca_der, server_key_der).
pub fn generate_and_save_certs(
    custom_cn: Option<&str>,
) -> Result<(Vec<u8>, Vec<u8>, Vec<u8>), Box<dyn std::error::Error + Send + Sync>> {
    let artifacts = generate_chain(Path::new("."), custom_cn)?;

    // Also copy the modulus to the game directory if it is there.
    let game_modulus_path = Path::new(r"G:\Games\FIFA 22\f22_ca_modulus.bin");
    if game_modulus_path.parent().map(|p| p.exists()).unwrap_or(false) {
        let _ = std::fs::write(game_modulus_path, &artifacts.ca_modulus);
        tracing::info!("[TLS] Deployed f22_ca_modulus.bin directly to G:\\Games\\FIFA 22\\");
    }

    // Install Root CA into Windows Certificate Store
    install_cert_into_windows_root(Path::new("f22_ca.crt"));

    Ok((artifacts.leaf_der, artifacts.ca_der, artifacts.leaf_key_der))
}

/// Builds the DER-encoded certificate revocation list for the emulator's CA.
///
/// The list is intentionally empty -- nothing is revoked -- but it is
/// *signed by the CA*, carries the CA's `authorityKeyIdentifier` and a CRL
/// number, exactly like a real CA's list. Windows matches the fetched CRL
/// against the issuer of the certificate it is validating; a list that is
/// not signed by that issuer is ignored and the chain stays unverifiable.
pub fn build_crl(
    ca_cert: &rcgen::Certificate,
    ca_key_pair: &KeyPair,
) -> Result<Vec<u8>, Box<dyn std::error::Error + Send + Sync>> {
    let now = OffsetDateTime::now_utc();
    let crl = CertificateRevocationListParams {
        this_update: now,
        next_update: now + Duration::days(365),
        crl_number: SerialNumber::from(1),
        issuing_distribution_point: None,
        revoked_certs: Vec::new(),
        key_identifier_method: KeyIdMethod::Sha256,
    }
    .signed_by(ca_cert, ca_key_pair)?;

    Ok(crl.der().to_vec())
}

/// Reads the persisted CRL from disk, if it is there.
pub fn load_crl() -> Option<Vec<u8>> {
    std::fs::read(Path::new(CRL_FILE_NAME)).ok()
}

/// CLI subcommand implementation to regenerate f22_server.crt/f22_ca.crt with configurable CN
/// and print the SHA-256 fingerprint for shim pinning.
pub fn regenerate_certificates(
    custom_cn: Option<&str>,
) -> Result<(String, String), Box<dyn std::error::Error + Send + Sync>> {
    let (server_der, ca_der, _) = generate_and_save_certs(custom_cn)?;
    let server_fp = format_fingerprint(&sha256_digest(&server_der));
    let ca_fp = format_fingerprint(&sha256_digest(&ca_der));

    println!("============================================================");
    println!(" FIFA 22 Clean-Room Certificate Generation (certgen)");
    println!("============================================================");
    println!("Common Name (CN):              {}", custom_cn.unwrap_or("gosredirector.ea.com"));
    println!("Server Certificate:            f22_server.crt (PEM: f22_server.pem)");
    println!("Root CA Certificate:          f22_ca.crt (PEM: f22_ca.pem)");
    println!("Root CA Modulus:               f22_ca_modulus.bin (256 bytes)");
    println!("------------------------------------------------------------");
    println!("SHA-256 Fingerprint (Server):  {}", server_fp);
    println!("SHA-256 Fingerprint (Root CA): {}", ca_fp);
    println!("Shim Pinning Hex:              {}", server_fp.replace(':', ""));
    println!("============================================================");

    Ok((server_fp, ca_fp))
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
        // A chain that was issued before the revocation list existed has no
        // `crlDistributionPoints` extension, so a strict TLS client has
        // nothing to check and rejects it. The presence of the CRL file is
        // the marker that the chain on disk carries the distribution point.
        && Path::new(CRL_FILE_NAME).exists()
    {
        tracing::info!("[TLS] Loading existing RSA 2048 certificate chain from disk");
        (
            std::fs::read(leaf_cert_path)?,
            std::fs::read(ca_cert_path)?,
            std::fs::read(leaf_key_path)?,
        )
    } else {
        generate_and_save_certs(None)?
    };

    // The chain on disk is only trusted by the game if our root CA is in the
    // Windows trusted-root store. The game's DirtySDK ProtoSSL stack is
    // handled by patching the DigiCert root's RSA modulus (see the shim's
    // `f22_ca_modulus.bin` mechanism), but a *Windows* TLS client -- which
    // is what FIFA 22's HTTP/2 stack is -- validates against the store and
    // rejects our leaf with `UntrustedRoot` otherwise.
    //
    // This must run on the "load existing" path too: the certificates
    // persist across restarts, so the install would otherwise only ever
    // happen on the very first run.
    install_cert_into_windows_root(ca_cert_path);

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

/// Installs the emulator's root CA into the current user's Windows
/// certificate stores so that Windows TLS clients trust it.
///
/// This is **opt-in**, because it changes the machine's trust configuration:
/// set `F22_TRUST_CA=1` in the environment before starting the emulator.
/// The certificate is
///
/// ```text
/// CN=DigiCert Global Root CA, OU=www.digicert.com, O=DigiCert Inc, C=US
/// ```
///
/// and it is written to `Cert:\CurrentUser\Root` (plus `CA` and
/// `TrustedPublisher`), user scope only, so no elevation is required.
/// Remove it again with:
///
/// ```text
/// certutil -user -delstore Root "DigiCert Global Root CA"
/// ```
///
/// The game itself does not need this: FIFA 22 validates the emulator's
/// certificate against the root table that is compiled into its own modules
/// (`FIFA22.exe`, `EAWebKit.dll`), which the shim patches in-process
/// (`shim/src/cert_bypass.cpp`).  The store matters only for a stock
/// Windows TLS client -- `curl.exe`, for instance -- that has to talk to
/// the emulator's HTTPS sink.
///
/// `Root` is the store that decides whether a leaf certificate validates: it
/// is the "trusted root" store, and a self-signed CA is only a *root* there.
/// The `CA` store holds *intermediate* authorities, and a certificate sitting
/// in it is not trusted as a root -- a Windows TLS client still reports
/// `UntrustedRoot` for it.
fn install_cert_into_windows_root(cert_path: &Path) {
    let enabled = std::env::var("F22_TRUST_CA")
        .map(|v| !v.is_empty() && v != "0")
        .unwrap_or(false);

    if !enabled {
        tracing::info!(
            "[TLS] Not installing the root CA into the Windows certificate store \
             (set F22_TRUST_CA=1 to enable it for stock Windows TLS clients)"
        );
        return;
    }

    let Some(path_str) = cert_path.to_str() else {
        tracing::warn!("[TLS] CA path is not valid UTF-8; not installing into the Windows store");
        return;
    };

    for store in ["Root", "CA", "TrustedPublisher"] {
        match std::process::Command::new("certutil")
            .args(["-user", "-f", "-addstore", store, path_str])
            .output()
        {
            Ok(out) if out.status.success() => {
                tracing::info!(
                    "[TLS] Installed root CA into the Windows store '{}' (user scope)",
                    store
                );
            }
            Ok(out) => {
                tracing::warn!(
                    "[TLS] certutil -addstore {} failed: {}",
                    store,
                    String::from_utf8_lossy(&out.stderr).trim()
                );
            }
            Err(e) => {
                tracing::warn!("[TLS] Could not run certutil for store '{}': {}", store, e);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sha256_digest_empty() {
        let empty_digest = sha256_digest(b"");
        let hex = format_fingerprint(&empty_digest).replace(':', "").to_lowercase();
        assert_eq!(hex, "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855");
    }

    #[test]
    fn test_sha256_digest_quick_brown_fox() {
        let digest = sha256_digest(b"The quick brown fox jumps over the lazy dog");
        let hex = format_fingerprint(&digest).replace(':', "").to_lowercase();
        assert_eq!(hex, "d7a8fbb307d7809469ca9abcb0082e4f8d5651e46d3cdb762d02d0bf37c9e592");
    }
}


