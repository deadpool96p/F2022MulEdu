//! The emulator's `preAuth` reply must have the same shape as EA's.
//!
//! The two fixtures are authentic EA `Util` (component `0x0009`) `preAuth`
//! (command `0x0007`) replies, captured off the wire in the Fire2 framing
//! generation. They are the only ground truth available for what this reply
//! looks like, and they are what settled the command ID question: a Blaze
//! server dispatches on the numeric command ID alone, so answering the wrong
//! command fails silently.
//!
//! Decoded with the crate's own codec, the two replies carry these root
//! members:
//!
//! ```text
//! Blaze 15.1.1.4.6      Blaze 18.3.0
//!   ASRC                  ASRC
//!   CIDS                  CIDS
//!   CLID                  CLID
//!   CONF                  CONF
//!   ESRC                  ESRC
//!   INST                  HPLT      <- game specific
//!   MAID                  INST
//!   MINR                  LQTK      <- game specific
//!   NASP                  MAID
//!   PILD                  MINR
//!   PLAT                  NASP
//!   QOSS                  PILD
//!   RSRC                  PLAT
//!   SVER                  QOSS
//!                          RELT      <- game specific
//!                          RSRC
//!                          SVER
//! ```
//!
//! The three extra members in the second reply belong to that game's
//! schema, so the members both replies agree on are the ones the emulator
//! must send.

use blaze_core::{BlazePacket, MessageType};
use blaze_tdf::{deserialize_tdf, TdfStruct, TdfValue};
use services_blaze::util::*;

/// The authentic EA replies, with the server version each one reports.
const EA_REPLIES: &[(&str, &[u8])] = &[
    (
        "Blaze 15.1.1.4.6",
        include_bytes!("fixtures/ea_preauth_blaze_15_1_1_4_6.bin"),
    ),
    (
        "Blaze 18.3.0",
        include_bytes!("fixtures/ea_preauth_blaze_18_3_0.bin"),
    ),
];

/// The root members of an EA `preAuth` reply, as a sorted list of names.
fn root_members(frame: &[u8]) -> Vec<String> {
    let payload = &frame[16..];
    let decoded = deserialize_tdf(payload).expect("an EA preAuth reply must decode");
    let mut names: Vec<String> = decoded.fields.keys().cloned().collect();
    names.sort();
    names
}

/// The reply the emulator builds.
fn our_reply() -> TdfStruct {
    let req = BlazePacket::new(
        COMPONENT_UTIL,
        CMD_PRE_AUTH,
        0,
        MessageType::SubmitRequest,
        0,
        1,
        TdfStruct::new(),
    )
    .expect("request");
    handle_pre_auth(&req, "127.0.0.1", 17498)
        .expect("preAuth handler must not error")
        .payload
}

/// The members both authentic EA replies carry.
fn members_ea_agrees_on() -> Vec<String> {
    let mut common: Option<Vec<String>> = None;
    for (_, frame) in EA_REPLIES {
        let members = root_members(frame);
        common = Some(match common {
            None => members,
            Some(previous) => previous
                .into_iter()
                .filter(|m| members.contains(m))
                .collect(),
        });
    }
    common.expect("there are EA replies to compare")
}

/// The fixtures must really be EA `preAuth` replies, or the comparison
/// below would be against the wrong thing.
#[test]
fn the_fixtures_are_ea_preauth_replies() {
    for (version, frame) in EA_REPLIES {
        assert!(frame.len() >= 16, "{}: too short to be a Fire2 frame", version);

        let payload_size = u32::from_be_bytes([frame[0], frame[1], frame[2], frame[3]]) as usize;
        let component = u16::from_be_bytes([frame[6], frame[7]]);
        let command = u16::from_be_bytes([frame[8], frame[9]]);
        let message_type = frame[13] >> 5;

        assert_eq!(component, COMPONENT_UTIL, "{}: component", version);
        assert_eq!(command, CMD_PRE_AUTH, "{}: command", version);
        assert_eq!(message_type, 1, "{}: a reply, not a request", version);
        assert_eq!(
            frame.len(),
            16 + payload_size,
            "{}: the frame length must match its declared payload size",
            version
        );
    }
}

/// The reply must carry exactly the members EA's own replies agree on.
///
/// Not a subset -- EA sends the same member set from one Blaze generation to
/// the next, and a member the emulator invents is a tag the client has to
/// skip, while a member it omits is a field the client reads as a default.
#[test]
fn our_preauth_reply_carries_exactly_ea_s_members() {
    let expected = members_ea_agrees_on();
    let mut actual: Vec<String> = our_reply().fields.keys().cloned().collect();
    actual.sort();

    assert_eq!(
        actual, expected,
        "the preAuth reply's member set must match the one both authentic EA replies carry"
    );
}

/// The `CONF` member is a struct whose only member is the configuration
/// map -- not a bare map. EA's `FetchConfigResponse` says so, and so does
/// the wire.
#[test]
fn our_conf_member_has_ea_s_shape() {
    for (version, frame) in EA_REPLIES {
        let decoded = deserialize_tdf(&frame[16..]).expect("decode");
        let ea_conf = decoded.get("CONF").expect("EA reply carries CONF");
        assert!(
            matches!(ea_conf, TdfValue::Struct(_)),
            "{}: EA's CONF is a struct",
            version
        );
    }

    let ours = our_reply();
    let our_conf = ours.get("CONF").expect("our reply carries CONF");
    let our_conf = match our_conf {
        TdfValue::Struct(s) => s,
        other => panic!("our CONF must be a struct, got {:?}", other),
    };
    assert!(
        matches!(our_conf.get("CONF"), Some(TdfValue::Map(_))),
        "our CONF struct must hold the configuration map"
    );
}

/// The `QOSS` member must carry EA's `QosConfigInfo` members, and every ping
/// site must be the emulator's own responder -- never a real EA host.
#[test]
fn our_qoss_member_has_ea_s_shape_and_points_at_the_emulator() {
    let ours = our_reply();
    let qoss = match ours.get("QOSS") {
        Some(TdfValue::Struct(s)) => s,
        other => panic!("QOSS must be a struct, got {:?}", other),
    };

    for member in ["BWPS", "LNP", "LTPS", "SVID"] {
        assert!(qoss.get(member).is_some(), "QOSS must carry `{}`", member);
    }

    let bwps = match qoss.get("BWPS") {
        Some(TdfValue::Struct(s)) => s,
        other => panic!("BWPS must be a struct, got {:?}", other),
    };
    assert_eq!(
        bwps.get("PSA"),
        Some(&TdfValue::String("127.0.0.1".to_string())),
        "the default ping site must be the emulator host"
    );
    assert_eq!(
        bwps.get("PSP"),
        Some(&TdfValue::Integer(17498)),
        "the default ping site must be the emulator's UDP responder port"
    );
}
