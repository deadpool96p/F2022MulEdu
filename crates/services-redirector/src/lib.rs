//! # services-redirector
//!
//! EA Blaze Redirector Service implementation (`gosredirector.ea.com:42127`).
//! Handles Component 0x0005 (Redirector) and Command 0x0001 (getServerInstance).

use std::io;
use blaze_core::BlazePacket;
use blaze_tdf::{TdfList, TdfStruct, TdfType, TdfUnion, TdfValue};

pub const COMPONENT_REDIRECTOR: u16 = 0x0005;
pub const COMMAND_GET_SERVER_INSTANCE: u16 = 0x0001;

/// 127.0.0.1 in EA's big-endian u32 form (`blaze_tdf::ipv4_to_u32`).
pub const LOOPBACK_IP_U32: u32 = 2_130_706_433;

/// The service name the Blaze server is advertised under.
pub const BLAZE_SERVICE_NAME: &str = "blaze";

/// The service name the QoS (ProtoPing HTTP) service is advertised under.
///
/// The client binds the QoS service out of the redirector reply; when the
/// reply does not advertise it, the client falls back to a hard-coded EA
/// address (26.178.140.32:443) and issues the QoS request with an empty TLS
/// SNI, which fails the handshake.
pub const QOS_SERVICE_NAME: &str = "qosapi";

/// The default host name the QoS service is advertised under.
///
/// This has to be a *name*, not an IP literal: the client turns the
/// advertised host into the TLS SNI, and an IP literal makes it send a
/// zero-length `server_name`, which rustls rejects.
///
/// It is a `gosredirector` name and **not** one of EA's `qos-prod-*.gos.ea.com`
/// names on purpose: the leaf certificate that is currently on disk
/// (`f22_server.crt`) only carries the `gosredirector.ea.com` SANs, and
/// `*.ea.com` does not match a multi-label name such as
/// `qos-prod-bio-dub-common-common.gos.ea.com`.  A name the certificate does
/// not cover would fail the TLS handshake on the QoS connection.
///
/// The host is only used by the client to build the URL and the SNI -- the
/// emulator's `:443` sink answers the QoS paths regardless of the `Host`
/// header -- and the shim's `getaddrinfo` hook keeps the name local.
pub const DEFAULT_QOS_HOSTNAME: &str = "spring18.gosredirector.ea.com";

/// The default port the QoS HTTP service is advertised under.
pub const DEFAULT_QOS_PORT: u16 = 443;

/// A single service instance advertised in the redirector reply.
///
/// This is the registered Blaze Redirector `ServerInstanceInfo` shape
/// (FIFA 22 generation): a named service plus the address it is reached at.
#[derive(Debug, Clone, PartialEq)]
pub struct ServiceInstance {
    /// The name the client binds this instance to (`"blaze"`, `"qosapi"`).
    pub service_name: String,
    /// Every service name this instance answers to.
    pub service_names: Vec<String>,
    /// The instance type (`0` = the default instance).
    pub instance_type: i64,
    /// The host name the client should connect to.
    pub hostname: String,
    /// The IPv4 address in EA's u32 form (127.0.0.1 == 2130706433).
    pub ip: u32,
    /// The port the service listens on.
    pub port: u16,
    /// The source port of the address entry.
    pub src_port: u16,
    /// The destination port of the address entry.
    pub dst_port: u16,
    /// The network mask of the address entry.
    pub netmask: u32,
    /// The numeric service id.
    pub service_id: u32,
}

/// Configuration for redirecting clients to the target Blaze cluster.
#[derive(Debug, Clone)]
pub struct RedirectorConfig {
    pub host: String,
    pub port: u16,
    pub secure: bool,
    /// The IPv4 address the services are advertised with, in EA's u32 form.
    pub ip: u32,
    /// The service name the Blaze server is advertised under.
    pub blaze_service_name: String,
    /// The service name the QoS service is advertised under.
    pub qos_service_name: String,
    /// The host name the QoS service is advertised under. Must be a name so
    /// that the client's TLS SNI is not empty.
    pub qos_hostname: String,
    /// The port the QoS HTTP service is advertised under.
    pub qos_port: u16,
}

impl Default for RedirectorConfig {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".to_string(),
            port: 42128,
            secure: true, // 1 = TLS (satisfies standardSecure_v4 profile)
            ip: LOOPBACK_IP_U32,
            blaze_service_name: BLAZE_SERVICE_NAME.to_string(),
            qos_service_name: QOS_SERVICE_NAME.to_string(),
            qos_hostname: DEFAULT_QOS_HOSTNAME.to_string(),
            qos_port: DEFAULT_QOS_PORT,
        }
    }
}

impl RedirectorConfig {
    /// The service instances advertised in the redirector reply.
    ///
    /// The reply always advertises *both* the Blaze service and the QoS
    /// service: FIFA 22 binds the QoS (`qosapi`) service out of the
    /// redirector reply, and falls back to a hard-coded EA address when it
    /// is not advertised.
    pub fn advertised_services(&self) -> Vec<ServiceInstance> {
        self.advertised_services_for(None)
    }

    /// The advertised service instances, with `preferred` moved to the front.
    ///
    /// The client may ask the redirector for one specific service; the
    /// instance that answers to that name is advertised first so that a
    /// client reading the first advertised instance gets the one it asked
    /// for.
    pub fn advertised_services_for(&self, preferred: Option<&str>) -> Vec<ServiceInstance> {
        let blaze = ServiceInstance {
            service_name: self.blaze_service_name.clone(),
            service_names: vec![self.blaze_service_name.clone()],
            instance_type: 0,
            hostname: self.host.clone(),
            ip: self.ip,
            port: self.port,
            src_port: self.port,
            dst_port: self.port,
            netmask: u32::MAX,
            service_id: 1,
        };
        let qos = ServiceInstance {
            service_name: self.qos_service_name.clone(),
            service_names: vec![self.qos_service_name.clone()],
            instance_type: 0,
            hostname: self.qos_hostname.clone(),
            ip: self.ip,
            port: self.qos_port,
            src_port: self.qos_port,
            dst_port: self.qos_port,
            netmask: u32::MAX,
            service_id: 2,
        };

        match preferred {
            Some(name) if name == qos.service_name => vec![qos, blaze],
            Some(name) if name == blaze.service_name => vec![blaze, qos],
            _ => vec![blaze, qos],
        }
    }
}

/// Metadata extracted from a client's getServerInstance request.
#[derive(Debug, Clone, Default)]
pub struct ClientMetadata {
    pub name: String,
    pub client_name: String,
    pub blaze_sdk_version: String,
    pub platform: String,
    pub locale: u64,
}

impl ClientMetadata {
    pub fn from_tdf(payload: &TdfStruct) -> Self {
        let name = match payload.get("NAME") {
            Some(TdfValue::String(s)) => s.clone(),
            _ => "fifa-2022-pc".to_string(),
        };

        let client_name = match payload.get("CLNT") {
            Some(TdfValue::String(s)) => s.clone(),
            _ => "FIFA22".to_string(),
        };

        let blaze_sdk_version = match payload.get("BSDK") {
            Some(TdfValue::String(s)) => s.clone(),
            _ => "unknown".to_string(),
        };

        let platform = match payload.get("PLAT") {
            Some(TdfValue::String(s)) => s.clone(),
            _ => "PC".to_string(),
        };

        let locale = match payload.get("LOC") {
            Some(TdfValue::Integer(v)) => *v,
            _ => 0,
        };

        Self {
            name,
            client_name,
            blaze_sdk_version,
            platform,
            locale,
        }
    }
}

/// Dispatches an incoming Blaze packet for the Redirector component.
pub fn handle_redirector_packet(
    req: &BlazePacket,
    config: &RedirectorConfig,
) -> io::Result<BlazePacket> {
    if req.header.component != COMPONENT_REDIRECTOR {
        tracing::warn!(
            "Redirector received unexpected component 0x{:04X}",
            req.header.component
        );
        return BlazePacket::error_response(&req.header, 0x4001);
    }

    match req.header.command {
        COMMAND_GET_SERVER_INSTANCE => handle_get_server_instance(req, config),
        cmd => {
            tracing::info!("Redirector received unknown command 0x{:04X} -> defaulting to ServerInstance redirection", cmd);
            handle_get_server_instance(req, config)
        }
    }
}

/// Handles Command 0x0001 (getServerInstance).
pub fn handle_get_server_instance(
    req: &BlazePacket,
    config: &RedirectorConfig,
) -> io::Result<BlazePacket> {
    let client_meta = ClientMetadata::from_tdf(&req.payload);
    tracing::info!(
        "[Redirector] Client connected: Name='{}', Client='{}', SDK='{}', Platform='{}', Locale={}",
        client_meta.name,
        client_meta.client_name,
        client_meta.blaze_sdk_version,
        client_meta.platform,
        client_meta.locale
    );

    // Build ServerInstanceInfo TDF response
    let mut resp_tdf = TdfStruct::new();

    // ADDR: the `ServerAddress` union -- case 0, member tag "VALU", holding
    // an `IpAddress` struct. The byte-exact authentic EA
    // `Redirector::getServerInstance` reply has exactly this shape:
    // case 0, tag "VALU", struct { HOST, IP, PORT }.
    let host_str = if config.host == "::1" || config.host == "::" || config.host == "0.0.0.0" {
        "127.0.0.1".to_string()
    } else {
        config.host.clone()
    };
    let ip_numeric = blaze_tdf::ipv4_to_u32(&host_str).unwrap_or(0);
    let mut ip_addr_struct = TdfStruct::new();
    ip_addr_struct.set("HOST", TdfValue::String(host_str));
    ip_addr_struct.set("IP", TdfValue::Integer(ip_numeric as u64));
    ip_addr_struct.set("PORT", TdfValue::Integer(config.port as u64));

    resp_tdf.set(
        "ADDR",
        TdfValue::Union(TdfUnion::with_member(
            0,
            TdfUnion::DEFAULT_MEMBER_TAG,
            TdfValue::Struct(ip_addr_struct),
        )),
    );

    // SECU: 0 = Plain TCP (downgrade), 1 = TLS/SSL
    let secure_val = if config.secure { 1 } else { 0 };
    resp_tdf.set("SECU", TdfValue::Integer(secure_val));

    // XREF: 0
    resp_tdf.set("XREF", TdfValue::Integer(0));

    // MSIZ: Maximum frame size (131072 bytes)
    resp_tdf.set("MSIZ", TdfValue::Integer(131072));

    // AMNT: Target instance name
    resp_tdf.set("AMNT", TdfValue::String(client_meta.name));

    // The registered FIFA 22 Redirector shape: one `ServerInstanceInfo` per
    // advertised service.  FIFA 22 binds the `qosapi` service out of this
    // reply; when it is missing, the client falls back to a hard-coded EA
    // address and issues the QoS request with an empty TLS SNI.
    let services = config.advertised_services();
    resp_tdf.set(
        "INST",
        TdfValue::Struct(service_instance_to_tdf(&services[0])),
    );
    resp_tdf.set(
        "INSTS",
        TdfValue::List(TdfList::new(
            TdfType::Struct,
            services
                .iter()
                .map(|svc| TdfValue::Struct(service_instance_to_tdf(svc)))
                .collect(),
        )),
    );

    tracing::info!(
        "[Redirector] Redirecting client to {}:{} (Secure={}), advertising {} service(s): {}",
        config.host,
        config.port,
        config.secure,
        services.len(),
        services
            .iter()
            .map(|s| format!("{}={}:{}", s.service_name, s.hostname, s.port))
            .collect::<Vec<_>>()
            .join(", ")
    );

    BlazePacket::response(&req.header, resp_tdf)
}

/// Encodes an advertised service instance as the registered
/// `ServerInstanceInfo` / `ServerInstance` member set.
///
/// The member tags are the 4-character Blaze tags of the registered
/// Redirector vocabulary; the names they carry are the registered member
/// names (`serviceName`, `serviceNames`, `srcIp`, `netMask`, `srcPort`,
/// `dstPort`, `serviceId`, `hostname`, `addresses`).
fn service_instance_to_tdf(svc: &ServiceInstance) -> TdfStruct {
    let mut instance = TdfStruct::new();
    instance.set("TYPE", TdfValue::Integer(svc.instance_type as u64));
    instance.set("SNAM", TdfValue::String(svc.service_name.clone()));
    instance.set(
        "SNMS",
        TdfValue::List(TdfList::new(
            TdfType::String,
            svc.service_names
                .iter()
                .cloned()
                .map(TdfValue::String)
                .collect(),
        )),
    );
    instance.set("SVID", TdfValue::Integer(svc.service_id as u64));
    instance.set("HOST", TdfValue::String(svc.hostname.clone()));
    instance.set("IP", TdfValue::Integer(svc.ip as u64));
    instance.set("PORT", TdfValue::Integer(svc.port as u64));
    instance.set("SRIP", TdfValue::Integer(svc.ip as u64));
    instance.set("NMAS", TdfValue::Integer(svc.netmask as u64));
    instance.set("SRPT", TdfValue::Integer(svc.src_port as u64));
    instance.set("DSPT", TdfValue::Integer(svc.dst_port as u64));

    // addresses: the ServerAddressInfo list -- hostname / ip / port.
    let mut address = TdfStruct::new();
    address.set("HOST", TdfValue::String(svc.hostname.clone()));
    address.set("IP", TdfValue::Integer(svc.ip as u64));
    address.set("PORT", TdfValue::Integer(svc.port as u64));
    instance.set(
        "AS",
        TdfValue::List(TdfList::new(TdfType::Struct, vec![TdfValue::Struct(address)])),
    );

    instance
}

#[cfg(test)]
mod tests {
    use super::*;
    use blaze_core::{BlazeHeader, MessageType};

    #[test]
    fn test_redirector_plain_tcp_response() {
        let mut req_payload = TdfStruct::new();
        req_payload.set("NAME", TdfValue::String("fifa-2022-pc".to_string()));
        req_payload.set("CLNT", TdfValue::String("FIFA22".to_string()));
        req_payload.set("BSDK", TdfValue::String("15.1.1.2.0".to_string()));
        req_payload.set("PLAT", TdfValue::String("PC".to_string()));

        let req = BlazePacket::new(
            COMPONENT_REDIRECTOR,
            COMMAND_GET_SERVER_INSTANCE,
            0,
            MessageType::SubmitRequest,
            0,
            1,
            req_payload,
        )
        .expect("Failed to build request packet");

        let config = RedirectorConfig {
            host: "127.0.0.1".to_string(),
            port: 42128,
            secure: false,
            ..Default::default()
        };

        let response =
            handle_redirector_packet(&req, &config).expect("Redirector handling failed");

        assert_eq!(response.header.component, COMPONENT_REDIRECTOR);
        assert_eq!(response.header.command, COMMAND_GET_SERVER_INSTANCE);
        assert_eq!(response.header.error_code, 0);
        assert_eq!(response.header.msg_type, MessageType::RecordResponse);
        assert_eq!(response.header.seq_no, 1);

        // Verify payload TDF
        assert_eq!(response.payload.get("SECU"), Some(&TdfValue::Integer(0)));
        assert_eq!(
            response.payload.get("AMNT"),
            Some(&TdfValue::String("fifa-2022-pc".to_string()))
        );

        if let Some(TdfValue::Union(u)) = response.payload.get("ADDR") {
            assert_eq!(u.active_tag, 0x00);
            if let Some(box_val) = &u.value {
                if let TdfValue::Struct(s) = box_val.as_ref() {
                    assert_eq!(
                        s.get("HOST"),
                        Some(&TdfValue::String("127.0.0.1".to_string()))
                    );
                    assert_eq!(s.get("PORT"), Some(&TdfValue::Integer(42128)));
                } else {
                    panic!("Expected Struct inside ADDR union");
                }
            } else {
                panic!("Expected value in ADDR union");
            }
        } else {
            panic!("Expected ADDR union");
        }
    }

    #[test]
    fn test_redirector_secure_response() {
        let req_header = BlazeHeader::new(
            0,
            COMPONENT_REDIRECTOR,
            COMMAND_GET_SERVER_INSTANCE,
            0,
            MessageType::SubmitRequest,
            0,
            5,
        );
        let req = BlazePacket::new(
            req_header.component,
            req_header.command,
            req_header.error_code,
            req_header.msg_type,
            req_header.qos,
            req_header.seq_no,
            TdfStruct::new(),
        )
        .unwrap();

        let config = RedirectorConfig {
            host: "192.168.1.100".to_string(),
            port: 42128,
            secure: true,
            ..Default::default()
        };

        let response = handle_redirector_packet(&req, &config).unwrap();
        assert_eq!(response.payload.get("SECU"), Some(&TdfValue::Integer(1)));
    }

    /// The redirector has to advertise *both* the Blaze service and the QoS
    /// (`qosapi`) service.  FIFA 22 binds the QoS service out of the
    /// redirector reply and falls back to a hard-coded EA address when it is
    /// not advertised.
    #[test]
    fn test_advertised_services_contain_blaze_and_qosapi() {
        let config = RedirectorConfig::default();
        let services = config.advertised_services();

        assert_eq!(services.len(), 2);

        let blaze = &services[0];
        assert_eq!(blaze.service_name, "blaze");
        assert_eq!(blaze.service_names, vec!["blaze".to_string()]);
        assert_eq!(blaze.hostname, "127.0.0.1");
        assert_eq!(blaze.ip, LOOPBACK_IP_U32);
        assert_eq!(blaze.ip, 2_130_706_433);
        assert_eq!(blaze.port, 42128);

        let qos = &services[1];
        assert_eq!(qos.service_name, "qosapi");
        assert_eq!(qos.service_names, vec!["qosapi".to_string()]);
        // A *name*, never an IP literal: the client turns it into the TLS
        // SNI, and an IP literal would make it send an empty SNI.  It has to
        // be a name the on-disk leaf certificate covers.
        assert_eq!(qos.hostname, "spring18.gosredirector.ea.com");
        assert!(!qos.hostname.contains("127.0.0.1"));
        assert_eq!(qos.ip, 2_130_706_433);
        assert_eq!(qos.port, 443);
    }

    /// The preferred service is advertised first, so a client that asked the
    /// redirector for one specific service reads that one.
    #[test]
    fn test_advertised_services_for_prefers_the_requested_service() {
        let config = RedirectorConfig::default();

        let services = config.advertised_services_for(Some("qosapi"));
        assert_eq!(services[0].service_name, "qosapi");
        assert_eq!(services[1].service_name, "blaze");

        let services = config.advertised_services_for(Some("blaze"));
        assert_eq!(services[0].service_name, "blaze");

        // An unknown name keeps the default order.
        let services = config.advertised_services_for(Some("nope"));
        assert_eq!(services[0].service_name, "blaze");
        assert_eq!(services[1].service_name, "qosapi");
    }

    /// The TDF reply carries the same two service instances.
    #[test]
    fn test_redirector_tdf_reply_advertises_qosapi() {
        let req = BlazePacket::new(
            COMPONENT_REDIRECTOR,
            COMMAND_GET_SERVER_INSTANCE,
            0,
            MessageType::SubmitRequest,
            0,
            1,
            TdfStruct::new(),
        )
        .unwrap();

        let config = RedirectorConfig::default();
        let response = handle_redirector_packet(&req, &config).unwrap();

        let instances = match response.payload.get("INSTS") {
            Some(TdfValue::List(list)) => list,
            other => panic!("Expected INSTS list in the reply, got {:?}", other),
        };
        assert_eq!(instances.items.len(), 2);

        // The Blaze instance is still advertised first.
        let blaze = match &instances.items[0] {
            TdfValue::Struct(s) => s,
            other => panic!("Expected a struct instance, got {:?}", other),
        };
        assert_eq!(
            blaze.get("SNAM"),
            Some(&TdfValue::String("blaze".to_string()))
        );
        assert_eq!(blaze.get("PORT"), Some(&TdfValue::Integer(42128)));

        // ... and the QoS instance carries the name, host, ip and port the
        // client needs to reach the emulator's QoS HTTP endpoints.
        let qos = match &instances.items[1] {
            TdfValue::Struct(s) => s,
            other => panic!("Expected a struct instance, got {:?}", other),
        };
        assert_eq!(
            qos.get("SNAM"),
            Some(&TdfValue::String("qosapi".to_string()))
        );
        assert_eq!(
            qos.get("HOST"),
            Some(&TdfValue::String("spring18.gosredirector.ea.com".to_string()))
        );
        assert_eq!(qos.get("IP"), Some(&TdfValue::Integer(2_130_706_433)));
        assert_eq!(qos.get("PORT"), Some(&TdfValue::Integer(443)));
        assert_eq!(qos.get("SRIP"), Some(&TdfValue::Integer(2_130_706_433)));
        assert_eq!(qos.get("SRPT"), Some(&TdfValue::Integer(443)));
        assert_eq!(qos.get("DSPT"), Some(&TdfValue::Integer(443)));
        assert_eq!(qos.get("SVID"), Some(&TdfValue::Integer(2)));
    }
}
