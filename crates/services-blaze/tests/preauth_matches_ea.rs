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

/// The `QOSS` member must carry exactly the member set EA's Blaze 15
/// `preAuth` reply carries, and every ping site must be the emulator's own
/// responder -- never a real EA host.
///
/// The open-source model files
/// (`references/Skate3BlazeServer/.../Util/Models/QosConfigInfo.cs` and
/// `QosPingSiteInfo.cs`) reconstruct the *older* Blaze 2/3 schema, whose
/// `QosConfigInfo` has `SVID` and whose `QosPingSiteInfo` has `SNA`. The
/// Blaze 15 server does not send either: the retail reply below carries
/// `TIME` and its ping sites carry only `PSA` and `PSP`. A member tag the
/// client's schema does not know is fatal to EA's TDF decoder
/// (`BLAZE_ERR_TDF_TAG_NOT_FOUND`), so the emulator has to send the Blaze 15
/// member set, not the older one.
#[test]
fn our_qoss_member_has_ea_s_shape_and_points_at_the_emulator() {
    // The member set EA's own Blaze 15 reply carries.
    let frame = EA_REPLIES
        .iter()
        .find(|(version, _)| *version == "Blaze 15.1.1.4.6")
        .expect("the Blaze 15 fixture")
        .1;
    let decoded = deserialize_tdf(&frame[16..]).expect("decode");
    let ea_qoss = match decoded.get("QOSS") {
        Some(TdfValue::Struct(s)) => s,
        other => panic!("EA's QOSS is a struct, got {:?}", other),
    };
    let mut ea_members: Vec<String> = ea_qoss.fields.keys().cloned().collect();
    ea_members.sort();

    let ours = our_reply();
    let qoss = match ours.get("QOSS") {
        Some(TdfValue::Struct(s)) => s,
        other => panic!("QOSS must be a struct, got {:?}", other),
    };
    let mut our_members: Vec<String> = qoss.fields.keys().cloned().collect();
    our_members.sort();

    assert_eq!(
        our_members, ea_members,
        "QOSS must carry exactly the members EA's Blaze 15 preAuth reply carries"
    );

    // The Blaze 2/3 members must not be sent.
    for stale in ["SVID"] {
        assert!(
            !our_members.iter().any(|m| m == stale),
            "QOSS must not carry `{}`; it is not part of the Blaze 15 schema",
            stale
        );
    }

    for member in ["BWPS", "LNP", "LTPS", "TIME"] {
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

    // A ping site carries `PSA` and `PSP` -- and nothing else.
    let mut bwps_members: Vec<String> = bwps.fields.keys().cloned().collect();
    bwps_members.sort();
    assert_eq!(
        bwps_members,
        vec!["PSA".to_string(), "PSP".to_string()],
        "a QosPingSiteInfo must carry exactly PSA and PSP in Blaze 15"
    );
    assert!(
        !bwps_members.iter().any(|m| m == "SNA"),
        "`SNA` is not part of the Blaze 15 QosPingSiteInfo schema"
    );
}

/// The `CIDS` list must name exactly the components the emulator
/// implements.
///
/// EA's Blaze 15 server advertises 22 components; the emulator implements
/// eight of them. The client uses the list to decide which components it
/// may call, so the list has to contain every component the emulator can
/// answer, and no component it cannot.
#[test]
fn our_cids_list_names_the_components_the_emulator_implements() {
    let ours = our_reply();
    let cids = match ours.get("CIDS") {
        Some(TdfValue::List(l)) => l,
        other => panic!("CIDS must be a list, got {:?}", other),
    };

    let ids: Vec<u64> = cids
        .items
        .iter()
        .map(|v| match v {
            TdfValue::Integer(i) => *i,
            other => panic!("a CIDS entry must be an integer, got {:?}", other),
        })
        .collect();

    assert_eq!(
        ids,
        vec![1, 4, 7, 9, 10, 25, 2234, 30722],
        "CIDS must list exactly the components the emulator implements"
    );

    // Every component the emulator implements must be on the list.
    for id in [
        1u64,    // Authentication
        4,       // GameManager
        7,       // Stats
        9,       // Util
        10,      // CensusData
        25,      // AssociationLists
        2234,    // PerfComponent (0x08BA)
        30722,   // UserSessions (0x7802)
    ] {
        assert!(ids.contains(&id), "CIDS must contain {}", id);
    }

    // The components EA's Blaze 15 server advertises but the emulator does
    // not implement must not be advertised.
    for id in [
        15u64,  // Messaging
        27,     // GpsContentController
        33,     // Achievements
        2000,   // game-specific
        63490,  // game-specific
        30720,  // 0x7800-range
        30721,  // 0x7801-range
        30723,  // 0x7803-range
    ] {
        assert!(
            !ids.contains(&id),
            "CIDS must not contain {}; the emulator does not implement it",
            id
        );
    }
}

/// The `CONF` map must carry EA's own service host names, verbatim.
///
/// A URL whose host is an IP literal (`https://127.0.0.1`) makes the
/// client send a TLS `server_name` extension with a zero-length host
/// name, which rustls 0.23 rejects with
/// `InvalidMessage(IllegalEmptyValue)` -- the handshake dies before any
/// HTTP request is made. EA's server advertises host names, and so must
/// the emulator: the local shim reroutes every `.ea.com` name to the
/// emulator (`shim/src/hooks_winsock.cpp`, `ShouldRerouteHost`), and the
/// emulator's certificate already carries `*.ea.com` and `*.ea.net` SANs.
#[test]
fn our_conf_map_carries_ea_s_hostnames_and_no_loopback_literal() {
    // EA's own Blaze 15 configuration map.
    let frame = EA_REPLIES
        .iter()
        .find(|(version, _)| *version == "Blaze 15.1.1.4.6")
        .expect("the Blaze 15 fixture")
        .1;
    let decoded = deserialize_tdf(&frame[16..]).expect("decode");
    let ea_conf = match decoded.get("CONF") {
        Some(TdfValue::Struct(s)) => s,
        other => panic!("EA's CONF is a struct, got {:?}", other),
    };
    let ea_map = match ea_conf.get("CONF") {
        Some(TdfValue::Map(m)) => m,
        other => panic!("EA's CONF holds a map, got {:?}", other),
    };
    let ea: Vec<(String, String)> = ea_map
        .entries
        .iter()
        .map(|(k, v)| match (k, v) {
            (TdfValue::String(k), TdfValue::String(v)) => (k.clone(), v.clone()),
            _ => panic!("EA's configuration entries must be string -> string"),
        })
        .collect();

    let ours = our_reply();
    let conf = match ours.get("CONF") {
        Some(TdfValue::Struct(s)) => s,
        other => panic!("our CONF must be a struct, got {:?}", other),
    };
    let our_map = match conf.get("CONF") {
        Some(TdfValue::Map(m)) => m,
        other => panic!("our CONF holds a map, got {:?}", other),
    };
    let our: Vec<(String, String)> = our_map
        .entries
        .iter()
        .map(|(k, v)| match (k, v) {
            (TdfValue::String(k), TdfValue::String(v)) => (k.clone(), v.clone()),
            _ => panic!("our configuration entries must be string -> string"),
        })
        .collect();

    // No value may be a loopback address.
    for (key, value) in &our {
        assert!(
            !value.contains("127.0.0.1") && !value.contains("localhost"),
            "configuration entry `{}` must not name a loopback address: {}",
            key,
            value
        );
    }

    // Every service host name EA sends must be sent verbatim.
    //
    // `bytevaultPort` is the one exception: EA's Bytevault listens on
    // 42210, and the shim rewrites the host but not the port, so the
    // emulator advertises the port its own HTTPS sink serves.
    for (key, ea_value) in &ea {
        if key == "bytevaultPort" {
            continue;
        }
        let ours_value = our
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str());
        assert_eq!(
            ours_value,
            Some(ea_value.as_str()),
            "configuration entry `{}` must carry EA's own value",
            key
        );
    }
}

