//! # Canonical Squad Database & Synchronization Store
//!
//! Tracks the canonical squad database binary and its SHA-256 hash.
//! Used during lobby negotiation (via `ATTR_SQDH` and HTTP) to guarantee
//! deterministic simulation lockstep.

use parking_lot::RwLock;
use sha2::{Digest, Sha256};
use std::sync::OnceLock;

/// Computes lowercase hex-encoded SHA-256 hash of a byte slice.
pub fn compute_sha256(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    let result = hasher.finalize();
    format!("{:x}", result)
}

/// Thread-safe canonical squad store.
pub struct SquadStore {
    canonical_data: RwLock<Vec<u8>>,
    canonical_hash: RwLock<String>,
}

impl Default for SquadStore {
    fn default() -> Self {
        Self::new()
    }
}

impl SquadStore {
    pub fn new() -> Self {
        // Check standard game asset directories for canonical layout/squad TOC
        let candidate_paths = [
            "G:\\Games\\FIFA 22\\Data\\layout.toc",
            "C:\\Program Files (x86)\\Steam\\steamapps\\common\\FIFA 22\\Data\\layout.toc",
            "Data\\layout.toc",
        ];
        let mut initial_data = None;
        for path in &candidate_paths {
            if let Ok(bytes) = std::fs::read(path) {
                if !bytes.is_empty() {
                    tracing::info!("[SquadStore] Initialized canonical database from game asset: {} ({} bytes)", path, bytes.len());
                    initial_data = Some(bytes);
                    break;
                }
            }
        }

        let default_data = initial_data.unwrap_or_else(|| b"FIFA_22_CANONICAL_SQUADS_DB_OFFICIAL_ROSTER_V1".to_vec());
        let default_hash = compute_sha256(&default_data);
        Self {
            canonical_data: RwLock::new(default_data),
            canonical_hash: RwLock::new(default_hash),
        }
    }

    /// Global singleton instance.
    pub fn global() -> &'static SquadStore {
        static INSTANCE: OnceLock<SquadStore> = OnceLock::new();
        INSTANCE.get_or_init(SquadStore::new)
    }

    /// Returns the active canonical squad SHA-256 hash and size in bytes.
    pub fn get_hash_and_size(&self) -> (String, usize) {
        let hash = self.canonical_hash.read().clone();
        let size = self.canonical_data.read().len();
        (hash, size)
    }

    /// Returns the active canonical squad SHA-256 hash.
    pub fn get_canonical_hash(&self) -> String {
        self.canonical_hash.read().clone()
    }

    /// Returns a copy of the canonical squad binary database.
    pub fn get_squad_bytes(&self) -> Vec<u8> {
        self.canonical_data.read().clone()
    }

    /// Updates the canonical squad database and returns its new SHA-256 hash and size.
    pub fn update_squad(&self, new_data: Vec<u8>) -> (String, usize) {
        let new_hash = compute_sha256(&new_data);
        let size = new_data.len();
        *self.canonical_data.write() = new_data;
        *self.canonical_hash.write() = new_hash.clone();
        tracing::info!(
            "[SquadStore] Updated canonical squad database: SHA256={}, size={} bytes",
            new_hash,
            size
        );
        (new_hash, size)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_squad_store_hash_and_update() {
        let store = SquadStore::new();
        let (initial_hash, initial_size) = store.get_hash_and_size();
        assert!(initial_size > 0);
        assert!(!initial_hash.is_empty());

        let custom_roster = b"CUSTOM_ROSTER_DATA_2022".to_vec();
        let expected_hash = compute_sha256(&custom_roster);
        let (updated_hash, updated_size) = store.update_squad(custom_roster.clone());

        assert_eq!(updated_hash, expected_hash);
        assert_eq!(updated_size, custom_roster.len());
        assert_eq!(store.get_canonical_hash(), expected_hash);
        assert_eq!(store.get_squad_bytes(), custom_roster);
    }
}
