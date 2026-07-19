//! The inspector: what the current selection is, in detail.
//!
//! One arm per kind of thing that can be selected. Each arm reads through
//! the shared [`Lookup`] and writes only through the mutable state it is
//! handed, so an arm can be read — and changed — without reference to its
//! neighbours.

use aeon_data::model::{HouseTier, OrgKind};
use aeon_sim::forces::{ArmyRecord, ShipLocation, ShipRecord};
use aeon_sim::obligations::ObligationRecord;
use aeon_sim::order::ORDER_MAX;
use aeon_sim::politics::{CharacterView, opinion_of};
use aeon_sim::presence::Location;
use aeon_sim::{PlayerCommand, TitleHolder};
use bevy_egui::egui;

use crate::ui::actions::{JobScope, draw_context_jobs};
use crate::ui::data::CharacterParts;
use crate::ui::panel::{PanelCtx, PanelOut};
use crate::ui::theme::TargetState;
use crate::ui::widgets::{kind_label, linked, resource_readout};
use crate::view::{MapView, Selection};

/// Draws the inspector for whatever is currently selected.
pub fn draw_inspector(ui: &mut egui::Ui, ctx: &PanelCtx, out: &mut PanelOut) {
    match out.view.selected {
        None => {
            ui.label("Select a body, province, house, or character.");
        }
        Some(Selection::Body(id)) => {
            if let Some((record, name)) = ctx.data.bodies.iter().find(|(record, _)| record.id == id)
            {
                ui.strong(&name.0);
                ui.label(kind_label(record.kind));
                ui.separator();
                egui::Grid::new("body-facts").show(ui, |ui| {
                    ui.label("Stable ID");
                    ui.monospace(record.id.to_string());
                    ui.end_row();
                    ui.label("Radius");
                    ui.label(format!("{} km", record.radius_km));
                    ui.end_row();
                    ui.label("Provinces");
                    ui.label(
                        ctx.data
                            .provinces
                            .iter()
                            .filter(|(p, _, _)| p.body == id)
                            .count()
                            .to_string(),
                    );
                    ui.end_row();
                });
                if ui.button("Open strategic out.view").clicked() {
                    out.view.view = MapView::Body(id);
                }
            }
        }
        Some(Selection::Province(id)) => {
            if let Some((record, name, geo)) = ctx
                .data
                .provinces
                .iter()
                .find(|(record, _, _)| record.id == id)
            {
                ui.strong(&name.0);
                let body_name = ctx
                    .data
                    .bodies
                    .iter()
                    .find(|(body, _)| body.id == record.body)
                    .map(|(_, name)| name.0.as_str())
                    .unwrap_or("Unknown");
                ui.label(format!("Province of {body_name}"));
                ui.separator();

                let holder = ctx
                    .politics
                    .province_titles
                    .get(&id)
                    .and_then(|title_id| ctx.politics.titles.get(title_id))
                    .and_then(|entity| ctx.data.titles.get(*entity).ok())
                    .map(|title| title.holder);
                egui::Grid::new("province-facts").show(ui, |ui| {
                    ui.label("Held by");
                    match holder {
                        Some(TitleHolder::Org(org)) => {
                            if linked(ui, &ctx.lookup.org_name(org), &ctx.lookup.org_hover(org)) {
                                out.view.selected = Some(Selection::Org(org));
                            }
                        }
                        Some(TitleHolder::Character(character)) => {
                            let name = ctx
                                .lookup
                                .chars
                                .get(&character)
                                .map(|(r, ..)| r.name.clone())
                                .unwrap_or_default();
                            if linked(ui, &name, &ctx.lookup.char_hover(character)) {
                                out.view.selected = Some(Selection::Character(character));
                            }
                        }
                        _ => {
                            ui.label("No one");
                        }
                    }
                    ui.end_row();
                    if let Some(def) = ctx.content.provinces.get(&record.key) {
                        ui.label("Monthly output");
                        ui.label(format!(
                            "W {} / M {} / S {}",
                            def.wealth_output, def.manpower_output, def.supplies_output
                        ));
                        ui.end_row();
                    }
                    ui.label("Latitude");
                    ui.label(format!("{:.2}\u{00b0}", geo.latitude_mdeg as f32 / 1000.0));
                    ui.end_row();
                    ui.label("Longitude");
                    ui.label(format!("{:.2}\u{00b0}", geo.longitude_mdeg as f32 / 1000.0));
                    ui.end_row();
                });

                // What the active map ctx.mode says about it.
                if let Some(entry) = ctx.data.readout.provinces.get(&id)
                    && !entry.hint.is_empty()
                {
                    ui.separator();
                    ui.horizontal_wrapped(|ui| {
                        ui.weak(format!("{}:", ctx.mode.label()));
                        ui.label(&entry.hint);
                    });
                }

                // How governable the province is, and why.
                if let Some((_, state)) = ctx.data.order.iter().find(|(record, _)| record.id == id)
                {
                    ui.separator();
                    let percent = state.order * 100 / ORDER_MAX;
                    ui.horizontal(|ui| {
                        ui.label("Order");
                        let colour = if state.in_unrest() {
                            egui::Color32::from(ctx.data.theme.semantics.urgent)
                        } else if percent < 50 {
                            egui::Color32::from(ctx.data.theme.semantics.notable)
                        } else {
                            egui::Color32::from(ctx.data.theme.semantics.calm)
                        };
                        ui.colored_label(colour, format!("{percent}%"))
                            .on_hover_text(
                                "How governable this province is. It scales                                                  what the province pays and how reliably it                                                  is defended, and a province left in unrest                                                  will throw off its ruler entirely.",
                            );
                    });
                    if let Some(days) = state.days_to_revolt() {
                        ui.colored_label(
                            egui::Color32::from(ctx.data.theme.semantics.urgent),
                            format!("In unrest — revolts in {days} days"),
                        )
                        .on_hover_text(
                            "Garrison it, go there in person, or hold court                                              to restore order before the province is lost.",
                        );
                    }
                }

                // Forces standing at this province.
                let armies_here: Vec<&ArmyRecord> = ctx
                    .data
                    .armies
                    .iter()
                    .filter(|a| a.location == id)
                    .collect();
                let ships_here: Vec<&ShipRecord> = ctx
                    .data
                    .ships
                    .iter()
                    .filter(|s| matches!(s.location, ShipLocation::Docked(p) if p == id))
                    .collect();
                if !armies_here.is_empty() || !ships_here.is_empty() {
                    ui.separator();
                    ui.label("Forces here:");
                    for army in armies_here {
                        ui.horizontal(|ui| {
                            ui.label(format!("\u{2694} {} ({} men)", army.name, army.manpower));
                            if let Some((general, ..)) = ctx.lookup.chars.get(&army.general) {
                                ui.label("·");
                                if linked(ui, &general.name, &ctx.lookup.char_hover(army.general)) {
                                    out.view.selected = Some(Selection::Character(army.general));
                                }
                            }
                        });
                    }
                    for ship in ships_here {
                        ui.horizontal(|ui| {
                            ui.label(format!("\u{2693} {}", ship.name));
                            if let Some(captain) = ship.captain
                                && let Some((c, ..)) = ctx.lookup.chars.get(&captain)
                            {
                                ui.label("·");
                                if linked(ui, &c.name, &ctx.lookup.char_hover(captain)) {
                                    out.view.selected = Some(Selection::Character(captain));
                                }
                            }
                        });
                    }
                }

                if let Some(org) = ctx.player_org {
                    draw_context_jobs(
                        ui,
                        JobScope::Province(id),
                        ctx.content,
                        ctx.politics,
                        org,
                        ctx.player_head,
                        ctx.data,
                        &ctx.data.cache,
                        out.form,
                        out.queue,
                        out.picker,
                    );
                }
            }
        }
        Some(Selection::Org(id)) => {
            if let Some((record, resources)) = ctx.lookup.orgs.get(&id).copied() {
                let def = ctx.content.organisations.get(&record.key);
                ui.strong(def.map(|d| d.name.as_str()).unwrap_or("Unknown"));
                match (record.kind, record.tier) {
                    (OrgKind::SanctoraImperim, _) => {
                        ui.label("Imperial government");
                    }
                    (_, Some(HouseTier::Great)) => {
                        ui.label("Great house");
                    }
                    (_, Some(HouseTier::Vassal)) => {
                        ui.horizontal(|ui| {
                            ui.label("Vassal of");
                            match record.liege {
                                Some(liege) => {
                                    if linked(
                                        ui,
                                        &ctx.lookup.org_name(liege),
                                        &ctx.lookup.org_hover(liege),
                                    ) {
                                        out.view.selected = Some(Selection::Org(liege));
                                    }
                                }
                                None => {
                                    ui.label("—");
                                }
                            }
                        });
                    }
                    (_, Some(HouseTier::Independent)) => {
                        ui.label("Independent house");
                    }
                    _ => {}
                }
                if record.defunct {
                    ui.colored_label(
                        egui::Color32::from(ctx.data.theme.semantics.urgent),
                        "DEFUNCT",
                    );
                }
                if let Some(resources) = resources {
                    ui.horizontal(|ui| resource_readout(ui, resources));
                }
                ui.separator();

                ui.horizontal(|ui| {
                    ui.label("Head:");
                    match record.head.and_then(|h| ctx.lookup.chars.get(&h)) {
                        Some((head_record, ..)) => {
                            if linked(
                                ui,
                                &head_record.name,
                                &ctx.lookup.char_hover(head_record.id),
                            ) {
                                out.view.selected = Some(Selection::Character(head_record.id));
                            }
                        }
                        None => {
                            ui.label("None");
                        }
                    }
                });

                let held = ctx
                    .data
                    .titles
                    .iter()
                    .filter(|t| t.holder == TitleHolder::Org(id))
                    .count();
                ui.label(format!("Titles held: {held}"));

                // Standing obligations: what this house is
                // bound by, kept apart from what it feels.
                if let Some(ledger) = &ctx.data.obligations {
                    let mut entries: Vec<&ObligationRecord> = ledger.involving(id).collect();
                    entries.sort_by_key(|entry| (entry.kind, entry.id));
                    if !entries.is_empty() {
                        ui.separator();
                        ui.label("Obligations:").on_hover_text(
                            "Favours, promises and grievances binding this                                              house. These are political facts, separate                                              from how anyone feels: a house may dislike                                              its creditor and still owe it.",
                        );
                        for entry in entries {
                            let other = if entry.debtor == id {
                                entry.creditor
                            } else {
                                entry.debtor
                            };
                            let owes_out = entry.debtor == id;
                            let colour = match (entry.kind, owes_out) {
                                (aeon_sim::ObligationKind::Grievance, _) => ctx
                                    .data
                                    .theme
                                    .semantics
                                    .target(TargetState::IneligibleFixable),
                                (_, true) => egui::Color32::from(ctx.data.theme.semantics.notable),
                                (_, false) => egui::Color32::from(ctx.data.theme.semantics.valid),
                            };
                            let mut detail = format!(
                                "{}
Origin: {}",
                                entry.summary(|org| ctx.lookup.org_name(org)),
                                entry.origin
                            );
                            match entry.expires {
                                Some(expiry) => detail.push_str(&format!(
                                    "
Lapses {expiry} ({} days)",
                                    ctx.date.days_until(expiry).max(0)
                                )),
                                None => detail.push_str(
                                    "
Stands until settled",
                                ),
                            }
                            detail.push_str(&format!(
                                "
Weight {}",
                                entry.weight
                            ));
                            ui.horizontal(|ui| {
                                ui.colored_label(
                                    colour,
                                    format!(
                                        "{} {}",
                                        entry.kind.label(),
                                        if owes_out { "to" } else { "from" }
                                    ),
                                )
                                .on_hover_text(&detail);
                                if linked(
                                    ui,
                                    &ctx.lookup.org_name(other),
                                    &ctx.lookup.org_hover(other),
                                ) {
                                    out.view.selected = Some(Selection::Org(other));
                                }
                            });
                        }
                    }
                }

                ui.separator();
                ui.label("Members:");
                for (char_id, (record, ..)) in &ctx.lookup.chars {
                    if record.organisation != Some(id) || !record.alive() {
                        continue;
                    }
                    if linked(ui, &record.name, &ctx.lookup.char_hover(*char_id)) {
                        out.view.selected = Some(Selection::Character(*char_id));
                    }
                }
            }
        }
        Some(Selection::Character(id)) => {
            if let Some((record, skills, char_traits, lineage, _)) =
                ctx.lookup.chars.get(&id).copied()
            {
                ui.strong(&record.name);
                match record.death {
                    None => {
                        ui.label(format!("Age {}", record.age_years(ctx.date)));
                    }
                    Some(death) => {
                        ui.label(format!("Died {death}"));
                    }
                }
                if let Some(org) = record.organisation {
                    ui.horizontal(|ui| {
                        if linked(ui, &ctx.lookup.org_name(org), &ctx.lookup.org_hover(org)) {
                            out.view.selected = Some(Selection::Org(org));
                        }
                    });
                }

                // Location and travel.
                let location = ctx
                    .politics
                    .characters
                    .get(&id)
                    .and_then(|e| ctx.data.locations.get(*e).ok());
                ui.label(format!("At: {}", ctx.lookup.location_label(location)));
                if record.alive()
                    && record.organisation == ctx.player_org
                    && let Some(Location::Province(at)) = location.map(|l| l.0)
                {
                    egui::ComboBox::from_id_salt("travel-to")
                        .selected_text("Travel to...")
                        .show_ui(ui, |ui| {
                            let mut sorted: Vec<_> = ctx.lookup.province_names.iter().collect();
                            sorted.sort_by_key(|(id, _)| **id);
                            for (province, name) in sorted {
                                if *province == at {
                                    continue;
                                }
                                if ui.selectable_label(false, *name).clicked() {
                                    out.queue.0.push(PlayerCommand::Travel {
                                        character: id,
                                        destination: *province,
                                    });
                                }
                            }
                        });
                }
                ui.separator();

                egui::Grid::new("skills").show(ui, |ui| {
                    ui.label("Command");
                    ui.label(skills.0.command.to_string());
                    ui.end_row();
                    ui.label("Diplomacy");
                    ui.label(skills.0.diplomacy.to_string());
                    ui.end_row();
                    ui.label("Intrigue");
                    ui.label(skills.0.intrigue.to_string());
                    ui.end_row();
                    ui.label("Stewardship");
                    ui.label(skills.0.stewardship.to_string());
                    ui.end_row();
                });

                let trait_names: Vec<String> = char_traits
                    .0
                    .iter()
                    .filter_map(|key| ctx.content.traits.get(key))
                    .map(|def| def.name.clone())
                    .collect();
                if !trait_names.is_empty() {
                    ui.label(format!("Traits: {}", trait_names.join(", ")));
                }

                ui.separator();
                if let Some(spouse) = lineage.spouse
                    && let Some((spouse_record, ..)) = ctx.lookup.chars.get(&spouse)
                {
                    ui.horizontal(|ui| {
                        ui.label("Spouse:");
                        if linked(ui, &spouse_record.name, &ctx.lookup.char_hover(spouse)) {
                            out.view.selected = Some(Selection::Character(spouse));
                        }
                    });
                }
                for parent in &lineage.parents {
                    if let Some((parent_record, ..)) = ctx.lookup.chars.get(parent) {
                        ui.horizontal(|ui| {
                            ui.label("Parent:");
                            if linked(ui, &parent_record.name, &ctx.lookup.char_hover(*parent)) {
                                out.view.selected = Some(Selection::Character(*parent));
                            }
                        });
                    }
                }

                if let Some(head_id) = ctx.player_head
                    && head_id != id
                    && let (Some(head), Some(them)) =
                        (ctx.lookup.chars.get(&head_id), ctx.lookup.chars.get(&id))
                {
                    fn as_view<'a>(p: &CharacterParts<'a>) -> CharacterView<'a> {
                        CharacterView {
                            record: p.0,
                            traits: p.2,
                            lineage: p.3,
                            ledger: p.4,
                        }
                    }
                    ui.separator();
                    ui.label(format!(
                        "Your head's opinion of them: {:+}",
                        opinion_of(ctx.content, ctx.date, as_view(head), as_view(them)),
                    ));
                    ui.label(format!(
                        "Their opinion of your head: {:+}",
                        opinion_of(ctx.content, ctx.date, as_view(them), as_view(head)),
                    ));
                }

                if record.alive()
                    && let Some(org) = ctx.player_org
                {
                    let scope = if record.organisation == Some(org) {
                        JobScope::OwnCharacter(id)
                    } else {
                        JobScope::OutsideCharacter(id)
                    };
                    draw_context_jobs(
                        ui,
                        scope,
                        ctx.content,
                        ctx.politics,
                        org,
                        ctx.player_head,
                        ctx.data,
                        &ctx.data.cache,
                        out.form,
                        out.queue,
                        out.picker,
                    );
                }
            }
        }
    }
}
