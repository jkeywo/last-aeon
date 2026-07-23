//! Assignment UI plumbing: the in-progress action form, the log filter, result
//! popups, and the queue that carries UI intents into the authoritative
//! command pipeline.
//!
//! Assignments are *started* from context buttons in the inspector, and the log
//! and assignments listings are dockable panels of their own. What is left here
//! is the shared state those surfaces read and write.

use std::collections::BTreeSet;

use aeon_data::ContentKey;
use aeon_sim::command::submit_command;
use aeon_sim::{
    ArmyId, AssignmentTarget, CharacterId, LogChannel, LogEntry, OrgId, PendingPopups,
    PlayerCommand, ShipId,
};
use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};

use crate::sim_driver::TimeControl;

/// Player intents collected by UI systems this frame, submitted as
/// commands by [`flush_ui_commands`].
#[derive(Resource, Default)]
pub struct UiCommandQueue(pub Vec<PlayerCommand>);

/// Which province slot a map-pick fills in, once the player clicks the map.
///
/// A province target and a force's destination are different fields on the
/// form, so the "pick on map" button records which one it is standing in for.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum ProvinceSlot {
    /// The assignment's own province target.
    Target,
    /// A force's destination.
    Destination,
}

/// The inspector's in-progress assignment choice, expanded by a context button
/// and filled in by inline pickers before it is confirmed.
#[derive(Resource, Default)]
pub struct AssignmentForm {
    /// The assignment whose inline picker is currently expanded, if any.
    pub assignment: Option<ContentKey>,
    /// Chosen leader.
    pub leader: Option<CharacterId>,
    /// Chosen target.
    pub target: Option<AssignmentTarget>,
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
    /// Free-text filter over the organisation picker's list.
    pub org_filter: String,
    /// Free-text filter over the province picker's list.
    pub province_filter: String,
    /// Set while the player is picking a province by clicking the map. The
    /// next province click on the globe fills this slot and clears the flag.
    pub map_pick: Option<ProvinceSlot>,
}

impl AssignmentForm {
    /// Clears the in-progress choice after a command is queued.
    pub fn reset(&mut self) {
        self.assignment = None;
        self.leader = None;
        self.target = None;
        self.army = None;
        self.ship = None;
        self.province = None;
        self.about = None;
        self.org_filter.clear();
        self.province_filter.clear();
        self.map_pick = None;
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
            world.resource_mut::<AssignmentForm>().notice = Some(rejection.to_string());
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

#[cfg(test)]
mod tests {
    use super::*;
    use aeon_core::calendar::GameDate;

    fn entry(channel: LogChannel, text: &str, org: Option<u64>) -> LogEntry {
        LogEntry::new(GameDate::from_days(1), text, channel)
            .by(org.map(|raw| OrgId::from_raw(raw).unwrap()))
    }

    #[test]
    fn the_default_filter_admits_everything() {
        let filter = LogFilter::default();
        for channel in LogChannel::ALL {
            assert!(filter.admits(&entry(channel, "anything", Some(1)), None));
        }
    }

    #[test]
    fn hidden_channels_are_filtered_out() {
        let mut filter = LogFilter::default();
        filter.channels.remove(&LogChannel::Military);
        assert!(!filter.admits(&entry(LogChannel::Military, "battle", Some(1)), None));
        assert!(filter.admits(&entry(LogChannel::Politics, "intrigue", Some(1)), None));
    }

    #[test]
    fn mine_only_keeps_the_players_own_entries() {
        let filter = LogFilter {
            mine_only: true,
            ..Default::default()
        };
        let player = OrgId::from_raw(1);
        assert!(filter.admits(&entry(LogChannel::Assignments, "ours", Some(1)), player));
        assert!(!filter.admits(&entry(LogChannel::Assignments, "theirs", Some(2)), player));
        assert!(
            !filter.admits(&entry(LogChannel::Assignments, "ours", Some(1)), None),
            "with no player house, mine-only admits nothing"
        );
    }

    #[test]
    fn the_text_filter_is_a_case_insensitive_substring() {
        let filter = LogFilter {
            text: "  Harrow ".to_owned(),
            ..Default::default()
        };
        assert!(filter.admits(
            &entry(LogChannel::Assignments, "House harrow marches", Some(1)),
            None
        ));
        assert!(!filter.admits(
            &entry(LogChannel::Assignments, "House Veyrin marches", Some(1)),
            None
        ));
    }
}
