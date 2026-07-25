//! Deterministic random number generation.
//!
//! The game's replay guarantee rests on random streams that are identical
//! forever, across platforms and releases. The generator is the fleet's —
//! `vellum-rng`'s unified PCG32 construction, adopted under the fleet
//! decision `rng-unification-breaks-saves` (this replaced an in-crate
//! xoshiro256**; the golden-value tests below were re-pinned as part of
//! that deliberate break, and the snapshot format version gates out saves
//! from before it).
//!
//! What survives the migration unchanged is this game's *pattern*: streams
//! are *derived*, not shared. Each use site derives its own generator from
//! the campaign seed, a purpose label, and the stable identities involved
//! (typically an entity ID and the current day). Systems therefore cannot
//! perturb each other's sequences when code is added or reordered, and no
//! RNG state needs to live in snapshots. Purpose labels are hashed into the
//! stream selector, so they are *identities*, not names: renaming one
//! silently re-rolls every outcome it has ever produced. Labels are frozen
//! once written, even when the concept they refer to is renamed around them.

use serde::{Deserialize, Serialize};
use vellum_digest::fnv1a;
use vellum_rng::{Pcg32, split_mix_64};

/// A deterministic generator over the fleet's PCG32.
///
/// Serialisable so a stream can be persisted mid-use if a future system
/// needs that, though the intended pattern is fresh derivation per use.
/// `serde(transparent)` keeps the serialised shape exactly the shared
/// `{ state, inc }`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct DeterministicRng {
    inner: Pcg32,
}

impl DeterministicRng {
    /// Creates a generator from a bare seed on the default stream.
    pub fn from_seed(seed: u64) -> Self {
        Self {
            inner: Pcg32::seeded(seed, 0),
        }
    }

    /// Derives the stream for one purpose acting on specific subjects.
    ///
    /// `purpose` is a short stable label such as `"job-resolution"`; it and
    /// the subjects fold into the stream selector, so every (purpose,
    /// subjects) pair is its own independent sequence of the campaign seed.
    pub fn derive(campaign_seed: u64, purpose: &str, subjects: &[u64]) -> Self {
        let mut stream = fnv1a(purpose.as_bytes());
        for &subject in subjects {
            stream = split_mix_64(stream ^ subject);
        }
        Self {
            inner: Pcg32::seeded(campaign_seed, stream),
        }
    }

    /// The next raw 64-bit value, from two draws of the 32-bit generator.
    pub fn next_u64(&mut self) -> u64 {
        let high = u64::from(self.inner.next_u32());
        let low = u64::from(self.inner.next_u32());
        (high << 32) | low
    }

    /// A uniform value in `0..bound` without modulo bias.
    ///
    /// Game bounds are small — dice, permille, slice lengths, day spans —
    /// so the fleet's 32-bit draw carries them all; the u64 signature is
    /// this game's vocabulary.
    ///
    /// # Panics
    /// Panics if `bound` is zero or exceeds `u32::MAX` — both are always
    /// caller bugs.
    pub fn roll(&mut self, bound: u64) -> u64 {
        assert!(bound > 0, "roll bound must be positive");
        let bound = u32::try_from(bound).expect("roll bounds are game quantities, within u32");
        u64::from(self.inner.below(bound))
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
        self.inner.below(1000)
    }

    /// Whether a check with the given permille chance succeeds.
    pub fn check_permille(&mut self, chance: u32) -> bool {
        self.inner.chance(chance, 1000)
    }

    /// Fisher–Yates shuffle.
    pub fn shuffle<T>(&mut self, slice: &mut [T]) {
        self.inner.shuffle(slice);
    }

    /// A uniformly chosen element, or `None` if the slice is empty.
    pub fn pick<'a, T>(&mut self, slice: &'a [T]) -> Option<&'a T> {
        if slice.is_empty() {
            None
        } else {
            Some(&slice[self.inner.pick_index(slice.len())])
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Golden values locking the streams permanently: if this test ever
    /// fails, replay compatibility with existing campaigns has been broken.
    /// These were re-pinned once, deliberately, when the fleet RNG replaced
    /// the in-crate xoshiro256** — the break the snapshot format version
    /// gates out.
    #[test]
    fn from_seed_matches_golden_values() {
        let mut rng = DeterministicRng::from_seed(0x00C0_FFEE);
        assert_eq!(rng.next_u64(), 0x43d95d2a0d5301cd);
        assert_eq!(rng.next_u64(), 0xe8367bfbf9ec2845);
        assert_eq!(rng.next_u64(), 0xead79b823a4262a3);
        assert_eq!(rng.next_u64(), 0x3f5f5e2035d682e6);
    }

    #[test]
    fn derive_matches_golden_values() {
        let mut rng = DeterministicRng::derive(0x00C0_FFEE, "job-resolution", &[42, 7]);
        assert_eq!(rng.next_u64(), 0xb7fba1e7ed9831de);
        assert_eq!(rng.next_u64(), 0x0a272acedd60f950);
    }

    #[test]
    fn derived_streams_differ_by_subject_and_purpose() {
        let mut by_subject = DeterministicRng::derive(0x00C0_FFEE, "job-resolution", &[42, 8]);
        let mut by_purpose = DeterministicRng::derive(0x00C0_FFEE, "other-purpose", &[42, 7]);
        assert_ne!(by_subject.next_u64(), by_purpose.next_u64());
        let mut original = DeterministicRng::derive(0x00C0_FFEE, "job-resolution", &[42, 7]);
        let mut again = DeterministicRng::derive(0x00C0_FFEE, "job-resolution", &[42, 7]);
        assert_eq!(original.next_u64(), again.next_u64());
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
