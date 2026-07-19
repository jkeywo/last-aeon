//! Read-only 2D information panels plus presence and forces controls.
//!
//! A top bar (campaign, date, resources, time control, view breadcrumb),
//! a left inspector for the current selection (body, province, house, or
//! character, including location and travel), and a right listing panel
//! (bodies, houses, and the player's forces). Mutations travel through
//! the UI command queue into the authoritative command pipeline.

use aeon_core::calendar::GameDate;
use aeon_data::ContentSet;
use aeon_data::model::{HouseTier, JobDef, JobTargetKind, OrgKind, ShipClass};
use aeon_sim::forces::{ArmyRecord, ShipLocation, ShipRecord};
use aeon_sim::obligations::ObligationRecord;
use aeon_sim::order::ORDER_MAX;
use aeon_sim::politics::{ADULT_AGE, CharacterView, opinion_of};
use aeon_sim::presence::Location;
use aeon_sim::state::{CampaignMeta, ContentDb};
use aeon_sim::{
    CampaignClock, CampaignOver, CharacterId, JobTarget, OrgId, PlayerCommand, PlayerHouse,
    PoliticsIndex, ProvinceId, TitleHolder, TitleKind,
};
use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};

use crate::forecast_view::ForecastCache;
use crate::jobs_ui::{JobForm, UiCommandQueue};
use crate::sim_driver::{SPEED_STEPS, TimeControl};
use crate::ui::data::{CharacterParts, JobUi, MapUi, PanelData};
use crate::ui::forecast::{draw_forecast_body, permille_text};
use crate::ui::icons::draw_mode_bar;
use crate::ui::lookup::Lookup;
use crate::ui::picker::PickerState;
use crate::ui::theme::{TargetState, UiTheme};
use crate::ui::widgets::{draw_identity, kind_label, linked, resource_readout};
use crate::view::{MapView, SearchState, Selection, ViewState};

/// One global-search result.
enum SearchHit {
    Character(CharacterId),
    Org(OrgId),
    /// A province and the body it sits on (so the view can focus it).
    Province(ProvinceId, aeon_sim::BodyId),
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
    cache: &ForecastCache,
    form: &mut JobForm,
    queue: &mut UiCommandQueue,
    picker: &mut PickerState,
) {
    let busy: Vec<CharacterId> = data.active_jobs.iter().map(|j| j.leader).collect();
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

    // Every action expands to a forecast before it can be confirmed, so
    // nothing is ever committed to unseen.
    match scope {
        JobScope::OwnCharacter(leader) => {
            if !leader_ok(leader) {
                // Say which of the several possible reasons applies.
                ui.weak(
                    data.availability
                        .of(leader)
                        .map(|state| {
                            state.describe(|key| {
                                content
                                    .jobs
                                    .get(key)
                                    .map(|def| def.title.clone())
                                    .unwrap_or_else(|| key.to_string())
                            })
                        })
                        .unwrap_or_else(|| "unavailable".to_owned()),
                );
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
                        form.reset();
                        form.job = Some(key.clone());
                        form.leader = Some(leader);
                        form.about = Some(leader);
                        if def.target == JobTargetKind::None {
                            form.target = Some(JobTarget::None);
                        }
                    }
                    // Anchored to the character whose panel this is, not to
                    // the leader chosen: picking someone else to lead must
                    // not collapse the panel it was picked in.
                    let expanded = form.job.as_ref() == Some(key) && form.about == Some(leader);
                    if expanded {
                        ui.indent(key.to_string(), |ui| {
                            if def.target != JobTargetKind::None {
                                pick_target(
                                    ui, def.target, content, politics, player_org, data, form,
                                );
                            }
                            draw_forecast(ui, &data.theme, cache, form, picker, LeaderChoice::Free);
                            confirm_job(ui, key, cache, form, queue);
                        });
                    }
                }
            }
        }
        JobScope::OutsideCharacter(target_char) => {
            let mut offered: Vec<(aeon_data::ContentKey, JobDef)> =
                jobs_of(&[JobTargetKind::Character]);
            // If this character holds the Consul title, the head can petition.
            let is_consul = data.titles.iter().any(|t| {
                t.kind == TitleKind::Consul && t.holder == TitleHolder::Character(target_char)
            });
            if is_consul
                && let Some((key, def)) = content
                    .jobs
                    .iter()
                    .find(|(k, _)| k.as_str() == "petition-the-consul")
            {
                offered.push((key.clone(), def.clone()));
            }

            for (key, def) in &offered {
                let targets_them = def.target == JobTargetKind::Character;
                if ui
                    .button(&def.title)
                    .on_hover_text(job_hover(def))
                    .clicked()
                {
                    form.reset();
                    form.job = Some(key.clone());
                    form.target = Some(if targets_them {
                        JobTarget::Character(target_char)
                    } else {
                        JobTarget::None
                    });
                    form.leader = player_head.filter(|h| leader_ok(*h));
                    form.about = Some(target_char);
                }
                let expanded = form.job.as_ref() == Some(key) && form.about == Some(target_char);
                if expanded {
                    ui.indent(key.to_string(), |ui| {
                        draw_forecast(ui, &data.theme, cache, form, picker, LeaderChoice::Free);
                        confirm_job(ui, key, cache, form, queue);
                    });
                }
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
                        // Publish the resolved target and leader so the
                        // forecast is for exactly what would be ordered.
                        let action = province_action(def.target, province, data, form);
                        form.target = action.target;
                        form.leader = action.leader;
                        // An obstacle is stated where the choice is made,
                        // not discovered after pressing Confirm.
                        if let Some(obstacle) = &action.obstacle {
                            ui.colored_label(
                                data.theme.semantics.target(TargetState::IneligibleFixable),
                                obstacle,
                            );
                        }
                        draw_forecast(
                            ui,
                            &data.theme,
                            cache,
                            form,
                            picker,
                            LeaderChoice::Fixed("led by the force's own commander"),
                        );
                        confirm_job(ui, key, cache, form, queue);
                    });
                }
            }
        }
    }

    if let Some(notice) = &form.notice {
        ui.colored_label(
            data.theme.semantics.target(TargetState::IneligibleFixable),
            notice,
        );
    }
}

/// A permille chance as a player-facing percentage.
/// The Confirm button for an expanded action.
fn confirm_job(
    ui: &mut egui::Ui,
    key: &aeon_data::ContentKey,
    cache: &ForecastCache,
    form: &mut JobForm,
    queue: &mut UiCommandQueue,
) {
    // The forecast already knows whether this can be ordered; Confirm must
    // agree with it rather than letting the player press it and be refused.
    let forecast_allows = cache
        .forecast
        .as_ref()
        .map(|view| view.startable())
        .unwrap_or(false);
    let ready = form.leader.is_some() && form.target.is_some() && forecast_allows;
    if ui
        .add_enabled(ready, egui::Button::new("Confirm"))
        .clicked()
        && let (Some(leader), Some(target)) = (form.leader, form.target)
    {
        queue.0.push(PlayerCommand::StartJob {
            job: key.clone(),
            leader,
            target,
        });
        form.reset();
        form.notice = None;
    }
}

/// Whether the player picks who leads an action, or the action settles it.
#[derive(Copy, Clone, PartialEq, Eq)]
enum LeaderChoice {
    /// Any eligible member of the house may be chosen.
    Free,
    /// Fixed by what is being ordered, with the reason why.
    ///
    /// A force is led by the character who commands it and nobody else, so
    /// offering a picker for a march would be offering a choice that does
    /// not exist.
    Fixed(&'static str),
}

/// Renders the simulation's forecast for the expanded action, and the way
/// in to choosing who leads it.
///
/// The breakdown itself is drawn by [`draw_forecast_body`], shared with the
/// character picker, so the figures a player compares candidates on are the
/// figures they commit to.
fn draw_forecast(
    ui: &mut egui::Ui,
    theme: &UiTheme,
    cache: &ForecastCache,
    form: &mut JobForm,
    picker: &mut PickerState,
    choice: LeaderChoice,
) {
    draw_leader_slot(ui, theme, cache, form, picker, choice);

    let Some(view) = &cache.forecast else {
        ui.weak("Choose the remaining details to see the forecast.");
        return;
    };

    egui::Frame::group(ui.style()).show(ui, |ui| {
        draw_forecast_body(ui, theme, view);
    });
}

/// Who leads this action, and the way to change it.
///
/// One control, in one place, for every action: the inline dropdown and the
/// separate "compare leaders" list it used to sit beside were two ways of
/// answering the same question, and they did not agree with each other.
fn draw_leader_slot(
    ui: &mut egui::Ui,
    theme: &UiTheme,
    cache: &ForecastCache,
    form: &mut JobForm,
    picker: &mut PickerState,
    choice: LeaderChoice,
) {
    let chosen = form
        .leader
        .and_then(|id| cache.leaders.iter().find(|option| option.id == id));

    ui.horizontal(|ui| {
        ui.label("Led by");
        match chosen {
            Some(option) => {
                ui.colored_label(
                    theme.semantics.target(TargetState::Valid),
                    format!("{} — {}", option.name, permille_text(option.success())),
                );
            }
            None => {
                ui.colored_label(
                    theme.semantics.target(TargetState::IneligibleFixable),
                    "nobody yet",
                );
            }
        }
        if let LeaderChoice::Fixed(reason) = choice {
            ui.weak(format!("({reason})"));
            return;
        }
        let free = cache
            .leaders
            .iter()
            .filter(|option| option.blocked().is_none())
            .count();
        if ui
            .button("Choose…")
            .on_hover_text(format!(
                "Compare everyone in your house for this job.                  {free} of {} could take it on now.",
                cache.leaders.len()
            ))
            .clicked()
        {
            picker.open();
        }
    });
}

/// What a province-scoped military order would be, and who would carry it.
///
/// A force is led by the character who commands it and nobody else, so a
/// ship with no captain has no order to give — reported here rather than
/// silently substituting the head of the house and failing later.
struct ProvinceAction {
    target: Option<JobTarget>,
    leader: Option<CharacterId>,
    /// Why this cannot be ordered yet, in words, for showing at the slot.
    obstacle: Option<String>,
}

fn province_action(
    kind: JobTargetKind,
    province: ProvinceId,
    data: &PanelData,
    form: &JobForm,
) -> ProvinceAction {
    let army = form
        .army
        .and_then(|id| data.armies.iter().find(|a| a.id == id));
    let ship = form
        .ship
        .and_then(|id| data.ships.iter().find(|s| s.id == id));

    match kind {
        JobTargetKind::OwnArmy | JobTargetKind::OwnArmyAndProvince => ProvinceAction {
            target: form.army.map(|id| match kind {
                JobTargetKind::OwnArmy => JobTarget::OwnArmy(id),
                _ => JobTarget::ArmyToProvince(id, province),
            }),
            leader: army.map(|a| a.general),
            obstacle: None,
        },
        JobTargetKind::OwnShipAndProvince => ProvinceAction {
            target: form.ship.map(|id| JobTarget::ShipToProvince(id, province)),
            leader: ship.and_then(|s| s.captain),
            obstacle: match ship {
                Some(ship) if ship.captain.is_none() => Some(format!(
                    "{} has no captain. A ship is ordered by the officer who \
                     commands it — assign one first.",
                    ship.name
                )),
                _ => None,
            },
        },
        _ => ProvinceAction {
            target: None,
            leader: None,
            obstacle: None,
        },
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
    map_ui: MapUi,
    job_ui: JobUi,
    log: Option<Res<aeon_sim::MessageLog>>,
    mut filter: ResMut<crate::jobs_ui::LogFilter>,
    data: PanelData,
) {
    let JobUi {
        mut form,
        mut picker,
    } = job_ui;
    let MapUi {
        mut mode,
        mut ledger,
    } = map_ui;
    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };
    let (Some(clock), Some(meta), Some(content), Some(politics)) = (clock, meta, content, politics)
    else {
        return;
    };
    let date = clock.date;
    let theme = &data.theme;
    let player_org = player.as_ref().and_then(|p| p.0);

    // Every name, label and hover summary the panels need, built once.
    let lookup = Lookup::build(&data, &content.0, date);
    let player_head: Option<CharacterId> =
        player_org.and_then(|org| lookup.orgs.get(&org).and_then(|(r, _)| r.head));

    let mut viewport = egui::Ui::new(
        ctx.clone(),
        "viewport".into(),
        egui::UiBuilder::new()
            .layer_id(egui::LayerId::background())
            .max_rect(ctx.viewport_rect()),
    );

    egui::Panel::top("top-bar").show(&mut viewport, |ui| {
        ui.horizontal(|ui| {
            // Who you are, first and always.
            if let Some(hit) = draw_identity(ui, &lookup, player_org, player_head) {
                view.selected = Some(hit);
            }
            ui.separator();
            ui.strong(&meta.name);
            ui.separator();
            ui.monospace(date.to_string());
            ui.separator();

            if let Some((_, Some(resources))) = player_org.and_then(|org| lookup.orgs.get(&org)) {
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
                    if let Some(picked) = draw_mode_bar(ui, &data.theme, *mode) {
                        *mode = picked;
                    }
                    ui.toggle_value(&mut ledger.open, "?")
                        .on_hover_text("Show what the map's colours mean.");
                }
            }

            if let Some(over) = &over {
                ui.separator();
                ui.colored_label(
                    egui::Color32::from(theme.semantics.urgent),
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
        for (id, (record, ..)) in &lookup.chars {
            if record.name.to_lowercase().contains(&query) {
                hits.push((record.name.clone(), SearchHit::Character(*id)));
            }
        }
        for (id, (record, _)) in &lookup.orgs {
            let name = lookup.org_name(*id);
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

    // ------------------------------------------------------------------
    // Situation strip: what needs attention, and a way straight to it.
    // ------------------------------------------------------------------
    if matches!(view.view, MapView::Body(_)) && !data.readout.situation.is_empty() {
        egui::Area::new("situation-strip".into())
            .fixed_pos(egui::pos2(276.0, 34.0))
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        ui.strong("Needs attention:")
                            .on_hover_text("Threats to your holdings. Click one to go to it.");
                        for item in &data.readout.situation {
                            let colour = if item.urgent {
                                egui::Color32::from(theme.semantics.urgent)
                            } else {
                                egui::Color32::from(theme.semantics.notable)
                            };
                            if ui
                                .add(egui::Button::new(
                                    egui::RichText::new(&item.headline).color(colour),
                                ))
                                .on_hover_text(&item.detail)
                                .clicked()
                            {
                                view.view = MapView::Body(item.body);
                                view.selected = Some(Selection::Province(item.province));
                            }
                        }
                    });
                });
            });
    }

    // ------------------------------------------------------------------
    // Legend for the active map mode.
    // ------------------------------------------------------------------
    if ledger.open && matches!(view.view, MapView::Body(_)) && !data.readout.legend.is_empty() {
        let bottom = ctx.viewport_rect().height() - 180.0;
        egui::Area::new("map-legend".into())
            .fixed_pos(egui::pos2(276.0, bottom))
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.strong(mode.label()).on_hover_text(mode.description());
                    for (label, colour) in &data.readout.legend {
                        ui.horizontal(|ui| {
                            let (rect, _) = ui
                                .allocate_exact_size(egui::vec2(12.0, 12.0), egui::Sense::hover());
                            ui.painter().rect_filled(
                                rect,
                                2.0,
                                egui::Color32::from_rgb(colour[0], colour[1], colour[2]),
                            );
                            ui.label(label);
                        });
                    }
                    if !mode.is_political() {
                        ui.weak("Values are printed on the map.");
                    }
                });
            });
    }

    // The bottom bar is declared before the side panels so they stop
    // above it rather than running underneath.
    if let Some(log) = &log {
        crate::jobs_ui::draw_bottom_bar(
            &mut viewport,
            &content,
            &politics,
            date,
            player_org,
            log,
            &mut filter,
            &mut view,
            &mut queue,
            &data.active_jobs,
            &data.character_records,
            &data.province_records,
        );
    }

    egui::Panel::left("inspector")
        .default_size(260.0)
        .show(&mut viewport, |ui| {
            ui.heading("Inspector");
            ui.separator();
            // A forecast and its leader comparison can outgrow the panel,
            // so the whole inspector scrolls.
            egui::ScrollArea::vertical()
                .id_salt("inspector-scroll")
                .show(ui, |ui| {
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
                                            if linked(ui, &lookup.org_name(org), &lookup.org_hover(org)) {
                                                view.selected = Some(Selection::Org(org));
                                            }
                                        }
                                        Some(TitleHolder::Character(character)) => {
                                            let name = lookup.chars
                                                .get(&character)
                                                .map(|(r, ..)| r.name.clone())
                                                .unwrap_or_default();
                                            if linked(ui, &name, &lookup.char_hover(character)) {
                                                view.selected =
                                                    Some(Selection::Character(character));
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
                                            def.wealth_output,
                                            def.manpower_output,
                                            def.supplies_output
                                        ));
                                        ui.end_row();
                                    }
                                    ui.label("Latitude");
                                    ui.label(format!(
                                        "{:.2}\u{00b0}",
                                        geo.latitude_mdeg as f32 / 1000.0
                                    ));
                                    ui.end_row();
                                    ui.label("Longitude");
                                    ui.label(format!(
                                        "{:.2}\u{00b0}",
                                        geo.longitude_mdeg as f32 / 1000.0
                                    ));
                                    ui.end_row();
                                });

                                // What the active map mode says about it.
                                if let Some(entry) = data.readout.provinces.get(&id)
                                    && !entry.hint.is_empty()
                                {
                                    ui.separator();
                                    ui.horizontal_wrapped(|ui| {
                                        ui.weak(format!("{}:", mode.label()));
                                        ui.label(&entry.hint);
                                    });
                                }

                                // How governable the province is, and why.
                                if let Some((_, state)) =
                                    data.order.iter().find(|(record, _)| record.id == id)
                                {
                                    ui.separator();
                                    let percent = state.order * 100 / ORDER_MAX;
                                    ui.horizontal(|ui| {
                                        ui.label("Order");
                                        let colour = if state.in_unrest() {
                                            egui::Color32::from(theme.semantics.urgent)
                                        } else if percent < 50 {
                                            egui::Color32::from(theme.semantics.notable)
                                        } else {
                                            egui::Color32::from(theme.semantics.calm)
                                        };
                                        ui.colored_label(colour, format!("{percent}%"))
                                            .on_hover_text(
                                                "How governable this province is. It scales                                                  what the province pays and how reliably it                                                  is defended, and a province left in unrest                                                  will throw off its ruler entirely.",
                                            );
                                    });
                                    if let Some(days) = state.days_to_revolt() {
                                        ui.colored_label(
                                            egui::Color32::from(theme.semantics.urgent),
                                            format!("In unrest — revolts in {days} days"),
                                        )
                                        .on_hover_text(
                                            "Garrison it, go there in person, or hold court                                              to restore order before the province is lost.",
                                        );
                                    }
                                }

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
                                            if let Some((general, ..)) = lookup.chars.get(&army.general) {
                                                ui.label("·");
                                                if linked(
                                                    ui,
                                                    &general.name,
                                                    &lookup.char_hover(army.general),
                                                ) {
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
                                                && let Some((c, ..)) = lookup.chars.get(&captain)
                                            {
                                                ui.label("·");
                                                if linked(ui, &c.name, &lookup.char_hover(captain)) {
                                                    view.selected =
                                                        Some(Selection::Character(captain));
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
                                        &data.cache,
                                        &mut form,
                                        &mut queue,
                                        &mut picker,
                                    );
                                }
                            }
                        }
                        Some(Selection::Org(id)) => {
                            if let Some((record, resources)) = lookup.orgs.get(&id).copied() {
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
                                                    if linked(
                                                        ui,
                                                        &lookup.org_name(liege),
                                                        &lookup.org_hover(liege),
                                                    ) {
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
                                    ui.colored_label(
                                        egui::Color32::from(theme.semantics.urgent),
                                        "DEFUNCT",
                                    );
                                }
                                if let Some(resources) = resources {
                                    ui.horizontal(|ui| resource_readout(ui, resources));
                                }
                                ui.separator();

                                ui.horizontal(|ui| {
                                    ui.label("Head:");
                                    match record.head.and_then(|h| lookup.chars.get(&h)) {
                                        Some((head_record, ..)) => {
                                            if linked(
                                                ui,
                                                &head_record.name,
                                                &lookup.char_hover(head_record.id),
                                            ) {
                                                view.selected =
                                                    Some(Selection::Character(head_record.id));
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

                                // Standing obligations: what this house is
                                // bound by, kept apart from what it feels.
                                if let Some(ledger) = &data.obligations {
                                    let mut entries: Vec<&ObligationRecord> =
                                        ledger.involving(id).collect();
                                    entries.sort_by_key(|entry| (entry.kind, entry.id));
                                    if !entries.is_empty() {
                                        ui.separator();
                                        ui.label("Obligations:").on_hover_text(
                                            "Favours, promises and grievances binding this                                              house. These are political facts, separate                                              from how anyone feels: a house may dislike                                              its creditor and still owe it.",
                                        );
                                        for entry in entries {
                                            let other = if entry.debtor == id {
                                                entry.creditor
                                            } else {
                                                entry.debtor
                                            };
                                            let owes_out = entry.debtor == id;
                                            let colour = match (entry.kind, owes_out) {
                                                (aeon_sim::ObligationKind::Grievance, _) => theme
                                                    .semantics
                                                    .target(TargetState::IneligibleFixable),
                                                (_, true) => {
                                                    egui::Color32::from(theme.semantics.notable)
                                                }
                                                (_, false) => {
                                                    egui::Color32::from(theme.semantics.valid)
                                                }
                                            };
                                            let mut detail = format!(
                                                "{}
Origin: {}",
                                                entry.summary(|org| lookup.org_name(org)),
                                                entry.origin
                                            );
                                            match entry.expires {
                                                Some(expiry) => detail.push_str(&format!(
                                                    "
Lapses {expiry} ({} days)",
                                                    date.days_until(expiry).max(0)
                                                )),
                                                None => {
                                                    detail.push_str("
Stands until settled")
                                                }
                                            }
                                            detail.push_str(&format!("
Weight {}", entry.weight));
                                            ui.horizontal(|ui| {
                                                ui.colored_label(
                                                    colour,
                                                    format!(
                                                        "{} {}",
                                                        entry.kind.label(),
                                                        if owes_out { "to" } else { "from" }
                                                    ),
                                                )
                                                .on_hover_text(&detail);
                                                if linked(ui, &lookup.org_name(other), &lookup.org_hover(other)) {
                                                    view.selected = Some(Selection::Org(other));
                                                }
                                            });
                                        }
                                    }
                                }

                                ui.separator();
                                ui.label("Members:");
                                for (char_id, (record, ..)) in &lookup.chars {
                                    if record.organisation != Some(id) || !record.alive() {
                                        continue;
                                    }
                                    if linked(ui, &record.name, &lookup.char_hover(*char_id)) {
                                        view.selected = Some(Selection::Character(*char_id));
                                    }
                                }
                            }
                        }
                        Some(Selection::Character(id)) => {
                            if let Some((record, skills, char_traits, lineage, _)) =
                                lookup.chars.get(&id).copied()
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
                                        if linked(ui, &lookup.org_name(org), &lookup.org_hover(org)) {
                                            view.selected = Some(Selection::Org(org));
                                        }
                                    });
                                }

                                // Location and travel.
                                let location = politics
                                    .characters
                                    .get(&id)
                                    .and_then(|e| data.locations.get(*e).ok());
                                ui.label(format!("At: {}", lookup.location_label(location)));
                                if record.alive()
                                    && record.organisation == player_org
                                    && let Some(Location::Province(at)) = location.map(|l| l.0)
                                {
                                    egui::ComboBox::from_id_salt("travel-to")
                                        .selected_text("Travel to...")
                                        .show_ui(ui, |ui| {
                                            let mut sorted: Vec<_> =
                                                lookup.province_names.iter().collect();
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
                                    && let Some((spouse_record, ..)) = lookup.chars.get(&spouse)
                                {
                                    ui.horizontal(|ui| {
                                        ui.label("Spouse:");
                                        if linked(ui, &spouse_record.name, &lookup.char_hover(spouse)) {
                                            view.selected = Some(Selection::Character(spouse));
                                        }
                                    });
                                }
                                for parent in &lineage.parents {
                                    if let Some((parent_record, ..)) = lookup.chars.get(parent) {
                                        ui.horizontal(|ui| {
                                            ui.label("Parent:");
                                            if linked(ui, &parent_record.name, &lookup.char_hover(*parent))
                                            {
                                                view.selected = Some(Selection::Character(*parent));
                                            }
                                        });
                                    }
                                }

                                if let Some(head_id) = player_head
                                    && head_id != id
                                    && let (Some(head), Some(them)) =
                                        (lookup.chars.get(&head_id), lookup.chars.get(&id))
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
                                        &data.cache,
                                        &mut form,
                                        &mut queue,
                                        &mut picker,
                                    );
                                }
                            }
                        }
                    }
                });
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
                for (org_id, (record, _)) in &lookup.orgs {
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
                                ShipLocation::Docked(province) => lookup
                                    .province_names
                                    .get(&province)
                                    .map(|n| (*n).to_owned())
                                    .unwrap_or_default(),
                                ShipLocation::Transit { to, .. } => format!(
                                    "-> {}",
                                    lookup.province_names.get(&to).copied().unwrap_or("...")
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
                                        let mut sorted: Vec<_> =
                                            lookup.province_names.iter().collect();
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
                            let place = lookup
                                .province_names
                                .get(&army.location)
                                .copied()
                                .unwrap_or("...");
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
