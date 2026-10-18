//! # services-blaze
//!
//! Core Blaze RPC services handling Handshake (Util), Identity (Authentication),
//! and Network Registration (UserSessions).

pub mod auth;
pub mod router;
pub mod session;
pub mod usersessions;
pub mod util;

pub use auth::*;
pub use router::BlazeRouter;
pub use session::SessionState;
pub use usersessions::*;
pub use util::*;

#[cfg(test)]
mod tests {
    use super::*;
    use blaze_core::{BlazePacket, MessageType};
    use blaze_tdf::{TdfStruct, TdfUnion, TdfValue};

    #[test]
    fn test_router_handshake_flow() {
        let mut router = BlazeRouter::new("127.0.0.1".to_string(), 17498);

        // 1. Test preAuth
        let pre_auth_req = BlazePacket::new(
            COMPONENT_UTIL,
            CMD_PRE_AUTH,
            0,
            MessageType::SubmitRequest,
            0,
            1,
            TdfStruct::new(),
        )
        .unwrap();

        let (pre_auth_resp, notif) = router.handle_packet(&pre_auth_req).unwrap();
        assert!(notif.is_none());
        assert_eq!(pre_auth_resp.header.component, COMPONENT_UTIL);
        assert_eq!(pre_auth_resp.header.command, CMD_PRE_AUTH);
        assert_eq!(pre_auth_resp.header.seq_no, 1);
        assert!(pre_auth_resp.payload.get("QOSS").is_some());
        assert!(pre_auth_resp.payload.get("CIDS").is_some());

        // 2. Test expressLogin
        let mut login_req_payload = TdfStruct::new();
        login_req_payload.set("AUTH", TdfValue::String("MOCK_ORIGIN_TOKEN".to_string()));
        let login_req = BlazePacket::new(
            COMPONENT_AUTH,
            CMD_EXPRESS_LOGIN,
            0,
            MessageType::SubmitRequest,
            0,
            2,
            login_req_payload,
        )
        .unwrap();

        let (login_resp, _) = router.handle_packet(&login_req).unwrap();
        assert_eq!(login_resp.header.component, COMPONENT_AUTH);
        assert_eq!(login_resp.header.command, CMD_EXPRESS_LOGIN);
        assert_eq!(login_resp.header.seq_no, 2);
        assert_eq!(
            login_resp.payload.get("DSNM"),
            Some(&TdfValue::String("F22_Player1".to_string()))
        );
        assert!(login_resp.payload.get("SKEY").is_some());
        assert!(router.session.is_logged_in);

        // 3. Test postAuth
        let post_auth_req = BlazePacket::new(
            COMPONENT_UTIL,
            CMD_POST_AUTH,
            0,
            MessageType::SubmitRequest,
            0,
            3,
            TdfStruct::new(),
        )
        .unwrap();

        let (post_auth_resp, _) = router.handle_packet(&post_auth_req).unwrap();
        assert_eq!(post_auth_resp.header.command, CMD_POST_AUTH);
        assert!(post_auth_resp.payload.get("TELE").is_some());

        // 4. Test ping
        let ping_req = BlazePacket::new(
            COMPONENT_UTIL,
            CMD_PING,
            0,
            MessageType::SubmitRequest,
            0,
            4,
            TdfStruct::new(),
        )
        .unwrap();

        let (ping_resp, _) = router.handle_packet(&ping_req).unwrap();
        assert_eq!(ping_resp.header.command, CMD_PING);
        assert!(ping_resp.payload.get("STIM").is_some());

        // 5. Test updateNetworkInfo (generates response + async notification)
        let mut net_req_payload = TdfStruct::new();
        let mut addr_struct = TdfStruct::new();
        addr_struct.set("PORT", TdfValue::Integer(3659));
        net_req_payload.set(
            "ADDR",
            TdfValue::Union(TdfUnion::with_value(0x00, TdfValue::Struct(addr_struct))),
        );
        net_req_payload.set("NATT", TdfValue::Integer(0)); // Open NAT

        let net_req = BlazePacket::new(
            COMPONENT_USER_SESSIONS,
            CMD_UPDATE_NETWORK_INFO,
            0,
            MessageType::SubmitRequest,
            0,
            5,
            net_req_payload,
        )
        .unwrap();

        let (net_resp, net_notif) = router.handle_packet(&net_req).unwrap();
        assert_eq!(net_resp.header.command, CMD_UPDATE_NETWORK_INFO);
        assert_eq!(router.session.port, 3659);
        assert_eq!(router.session.nat_type, 0);

        let notif_pkt = net_notif.expect("Expected async notification on updateNetworkInfo");
        assert_eq!(notif_pkt.header.msg_type, MessageType::Notification);
        assert_eq!(
            notif_pkt.header.component,
            COMPONENT_USER_SESSIONS
        );
        assert_eq!(
            notif_pkt.header.command,
            NOTIFY_USER_SESSION_EXTENDED_DATA
        );
    }
}
