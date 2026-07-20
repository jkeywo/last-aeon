//! Composing an assignment: who goes, where, and whether the odds are
//! worth it.
//!
//! This used to expand in place under the button that opened it, inside a
//! docked panel a couple of hundred points wide. Choosing between people
//! is comparison work and comparison work wants room, so it is a window
//! now.
//!
//! Non-modal, like the character picker it contains and for the same
//! reason: the map, the log and the rest of the interface are all things
//! a player legitimately wants to consult while deciding, and a modal
//! makes each of them a reason to start over.
//!
//! Nothing here decides anything. The slots are the ones the assignment's
//! target kind asks for, the odds come from the simulation's forecast, and
//! Confirm is enabled by the same forecast that would refuse the order.

use aeon_data::model::AssignmentTargetKind;
use aeon_sim::state::ContentDb;
use aeon_sim::{AssignmentTarget, PlayerHouse, PoliticsIndex, TextDb};
use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};

use crate::assignment_ui::{AssignmentForm, UiCommandQueue};
use crate::ui::actions::{LeaderChoice, confirm_assignment, draw_forecast, force_leader};
use crate::ui::data::PanelData;
use crate::ui::picker::PickerState;
use crate::ui::theme::{TargetState, UiTheme};

/// Whether the assignment popup is up.
#[derive(Resource, Default)]
pub struct AssignmentPopup {
    /// Whether the window is showing.
    pub open: bool,
}

impl AssignmentPopup {
    /// Opens it.
    pub fn open(&mut self) {
        self.open = true;
    }
}

/// Draws the popup while an assignment is being composed.
#[allow(clippy::too_many_arguments)]
pub fn draw_assignment_popup(
    mut contexts: EguiContexts,
    mut popup: ResMut<AssignmentPopup>,
    mut form: ResMut<AssignmentForm>,
    mut queue: ResMut<UiCommandQueue>,
    mut picker: ResMut<PickerState>,
    content: Option<Res<ContentDb>>,
    politics: Option<Res<PoliticsIndex>>,
    player: Option<Res<PlayerHouse>>,
    theme: Res<UiTheme>,
    strings: Option<Res<TextDb>>,
    data: PanelData,
) {
    if !popup.open {
        return;
    }
    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };
    let (Some(content), Some(politics), Some(strings)) = (content, politics, strings) else {
        return;
    };
    // Nothing being composed means nothing to compose: the window closes
    // itself rather than standing empty.
    let Some(key) = form.assignment.clone() else {
        popup.open = false;
        return;
    };
    let Some(def) = content.0.assignments.get(&key).cloned() else {
        popup.open = false;
        return;
    };
    let Some(player_org) = player.as_ref().and_then(|house| house.0) else {
        return;
    };

    let mut open = true;
    egui::Window::new(&def.title)
        .id(egui::Id::new("assignment-popup"))
        .open(&mut open)
        .resizable(true)
        .default_width(f32::from(theme.components.picker_width))
        .show(ctx, |ui| {
            ui.label(&def.summary);
            ui.separator();

            // Only the slots this assignment genuinely leaves open. A
            // force's commander is not a choice, and a target the
            // selection already settled is not asked for again.
            let choice = match def.target {
                AssignmentTargetKind::OwnArmy
                | AssignmentTargetKind::OwnArmyAndProvince
                | AssignmentTargetKind::OwnShipAndProvince => {
                    LeaderChoice::Fixed("ui.actions.leader-fixed")
                }
                _ => LeaderChoice::Free,
            };

            match def.target {
                AssignmentTargetKind::OwnArmyAndProvince => {
                    let army = form.army;
                    crate::ui::actions::pick_destination(ui, &strings, &data, &mut form);
                    form.target = match (army, form.province) {
                        (Some(army), Some(to)) => Some(AssignmentTarget::ArmyToProvince(army, to)),
                        _ => None,
                    };
                    if let Some(record) =
                        army.and_then(|id| data.armies.iter().find(|a| a.id == id))
                    {
                        let (leader, _) = force_leader(Some(record), None);
                        form.leader = leader;
                    }
                }
                AssignmentTargetKind::OwnShipAndProvince => {
                    let ship = form.ship;
                    crate::ui::actions::pick_destination(ui, &strings, &data, &mut form);
                    form.target = match (ship, form.province) {
                        (Some(ship), Some(to)) => Some(AssignmentTarget::ShipToProvince(ship, to)),
                        _ => None,
                    };
                    let record = ship.and_then(|id| data.ships.iter().find(|s| s.id == id));
                    let (leader, obstacle) = force_leader(None, record);
                    form.leader = leader;
                    // Stated where the choice is made, not discovered
                    // after pressing Confirm.
                    if let Some(obstacle) = obstacle {
                        ui.colored_label(
                            theme.semantics.target(TargetState::IneligibleFixable),
                            strings.text(obstacle.text_key()),
                        );
                    }
                }
                AssignmentTargetKind::Organisation | AssignmentTargetKind::Province => {
                    // Settled by the selection when the button was pressed,
                    // unless the assignment was started from a character,
                    // in which case it still needs aiming.
                    if form.target.is_none() {
                        crate::ui::actions::pick_target(
                            ui, &strings, def.target, &content.0, &politics, player_org, &data,
                            &mut form,
                        );
                    }
                }
                AssignmentTargetKind::Character | AssignmentTargetKind::None => {}
                AssignmentTargetKind::OwnArmy => {}
            }

            draw_forecast(
                ui,
                &theme,
                &strings,
                &data.cache,
                &mut form,
                &mut picker,
                choice,
            );
            ui.separator();
            confirm_assignment(ui, &strings, &key, &data.cache, &mut form, &mut queue);
        });

    // Confirming clears the form, which is what closes the window: the
    // order has been given and there is nothing left to compose.
    if !open || form.assignment.is_none() {
        popup.open = false;
    }
}
