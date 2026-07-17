//! Stable game identity.
//!
//! Every durable game entity carries a stable ID allocated once for the
//! lifetime of a campaign and never reused. Saves, command logs, scripts,
//! UI selection, and cross-system links hold stable IDs only; Bevy ECS
//! `Entity` handles are transient runtime details that must never leak into
//! anything durable.

use core::fmt;
use core::hash::{Hash, Hasher};
use core::marker::PhantomData;
use core::num::NonZeroU64;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Marker trait for a kind of stable ID (character, house, province, ...).
///
/// Implement via [`define_id_kind!`]; the kind name appears in debug output
/// and keeps IDs of different kinds from being confused at compile time.
pub trait IdKind: Copy + Clone + Eq + Ord + Hash + 'static {
    /// Short lowercase noun naming the kind, e.g. `"character"`.
    const KIND_NAME: &'static str;
}

/// Defines a stable-ID kind marker type.
///
/// ```
/// aeon_core::define_id_kind!(pub CharacterKind => "character");
/// type CharacterId = aeon_core::id::Id<CharacterKind>;
/// ```
#[macro_export]
macro_rules! define_id_kind {
    ($(#[$meta:meta])* $vis:vis $kind:ident => $name:literal) => {
        $(#[$meta])*
        #[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
        $vis struct $kind;

        impl $crate::id::IdKind for $kind {
            const KIND_NAME: &'static str = $name;
        }
    };
}

/// A stable, typed identifier for a durable game entity.
///
/// Serialises as a plain positive integer. Ordering is allocation order,
/// which is what deterministic iteration relies on.
pub struct Id<K: IdKind> {
    value: NonZeroU64,
    _kind: PhantomData<fn() -> K>,
}

impl<K: IdKind> Id<K> {
    /// Reconstructs an ID from its raw value (e.g. when deserialising).
    pub fn from_raw(value: u64) -> Option<Self> {
        NonZeroU64::new(value).map(|value| Self {
            value,
            _kind: PhantomData,
        })
    }

    /// The raw numeric value.
    pub fn raw(self) -> u64 {
        self.value.get()
    }
}

impl<K: IdKind> Copy for Id<K> {}

impl<K: IdKind> Clone for Id<K> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<K: IdKind> PartialEq for Id<K> {
    fn eq(&self, other: &Self) -> bool {
        self.value == other.value
    }
}

impl<K: IdKind> Eq for Id<K> {}

impl<K: IdKind> PartialOrd for Id<K> {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl<K: IdKind> Ord for Id<K> {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        self.value.cmp(&other.value)
    }
}

impl<K: IdKind> Hash for Id<K> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.value.hash(state);
    }
}

impl<K: IdKind> fmt::Debug for Id<K> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}#{}", K::KIND_NAME, self.value)
    }
}

impl<K: IdKind> fmt::Display for Id<K> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}#{}", K::KIND_NAME, self.value)
    }
}

impl<K: IdKind> Serialize for Id<K> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_u64(self.value.get())
    }
}

impl<'de, K: IdKind> Deserialize<'de> for Id<K> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = u64::deserialize(deserializer)?;
        Id::from_raw(raw).ok_or_else(|| serde::de::Error::custom("stable IDs must be non-zero"))
    }
}

/// Allocates stable IDs for a campaign.
///
/// One allocator serves every kind: raw values are unique across kinds, so a
/// mistyped cross-kind lookup can never silently resolve. The allocator is
/// part of the campaign snapshot.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct IdAllocator {
    next: u64,
}

impl IdAllocator {
    /// A fresh allocator for a new campaign.
    pub fn new() -> Self {
        Self { next: 1 }
    }

    /// Allocates the next stable ID of the given kind.
    pub fn allocate<K: IdKind>(&mut self) -> Id<K> {
        let id = Id::from_raw(self.next).expect("allocator next value is always non-zero");
        self.next += 1;
        id
    }
}

impl Default for IdAllocator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    define_id_kind!(TestCharacterKind => "character");
    define_id_kind!(TestProvinceKind => "province");

    type CharacterId = Id<TestCharacterKind>;
    type ProvinceId = Id<TestProvinceKind>;

    #[test]
    fn allocation_is_sequential_and_unique_across_kinds() {
        let mut alloc = IdAllocator::new();
        let a: CharacterId = alloc.allocate();
        let b: ProvinceId = alloc.allocate();
        let c: CharacterId = alloc.allocate();
        assert_eq!(a.raw(), 1);
        assert_eq!(b.raw(), 2);
        assert_eq!(c.raw(), 3);
        assert!(a < c);
    }

    #[test]
    fn ids_serialise_as_plain_integers() {
        let id: CharacterId = Id::from_raw(42).unwrap();
        assert_eq!(serde_json::to_string(&id).unwrap(), "42");
        let back: CharacterId = serde_json::from_str("42").unwrap();
        assert_eq!(back, id);
        assert!(serde_json::from_str::<CharacterId>("0").is_err());
    }

    #[test]
    fn display_names_the_kind() {
        let id: ProvinceId = Id::from_raw(7).unwrap();
        assert_eq!(id.to_string(), "province#7");
    }

    #[test]
    fn allocator_round_trips_through_serde() {
        let mut alloc = IdAllocator::new();
        let _: CharacterId = alloc.allocate();
        let json = serde_json::to_string(&alloc).unwrap();
        let mut restored: IdAllocator = serde_json::from_str(&json).unwrap();
        let next: CharacterId = restored.allocate();
        assert_eq!(next.raw(), 2);
    }
}
