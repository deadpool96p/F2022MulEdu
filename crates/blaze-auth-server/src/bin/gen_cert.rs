use rcgen::{date_time_ymd, BasicConstraints, CertificateParams, DnType, IsCa, KeyPair, SanType};
use std::net::IpAddr;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let ca_key_der = include_bytes!("../../certs/f22_ca.key");
    let server_key_der = include_bytes!("../../certs/f22_server.key");

    let ca_key = KeyPair::try_from(ca_key_der.as_slice())?;
    let mut ca_params = CertificateParams::default();
    ca_params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
    ca_params.distinguished_name.push(DnType::CommonName, "DigiCert Global Root CA");
    ca_params.distinguished_name.push(DnType::OrganizationName, "DigiCert Inc");
    ca_params.distinguished_name.push(DnType::OrganizationalUnitName, "www.digicert.com");
    ca_params.distinguished_name.push(DnType::CountryName, "US");

    let not_before = date_time_ymd(2020, 1, 1);
    let not_after = date_time_ymd(2035, 12, 31);

    ca_params.not_before = not_before;
    ca_params.not_after = not_after;

    let ca_cert = ca_params.self_signed(&ca_key)?;

    let server_key = KeyPair::try_from(server_key_der.as_slice())?;

    let mut params = CertificateParams::default();
    params.distinguished_name.push(DnType::CommonName, "gosredirector.ea.com");
    params.distinguished_name.push(DnType::OrganizationName, "Electronic Arts Inc");
    params.distinguished_name.push(DnType::CountryName, "US");
    params.not_before = not_before;
    params.not_after = not_after;

    params.subject_alt_names = vec![
        SanType::IpAddress(IpAddr::from([127, 0, 0, 1])),
        SanType::IpAddress(IpAddr::from([0, 0, 0, 0])),
        SanType::IpAddress("::1".parse()?),
        SanType::IpAddress("::ffff:127.0.0.1".parse()?),
        SanType::IpAddress("26.178.140.32".parse()?),
        SanType::DnsName("spring18.gosredirector.ea.com".try_into()?),
        SanType::DnsName("*.gosredirector.ea.com".try_into()?),
        SanType::DnsName("gosredirector.ea.com".try_into()?),
        SanType::DnsName("*.ea.com".try_into()?),
        SanType::DnsName("ea.com".try_into()?),
        SanType::DnsName("*.ea.net".try_into()?),
        SanType::DnsName("*.gameservices.ea.com".try_into()?),
        SanType::DnsName("bytevault.gameservices.ea.com".try_into()?),
        SanType::DnsName("accounts.ea.com".try_into()?),
        SanType::DnsName("accounts2s.ea.com".try_into()?),
        SanType::DnsName("signin.ea.com".try_into()?),
        SanType::DnsName("gateway.ea.com".try_into()?),
        SanType::DnsName("fifa22.pc.ea.com".try_into()?),
        SanType::DnsName("river.data.ea.com".try_into()?),
        SanType::DnsName("pin-river.data.ea.com".try_into()?),
        SanType::DnsName("freeform-river.data.ea.com".try_into()?),
        SanType::DnsName("rl.data.ea.com".try_into()?),
        SanType::DnsName("localhost".try_into()?),
        SanType::DnsName("*.gos.ea.com".try_into()?),
    ];

    let server_cert = params.signed_by(&server_key, &ca_cert, &ca_key)?;
    let server_cert_der = server_cert.der();

    std::fs::write("crates/blaze-auth-server/certs/f22_server.crt", server_cert_der)?;
    std::fs::write("crates/blaze-auth-server/certs/f22_server.pem", server_cert.pem())?;
    println!("Successfully generated new f22_server.crt with 2020-2035 validity ({} bytes)", server_cert_der.len());

    Ok(())
}
