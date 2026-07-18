//! Job UI: the bottom panel (message log + active jobs), result popups,
//! and the queue that carries UI intents into the authoritative command
//! pipeline. Jobs are *started* from context buttons in the inspector
//! (see `panels.rs`); this module runs everything after they are queued.

use aeon_data::ContentKey;
use aeon_sim::command::submit_command;
use aeon_sim::state::ContentDb;
use aeon_sim::{
    ActiveJob, ArmyId, CampaignClock, CharacterId, CharacterRecord, JobTarget, MessageLog,
    PendingPopups, PlayerCommand, PlayerHouse, PoliticsIndex, ShipId,
};
use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};

use crate::sim_driver::TimeControl;

/// Player intents collected by UI systems this frame, submitted as
/// commands by [`flush_ui_commands`].
#[derive(Resource, Default)]
pub struct UiCommandQueue(pub Vec<PlayerCommand>);

/// The inspector's in-progress job choice, expanded by a context button
/// and filled in by inline pickers before it is confirmed.
#[derive(Resource, Default)]
pub struct JobForm {
    /// The job whose inline picker is currently expanded, if any.
    pub job: Option<ContentKey>,
    /// Chosen leader.
    pub leader: Option<CharacterId>,
    /// Chosen target.
    pub target: Option<JobTarget>,
    /// Chosen army, for military targets.
    pub army: Option<ArmyId>,
    /// Chosen ship, for blockade targets.
    pub ship: Option<ShipId>,
    /// Chosen destination province, for compound military targets.
    pub province: Option<aeon_sim::ProvinceId>,
    /// Last rejection message, shown until the next attempt.
    pub notice: Option<String>,
}

impl JobForm {
    /// Clears the in-progress choice after a command is queued.
    pub fn reset(&mut self) {
        self.job = None;
        self.leader = None;
        self.target = None;
        self.army = None;
        self.ship = None;
        self.province = None;
    }
}

/// Submits queued UI commands through the shared command pipeline.
pub fn flush_ui_commands(world: &mut World) {
    let queued: Vec<PlayerCommand> = std::mem::take(&mut world.resource_mut::<UiCommandQueue>().0);
    for command in queued {
        if let Err(rejection) = submit_command(world, command) {
            world.resource_mut::<JobForm>().notice = Some(rejection.to_string());
        }
    }
}

/// Pauses the campaign when a new popup arrives.
pub fn auto_pause_on_popups(
    popups: Option<Res<PendingPopups>>,
    mut control: ResMut<TimeControl>,
    mut last_seen: Local<u64>,
) {
    let Some(popups) = popups else {
        return;
    };
    if popups.next_id != *last_seen {
        *last_seen = popups.next_id;
        if !popups.popups.is_empty() {
            control.paused = true;
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub fn draw_jobs_ui(
    mut contexts: EguiContexts,
    clock: Option<Res<CampaignClock>>,
    content: Option<Res<ContentDb>>,
    politics: Option<Res<PoliticsIndex>>,
    player: Option<Res<PlayerHouse>>,
    popups: Option<Res<PendingPopups>>,
    log: Option<Res<MessageLog>>,
    mut queue: ResMut<UiCommandQueue>,
    jobs: Query<&ActiveJob>,
    characters: Query<&CharacterRecord>,
) {
    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };
    let (Some(clock), Some(content), Some(politics), Some(player), Some(popups), Some(log)) =
        (clock, content, politics, player, popups, log)
    else {
        return;
    };
    let date = clock.date;
    let Some(player_org) = player.0 else {
        return;
    };

    let mut viewport = egui::Ui::new(
        ctx.clone(),
        "jobs-viewport".into(),
        egui::UiBuilder::new()
            .layer_id(egui::LayerId::background())
            .max_rect(ctx.viewport_rect()),
    );

    // ------------------------------------------------------------------
    // Bottom panel: message log | active jobs.
    // ------------------------------------------------------------------
    egui::Panel::bottom("jobs-bar")
        .default_size(150.0)
        .show(&mut viewport, |ui| {
            ui.columns(2, |columns| {
                // Message log.
                columns[0].heading("Log");
                egui::ScrollArea::vertical()
                    .id_salt("log-scroll")
                    .stick_to_bottom(true)
                    .show(&mut columns[0], |ui| {
                        for entry in log.entries.iter().rev().take(100).rev() {
                            ui.label(format!("{}  {}", entry.date, entry.text));
                        }
                    });

                // Active jobs.
                columns[1].heading("Active Jobs");
                egui::ScrollArea::vertical()
                    .id_salt("jobs-scroll")
                    .show(&mut columns[1], |ui| {
                        let mut sorted: Vec<&ActiveJob> =
                            jobs.iter().filter(|j| j.owner == player_org).collect();
                        sorted.sort_by_key(|j| j.id);
                        if sorted.is_empty() {
                            ui.label("No jobs under way.");
                        }
                        for job in sorted {
                            let title = content
                                .0
                                .jobs
                                .get(&job.def)
                                .map(|d| d.title.as_str())
                                .unwrap_or("Unknown");
                            let leader = politics
                                .characters
                                .get(&job.leader)
                                .and_then(|e| characters.get(*e).ok())
                                .map(|r| r.name.clone())
                                .unwrap_or_default();
                            let remaining = date.days_until(job.completes).max(0);
                            ui.horizontal(|ui| {
                                ui.label(format!("{title} — {leader} ({remaining}d left)"));
                                if ui.small_button("Cancel").clicked() {
                                    queue.0.push(PlayerCommand::CancelJob { job: job.id });
                                }
                            });
                        }
                    });
            });
        });

    // ------------------------------------------------------------------
    // Result popups: modal-style windows, oldest first.
    // ------------------------------------------------------------------
    if let Some(popup) = popups.popups.first() {
        egui::Window::new("A Matter Requires Your Attention")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.label(&popup.text);
                ui.separator();
                ui.horizontal(|ui| {
                    for (choice_id, label) in &popup.choices {
                        if ui.button(label).clicked() {
                            queue.0.push(PlayerCommand::AnswerPopup {
                                popup: popup.id,
                                choice: choice_id.clone(),
                            });
                        }
                    }
                });
            });
    }
}
