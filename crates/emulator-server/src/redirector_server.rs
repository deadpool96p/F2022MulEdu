//! Redirector Service TCP Server (:42127).

use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio_rustls::TlsAcceptor;
use blaze_core::frame::BlazeHeader;
use blaze_core::framing::{detect_framing, Framing};
use blaze_core::BlazePacket;
use blaze_tdf::deserialize_tdf;
use services_redirector::{handle_redirector_packet, RedirectorConfig, ServiceInstance};
use crate::listener::{accept_dual_stream, DualStream};

/// Escapes the characters that are not allowed in XML text.
fn xml_escape(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            _ => out.push(ch),
        }
    }
    out
}

/// Renders the registered `ServerInstanceInfo` record of one advertised
/// service instance as XML.
///
/// The element names are the classic EA Redirector names: `type`,
/// `servicename`, `servicenames`, `serviceid`, `hostname`, `ip`, `port`,
/// `srcip`, `netmask`, `srcport`, `dstport` and the `addresses` list of
/// `ServerAddressInfo` entries.
///
/// The host name of the QoS instance is a `gosredirector` name and **not** one
/// of EA's `qos-prod-*.gos.ea.com` names: the client turns it into the TLS
/// SNI of the QoS connection, and the leaf certificate that is on disk
/// (`f22_server.crt`) only covers the `gosredirector.ea.com` names -- the
/// `*.ea.com` SAN does not match a multi-label `qos-prod-*.gos.ea.com` name.
/// A name the certificate does not cover would fail the handshake.
fn render_service_instance_xml(svc: &ServiceInstance) -> String {
    let service_names = svc
        .service_names
        .iter()
        .map(|n| xml_escape(n))
        .collect::<Vec<_>>()
        .join(",");

    let mut out = String::new();
    out.push_str("  <instance>\n");
    out.push_str(&format!("    <type>{}</type>\n", svc.instance_type));
    out.push_str(&format!(
        "    <servicename>{}</servicename>\n",
        xml_escape(&svc.service_name)
    ));
    out.push_str(&format!(
        "    <servicenames>{}</servicenames>\n",
        service_names
    ));
    out.push_str(&format!(
        "    <serviceid>{}</serviceid>\n",
        svc.service_id
    ));
    out.push_str(&format!(
        "    <hostname>{}</hostname>\n",
        xml_escape(&svc.hostname)
    ));
    out.push_str(&format!("    <ip>{}</ip>\n", svc.ip));
    out.push_str(&format!("    <port>{}</port>\n", svc.port));
    out.push_str(&format!("    <srcip>{}</srcip>\n", svc.ip));
    out.push_str(&format!("    <netmask>{}</netmask>\n", svc.netmask));
    out.push_str(&format!("    <srcport>{}</srcport>\n", svc.src_port));
    out.push_str(&format!("    <dstport>{}</dstport>\n", svc.dst_port));
    out.push_str("    <addresses>\n");
    out.push_str("      <address member=\"0\">\n");
    out.push_str("        <valu>\n");
    out.push_str("          <ipaddress>\n");
    out.push_str(&format!(
        "            <hostname>{}</hostname>\n",
        xml_escape(&svc.hostname)
    ));
    out.push_str(&format!("            <ip>{}</ip>\n", svc.ip));
    out.push_str(&format!("            <port>{}</port>\n", svc.port));
    out.push_str(&format!("            <srcip>{}</srcip>\n", svc.ip));
    out.push_str(&format!("            <netmask>{}</netmask>\n", svc.netmask));
    out.push_str(&format!("            <srcport>{}</srcport>\n", svc.src_port));
    out.push_str(&format!("            <dstport>{}</dstport>\n", svc.dst_port));
    out.push_str(&format!(
        "            <serviceid>{}</serviceid>\n",
        svc.service_id
    ));
    out.push_str("          </ipaddress>\n");
    out.push_str("        </valu>\n");
    out.push_str("      </address>\n");
    out.push_str("    </addresses>\n");
    out.push_str("  </instance>\n");
    out
}

/// Renders the redirector reply body as XML.
///
/// The body keeps the legacy `ServerInstanceInfo` elements that already make
/// the client reach the Blaze port (`address` / `valu` / `ipaddress`,
/// `secure`, `msiz`, `trialService`, `defaultDnsAddress`) and adds the
/// registered members plus one `<instance>` record per advertised service,
/// so that FIFA 22 finds the `qosapi` service instead of falling back to a
/// hard-coded EA address.
pub fn render_redirector_xml(config: &RedirectorConfig, services: &[ServiceInstance]) -> String {
    let secure_val = if config.secure { 1 } else { 0 };
    let primary = &services[0];

    let mut out = String::new();
    out.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    out.push_str("<serverinstanceinfo>\n");
    // The legacy address block always points at the Blaze service: that is
    // the address the client uses to reach the Blaze port.
    out.push_str("  <address member=\"0\">\n");
    out.push_str("    <valu>\n");
    out.push_str(&format!(
        "      <hostname>{}</hostname>\n",
        xml_escape(&config.host)
    ));
    out.push_str(&format!("      <ip>{}</ip>\n", config.ip));
    out.push_str(&format!("      <port>{}</port>\n", config.port));
    out.push_str("    </valu>\n");
    out.push_str("    <ipaddress>\n");
    out.push_str(&format!(
        "      <hostname>{}</hostname>\n",
        xml_escape(&config.host)
    ));
    out.push_str(&format!("      <ip>{}</ip>\n", config.ip));
    out.push_str(&format!("      <port>{}</port>\n", config.port));
    out.push_str("    </ipaddress>\n");
    out.push_str("  </address>\n");
    out.push_str(&format!("  <secure>{}</secure>\n", secure_val));
    out.push_str("  <msiz>131072</msiz>\n");
    out.push_str("  <trialService>0</trialService>\n");
    out.push_str("  <defaultDnsAddress>0</defaultDnsAddress>\n");
    out.push_str(&format!("  <type>{}</type>\n", primary.instance_type));
    out.push_str(&format!(
        "  <servicename>{}</servicename>\n",
        xml_escape(&primary.service_name)
    ));
    out.push_str(&format!(
        "  <servicenames>{}</servicenames>\n",
        primary
            .service_names
            .iter()
            .map(|n| xml_escape(n))
            .collect::<Vec<_>>()
            .join(",")
    ));
    for svc in services {
        out.push_str(&render_service_instance_xml(svc));
    }
    out.push_str("</serverinstanceinfo>");
    out
}

/// Renders the redirector reply body as JSON, mirroring the XML body.
pub fn render_redirector_json(config: &RedirectorConfig, services: &[ServiceInstance]) -> String {
    let secure_val = if config.secure { 1 } else { 0 };
    let primary = &services[0];

    let mut out = String::new();
    out.push('{');
    // The legacy address object is kept as-is.
    out.push_str(&format!(
        "\"address\":{{\"ipAddress\":{{\"hostname\":\"{}\",\"ip\":{},\"port\":{}}}}},",
        config.host, config.ip, config.port
    ));
    out.push_str(&format!("\"secure\":{},", secure_val));
    out.push_str("\"msiz\":131072,");
    out.push_str(&format!("\"trialService\":0,"));
    out.push_str("\"defaultDnsAddress\":0,");
    out.push_str(&format!("\"type\":{},", primary.instance_type));
    out.push_str(&format!(
        "\"serviceName\":\"{}\",",
        primary.service_name
    ));
    out.push_str(&format!(
        "\"serviceNames\":[{}],",
        primary
            .service_names
            .iter()
            .map(|n| format!("\"{}\"", n))
            .collect::<Vec<_>>()
            .join(",")
    ));
    out.push_str("\"instance\":");
    out.push_str(&render_service_instance_json(primary));
    out.push_str(",\"instances\":[");
    for (i, svc) in services.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push_str(&render_service_instance_json(svc));
    }
    out.push_str("]}");
    out
}

/// Renders one advertised service instance as JSON.
fn render_service_instance_json(svc: &ServiceInstance) -> String {
    format!(
        "{{\"type\":{},\"serviceName\":\"{}\",\"serviceNames\":[{}],\"serviceId\":{},\"hostname\":\"{}\",\"ip\":{},\"port\":{},\"srcIp\":{},\"netMask\":{},\"srcPort\":{},\"dstPort\":{},\"addresses\":[{{\"hostname\":\"{}\",\"ip\":{},\"port\":{}}}]}}",
        svc.instance_type,
        svc.service_name,
        svc.service_names
            .iter()
            .map(|n| format!("\"{}\"", n))
            .collect::<Vec<_>>()
            .join(","),
        svc.service_id,
        svc.hostname,
        svc.ip,
        svc.port,
        svc.ip,
        svc.netmask,
        svc.src_port,
        svc.dst_port,
        svc.hostname,
        svc.ip,
        svc.port
    )
}

/// Extracts the text of the first `<tag>...</tag>` element in `text`.
fn xml_element_value(text: &str, tag: &str) -> Option<String> {
    let open = format!("<{}>", tag);
    let close = format!("</{}>", tag);
    let start = text.find(&open)? + open.len();
    let end = text[start..].find(&close)? + start;
    Some(text[start..end].trim().to_string())
}

/// The service the client asked the redirector for, if it named one.
///
/// FIFA 22's own request body does not name a service -- the reply has to
/// advertise every service the client may bind.  A request that does name
/// one gets that service advertised first.
fn requested_service(request: &str, config: &RedirectorConfig) -> Option<String> {
    let known = [
        config.blaze_service_name.as_str(),
        config.qos_service_name.as_str(),
    ];

    // Query parameters of the request line: "?service=qosapi".
    let request_line = request.lines().next().unwrap_or_default();
    if let Some(query) = request_line.split_once('?').map(|(_, q)| q) {
        // The query string ends at the first space (the HTTP version).
        let query = query.split_whitespace().next().unwrap_or_default();
        for pair in query.split('&') {
            let Some((key, value)) = pair.split_once('=') else {
                continue;
            };
            if !matches!(key.to_ascii_lowercase().as_str(), "service" | "servicename" | "svc") {
                continue;
            }
            let value = value.trim().to_ascii_lowercase();
            if known.contains(&value.as_str()) {
                return Some(value);
            }
        }
    }

    // Or an element in the request body: "<servicename>qosapi</servicename>".
    for tag in ["servicename", "serviceName"] {
        if let Some(value) = xml_element_value(request, tag) {
            let value = value.to_ascii_lowercase();
            if known.contains(&value.as_str()) {
                return Some(value);
            }
        }
    }

    None
}

/// Runs the Redirector service listener loop.
pub async fn run_redirector_server(
    listener: TcpListener,
    tls_acceptor: TlsAcceptor,
    config: RedirectorConfig,
) {
    let config = Arc::new(config);

    while let Ok((tcp_stream, peer_addr)) = listener.accept().await {
        let acceptor = tls_acceptor.clone();
        let mut cfg = (*config).clone();
        if let Ok(local_addr) = tcp_stream.local_addr() {
            let ip = local_addr.ip();
            match ip {
                std::net::IpAddr::V4(ipv4) => {
                    if !ipv4.is_unspecified() {
                        cfg.host = ipv4.to_string();
                    }
                }
                std::net::IpAddr::V6(ipv6) => {
                    if let Some(ipv4) = ipv6.to_ipv4_mapped() {
                        cfg.host = ipv4.to_string();
                    } else {
                        // Frostbite DirtySDK cannot parse raw IPv6 strings ("::1") in Blaze ADDR union.
                        // Always fallback to standard IPv4 loopback "127.0.0.1" for local loopback connections.
                        cfg.host = "127.0.0.1".to_string();
                    }
                }
            }
        }

        let cfg = Arc::new(cfg);

        tokio::spawn(async move {
            tracing::info!("[Redirector] Connection from {} (redirecting to {})", peer_addr, cfg.host);
            match accept_dual_stream(tcp_stream, &acceptor).await {
                Ok(mut stream) => {
                    if let Err(e) = handle_redirector_connection(&mut stream, &cfg).await {
                        tracing::debug!("[Redirector] Session ended with {}: {:?}", peer_addr, e);
                    }
                }
                Err(e) => {
                    tracing::warn!("[Redirector] Failed to accept stream from {}: {:?}", peer_addr, e);
                }
            }
        });
    }
}

async fn handle_redirector_connection(
    stream: &mut DualStream,
    config: &RedirectorConfig,
) -> std::io::Result<()> {
    // Read the first 4 bytes to distinguish HTTP (POST/GET) from binary Blaze header
    let mut prefix = [0u8; 4];
    stream.read_exact(&mut prefix).await?;

    if &prefix == b"POST" || &prefix == b"GET " {
        handle_http_redirector(stream, &prefix, config).await
    } else {
        // Read initial chunk into buffer
        let mut buffer = Vec::with_capacity(1024);
        buffer.extend_from_slice(&prefix);

        // Read at least 12 more bytes to determine 12-byte vs 16-byte header
        let mut rest_hdr = [0u8; 12];
        stream.read_exact(&mut rest_hdr).await?;
        buffer.extend_from_slice(&rest_hdr);

        // Decide which header generation the client is speaking. A client's
        // first frame is always a request, and for a request the two
        // generations are mutually exclusive. See `blaze_core::framing`.
        let framing = detect_framing(&buffer).ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "could not determine the Blaze framing generation",
            )
        })?;

        let (header, payload_len, header_size, metadata_len) = match framing {
            Framing::Fire2 => {
                // The Fire2 header is 16 bytes; read the rest of it.
                let mut extra = [0u8; 4];
                stream.read_exact(&mut extra).await?;
                buffer.extend_from_slice(&extra);

                let header_slice: [u8; 16] = buffer[0..16].try_into().unwrap();
                let header = BlazeHeader::from_bytes_16(&header_slice)?;
                let metadata_len = header.metadata_size as usize;
                (header, header.length as usize, 16usize, metadata_len)
            }
            Framing::Legacy => {
                let header_slice: [u8; 12] = buffer[0..12].try_into().unwrap();
                let header = BlazeHeader::from_bytes(&header_slice)?;
                (header, header.length as usize, 12usize, 0usize)
            }
        };

        tracing::info!(
            "[Redirector] Parsed header ({:?}): Comp=0x{:04X}, Cmd=0x{:04X}, Seq={}, Len={}, MsgType={:?}",
            framing,
            header.component,
            header.command,
            header.seq_no,
            header.length,
            header.msg_type
        );

        // Read the metadata section (Fire2 only) and then the payload.
        let mut payload_buf = Vec::with_capacity(payload_len);
        if metadata_len > 0 {
            let mut meta = vec![0u8; metadata_len];
            stream.read_exact(&mut meta).await?;
        }

        let already_read = buffer.len() - header_size;
        if already_read > 0 {
            let take = already_read.min(payload_len);
            payload_buf.extend_from_slice(&buffer[header_size..header_size + take]);
        }
        while payload_buf.len() < payload_len {
            let mut chunk = vec![0u8; payload_len - payload_buf.len()];
            stream.read_exact(&mut chunk).await?;
            payload_buf.extend_from_slice(&chunk);
        }

        let payload = if payload_buf.is_empty() {
            blaze_tdf::TdfStruct::new()
        } else {
            deserialize_tdf(&payload_buf).unwrap_or_else(|_| blaze_tdf::TdfStruct::new())
        };

        let req_packet = BlazePacket { header, payload };
        let resp_packet = handle_redirector_packet(&req_packet, config)?;

        let encoded_resp = match framing {
            Framing::Fire2 => resp_packet.encode_16()?,
            Framing::Legacy => resp_packet.encode()?,
        };

        tracing::info!(
            "[Redirector] Sending ServerInstance response to client ({:?}, {} bytes):\n{}",
            framing,
            encoded_resp.len(),
            blaze_core::hex_dump(&encoded_resp)
        );

        stream.write_all(&encoded_resp).await?;
        stream.flush().await?;
        tracing::info!("[Redirector] ServerInstance response sent and flushed successfully.");

        Ok(())
    }
}

async fn handle_http_redirector(
    stream: &mut DualStream,
    prefix: &[u8; 4],
    config: &RedirectorConfig,
) -> std::io::Result<()> {
    let mut req_bytes = Vec::with_capacity(4096);
    req_bytes.extend_from_slice(prefix);

    let mut buf = [0u8; 1024];
    let mut body_start = 0;
    let mut content_length = 0usize;

    // Read until header boundary "\r\n\r\n" or 16KB limit
    loop {
        if let Some(pos) = req_bytes.windows(4).position(|w| w == b"\r\n\r\n") {
            body_start = pos + 4;
            let headers_str = String::from_utf8_lossy(&req_bytes[..pos]);
            for line in headers_str.lines() {
                if line.to_ascii_lowercase().starts_with("content-length:") {
                    if let Some(val) = line.split(':').nth(1) {
                        content_length = val.trim().parse::<usize>().unwrap_or(0);
                    }
                }
            }
            break;
        }

        let n = stream.read(&mut buf).await?;
        if n == 0 {
            break;
        }
        req_bytes.extend_from_slice(&buf[..n]);
        if req_bytes.len() > 16384 {
            break;
        }
    }

    // Read remaining body bytes if Content-Length was specified
    let total_needed = body_start + content_length;
    while req_bytes.len() < total_needed {
        let n = stream.read(&mut buf).await?;
        if n == 0 {
            break;
        }
        req_bytes.extend_from_slice(&buf[..n]);
    }

    let req_str = String::from_utf8_lossy(&req_bytes);
    tracing::info!("[Redirector HTTP] Request received:\n{}", req_str);

    let is_json = req_str.to_ascii_lowercase().contains("application/json");

    // When config.secure is false, downgrade to Plain TCP (0) to allow clean certificate-free socket flow
    let secure_val = if config.secure { 1 } else { 0 };

    // The service instances the reply advertises.  The Blaze service comes
    // first unless the client asked for the QoS service by name.
    let requested = requested_service(&req_str, config);
    let services = config.advertised_services_for(requested.as_deref());

    let response_body = if is_json {
        render_redirector_json(config, &services)
    } else {
        render_redirector_xml(config, &services)
    };

    let content_type = if is_json {
        "application/json; charset=utf-8"
    } else {
        "application/xml; charset=utf-8"
    };

    let http_response = format!(
        "HTTP/1.1 200 OK\r\n\
Content-Type: {}\r\n\
Content-Length: {}\r\n\
Connection: close\r\n\
X-BLAZE-ERRORCODE: 0\r\n\
X-BLAZE-COMPONENT: redirector\r\n\
X-BLAZE-COMMAND: getServerInstance\r\n\
X-BLAZE-SEQNO: 0\r\n\r\n{}",
        content_type,
        response_body.len(),
        response_body
    );

    tracing::info!(
        "[Redirector HTTP] Responded 200 OK: redirecting client to {}:{} (Format={}, Secure={}), advertised services: {}",
        config.host,
        config.port,
        if is_json { "JSON" } else { "XML" },
        secure_val,
        services
            .iter()
            .map(|s| format!("{}={}:{}", s.service_name, s.hostname, s.port))
            .collect::<Vec<_>>()
            .join(", ")
    );

    stream.write_all(http_response.as_bytes()).await?;
    stream.flush().await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn default_config() -> RedirectorConfig {
        RedirectorConfig {
            host: "127.0.0.1".to_string(),
            port: 42128,
            secure: true,
            ..Default::default()
        }
    }

    /// The reply has to advertise the `qosapi` service with the EA host
    /// name, the loopback address in EA's u32 form and port 443.
    #[test]
    fn test_redirector_xml_advertises_the_qosapi_service() {
        let config = default_config();
        let services = config.advertised_services();
        let body = render_redirector_xml(&config, &services);

        assert!(body.contains("<servicename>qosapi</servicename>"), "{}", body);
        assert!(body.contains("<servicenames>qosapi</servicenames>"), "{}", body);
        assert!(
            body.contains("<hostname>spring18.gosredirector.ea.com</hostname>"),
            "{}",
            body
        );
        assert!(body.contains("<ip>2130706433</ip>"), "{}", body);
        assert!(body.contains("<port>443</port>"), "{}", body);
        assert!(body.contains("<srcport>443</srcport>"), "{}", body);
        assert!(body.contains("<dstport>443</dstport>"), "{}", body);
        assert!(body.contains("<serviceid>2</serviceid>"), "{}", body);
    }

    /// The Blaze service must still be advertised, and the legacy elements
    /// that make the client reach the Blaze port must not change.
    #[test]
    fn test_redirector_xml_still_advertises_the_blaze_service() {
        let config = default_config();
        let services = config.advertised_services();
        let body = render_redirector_xml(&config, &services);

        assert!(body.contains("<servicename>blaze</servicename>"), "{}", body);
        assert!(body.contains("<port>42128</port>"), "{}", body);

        // The legacy ServerInstanceInfo elements.
        assert!(body.contains("<address member=\"0\">"), "{}", body);
        assert!(body.contains("<valu>"), "{}", body);
        assert!(body.contains("<ipaddress>"), "{}", body);
        assert!(body.contains("<secure>1</secure>"), "{}", body);
        assert!(body.contains("<msiz>131072</msiz>"), "{}", body);
        assert!(body.contains("<trialService>0</trialService>"), "{}", body);
        assert!(
            body.contains("<defaultDnsAddress>0</defaultDnsAddress>"),
            "{}",
            body
        );

        // The Blaze address still points at the Blaze port.
        let blaze = &services[0];
        assert_eq!(blaze.hostname, "127.0.0.1");
        assert_eq!(blaze.port, 42128);
    }

    /// The JSON body mirrors the XML body.
    #[test]
    fn test_redirector_json_advertises_the_qosapi_service() {
        let config = default_config();
        let services = config.advertised_services();
        let body = render_redirector_json(&config, &services);

        assert!(body.contains("\"serviceName\":\"qosapi\""), "{}", body);
        assert!(
            body.contains("\"hostname\":\"spring18.gosredirector.ea.com\""),
            "{}",
            body
        );
        assert!(body.contains("\"ip\":2130706433"), "{}", body);
        assert!(body.contains("\"port\":443"), "{}", body);
        assert!(body.contains("\"serviceName\":\"blaze\""), "{}", body);
        assert!(body.contains("\"port\":42128"), "{}", body);
        // The legacy address object is still there.
        assert!(
            body.contains("\"address\":{\"ipAddress\":{\"hostname\":\"127.0.0.1\",\"ip\":2130706433,\"port\":42128}}"),
            "{}",
            body
        );
    }

    /// A request that names a service gets that service advertised first.
    #[test]
    fn test_requested_service_is_taken_from_the_request() {
        let config = default_config();

        // No service named -> the default order.
        assert_eq!(requested_service("POST /redirector/getServerInstance HTTP/1.1", &config), None);

        // From the query string.
        assert_eq!(
            requested_service("POST /redirector/getServerInstance?service=qosapi HTTP/1.1", &config),
            Some("qosapi".to_string())
        );

        // From the request body.
        let body_request = "POST /redirector/getServerInstance HTTP/1.1\r\n\r\n\
            <?xml version=\"1.0\"?>\n<serverinstancerequest>\n  \
            <servicename>qosapi</servicename>\n</serverinstancerequest>";
        assert_eq!(
            requested_service(body_request, &config),
            Some("qosapi".to_string())
        );

        // An unknown service name is ignored.
        assert_eq!(
            requested_service("POST /redirector/getServerInstance?service=nope HTTP/1.1", &config),
            None
        );
    }
}
