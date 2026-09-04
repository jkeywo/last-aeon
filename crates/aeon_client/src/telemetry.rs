//! Opt-in, presentation-owned onboarding telemetry.
//!
//! Nothing here is authoritative. The buffer is a client resource in
//! `aeon_client`, so it is structurally outside `CampaignState`: the
//! snapshot enumerates `aeon_sim` resources by hand and cannot reach a
//! client crate. Telemetry issues no command, derives no random stream,
//! reads no simulation resource mutably, is never consulted by autosave or
//! load, and contributes nothing to the state hash. It records only what
//! the player has already seen on their own screen; it emits nothing back
//! into the fiction, so it is outside the in-fiction information contract
//! entirely.
//!
//! Consent is the gate, and it fails safe. Absent, corrupt, inaccessible,
//! or future-version consent documents all mean *not consented*, and
//! [`OnboardingTelemetry::record`] is a no-op until the player has opted
//! in. Withdrawing consent clears the session buffer and erases the stored
//! document, so a player who opts out leaves nothing behind.

use std::collections::BTreeSet;

use bevy::prelude::*;
use bevy_egui::egui;
use serde::{Deserialize, Serialize};

use crate::preferences::{DocumentStore, UiPreferences};

/// Version of the stored consent document.
const CONSENT_DOCUMENT_VERSION: u32 = 1;
/// Version stamped on the first line of the captured-event document.
const TELEMETRY_DOCUMENT_VERSION: u32 = 1;

#[cfg(not(target_arch = "wasm32"))]
const CONSENT_FILENAME: &str = "onboarding-consent.json";
#[cfg(not(target_arch = "wasm32"))]
const TELEMETRY_FILENAME: &str = "onboarding-telemetry.jsonl";

#[cfg(any(test, target_arch = "wasm32"))]
const CONSENT_STORAGE_KEY: &str = "last-aeon.onboarding.consent";
#[cfg(any(test, target_arch = "wasm32"))]
const TELEMETRY_STORAGE_KEY: &str = "last-aeon.onboarding.telemetry";

/// The three household demands whose resolutions are reported separately
/// from ordinary objective reach.
pub(crate) const HOUSEHOLD_DEMANDS: [&str; 3] =
    ["kessarin-order", "aleyn-levies", "torvald-standing"];

/// Whether the player has agreed to local onboarding measurement.
///
/// `Unset` and `Declined` are both "not consented"; they are kept apart
/// only so a stored decline is not mistaken for a document that has never
/// been written. Anything that cannot be decoded becomes `Unset`.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TelemetryConsent {
    /// The player has never answered. Nothing is recorded.
    #[default]
    Unset,
    /// The player opted in. Events may be recorded.
    Granted,
    /// The player opted out, or withdrew. Nothing is recorded.
    Declined,
}

impl TelemetryConsent {
    /// The single question every capture site asks.
    pub fn records(self) -> bool {
        matches!(self, Self::Granted)
    }
}

/// One thing the player did, in the terms the onboarding programme asks
/// about. Every field is a content key, an interface role, or a boolean —
/// never a free-text player input, an identity, or a machine fact.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum OnboardingEvent {
    /// The First Reign guidance preference was turned on or off.
    GuidanceChoice { enabled: bool },
    /// Campaign time ran for the first time in this session.
    FirstUnpause,
    /// An authored Situation the player was working on reached an
    /// authoritative resolution, observed as a resolution notice newly
    /// appearing in the projected panel view.
    ObjectiveReached { definition: String, outcome: String },
    /// The same, for one of the three household demands.
    HouseholdOutcome { definition: String, outcome: String },
    /// The player pinned an explanation snapshot. `subject` is the
    /// explanation's non-identifying discriminator — a content key or an
    /// interface role, never the displayed title, which can be a
    /// character's name. `forecast` says whether it carried authoritative
    /// forecast numbers or was prose-only help.
    ForecastInspected { subject: String, forecast: bool },
    /// The client recomputed a forecast for a new assignment, leader, or
    /// target — the act of comparing candidates before committing.
    ForecastCompared {
        assignment: String,
        leader_chosen: bool,
    },
    /// A queued UI command was accepted by the authoritative pipeline.
    CommandAccepted { command: String },
    /// A queued UI command was refused by ordinary validation. `reason` is
    /// the rejection's variant path, never its rendered message: a message
    /// can interpolate an authored or scripted string, and a variant name
    /// cannot.
    CommandRefused { command: String, reason: String },
    /// The player interacted with a Situation card. `interaction` is the
    /// interface role plus the authored key it names.
    SituationInteraction {
        definition: String,
        interaction: String,
    },
    /// The player returned to a consequence they had already been shown.
    ConsequenceRevisit {
        definition: String,
        occasion: String,
    },
}

/// One captured event with its position in the session and the campaign
/// day it happened on.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TelemetryRecord {
    /// Monotonic within the stored document.
    pub sequence: u64,
    /// Campaign date as displayed, when a campaign was running.
    pub date: Option<String>,
    /// What happened.
    pub event: OnboardingEvent,
}

/// Header stamped on the first line of the captured-event document, so a
/// document from an unsupported version is discarded rather than misread.
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
struct TelemetryHeader {
    telemetry_version: u32,
}

/// The stored consent document.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct ConsentDocument {
    version: u32,
    consent: TelemetryConsent,
}

/// Client-owned onboarding measurement. Presentation state only.
#[derive(Resource, Debug, Default)]
pub struct OnboardingTelemetry {
    /// The gate. Defaults to [`TelemetryConsent::Unset`] — not consented.
    consent: TelemetryConsent,
    /// Captured events, oldest first.
    records: Vec<TelemetryRecord>,
    /// Next sequence number, continuing any document already on disk.
    next_sequence: u64,
    /// A withdrawal is waiting to erase the stored document.
    erase_pending: bool,
    /// Campaign time has run at least once this session.
    unpaused: bool,
    /// Resolution notices already reported as reached.
    seen_resolutions: BTreeSet<u64>,
    /// Resolution notices projected on the previous frame.
    present_resolutions: BTreeSet<u64>,
    /// Whether the outcome observer has taken its baseline for the campaign
    /// now running. Until it has, the first projection it sees is seeded
    /// rather than reported.
    primed: bool,
}

impl OnboardingTelemetry {
    /// The player's current answer.
    pub fn consent(&self) -> TelemetryConsent {
        self.consent
    }

    /// Everything captured this session, oldest first.
    pub fn records(&self) -> &[TelemetryRecord] {
        &self.records
    }

    /// Whether a resolution notice has already been reported. Capture sites
    /// use this to tell a first sighting from a revisit.
    pub fn has_seen_resolution(&self, notice: u64) -> bool {
        self.seen_resolutions.contains(&notice)
    }

    /// A campaign has just been started or restored: forget what the
    /// outcome observer saw of the previous one.
    ///
    /// Resolution notices are authoritative state that survives in the
    /// save, so a restored campaign arrives carrying every notice the
    /// player never dismissed. Without this reset the observer would either
    /// re-report those as freshly reached, or — for a second campaign in
    /// the same process — mistake the previous run's notice identities for
    /// this one's and suppress genuine reaches. The client still derives no
    /// rule of its own: it simply takes a new baseline before diffing.
    pub fn begin_campaign_session(&mut self) {
        self.seen_resolutions.clear();
        self.present_resolutions.clear();
        self.primed = false;
    }

    /// Records one event, if and only if the player has opted in.
    ///
    /// The consent gate lives here rather than at each call site, so a
    /// capture site can never be the thing that forgets to ask.
    pub fn record(&mut self, date: Option<String>, event: OnboardingEvent) {
        if !self.consent.records() {
            debug_assert!(
                self.records.is_empty(),
                "no onboarding event may be buffered without consent"
            );
            return;
        }
        self.next_sequence += 1;
        self.records.push(TelemetryRecord {
            sequence: self.next_sequence,
            date,
            event,
        });
    }

    /// The player opts in. Recording starts from here; nothing before it
    /// was captured.
    pub fn grant(&mut self) {
        self.consent = TelemetryConsent::Granted;
    }

    /// The player opts out or withdraws.
    ///
    /// Withdrawal clears rather than keeps: the acceptance rule is that the
    /// player opts in *before* anything is recorded, so a withdrawal must
    /// leave the same nothing behind that a refusal would have.
    pub fn withdraw(&mut self) {
        self.consent = TelemetryConsent::Declined;
        self.clear();
    }

    /// Discards every captured event and schedules erasure of the stored
    /// document. Leaves consent alone.
    pub fn clear(&mut self) {
        self.records.clear();
        self.next_sequence = 0;
        self.erase_pending = true;
    }

    /// The captured events as the exported document: a version header line
    /// followed by one JSON object per event.
    pub fn to_jsonl(&self) -> String {
        let mut out = serde_json::to_string(&TelemetryHeader {
            telemetry_version: TELEMETRY_DOCUMENT_VERSION,
        })
        .unwrap_or_else(|_| format!("{{\"telemetry_version\":{TELEMETRY_DOCUMENT_VERSION}}}"));
        for record in &self.records {
            out.push('\n');
            match serde_json::to_string(record) {
                Ok(line) => out.push_str(&line),
                Err(_) => out.push_str("{}"),
            }
        }
        out
    }
}

/// Reads a captured-event document. A missing header, an unsupported
/// version, or an unreadable line yields no events rather than a guess.
fn decode_events(document: &str) -> Vec<TelemetryRecord> {
    let mut lines = document.lines();
    let Some(header) = lines
        .next()
        .and_then(|line| serde_json::from_str::<TelemetryHeader>(line).ok())
    else {
        return Vec::new();
    };
    if header.telemetry_version != TELEMETRY_DOCUMENT_VERSION {
        return Vec::new();
    }
    lines
        .filter(|line| !line.trim().is_empty())
        .map(serde_json::from_str::<TelemetryRecord>)
        .collect::<Result<Vec<_>, _>>()
        .unwrap_or_default()
}

fn encode_consent(consent: TelemetryConsent) -> Result<String, String> {
    serde_json::to_string_pretty(&ConsentDocument {
        version: CONSENT_DOCUMENT_VERSION,
        consent,
    })
    .map_err(|error| error.to_string())
}

/// Decodes a consent document. Every failure path is *not consented*.
fn decode_consent(document: &str) -> TelemetryConsent {
    let Ok(document) = serde_json::from_str::<ConsentDocument>(document) else {
        return TelemetryConsent::Unset;
    };
    if document.version != CONSENT_DOCUMENT_VERSION {
        return TelemetryConsent::Unset;
    }
    document.consent
}

/// Loads consent from a store, failing safe in every direction.
fn load_consent_from(store: &impl DocumentStore) -> TelemetryConsent {
    store
        .load()
        .ok()
        .flatten()
        .map(|document| decode_consent(&document))
        .unwrap_or_default()
}

fn save_consent_to(store: &impl DocumentStore, consent: TelemetryConsent) -> Result<(), String> {
    store.save(&encode_consent(consent)?)
}

fn load_events_from(store: &impl DocumentStore) -> Vec<TelemetryRecord> {
    store
        .load()
        .ok()
        .flatten()
        .map(|document| decode_events(&document))
        .unwrap_or_default()
}

fn save_events_to(store: &impl DocumentStore, document: &str) -> Result<(), String> {
    store.save(document)
}

// --- platform stores -------------------------------------------------------

#[cfg(not(target_arch = "wasm32"))]
fn consent_store() -> Result<crate::preferences::NativeStore, String> {
    Ok(crate::preferences::NativeStore::at(
        crate::preferences::native_config_root()?.join(CONSENT_FILENAME),
    ))
}

#[cfg(not(target_arch = "wasm32"))]
fn events_store() -> Result<crate::preferences::NativeStore, String> {
    Ok(crate::preferences::NativeStore::at(
        crate::preferences::native_config_root()?.join(TELEMETRY_FILENAME),
    ))
}

#[cfg(target_arch = "wasm32")]
fn consent_store()
-> Result<crate::preferences::KeyValueStore<crate::preferences::BrowserBackend>, String> {
    Ok(crate::preferences::KeyValueStore {
        backend: crate::preferences::BrowserBackend,
        key: CONSENT_STORAGE_KEY,
    })
}

#[cfg(target_arch = "wasm32")]
fn events_store()
-> Result<crate::preferences::KeyValueStore<crate::preferences::BrowserBackend>, String> {
    Ok(crate::preferences::KeyValueStore {
        backend: crate::preferences::BrowserBackend,
        key: TELEMETRY_STORAGE_KEY,
    })
}

/// Where a maintainer finds the exported document on this build.
pub fn export_location() -> String {
    #[cfg(not(target_arch = "wasm32"))]
    {
        crate::preferences::native_config_root()
            .map(|root| root.join(TELEMETRY_FILENAME).display().to_string())
            .unwrap_or_else(|error| error)
    }
    #[cfg(target_arch = "wasm32")]
    {
        format!("localStorage: {TELEMETRY_STORAGE_KEY}")
    }
}

// --- systems ---------------------------------------------------------------

/// Loads consent once, then any events already stored for a consenting
/// player so the review surface shows the whole local history.
pub fn load_telemetry(mut telemetry: ResMut<OnboardingTelemetry>) {
    telemetry.consent = consent_store()
        .map(|store| load_consent_from(&store))
        .unwrap_or_default();
    if !telemetry.consent.records() {
        return;
    }
    let stored = events_store()
        .map(|store| load_events_from(&store))
        .unwrap_or_default();
    telemetry.next_sequence = stored.last().map(|record| record.sequence).unwrap_or(0);
    telemetry.records = stored;
}

/// Persists consent and the captured document when either has changed.
///
/// Never panics and never blocks the frame on failure: a store that cannot
/// be written warns exactly like the preference document does.
pub fn persist_telemetry(
    mut telemetry: ResMut<OnboardingTelemetry>,
    mut last: Local<Option<(TelemetryConsent, usize)>>,
) {
    let state = (telemetry.consent, telemetry.records.len());
    let erase = telemetry.erase_pending;
    if *last == Some(state) && !erase {
        return;
    }
    match consent_store().and_then(|store| save_consent_to(&store, telemetry.consent)) {
        Ok(()) => {}
        Err(error) => warn!("onboarding consent could not be saved: {error}"),
    }
    // A document is only written once there is something to write, or once
    // a withdrawal has asked for the stored one to be erased. A player who
    // never opts in leaves no telemetry file behind at all.
    if !telemetry.records.is_empty() || erase {
        let document = if telemetry.consent.records() {
            telemetry.to_jsonl()
        } else {
            String::new()
        };
        match events_store().and_then(|store| save_events_to(&store, &document)) {
            Ok(()) => telemetry.erase_pending = false,
            Err(error) => warn!("onboarding telemetry could not be saved: {error}"),
        }
    }
    *last = Some(state);
}

fn today(clock: Option<&aeon_sim::CampaignClock>) -> Option<String> {
    clock.map(|clock| clock.date.to_string())
}

/// Records a change to the First Reign guidance preference.
///
/// The first observation only remembers the current value: an existing
/// setting the player has not touched is not a choice they just made.
pub fn observe_guidance_choice(
    preferences: Res<UiPreferences>,
    clock: Option<Res<aeon_sim::CampaignClock>>,
    mut telemetry: ResMut<OnboardingTelemetry>,
    mut last: Local<Option<bool>>,
) {
    let enabled = preferences.guidance;
    match *last {
        Some(previous) if previous == enabled => {}
        Some(_) => telemetry.record(
            today(clock.as_deref()),
            OnboardingEvent::GuidanceChoice { enabled },
        ),
        None => {}
    }
    *last = Some(enabled);
}

/// Records the first time campaign time runs in this session.
pub fn observe_first_unpause(
    control: Res<crate::sim_driver::TimeControl>,
    clock: Option<Res<aeon_sim::CampaignClock>>,
    mut telemetry: ResMut<OnboardingTelemetry>,
) {
    if control.paused || telemetry.unpaused {
        return;
    }
    // The flag is set whether or not consent allows recording, so opting in
    // later never back-fills an unpause the player already made.
    telemetry.unpaused = true;
    telemetry.record(today(clock.as_deref()), OnboardingEvent::FirstUnpause);
}

/// Records a pinned explanation as an inspection of what it explains.
pub fn observe_explanation_pins(
    explanations: Res<crate::ui::explanations::ExplanationState>,
    clock: Option<Res<aeon_sim::CampaignClock>>,
    mut telemetry: ResMut<OnboardingTelemetry>,
    mut last: Local<Option<String>>,
) {
    // The pinned topic's *subject*, never its title: a title is display
    // copy and on the candidate-comparison path it is a person's name.
    let current = explanations
        .pinned
        .as_ref()
        .map(|topic| (topic.subject.clone(), topic.forecast.is_some()));
    match current {
        Some((subject, forecast)) if last.as_deref() != Some(subject.as_str()) => {
            telemetry.record(
                today(clock.as_deref()),
                OnboardingEvent::ForecastInspected {
                    subject: subject.clone(),
                    forecast,
                },
            );
            *last = Some(subject);
        }
        Some(_) => {}
        None => *last = None,
    }
}

/// Records each new assignment/leader/target combination the client
/// forecasts — the act of comparing candidates before committing.
pub fn observe_forecast_comparison(
    cache: Res<crate::forecast_view::ForecastCache>,
    clock: Option<Res<aeon_sim::CampaignClock>>,
    mut telemetry: ResMut<OnboardingTelemetry>,
    mut last: Local<Option<String>>,
) {
    // The cache also keys on the campaign day; a comparison is the subject
    // changing, not the calendar turning.
    let subject = cache.subject().map(|(assignment, leader, target)| {
        (
            assignment.to_string(),
            format!("{assignment}|{leader:?}|{target:?}"),
            leader.is_some(),
        )
    });
    match subject {
        Some((assignment, signature, leader_chosen)) => {
            if last.as_deref() != Some(signature.as_str()) {
                telemetry.record(
                    today(clock.as_deref()),
                    OnboardingEvent::ForecastCompared {
                        assignment,
                        leader_chosen,
                    },
                );
                *last = Some(signature);
            }
        }
        None => *last = None,
    }
}

/// Reports objective reach, household outcomes, and reappearing
/// consequences by diffing the projected Situation panel view.
///
/// The client owns no rule here: it compares this frame's authoritative
/// projection against the last one it saw and reports what newly appeared.
pub fn observe_situation_outcomes(
    view: Res<crate::ui::situations_panel::SituationPanelView>,
    clock: Option<Res<aeon_sim::CampaignClock>>,
    mut telemetry: ResMut<OnboardingTelemetry>,
) {
    // The first projection after a campaign is started or restored is the
    // baseline, not a report: a restored save carries every resolution the
    // player left undismissed, and those were reached in an earlier
    // session. Priming keeps the rule out of the client — nothing here
    // decides *when* an outcome happened, only what is newly on screen.
    if !telemetry.primed {
        telemetry.seen_resolutions = view
            .resolutions
            .iter()
            .map(|notice| notice.resolution.id)
            .collect();
        telemetry.present_resolutions = telemetry.seen_resolutions.clone();
        telemetry.primed = true;
        return;
    }
    let date = today(clock.as_deref());
    let mut present = BTreeSet::new();
    let mut newly_reached = Vec::new();
    let mut reappeared = Vec::new();
    for notice in &view.resolutions {
        let id = notice.resolution.id;
        present.insert(id);
        let definition = notice.resolution.situation.definition.to_string();
        if !telemetry.seen_resolutions.contains(&id) {
            newly_reached.push((id, definition, notice.resolution.outcome.to_string()));
        } else if !telemetry.present_resolutions.contains(&id) {
            reappeared.push(definition);
        }
    }
    for (id, definition, outcome) in newly_reached {
        telemetry.seen_resolutions.insert(id);
        let event = if HOUSEHOLD_DEMANDS.contains(&definition.as_str()) {
            OnboardingEvent::HouseholdOutcome {
                definition,
                outcome,
            }
        } else {
            OnboardingEvent::ObjectiveReached {
                definition,
                outcome,
            }
        };
        telemetry.record(date.clone(), event);
    }
    for definition in reappeared {
        telemetry.record(
            date.clone(),
            OnboardingEvent::ConsequenceRevisit {
                definition,
                occasion: "reappeared".to_owned(),
            },
        );
    }
    telemetry.present_resolutions = present;
}

// --- capture helpers for interactive surfaces ------------------------------

/// Records one Situation-card interaction: an action, a recorded response,
/// a guidance help route, or a dismissal.
pub fn record_situation_interaction(
    telemetry: &mut OnboardingTelemetry,
    date: Option<String>,
    definition: &aeon_data::ContentKey,
    interaction: impl Into<String>,
) {
    telemetry.record(
        date,
        OnboardingEvent::SituationInteraction {
            definition: definition.to_string(),
            interaction: interaction.into(),
        },
    );
}

/// Records a deliberate return to a consequence already shown.
pub fn record_consequence_revisit(
    telemetry: &mut OnboardingTelemetry,
    date: Option<String>,
    definition: &aeon_data::ContentKey,
    occasion: &str,
) {
    telemetry.record(
        date,
        OnboardingEvent::ConsequenceRevisit {
            definition: definition.to_string(),
            occasion: occasion.to_owned(),
        },
    );
}

/// The stable variant name of a command, with no identities or arguments.
pub(crate) fn command_kind(command: &aeon_sim::PlayerCommand) -> String {
    let rendered = format!("{command:?}");
    rendered
        .split(|character: char| !character.is_alphanumeric())
        .next()
        .unwrap_or("Unknown")
        .to_owned()
}

/// The stable variant path of a rejection, with no message text.
///
/// Rendered rejection messages interpolate authored and scripted strings —
/// `SituationError::BadSubject` carries whatever the sandbox boundary
/// returned — so the reason is taken from the `Debug` shape instead and cut
/// at the first token that is not a variant name, giving at most
/// `Outer.Inner`.
pub(crate) fn rejection_kind(rejection: &aeon_sim::CommandRejection) -> String {
    let rendered = format!("{rejection:?}");
    let path: Vec<&str> = rendered
        .split(|character: char| !character.is_alphanumeric())
        .filter(|token| !token.is_empty())
        .take_while(|token| token.starts_with(char::is_uppercase))
        .take(2)
        .collect();
    if path.is_empty() {
        "Unknown".to_owned()
    } else {
        path.join(".")
    }
}

/// Records the outcome of one submitted command from the exclusive flush
/// system, where the world rather than a resource reference is to hand.
pub(crate) fn record_command_outcome(
    world: &mut World,
    command: &aeon_sim::PlayerCommand,
    rejection: Option<&aeon_sim::CommandRejection>,
) {
    let date = world
        .get_resource::<aeon_sim::CampaignClock>()
        .map(|clock| clock.date.to_string());
    let kind = command_kind(command);
    let Some(mut telemetry) = world.get_resource_mut::<OnboardingTelemetry>() else {
        return;
    };
    let event = match rejection {
        Some(rejection) => OnboardingEvent::CommandRefused {
            command: kind,
            reason: rejection_kind(rejection),
        },
        None => OnboardingEvent::CommandAccepted { command: kind },
    };
    telemetry.record(date, event);
}

// --- consent and review surface -------------------------------------------

/// Draws the consent tickbox and the local review surface beneath the
/// interface preferences, on the title screen and in campaign settings.
pub fn draw_consent_controls(
    ui: &mut egui::Ui,
    strings: &aeon_sim::TextDb,
    telemetry: &mut OnboardingTelemetry,
) {
    let mut consented = telemetry.consent().records();
    let response = ui.checkbox(
        &mut consented,
        strings.text("ui.preferences.telemetry").to_owned(),
    );
    crate::ui::keyboard::capture_action(
        ui,
        crate::ui::keyboard::LogicalFocus::new("preference-telemetry"),
        "preference-telemetry",
        crate::ui::keyboard::FocusBand::Floating,
        &response,
    )
    .register();
    if response.changed() {
        if consented {
            telemetry.grant();
        } else {
            telemetry.withdraw();
        }
    }
    ui.weak(strings.text("ui.preferences.telemetry-note"));
    ui.weak(strings.format(
        "ui.preferences.telemetry-location",
        &[("location", &export_location())],
    ));

    let captured = telemetry.records().len();
    egui::CollapsingHeader::new(strings.text("ui.preferences.telemetry-review"))
        .id_salt("onboarding-telemetry-review")
        .show(ui, |ui| {
            if captured == 0 {
                ui.weak(strings.text("ui.preferences.telemetry-empty"));
                return;
            }
            ui.weak(strings.format(
                "ui.preferences.telemetry-count",
                &[("count", &captured.to_string())],
            ));
            let document = telemetry.to_jsonl();
            egui::ScrollArea::vertical()
                .id_salt("onboarding-telemetry-scroll")
                .max_height(180.0)
                .show(ui, |ui| {
                    // Read-only but selectable, so the whole document can be
                    // copied out of a browser where there is no file to
                    // open. A `&str` buffer is egui's read-only text
                    // buffer: `interactive(false)` would disable selection
                    // along with editing, which would leave the web build
                    // with no export path at all.
                    let mut view = document.as_str();
                    let text =
                        ui.add(egui::TextEdit::multiline(&mut view).desired_width(f32::INFINITY));
                    // Selectable text is a focusable, so it is registered
                    // like every other one the keyboard audit expects.
                    crate::ui::keyboard::capture_action(
                        ui,
                        crate::ui::keyboard::LogicalFocus::new("preference-telemetry-document"),
                        "preference-telemetry-document",
                        crate::ui::keyboard::FocusBand::Floating,
                        &text,
                    )
                    .register();
                });
            // Selection alone is a fiddly export for a long document, so the
            // whole of it goes to the clipboard on one keyboard-reachable
            // button. This is egui's own clipboard output — no web-sys
            // download path, no anchor, no Blob.
            let copy = ui.button(strings.text("ui.preferences.telemetry-copy"));
            crate::ui::keyboard::capture_action(
                ui,
                crate::ui::keyboard::LogicalFocus::new("preference-telemetry-copy"),
                "preference-telemetry-copy",
                crate::ui::keyboard::FocusBand::Floating,
                &copy,
            )
            .register();
            if copy.clicked() {
                ui.ctx().copy_text(document);
            }
            let clear = ui.button(strings.text("ui.preferences.telemetry-clear"));
            crate::ui::keyboard::capture_action(
                ui,
                crate::ui::keyboard::LogicalFocus::new("preference-telemetry-clear"),
                "preference-telemetry-clear",
                crate::ui::keyboard::FocusBand::Floating,
                &clear,
            )
            .register();
            if clear.clicked() {
                telemetry.clear();
            }
        });
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::collections::BTreeMap;
    #[cfg(not(target_arch = "wasm32"))]
    use std::sync::atomic::{AtomicU64, Ordering};

    use super::*;
    use bevy::ecs::system::RunSystemOnce;

    use crate::preferences::{KeyValueBackend, KeyValueStore};
    use aeon_core::calendar::CalendarDate;
    use aeon_sim::{CampaignConfig, PlayerCommand, SimHost, persistence};

    #[derive(Default)]
    struct MemoryStore(RefCell<Option<String>>);

    impl MemoryStore {
        fn holding(document: &str) -> Self {
            Self(RefCell::new(Some(document.to_owned())))
        }
    }

    impl DocumentStore for MemoryStore {
        fn load(&self) -> Result<Option<String>, String> {
            Ok(self.0.borrow().clone())
        }

        fn save(&self, document: &str) -> Result<(), String> {
            *self.0.borrow_mut() = Some(document.to_owned());
            Ok(())
        }
    }

    #[derive(Clone, Default)]
    struct MemoryKeyValue(std::rc::Rc<RefCell<BTreeMap<String, String>>>);

    impl KeyValueBackend for MemoryKeyValue {
        fn get(&self, key: &str) -> Result<Option<String>, String> {
            Ok(self.0.borrow().get(key).cloned())
        }

        fn set(&self, key: &str, value: &str) -> Result<(), String> {
            self.0.borrow_mut().insert(key.to_owned(), value.to_owned());
            Ok(())
        }
    }

    struct InaccessibleKeyValue;

    impl KeyValueBackend for InaccessibleKeyValue {
        fn get(&self, _key: &str) -> Result<Option<String>, String> {
            Err("storage unavailable".to_owned())
        }

        fn set(&self, _key: &str, _value: &str) -> Result<(), String> {
            Err("storage unavailable".to_owned())
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    struct TestDirectory(std::path::PathBuf);

    #[cfg(not(target_arch = "wasm32"))]
    impl TestDirectory {
        fn new(label: &str) -> Self {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let path = std::env::temp_dir().join(format!(
                "last-aeon-telemetry-{label}-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            std::fs::create_dir_all(&path).expect("isolated test directory is created");
            Self(path)
        }

        fn join(&self, path: &str) -> std::path::PathBuf {
            self.0.join(path)
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn sample_event() -> OnboardingEvent {
        OnboardingEvent::GuidanceChoice { enabled: false }
    }

    #[test]
    fn consent_defaults_to_not_recording() {
        let telemetry = OnboardingTelemetry::default();
        assert_eq!(telemetry.consent(), TelemetryConsent::Unset);
        assert!(!telemetry.consent().records());
        assert!(telemetry.records().is_empty());
    }

    #[test]
    fn nothing_is_recorded_before_the_player_opts_in() {
        let mut telemetry = OnboardingTelemetry::default();
        telemetry.record(None, sample_event());
        telemetry.record(None, OnboardingEvent::FirstUnpause);
        assert!(
            telemetry.records().is_empty(),
            "the unset default records nothing"
        );

        telemetry.withdraw();
        telemetry.record(None, sample_event());
        assert!(
            telemetry.records().is_empty(),
            "an explicit decline records nothing"
        );

        telemetry.grant();
        telemetry.record(None, sample_event());
        assert_eq!(telemetry.records().len(), 1);
        assert_eq!(telemetry.records()[0].sequence, 1);
    }

    #[test]
    fn withdrawal_stops_recording_and_discards_what_was_captured() {
        let mut telemetry = OnboardingTelemetry::default();
        telemetry.grant();
        telemetry.record(None, sample_event());
        telemetry.record(None, OnboardingEvent::FirstUnpause);
        assert_eq!(telemetry.records().len(), 2);

        telemetry.withdraw();
        assert_eq!(telemetry.consent(), TelemetryConsent::Declined);
        assert!(
            telemetry.records().is_empty(),
            "withdrawal clears rather than keeps"
        );
        assert!(
            telemetry.erase_pending,
            "the stored document is scheduled for erasure"
        );
        telemetry.record(None, sample_event());
        assert!(telemetry.records().is_empty());

        // The document written after a withdrawal carries no events.
        let store = MemoryStore::default();
        let document = if telemetry.consent().records() {
            telemetry.to_jsonl()
        } else {
            String::new()
        };
        save_events_to(&store, &document).expect("memory store accepts the erasure");
        assert!(load_events_from(&store).is_empty());
    }

    #[test]
    fn consent_round_trips_through_the_storage_contract() {
        for consent in [
            TelemetryConsent::Unset,
            TelemetryConsent::Granted,
            TelemetryConsent::Declined,
        ] {
            let store = MemoryStore::default();
            save_consent_to(&store, consent).expect("memory store accepts the document");
            assert_eq!(load_consent_from(&store), consent);
        }
    }

    #[test]
    fn an_absent_consent_document_is_not_consent() {
        let store = MemoryStore::default();
        assert_eq!(load_consent_from(&store), TelemetryConsent::Unset);
        assert!(!load_consent_from(&store).records());
    }

    #[test]
    fn malformed_and_future_consent_documents_fail_safe() {
        for document in [
            "",
            "not json",
            "true",
            r#"{"version":2,"consent":"granted"}"#,
            r#"{"version":1,"consent":"maybe"}"#,
            r#"{"version":1}"#,
            r#"{"consent":"granted"}"#,
            r#"[{"version":1,"consent":"granted"}]"#,
        ] {
            let consent = load_consent_from(&MemoryStore::holding(document));
            assert_eq!(
                consent,
                TelemetryConsent::Unset,
                "a document that cannot be trusted means no consent: {document}"
            );
            assert!(
                !consent.records(),
                "a malformed stored preference must fail safe, never on: {document}"
            );
        }
    }

    #[test]
    fn a_store_that_cannot_be_read_is_not_consent() {
        let inaccessible = KeyValueStore {
            backend: InaccessibleKeyValue,
            key: CONSENT_STORAGE_KEY,
        };
        assert_eq!(load_consent_from(&inaccessible), TelemetryConsent::Unset);
        assert!(save_consent_to(&inaccessible, TelemetryConsent::Granted).is_err());
        assert!(load_events_from(&inaccessible).is_empty());
    }

    #[test]
    fn the_browser_shaped_adapter_uses_its_own_keys_and_the_shared_codec() {
        let backend = MemoryKeyValue::default();
        let consent = KeyValueStore {
            backend: backend.clone(),
            key: CONSENT_STORAGE_KEY,
        };
        let events = KeyValueStore {
            backend: backend.clone(),
            key: TELEMETRY_STORAGE_KEY,
        };
        assert_eq!(load_consent_from(&consent), TelemetryConsent::Unset);

        save_consent_to(&consent, TelemetryConsent::Granted).expect("browser-shaped store writes");
        let mut telemetry = OnboardingTelemetry::default();
        telemetry.grant();
        telemetry.record(Some("411-01-01".to_owned()), sample_event());
        save_events_to(&events, &telemetry.to_jsonl()).expect("browser-shaped store writes");

        assert_eq!(load_consent_from(&consent), TelemetryConsent::Granted);
        assert_eq!(load_events_from(&events), telemetry.records());
        assert!(backend.0.borrow().contains_key(CONSENT_STORAGE_KEY));
        assert!(backend.0.borrow().contains_key(TELEMETRY_STORAGE_KEY));
        assert_ne!(
            CONSENT_STORAGE_KEY, TELEMETRY_STORAGE_KEY,
            "telemetry never shares a key with the preference document"
        );
        assert!(
            !backend.0.borrow().contains_key("last-aeon.ui.preferences"),
            "telemetry never writes the interface preference entry"
        );
    }

    #[test]
    fn the_browser_shaped_adapter_fails_safe_on_bad_documents() {
        let backend = MemoryKeyValue::default();
        let consent = KeyValueStore {
            backend: backend.clone(),
            key: CONSENT_STORAGE_KEY,
        };
        for document in ["not json", r#"{"version":9,"consent":"granted"}"#, "{}"] {
            backend
                .set(CONSENT_STORAGE_KEY, document)
                .expect("memory backend is writable");
            assert!(!load_consent_from(&consent).records());
        }
    }

    #[test]
    fn captured_documents_round_trip_and_reject_other_versions() {
        let mut telemetry = OnboardingTelemetry::default();
        telemetry.grant();
        telemetry.record(Some("411-01-04".to_owned()), sample_event());
        telemetry.record(None, OnboardingEvent::FirstUnpause);
        telemetry.record(
            Some("411-01-08".to_owned()),
            OnboardingEvent::HouseholdOutcome {
                definition: "kessarin-order".to_owned(),
                outcome: "achieved".to_owned(),
            },
        );
        let document = telemetry.to_jsonl();
        assert_eq!(
            document.lines().count(),
            4,
            "a header line plus one per event"
        );
        assert_eq!(
            load_events_from(&MemoryStore::holding(&document)),
            telemetry.records()
        );

        for bad in [
            "",
            "not json",
            "{\"telemetry_version\":2}\n{\"sequence\":1}",
            "{\"sequence\":1,\"date\":null,\"event\":{\"kind\":\"first-unpause\"}}",
        ] {
            assert!(
                load_events_from(&MemoryStore::holding(bad)).is_empty(),
                "an untrusted captured document yields nothing: {bad}"
            );
        }
    }

    #[test]
    fn a_command_kind_names_the_variant_and_carries_no_identities() {
        let kind = command_kind(&PlayerCommand::Noop);
        assert_eq!(kind, "Noop");
        assert!(kind.chars().all(char::is_alphanumeric));
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn the_native_store_round_trips_consent_and_events() {
        let directory = TestDirectory::new("round-trip");
        let consent = crate::preferences::NativeStore::at(directory.join("cfg/consent.json"));
        let events = crate::preferences::NativeStore::at(directory.join("cfg/telemetry.jsonl"));
        save_consent_to(&consent, TelemetryConsent::Granted).expect("native consent is written");
        assert_eq!(load_consent_from(&consent), TelemetryConsent::Granted);

        let mut telemetry = OnboardingTelemetry::default();
        telemetry.grant();
        telemetry.record(None, sample_event());
        save_events_to(&events, &telemetry.to_jsonl()).expect("native events are written");
        assert!(directory.join("cfg/telemetry.jsonl").is_file());
        assert_eq!(load_events_from(&events), telemetry.records());

        // Withdrawal erases what was on disk.
        save_events_to(&events, "").expect("native erasure is written");
        assert!(load_events_from(&events).is_empty());
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn the_native_store_fails_safe_for_missing_bad_and_unwritable_documents() {
        let directory = TestDirectory::new("fail-safe");
        let missing = crate::preferences::NativeStore::at(directory.join("nested/consent.json"));
        assert_eq!(load_consent_from(&missing), TelemetryConsent::Unset);

        let store = crate::preferences::NativeStore::at(directory.join("consent.json"));
        for document in ["not json", r#"{"version":4,"consent":"granted"}"#] {
            store.save(document).expect("test document is written");
            assert!(!load_consent_from(&store).records());
        }

        let parent_is_file = directory.join("parent-is-file");
        std::fs::write(&parent_is_file, "block parent creation").expect("blocker is written");
        let unwritable = crate::preferences::NativeStore::at(parent_is_file.join("consent.json"));
        assert!(save_consent_to(&unwritable, TelemetryConsent::Granted).is_err());
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn the_documented_export_location_is_a_real_path_beside_the_preferences() {
        let location = export_location();
        assert!(
            location.ends_with(TELEMETRY_FILENAME),
            "the documented workflow names the captured document: {location}"
        );
    }

    #[test]
    fn telemetry_cannot_change_authoritative_artifacts() {
        let start_date = CalendarDate {
            year: 411,
            month: 1,
            day: 1,
        }
        .to_date()
        .expect("valid test date");
        let mut host = SimHost::new(CampaignConfig {
            name: "Telemetry Isolation".to_owned(),
            seed: 91,
            start_date,
        });
        host.submit(PlayerCommand::Noop)
            .expect("ordinary command is accepted");
        host.advance_days(1);

        let snapshot_before = persistence::snapshot_to_ron(&host.snapshot())
            .expect("authoritative snapshot serialises");
        let hash_before = host.state_hash();
        let mut log_before = Vec::new();
        persistence::write_command_log(&mut log_before, &host.applied_commands())
            .expect("command log serialises");

        // Consent, a full session of capture, review, and withdrawal.
        let mut telemetry = OnboardingTelemetry::default();
        telemetry.grant();
        telemetry.record(Some("411-01-02".to_owned()), sample_event());
        telemetry.record(
            Some("411-01-02".to_owned()),
            OnboardingEvent::CommandAccepted {
                command: command_kind(&PlayerCommand::Noop),
            },
        );
        let store = MemoryStore::default();
        save_consent_to(&store, telemetry.consent()).expect("consent persists");
        let events = MemoryStore::default();
        save_events_to(&events, &telemetry.to_jsonl()).expect("captured events persist");
        assert_eq!(load_events_from(&events), telemetry.records());
        telemetry.withdraw();
        save_events_to(&events, "").expect("withdrawal erases");

        assert_eq!(host.state_hash(), hash_before);
        assert_eq!(
            persistence::snapshot_to_ron(&host.snapshot())
                .expect("authoritative snapshot still serialises"),
            snapshot_before
        );
        let mut log_after = Vec::new();
        persistence::write_command_log(&mut log_after, &host.applied_commands())
            .expect("command log still serialises");
        assert_eq!(log_after, log_before);
        assert_eq!(
            host.applied_commands().len(),
            1,
            "telemetry submits no command of its own"
        );
    }

    /// A resolution notice as the panel projects it.
    fn notice(id: u64, definition: &str) -> crate::ui::situations_panel::SituationResolutionView {
        use aeon_core::calendar::GameDate;
        use aeon_data::ContentKey;
        use aeon_data::model::SituationSubjectKind;
        use aeon_sim::situations::{SituationInstanceKey, SituationResolution, SituationSource};
        crate::ui::situations_panel::SituationResolutionView {
            resolution: SituationResolution {
                id,
                situation: SituationInstanceKey {
                    definition: ContentKey::new(definition).unwrap(),
                    source: SituationSource {
                        kind: SituationSubjectKind::Scenario,
                        key: ContentKey::new("first-reign").unwrap(),
                        id: None,
                    },
                    bindings: BTreeMap::new(),
                },
                activated: GameDate::from_days(1),
                resolved: GameDate::from_days(5),
                outcome: ContentKey::new("achieved").unwrap(),
                text: format!("notice-{id}"),
                participants: Vec::new(),
                participant_groups: Vec::new(),
                links: Vec::new(),
            },
            history: Vec::new(),
        }
    }

    /// A world holding just what the outcome observer reads and writes.
    fn observing_world(
        notices: Vec<crate::ui::situations_panel::SituationResolutionView>,
    ) -> World {
        let mut world = World::new();
        let mut telemetry = OnboardingTelemetry::default();
        telemetry.grant();
        world.insert_resource(telemetry);
        world.insert_resource(crate::ui::situations_panel::SituationPanelView {
            active: Vec::new(),
            resolutions: notices,
        });
        world
    }

    /// Projects one frame's resolution notices and runs the observer.
    fn project(
        world: &mut World,
        notices: Vec<crate::ui::situations_panel::SituationResolutionView>,
    ) {
        world
            .resource_mut::<crate::ui::situations_panel::SituationPanelView>()
            .resolutions = notices;
        world
            .run_system_once(observe_situation_outcomes)
            .expect("the outcome observer runs");
    }

    fn captured(world: &World) -> Vec<OnboardingEvent> {
        world
            .resource::<OnboardingTelemetry>()
            .records()
            .iter()
            .map(|record| record.event.clone())
            .collect()
    }

    #[test]
    fn a_restored_campaign_reports_no_outcome_for_a_notice_it_arrived_holding() {
        // Undismissed resolution notices are authoritative state that
        // survives in the save: Continue restores a campaign already
        // holding them, and they were reached in an earlier session.
        let mut world =
            observing_world(vec![notice(1, "kessarin-order"), notice(2, "cold-border")]);
        world
            .resource_mut::<OnboardingTelemetry>()
            .begin_campaign_session();

        world
            .run_system_once(observe_situation_outcomes)
            .expect("the outcome observer runs");
        assert!(
            captured(&world).is_empty(),
            "a restored notice is a baseline, not a reach: {:?}",
            captured(&world)
        );

        // A genuinely new resolution in the same session is still reported.
        project(
            &mut world,
            vec![
                notice(1, "kessarin-order"),
                notice(2, "cold-border"),
                notice(3, "aleyn-levies"),
            ],
        );
        assert_eq!(
            captured(&world),
            vec![OnboardingEvent::HouseholdOutcome {
                definition: "aleyn-levies".to_owned(),
                outcome: "achieved".to_owned(),
            }],
            "only the outcome reached this session is reported"
        );
    }

    #[test]
    fn a_second_campaign_in_one_process_reports_its_own_outcomes() {
        // Notice identities restart with the campaign, so without a reset
        // the second campaign's first reach would look like one already
        // seen — and be suppressed.
        let mut world = observing_world(Vec::new());
        world
            .resource_mut::<OnboardingTelemetry>()
            .begin_campaign_session();
        project(&mut world, Vec::new());
        project(&mut world, vec![notice(7, "cold-border")]);
        assert_eq!(
            captured(&world),
            vec![OnboardingEvent::ObjectiveReached {
                definition: "cold-border".to_owned(),
                outcome: "achieved".to_owned(),
            }]
        );

        // The player returns to the title screen and starts again.
        world.resource_mut::<OnboardingTelemetry>().clear();
        world
            .resource_mut::<OnboardingTelemetry>()
            .begin_campaign_session();
        project(&mut world, Vec::new());
        project(&mut world, vec![notice(7, "cold-border")]);
        assert_eq!(
            captured(&world),
            vec![OnboardingEvent::ObjectiveReached {
                definition: "cold-border".to_owned(),
                outcome: "achieved".to_owned(),
            }],
            "the new campaign's own reach is reported, not suppressed"
        );
    }

    #[test]
    fn launching_a_campaign_takes_a_fresh_baseline() {
        let mut world = World::new();
        let mut telemetry = OnboardingTelemetry::default();
        telemetry.grant();
        world.insert_resource(telemetry);
        world.insert_resource(crate::title::TitleState {
            spectator: false,
            // The restore itself cannot proceed without an autosave; what
            // this proves is that the baseline is dropped before launch
            // does anything else with the campaign.
            pending: Some(crate::title::TitleAction::Continue),
            autosave: None,
        });
        {
            let mut telemetry = world.resource_mut::<OnboardingTelemetry>();
            telemetry.seen_resolutions.insert(4);
            telemetry.present_resolutions.insert(4);
            telemetry.primed = true;
        }

        crate::title::launch(&mut world);

        let telemetry = world.resource::<OnboardingTelemetry>();
        assert!(!telemetry.primed, "the observer takes a new baseline");
        assert!(telemetry.seen_resolutions.is_empty());
        assert!(telemetry.present_resolutions.is_empty());
        assert_eq!(
            telemetry.consent(),
            TelemetryConsent::Granted,
            "consent belongs to the player, not to the campaign"
        );
    }

    /// The real persistence system, against a redirected configuration root:
    /// a withdrawal must erase the document that is actually stored.
    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn persisting_a_withdrawal_erases_the_stored_document() {
        let directory = TestDirectory::new("persist-withdrawal");
        let _redirected = crate::preferences::test_config_root::Redirected::to(&directory.0);

        let mut world = World::new();
        let mut telemetry = OnboardingTelemetry::default();
        telemetry.grant();
        telemetry.record(Some("411-01-02".to_owned()), sample_event());
        world.insert_resource(telemetry);
        world
            .run_system_once(persist_telemetry)
            .expect("the persistence system runs");

        let events = events_store().expect("the redirected events store resolves");
        assert_eq!(
            load_events_from(&events).len(),
            1,
            "the consenting player's document is written"
        );
        assert_eq!(
            load_consent_from(&consent_store().expect("the redirected consent store resolves")),
            TelemetryConsent::Granted
        );

        world.resource_mut::<OnboardingTelemetry>().withdraw();
        world
            .run_system_once(persist_telemetry)
            .expect("the persistence system runs again");

        assert!(
            load_events_from(&events).is_empty(),
            "withdrawal leaves no readable event behind"
        );
        assert_eq!(
            std::fs::read_to_string(directory.join(TELEMETRY_FILENAME))
                .expect("the stored document is still readable"),
            "",
            "withdrawal erases the stored document itself, header and all"
        );
        assert_eq!(
            load_consent_from(&consent_store().expect("the redirected consent store resolves")),
            TelemetryConsent::Declined,
            "the withdrawal itself is remembered"
        );
        assert!(
            !world.resource::<OnboardingTelemetry>().erase_pending,
            "a completed erasure is not rescheduled every frame"
        );
    }

    /// The browser adapter satisfies the shared contract. The body can only
    /// compile for wasm, so the wasm-bindgen harness — not the native test
    /// harness — is the one that must collect it.
    #[cfg(target_arch = "wasm32")]
    #[wasm_bindgen_test::wasm_bindgen_test]
    fn the_browser_adapter_implements_the_shared_contract() {
        fn accepts_backend(_: &impl KeyValueBackend) {}
        accepts_backend(&crate::preferences::BrowserBackend);
        assert!(!CONSENT_STORAGE_KEY.is_empty());
        assert!(!TELEMETRY_STORAGE_KEY.is_empty());
    }
}
