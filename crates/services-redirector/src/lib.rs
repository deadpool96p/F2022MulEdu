//! # services-redirector
//!
//! EA Blaze Redirector Service implementation (`gosredirector.ea.com:42127`).
//! Handles Component 0x0005 (Redirector) and Command 0x0001 (getServerInstance).

use std::io;
use blaze_core::BlazePacket;
use blaze_tdf::{TdfStruct, TdfUnion, TdfValue};

pub const COMPONENT_REDIRECTOR: u16 = 0x0005;
pub const COMMAND_GET_SERVER_INSTANCE: u16 = 0x0001;

/// Configuration for redirecting clients to the target Blaze cluster.
#[derive(Debug, Clone)]
pub struct RedirectorConfig {
    pub host: String,
    pub port: u16,
    pub secure: bool,
}

impl Default for RedirectorConfig {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".to_string(),
            port: 42128,
            secure: true, // 1 = TLS (satisfies standardSecure_v4 profile)
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

    tracing::info!(
        "[Redirector] Redirecting client to {}:{} (Secure={})",
        config.host,
        config.port,
        config.secure
    );

    BlazePacket::response(&req.header, resp_tdf)
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
        };

        let response = handle_redirector_packet(&req, &config).unwrap();
        assert_eq!(response.payload.get("SECU"), Some(&TdfValue::Integer(1)));
    }
}
