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

use crate::ui::actions::{AssignmentScope, draw_context_assignments};
use crate::ui::data::CharacterParts;
use crate::ui::panel::{PanelCtx, PanelOut};
use crate::ui::theme::TargetState;
use crate::ui::widgets::{kind_label_key, linked, resource_readout};
use crate::view::{MapView, Selection};

/// Draws the inspector for whatever is currently selected.
pub fn draw_inspector(ui: &mut egui::Ui, ctx: &PanelCtx, out: &mut PanelOut) {
    let strings = ctx.strings;
    match out.view.selected {
        None => {
            ui.label(strings.text("ui.inspector.nothing-selected"));
        }
        Some(Selection::Body(id)) => {
            if let Some((record, name)) = ctx.data.bodies.iter().find(|(record, _)| record.id == id)
            {
                ui.strong(&name.0);
                ui.label(strings.text(kind_label_key(record.kind)));
                ui.separator();
                egui::Grid::new("body-facts").show(ui, |ui| {
                    ui.label(strings.text("ui.inspector.body.stable-id"));
                    ui.monospace(record.id.to_string());
                    ui.end_row();
                    ui.label(strings.text("ui.inspector.body.radius"));
                    ui.label(strings.format(
                        "ui.inspector.body.radius-value",
                        &[("km", &record.radius_km.to_string())],
                    ));
                    ui.end_row();
                    ui.label(strings.text("ui.inspector.body.provinces"));
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
                if ui
                    .button(strings.text("ui.inspector.body.open-map"))
                    .clicked()
                {
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
                    .unwrap_or_else(|| strings.text("ui.inspector.unknown"));
                ui.label(strings.format("ui.inspector.province.of-body", &[("body", body_name)]));
                ui.separator();

                let holder = ctx
                    .politics
                    .province_titles
                    .get(&id)
                    .and_then(|title_id| ctx.politics.titles.get(title_id))
                    .and_then(|entity| ctx.data.titles.get(*entity).ok())
                    .map(|title| title.holder);
                egui::Grid::new("province-facts").show(ui, |ui| {
                    ui.label(strings.text("ui.inspector.province.held-by"));
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
                            ui.label(strings.text("ui.inspector.province.no-holder"));
                        }
                    }
                    ui.end_row();
                    if let Some(def) = ctx.content.provinces.get(&record.key) {
                        ui.label(strings.text("ui.inspector.province.output"));
                        ui.label(strings.format(
                            "ui.inspector.province.output-value",
                            &[
                                ("wealth", &def.wealth_output.to_string()),
                                ("manpower", &def.manpower_output.to_string()),
                                ("supplies", &def.supplies_output.to_string()),
                            ],
                        ));
                        ui.end_row();
                    }
                    ui.label(strings.text("ui.inspector.province.latitude"));
                    ui.label(format!("{:.2}\u{00b0}", geo.latitude_mdeg as f32 / 1000.0));
                    ui.end_row();
                    ui.label(strings.text("ui.inspector.province.longitude"));
                    ui.label(format!("{:.2}\u{00b0}", geo.longitude_mdeg as f32 / 1000.0));
                    ui.end_row();
                });

                // What the active map ctx.mode says about it.
                if let Some(entry) = ctx.data.readout.provinces.get(&id)
                    && !entry.hint.is_empty()
                {
                    ui.separator();
                    ui.horizontal_wrapped(|ui| {
                        ui.weak(strings.format(
                            "ui.inspector.province.map-mode-says",
                            &[("mode", strings.text(&ctx.mode.label_key()))],
                        ));
                        ui.label(&entry.hint);
                    });
                }

                // How governable the province is, and why.
                if let Some((_, state)) = ctx.data.order.iter().find(|(record, _)| record.id == id)
                {
                    ui.separator();
                    let percent = state.order * 100 / ORDER_MAX;
                    ui.horizontal(|ui| {
                        ui.label(strings.text("ui.inspector.province.order"));
                        let colour = if state.in_unrest() {
                            egui::Color32::from(ctx.data.theme.semantics.urgent)
                        } else if percent < 50 {
                            egui::Color32::from(ctx.data.theme.semantics.notable)
                        } else {
                            egui::Color32::from(ctx.data.theme.semantics.calm)
                        };
                        ui.colored_label(colour, format!("{percent}%"))
                            .on_hover_text(strings.text("ui.inspector.province.order.hover"));
                    });
                    if let Some(days) = state.days_to_revolt() {
                        ui.colored_label(
                            egui::Color32::from(ctx.data.theme.semantics.urgent),
                            strings.format(
                                "ui.inspector.province.unrest",
                                &[("days", &days.to_string())],
                            ),
                        )
                        .on_hover_text(strings.text("ui.inspector.province.unrest.hover"));
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
                    ui.label(strings.text("ui.inspector.province.forces"));
                    for army in armies_here {
                        ui.horizontal(|ui| {
                            if linked(
                                ui,
                                &strings.format(
                                    "ui.inspector.province.army",
                                    &[("army", &army.name), ("men", &army.manpower.to_string())],
                                ),
                                strings.text("ui.inspector.province.army.hover"),
                            ) {
                                out.view.selected = Some(Selection::Army(army.id));
                            }
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
                            if linked(
                                ui,
                                &strings
                                    .format("ui.inspector.province.ship", &[("ship", &ship.name)]),
                                strings.text("ui.inspector.province.ship.hover"),
                            ) {
                                out.view.selected = Some(Selection::Ship(ship.id));
                            }
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
                    draw_context_assignments(
                        ui,
                        AssignmentScope::Province(id),
                        ctx.content,
                        ctx.data,
                        org,
                        ctx.player_head,
                        out.form,
                        out.popup,
                    );
                }
            }
        }
        Some(Selection::Army(id)) => {
            let Some(army) = ctx.data.armies.iter().find(|a| a.id == id) else {
                return;
            };
            ui.strong(&army.name);
            ui.label(strings.format(
                "ui.inspector.army.strength",
                &[
                    ("men", &army.manpower.to_string()),
                    ("supplies", &army.supplies.to_string()),
                ],
            ));
            ui.horizontal(|ui| {
                ui.label(strings.text("ui.inspector.army.standing-at"));
                let place = ctx.lookup.province_name(army.location);
                if linked(ui, &place, &place) {
                    out.view.selected = Some(Selection::Province(army.location));
                }
            });
            ui.horizontal(|ui| {
                ui.label(strings.text("ui.inspector.army.general"));
                let name = ctx.lookup.char_name(army.general);
                if linked(ui, &name, &ctx.lookup.char_hover(army.general)) {
                    out.view.selected = Some(Selection::Character(army.general));
                }
            });
            if Some(army.owner) == ctx.player_org {
                draw_standing_orders(ui, ctx, out, &army.name, id, &army.standing_order);
                draw_context_assignments(
                    ui,
                    AssignmentScope::Army(id),
                    ctx.content,
                    ctx.data,
                    army.owner,
                    ctx.player_head,
                    out.form,
                    out.popup,
                );
            }
        }
        Some(Selection::Ship(id)) => {
            let Some(ship) = ctx.data.ships.iter().find(|s| s.id == id) else {
                return;
            };
            ui.strong(&ship.name);
            ui.horizontal(|ui| {
                ui.label(strings.text("ui.inspector.ship.captain"));
                match ship.captain {
                    Some(captain) => {
                        let name = ctx.lookup.char_name(captain);
                        if linked(ui, &name, &ctx.lookup.char_hover(captain)) {
                            out.view.selected = Some(Selection::Character(captain));
                        }
                    }
                    None => {
                        ui.weak(strings.text("ui.inspector.ship.no-captain"));
                    }
                }
            });
            if Some(ship.owner) == ctx.player_org {
                draw_context_assignments(
                    ui,
                    AssignmentScope::Ship(id),
                    ctx.content,
                    ctx.data,
                    ship.owner,
                    ctx.player_head,
                    out.form,
                    out.popup,
                );
            }
        }
        Some(Selection::Org(id)) => {
            if let Some((record, resources)) = ctx.lookup.orgs.get(&id).copied() {
                let def = ctx.content.organisations.get(&record.key);
                ui.strong(
                    def.map(|d| d.name.as_str())
                        .unwrap_or_else(|| strings.text("ui.inspector.unknown")),
                );
                match (record.kind, record.tier) {
                    (OrgKind::SanctoraImperim, _) => {
                        ui.label(strings.text("ui.inspector.org.imperial"));
                    }
                    (_, Some(HouseTier::Great)) => {
                        ui.label(strings.text("ui.inspector.org.great-house"));
                    }
                    (_, Some(HouseTier::Vassal)) => {
                        ui.horizontal(|ui| {
                            ui.label(strings.text("ui.inspector.org.vassal-of"));
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
                                    ui.label(strings.text("ui.inspector.org.no-liege"));
                                }
                            }
                        });
                    }
                    (_, Some(HouseTier::Independent)) => {
                        ui.label(strings.text("ui.inspector.org.independent"));
                    }
                    _ => {}
                }
                if record.defunct {
                    ui.colored_label(
                        egui::Color32::from(ctx.data.theme.semantics.urgent),
                        strings.text("ui.inspector.org.defunct"),
                    );
                }
                if let Some(resources) = resources {
                    ui.horizontal(|ui| resource_readout(ui, strings, resources));
                }
                ui.separator();

                ui.horizontal(|ui| {
                    ui.label(strings.text("ui.inspector.org.head"));
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
                            ui.label(strings.text("ui.inspector.org.no-head"));
                        }
                    }
                });

                let held = ctx
                    .data
                    .titles
                    .iter()
                    .filter(|t| t.holder == TitleHolder::Org(id))
                    .count();
                ui.label(strings.format(
                    "ui.inspector.org.titles-held",
                    &[("count", &held.to_string())],
                ));

                // Standing obligations: what this house is
                // bound by, kept apart from what it feels.
                if let Some(ledger) = &ctx.data.obligations {
                    let mut entries: Vec<&ObligationRecord> = ledger.involving(id).collect();
                    entries.sort_by_key(|entry| (entry.kind, entry.id));
                    if !entries.is_empty() {
                        ui.separator();
                        ui.label(strings.text("ui.inspector.org.obligations"))
                            .on_hover_text(strings.text("ui.inspector.org.obligations.hover"));
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
                                "{}\n{}",
                                entry.summary(strings, |org| ctx.lookup.org_name(org)),
                                strings
                                    .format("ui.obligation.origin", &[("origin", &entry.origin)])
                            );
                            detail.push('\n');
                            match entry.expires {
                                Some(expiry) => detail.push_str(&strings.format(
                                    "ui.obligation.lapses",
                                    &[
                                        ("date", &expiry.to_string()),
                                        ("days", &ctx.date.days_until(expiry).max(0).to_string()),
                                    ],
                                )),
                                None => detail.push_str(strings.text("ui.obligation.stands")),
                            }
                            detail.push('\n');
                            detail.push_str(&strings.format(
                                "ui.obligation.weight",
                                &[("weight", &entry.weight.to_string())],
                            ));
                            ui.horizontal(|ui| {
                                ui.colored_label(
                                    colour,
                                    strings.text(&if owes_out {
                                        entry.kind.owed_to_key()
                                    } else {
                                        entry.kind.owed_from_key()
                                    }),
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
                ui.label(strings.text("ui.inspector.org.members"));
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
                        ui.label(strings.format(
                            "ui.inspector.character.age",
                            &[("years", &record.age_years(ctx.date).to_string())],
                        ));
                    }
                    Some(death) => {
                        ui.label(strings.format(
                            "ui.inspector.character.died",
                            &[("date", &death.to_string())],
                        ));
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
                ui.label(strings.format(
                    "ui.inspector.character.at",
                    &[("place", &ctx.lookup.location_label(location))],
                ));
                if record.alive()
                    && record.organisation == ctx.player_org
                    && let Some(Location::Province(at)) = location.map(|l| l.0)
                {
                    egui::ComboBox::from_id_salt("travel-to")
                        .selected_text(strings.text("ui.inspector.character.travel-to"))
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
                // What an autonomous character has set their mind to. The
                // game's stance is that AI reasons are visible: the plan
                // is named openly, matching how the log already explains
                // why houses act.
                if record.organisation != ctx.player_org
                    && let Some(plan) = ctx.plans.and_then(|plans| plans.active.get(&id))
                    && let Some(def) = ctx.content.plans.get(&plan.def)
                {
                    ui.label(
                        strings.format("ui.inspector.character.pursuing", &[("plan", &def.title)]),
                    );
                }
                ui.separator();

                egui::Grid::new("skills").show(ui, |ui| {
                    ui.label(strings.text("ui.inspector.skill.command"));
                    ui.label(skills.0.command.to_string());
                    ui.end_row();
                    ui.label(strings.text("ui.inspector.skill.diplomacy"));
                    ui.label(skills.0.diplomacy.to_string());
                    ui.end_row();
                    ui.label(strings.text("ui.inspector.skill.intrigue"));
                    ui.label(skills.0.intrigue.to_string());
                    ui.end_row();
                    ui.label(strings.text("ui.inspector.skill.stewardship"));
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
                    ui.label(strings.format(
                        "ui.inspector.character.traits",
                        &[("traits", &trait_names.join(", "))],
                    ));
                }

                ui.separator();
                if let Some(spouse) = lineage.spouse
                    && let Some((spouse_record, ..)) = ctx.lookup.chars.get(&spouse)
                {
                    ui.horizontal(|ui| {
                        ui.label(strings.text("ui.inspector.character.spouse"));
                        if linked(ui, &spouse_record.name, &ctx.lookup.char_hover(spouse)) {
                            out.view.selected = Some(Selection::Character(spouse));
                        }
                    });
                }
                for parent in &lineage.parents {
                    if let Some((parent_record, ..)) = ctx.lookup.chars.get(parent) {
                        ui.horizontal(|ui| {
                            ui.label(strings.text("ui.inspector.character.parent"));
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
                    ui.label(strings.format(
                        "ui.inspector.character.opinion-of-them",
                        &[(
                            "opinion",
                            &format!(
                                "{:+}",
                                opinion_of(ctx.content, ctx.date, as_view(head), as_view(them))
                            ),
                        )],
                    ));
                    ui.label(strings.format(
                        "ui.inspector.character.opinion-of-you",
                        &[(
                            "opinion",
                            &format!(
                                "{:+}",
                                opinion_of(ctx.content, ctx.date, as_view(them), as_view(head))
                            ),
                        )],
                    ));
                }

                if record.alive()
                    && let Some(org) = ctx.player_org
                {
                    let scope = if record.organisation == Some(org) {
                        AssignmentScope::OwnCharacter(id)
                    } else {
                        AssignmentScope::OutsideCharacter(id)
                    };
                    draw_context_assignments(
                        ui,
                        scope,
                        ctx.content,
                        ctx.data,
                        org,
                        ctx.player_head,
                        out.form,
                        out.popup,
                    );
                }
            }
        }
    }
}

/// The list of assignments an army may start on its own.
///
/// Every assignment an army can be given is offered here, and the client
/// names none of them: it asks the content what an army does, and the
/// simulation whether the requirements hold. That is the whole reason the
/// old two-variant standing order had a content key compiled into the
/// engine, and why this one does not.
fn draw_standing_orders(
    ui: &mut egui::Ui,
    ctx: &PanelCtx,
    out: &mut PanelOut,
    army_name: &str,
    army: aeon_sim::ArmyId,
    current: &aeon_sim::warfare::StandingOrders,
) {
    let strings = ctx.strings;
    ui.separator();
    ui.strong(strings.text("ui.inspector.army.standing-orders"))
        .on_hover_text(strings.text("ui.inspector.army.standing-orders.hover"));
    let _ = army_name;

    let mut wanted = current.0.clone();
    let mut changed = false;
    let mut offered: Vec<_> = ctx
        .content
        .assignments
        .iter()
        .filter(|(_, def)| {
            matches!(
                def.target,
                aeon_data::model::AssignmentTargetKind::OwnArmy
                    | aeon_data::model::AssignmentTargetKind::OwnArmyAndProvince
            )
        })
        .collect();
    offered.sort_by(|a, b| a.1.title.cmp(&b.1.title));

    for (key, def) in offered {
        let mut on = wanted.contains(key);
        if ui.checkbox(&mut on, &def.title).changed() {
            if on {
                wanted.push(key.clone());
            } else {
                wanted.retain(|k| k != key);
            }
            changed = true;
        }
    }

    if current.0.is_empty() {
        ui.weak(strings.text("ui.inspector.army.no-standing-orders"));
    }
    if changed {
        out.queue
            .0
            .push(aeon_sim::PlayerCommand::SetStandingOrders {
                army,
                orders: aeon_sim::warfare::StandingOrders(wanted),
            });
    }
}
