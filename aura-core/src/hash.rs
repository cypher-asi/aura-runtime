//! Hashing utilities for the Aura system.
//!
//! Uses BLAKE3 for all hashing operations.

use crate::ids::TxId;

/// Hash arbitrary bytes using BLAKE3.
#[must_use]
pub fn hash_bytes(data: &[u8]) -> [u8; 32] {
    *blake3::hash(data).as_bytes()
}

/// Hash multiple byte slices together.
#[must_use]
pub fn hash_many(parts: &[&[u8]]) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    for part in parts {
        hasher.update(part);
    }
    *hasher.finalize().as_bytes()
}

/// Compute a context hash from transaction bytes and record window.
///
/// This is used to create a deterministic fingerprint of the inputs
/// used to make a decision.
#[must_use]
pub fn compute_context_hash(tx_bytes: &[u8], record_window_bytes: &[u8]) -> [u8; 32] {
    hash_many(&[tx_bytes, record_window_bytes])
}

/// Generate a transaction ID from its content.
#[must_use]
pub fn tx_id_from_content(content: &[u8]) -> TxId {
    TxId::new(hash_bytes(content))
}

/// Incremental hasher for building hashes from multiple parts.
pub struct Hasher {
    inner: blake3::Hasher,
}

impl Hasher {
    /// Create a new hasher.
    #[must_use]
    pub fn new() -> Self {
        Self {
            inner: blake3::Hasher::new(),
        }
    }

    /// Update the hasher with more data.
    pub fn update(&mut self, data: &[u8]) -> &mut Self {
        self.inner.update(data);
        self
    }

    /// Finalize and return the hash.
    #[must_use]
    pub fn finalize(self) -> [u8; 32] {
        *self.inner.finalize().as_bytes()
    }
}

impl Default for Hasher {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_bytes_deterministic() {
        let data = b"test data";
        let hash1 = hash_bytes(data);
        let hash2 = hash_bytes(data);
        assert_eq!(hash1, hash2);
    }

    #[test]
    fn hash_bytes_different_input() {
        let hash1 = hash_bytes(b"data1");
        let hash2 = hash_bytes(b"data2");
        assert_ne!(hash1, hash2);
    }

    #[test]
    fn hash_many_order_matters() {
        let hash1 = hash_many(&[b"part1", b"part2"]);
        let hash2 = hash_many(&[b"part2", b"part1"]);
        assert_ne!(hash1, hash2);
    }

    #[test]
    fn incremental_hasher() {
        let direct = hash_many(&[b"part1", b"part2"]);

        let mut hasher = Hasher::new();
        hasher.update(b"part1").update(b"part2");
        let incremental = hasher.finalize();

        assert_eq!(direct, incremental);
    }

    #[test]
    fn context_hash_deterministic() {
        let tx = b"transaction data";
        let window = b"record window data";

        let hash1 = compute_context_hash(tx, window);
        let hash2 = compute_context_hash(tx, window);
        assert_eq!(hash1, hash2);
    }
}
