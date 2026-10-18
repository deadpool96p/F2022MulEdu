//! # services-blaze
//!
//! Core Blaze RPC services handling Handshake (Util), Identity (Authentication),
//! Network Registration & Lookups (UserSessions), Virtual Friends (AssociationLists),
//! and Game Session / Lobby Orchestration (GameManager).

pub mod associations;
pub mod auth;
pub mod gamemanager;
pub mod router;
pub mod session;
pub mod session_store;
pub mod usersessions;
pub mod util;

pub use associations::*;
pub use auth::*;
pub use gamemanager::*;
pub use router::BlazeRouter;
pub use session::SessionState;
pub use session_store::{SessionData, SessionStore, SteamFriend, SteamProfile};
pub use usersessions::*;
pub use util::*;

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use blaze_core::{BlazePacket, MessageType};
    use blaze_tdf::{TdfStruct, TdfUnion, TdfValue};

    #[test]
    fn test_router_handshake_and_multiuser_flow() {
        let store = Arc::new(SessionStore::new());
        let games = Arc::new(GameRegistry::new());
        let mut router1 = BlazeRouter::new("127.0.0.1".to_string(), 17498, store.clone(), games.clone());

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

        let (pre_auth_resp, notif) = router1.handle_packet(&pre_auth_req).unwrap();
        assert!(notif.is_none());
        assert_eq!(pre_auth_resp.header.component, COMPONENT_UTIL);
        assert_eq!(pre_auth_resp.header.command, CMD_PRE_AUTH);
        assert_eq!(pre_auth_resp.header.seq_no, 1);
        assert!(pre_auth_resp.payload.get("QOSS").is_some());
        assert!(pre_auth_resp.payload.get("CIDS").is_some());

        // 2. Test expressLogin for Player 1
        let mut login_req_payload = TdfStruct::new();
        login_req_payload.set("DSNM", TdfValue::String("PlayerOne".to_string()));
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

        let (login_resp, _) = router1.handle_packet(&login_req).unwrap();
        assert_eq!(login_resp.header.component, COMPONENT_AUTH);
        assert_eq!(login_resp.header.command, CMD_EXPRESS_LOGIN);
        assert_eq!(
            login_resp.payload.get("DSNM"),
            Some(&TdfValue::String("PlayerOne".to_string()))
        );
        assert!(router1.session.is_logged_in);
        assert_eq!(router1.session.user_id, 10001);

        // 3. Test second player login on separate router sharing same SessionStore and GameRegistry
        let mut router2 = BlazeRouter::new("127.0.0.1".to_string(), 17498, store.clone(), games.clone());
        let mut login2_payload = TdfStruct::new();
        login2_payload.set("DSNM", TdfValue::String("PlayerTwo".to_string()));
        let login2_req = BlazePacket::new(
            COMPONENT_AUTH,
            CMD_LOGIN,
            0,
            MessageType::SubmitRequest,
            0,
            1,
            login2_payload,
        )
        .unwrap();
        let (login2_resp, _) = router2.handle_packet(&login2_req).unwrap();
        assert_eq!(
            login2_resp.payload.get("DSNM"),
            Some(&TdfValue::String("PlayerTwo".to_string()))
        );
        assert_eq!(router2.session.user_id, 10002);
        assert_eq!(store.count(), 2);

        // 4. Test updateNetworkInfo for Player 2
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
            2,
            net_req_payload,
        )
        .unwrap();
        let (net_resp, net_notif) = router2.handle_packet(&net_req).unwrap();
        assert_eq!(net_resp.header.command, CMD_UPDATE_NETWORK_INFO);
        assert!(net_notif.is_some());

        // 5. Test lookupUsers from Player 1's perspective
        let lookup_req = BlazePacket::new(
            COMPONENT_USER_SESSIONS,
            CMD_LOOKUP_USERS,
            0,
            MessageType::SubmitRequest,
            0,
            3,
            TdfStruct::new(),
        )
        .unwrap();
        let (lookup_resp, _) = router1.handle_packet(&lookup_req).unwrap();
        assert_eq!(lookup_resp.header.command, CMD_LOOKUP_USERS);
        if let Some(TdfValue::List(ulist)) = lookup_resp.payload.get("ULST") {
            assert_eq!(ulist.items.len(), 2);
        } else {
            panic!("Expected ULST list in lookupUsers response");
        }

        // 6. Test AssociationLists (virtual online friends list)
        let friends_req = BlazePacket::new(
            COMPONENT_ASSOCIATION_LISTS,
            CMD_GET_LISTS,
            0,
            MessageType::SubmitRequest,
            0,
            4,
            TdfStruct::new(),
        )
        .unwrap();
        let (friends_resp, _) = router1.handle_packet(&friends_req).unwrap();
        assert_eq!(friends_resp.header.command, CMD_GET_LISTS);
        if let Some(TdfValue::List(lists)) = friends_resp.payload.get("LST") {
            if let TdfValue::Struct(fl) = &lists.items[0] {
                if let Some(TdfValue::List(members)) = fl.get("LMEM") {
                    // Player 1 sees Player 2 as online friend
                    assert_eq!(members.items.len(), 1);
                }
            }
        }

        // 7. Test GameManager createGame, joinGame, and Notification Delivery
        let (tx1, mut rx1) = tokio::sync::mpsc::unbounded_channel();
        let (tx2, mut rx2) = tokio::sync::mpsc::unbounded_channel();
        store.register_notification_channel(router1.session.user_id, tx1);
        store.register_notification_channel(router2.session.user_id, tx2);

        // Player 1 creates game
        let create_game_req = BlazePacket::new(
            COMPONENT_GAME_MANAGER,
            CMD_CREATE_GAME,
            0,
            MessageType::SubmitRequest,
            0,
            5,
            TdfStruct::new(),
        )
        .unwrap();
        let (create_resp, _) = router1.handle_packet(&create_game_req).unwrap();
        assert_eq!(create_resp.header.command, CMD_CREATE_GAME);
        let game_id = match create_resp.payload.get("GID").unwrap() {
            TdfValue::Integer(id) => *id,
            _ => panic!("Expected GID integer"),
        };

        // Check async notifications arrived for Player 1 (NotifyGameCreated, NotifyGameSetup)
        let n1 = rx1.try_recv().expect("Expected NotifyGameCreated");
        assert_eq!(n1.header.command, NOTIFY_GAME_CREATED);
        let n2 = rx1.try_recv().expect("Expected NotifyGameSetup");
        assert_eq!(n2.header.command, NOTIFY_GAME_SETUP);

        // Player 2 joins game
        let mut join_payload = TdfStruct::new();
        join_payload.set("GID", TdfValue::Integer(game_id));
        let join_req = BlazePacket::new(
            COMPONENT_GAME_MANAGER,
            CMD_JOIN_GAME,
            0,
            MessageType::SubmitRequest,
            0,
            3,
            join_payload,
        )
        .unwrap();
        let (join_resp, _) = router2.handle_packet(&join_req).unwrap();
        assert_eq!(join_resp.header.command, CMD_JOIN_GAME);

        // Check Player 2 received NotifyGameSetup
        let p2_setup = rx2.try_recv().expect("Expected NotifyGameSetup on join");
        assert_eq!(p2_setup.header.command, NOTIFY_GAME_SETUP);

        // Check Player 1 (Host) received NotifyPlayerJoining!
        let p1_joining = rx1.try_recv().expect("Expected NotifyPlayerJoining on host");
        assert_eq!(p1_joining.header.command, NOTIFY_PLAYER_JOINING);

        // 8. Test Wildcard Unknown-RPC Fallback
        let unknown_req = BlazePacket::new(
            0x00F0, // Unimplemented Component
            0x00A1, // Unimplemented Command
            0,
            MessageType::SubmitRequest,
            0,
            99,
            TdfStruct::new(),
        )
        .unwrap();
        let (unknown_resp, _) = router1.handle_packet(&unknown_req).unwrap();
        assert_eq!(unknown_resp.header.component, 0x00F0);
        assert_eq!(unknown_resp.header.command, 0x00A1);
        assert_eq!(unknown_resp.header.error_code, 0x0000); // ERR_OK!
    }
}
