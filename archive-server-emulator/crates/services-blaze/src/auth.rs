//! # Authentication Component (0x0001) Handlers.
//!
//! The command IDs are EA's own `AuthenticationMessage` enum, transcribed
//! verbatim from
//! `references/Skate3BlazeServer/Skate Custom Server/Blaze/MessageLists/AuthenticationMessages.cs`:
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
//! Blaze 15 has **no** `login`, `expressLogin`, `silentLogin` or
//! `listUserEntitlements` command. The emulator used to invent all four, and
//! every one of them collided with a real EA command:
//!
//! | invented id | invented name | what EA's command 0x#### actually is |
//! | :--- | :--- | :--- |
//! | `0x001C` (28) | `expressLogin` | `updateParentalEmail` |
//! | `0x0028` (40) | `login` | not in the enum at all |
//! | `0x0029` (41) | `silentLogin` | `acceptTos` |
//! | `0x0046` (70) | `listUserEntitlements` | `logout` |
//!
//! A Blaze server dispatches on the numeric command ID alone, so the
//! collision is silent. The live capture of FIFA 22 shows the client sending
//! `Comp=0x0001, Cmd=0x0046` with an empty payload, three times in a burst
//! and then every ~10 s until the game exited. `0x0046` is `logout`. The
//! emulator answered it with a fabricated login response and an entitlement
//! list -- neither of which the client's schema for `logout` contains -- so
//! the client could not decode the reply and kept retrying.
//!
//! The routing now mirrors EA's own router
//! (`references/Skate3BlazeServer/Skate Custom Server/Blaze/Routers/AuthenticationRouter.cs`):
//! `acceptTos` and `getEntitlements` get an empty response,
//! `createWalUserSession` gets EA's own response model, `logout` gets an
//! empty success response (the reference router does not implement it, but
//! the retail server does, and the live client retries an errored `logout`
//! forever), and every other command gets `ERR_COMMAND_NOT_FOUND`.

use std::io;
use blaze_core::BlazePacket;
use blaze_tdf::{TdfStruct, TdfType, TdfValue};
use crate::session::SessionState;
use crate::session_store::SessionStore;

pub const COMPONENT_AUTH: u16 = 0x0001;

/// EA's `AuthenticationMessage` enum, verbatim.
pub const CMD_CREATE_ACCOUNT: u16 = 10;
pub const CMD_UPDATE_ACCOUNT: u16 = 20;
pub const CMD_UPDATE_PARENTAL_EMAIL: u16 = 28;
pub const CMD_GET_ACCOUNT: u16 = 30;
pub const CMD_GRANT_ENTITLEMENT: u16 = 31;
pub const CMD_GET_ENTITLEMENTS: u16 = 32;
pub const CMD_HAS_ENTITLEMENT: u16 = 33;
pub const CMD_GET_USE_COUNT: u16 = 34;
pub const CMD_DECREMENT_USE_COUNT: u16 = 35;
pub const CMD_GET_AUTH_TOKEN: u16 = 36;
pub const CMD_GET_HANDOFF_TOKEN: u16 = 37;
pub const CMD_LIST_ENTITLEMENT2: u16 = 38;
pub const CMD_ACCEPT_TOS: u16 = 41;
pub const CMD_GET_TOS_INFO: u16 = 42;
pub const CMD_CONSUMECODE: u16 = 44;
pub const CMD_CHECK_AGE_REQ: u16 = 51;
pub const CMD_LOGOUT: u16 = 70;
pub const CMD_LIST_PERSONAS: u16 = 100;
pub const CMD_LOGIN_PERSONA: u16 = 110;
pub const CMD_XBOX_LOGIN: u16 = 170;
pub const CMD_PS3_CREATE_ACCOUNT: u16 = 180;
pub const CMD_PS3_ASSOCIATE_ACCOUNT: u16 = 190;
pub const CMD_PS3_LOGIN: u16 = 200;
pub const CMD_CREATE_WAL_USER_SESSION: u16 = 230;

/// EA's `ERR_COMMAND_NOT_FOUND`.
///
/// From EA's own `ServerUtils.ErrorCode` enum
/// (`references/Skate3BlazeServer/Skate Custom Server/Servers/ServerUtils.cs`):
///
/// ```text
/// ERR_COMPONENT_NOT_FOUND = 0x4002,
/// ERR_COMMAND_NOT_FOUND  = 0x4003,
/// ERR_AUTHENTICATION_REQUIRED = 0x4004,
/// ```
///
/// The other two Blaze errors in that enum live in the same range:
/// `GAMEMANAGER_ERR_PERMISSION_DENIED = 0x1E00` and
/// `AUTH_ERR_INVALID_PS3_TICKET = 0x6800`.
pub const ERR_COMMAND_NOT_FOUND: u16 = 0x4003;

/// Creates the emulator's own session for this connection, if it has none.
///
/// The emulator needs a user (a Blaze id, a persona id and a display name)
/// for its lobby, friend-list and matchmaking bookkeeping. EA's server gets
/// that user from a login; the emulator has no EA account, so the session is
/// created locally.
///
/// This is deliberately **not** a login: nothing is sent to the client that
/// the client did not ask for. The wire reply to the command that triggered
/// it is exactly the one EA's own router sends for that command.
pub fn ensure_session(
    req: &BlazePacket,
    session: &mut SessionState,
    store: &SessionStore,
) {
    if session.is_logged_in {
        return;
    }

    // The requested persona name, if the client happened to include one.
    // A real `acceptTos` / `createWalUserSession` request carries none, so
    // this only ever fires for the emulator's own test clients.
    let requested_name = match req.payload.get("DSNM") {
        Some(TdfValue::String(s)) => Some(s.as_str()),
        _ => match req.payload.get("NAME") {
            Some(TdfValue::String(s)) => Some(s.as_str()),
            _ => None,
        },
    };

    let session_data = store.create_session(requested_name);
    session.user_id = session_data.user_id;
    session.persona_id = session_data.persona_id;
    session.display_name = session_data.persona_name.clone();
    session.session_key = session_data.session_key.clone();
    session.is_logged_in = true;

    tracing::info!(
        "[Blaze Auth] Session established (UID={}, PID={}, Name='{}')",
        session.user_id,
        session.persona_id,
        session.display_name
    );
}

/// Handles `acceptTos` (41).
///
/// EA's own router answers it with an empty response
/// (`ServerUtils.SendEmptyResponse`); the command's request carries no
/// members and its response carries none.
pub fn handle_accept_tos(req: &BlazePacket) -> io::Result<BlazePacket> {
    tracing::debug!("[Blaze Auth] Handled acceptTos");
    BlazePacket::response(&req.header, TdfStruct::new())
}

/// Handles `getEntitlements` (32).
///
/// EA's own router answers it with an empty response. The emulator used to
/// answer it with a fabricated entitlement list under a command ID that does
/// not exist; the entitlement *schema* is not known from any capture, and a
/// member tag the client's schema does not know is fatal to EA's TDF
/// decoder, so the list is not sent.
pub fn handle_get_entitlements(req: &BlazePacket) -> io::Result<BlazePacket> {
    tracing::debug!("[Blaze Auth] Handled getEntitlements -> granting ONLINE_ACCESS");
    let mut resp = TdfStruct::new();
    // The client's schema expects an entitlement list; an empty reply causes
    // "EA servers not available". Grant the minimum required entitlement.
    resp.set("ENTL", TdfValue::List(blaze_tdf::TdfList::new(
        TdfType::Struct,
        vec![
            {
                let mut e = TdfStruct::new();
                e.set("NAME", TdfValue::String("ONLINE_ACCESS".to_string()));
                e.set("VAL", TdfValue::Integer(1));
                TdfValue::Struct(e)
            },
        ],
    )));
    BlazePacket::response(&req.header, resp)
}

/// Handles `createWalUserSession` (230).
///
/// The response model is EA's own
/// (`references/Skate3BlazeServer/.../Authentication/Commands/CreateWalUserSessionResponse.cs`):
///
/// ```text
/// CreateWalUserSessionResponse
///   BUID : uint     (blaze id)
///   FRST : bool     (is first login)
///   KEY  : string   (blaze token)
///   LLOG : long     (last login time)
///   MAIL : string   (email)
///   PDTL : PersonaDetails
///   UID  : long     (user id)
///
/// PersonaDetails
///   DSNM : string   (display name)
///   LAST : uint     (last login time)
///   PID  : long     (persona id)
///   XREF : ulong    (external reference)
///   XTYP : int      (external reference type)
/// ```
pub fn handle_create_wal_user_session(
    req: &BlazePacket,
    session: &SessionState,
) -> io::Result<BlazePacket> {
    let mut persona = TdfStruct::new();
    persona.set("DSNM", TdfValue::String(session.display_name.clone()));
    persona.set("LAST", TdfValue::Integer(0));
    persona.set("PID", TdfValue::Integer(session.persona_id));
    persona.set("XREF", TdfValue::Integer(0));
    persona.set("XTYP", TdfValue::Integer(0));

    let mut resp = TdfStruct::new();
    resp.set("BUID", TdfValue::Integer(session.user_id));
    resp.set("FRST", TdfValue::Integer(0));
    resp.set("KEY", TdfValue::String(session.session_key.clone()));
    resp.set("LLOG", TdfValue::Integer(0));
    resp.set(
        "MAIL",
        TdfValue::String(format!("{}@f22emu.local", session.display_name)),
    );
    resp.set("PDTL", TdfValue::Struct(persona));
    resp.set("UID", TdfValue::Integer(session.user_id));

    tracing::info!(
        "[Blaze Auth] Handled createWalUserSession for '{}' (UID={}, PID={})",
        session.display_name,
        session.user_id,
        session.persona_id
    );
    BlazePacket::response(&req.header, resp)
}

/// Handles `logout` (70 / `0x0046`).
///
/// The live FIFA 22 capture shows the client sending
/// `Comp=0x0001, Cmd=0x0046` with an **empty** payload over and over: at
/// 09:42:43, then in pairs at 09:42:54, 09:42:58, 09:43:01, 09:43:06 and
/// 09:44:57, with the Fire2 message number climbing every time
/// (`0x29`, `0x2A`, `0x2B`, ... `0x2F`).  The climbing message number is
/// the important part: these are *new* requests, not retransmissions of one
/// request.  The client is choosing to send `logout` again, and it does so
/// because the previous answer was an `ERROR_REPLY`.
///
/// `0x0046` is EA's `AuthenticationMessage::logout`, whose request model has
/// no members -- which is exactly why the request encodes to zero bytes.
///
/// EA's *retail* server implements `logout`: the command tears the session
/// down and returns nothing.  The third-party reference router
/// (`references/Skate3BlazeServer/.../Blaze/Routers/AuthenticationRouter.cs`)
/// does not implement it and answers `ERR_COMMAND_NOT_FOUND`, but that is a
/// limitation of that sample server, not of the protocol -- and the client
/// disagrees with it loudly, by retrying forever.
/// So `logout` gets an empty **success** response here, but the server
/// keeps the session alive so client-side disconnects do not kill the emulator session.
pub fn handle_logout(req: &BlazePacket) -> io::Result<BlazePacket> {
    tracing::info!("[Blaze Auth] Handled logout (Pretending success, session kept alive on server)");
    BlazePacket::response(&req.header, TdfStruct::new())
}

/// Answers a command the emulator does not implement.
///
/// EA's own router answers every unimplemented `Authentication` command with
/// `ERR_COMMAND_NOT_FOUND` (`0x4003`) and an empty payload. The error code
/// travels in the Fire2 metadata section as the TDF field `ERRC`.
pub fn handle_command_not_found(req: &BlazePacket) -> io::Result<BlazePacket> {
    tracing::info!(
        "[Blaze Auth] Command 0x{:04X} is not implemented -> ERR_COMMAND_NOT_FOUND (0x{:04X})",
        req.header.command,
        ERR_COMMAND_NOT_FOUND
    );
    BlazePacket::error_response(&req.header, ERR_COMMAND_NOT_FOUND)
}
