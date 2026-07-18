//! Stable-ID kinds for simulation entities.
//!
//! One kind per durable entity family. Raw values stay unique across kinds
//! (one allocator), so a cross-kind mix-up can never resolve to a live
//! entity.

use aeon_core::define_id_kind;
use aeon_core::id::Id;

define_id_kind!(pub BodyIdKind => "body");
define_id_kind!(pub ProvinceIdKind => "province");
define_id_kind!(pub CharacterIdKind => "character");
define_id_kind!(pub OrgIdKind => "org");
define_id_kind!(pub TitleIdKind => "title");
define_id_kind!(pub OfficeIdKind => "office");
define_id_kind!(pub JobIdKind => "job");

/// Stable ID of a celestial body.
pub type BodyId = Id<BodyIdKind>;
/// Stable ID of a province.
pub type ProvinceId = Id<ProvinceIdKind>;
/// Stable ID of a character.
pub type CharacterId = Id<CharacterIdKind>;
/// Stable ID of an organisation.
pub type OrgId = Id<OrgIdKind>;
/// Stable ID of a title.
pub type TitleId = Id<TitleIdKind>;
/// Stable ID of an office.
pub type OfficeId = Id<OfficeIdKind>;
/// Stable ID of an active job.
pub type JobId = Id<JobIdKind>;
