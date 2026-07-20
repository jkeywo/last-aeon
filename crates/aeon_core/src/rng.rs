//! Deterministic random number generation.
//!
//! The game's replay guarantee rests on random streams that are identical
//! forever, across platforms and releases. Depending on an external RNG
//! crate would tie that guarantee to someone else's versioning, so the
//! generator is implemented here: xoshiro256** seeded via splitmix64, with
//! golden-value tests locking the streams permanently.
//!
//! Streams are *derived*, not shared: each use site derives its own
//! generator from the campaign seed, a purpose label, and the stable
//! identities involved (typically an entity ID and the current day). Systems
//! therefore cannot perturb each other's sequences when code is added or
//! reordered, and no RNG state needs to live in snapshots.

use serde::{Deserialize, Serialize};

const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

fn fnv1a(bytes: &[u8]) -> u64 {
    let mut hash = FNV_OFFSET;
    for &byte in bytes {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    hash
}

/// Advances a splitmix64 state and returns the next output.
fn splitmix64_next(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// One splitmix64 step of a bare value, used when folding stream subjects.
fn splitmix64_mix(value: u64) -> u64 {
    let mut state = value;
    splitmix64_next(&mut state)
}

/// A deterministic xoshiro256** generator.
///
/// Serialisable so a stream can be persisted mid-use if a future system
/// needs that, though the intended pattern is fresh derivation per use.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeterministicRng {
    state: [u64; 4],
}

impl DeterministicRng {
    /// Creates a generator from a bare seed via splitmix64 expansion.
    pub fn from_seed(seed: u64) -> Self {
        let mut sm_state = seed;
        let mut state = [0u64; 4];
        for slot in &mut state {
            *slot = splitmix64_next(&mut sm_state);
        }
        if state == [0, 0, 0, 0] {
            // xoshiro must not start all-zero; unreachable in practice.
            state[0] = 1;
        }
        Self { state }
    }

    /// Derives the stream for one purpose acting on specific subjects.
    ///
    /// `purpose` is a short stable label such as `"job-resolution"`.
    ///
    /// It is hashed into the stream, so it is an *identity*, not a name:
    /// renaming one silently re-rolls every outcome it has ever produced.
    /// Labels are frozen once written, even when the concept they refer to
    /// is renamed around them.
    /// `subjects` are the stable numeric identities involved — typically an
    /// entity's raw ID and the current day — folded in order.
    pub fn derive(campaign_seed: u64, purpose: &str, subjects: &[u64]) -> Self {
        let mut hash = fnv1a(purpose.as_bytes());
        for &subject in subjects {
            hash = splitmix64_mix(hash ^ subject);
        }
        Self::from_seed(campaign_seed ^ hash)
    }

    /// The next raw 64-bit value.
    pub fn next_u64(&mut self) -> u64 {
        let result = self.state[1].wrapping_mul(5).rotate_left(7).wrapping_mul(9);
        let t = self.state[1] << 17;
        self.state[2] ^= self.state[0];
        self.state[3] ^= self.state[1];
        self.state[1] ^= self.state[2];
        self.state[0] ^= self.state[3];
        self.state[2] ^= t;
        self.state[3] = self.state[3].rotate_left(45);
        result
    }

    /// A uniform value in `0..bound` without modulo bias.
    ///
    /// # Panics
    /// Panics if `bound` is zero — that is always a caller bug.
    pub fn roll(&mut self, bound: u64) -> u64 {
        assert!(bound > 0, "roll bound must be positive");
        // Reject the top partial cycle so every residue is equally likely.
        let overhang = (u64::MAX % bound).wrapping_add(1) % bound;
        loop {
            let x = self.next_u64();
            if x <= u64::MAX - overhang {
                return x % bound;
            }
        }
    }

    /// A uniform value in the inclusive range `lo..=hi`.
    ///
    /// # Panics
    /// Panics if `lo > hi`.
    pub fn roll_range(&mut self, lo: i64, hi: i64) -> i64 {
        assert!(lo <= hi, "roll_range requires lo <= hi");
        let span = (i128::from(hi) - i128::from(lo) + 1) as u64;
        let offset = self.roll(span);
        (i128::from(lo) + i128::from(offset)) as i64
    }

    /// A uniform value in `0..1000`, the standard chance resolution.
    pub fn permille(&mut self) -> u32 {
        self.roll(1000) as u32
    }

    /// Whether a check with the given permille chance succeeds.
    pub fn check_permille(&mut self, chance: u32) -> bool {
        self.permille() < chance
    }

    /// Fisher–Yates shuffle.
    pub fn shuffle<T>(&mut self, slice: &mut [T]) {
        for i in (1..slice.len()).rev() {
            let j = self.roll(i as u64 + 1) as usize;
            slice.swap(i, j);
        }
    }

    /// A uniformly chosen element, or `None` if the slice is empty.
    pub fn pick<'a, T>(&mut self, slice: &'a [T]) -> Option<&'a T> {
        if slice.is_empty() {
            None
        } else {
            Some(&slice[self.roll(slice.len() as u64) as usize])
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Golden values computed by an independent reference implementation.
    /// These lock the streams permanently: if this test ever fails, replay
    /// compatibility with existing campaigns has been broken.
    #[test]
    fn from_seed_matches_golden_values() {
        let mut rng = DeterministicRng::from_seed(0x00C0_FFEE);
        assert_eq!(rng.next_u64(), 0x120e_99a6_dde4_a550);
        assert_eq!(rng.next_u64(), 0x8f98_9ef9_7733_d4b4);
        assert_eq!(rng.next_u64(), 0xf0a2_8eb2_e4fd_367b);
        assert_eq!(rng.next_u64(), 0x50c2_9bfe_8734_f5d2);
    }

    #[test]
    fn derive_matches_golden_values() {
        let mut rng = DeterministicRng::derive(0x00C0_FFEE, "job-resolution", &[42, 7]);
        assert_eq!(rng.next_u64(), 0xd835_499f_cb8a_bc6e);
        assert_eq!(rng.next_u64(), 0x3729_b541_07c4_fbad);
    }

    #[test]
    fn derived_streams_differ_by_subject_and_purpose() {
        let mut by_subject = DeterministicRng::derive(0x00C0_FFEE, "job-resolution", &[42, 8]);
        let mut by_purpose = DeterministicRng::derive(0x00C0_FFEE, "other-purpose", &[42, 7]);
        assert_eq!(by_subject.next_u64(), 0x252c_967f_0e3a_4b74);
        assert_eq!(by_purpose.next_u64(), 0x30d8_7264_bd59_7fa8);
    }

    #[test]
    fn roll_stays_in_bounds_and_covers_values() {
        let mut rng = DeterministicRng::from_seed(1);
        let mut seen = [false; 6];
        for _ in 0..1000 {
            let v = rng.roll(6);
            assert!(v < 6);
            seen[v as usize] = true;
        }
        assert!(seen.iter().all(|&s| s), "all faces should appear");
    }

    #[test]
    fn roll_range_is_inclusive_and_handles_negatives() {
        let mut rng = DeterministicRng::from_seed(2);
        let mut lo_seen = false;
        let mut hi_seen = false;
        for _ in 0..2000 {
            let v = rng.roll_range(-3, 3);
            assert!((-3..=3).contains(&v));
            lo_seen |= v == -3;
            hi_seen |= v == 3;
        }
        assert!(lo_seen && hi_seen);
    }

    #[test]
    fn shuffle_permutes_without_loss() {
        let mut rng = DeterministicRng::from_seed(3);
        let mut values: Vec<u32> = (0..20).collect();
        rng.shuffle(&mut values);
        let mut sorted = values.clone();
        sorted.sort_unstable();
        assert_eq!(sorted, (0..20).collect::<Vec<_>>());
    }

    #[test]
    fn generator_state_round_trips_through_serde() {
        let mut rng = DeterministicRng::from_seed(4);
        rng.next_u64();
        let json = serde_json::to_string(&rng).unwrap();
        let mut restored: DeterministicRng = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.next_u64(), rng.next_u64());
    }
}
