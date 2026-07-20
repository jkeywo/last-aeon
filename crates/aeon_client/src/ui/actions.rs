//! Context-sensitive actions: the job buttons under a selection, the
//! forecast they expand into, and the slot pickers that fill in whatever
//! the context does not already supply.
//!
//! Issuing stays on the authoritative path throughout: every action ends
//! as a queued `PlayerCommand::StartJob`, and nothing here decides whether
//! a job is allowed — it renders what the simulation's forecast reports.

use aeon_data::ContentSet;
use aeon_data::model::{JobDef, JobTargetKind};
use aeon_sim::forces::{ArmyRecord, ShipLocation, ShipRecord};
use aeon_sim::{
    CharacterId, JobTarget, LeaderAvailability, OrgId, PlayerCommand, PoliticsIndex, ProvinceId,
    TextDb, TitleHolder, TitleKind,
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
fn job_hover(strings: &TextDb, def: &JobDef) -> String {
    let mut text = def.summary.clone();
    let mut costs = Vec::new();
    for (amount, key) in [
        (def.wealth_cost, "ui.cost.wealth"),
        (def.manpower_cost, "ui.cost.manpower"),
        (def.supplies_cost, "ui.cost.supplies"),
        (def.influence_cost, "ui.cost.influence"),
    ] {
        if amount > 0 {
            costs.push(strings.format(key, &[("amount", &amount.to_string())]));
        }
    }
    if !costs.is_empty() {
        text.push('\n');
        text.push_str(&strings.format("ui.job.costs", &[("costs", &costs.join(", "))]));
    }
    if !def.risks.is_empty() {
        let risks: Vec<&str> = def
            .risks
            .iter()
            .map(|risk| strings.text(risk.label_key()))
            .collect();
        text.push('\n');
        text.push_str(&strings.format("ui.job.risks", &[("risks", &risks.join(", "))]));
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
    data: &PanelData,
    cache: &ForecastCache,
    form: &mut JobForm,
    queue: &mut UiCommandQueue,
    picker: &mut PickerState,
) {
    // The simulation's leader_availability is the single source for
    // whether someone can take on new work; the interface only asks. A
    // standing command (a general, a captain) is not a blocker in itself.
    let leader_ok = |id: CharacterId| -> bool {
        data.availability.of(id).is_some_and(|state| {
            matches!(
                state,
                LeaderAvailability::Available | LeaderAvailability::Assigned(_)
            )
        })
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

    let strings = data.strings.as_deref().expect("a campaign is running");
    ui.separator();
    ui.strong(strings.text("ui.actions.heading"));

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
                            state.describe(strings, |key| {
                                content
                                    .jobs
                                    .get(key)
                                    .map(|def| def.title.clone())
                                    .unwrap_or_else(|| key.to_string())
                            })
                        })
                        .unwrap_or_else(|| strings.text("ui.actions.unavailable").to_owned()),
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
                        .on_hover_text(job_hover(strings, def))
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
                                    ui, strings, def.target, content, politics,
                                    player_org, data, form,
                                );
                            }
                            draw_forecast(ui, &data.theme, strings, cache, form, picker, LeaderChoice::Free);
                            confirm_job(ui, strings, key, cache, form, queue);
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
                    .on_hover_text(job_hover(strings, def))
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
                        draw_forecast(ui, &data.theme, strings, cache, form, picker, LeaderChoice::Free);
                        confirm_job(ui, strings, key, cache, form, queue);
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
                    .on_hover_text(job_hover(strings, def))
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
                            pick_ship(ui, strings, player_org, data, form);
                        } else {
                            pick_army(ui, strings, player_org, data, form);
                        }
                        // Publish the resolved target and leader so the
                        // forecast is for exactly what would be ordered.
                        let army = form
                            .army
                            .and_then(|id| data.armies.iter().find(|a| a.id == id));
                        let ship = form
                            .ship
                            .and_then(|id| data.ships.iter().find(|s| s.id == id));
                        let action = province_action(def.target, province, army, ship);
                        form.target = action.target;
                        form.leader = action.leader;
                        // An obstacle is stated where the choice is made,
                        // not discovered after pressing Confirm.
                        if let Some(obstacle) = &action.obstacle {
                            ui.colored_label(
                                data.theme.semantics.target(TargetState::IneligibleFixable),
                                strings.text(obstacle.text_key()),
                            );
                        }
                        draw_forecast(
                            ui,
                            &data.theme,
                            strings,
                            cache,
                            form,
                            picker,
                            LeaderChoice::Fixed("ui.actions.leader-fixed"),
                        );
                        confirm_job(ui, strings, key, cache, form, queue);
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
    strings: &TextDb,
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
        .add_enabled(ready, egui::Button::new(strings.text("ui.actions.confirm")))
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
    /// Fixed by what is being ordered, holding the key of the reason why.
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
    strings: &TextDb,
    cache: &ForecastCache,
    form: &mut JobForm,
    picker: &mut PickerState,
    choice: LeaderChoice,
) {
    draw_leader_slot(ui, theme, strings, cache, form, picker, choice);

    let Some(view) = &cache.forecast else {
        ui.weak(strings.text("ui.actions.forecast-pending"));
        return;
    };

    egui::Frame::group(ui.style()).show(ui, |ui| {
        draw_forecast_body(ui, theme, strings, view);
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
    strings: &TextDb,
    cache: &ForecastCache,
    form: &mut JobForm,
    picker: &mut PickerState,
    choice: LeaderChoice,
) {
    let chosen = form
        .leader
        .and_then(|id| cache.leaders.iter().find(|option| option.id == id));

    ui.horizontal(|ui| {
        ui.label(strings.text("ui.actions.led-by"));
        match chosen {
            Some(option) => {
                ui.colored_label(
                    theme.semantics.target(TargetState::Valid),
                    strings.format(
                        "ui.actions.leader-chosen",
                        &[
                            ("leader", &option.name),
                            ("chance", &permille_text(option.success())),
                        ],
                    ),
                );
            }
            None => {
                ui.colored_label(
                    theme.semantics.target(TargetState::IneligibleFixable),
                    strings.text("ui.actions.no-leader"),
                );
            }
        }
        if let LeaderChoice::Fixed(reason) = choice {
            ui.weak(strings.format(
                "ui.actions.leader-fixed-note",
                &[("reason", strings.text(reason))],
            ));
            return;
        }
        let free = cache
            .leaders
            .iter()
            .filter(|option| option.blocked().is_none())
            .count();
        if ui
            .button(strings.text("ui.actions.choose-leader"))
            .on_hover_text(strings.format(
                "ui.actions.choose-leader.hover",
                &[
                    ("free", &free.to_string()),
                    ("total", &cache.leaders.len().to_string()),
                ],
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
#[derive(Debug, PartialEq, Eq)]
struct ProvinceAction {
    target: Option<JobTarget>,
    leader: Option<CharacterId>,
    /// Why this cannot be ordered yet, for showing at the slot.
    obstacle: Option<Obstacle>,
}

/// Something standing between a chosen force and the order it would carry.
///
/// Named rather than worded, so [`province_action`] stays pure over the
/// records it is given and its tests assert the reason rather than a
/// phrase any rewording would break.
#[derive(Debug, PartialEq, Eq)]
enum Obstacle {
    /// The ship has nobody to order it.
    ShipHasNoCaptain,
}

impl Obstacle {
    /// The key of the sentence explaining this obstacle.
    fn text_key(&self) -> &'static str {
        match self {
            Obstacle::ShipHasNoCaptain => "ui.actions.obstacle.ship-has-no-captain",
        }
    }
}

/// Resolves the order a chosen force would carry out against a province.
///
/// Pure over the chosen force's records, so what the interface offers —
/// and refuses — is testable without a world or a frame.
fn province_action(
    kind: JobTargetKind,
    province: ProvinceId,
    army: Option<&ArmyRecord>,
    ship: Option<&ShipRecord>,
) -> ProvinceAction {
    match kind {
        JobTargetKind::OwnArmy | JobTargetKind::OwnArmyAndProvince => ProvinceAction {
            target: army.map(|a| match kind {
                JobTargetKind::OwnArmy => JobTarget::OwnArmy(a.id),
                _ => JobTarget::ArmyToProvince(a.id, province),
            }),
            leader: army.map(|a| a.general),
            obstacle: None,
        },
        JobTargetKind::OwnShipAndProvince => ProvinceAction {
            target: ship.map(|s| JobTarget::ShipToProvince(s.id, province)),
            leader: ship.and_then(|s| s.captain),
            obstacle: match ship {
                Some(ship) if ship.captain.is_none() => Some(Obstacle::ShipHasNoCaptain),
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

#[allow(clippy::too_many_arguments)]
fn pick_target(
    ui: &mut egui::Ui,
    strings: &TextDb,
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
                _ => strings.text("ui.actions.choose-org").to_owned(),
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
                _ => strings.text("ui.actions.choose-character").to_owned(),
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
                _ => strings.text("ui.actions.choose-province").to_owned(),
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

fn pick_army(
    ui: &mut egui::Ui,
    strings: &TextDb,
    player_org: OrgId,
    data: &PanelData,
    form: &mut JobForm,
) {
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
        .unwrap_or_else(|| strings.text("ui.actions.choose-army").to_owned());
    if armies.is_empty() {
        ui.weak(strings.text("ui.actions.no-armies"));
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

fn pick_ship(
    ui: &mut egui::Ui,
    strings: &TextDb,
    player_org: OrgId,
    data: &PanelData,
    form: &mut JobForm,
) {
    let mut ships: Vec<&ShipRecord> = data
        .ships
        .iter()
        .filter(|s| s.owner == player_org && matches!(s.location, ShipLocation::Docked(_)))
        .collect();
    ships.sort_by_key(|s| s.id);
    if ships.is_empty() {
        ui.weak(strings.text("ui.actions.no-ships"));
        return;
    }
    let label = form
        .ship
        .and_then(|id| ships.iter().find(|s| s.id == id))
        .map(|s| s.name.clone())
        .unwrap_or_else(|| strings.text("ui.actions.choose-ship").to_owned());
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

#[cfg(test)]
mod tests {
    use super::*;
    use aeon_sim::warfare::StandingOrder;
    use aeon_sim::{ArmyId, ShipId};

    fn army(general: u64) -> ArmyRecord {
        ArmyRecord {
            id: ArmyId::from_raw(10).unwrap(),
            name: "First Levy".to_owned(),
            owner: OrgId::from_raw(1).unwrap(),
            general: CharacterId::from_raw(general).unwrap(),
            manpower: 500,
            supplies: 100,
            location: ProvinceId::from_raw(3).unwrap(),
            standing_order: StandingOrder::default(),
        }
    }

    fn ship(captain: Option<u64>) -> ShipRecord {
        ShipRecord {
            id: ShipId::from_raw(20).unwrap(),
            key: aeon_data::ContentKey::new("lantern").unwrap(),
            name: "The Lantern".to_owned(),
            class: aeon_data::model::ShipClass::Capital,
            owner: OrgId::from_raw(1).unwrap(),
            captain: captain.map(|c| CharacterId::from_raw(c).unwrap()),
            location: ShipLocation::Docked(ProvinceId::from_raw(3).unwrap()),
            blockading: None,
        }
    }

    fn province() -> ProvinceId {
        ProvinceId::from_raw(7).unwrap()
    }

    #[test]
    fn a_march_is_led_by_the_armys_general() {
        let army = army(42);
        let action = province_action(
            JobTargetKind::OwnArmyAndProvince,
            province(),
            Some(&army),
            None,
        );
        assert_eq!(
            action.target,
            Some(JobTarget::ArmyToProvince(army.id, province()))
        );
        assert_eq!(action.leader, Some(army.general));
        assert_eq!(action.obstacle, None);
    }

    #[test]
    fn a_ship_without_a_captain_has_no_order_to_give() {
        let ship = ship(None);
        let action = province_action(
            JobTargetKind::OwnShipAndProvince,
            province(),
            None,
            Some(&ship),
        );
        assert_eq!(action.leader, None, "nobody is silently substituted");
        assert_eq!(
            action.obstacle,
            Some(Obstacle::ShipHasNoCaptain),
            "the obstacle is stated where the choice is made"
        );
    }

    #[test]
    fn a_captained_ship_is_ordered_by_its_captain() {
        let ship = ship(Some(9));
        let action = province_action(
            JobTargetKind::OwnShipAndProvince,
            province(),
            None,
            Some(&ship),
        );
        assert_eq!(
            action.target,
            Some(JobTarget::ShipToProvince(ship.id, province()))
        );
        assert_eq!(action.leader, ship.captain);
        assert_eq!(action.obstacle, None);
    }

    #[test]
    fn no_chosen_force_means_nothing_to_confirm() {
        let action = province_action(JobTargetKind::OwnArmy, province(), None, None);
        assert_eq!(action.target, None);
        assert_eq!(action.leader, None);
    }
}
