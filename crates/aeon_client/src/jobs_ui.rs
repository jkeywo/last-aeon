//! Job UI: the bottom panel (message log + active jobs), result popups,
//! and the queue that carries UI intents into the authoritative command
//! pipeline. Jobs are *started* from context buttons in the inspector
//! (see `panels.rs`); this module runs everything after they are queued.

use std::collections::BTreeSet;

use aeon_data::ContentKey;
use aeon_sim::command::submit_command;
use aeon_sim::map::ProvinceRecord;
use aeon_sim::state::ContentDb;
use aeon_sim::{
    ActiveJob, ArmyId, CharacterId, CharacterRecord, JobTarget, LogChannel, LogEntry, LogSubject,
    MessageLog, OrgId, PendingPopups, PlayerCommand, PoliticsIndex, ShipId,
};
use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};

use crate::sim_driver::TimeControl;
use crate::view::{MapView, Selection, ViewState};

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
    /// The outside character an expanded action is anchored to, so the
    /// picker does not follow the selection to someone else.
    pub about: Option<CharacterId>,
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
        self.about = None;
    }
}

/// What the message log is currently showing.
#[derive(Resource, Clone, Debug)]
pub struct LogFilter {
    /// Which channels are visible.
    pub channels: BTreeSet<LogChannel>,
    /// Free-text filter over entry text.
    pub text: String,
    /// Restrict to entries concerning the player's own house.
    pub mine_only: bool,
}

impl Default for LogFilter {
    fn default() -> Self {
        Self {
            channels: LogChannel::ALL.into_iter().collect(),
            text: String::new(),
            mine_only: false,
        }
    }
}

impl LogFilter {
    /// Whether an entry passes the current filter.
    fn admits(&self, entry: &LogEntry, player_org: Option<OrgId>) -> bool {
        if !self.channels.contains(&entry.channel) {
            return false;
        }
        if self.mine_only && (player_org.is_none() || entry.org != player_org) {
            return false;
        }
        let needle = self.text.trim().to_lowercase();
        needle.is_empty() || entry.text.to_lowercase().contains(&needle)
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

/// Height of the bottom bar.
///
/// Fixed rather than content-derived: egui persists a panel's *measured*
/// size after the first frame, so a bar whose height is decided by a
/// scroll area that in turn sizes itself from the available height will
/// collapse toward the 20px floor and take its contents with it.
const BOTTOM_BAR_HEIGHT: f32 = 190.0;

/// Draws the bottom bar — message log and active jobs — into the shell's
/// viewport.
///
/// Takes the shell's `Ui` rather than building its own: two independent
/// root `Ui`s over the same rect do not know about each other's panels,
/// which is what left this bar overlapping the side panels and splitting
/// its columns at the middle of the *screen* instead of the middle of the
/// bar.
#[allow(clippy::too_many_arguments)]
pub(crate) fn draw_bottom_bar(
    viewport: &mut egui::Ui,
    content: &ContentDb,
    politics: &PoliticsIndex,
    date: aeon_core::calendar::GameDate,
    player_org: Option<aeon_sim::OrgId>,
    log: &MessageLog,
    filter: &mut LogFilter,
    view: &mut ViewState,
    queue: &mut UiCommandQueue,
    jobs: &Query<&ActiveJob>,
    characters: &Query<&CharacterRecord>,
    provinces: &Query<&ProvinceRecord>,
) {
    egui::Panel::bottom("jobs-bar")
        .exact_size(BOTTOM_BAR_HEIGHT)
        .show(viewport, |ui| {
            ui.columns(2, |columns| {
                // Message log, filterable and linked to its subjects.
                columns[0].horizontal_wrapped(|ui| {
                    ui.heading("Log");
                    for channel in LogChannel::ALL {
                        let mut on = filter.channels.contains(&channel);
                        if ui.toggle_value(&mut on, channel.label()).changed() {
                            if on {
                                filter.channels.insert(channel);
                            } else {
                                filter.channels.remove(&channel);
                            }
                        }
                    }
                    ui.toggle_value(&mut filter.mine_only, "Mine")
                        .on_hover_text("Show only entries concerning your own house.");
                    ui.add(
                        egui::TextEdit::singleline(&mut filter.text)
                            .hint_text("Filter…")
                            .desired_width(90.0),
                    );
                });
                egui::ScrollArea::vertical()
                    .id_salt("log-scroll")
                    .max_height(BOTTOM_BAR_HEIGHT - 48.0)
                    .stick_to_bottom(true)
                    .show(&mut columns[0], |ui| {
                        let visible: Vec<&LogEntry> = log
                            .entries
                            .iter()
                            .filter(|entry| filter.admits(entry, player_org))
                            .rev()
                            .take(200)
                            .collect();
                        if visible.is_empty() {
                            ui.weak("Nothing matches this filter.");
                        }
                        for entry in visible.into_iter().rev() {
                            ui.horizontal_wrapped(|ui| {
                                ui.weak(entry.date.to_string());
                                match entry.subject {
                                    // A subject makes the entry a way in.
                                    Some(subject) => {
                                        if ui
                                            .link(&entry.text)
                                            .on_hover_text("Show what this is about")
                                            .clicked()
                                        {
                                            match subject {
                                                LogSubject::Character(id) => {
                                                    view.selected = Some(Selection::Character(id));
                                                }
                                                LogSubject::Org(id) => {
                                                    view.selected = Some(Selection::Org(id));
                                                }
                                                LogSubject::Province(id) => {
                                                    view.selected = Some(Selection::Province(id));
                                                    if let Some(body) = provinces
                                                        .iter()
                                                        .find(|record| record.id == id)
                                                        .map(|record| record.body)
                                                    {
                                                        view.view = MapView::Body(body);
                                                    }
                                                }
                                            }
                                        }
                                    }
                                    None => {
                                        ui.label(&entry.text);
                                    }
                                }
                            });
                        }
                    });

                // Active jobs.
                columns[1].heading("Active Jobs");
                egui::ScrollArea::vertical()
                    .id_salt("jobs-scroll")
                    .max_height(BOTTOM_BAR_HEIGHT - 48.0)
                    .show(&mut columns[1], |ui| {
                        let mut sorted: Vec<&ActiveJob> = jobs
                            .iter()
                            .filter(|j| Some(j.owner) == player_org)
                            .collect();
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
}

/// Result popups: modal-style windows over the whole interface.
///
/// These float above every panel, so they need no place in the layout and
/// keep their own system.
pub fn draw_popups(
    mut contexts: EguiContexts,
    popups: Option<Res<PendingPopups>>,
    mut queue: ResMut<UiCommandQueue>,
) {
    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };
    let Some(popups) = popups else {
        return;
    };
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
