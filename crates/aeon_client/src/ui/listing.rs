//! The right-hand listing: what exists, as opposed to what is selected.
//!
//! Bodies in the system view; houses and the player's own forces in a body
//! view. Every row is a way into the inspector.

use aeon_data::ContentSet;
use aeon_data::model::ShipClass;
use aeon_sim::forces::{ArmyRecord, ShipLocation, ShipRecord};
use aeon_sim::{CharacterId, OrgId, PlayerCommand};
use bevy_egui::egui;

use crate::jobs_ui::UiCommandQueue;
use crate::ui::data::PanelData;
use crate::ui::lookup::Lookup;
use crate::view::{MapView, Selection, ViewState};

/// Draws the listing panel into the shell's viewport.
#[allow(clippy::too_many_arguments)]
pub fn draw_listing(
    viewport: &mut egui::Ui,
    lookup: &Lookup,
    data: &PanelData,
    content: &ContentSet,
    player_org: Option<OrgId>,
    _player_head: Option<CharacterId>,
    view: &mut ViewState,
    queue: &mut UiCommandQueue,
) {
    egui::Panel::right("listing")
        .default_size(230.0)
        .show(viewport, |ui| match view.view {
            MapView::System => {
                ui.heading("Bodies");
                ui.separator();
                let mut sorted: Vec<_> = data.bodies.iter().collect();
                sorted.sort_by_key(|(record, _)| record.id);
                for (record, name) in sorted {
                    let selected = view.selected == Some(Selection::Body(record.id));
                    if ui.selectable_label(selected, &name.0).clicked() {
                        view.selected = Some(Selection::Body(record.id));
                    }
                }

                ui.add_space(8.0);
                ui.heading("Houses");
                ui.separator();
                for (org_id, (record, _)) in &lookup.orgs {
                    let def = content.organisations.get(&record.key);
                    let label = def.map(|d| d.name.clone()).unwrap_or_default();
                    let selected = view.selected == Some(Selection::Org(*org_id));
                    if ui.selectable_label(selected, label).clicked() {
                        view.selected = Some(Selection::Org(*org_id));
                    }
                }

                ui.add_space(8.0);
                ui.heading("Forces");
                ui.separator();
                egui::ScrollArea::vertical()
                    .id_salt("forces")
                    .show(ui, |ui| {
                        let mut ships: Vec<&ShipRecord> = data.ships.iter().collect();
                        ships.sort_by_key(|s| s.id);
                        for ship in ships {
                            let class = match ship.class {
                                ShipClass::Capital => "Capital",
                                ShipClass::Transport => "Transport",
                                ShipClass::Patrol => "Patrol",
                            };
                            let place = match ship.location {
                                ShipLocation::Docked(province) => lookup
                                    .province_names
                                    .get(&province)
                                    .map(|n| (*n).to_owned())
                                    .unwrap_or_default(),
                                ShipLocation::Transit { to, .. } => format!(
                                    "-> {}",
                                    lookup.province_names.get(&to).copied().unwrap_or("...")
                                ),
                            };
                            ui.horizontal(|ui| {
                                ui.label(format!("{} ({class}) — {place}", ship.name));
                            });
                            if Some(ship.owner) == player_org
                                && matches!(ship.location, ShipLocation::Docked(_))
                            {
                                egui::ComboBox::from_id_salt(("move-ship", ship.id))
                                    .selected_text("Move to...")
                                    .show_ui(ui, |ui| {
                                        let mut sorted: Vec<_> =
                                            lookup.province_names.iter().collect();
                                        sorted.sort_by_key(|(id, _)| **id);
                                        for (province, name) in sorted {
                                            if ui.selectable_label(false, *name).clicked() {
                                                queue.0.push(PlayerCommand::MoveShip {
                                                    ship: ship.id,
                                                    destination: *province,
                                                });
                                            }
                                        }
                                    });
                            }
                        }

                        let mut armies: Vec<&ArmyRecord> = data.armies.iter().collect();
                        armies.sort_by_key(|a| a.id);
                        for army in armies {
                            let place = lookup
                                .province_names
                                .get(&army.location)
                                .copied()
                                .unwrap_or("...");
                            ui.horizontal(|ui| {
                                ui.label(format!(
                                    "{} — {} men, {} supplies — {place}",
                                    army.name, army.manpower, army.supplies
                                ));
                                if Some(army.owner) == player_org {
                                    let defending = army.standing_order
                                        == aeon_sim::warfare::StandingOrder::DefendHoldings;
                                    let label = if defending { "Defending" } else { "Hold fast" };
                                    if ui
                                        .small_button(label)
                                        .on_hover_text(
                                            "Toggle the standing order: defending armies \
                                             march to answer threats against your holdings",
                                        )
                                        .clicked()
                                    {
                                        let order = if defending {
                                            aeon_sim::warfare::StandingOrder::HoldFast
                                        } else {
                                            aeon_sim::warfare::StandingOrder::DefendHoldings
                                        };
                                        queue.0.push(PlayerCommand::SetStandingOrder {
                                            army: army.id,
                                            order,
                                        });
                                    }
                                    if ui.small_button("Disband").clicked() {
                                        queue.0.push(PlayerCommand::DisbandArmy { army: army.id });
                                    }
                                }
                            });
                        }
                    });
            }
            MapView::Body(body_id) => {
                ui.heading("Provinces");
                ui.separator();
                egui::ScrollArea::vertical().show(ui, |ui| {
                    let mut sorted: Vec<_> = data
                        .provinces
                        .iter()
                        .filter(|(record, _, _)| record.body == body_id)
                        .collect();
                    sorted.sort_by_key(|(record, _, _)| record.id);
                    for (record, name, _) in sorted {
                        let selected = view.selected == Some(Selection::Province(record.id));
                        if ui.selectable_label(selected, &name.0).clicked() {
                            view.selected = Some(Selection::Province(record.id));
                        }
                    }
                });
            }
        });
}
