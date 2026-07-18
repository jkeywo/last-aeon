//! The authored-content model.
//!
//! Definitions grow richer with each milestone; this module owns their
//! validated Rust shapes. Everything is ordered by [`ContentKey`] so
//! iteration over content is deterministic everywhere.

use std::collections::BTreeMap;

use aeon_core::hash::StateHash;
use serde::{Deserialize, Serialize};

use crate::key::ContentKey;

/// A reference to a named function in an authored script file.
///
/// Function references are file-local: a definition may only name functions
/// defined in its own file. That keeps every content file a self-contained
/// unit and makes cross-file behaviour impossible to author by accident.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScriptFnRef {
    /// Content-relative path of the defining file.
    pub path: String,
    /// The function name inside that file.
    pub name: String,
}

/// How a job behaves when it fails, and how much attention it demands.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum JobCategory {
    /// Fails cheaply and automatically retries; only time is lost.
    Routine,
    /// Failure creates a setback, disaster creates a new problem.
    Consequential,
}

/// The four graded outcomes of a job.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Debug, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum JobResultKind {
    /// Better than intended.
    CriticalSuccess,
    /// The job achieves its objective.
    Success,
    /// A setback (or a retry, for routine jobs).
    Failure,
    /// A severe failure creating a new problem.
    Disaster,
}

impl JobResultKind {
    /// All kinds, in canonical order.
    pub const ALL: [JobResultKind; 4] = [
        JobResultKind::CriticalSuccess,
        JobResultKind::Success,
        JobResultKind::Failure,
        JobResultKind::Disaster,
    ];
}

/// One possible result of a job.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct JobResultDef {
    /// Relative weight of this outcome before modifiers.
    pub weight: u32,
    /// Whether this result opens a player-facing popup.
    pub popup: bool,
    /// Whether this result is flagged for the notable-result message log.
    pub log: bool,
    /// Effect function applied when this result occurs.
    pub effect_fn: Option<ScriptFnRef>,
}

/// An authored job definition.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct JobDef {
    /// The job's stable content key.
    pub key: ContentKey,
    /// Short player-facing title.
    pub title: String,
    /// One-sentence player-facing summary.
    pub summary: String,
    /// Routine or consequential.
    pub category: JobCategory,
    /// Base duration in days.
    pub duration_days: u32,
    /// Possible outcomes, keyed by kind. Success and failure are mandatory.
    pub results: BTreeMap<JobResultKind, JobResultDef>,
}

/// What kind of celestial body a map body is.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum BodyKind {
    /// The system's world.
    Planet,
    /// A moon of a planet.
    Moon,
    /// An orbital starbase; a province in its own right.
    Starbase,
}

/// A celestial body in the local system.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BodyDef {
    /// The body's stable content key.
    pub key: ContentKey,
    /// Player-facing name.
    pub name: String,
    /// What kind of body this is.
    pub kind: BodyKind,
    /// Visual radius in kilometres (presentation scale, authored).
    pub radius_km: u32,
    /// Orbit radius around its parent in megametres.
    pub orbit_radius_mm: u32,
    /// Days for one full orbit around its parent; zero for the primary.
    pub orbit_days: u32,
    /// The body this one orbits, if any.
    pub parent: Option<ContentKey>,
}

/// A province on a body.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProvinceDef {
    /// The province's stable content key.
    pub key: ContentKey,
    /// Player-facing name.
    pub name: String,
    /// The body this province is on.
    pub body: ContentKey,
    /// Latitude of the province centre in millidegrees, -90000..=90000.
    pub latitude_mdeg: i32,
    /// Longitude of the province centre in millidegrees, -180000..180000.
    pub longitude_mdeg: i32,
}

/// Scenario metadata. Extended by the authored-scenario milestone.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScenarioDef {
    /// The scenario's stable content key.
    pub key: ContentKey,
    /// Player-facing scenario name.
    pub name: String,
    /// Campaign start year in the scenario's own numbering.
    pub start_year: i64,
    /// Campaign start month, 1..=12.
    pub start_month: u8,
    /// Campaign start day, 1..=30.
    pub start_day: u8,
}

/// A loaded, validated content database.
///
/// Definitions are behaviour-free data; the retained per-file ASTs hold the
/// named script functions definitions may reference.
pub struct ContentSet {
    /// Job definitions by key.
    pub jobs: BTreeMap<ContentKey, JobDef>,
    /// Celestial bodies by key.
    pub bodies: BTreeMap<ContentKey, BodyDef>,
    /// Provinces by key.
    pub provinces: BTreeMap<ContentKey, ProvinceDef>,
    /// The scenario, if this content set defines one.
    pub scenario: Option<ScenarioDef>,
    /// Compiled ASTs by content-relative path, for runtime function calls.
    pub asts: BTreeMap<String, rhai::AST>,
    /// Hash over all source files; binds snapshots to content.
    pub content_hash: StateHash,
}

impl ContentSet {
    /// Structural equality over the authored data (ASTs excluded; the
    /// content hash covers source identity).
    pub fn data_eq(&self, other: &ContentSet) -> bool {
        self.jobs == other.jobs
            && self.bodies == other.bodies
            && self.provinces == other.provinces
            && self.scenario == other.scenario
            && self.content_hash == other.content_hash
    }
}
