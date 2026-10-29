//! # blaze-core
//!
//! Core framing, packet routing, and connection management for EA Blaze Protocol.

pub mod frame;
pub mod framing;

pub use frame::{hex_dump, BlazeHeader, BlazePacket, MessageType};
pub use framing::{
    detect_framing, fire2_frame_len, looks_like_fire2, read_fire2_frame, try_parse_fire2,
    try_parse_fire2_packet, try_parse_legacy_packet, try_parse_packet, Fire2Frame, Framing,
    MAX_FRAME_SIZE,
};
