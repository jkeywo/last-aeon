//! Jobs currently under way, and how long each has left.
//!
//! The other half of what used to be the bottom bar. Like the log, it now
//! fills whatever space its side gives it rather than assuming a fixed
//! strip along the bottom.

use aeon_sim::state::ContentDb;
use aeon_sim::{ActiveJob, OrgId, PlayerCommand};
use bevy::prelude::Query;
use bevy_egui::egui;

use crate::jobs_ui::UiCommandQueue;
use crate::ui::lookup::Lookup;

/// Draws the player's jobs in progress.
pub fn draw_jobs_panel(
    ui: &mut egui::Ui,
    lookup: &Lookup,
    content: &ContentDb,
    player_org: Option<OrgId>,
    date: aeon_core::calendar::GameDate,
    jobs: &Query<&ActiveJob>,
    queue: &mut UiCommandQueue,
) {
    egui::ScrollArea::vertical()
        .id_salt("jobs-scroll")
        .show(ui, |ui| {
            let mut sorted: Vec<&ActiveJob> = jobs
                .iter()
                .filter(|job| Some(job.owner) == player_org)
                .collect();
            sorted.sort_by_key(|job| job.id);
            if sorted.is_empty() {
                ui.label("No jobs under way.");
            }
            for job in sorted {
                let title = content
                    .0
                    .jobs
                    .get(&job.def)
                    .map(|def| def.title.as_str())
                    .unwrap_or("Unknown");
                let leader = lookup.char_name(job.leader);
                let remaining = date.days_until(job.completes).max(0);
                ui.horizontal(|ui| {
                    ui.label(format!("{title} — {leader} ({remaining}d left)"));
                    if ui.small_button("Cancel").clicked() {
                        queue.0.push(PlayerCommand::CancelJob { job: job.id });
                    }
                });
            }
        });
}
