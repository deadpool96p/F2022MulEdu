//! # blaze-core
//!
//! Core framing, packet routing, and connection management for EA Blaze Protocol.

pub mod frame;

pub use frame::{BlazeHeader, BlazePacket, MessageType};
