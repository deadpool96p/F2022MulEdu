//! Session and room management for Seamless FIFA relay.

use std::net::SocketAddr;
use std::time::Instant;
use dashmap::DashMap;
use crate::protocol::{PEER_ID_GUEST, PEER_ID_HOST, PEER_ID_UNASSIGNED};

pub type RoomHash = [u8; 32];

#[derive(Debug, Clone)]
pub struct PeerEndpoint {
    pub addr: SocketAddr,
    pub last_seen: Instant,
    pub packets_relayed: u64,
}

#[derive(Debug, Clone)]
pub struct RoomSession {
    pub room_hash: RoomHash,
    pub created_at: Instant,
    pub last_activity: Instant,
    pub host: Option<PeerEndpoint>,
    pub guest: Option<PeerEndpoint>,
}

impl RoomSession {
    pub fn new(room_hash: RoomHash) -> Self {
        let now = Instant::now();
        Self {
            room_hash,
            created_at: now,
            last_activity: now,
            host: None,
            guest: None,
        }
    }

    /// Register or update peer in the room. Returns the assigned peer_id (0 for Host, 1 for Guest).
    pub fn register_peer(&mut self, addr: SocketAddr, requested_role: Option<u8>) -> u8 {
        self.last_activity = Instant::now();
        let now = Instant::now();

        // If client is already host or guest, refresh endpoint
        if let Some(host) = &mut self.host {
            if host.addr == addr {
                host.last_seen = now;
                return PEER_ID_HOST;
            }
        }
        if let Some(guest) = &mut self.guest {
            if guest.addr == addr {
                guest.last_seen = now;
                return PEER_ID_GUEST;
            }
        }

        // Assign based on requested role or availability
        if requested_role == Some(PEER_ID_HOST) || self.host.is_none() {
            if self.host.is_none() {
                self.host = Some(PeerEndpoint {
                    addr,
                    last_seen: now,
                    packets_relayed: 0,
                });
                return PEER_ID_HOST;
            }
        }

        if self.guest.is_none() {
            self.guest = Some(PeerEndpoint {
                addr,
                last_seen: now,
                packets_relayed: 0,
            });
            return PEER_ID_GUEST;
        }

        // Room full, re-bind to guest if stale
        if let Some(guest) = &mut self.guest {
            guest.addr = addr;
            guest.last_seen = now;
            return PEER_ID_GUEST;
        }

        PEER_ID_UNASSIGNED
    }

    /// Retrieve the destination socket address for a packet sent by `sender_addr`.
    pub fn get_target_for(&mut self, sender_addr: SocketAddr) -> Option<(SocketAddr, u8)> {
        self.last_activity = Instant::now();
        let now = Instant::now();

        if let Some(host) = &mut self.host {
            if host.addr == sender_addr {
                host.last_seen = now;
                host.packets_relayed += 1;
                return self.guest.as_ref().map(|g| (g.addr, PEER_ID_GUEST));
            }
        }

        if let Some(guest) = &mut self.guest {
            if guest.addr == sender_addr {
                guest.last_seen = now;
                guest.packets_relayed += 1;
                return self.host.as_ref().map(|h| (h.addr, PEER_ID_HOST));
            }
        }

        None
    }
}

pub struct RoomManager {
    rooms: DashMap<RoomHash, RoomSession>,
}

impl RoomManager {
    pub fn new() -> Self {
        Self {
            rooms: DashMap::new(),
        }
    }

    pub fn get_or_create(&self, room_hash: RoomHash) -> dashmap::mapref::one::RefMut<'_, RoomHash, RoomSession> {
        self.rooms.entry(room_hash).or_insert_with(|| RoomSession::new(room_hash))
    }

    pub fn remove_stale(&self, timeout_secs: u64) -> usize {
        let now = Instant::now();
        let mut to_remove = Vec::new();
        for entry in self.rooms.iter() {
            if now.duration_since(entry.value().last_activity).as_secs() > timeout_secs {
                to_remove.push(*entry.key());
            }
        }
        let count = to_remove.len();
        for k in to_remove {
            self.rooms.remove(&k);
        }
        count
    }
}

