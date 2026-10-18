//! # AssociationLists Component (0x0019) Handlers
//!
//! Exposes virtual online friends and association lists populated from the active
//! `SessionStore`, allowing native FIFA 22 friend pickers to discover connected peers.

use std::io;
use blaze_core::BlazePacket;
use blaze_tdf::{TdfList, TdfStruct, TdfType, TdfValue};
use crate::session_store::SessionStore;

pub const COMPONENT_ASSOCIATION_LISTS: u16 = 0x0019;

pub const CMD_GET_LISTS: u16 = 0x0001;
pub const CMD_SET_USERS_ATTRIBUTES: u16 = 0x0002;
pub const CMD_SUBSCRIBE_TO_LISTS: u16 = 0x0004;
pub const CMD_GET_MEMBERS: u16 = 0x0005;

/// Handles getLists (0x0001), subscribeToLists (0x0004), and getMembers (0x0005).
/// Returns active players registered in the SessionStore as virtual online friends.
pub fn handle_get_lists(
    req: &BlazePacket,
    current_user_id: u64,
    session_store: &SessionStore,
) -> io::Result<BlazePacket> {
    let mut resp = TdfStruct::new();

    // Build the friendList struct
    let mut friend_list_struct = TdfStruct::new();

    // List identification (LID)
    let mut lid_struct = TdfStruct::new();
    lid_struct.set("LNAM", TdfValue::String("friendList".to_string()));
    lid_struct.set("LTYP", TdfValue::Integer(1)); // 1 = Friends List
    friend_list_struct.set("LID", TdfValue::Struct(lid_struct));

    // Populate active online friends from SessionStore
    let all_players = session_store.list_active_sessions();
    let mut members = Vec::new();

    for player in all_players {
        if player.user_id == current_user_id {
            // Do not list self as friend
            continue;
        }

        let mut member_struct = TdfStruct::new();
        member_struct.set("BLID", TdfValue::Integer(0));
        member_struct.set("EID", TdfValue::Integer(player.user_id));
        member_struct.set("NAME", TdfValue::String(player.persona_name.clone()));
        member_struct.set("STUS", TdfValue::Integer(2)); // 2 = Online
        member_struct.set("TIMS", TdfValue::Integer(1700000000));
        member_struct.set("XREF", TdfValue::Integer(0));

        members.push(TdfValue::Struct(member_struct));
    }

    friend_list_struct.set("LMEM", TdfValue::List(TdfList::new(TdfType::Struct, members)));

    let lists = vec![TdfValue::Struct(friend_list_struct)];
    resp.set("LST", TdfValue::List(TdfList::new(TdfType::Struct, lists)));

    tracing::info!(
        "[AssociationLists] Served friend lists for user {}: {} peers online",
        current_user_id,
        session_store.count().saturating_sub(1)
    );

    BlazePacket::response(&req.header, resp)
}

#[cfg(test)]
mod tests {
    use super::*;
    use blaze_core::MessageType;

    #[test]
    fn test_virtual_friends_list_population() {
        let store = SessionStore::new();
        let u1 = store.create_session(Some("Alice"));
        let _u2 = store.create_session(Some("Bob"));
        let _u3 = store.create_session(Some("Charlie"));

        let req = BlazePacket::new(
            COMPONENT_ASSOCIATION_LISTS,
            CMD_GET_LISTS,
            0,
            MessageType::SubmitRequest,
            0,
            1,
            TdfStruct::new(),
        )
        .unwrap();

        let resp = handle_get_lists(&req, u1.user_id, &store).unwrap();
        assert_eq!(resp.header.component, COMPONENT_ASSOCIATION_LISTS);
        assert_eq!(resp.header.command, CMD_GET_LISTS);

        let lst = resp.payload.get("LST").unwrap();
        if let TdfValue::List(outer_list) = lst {
            assert_eq!(outer_list.items.len(), 1);
            if let TdfValue::Struct(friend_list) = &outer_list.items[0] {
                if let Some(TdfValue::List(members)) = friend_list.get("LMEM") {
                    // Should contain Bob and Charlie (2 peers, excluding Alice)
                    assert_eq!(members.items.len(), 2);
                    let mut names = Vec::new();
                    for m in &members.items {
                        if let TdfValue::Struct(st) = m {
                            if let Some(TdfValue::String(name)) = st.get("NAME") {
                                names.push(name.clone());
                            }
                        }
                    }
                    assert!(names.contains(&"Bob".to_string()));
                    assert!(names.contains(&"Charlie".to_string()));
                }
            }
        }
    }
}
