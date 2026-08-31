//! Assignments currently under way, and how long each has left.
//!
//! The other half of what used to be the bottom bar. Like the log, it now
//! fills whatever space its side gives it rather than assuming a fixed
//! strip along the bottom.

use aeon_sim::command::PendingCommands;
use aeon_sim::state::ContentDb;
use aeon_sim::{ActiveAssignment, AssignmentTarget, OrgId, PlayerCommand};
use bevy::prelude::Query;
use bevy_egui::egui;

use crate::assignment_ui::UiCommandQueue;
use crate::ui::lookup::Lookup;

/// Draws the player's assignments in progress.
#[allow(clippy::too_many_arguments)]
pub fn draw_assignments_panel(
    ui: &mut egui::Ui,
    lookup: &Lookup,
    content: &ContentDb,
    player_org: Option<OrgId>,
    date: aeon_core::calendar::GameDate,
    assignments: &Query<&ActiveAssignment>,
    pending: Option<&PendingCommands>,
    queue: &mut UiCommandQueue,
) {
    let strings = lookup.strings;
    egui::ScrollArea::vertical()
        .id_salt("assignments-scroll")
        .show(ui, |ui| {
            let mut sorted: Vec<&ActiveAssignment> = assignments
                .iter()
                .filter(|assignment| Some(assignment.owner) == player_org)
                .collect();
            sorted.sort_by_key(|assignment| assignment.id);

            // Orders that have been given but not yet reached the field. An
            // order carries a delay and only becomes an active assignment
            // when a day ticks, so without this a player who starts one while
            // paused would see nothing at all. Shown ahead of what is under
            // way, and plainly marked as still on its way.
            let en_route: Vec<(String, String, aeon_core::calendar::GameDate)> = pending
                .map(|pending| {
                    pending
                        .entries()
                        .iter()
                        .filter_map(|envelope| match &envelope.command {
                            PlayerCommand::StartAssignment {
                                assignment,
                                leader,
                                target,
                            } => {
                                let title = content
                                    .0
                                    .assignments
                                    .get(assignment)
                                    .map(|def| def.title.as_str())
                                    .unwrap_or_else(|| strings.text("ui.inspector.unknown"));
                                Some((
                                    en_route_title(title, *target, player_org, lookup),
                                    lookup.char_name(*leader),
                                    envelope.day,
                                ))
                            }
                            PlayerCommand::StartSituationAssignment {
                                situation,
                                action,
                                leader,
                                target,
                                ..
                            } => {
                                let assignment = content
                                    .0
                                    .situations
                                    .get(&situation.definition)
                                    .and_then(|definition| {
                                        definition
                                            .actions
                                            .iter()
                                            .find(|candidate| candidate.key == *action)
                                    })
                                    .and_then(|action| {
                                        content.0.assignments.get(&action.assignment)
                                    });
                                let title = assignment
                                    .map(|def| def.title.as_str())
                                    .unwrap_or_else(|| strings.text("ui.inspector.unknown"));
                                Some((
                                    en_route_title(title, *target, player_org, lookup),
                                    lookup.char_name(*leader),
                                    envelope.day,
                                ))
                            }
                            _ => None,
                        })
                        .collect()
                })
                .unwrap_or_default();

            if sorted.is_empty() && en_route.is_empty() {
                ui.label(strings.text("ui.assignments.none"));
            }

            for (title, leader, arrives) in &en_route {
                ui.weak(strings.format(
                    "ui.assignments.en-route",
                    &[
                        ("assignment", title.as_str()),
                        ("leader", leader),
                        ("date", &arrives.to_string()),
                    ],
                ));
            }
            if !en_route.is_empty() && !sorted.is_empty() {
                ui.separator();
            }
            for assignment in sorted {
                let title = content
                    .0
                    .assignments
                    .get(&assignment.def)
                    .map(|def| def.title.as_str())
                    .unwrap_or_else(|| strings.text("ui.inspector.unknown"));
                let leader = lookup.char_name(assignment.leader);
                let remaining = date.days_until(assignment.completes).max(0);
                // Which phase it has reached, and whether it is still
                // yours to call off.
                let def = content.0.assignments.get(&assignment.def);
                let recallable = def.is_some_and(|def| assignment.interruptible_on(def, date));
                let phase = def
                    .filter(|def| def.stages.len() > 1)
                    .map(|def| def.stages[assignment.stage(def, date)].id.clone());
                ui.horizontal(|ui| {
                    ui.label(strings.format(
                        "ui.assignments.row",
                        &[
                            ("assignment", title),
                            ("leader", &leader),
                            ("days", &remaining.to_string()),
                        ],
                    ));
                    if let Some(phase) = &phase {
                        ui.weak(strings.format("ui.assignments.phase", &[("phase", phase)]));
                    }
                    if assignment.cancel_requested {
                        // The click landed; it is simply waiting for a
                        // phase that can be interrupted.
                        ui.weak(strings.text("ui.assignments.cancel-pending"));
                    } else if ui
                        .add_enabled(
                            recallable,
                            egui::Button::new(strings.text("ui.assignments.cancel")).small(),
                        )
                        .on_disabled_hover_text(strings.text("ui.assignments.cannot-recall"))
                        .clicked()
                    {
                        queue.0.push(PlayerCommand::CancelAssignment {
                            assignment: assignment.id,
                        });
                    }
                });
            }
        });
}

fn en_route_title(
    title: &str,
    target: AssignmentTarget,
    player_org: Option<OrgId>,
    lookup: &Lookup,
) -> String {
    let target = match target {
        AssignmentTarget::None => player_org.map(|org| lookup.org_display(org)),
        AssignmentTarget::Character(character) => Some(lookup.char_name(character)),
        AssignmentTarget::Org(org) => Some(lookup.org_display(org)),
        AssignmentTarget::Province(province) => Some(lookup.province_name(province)),
        AssignmentTarget::War(war) => Some(war.to_string()),
        AssignmentTarget::WarSide(war, side) => Some(format!("{war} ({side:?})")),
        AssignmentTarget::OwnArmy(army) => Some(army.to_string()),
        AssignmentTarget::ArmyToProvince(_, province)
        | AssignmentTarget::ShipToProvince(_, province) => Some(lookup.province_name(province)),
    }
    .filter(|label| !label.is_empty());
    match target {
        Some(target) => format!("{title} → {target}"),
        None => title.to_owned(),
    }
}
