//! Assignments currently under way, and how long each has left.
//!
//! The other half of what used to be the bottom bar. Like the log, it now
//! fills whatever space its side gives it rather than assuming a fixed
//! strip along the bottom.

use aeon_sim::command::PendingCommands;
use aeon_sim::state::ContentDb;
use aeon_sim::{ActiveAssignment, OrgId, PlayerCommand};
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
            let en_route: Vec<(&str, String, aeon_core::calendar::GameDate)> = pending
                .map(|pending| {
                    pending
                        .entries()
                        .iter()
                        .filter_map(|envelope| match &envelope.command {
                            PlayerCommand::StartAssignment {
                                assignment, leader, ..
                            } => {
                                let title = content
                                    .0
                                    .assignments
                                    .get(assignment)
                                    .map(|def| def.title.as_str())
                                    .unwrap_or_else(|| strings.text("ui.inspector.unknown"));
                                Some((title, lookup.char_name(*leader), envelope.day))
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
                        ("assignment", title),
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
