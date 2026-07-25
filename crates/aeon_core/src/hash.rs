//! Canonical state hashing.
//!
//! Replay verification compares campaign states by the fleet's digest —
//! FNV-1a 64 from `vellum-digest` — over a canonical serialisation. The
//! digest is defined identically everywhere, so a hash computed in a native
//! test, a wasm build, and CI must agree; any divergence is a determinism
//! bug, which is exactly what the hash exists to catch.
//!
//! This replaced SHA-256 under the fleet decision recorded in vellum's spec:
//! the hash is a determinism fingerprint, not a security boundary — the
//! threat is accident, not forgery — and the swap dropped the `sha2`
//! dependency from the wasm build. Snapshots written before the swap are
//! refused by the format version gate (and their 64-character digests no
//! longer even parse), never misread.

use core::fmt;
use core::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// An FNV-1a 64 digest of a canonical state serialisation.
///
/// Serialises as a lowercase hex string so it stays readable inside RON
/// snapshots and CLI output.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct StateHash(u64);

impl StateHash {
    /// The raw digest value.
    pub fn as_u64(&self) -> u64 {
        self.0
    }

    /// Wrap an already-computed fleet digest — for a hash produced by a
    /// shared crate over its own framing (see `aeon_data`'s content hash).
    pub const fn from_u64(digest: u64) -> Self {
        Self(digest)
    }
}

/// Hashes a canonical byte serialisation.
pub fn hash_bytes(bytes: &[u8]) -> StateHash {
    StateHash(vellum_digest::fnv1a(bytes))
}

impl fmt::Display for StateHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:016x}", self.0)
    }
}

/// A string that is not a 16-character lowercase hex digest.
#[derive(Debug, PartialEq, Eq, thiserror::Error)]
#[error("state hashes are 16 lowercase hex characters")]
pub struct InvalidStateHash;

impl FromStr for StateHash {
    type Err = InvalidStateHash;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.len() != 16 || s.bytes().any(|b| !matches!(b, b'0'..=b'9' | b'a'..=b'f')) {
            return Err(InvalidStateHash);
        }
        u64::from_str_radix(s, 16)
            .map(StateHash)
            .map_err(|_| InvalidStateHash)
    }
}

impl Serialize for StateHash {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for StateHash {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        text.parse().map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_the_published_fnv1a_test_vector() {
        // The canonical FNV-1a 64 vectors, so this really is the fleet's
        // digest and not merely self-consistent.
        assert_eq!(hash_bytes(b"").to_string(), "cbf29ce484222325");
        assert_eq!(hash_bytes(b"a").to_string(), "af63dc4c8601ec8c");
    }

    #[test]
    fn round_trips_through_string_and_serde() {
        let hash = hash_bytes(b"the last aeons");
        let parsed: StateHash = hash.to_string().parse().unwrap();
        assert_eq!(parsed, hash);
        let json = serde_json::to_string(&hash).unwrap();
        let back: StateHash = serde_json::from_str(&json).unwrap();
        assert_eq!(back, hash);
    }

    #[test]
    fn rejects_malformed_strings() {
        assert!("zz".parse::<StateHash>().is_err());
        assert!("CBF29CE484222325".parse::<StateHash>().is_err());
        // A pre-migration 64-character SHA-256 digest no longer parses:
        // old snapshots are refused, never misread.
        assert!(
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
                .parse::<StateHash>()
                .is_err()
        );
    }
}
