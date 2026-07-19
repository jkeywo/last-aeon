//! Context-sensitive actions: the job buttons under a selection, the
//! forecast they expand into, and the slot pickers that fill in whatever
//! the context does not already supply.
//!
//! Issuing stays on the authoritative path throughout: every action ends
//! as a queued `PlayerCommand::StartJob`, and nothing here decides whether
//! a job is allowed — it renders what the simulation's forecast reports.

use aeon_core::calendar::GameDate;
use aeon_data::ContentSet;
use aeon_data::model::{JobDef, JobTargetKind};
use aeon_sim::forces::{ArmyRecord, ShipLocation, ShipRecord};
use aeon_sim::politics::ADULT_AGE;
use aeon_sim::{
    CharacterId, JobTarget, OrgId, PlayerCommand, PoliticsIndex, ProvinceId, TitleHolder, TitleKind,
};
use bevy_egui::egui;

use crate::forecast_view::ForecastCache;
use crate::jobs_ui::{JobForm, UiCommandQueue};
use crate::ui::data::PanelData;
use crate::ui::forecast::{draw_forecast_body, permille_text};
use crate::ui::picker::PickerState;
use crate::ui::theme::{TargetState, UiTheme};

/// What the inspector's context-job section is anchored to.
pub enum JobScope {
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
pub fn draw_context_jobs(
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
