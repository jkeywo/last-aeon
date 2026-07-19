//! Job UI plumbing: the in-progress action form, the log filter, result
//! popups, and the queue that carries UI intents into the authoritative
//! command pipeline.
//!
//! Jobs are *started* from context buttons in the inspector, and the log
//! and jobs listings are dockable panels of their own. What is left here
//! is the shared state those surfaces read and write.

use std::collections::BTreeSet;

use aeon_data::ContentKey;
use aeon_sim::command::submit_command;
use aeon_sim::{
    ArmyId, CharacterId, JobTarget, LogChannel, LogEntry, OrgId, PendingPopups, PlayerCommand,
    ShipId,
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
    pub fn admits(&self, entry: &LogEntry, player_org: Option<OrgId>) -> bool {
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
