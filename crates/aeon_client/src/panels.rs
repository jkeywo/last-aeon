//! Read-only 2D information panels plus presence and forces controls.
//!
//! A top bar (campaign, date, resources, time control, view breadcrumb),
//! a left inspector for the current selection (body, province, house, or
//! character, including location and travel), and a right listing panel
//! (bodies, houses, and the player's forces). Mutations travel through
//! the UI command queue into the authoritative command pipeline.

use std::collections::BTreeMap;

use aeon_core::calendar::GameDate;
use aeon_data::ContentSet;
use aeon_data::model::{BodyKind, HouseTier, JobDef, JobTargetKind, OrgKind, ShipClass};
use aeon_sim::economy::OrgResources;
use aeon_sim::forces::{ArmyRecord, ShipLocation, ShipRecord};
use aeon_sim::jobs::CharacterCondition;
use aeon_sim::map::{BodyRecord, DisplayName, GeoPosition, ProvinceRecord};
use aeon_sim::politics::{
    ADULT_AGE, CharacterSkills, CharacterTraits, CharacterView, Lineage, OpinionLedger, opinion_of,
};
use aeon_sim::presence::{CharacterLocation, Location};
use aeon_sim::state::{CampaignMeta, ContentDb};
use aeon_sim::{
    ActiveJob, CampaignClock, CampaignOver, CharacterId, CharacterRecord, JobTarget, OrgId,
    OrgRecord, PlayerCommand, PlayerHouse, PoliticsIndex, ProvinceId, TitleHolder, TitleKind,
    TitleRecord,
};
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};

use crate::jobs_ui::{JobForm, UiCommandQueue};
use crate::sim_driver::{SPEED_STEPS, TimeControl};
use crate::view::{MapMode, MapView, SearchState, Selection, ViewState};

/// Character lookup shared across the panel helpers.
type CharMap<'a> = BTreeMap<CharacterId, CharacterParts<'a>>;
/// Organisation lookup shared across the panel helpers.
type OrgMap<'a> = BTreeMap<OrgId, (&'a OrgRecord, Option<&'a OrgResources>)>;

fn kind_label(kind: BodyKind) -> &'static str {
    match kind {
        BodyKind::Planet => "Planet",
        BodyKind::Moon => "Moon",
        BodyKind::Starbase => "Starbase",
    }
}

/// The player-facing name of an organisation.
fn org_name(content: &ContentSet, org_records: &OrgMap, id: OrgId) -> String {
    org_records
        .get(&id)
        .and_then(|(record, _)| content.organisations.get(&record.key))
        .map(|def| def.name.clone())
        .unwrap_or_else(|| id.to_string())
}

/// A one-line hover summary for a character link.
fn character_summary(
    content: &ContentSet,
    org_records: &OrgMap,
    chars: &CharMap,
    id: CharacterId,
    date: GameDate,
) -> String {
    let Some((record, skills, traits, ..)) = chars.get(&id).copied() else {
        return String::new();
    };
    let house = record
        .organisation
        .map(|o| org_name(content, org_records, o))
        .unwrap_or_else(|| "no house".to_owned());
    let age = match record.death {
        None => format!("age {}", record.age_years(date)),
        Some(death) => format!("died {death}"),
    };
    let trait_names: Vec<&str> = traits
        .0
        .iter()
        .filter_map(|k| content.traits.get(k).map(|d| d.name.as_str()))
        .collect();
    let mut summary = format!(
        "{} — {house}, {age}\nCmd {} · Dip {} · Int {} · Ste {}",
        record.name, skills.0.command, skills.0.diplomacy, skills.0.intrigue, skills.0.stewardship,
    );
    if !trait_names.is_empty() {
        summary.push_str(&format!("\n{}", trait_names.join(", ")));
    }
    summary
}

/// A one-line hover summary for an organisation link.
fn org_summary(
    content: &ContentSet,
    org_records: &OrgMap,
    chars: &CharMap,
    titles_held: usize,
    id: OrgId,
) -> String {
    let Some((record, resources)) = org_records.get(&id).copied() else {
        return String::new();
    };
    let name = org_name(content, org_records, id);
    let standing = match (record.kind, record.tier) {
        (OrgKind::SanctoraImperim, _) => "Imperial government".to_owned(),
        (_, Some(HouseTier::Great)) => "great house".to_owned(),
        (_, Some(HouseTier::Vassal)) => match record.liege {
            Some(liege) => format!("vassal of {}", org_name(content, org_records, liege)),
            None => "vassal house".to_owned(),
        },
        (_, Some(HouseTier::Independent)) => "independent house".to_owned(),
        _ => String::new(),
    };
    let head = record
        .head
        .and_then(|h| chars.get(&h))
        .map(|(r, ..)| r.name.as_str())
        .unwrap_or("none");
    let mut summary = format!("{name} — {standing}\nHead: {head} · {titles_held} titles held");
    if let Some(r) = resources {
        summary.push_str(&format!(
            "\nW {} · M {} · S {} · I {}/{}",
            r.wealth, r.manpower, r.supplies, r.influence, r.legitimacy
        ));
    }
    summary
}

/// Renders a link with a hover summary, returning whether it was clicked.
fn linked(ui: &mut egui::Ui, label: &str, summary: &str) -> bool {
    ui.link(label).on_hover_text(summary).clicked()
}

/// Renders the W/M/S/I resource readout, each value with its own tooltip.
fn resource_readout(ui: &mut egui::Ui, r: &OrgResources) {
    ui.label(format!("W {}", r.wealth))
        .on_hover_text("Wealth — funds jobs, personnel, and construction");
    ui.label(format!("M {}", r.manpower))
        .on_hover_text("Manpower — people to staff jobs, garrisons, and armies");
    ui.label(format!("S {}", r.supplies))
        .on_hover_text("Supplies — sustains armies, fleets, and expeditions");
    ui.label(format!("I {}/{}", r.influence, r.legitimacy))
        .on_hover_text(
            "Influence / Legitimacy — spendable political capital, capped and \
             recharged by your standing",
        );
}

/// One global-search result.
enum SearchHit {
    Character(CharacterId),
    Org(OrgId),
    /// A province and the body it sits on (so the view can focus it).
    Province(ProvinceId, aeon_sim::BodyId),
}

type CharacterQuery = (
    &'static CharacterRecord,
    &'static CharacterSkills,
    &'static CharacterTraits,
    &'static Lineage,
    &'static OpinionLedger,
);

type CharacterParts<'a> = (
    &'a CharacterRecord,
    &'a CharacterSkills,
    &'a CharacterTraits,
    &'a Lineage,
    &'a OpinionLedger,
);

/// Every world query the panels read, bundled to stay within system
/// parameter limits.
#[derive(SystemParam)]
pub struct PanelData<'w, 's> {
    bodies: Query<'w, 's, (&'static BodyRecord, &'static DisplayName)>,
    provinces: Query<
        'w,
        's,
        (
            &'static ProvinceRecord,
            &'static DisplayName,
            &'static GeoPosition,
        ),
    >,
    orgs: Query<'w, 's, (&'static OrgRecord, Option<&'static OrgResources>)>,
    characters: Query<'w, 's, CharacterQuery>,
    locations: Query<'w, 's, &'static CharacterLocation>,
    titles: Query<'w, 's, &'static TitleRecord>,
    ships: Query<'w, 's, &'static ShipRecord>,
    armies: Query<'w, 's, &'static ArmyRecord>,
    conditions: Query<'w, 's, &'static CharacterCondition>,
    active_jobs: Query<'w, 's, &'static ActiveJob>,
}

/// What the inspector's context-job section is anchored to.
enum JobScope {
    /// A living adult member of the player's house, who leads the job.
    OwnCharacter(CharacterId),
    /// A character outside the player's house, targeted by the job.
    OutsideCharacter(CharacterId),
    /// A province, targeted by military jobs.
    Province(ProvinceId),
}

/// A tooltip summarising a job's effect, costs, and risks.
fn job_hover(def: &JobDef) -> String {
    let mut text = def.summary.clone();
    let mut costs = Vec::new();
    if def.wealth_cost > 0 {
        costs.push(format!("W {}", def.wealth_cost));
    }
    if def.manpower_cost > 0 {
        costs.push(format!("M {}", def.manpower_cost));
    }
    if def.supplies_cost > 0 {
        costs.push(format!("S {}", def.supplies_cost));
    }
    if def.influence_cost > 0 {
        costs.push(format!("I {}", def.influence_cost));
    }
    if !costs.is_empty() {
        text.push_str(&format!("\nCost: {}", costs.join(", ")));
    }
    if !def.risks.is_empty() {
        let risks: Vec<String> = def.risks.iter().map(|r| format!("{r:?}")).collect();
        text.push_str(&format!("\nRisks: {}", risks.join(", ")));
    }
    text
}

/// Draws context-sensitive job buttons for the current selection, with an
/// inline picker for any slot the context does not already supply. Issuing
/// stays on the authoritative path: every action becomes a queued
/// [`PlayerCommand::StartJob`].
#[allow(clippy::too_many_arguments)]
fn draw_context_jobs(
    ui: &mut egui::Ui,
    scope: JobScope,
    content: &ContentSet,
    politics: &PoliticsIndex,
    player_org: OrgId,
    player_head: Option<CharacterId>,
    date: GameDate,
    data: &PanelData,
    form: &mut JobForm,
    queue: &mut UiCommandQueue,
) {
    let busy: Vec<CharacterId> = data.active_jobs.iter().map(|j| j.leader).collect();
    let char_name = |id: CharacterId| -> String {
        politics
            .characters
            .get(&id)
            .and_then(|e| data.characters.get(*e).ok())
            .map(|(r, ..)| r.name.clone())
            .unwrap_or_default()
    };
    let leader_ok = |id: CharacterId| -> bool {
        let Some(entity) = politics.characters.get(&id) else {
            return false;
        };
        let Ok((record, ..)) = data.characters.get(*entity) else {
            return false;
        };
        let can_lead = data
            .conditions
            .get(*entity)
            .map(|c| c.can_lead(date))
            .unwrap_or(true);
        record.alive()
            && record.organisation == Some(player_org)
            && record.age_years(date) >= ADULT_AGE
            && !busy.contains(&id)
            && can_lead
    };
    let eligible_leaders = || -> Vec<(CharacterId, String)> {
        let mut leaders: Vec<(CharacterId, String)> = politics
            .characters
            .keys()
            .filter(|id| leader_ok(**id))
            .map(|id| (*id, char_name(*id)))
            .collect();
        leaders.sort_by(|a, b| a.1.cmp(&b.1));
        leaders
    };
    let jobs_of = |kinds: &[JobTargetKind]| -> Vec<(aeon_data::ContentKey, JobDef)> {
        let mut jobs: Vec<(aeon_data::ContentKey, JobDef)> = content
            .jobs
            .iter()
            .filter(|(_, d)| kinds.contains(&d.target))
            .map(|(k, d)| (k.clone(), d.clone()))
            .collect();
        jobs.sort_by(|a, b| a.1.title.cmp(&b.1.title));
        jobs
    };

    ui.separator();
    ui.strong("Actions");

    match scope {
        JobScope::OwnCharacter(leader) => {
            if !leader_ok(leader) {
                ui.weak("Away, indisposed, or already leading a job.");
            } else {
                let jobs = jobs_of(&[
                    JobTargetKind::None,
                    JobTargetKind::Organisation,
                    JobTargetKind::Character,
                    JobTargetKind::Province,
                ]);
                for (key, def) in &jobs {
                    if ui
                        .button(&def.title)
                        .on_hover_text(job_hover(def))
                        .clicked()
                    {
                        if def.target == JobTargetKind::None {
                            queue.0.push(PlayerCommand::StartJob {
                                job: key.clone(),
                                leader,
                                target: JobTarget::None,
                            });
                            form.reset();
                            form.notice = None;
                        } else {
                            form.reset();
                            form.job = Some(key.clone());
                            form.leader = Some(leader);
                        }
                    }
                    // Inline target picker for the expanded job.
                    let expanded = form.job.as_ref() == Some(key) && form.leader == Some(leader);
                    if expanded && def.target != JobTargetKind::None {
                        ui.indent(key.to_string(), |ui| {
                            pick_target(ui, def.target, content, politics, player_org, data, form);
                            let ready = form.target.is_some();
                            if ui
                                .add_enabled(ready, egui::Button::new("Confirm"))
                                .clicked()
                                && let Some(target) = form.target
                            {
                                queue.0.push(PlayerCommand::StartJob {
                                    job: key.clone(),
                                    leader,
                                    target,
                                });
                                form.reset();
                                form.notice = None;
                            }
                        });
                    }
                }
            }
        }
        JobScope::OutsideCharacter(target_char) => {
            let jobs = jobs_of(&[JobTargetKind::Character]);
            for (key, def) in &jobs {
                if ui
                    .button(&def.title)
                    .on_hover_text(job_hover(def))
                    .clicked()
                {
                    form.reset();
                    form.job = Some(key.clone());
                    form.target = Some(JobTarget::Character(target_char));
                    form.leader = player_head.filter(|h| leader_ok(*h));
                }
                let expanded = form.job.as_ref() == Some(key)
                    && form.target == Some(JobTarget::Character(target_char));
                if expanded {
                    ui.indent(key.to_string(), |ui| {
                        pick_leader(ui, &eligible_leaders(), form);
                        let ready = form.leader.is_some();
                        if ui
                            .add_enabled(ready, egui::Button::new("Confirm"))
                            .clicked()
                            && let Some(leader) = form.leader
                        {
                            queue.0.push(PlayerCommand::StartJob {
                                job: key.clone(),
                                leader,
                                target: JobTarget::Character(target_char),
                            });
                            form.reset();
                            form.notice = None;
                        }
                    });
                }
            }
            // If this character holds the Consul title, the head can petition.
            let is_consul = data.titles.iter().any(|t| {
                t.kind == TitleKind::Consul && t.holder == TitleHolder::Character(target_char)
            });
            if is_consul
                && let Some((key, def)) = content
                    .jobs
                    .iter()
                    .find(|(k, _)| k.as_str() == "petition-the-consul")
                && let Some(head) = player_head.filter(|h| leader_ok(*h))
                && ui
                    .button(&def.title)
                    .on_hover_text(job_hover(def))
                    .clicked()
            {
                queue.0.push(PlayerCommand::StartJob {
                    job: key.clone(),
                    leader: head,
                    target: JobTarget::None,
                });
                form.reset();
                form.notice = None;
            }
        }
        JobScope::Province(province) => {
            let jobs = jobs_of(&[
                JobTargetKind::OwnArmy,
                JobTargetKind::OwnArmyAndProvince,
                JobTargetKind::OwnShipAndProvince,
            ]);
            for (key, def) in &jobs {
                if ui
                    .button(&def.title)
                    .on_hover_text(job_hover(def))
                    .clicked()
                {
                    form.reset();
                    form.job = Some(key.clone());
                    form.province = Some(province);
                }
                let expanded = form.job.as_ref() == Some(key) && form.province == Some(province);
                if expanded {
                    ui.indent(key.to_string(), |ui| {
                        if def.target == JobTargetKind::OwnShipAndProvince {
                            pick_ship(ui, player_org, data, form);
                        } else {
                            pick_army(ui, player_org, data, form);
                        }
                        let (target, leader) =
                            province_action(def.target, province, data, form, player_head);
                        let ready = target.is_some() && leader.is_some();
                        if ui
                            .add_enabled(ready, egui::Button::new("Confirm"))
                            .clicked()
                            && let (Some(target), Some(leader)) = (target, leader)
                        {
                            queue.0.push(PlayerCommand::StartJob {
                                job: key.clone(),
                                leader,
                                target,
                            });
                            form.reset();
                            form.notice = None;
                        }
                    });
                }
            }
        }
    }

    if let Some(notice) = &form.notice {
        ui.colored_label(egui::Color32::from_rgb(220, 60, 60), notice);
    }
}

/// Builds the target and derived leader for a province-scoped military job.
fn province_action(
    kind: JobTargetKind,
    province: ProvinceId,
    data: &PanelData,
    form: &JobForm,
    player_head: Option<CharacterId>,
) -> (Option<JobTarget>, Option<CharacterId>) {
    match kind {
        JobTargetKind::OwnArmy => {
            let target = form.army.map(JobTarget::OwnArmy);
            let leader = form
                .army
                .and_then(|id| data.armies.iter().find(|a| a.id == id))
                .map(|a| a.general);
            (target, leader)
        }
        JobTargetKind::OwnArmyAndProvince => {
            let target = form.army.map(|a| JobTarget::ArmyToProvince(a, province));
            let leader = form
                .army
                .and_then(|id| data.armies.iter().find(|a| a.id == id))
                .map(|a| a.general);
            (target, leader)
        }
        JobTargetKind::OwnShipAndProvince => {
            let ship = form
                .ship
                .and_then(|id| data.ships.iter().find(|s| s.id == id));
            let target = form.ship.map(|s| JobTarget::ShipToProvince(s, province));
            let leader = ship.and_then(|s| s.captain).or(player_head);
            (target, leader)
        }
        _ => (None, None),
    }
}

fn pick_target(
    ui: &mut egui::Ui,
    kind: JobTargetKind,
    content: &ContentSet,
    politics: &PoliticsIndex,
    player_org: OrgId,
    data: &PanelData,
    form: &mut JobForm,
) {
    match kind {
        JobTargetKind::Organisation => {
            let label = match form.target {
                Some(JobTarget::Org(org)) => politics
                    .orgs
                    .get(&org)
                    .and_then(|e| data.orgs.get(*e).ok())
                    .and_then(|(r, _)| content.organisations.get(&r.key))
                    .map(|d| d.name.clone())
                    .unwrap_or_default(),
                _ => "Choose an organisation".to_owned(),
            };
            egui::ComboBox::from_id_salt("ctx-org")
                .selected_text(label)
                .show_ui(ui, |ui| {
                    for (id, entity) in &politics.orgs {
                        if *id == player_org {
                            continue;
                        }
                        let Ok((record, _)) = data.orgs.get(*entity) else {
                            continue;
                        };
                        let Some(def) = content.organisations.get(&record.key) else {
                            continue;
                        };
                        if ui
                            .selectable_label(form.target == Some(JobTarget::Org(*id)), &def.name)
                            .clicked()
                        {
                            form.target = Some(JobTarget::Org(*id));
                        }
                    }
                });
        }
        JobTargetKind::Character => {
            let label = match form.target {
                Some(JobTarget::Character(id)) => politics
                    .characters
                    .get(&id)
                    .and_then(|e| data.characters.get(*e).ok())
                    .map(|(r, ..)| r.name.clone())
                    .unwrap_or_default(),
                _ => "Choose a character".to_owned(),
            };
            egui::ComboBox::from_id_salt("ctx-char")
                .selected_text(label)
                .show_ui(ui, |ui| {
                    let mut people: Vec<(CharacterId, String)> = politics
                        .characters
                        .iter()
                        .filter_map(|(id, e)| {
                            let (record, ..) = data.characters.get(*e).ok()?;
                            (record.alive() && record.organisation != Some(player_org))
                                .then(|| (*id, record.name.clone()))
                        })
                        .collect();
                    people.sort_by(|a, b| a.1.cmp(&b.1));
                    for (id, name) in people {
                        if ui
                            .selectable_label(form.target == Some(JobTarget::Character(id)), &name)
                            .clicked()
                        {
                            form.target = Some(JobTarget::Character(id));
                        }
                    }
                });
        }
        JobTargetKind::Province => {
            let label = match form.target {
                Some(JobTarget::Province(id)) => data
                    .provinces
                    .iter()
                    .find(|(r, _, _)| r.id == id)
                    .map(|(_, n, _)| n.0.clone())
                    .unwrap_or_default(),
                _ => "Choose a province".to_owned(),
            };
            egui::ComboBox::from_id_salt("ctx-prov")
                .selected_text(label)
                .show_ui(ui, |ui| {
                    let mut sorted: Vec<_> = data.provinces.iter().collect();
                    sorted.sort_by_key(|(r, _, _)| r.id);
                    for (record, name, _) in sorted {
                        if ui
                            .selectable_label(
                                form.target == Some(JobTarget::Province(record.id)),
                                &name.0,
                            )
                            .clicked()
                        {
                            form.target = Some(JobTarget::Province(record.id));
                        }
                    }
                });
        }
        _ => {}
    }
}

fn pick_leader(ui: &mut egui::Ui, leaders: &[(CharacterId, String)], form: &mut JobForm) {
    let label = form
        .leader
        .and_then(|id| leaders.iter().find(|(l, _)| *l == id))
        .map(|(_, n)| n.clone())
        .unwrap_or_else(|| "Choose a leader".to_owned());
    egui::ComboBox::from_id_salt("ctx-leader")
        .selected_text(label)
        .show_ui(ui, |ui| {
            for (id, name) in leaders {
                if ui
                    .selectable_label(form.leader == Some(*id), name)
                    .clicked()
                {
                    form.leader = Some(*id);
                }
            }
        });
}

fn pick_army(ui: &mut egui::Ui, player_org: OrgId, data: &PanelData, form: &mut JobForm) {
    let mut armies: Vec<&ArmyRecord> = data
        .armies
        .iter()
        .filter(|a| a.owner == player_org)
        .collect();
    armies.sort_by_key(|a| a.id);
    let label = form
        .army
        .and_then(|id| armies.iter().find(|a| a.id == id))
        .map(|a| a.name.clone())
        .unwrap_or_else(|| "Choose an army".to_owned());
    if armies.is_empty() {
        ui.weak("You command no armies. Muster the levies first.");
        return;
    }
    egui::ComboBox::from_id_salt("ctx-army")
        .selected_text(label)
        .show_ui(ui, |ui| {
            for army in &armies {
                if ui
                    .selectable_label(form.army == Some(army.id), &army.name)
                    .clicked()
                {
                    form.army = Some(army.id);
                }
            }
        });
}

fn pick_ship(ui: &mut egui::Ui, player_org: OrgId, data: &PanelData, form: &mut JobForm) {
    let mut ships: Vec<&ShipRecord> = data
        .ships
        .iter()
        .filter(|s| s.owner == player_org && matches!(s.location, ShipLocation::Docked(_)))
        .collect();
    ships.sort_by_key(|s| s.id);
    if ships.is_empty() {
        ui.weak("You have no docked ships.");
        return;
    }
    let label = form
        .ship
        .and_then(|id| ships.iter().find(|s| s.id == id))
        .map(|s| s.name.clone())
        .unwrap_or_else(|| "Choose a ship".to_owned());
    egui::ComboBox::from_id_salt("ctx-ship")
        .selected_text(label)
        .show_ui(ui, |ui| {
            for ship in &ships {
                if ui
                    .selectable_label(form.ship == Some(ship.id), &ship.name)
                    .clicked()
                {
                    form.ship = Some(ship.id);
                }
            }
        });
}

#[allow(clippy::too_many_arguments)]
pub fn draw_panels(
    mut contexts: EguiContexts,
    clock: Option<Res<CampaignClock>>,
    meta: Option<Res<CampaignMeta>>,
    content: Option<Res<ContentDb>>,
    politics: Option<Res<PoliticsIndex>>,
    player: Option<Res<PlayerHouse>>,
    over: Option<Res<CampaignOver>>,
    mut control: ResMut<TimeControl>,
    mut view: ResMut<ViewState>,
    mut queue: ResMut<UiCommandQueue>,
    mut search: ResMut<SearchState>,
    mut mode: ResMut<MapMode>,
    mut form: ResMut<JobForm>,
    data: PanelData,
) {
    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };
    let (Some(clock), Some(meta), Some(content), Some(politics)) = (clock, meta, content, politics)
    else {
        return;
    };
    let date = clock.date;
    let player_org = player.as_ref().and_then(|p| p.0);

    let chars: BTreeMap<CharacterId, CharacterParts> = data
        .characters
        .iter()
        .map(|parts| (parts.0.id, parts))
        .collect();
    let org_records: BTreeMap<OrgId, (&OrgRecord, Option<&OrgResources>)> = data
        .orgs
        .iter()
        .map(|(record, resources)| (record.id, (record, resources)))
        .collect();
    let province_names: BTreeMap<ProvinceId, &str> = data
        .provinces
        .iter()
        .map(|(record, name, _)| (record.id, name.0.as_str()))
        .collect();
    let org_label = |id: OrgId| -> String {
        org_records
            .get(&id)
            .and_then(|(record, _)| content.0.organisations.get(&record.key))
            .map(|def| def.name.clone())
            .unwrap_or_else(|| id.to_string())
    };
    let location_label = |location: Option<&CharacterLocation>| -> String {
        match location.map(|l| l.0) {
            Some(Location::Province(province)) => province_names
                .get(&province)
                .map(|n| (*n).to_owned())
                .unwrap_or_default(),
            Some(Location::Transit { to, arrives }) => {
                let dest = province_names.get(&to).copied().unwrap_or("...");
                format!("In transit to {dest} (arrives {arrives})")
            }
            None => "Unknown".to_owned(),
        }
    };
    let player_head: Option<CharacterId> =
        player_org.and_then(|org| org_records.get(&org).and_then(|(r, _)| r.head));

    // Hover-summary builders, reused at every link site.
    let titles = &data.titles;
    let org_hover = |id: OrgId| -> String {
        let held = titles
            .iter()
            .filter(|t| t.holder == TitleHolder::Org(id))
            .count();
        org_summary(&content.0, &org_records, &chars, held, id)
    };
    let char_hover = |id: CharacterId| -> String {
        character_summary(&content.0, &org_records, &chars, id, date)
    };

    let mut viewport = egui::Ui::new(
        ctx.clone(),
        "viewport".into(),
        egui::UiBuilder::new()
            .layer_id(egui::LayerId::background())
            .max_rect(ctx.viewport_rect()),
    );

    egui::Panel::top("top-bar").show(&mut viewport, |ui| {
        ui.horizontal(|ui| {
            ui.strong(&meta.name);
            ui.separator();
            ui.monospace(date.to_string());
            ui.separator();

            if let Some((_, Some(resources))) = player_org.and_then(|org| org_records.get(&org)) {
                resource_readout(ui, resources);
                ui.separator();
            }

            let pause_label = if control.paused { "Resume" } else { "Pause" };
            if ui.button(pause_label).clicked() {
                control.paused = !control.paused;
            }
            for (index, speed) in SPEED_STEPS.iter().enumerate() {
                let active = (control.days_per_second - speed).abs() < f32::EPSILON;
                if ui
                    .selectable_label(active, format!("{}x", index + 1))
                    .clicked()
                {
                    control.days_per_second = *speed;
                }
            }
            ui.separator();

            match view.view {
                MapView::System => {
                    ui.label("Local System");
                }
                MapView::Body(id) => {
                    if ui.button("< System").clicked() {
                        view.view = MapView::System;
                    }
                    let name = data
                        .bodies
                        .iter()
                        .find(|(record, _)| record.id == id)
                        .map(|(_, name)| name.0.as_str())
                        .unwrap_or("Unknown");
                    ui.label(name);
                    ui.separator();
                    if ui
                        .button(mode.label())
                        .on_hover_text(
                            "Toggle province colouring between the direct holder \
                             and its top great house.",
                        )
                        .clicked()
                    {
                        *mode = mode.toggled();
                    }
                }
            }

            if let Some(over) = &over {
                ui.separator();
                ui.colored_label(
                    egui::Color32::from_rgb(220, 60, 60),
                    format!("CAMPAIGN OVER — {}", over.reason),
                );
            }

            // Search box, pushed to the right end of the bar.
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut search.query)
                        .hint_text("Search…")
                        .desired_width(150.0),
                );
                ui.label("\u{1f50d}");
            });
        });
    });

    // Search results, floating below the top bar while the query is set.
    let query = search.query.trim().to_lowercase();
    if !query.is_empty() {
        let mut hits: Vec<(String, SearchHit)> = Vec::new();
        for (id, (record, ..)) in &chars {
            if record.name.to_lowercase().contains(&query) {
                hits.push((record.name.clone(), SearchHit::Character(*id)));
            }
        }
        for (id, (record, _)) in &org_records {
            let name = org_name(&content.0, &org_records, *id);
            if name.to_lowercase().contains(&query) {
                let _ = record;
                hits.push((name, SearchHit::Org(*id)));
            }
        }
        for (record, name, _) in &data.provinces {
            if name.0.to_lowercase().contains(&query) {
                hits.push((name.0.clone(), SearchHit::Province(record.id, record.body)));
            }
        }
        hits.sort_by(|a, b| a.0.cmp(&b.0));
        hits.truncate(30);

        egui::Area::new("search-results".into())
            .fixed_pos(egui::pos2(ctx.viewport_rect().width() - 260.0, 34.0))
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.set_min_width(240.0);
                    if hits.is_empty() {
                        ui.label("No matches.");
                    }
                    egui::ScrollArea::vertical()
                        .max_height(320.0)
                        .show(ui, |ui| {
                            for (label, hit) in &hits {
                                let tag = match hit {
                                    SearchHit::Character(_) => "character",
                                    SearchHit::Org(_) => "house",
                                    SearchHit::Province(..) => "province",
                                };
                                if ui
                                    .selectable_label(false, format!("{label}  ({tag})"))
                                    .clicked()
                                {
                                    match hit {
                                        SearchHit::Character(id) => {
                                            view.selected = Some(Selection::Character(*id));
                                        }
                                        SearchHit::Org(id) => {
                                            view.selected = Some(Selection::Org(*id));
                                        }
                                        SearchHit::Province(id, body) => {
                                            view.view = MapView::Body(*body);
                                            view.selected = Some(Selection::Province(*id));
                                        }
                                    }
                                    search.query.clear();
                                }
                            }
                        });
                });
            });
    }

    egui::Panel::left("inspector")
        .default_size(260.0)
        .show(&mut viewport, |ui| {
            ui.heading("Inspector");
            ui.separator();
            match view.selected {
                None => {
                    ui.label("Select a body, province, house, or character.");
                }
                Some(Selection::Body(id)) => {
                    if let Some((record, name)) =
                        data.bodies.iter().find(|(record, _)| record.id == id)
                    {
                        ui.strong(&name.0);
                        ui.label(kind_label(record.kind));
                        ui.separator();
                        egui::Grid::new("body-facts").show(ui, |ui| {
                            ui.label("Stable ID");
                            ui.monospace(record.id.to_string());
                            ui.end_row();
                            ui.label("Radius");
                            ui.label(format!("{} km", record.radius_km));
                            ui.end_row();
                            ui.label("Provinces");
                            ui.label(
                                data.provinces
                                    .iter()
                                    .filter(|(p, _, _)| p.body == id)
                                    .count()
                                    .to_string(),
                            );
                            ui.end_row();
                        });
                        if ui.button("Open strategic view").clicked() {
                            view.view = MapView::Body(id);
                        }
                    }
                }
                Some(Selection::Province(id)) => {
                    if let Some((record, name, geo)) =
                        data.provinces.iter().find(|(record, _, _)| record.id == id)
                    {
                        ui.strong(&name.0);
                        let body_name = data
                            .bodies
                            .iter()
                            .find(|(body, _)| body.id == record.body)
                            .map(|(_, name)| name.0.as_str())
                            .unwrap_or("Unknown");
                        ui.label(format!("Province of {body_name}"));
                        ui.separator();

                        let holder = politics
                            .province_titles
                            .get(&id)
                            .and_then(|title_id| politics.titles.get(title_id))
                            .and_then(|entity| data.titles.get(*entity).ok())
                            .map(|title| title.holder);
                        egui::Grid::new("province-facts").show(ui, |ui| {
                            ui.label("Held by");
                            match holder {
                                Some(TitleHolder::Org(org)) => {
                                    if linked(ui, &org_label(org), &org_hover(org)) {
                                        view.selected = Some(Selection::Org(org));
                                    }
                                }
                                Some(TitleHolder::Character(character)) => {
                                    let name = chars
                                        .get(&character)
                                        .map(|(r, ..)| r.name.clone())
                                        .unwrap_or_default();
                                    if linked(ui, &name, &char_hover(character)) {
                                        view.selected = Some(Selection::Character(character));
                                    }
                                }
                                _ => {
                                    ui.label("No one");
                                }
                            }
                            ui.end_row();
                            if let Some(def) = content.0.provinces.get(&record.key) {
                                ui.label("Monthly output");
                                ui.label(format!(
                                    "W {} / M {} / S {}",
                                    def.wealth_output, def.manpower_output, def.supplies_output
                                ));
                                ui.end_row();
                            }
                            ui.label("Latitude");
                            ui.label(format!("{:.2}\u{00b0}", geo.latitude_mdeg as f32 / 1000.0));
                            ui.end_row();
                            ui.label("Longitude");
                            ui.label(format!("{:.2}\u{00b0}", geo.longitude_mdeg as f32 / 1000.0));
                            ui.end_row();
                        });

                        // Forces standing at this province.
                        let armies_here: Vec<&ArmyRecord> =
                            data.armies.iter().filter(|a| a.location == id).collect();
                        let ships_here: Vec<&ShipRecord> = data
                            .ships
                            .iter()
                            .filter(|s| matches!(s.location, ShipLocation::Docked(p) if p == id))
                            .collect();
                        if !armies_here.is_empty() || !ships_here.is_empty() {
                            ui.separator();
                            ui.label("Forces here:");
                            for army in armies_here {
                                ui.horizontal(|ui| {
                                    ui.label(format!(
                                        "\u{2694} {} ({} men)",
                                        army.name, army.manpower
                                    ));
                                    if let Some((general, ..)) = chars.get(&army.general) {
                                        ui.label("·");
                                        if linked(ui, &general.name, &char_hover(army.general)) {
                                            view.selected =
                                                Some(Selection::Character(army.general));
                                        }
                                    }
                                });
                            }
                            for ship in ships_here {
                                ui.horizontal(|ui| {
                                    ui.label(format!("\u{2693} {}", ship.name));
                                    if let Some(captain) = ship.captain
                                        && let Some((c, ..)) = chars.get(&captain)
                                    {
                                        ui.label("·");
                                        if linked(ui, &c.name, &char_hover(captain)) {
                                            view.selected = Some(Selection::Character(captain));
                                        }
                                    }
                                });
                            }
                        }

                        if let Some(org) = player_org {
                            draw_context_jobs(
                                ui,
                                JobScope::Province(id),
                                &content.0,
                                &politics,
                                org,
                                player_head,
                                date,
                                &data,
                                &mut form,
                                &mut queue,
                            );
                        }
                    }
                }
                Some(Selection::Org(id)) => {
                    if let Some((record, resources)) = org_records.get(&id).copied() {
                        let def = content.0.organisations.get(&record.key);
                        ui.strong(def.map(|d| d.name.as_str()).unwrap_or("Unknown"));
                        match (record.kind, record.tier) {
                            (OrgKind::SanctoraImperim, _) => {
                                ui.label("Imperial government");
                            }
                            (_, Some(HouseTier::Great)) => {
                                ui.label("Great house");
                            }
                            (_, Some(HouseTier::Vassal)) => {
                                ui.horizontal(|ui| {
                                    ui.label("Vassal of");
                                    match record.liege {
                                        Some(liege) => {
                                            if linked(ui, &org_label(liege), &org_hover(liege)) {
                                                view.selected = Some(Selection::Org(liege));
                                            }
                                        }
                                        None => {
                                            ui.label("—");
                                        }
                                    }
                                });
                            }
                            (_, Some(HouseTier::Independent)) => {
                                ui.label("Independent house");
                            }
                            _ => {}
                        }
                        if record.defunct {
                            ui.colored_label(egui::Color32::from_rgb(220, 60, 60), "DEFUNCT");
                        }
                        if let Some(resources) = resources {
                            ui.horizontal(|ui| resource_readout(ui, resources));
                        }
                        ui.separator();

                        ui.horizontal(|ui| {
                            ui.label("Head:");
                            match record.head.and_then(|h| chars.get(&h)) {
                                Some((head_record, ..)) => {
                                    if linked(ui, &head_record.name, &char_hover(head_record.id)) {
                                        view.selected = Some(Selection::Character(head_record.id));
                                    }
                                }
                                None => {
                                    ui.label("None");
                                }
                            }
                        });

                        let held = data
                            .titles
                            .iter()
                            .filter(|t| t.holder == TitleHolder::Org(id))
                            .count();
                        ui.label(format!("Titles held: {held}"));

                        ui.separator();
                        ui.label("Members:");
                        egui::ScrollArea::vertical().show(ui, |ui| {
                            for (char_id, (record, ..)) in &chars {
                                if record.organisation != Some(id) || !record.alive() {
                                    continue;
                                }
                                if linked(ui, &record.name, &char_hover(*char_id)) {
                                    view.selected = Some(Selection::Character(*char_id));
                                }
                            }
                        });
                    }
                }
                Some(Selection::Character(id)) => {
                    if let Some((record, skills, char_traits, lineage, _)) = chars.get(&id).copied()
                    {
                        ui.strong(&record.name);
                        match record.death {
                            None => {
                                ui.label(format!("Age {}", record.age_years(date)));
                            }
                            Some(death) => {
                                ui.label(format!("Died {death}"));
                            }
                        }
                        if let Some(org) = record.organisation {
                            ui.horizontal(|ui| {
                                if linked(ui, &org_label(org), &org_hover(org)) {
                                    view.selected = Some(Selection::Org(org));
                                }
                            });
                        }

                        // Location and travel.
                        let location = politics
                            .characters
                            .get(&id)
                            .and_then(|e| data.locations.get(*e).ok());
                        ui.label(format!("At: {}", location_label(location)));
                        if record.alive()
                            && record.organisation == player_org
                            && let Some(Location::Province(at)) = location.map(|l| l.0)
                        {
                            egui::ComboBox::from_id_salt("travel-to")
                                .selected_text("Travel to...")
                                .show_ui(ui, |ui| {
                                    let mut sorted: Vec<_> = province_names.iter().collect();
                                    sorted.sort_by_key(|(id, _)| **id);
                                    for (province, name) in sorted {
                                        if *province == at {
                                            continue;
                                        }
                                        if ui.selectable_label(false, *name).clicked() {
                                            queue.0.push(PlayerCommand::Travel {
                                                character: id,
                                                destination: *province,
                                            });
                                        }
                                    }
                                });
                        }
                        ui.separator();

                        egui::Grid::new("skills").show(ui, |ui| {
                            ui.label("Command");
                            ui.label(skills.0.command.to_string());
                            ui.end_row();
                            ui.label("Diplomacy");
                            ui.label(skills.0.diplomacy.to_string());
                            ui.end_row();
                            ui.label("Intrigue");
                            ui.label(skills.0.intrigue.to_string());
                            ui.end_row();
                            ui.label("Stewardship");
                            ui.label(skills.0.stewardship.to_string());
                            ui.end_row();
                        });

                        let trait_names: Vec<String> = char_traits
                            .0
                            .iter()
                            .filter_map(|key| content.0.traits.get(key))
                            .map(|def| def.name.clone())
                            .collect();
                        if !trait_names.is_empty() {
                            ui.label(format!("Traits: {}", trait_names.join(", ")));
                        }

                        ui.separator();
                        if let Some(spouse) = lineage.spouse
                            && let Some((spouse_record, ..)) = chars.get(&spouse)
                        {
                            ui.horizontal(|ui| {
                                ui.label("Spouse:");
                                if linked(ui, &spouse_record.name, &char_hover(spouse)) {
                                    view.selected = Some(Selection::Character(spouse));
                                }
                            });
                        }
                        for parent in &lineage.parents {
                            if let Some((parent_record, ..)) = chars.get(parent) {
                                ui.horizontal(|ui| {
                                    ui.label("Parent:");
                                    if linked(ui, &parent_record.name, &char_hover(*parent)) {
                                        view.selected = Some(Selection::Character(*parent));
                                    }
                                });
                            }
                        }

                        if let Some(head_id) = player_head
                            && head_id != id
                            && let (Some(head), Some(them)) = (chars.get(&head_id), chars.get(&id))
                        {
                            fn as_view<'a>(p: &CharacterParts<'a>) -> CharacterView<'a> {
                                CharacterView {
                                    record: p.0,
                                    traits: p.2,
                                    lineage: p.3,
                                    ledger: p.4,
                                }
                            }
                            ui.separator();
                            ui.label(format!(
                                "Your head's opinion of them: {:+}",
                                opinion_of(&content.0, date, as_view(head), as_view(them)),
                            ));
                            ui.label(format!(
                                "Their opinion of your head: {:+}",
                                opinion_of(&content.0, date, as_view(them), as_view(head)),
                            ));
                        }

                        if record.alive()
                            && let Some(org) = player_org
                        {
                            let scope = if record.organisation == Some(org) {
                                JobScope::OwnCharacter(id)
                            } else {
                                JobScope::OutsideCharacter(id)
                            };
                            draw_context_jobs(
                                ui,
                                scope,
                                &content.0,
                                &politics,
                                org,
                                player_head,
                                date,
                                &data,
                                &mut form,
                                &mut queue,
                            );
                        }
                    }
                }
            }
        });

    egui::Panel::right("listing")
        .default_size(230.0)
        .show(&mut viewport, |ui| match view.view {
            MapView::System => {
                ui.heading("Bodies");
                ui.separator();
                let mut sorted: Vec<_> = data.bodies.iter().collect();
                sorted.sort_by_key(|(record, _)| record.id);
                for (record, name) in sorted {
                    let selected = view.selected == Some(Selection::Body(record.id));
                    if ui.selectable_label(selected, &name.0).clicked() {
                        view.selected = Some(Selection::Body(record.id));
                    }
                }

                ui.add_space(8.0);
                ui.heading("Houses");
                ui.separator();
                for (org_id, (record, _)) in &org_records {
                    let def = content.0.organisations.get(&record.key);
                    let label = def.map(|d| d.name.clone()).unwrap_or_default();
                    let selected = view.selected == Some(Selection::Org(*org_id));
                    if ui.selectable_label(selected, label).clicked() {
                        view.selected = Some(Selection::Org(*org_id));
                    }
                }

                ui.add_space(8.0);
                ui.heading("Forces");
                ui.separator();
                egui::ScrollArea::vertical()
                    .id_salt("forces")
                    .show(ui, |ui| {
                        let mut ships: Vec<&ShipRecord> = data.ships.iter().collect();
                        ships.sort_by_key(|s| s.id);
                        for ship in ships {
                            let class = match ship.class {
                                ShipClass::Capital => "Capital",
                                ShipClass::Transport => "Transport",
                                ShipClass::Patrol => "Patrol",
                            };
                            let place = match ship.location {
                                ShipLocation::Docked(province) => province_names
                                    .get(&province)
                                    .map(|n| (*n).to_owned())
                                    .unwrap_or_default(),
                                ShipLocation::Transit { to, .. } => format!(
                                    "-> {}",
                                    province_names.get(&to).copied().unwrap_or("...")
                                ),
                            };
                            ui.horizontal(|ui| {
                                ui.label(format!("{} ({class}) — {place}", ship.name));
                            });
                            if Some(ship.owner) == player_org
                                && matches!(ship.location, ShipLocation::Docked(_))
                            {
                                egui::ComboBox::from_id_salt(("move-ship", ship.id))
                                    .selected_text("Move to...")
                                    .show_ui(ui, |ui| {
                                        let mut sorted: Vec<_> = province_names.iter().collect();
                                        sorted.sort_by_key(|(id, _)| **id);
                                        for (province, name) in sorted {
                                            if ui.selectable_label(false, *name).clicked() {
                                                queue.0.push(PlayerCommand::MoveShip {
                                                    ship: ship.id,
                                                    destination: *province,
                                                });
                                            }
                                        }
                                    });
                            }
                        }

                        let mut armies: Vec<&ArmyRecord> = data.armies.iter().collect();
                        armies.sort_by_key(|a| a.id);
                        for army in armies {
                            let place =
                                province_names.get(&army.location).copied().unwrap_or("...");
                            ui.horizontal(|ui| {
                                ui.label(format!(
                                    "{} — {} men, {} supplies — {place}",
                                    army.name, army.manpower, army.supplies
                                ));
                                if Some(army.owner) == player_org {
                                    let defending = army.standing_order
                                        == aeon_sim::warfare::StandingOrder::DefendHoldings;
                                    let label = if defending { "Defending" } else { "Hold fast" };
                                    if ui
                                        .small_button(label)
                                        .on_hover_text(
                                            "Toggle the standing order: defending armies \
                                             march to answer threats against your holdings",
                                        )
                                        .clicked()
                                    {
                                        let order = if defending {
                                            aeon_sim::warfare::StandingOrder::HoldFast
                                        } else {
                                            aeon_sim::warfare::StandingOrder::DefendHoldings
                                        };
                                        queue.0.push(PlayerCommand::SetStandingOrder {
                                            army: army.id,
                                            order,
                                        });
                                    }
                                    if ui.small_button("Disband").clicked() {
                                        queue.0.push(PlayerCommand::DisbandArmy { army: army.id });
                                    }
                                }
                            });
                        }
                    });
            }
            MapView::Body(body_id) => {
                ui.heading("Provinces");
                ui.separator();
                egui::ScrollArea::vertical().show(ui, |ui| {
                    let mut sorted: Vec<_> = data
                        .provinces
                        .iter()
                        .filter(|(record, _, _)| record.body == body_id)
                        .collect();
                    sorted.sort_by_key(|(record, _, _)| record.id);
                    for (record, name, _) in sorted {
                        let selected = view.selected == Some(Selection::Province(record.id));
                        if ui.selectable_label(selected, &name.0).clicked() {
                            view.selected = Some(Selection::Province(record.id));
                        }
                    }
                });
            }
        });
}
