//! The `Authentication` component's command IDs and routing, pinned to EA's
//! own table.
//!
//! A Blaze server dispatches on the numeric command ID alone, so a command ID
//! that is off by a few does not fail loudly -- it answers the wrong request
//! with the wrong reply, and the client silently retries.
//!
//! That is what happened. The emulator invented four `Authentication`
//! commands (`expressLogin` = `0x001C`, `login` = `0x0028`, `silentLogin` =
//! `0x0029` and `listUserEntitlements` = `0x0046`) and used the `Authentication`
//! component as a catch-all that fabricated a login session for *any* command.
//!
//! EA's own `AuthenticationMessage` enum
//! (`references/Skate3BlazeServer/Skate Custom Server/Blaze/MessageLists/AuthenticationMessages.cs`)
//! says what those numbers really are:
//!
//! ```text
//! createAccount = 10, updateAccount = 20, updateParentalEmail = 28,
//! getAccount = 30, grantEntitlement = 31, getEntitlements = 32,
//! hasEntitlement = 33, getUseCount = 34, decrementUseCount = 35,
//! getAuthToken = 36, getHandoffToken = 37, listEntitlement2 = 38,
//! acceptTos = 41, getTosInfo = 42, consumecode = 44, checkAgeReq = 51,
//! logout = 70, listPersonas = 100, loginPersona = 110, xboxLogin = 170,
//! ps3CreateAccount = 180, ps3AssociateAccount = 190, ps3Login = 200,
//! createWalUserSession = 230
//! ```
//!
//! So `0x001C` is `updateParentalEmail`, `0x0029` is `acceptTos` and
//! `0x0046` is `logout`; `0x0028` (40) is not in the enum at all. Blaze 15
//! has no `login`, `expressLogin`, `silentLogin` or `listUserEntitlements`.
//!
//! The live capture of FIFA 22 shows the client sending
//! `Comp=0x0001, Cmd=0x0046` with an **empty** payload, three times in a
//! burst and then every ~10 s until the game process exited. `0x0046` is
//! `logout`. The emulator answered it with a fabricated login response and
//! an entitlement list -- neither of which belongs in a `logout` reply -- so
//! the client could not decode the answer and kept retrying.

use std::sync::Arc;

use blaze_core::{BlazePacket, MessageType};
use blaze_tdf::{TdfStruct, TdfValue};
use services_blaze::auth::*;
use services_blaze::gamemanager::GameRegistry;
use services_blaze::router::BlazeRouter;
use services_blaze::session_store::SessionStore;

/// EA's `AuthenticationMessage` enum, verbatim.
const EA_AUTH_COMMANDS: &[(&str, u16)] = &[
    ("createAccount", 10),
    ("updateAccount", 20),
    ("updateParentalEmail", 28),
    ("getAccount", 30),
    ("grantEntitlement", 31),
    ("getEntitlements", 32),
    ("hasEntitlement", 33),
    ("getUseCount", 34),
    ("decrementUseCount", 35),
    ("getAuthToken", 36),
    ("getHandoffToken", 37),
    ("listEntitlement2", 38),
    ("acceptTos", 41),
    ("getTosInfo", 42),
    ("consumecode", 44),
    ("checkAgeReq", 51),
    ("logout", 70),
    ("listPersonas", 100),
    ("loginPersona", 110),
    ("xboxLogin", 170),
    ("ps3CreateAccount", 180),
    ("ps3AssociateAccount", 190),
    ("ps3Login", 200),
    ("createWalUserSession", 230),
];

#[test]
fn the_auth_command_ids_are_ea_s() {
    assert_eq!(CMD_CREATE_ACCOUNT, 10, "createAccount");
    assert_eq!(CMD_UPDATE_ACCOUNT, 20, "updateAccount");
    assert_eq!(CMD_UPDATE_PARENTAL_EMAIL, 28, "updateParentalEmail");
    assert_eq!(CMD_GET_ACCOUNT, 30, "getAccount");
    assert_eq!(CMD_GRANT_ENTITLEMENT, 31, "grantEntitlement");
    assert_eq!(CMD_GET_ENTITLEMENTS, 32, "getEntitlements");
    assert_eq!(CMD_HAS_ENTITLEMENT, 33, "hasEntitlement");
    assert_eq!(CMD_GET_USE_COUNT, 34, "getUseCount");
    assert_eq!(CMD_DECREMENT_USE_COUNT, 35, "decrementUseCount");
    assert_eq!(CMD_GET_AUTH_TOKEN, 36, "getAuthToken");
    assert_eq!(CMD_GET_HANDOFF_TOKEN, 37, "getHandoffToken");
    assert_eq!(CMD_LIST_ENTITLEMENT2, 38, "listEntitlement2");
    assert_eq!(CMD_ACCEPT_TOS, 41, "acceptTos");
    assert_eq!(CMD_GET_TOS_INFO, 42, "getTosInfo");
    assert_eq!(CMD_CONSUMECODE, 44, "consumecode");
    assert_eq!(CMD_CHECK_AGE_REQ, 51, "checkAgeReq");
    assert_eq!(CMD_LOGOUT, 70, "logout");
    assert_eq!(CMD_LIST_PERSONAS, 100, "listPersonas");
    assert_eq!(CMD_LOGIN_PERSONA, 110, "loginPersona");
    assert_eq!(CMD_XBOX_LOGIN, 170, "xboxLogin");
    assert_eq!(CMD_PS3_CREATE_ACCOUNT, 180, "ps3CreateAccount");
    assert_eq!(CMD_PS3_ASSOCIATE_ACCOUNT, 190, "ps3AssociateAccount");
    assert_eq!(CMD_PS3_LOGIN, 200, "ps3Login");
    assert_eq!(CMD_CREATE_WAL_USER_SESSION, 230, "createWalUserSession");
}

/// No two `Authentication` commands may share an ID.
#[test]
fn no_two_auth_commands_share_an_id() {
    let mut seen: Vec<(&str, u16)> = EA_AUTH_COMMANDS.to_vec();
    seen.sort_by_key(|(_, id)| *id);
    for pair in seen.windows(2) {
        assert_ne!(
            pair[0].1, pair[1].1,
            "{} and {} share command id {}",
            pair[0].0,
            pair[1].0,
            pair[0].1
        );
    }
}

/// The error code EA uses for a command the server does not implement.
///
/// From EA's own `ServerUtils.ErrorCode` enum
/// (`references/Skate3BlazeServer/Skate Custom Server/Servers/ServerUtils.cs`).
#[test]
fn the_command_not_found_error_is_ea_s() {
    assert_eq!(ERR_COMMAND_NOT_FOUND, 0x4003, "ERR_COMMAND_NOT_FOUND");
}

/// The command the FIFA 22 client actually sent must be routed as EA routes
/// it: `0x0046` is `logout`, and Blaze 15 has no `listUserEntitleents`.
///
/// `logout` is answered with an empty **success**.
///
/// The third-party reference router
/// (`references/Skate3BlazeServer/.../Blaze/Routers/AuthenticationRouter.cs`)
/// answers it with `ERR_COMMAND_NOT_FOUND`, because that sample server never
/// implemented it.  The live client disagrees: the FIFA 22 capture shows it
/// sending `logout` again and again with a *fresh* Fire2 message number
/// (`0x29`, `0x2A`, `0x2B`, ... `0x2F`) after every errored answer, and it
/// never moved on to `Util::postAuth` (`0x0009`/`0x0008`) while it was doing
/// so.  The retail EA server implements `logout`, and the client's retry
/// loop is the evidence that the error reply is the wrong answer.
#[test]
fn the_logout_command_is_answered_with_an_empty_success() {
    let store = Arc::new(SessionStore::new());
    let games = Arc::new(GameRegistry::new());
    let mut router = BlazeRouter::new("127.0.0.1".to_string(), 17498, store.clone(), games);

    let req = BlazePacket::new(
        COMPONENT_AUTH,
        CMD_LOGOUT,
        0,
        MessageType::SubmitRequest,
        0,
        1,
        TdfStruct::new(),
    )
    .unwrap();

    let (resp, notif) = router.handle_packet(&req).unwrap();

    assert_eq!(resp.header.component, COMPONENT_AUTH);
    assert_eq!(resp.header.command, CMD_LOGOUT);
    assert_eq!(
        resp.header.error_code, 0x0000,
        "logout must be answered with a success, not with ERR_COMMAND_NOT_FOUND"
    );
    assert_eq!(
        resp.header.msg_type,
        MessageType::RecordResponse,
        "a successful logout is a REPLY, not an ERROR_REPLY"
    );
    assert!(
        resp.payload.fields.is_empty(),
        "logout returns nothing, so the reply must carry no payload"
    );
    assert!(
        notif.is_none(),
        "a logout must not produce a session notification"
    );
    assert!(
        !router.session.is_logged_in,
        "a logout must mark the local session as logged out"
    );
}

/// `acceptTos` and `getEntitlements` are answered with an empty response,
/// exactly as EA's own `AuthenticationRouter` does.
#[test]
fn accept_tos_and_get_entitlements_are_answered_empty() {
    let store = Arc::new(SessionStore::new());
    let games = Arc::new(GameRegistry::new());
    let mut router = BlazeRouter::new("127.0.0.1".to_string(), 17498, store.clone(), games);

    for cmd in [CMD_ACCEPT_TOS, CMD_GET_ENTITLEMENTS] {
        let req = BlazePacket::new(
            COMPONENT_AUTH,
            cmd,
            0,
            MessageType::SubmitRequest,
            0,
            1,
            TdfStruct::new(),
        )
        .unwrap();

        let (resp, _) = router.handle_packet(&req).unwrap();
        assert_eq!(resp.header.command, cmd);
        assert_eq!(resp.header.error_code, 0x0000);
        assert!(
            resp.payload.fields.is_empty(),
            "command 0x{:04X} must be answered with an empty response",
            cmd
        );
    }
}

/// `createWalUserSession` is answered with EA's own response model.
#[test]
fn create_wal_user_session_is_answered_with_ea_s_model() {
    let store = Arc::new(SessionStore::new());
    let games = Arc::new(GameRegistry::new());
    let mut router = BlazeRouter::new("127.0.0.1".to_string(), 17498, store.clone(), games);

    let req = BlazePacket::new(
        COMPONENT_AUTH,
        CMD_CREATE_WAL_USER_SESSION,
        0,
        MessageType::SubmitRequest,
        0,
        1,
        TdfStruct::new(),
    )
    .unwrap();

    let (resp, notif) = router.handle_packet(&req).unwrap();

    assert_eq!(resp.header.command, CMD_CREATE_WAL_USER_SESSION);
    assert_eq!(resp.header.error_code, 0x0000);

    for member in ["BUID", "FRST", "KEY", "LLOG", "MAIL", "PDTL", "UID"] {
        assert!(
            resp.payload.get(member).is_some(),
            "createWalUserSession must carry `{}`",
            member
        );
    }

    let persona = match resp.payload.get("PDTL") {
        Some(TdfValue::Struct(s)) => s,
        other => panic!("PDTL must be a struct, got {:?}", other),
    };
    for member in ["DSNM", "LAST", "PID", "XREF", "XTYP"] {
        assert!(
            persona.get(member).is_some(),
            "PersonaDetails must carry `{}`",
            member
        );
    }

    assert!(
        notif.is_some(),
        "a session that has been established must be announced to the client"
    );
    assert!(router.session.is_logged_in);
}

/// The `Authentication` component must no longer be a catch-all that
/// fabricates a login for every command: a command EA does not implement
/// must not create a session, and must not answer with a login response.
#[test]
fn an_unknown_auth_command_neither_creates_a_session_nor_fakes_a_login() {
    let store = Arc::new(SessionStore::new());
    let games = Arc::new(GameRegistry::new());
    let mut router = BlazeRouter::new("127.0.0.1".to_string(), 17498, store.clone(), games);

    for cmd in [0x001Cu16, 0x0028, 0x0047, 0x005A, 0x00C8] {
        let req = BlazePacket::new(
            COMPONENT_AUTH,
            cmd,
            0,
            MessageType::SubmitRequest,
            0,
            1,
            TdfStruct::new(),
        )
        .unwrap();

        let (resp, notif) = router.handle_packet(&req).unwrap();
        assert_eq!(
            resp.header.error_code, ERR_COMMAND_NOT_FOUND,
            "command 0x{:04X} must be answered with ERR_COMMAND_NOT_FOUND",
            cmd
        );
        assert!(resp.payload.fields.is_empty());
        assert!(notif.is_none());
        assert!(
            !router.session.is_logged_in,
            "command 0x{:04X} must not have created a session",
            cmd
        );
    }

    assert_eq!(store.count(), 0, "no session may have been created");
}
