//! Canonical state hashing.
//!
//! Replay verification compares campaign states by SHA-256 over a canonical
//! serialisation. SHA-256 is defined identically everywhere, so a hash
//! computed in a native test, a wasm build, and CI must agree — any
//! divergence is a determinism bug, which is exactly what the hash exists
//! to catch.

use core::fmt;
use core::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use sha2::{Digest, Sha256};

/// A SHA-256 digest of a canonical state serialisation.
///
/// Serialises as a lowercase hex string so it stays readable inside RON
/// snapshots and CLI output.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct StateHash([u8; 32]);

impl StateHash {
    /// The raw digest bytes.
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

/// Hashes a canonical byte serialisation.
pub fn hash_bytes(bytes: &[u8]) -> StateHash {
    let digest = Sha256::digest(bytes);
    StateHash(digest.into())
}

impl fmt::Display for StateHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for byte in &self.0 {
            write!(f, "{byte:02x}")?;
        }
        Ok(())
    }
}

/// A string that is not a 64-character lowercase hex digest.
#[derive(Debug, PartialEq, Eq, thiserror::Error)]
#[error("state hashes are 64 lowercase hex characters")]
pub struct InvalidStateHash;

impl FromStr for StateHash {
    type Err = InvalidStateHash;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.len() != 64 {
            return Err(InvalidStateHash);
        }
        let mut bytes = [0u8; 32];
        for (i, chunk) in s.as_bytes().chunks_exact(2).enumerate() {
            let high = hex_value(chunk[0]).ok_or(InvalidStateHash)?;
            let low = hex_value(chunk[1]).ok_or(InvalidStateHash)?;
            bytes[i] = (high << 4) | low;
        }
        Ok(StateHash(bytes))
    }
}

fn hex_value(c: u8) -> Option<u8> {
    match c {
        b'0'..=b'9' => Some(c - b'0'),
        b'a'..=b'f' => Some(c - b'a' + 10),
        _ => None,
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
    fn matches_the_published_sha256_test_vector() {
        let hash = hash_bytes(b"abc");
        assert_eq!(
            hash.to_string(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
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
        assert!(
            "ZA7816BF8F01CFEA414140DE5DAE2223B00361A396177A9CB410FF61F20015AD"
                .parse::<StateHash>()
                .is_err()
        );
    }
}
