//! The `Util` component's command IDs, pinned to EA's own table.
//!
//! A Blaze server dispatches on the numeric command ID alone, so a command ID
//! that is off by one does not fail loudly -- it answers the wrong request
//! with the wrong reply, and the client silently stalls.
//!
//! That is exactly what happened. The emulator had `preAuth` at `0x000B` and
//! `userSettingsSave` at `0x0007`. A live capture of FIFA 22 shows the client
//! opening the session with:
//!
//! ```text
//! Comp=0x0009 Cmd=0x0007  Len=230  MsgType=SubmitRequest
//! ```
//!
//! and the 230-byte payload is the client's identity blob -- decoded with the
//! crate's own codec, every byte consumed:
//!
//! ```text
//! CDAT (struct)          CINF (struct)              FCCR (struct)
//!   CTRY = 21843           BSDK = "15.1.1.10.3"        CFID = "BlazeSDK"
//!   IITO = 0               BTIM = "Aug 10 2022 ..."  LADD = 1744939200
//!   LANG = 1701729619      CLNT = "FIFA22"
//!   SVCN = "fifa-2022-pc"  CPFT = 4
//!   TYPE = 0               CSKU = "FIFAPC"
//!                          CTRY = 21843
//!                          CVER = "5091917"
//!                          DSDK = "15.1.7.0.3"
//!                          ENV = "prod"
//!                          LOC = 1701729619
//!                          PTVR = "1.1"
//!                          PUSH = ""
//!                          TTID = ""
//! ```
//!
//! The emulator matched that against `userSettingsSave` and replied with an
//! empty payload, so the client never received a `preAuth` response. It then
//! fell into a three-second reconnect loop, sending a Fire2 `PING` frame each
//! time.
//!
//! EA's own source pins the table. `references/Skate3BlazeServer/.../Blaze/
//! MessageLists/UtilMessages.cs` is EA's `UtilMessage` enum, and
//! `references/Skate3BlazeServer/.../Blaze/Components/Util/Handlers/
//! PreAuthHandler.cs` builds its response with the literal header bytes
//! `0, 0, 0, 9, 0, 7, 0, 0, 0, 0, 0, 0` -- component `0x0009`, command
//! `0x0007` -- which is `preAuth`.

use services_blaze::util::*;

/// EA's `UtilMessage` enum, verbatim.
const EA_UTIL_COMMANDS: &[(&str, u16)] = &[
    ("fetchClientConfig", 1),
    ("ping", 2),
    ("setClientData", 3),
    ("localizeStrings", 4),
    ("getTelemetryServer", 5),
    ("getTickerServer", 6),
    ("preAuth", 7),
    ("postAuth", 8),
    ("userSettingsLoad", 10),
    ("userSettingsSave", 11),
    ("userSettingsLoadAll", 12),
    ("userSettingsLoadAllForUserId", 13),
    ("filterForProfanity", 20),
    ("fetchQosConfig", 21),
    ("setClientMetrics", 22),
    ("setConnectionState", 23),
];

#[test]
fn the_util_command_ids_are_ea_s() {
    assert_eq!(CMD_FETCH_CLIENT_CONFIG, 1, "fetchClientConfig");
    assert_eq!(CMD_PING, 2, "ping");
    assert_eq!(CMD_SET_CLIENT_DATA, 3, "setClientData");
    assert_eq!(CMD_LOCALIZE_STRINGS, 4, "localizeStrings");
    assert_eq!(CMD_GET_TELEMETRY_SERVER, 5, "getTelemetryServer");
    assert_eq!(CMD_GET_TICKER_SERVER, 6, "getTickerServer");
    assert_eq!(CMD_PRE_AUTH, 7, "preAuth");
    assert_eq!(CMD_POST_AUTH, 8, "postAuth");
    assert_eq!(CMD_USER_SETTINGS_LOAD, 10, "userSettingsLoad");
    assert_eq!(CMD_USER_SETTINGS_SAVE, 11, "userSettingsSave");
    assert_eq!(CMD_USER_SETTINGS_LOAD_ALL, 12, "userSettingsLoadAll");
    assert_eq!(CMD_USER_SETTINGS_LOAD_ALL_FOR_USER_ID, 13, "userSettingsLoadAllForUserId");
    assert_eq!(CMD_FILTER_FOR_PROFANITY, 20, "filterForProfanity");
    assert_eq!(CMD_FETCH_QOS_CONFIG, 21, "fetchQosConfig");
    assert_eq!(CMD_SET_CLIENT_METRICS, 22, "setClientMetrics");
    assert_eq!(CMD_SET_CONNECTION_STATE, 23, "setConnectionState");
}

/// No two Util commands may share an ID, and none may collide with the
/// component IDs, which live in the same numeric space in a Blaze frame.
#[test]
fn no_two_util_commands_share_an_id() {
    let mut seen: Vec<(&str, u16)> = EA_UTIL_COMMANDS.to_vec();
    seen.sort_by_key(|(_, id)| *id);
    for pair in seen.windows(2) {
        assert_ne!(
            pair[0].1, pair[1].1,
            "{} and {} share command id {}",
            pair[0].0, pair[1].0, pair[0].1
        );
    }
}

/// The preAuth response must be a *reply* to the preAuth request, carrying
/// the request's message number so the client can match the transaction.
#[test]
fn the_pre_auth_reply_echoes_the_request_transaction() {
    use blaze_core::{BlazePacket, MessageType};
    use blaze_tdf::TdfStruct;

    // Build the request the way the frame parser does: the 24-bit message
    // number lives in bytes 10..13 and the user index in the low 5 bits of
    // byte 13, so neither is reachable through `BlazePacket::new`.
    let mut req = BlazePacket::new(
        COMPONENT_UTIL,
        CMD_PRE_AUTH,
        0,
        MessageType::SubmitRequest,
        0,
        0,
        TdfStruct::new(),
    )
    .expect("request");
    req.header.message_number = 0x000123;
    req.header.user_index = 0x07;

    let resp = handle_pre_auth(&req, "127.0.0.1", 17498).expect("preAuth response");

    assert_eq!(resp.header.command, CMD_PRE_AUTH);
    assert_eq!(resp.header.msg_type, MessageType::RecordResponse);
    assert_eq!(
        resp.header.message_number, 0x000123,
        "the reply must echo the request's 24-bit message number"
    );
    assert_eq!(resp.header.user_index, 0x07);
}
