//! The right-hand listing: what exists, as opposed to what is selected.
//!
//! Bodies in the system out.view; houses and the player's own forces in a body
//! out.view. Every row is a way into the inspector.

use aeon_data::model::ShipClass;
use aeon_sim::PlayerCommand;
use aeon_sim::forces::{ArmyRecord, ShipLocation, ShipRecord};
use bevy_egui::egui;

use crate::ui::panel::{PanelCtx, PanelOut};
use crate::view::{MapView, Selection};

/// Draws the listing's contents.
pub fn draw_listing(ui: &mut egui::Ui, ctx: &PanelCtx, out: &mut PanelOut) {
    let strings = ctx.strings;
    match out.view.view {
        MapView::System => {
            ui.heading(strings.text("ui.listing.bodies"));
            ui.separator();
            let mut sorted: Vec<_> = ctx.data.bodies.iter().collect();
            sorted.sort_by_key(|(record, _)| record.id);
            for (record, name) in sorted {
                let selected = out.view.selected == Some(Selection::Body(record.id));
                if ui.selectable_label(selected, &name.0).clicked() {
                    out.view.selected = Some(Selection::Body(record.id));
                }
            }

            ui.add_space(8.0);
            ui.heading(strings.text("ui.listing.houses"));
            ui.separator();
            for (org_id, (record, _)) in &ctx.lookup.orgs {
                let def = ctx.content.organisations.get(&record.key);
                let label = def.map(|d| d.name.clone()).unwrap_or_default();
                let selected = out.view.selected == Some(Selection::Org(*org_id));
                if ui.selectable_label(selected, label).clicked() {
                    out.view.selected = Some(Selection::Org(*org_id));
                }
            }

            ui.add_space(8.0);
            ui.heading(strings.text("ui.listing.forces"));
            ui.separator();
            egui::ScrollArea::vertical()
                .id_salt("forces")
                .show(ui, |ui| {
                    let mut ships: Vec<&ShipRecord> = ctx.data.ships.iter().collect();
                    ships.sort_by_key(|s| s.id);
                    for ship in ships {
                        let class = match ship.class {
                            ShipClass::Capital => "ui.ship-class.capital",
                            ShipClass::Transport => "ui.ship-class.transport",
                            ShipClass::Patrol => "ui.ship-class.patrol",
                        };
                        let place = match ship.location {
                            ShipLocation::Docked(province) => ctx
                                .lookup
                                .province_names
                                .get(&province)
                                .map(|n| (*n).to_owned())
                                .unwrap_or_default(),
                            ShipLocation::Transit { to, .. } => format!(
                                "-> {}",
                                ctx.lookup.province_names.get(&to).copied().unwrap_or("...")
                            ),
                        };
                        ui.horizontal(|ui| {
                            ui.label(format!("{} ({class}) — {place}", ship.name));
                        });
                        if Some(ship.owner) == ctx.player_org
                            && matches!(ship.location, ShipLocation::Docked(_))
                        {
                            egui::ComboBox::from_id_salt(("move-ship", ship.id))
                                .selected_text(strings.text("ui.listing.move-to"))
                                .show_ui(ui, |ui| {
                                    let mut sorted: Vec<_> =
                                        ctx.lookup.province_names.iter().collect();
                                    sorted.sort_by_key(|(id, _)| **id);
                                    for (province, name) in sorted {
                                        if ui.selectable_label(false, *name).clicked() {
                                            out.queue.0.push(PlayerCommand::MoveShip {
                                                ship: ship.id,
                                                destination: *province,
                                            });
                                        }
                                    }
                                });
                        }
                    }

                    let mut armies: Vec<&ArmyRecord> = ctx.data.armies.iter().collect();
                    armies.sort_by_key(|a| a.id);
                    for army in armies {
                        let place = ctx
                            .lookup
                            .province_names
                            .get(&army.location)
                            .copied()
                            .unwrap_or("...");
                        ui.horizontal(|ui| {
                            ui.label(format!(
                                "{} — {} men, {} supplies — {place}",
                                army.name, army.manpower, army.supplies
                            ));
                            if Some(army.owner) == ctx.player_org {
                                let defending = army.standing_order
                                    == aeon_sim::warfare::StandingOrder::DefendHoldings;
                                let label = strings.text(if defending {
                                    "ui.listing.defending"
                                } else {
                                    "ui.listing.hold-fast"
                                });
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
                                    out.queue.0.push(PlayerCommand::SetStandingOrder {
                                        army: army.id,
                                        order,
                                    });
                                }
                                if ui
                                    .small_button(strings.text("ui.listing.disband"))
                                    .clicked()
                                {
                                    out.queue
                                        .0
                                        .push(PlayerCommand::DisbandArmy { army: army.id });
                                }
                            }
                        });
                    }
                });
        }
        MapView::Body(body_id) => {
            ui.heading(strings.text("ui.listing.provinces"));
            ui.separator();
            egui::ScrollArea::vertical().show(ui, |ui| {
                let mut sorted: Vec<_> = ctx
                    .data
                    .provinces
                    .iter()
                    .filter(|(record, _, _)| record.body == body_id)
                    .collect();
                sorted.sort_by_key(|(record, _, _)| record.id);
                for (record, name, _) in sorted {
                    let selected = out.view.selected == Some(Selection::Province(record.id));
                    if ui.selectable_label(selected, &name.0).clicked() {
                        out.view.selected = Some(Selection::Province(record.id));
                    }
                }
            });
        }
    }
}
