//! Context-sensitive actions: the assignment buttons under a selection, the
//! forecast they expand into, and the slot pickers that fill in whatever
//! the context does not already supply.
//!
//! Issuing stays on the authoritative path throughout: every action ends
//! as a queued `PlayerCommand::StartAssignment`, and nothing here decides whether
//! a assignment is allowed — it renders what the simulation's forecast reports.

use aeon_data::ContentSet;
use aeon_data::model::{AssignmentDef, AssignmentTargetKind};
use aeon_sim::forces::{ArmyRecord, ShipLocation, ShipRecord};
use aeon_sim::{
    ArmyId, AssignmentTarget, CharacterId, LeaderAvailability, OrgId, PlayerCommand, PoliticsIndex,
    ProvinceId, ShipId, TextDb,
};
use bevy_egui::egui;

use crate::assignment_ui::{AssignmentForm, UiCommandQueue};
use crate::forecast_view::ForecastCache;
use crate::ui::data::PanelData;
use crate::ui::forecast::{draw_forecast_body, permille_text};
use crate::ui::picker::PickerState;
use crate::ui::theme::{TargetState, UiTheme};

/// What the inspector's context-assignment section is anchored to.
pub enum AssignmentScope {
    /// A living adult member of the player's house, who leads the assignment.
    OwnCharacter(CharacterId),
    /// A character outside the player's house, targeted by the assignment.
    OutsideCharacter(CharacterId),
    /// A province, targeted by assignments that act on ground.
    Province(ProvinceId),
    /// One of the player's armies. Its own orders live here, under it,
    /// rather than under whatever province it happens to be standing in.
    Army(ArmyId),
    /// One of the player's ships.
    Ship(ShipId),
}

/// A tooltip summarising a assignment's effect, costs, and risks.
fn assignment_hover(strings: &TextDb, def: &AssignmentDef) -> String {
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
        text.push_str(&strings.format("ui.assignment.costs", &[("costs", &costs.join(", "))]));
    }
    if !def.risks.is_empty() {
        let risks: Vec<&str> = def
            .risks
            .iter()
            .map(|risk| strings.text(risk.label_key()))
            .collect();
        text.push('\n');
        text.push_str(&strings.format("ui.assignment.risks", &[("risks", &risks.join(", "))]));
    }
    text
}

/// Draws context-sensitive assignment buttons for the current selection, with an
/// inline picker for any slot the context does not already supply. Issuing
/// stays on the authoritative path: every action becomes a queued
/// [`PlayerCommand::StartAssignment`].
#[allow(clippy::too_many_arguments)]
pub fn draw_context_assignments(
    ui: &mut egui::Ui,
    scope: AssignmentScope,
    content: &ContentSet,
    politics: &PoliticsIndex,
    player_org: OrgId,
    player_head: Option<CharacterId>,
    data: &PanelData,
    cache: &ForecastCache,
    form: &mut AssignmentForm,
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
                LeaderAvailability::Available | LeaderAvailability::Posted(_)
            )
        })
    };
    let assignments_of =
        |kinds: &[AssignmentTargetKind]| -> Vec<(aeon_data::ContentKey, AssignmentDef)> {
            let mut assignments: Vec<(aeon_data::ContentKey, AssignmentDef)> = content
                .assignments
                .iter()
                .filter(|(_, d)| kinds.contains(&d.target))
                .map(|(k, d)| (k.clone(), d.clone()))
                .collect();
            assignments.sort_by(|a, b| a.1.title.cmp(&b.1.title));
            assignments
        };

    let strings = data.strings.as_deref().expect("a campaign is running");
    ui.separator();
    ui.strong(strings.text("ui.actions.heading"));

    // Every action expands to a forecast before it can be confirmed, so
    // nothing is ever committed to unseen.
    match scope {
        AssignmentScope::OwnCharacter(leader) => {
            if !leader_ok(leader) {
                // Say which of the several possible reasons applies.
                ui.weak(
                    data.availability
                        .of(leader)
                        .map(|state| {
                            state.describe(strings, |key| {
                                content
                                    .assignments
                                    .get(key)
                                    .map(|def| def.title.clone())
                                    .unwrap_or_else(|| key.to_string())
                            })
                        })
                        .unwrap_or_else(|| strings.text("ui.actions.unavailable").to_owned()),
                );
            } else {
                let assignments = assignments_of(&[
                    AssignmentTargetKind::None,
                    AssignmentTargetKind::Organisation,
                    AssignmentTargetKind::Character,
                    AssignmentTargetKind::Province,
                ]);
                for (key, def) in &assignments {
                    if ui
                        .button(&def.title)
                        .on_hover_text(assignment_hover(strings, def))
                        .clicked()
                    {
                        form.reset();
                        form.assignment = Some(key.clone());
                        form.leader = Some(leader);
                        form.about = Some(leader);
                        if def.target == AssignmentTargetKind::None {
                            form.target = Some(AssignmentTarget::None);
                        }
                    }
                    // Anchored to the character whose panel this is, not to
                    // the leader chosen: picking someone else to lead must
                    // not collapse the panel it was picked in.
                    let expanded =
                        form.assignment.as_ref() == Some(key) && form.about == Some(leader);
                    if expanded {
                        ui.indent(key.to_string(), |ui| {
                            if def.target != AssignmentTargetKind::None {
                                pick_target(
                                    ui, strings, def.target, content, politics, player_org, data,
                                    form,
                                );
                            }
                            draw_forecast(
                                ui,
                                &data.theme,
                                strings,
                                cache,
                                form,
                                picker,
                                LeaderChoice::Free,
                            );
                            confirm_assignment(ui, strings, key, cache, form, queue);
                        });
                    }
                }
            }
        }
        AssignmentScope::OutsideCharacter(target_char) => {
            // Every assignment that can be aimed at a character, filtered
            // by what the simulation says is legal against this one. The
            // client used to name "petition-the-consul" in its own source
            // and check the Consul title itself; the requirement is now
            // authored on the assignment, and this asks rather than knows.
            let offered: Vec<(aeon_data::ContentKey, AssignmentDef)> =
                assignments_of(&[AssignmentTargetKind::Character])
                    .into_iter()
                    .filter(|(key, _)| data.offers.allows(key))
                    .collect();

            for (key, def) in &offered {
                let targets_them = def.target == AssignmentTargetKind::Character;
                if ui
                    .button(&def.title)
                    .on_hover_text(assignment_hover(strings, def))
                    .clicked()
                {
                    form.reset();
                    form.assignment = Some(key.clone());
                    form.target = Some(if targets_them {
                        AssignmentTarget::Character(target_char)
                    } else {
                        AssignmentTarget::None
                    });
                    form.leader = player_head.filter(|h| leader_ok(*h));
                    form.about = Some(target_char);
                }
                let expanded =
                    form.assignment.as_ref() == Some(key) && form.about == Some(target_char);
                if expanded {
                    ui.indent(key.to_string(), |ui| {
                        draw_forecast(
                            ui,
                            &data.theme,
                            strings,
                            cache,
                            form,
                            picker,
                            LeaderChoice::Free,
                        );
                        confirm_assignment(ui, strings, key, cache, form, queue);
                    });
                }
            }
        }
        AssignmentScope::Province(province) => {
            // A province offers what can be done *to* a province. What an
            // army does is offered under the army: it is the army being
            // given the order, not the ground it happens to stand on.
            let assignments = assignments_of(&[AssignmentTargetKind::Province])
                .into_iter()
                .filter(|(key, _)| data.offers.allows(key))
                .collect::<Vec<_>>();
            if assignments.is_empty() {
                ui.weak(strings.text("ui.actions.none-for-province"));
            }
            for (key, def) in &assignments {
                if ui
                    .button(&def.title)
                    .on_hover_text(assignment_hover(strings, def))
                    .clicked()
                {
                    form.reset();
                    form.assignment = Some(key.clone());
                    form.province = Some(province);
                    form.target = Some(AssignmentTarget::Province(province));
                }
                let expanded =
                    form.assignment.as_ref() == Some(key) && form.province == Some(province);
                if expanded {
                    ui.indent(key.to_string(), |ui| {
                        draw_forecast(
                            ui,
                            &data.theme,
                            strings,
                            cache,
                            form,
                            picker,
                            LeaderChoice::Free,
                        );
                        confirm_assignment(ui, strings, key, cache, form, queue);
                    });
                }
            }
        }
        AssignmentScope::Army(army_id) => {
            let Some(army) = data.armies.iter().find(|a| a.id == army_id) else {
                return;
            };
            // An army is commanded by its general and by nobody else, so
            // there is no leader to choose here — only where to go.
            let assignments = assignments_of(&[
                AssignmentTargetKind::OwnArmy,
                AssignmentTargetKind::OwnArmyAndProvince,
            ]);
            for (key, def) in &assignments {
                if ui
                    .button(&def.title)
                    .on_hover_text(assignment_hover(strings, def))
                    .clicked()
                {
                    form.reset();
                    form.assignment = Some(key.clone());
                    form.army = Some(army_id);
                    form.leader = Some(army.general);
                }
                let expanded = form.assignment.as_ref() == Some(key) && form.army == Some(army_id);
                if expanded {
                    ui.indent(key.to_string(), |ui| {
                        if def.target == AssignmentTargetKind::OwnArmyAndProvince {
                            pick_destination(ui, strings, data, form);
                            form.target = form
                                .province
                                .map(|to| AssignmentTarget::ArmyToProvince(army_id, to));
                        } else {
                            form.target = Some(AssignmentTarget::OwnArmy(army_id));
                        }
                        let (leader, _) = force_leader(Some(army), None);
                        form.leader = leader;
                        draw_forecast(
                            ui,
                            &data.theme,
                            strings,
                            cache,
                            form,
                            picker,
                            LeaderChoice::Fixed("ui.actions.leader-fixed"),
                        );
                        confirm_assignment(ui, strings, key, cache, form, queue);
                    });
                }
            }
        }
        AssignmentScope::Ship(ship_id) => {
            let Some(ship) = data.ships.iter().find(|s| s.id == ship_id) else {
                return;
            };
            let assignments = assignments_of(&[AssignmentTargetKind::OwnShipAndProvince]);
            for (key, def) in &assignments {
                if ui
                    .button(&def.title)
                    .on_hover_text(assignment_hover(strings, def))
                    .clicked()
                {
                    form.reset();
                    form.assignment = Some(key.clone());
                    form.ship = Some(ship_id);
                }
                let expanded = form.assignment.as_ref() == Some(key) && form.ship == Some(ship_id);
                if expanded {
                    ui.indent(key.to_string(), |ui| {
                        pick_destination(ui, strings, data, form);
                        form.target = form
                            .province
                            .map(|to| AssignmentTarget::ShipToProvince(ship_id, to));
                        let (leader, obstacle) = force_leader(None, Some(ship));
                        form.leader = leader;
                        // Stated where the choice is made, not discovered
                        // after pressing Confirm.
                        if let Some(obstacle) = obstacle {
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
                        confirm_assignment(ui, strings, key, cache, form, queue);
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
fn confirm_assignment(
    ui: &mut egui::Ui,
    strings: &TextDb,
    key: &aeon_data::ContentKey,
    cache: &ForecastCache,
    form: &mut AssignmentForm,
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
        queue.0.push(PlayerCommand::StartAssignment {
            assignment: key.clone(),
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
    form: &mut AssignmentForm,
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
    form: &mut AssignmentForm,
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

#[allow(clippy::too_many_arguments)]
fn pick_target(
    ui: &mut egui::Ui,
    strings: &TextDb,
    kind: AssignmentTargetKind,
    content: &ContentSet,
    politics: &PoliticsIndex,
    player_org: OrgId,
    data: &PanelData,
    form: &mut AssignmentForm,
) {
    match kind {
        AssignmentTargetKind::Organisation => {
            let label = match form.target {
                Some(AssignmentTarget::Org(org)) => politics
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
                            .selectable_label(
                                form.target == Some(AssignmentTarget::Org(*id)),
                                &def.name,
                            )
                            .clicked()
                        {
                            form.target = Some(AssignmentTarget::Org(*id));
                        }
                    }
                });
        }
        AssignmentTargetKind::Character => {
            let label = match form.target {
                Some(AssignmentTarget::Character(id)) => politics
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
                            .selectable_label(
                                form.target == Some(AssignmentTarget::Character(id)),
                                &name,
                            )
                            .clicked()
                        {
                            form.target = Some(AssignmentTarget::Character(id));
                        }
                    }
                });
        }
        AssignmentTargetKind::Province => {
            let label = match form.target {
                Some(AssignmentTarget::Province(id)) => data
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
                                form.target == Some(AssignmentTarget::Province(record.id)),
                                &name.0,
                            )
                            .clicked()
                        {
                            form.target = Some(AssignmentTarget::Province(record.id));
                        }
                    }
                });
        }
        _ => {}
    }
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
        let (leader, obstacle) = force_leader(Some(&army), None);
        assert_eq!(leader, Some(army.general), "and by nobody else");
        assert_eq!(obstacle, None);
    }

    #[test]
    fn a_ship_without_a_captain_has_no_order_to_give() {
        let ship = ship(None);
        let (leader, obstacle) = force_leader(None, Some(&ship));
        assert_eq!(leader, None, "nobody is silently substituted");
        assert_eq!(
            obstacle,
            Some(Obstacle::ShipHasNoCaptain),
            "the obstacle is stated where the choice is made"
        );
    }

    #[test]
    fn a_captained_ship_is_ordered_by_its_captain() {
        let ship = ship(Some(9));
        let action = province_action(
            AssignmentTargetKind::OwnShipAndProvince,
            province(),
            None,
            Some(&ship),
        );
        assert_eq!(
            action.target,
            Some(AssignmentTarget::ShipToProvince(ship.id, province()))
        );
        assert_eq!(action.leader, ship.captain);
        assert_eq!(action.obstacle, None);
    }

    #[test]
    fn no_chosen_force_means_nothing_to_confirm() {
        let action = province_action(AssignmentTargetKind::OwnArmy, province(), None, None);
        assert_eq!(action.target, None);
        assert_eq!(action.leader, None);
    }
}

/// What stops a force being ordered.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Obstacle {
    /// The ship has no captain, so there is nobody to give the order to.
    ShipHasNoCaptain,
}

impl Obstacle {
    /// The key of the sentence explaining this obstacle.
    fn text_key(self) -> &'static str {
        match self {
            Obstacle::ShipHasNoCaptain => "ui.actions.no-captain",
        }
    }
}

/// Who gives this force its orders, and what stops it.
///
/// An army is commanded by its general and a ship by its captain; neither
/// post is substitutable, and a vacant one is an obstacle rather than an
/// excuse to fall back on the house head. Kept as a function of its own
/// because it is a rule, and rules are worth testing.
fn force_leader(
    army: Option<&ArmyRecord>,
    ship: Option<&ShipRecord>,
) -> (Option<CharacterId>, Option<Obstacle>) {
    if let Some(army) = army {
        return (Some(army.general), None);
    }
    match ship {
        Some(ship) => match ship.captain {
            Some(captain) => (Some(captain), None),
            None => (None, Some(Obstacle::ShipHasNoCaptain)),
        },
        None => (None, None),
    }
}

/// Picks the province a force is being sent to.
fn pick_destination(
    ui: &mut egui::Ui,
    strings: &TextDb,
    data: &PanelData,
    form: &mut AssignmentForm,
) {
    let label = form
        .province
        .and_then(|id| data.provinces.iter().find(|(r, _, _)| r.id == id))
        .map(|(_, name, _)| name.0.clone())
        .unwrap_or_else(|| strings.text("ui.actions.choose-destination").to_owned());
    egui::ComboBox::from_id_salt("ctx-destination")
        .selected_text(label)
        .show_ui(ui, |ui| {
            let mut sorted: Vec<_> = data.provinces.iter().collect();
            sorted.sort_by_key(|(r, _, _)| r.id);
            for (record, name, _) in sorted {
                if ui
                    .selectable_label(form.province == Some(record.id), &name.0)
                    .clicked()
                {
                    form.province = Some(record.id);
                }
            }
        });
}
