//! Stable-ID kinds for simulation entities.
//!
//! One kind per durable entity family. Raw values stay unique across kinds
//! (one allocator), so a cross-kind mix-up can never resolve to a live
//! entity.

use aeon_core::define_id_kind;
use aeon_core::id::Id;

define_id_kind!(pub BodyIdKind => "body");
define_id_kind!(pub ProvinceIdKind => "province");

/// Stable ID of a celestial body.
pub type BodyId = Id<BodyIdKind>;
/// Stable ID of a province.
pub type ProvinceId = Id<ProvinceIdKind>;
