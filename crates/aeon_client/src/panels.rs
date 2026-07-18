//! Read-only 2D information panels plus presence and forces controls.
//!
//! A top bar (campaign, date, resources, time control, view breadcrumb),
//! a left inspector for the current selection (body, province, house, or
//! character, including location and travel), and a right listing panel
//! (bodies, houses, and the player's forces). Mutations travel through
//! the UI command queue into the authoritative command pipeline.

use std::collections::BTreeMap;

use aeon_data::model::{BodyKind, HouseTier, OrgKind, ShipClass};
use aeon_sim::economy::OrgResources;
use aeon_sim::forces::{ArmyRecord, ShipLocation, ShipRecord};
use aeon_sim::map::{BodyRecord, DisplayName, GeoPosition, ProvinceRecord};
use aeon_sim::politics::{
    CharacterSkills, CharacterTraits, CharacterView, Lineage, OpinionLedger, opinion_of,
};
use aeon_sim::presence::{CharacterLocation, Location};
use aeon_sim::state::{CampaignMeta, ContentDb};
use aeon_sim::{
    CampaignClock, CampaignOver, CharacterId, CharacterRecord, OrgId, OrgRecord, PlayerCommand,
    PlayerHouse, PoliticsIndex, ProvinceId, TitleHolder, TitleRecord,
};
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};

use crate::jobs_ui::UiCommandQueue;
use crate::sim_driver::{SPEED_STEPS, TimeControl};
use crate::view::{MapView, Selection, ViewState};

fn kind_label(kind: BodyKind) -> &'static str {
    match kind {
        BodyKind::Planet => "Planet",
        BodyKind::Moon => "Moon",
        BodyKind::Starbase => "Starbase",
    }
}

type CharacterQuery = (
    &'static CharacterRecord,
    &'static CharacterSkills,
    &'static CharacterTraits,
    &'static Lineage,
    &'static OpinionLedger,
);

type CharacterParts<'a> = (
    &'a CharacterRecord,
    &'a CharacterSkills,
    &'a CharacterTraits,
    &'a Lineage,
    &'a OpinionLedger,
);

/// Every world query the panels read, bundled to stay within system
/// parameter limits.
#[derive(SystemParam)]
pub struct PanelData<'w, 's> {
    bodies: Query<'w, 's, (&'static BodyRecord, &'static DisplayName)>,
    provinces: Query<
        'w,
        's,
        (
            &'static ProvinceRecord,
            &'static DisplayName,
            &'static GeoPosition,
        ),
    >,
    orgs: Query<'w, 's, (&'static OrgRecord, Option<&'static OrgResources>)>,
    characters: Query<'w, 's, CharacterQuery>,
    locations: Query<'w, 's, &'static CharacterLocation>,
    titles: Query<'w, 's, &'static TitleRecord>,
    ships: Query<'w, 's, &'static ShipRecord>,
    armies: Query<'w, 's, &'static ArmyRecord>,
}

#[allow(clippy::too_many_arguments)]
pub fn draw_panels(
    mut contexts: EguiContexts,
    clock: Option<Res<CampaignClock>>,
    meta: Option<Res<CampaignMeta>>,
    content: Option<Res<ContentDb>>,
    politics: Option<Res<PoliticsIndex>>,
    player: Option<Res<PlayerHouse>>,
    over: Option<Res<CampaignOver>>,
    mut control: ResMut<TimeControl>,
    mut view: ResMut<ViewState>,
    mut queue: ResMut<UiCommandQueue>,
    data: PanelData,
) {
    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };
    let (Some(clock), Some(meta), Some(content), Some(politics)) = (clock, meta, content, politics)
    else {
        return;
    };
    let date = clock.date;
    let player_org = player.as_ref().and_then(|p| p.0);

    let chars: BTreeMap<CharacterId, CharacterParts> = data
        .characters
        .iter()
        .map(|parts| (parts.0.id, parts))
        .collect();
    let org_records: BTreeMap<OrgId, (&OrgRecord, Option<&OrgResources>)> = data
        .orgs
        .iter()
        .map(|(record, resources)| (record.id, (record, resources)))
        .collect();
    let province_names: BTreeMap<ProvinceId, &str> = data
        .provinces
        .iter()
        .map(|(record, name, _)| (record.id, name.0.as_str()))
        .collect();
    let org_label = |id: OrgId| -> String {
        org_records
            .get(&id)
            .and_then(|(record, _)| content.0.organisations.get(&record.key))
            .map(|def| def.name.clone())
            .unwrap_or_else(|| id.to_string())
    };
    let location_label = |location: Option<&CharacterLocation>| -> String {
        match location.map(|l| l.0) {
            Some(Location::Province(province)) => province_names
                .get(&province)
                .map(|n| (*n).to_owned())
                .unwrap_or_default(),
            Some(Location::Transit { to, arrives }) => {
                let dest = province_names.get(&to).copied().unwrap_or("...");
                format!("In transit to {dest} (arrives {arrives})")
            }
            None => "Unknown".to_owned(),
        }
    };
    let player_head: Option<CharacterId> =
        player_org.and_then(|org| org_records.get(&org).and_then(|(r, _)| r.head));

    let mut viewport = egui::Ui::new(
        ctx.clone(),
        "viewport".into(),
        egui::UiBuilder::new()
            .layer_id(egui::LayerId::background())
            .max_rect(ctx.viewport_rect()),
    );

    egui::Panel::top("top-bar").show(&mut viewport, |ui| {
        ui.horizontal(|ui| {
            ui.strong(&meta.name);
            ui.separator();
            ui.monospace(date.to_string());
            ui.separator();

            if let Some((_, Some(resources))) = player_org.and_then(|org| org_records.get(&org)) {
                ui.label(format!(
                    "W {}  M {}  S {}  I {}/{}",
                    resources.wealth,
                    resources.manpower,
                    resources.supplies,
                    resources.influence,
                    resources.legitimacy,
                ))
                .on_hover_text("Wealth, Manpower, Supplies, Influence / Legitimacy");
                ui.separator();
            }

            let pause_label = if control.paused { "Resume" } else { "Pause" };
            if ui.button(pause_label).clicked() {
                control.paused = !control.paused;
            }
            for (index, speed) in SPEED_STEPS.iter().enumerate() {
                let active = (control.days_per_second - speed).abs() < f32::EPSILON;
                if ui
                    .selectable_label(active, format!("{}x", index + 1))
                    .clicked()
                {
                    control.days_per_second = *speed;
                }
            }
            ui.separator();

            match view.view {
                MapView::System => {
                    ui.label("Local System");
                }
                MapView::Body(id) => {
                    if ui.button("< System").clicked() {
                        view.view = MapView::System;
                    }
                    let name = data
                        .bodies
                        .iter()
                        .find(|(record, _)| record.id == id)
                        .map(|(_, name)| name.0.as_str())
                        .unwrap_or("Unknown");
                    ui.label(name);
                }
            }

            if let Some(over) = &over {
                ui.separator();
                ui.colored_label(
                    egui::Color32::from_rgb(220, 60, 60),
                    format!("CAMPAIGN OVER — {}", over.reason),
                );
            }
        });
    });

    egui::Panel::left("inspector")
        .default_size(260.0)
        .show(&mut viewport, |ui| {
            ui.heading("Inspector");
            ui.separator();
            match view.selected {
                None => {
                    ui.label("Select a body, province, house, or character.");
                }
                Some(Selection::Body(id)) => {
                    if let Some((record, name)) =
                        data.bodies.iter().find(|(record, _)| record.id == id)
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
                                data.provinces
                                    .iter()
                                    .filter(|(p, _, _)| p.body == id)
                                    .count()
                                    .to_string(),
                            );
                            ui.end_row();
                        });
                        if ui.button("Open strategic view").clicked() {
                            view.view = MapView::Body(id);
                        }
                    }
                }
                Some(Selection::Province(id)) => {
                    if let Some((record, name, geo)) =
                        data.provinces.iter().find(|(record, _, _)| record.id == id)
                    {
                        ui.strong(&name.0);
                        let body_name = data
                            .bodies
                            .iter()
                            .find(|(body, _)| body.id == record.body)
                            .map(|(_, name)| name.0.as_str())
                            .unwrap_or("Unknown");
                        ui.label(format!("Province of {body_name}"));
                        ui.separator();

                        let holder = politics
                            .province_titles
                            .get(&id)
                            .and_then(|title_id| politics.titles.get(title_id))
                            .and_then(|entity| data.titles.get(*entity).ok())
                            .map(|title| title.holder);
                        egui::Grid::new("province-facts").show(ui, |ui| {
                            ui.label("Held by");
                            match holder {
                                Some(TitleHolder::Org(org)) => {
                                    if ui.link(org_label(org)).clicked() {
                                        view.selected = Some(Selection::Org(org));
                                    }
                                }
                                Some(TitleHolder::Character(character)) => {
                                    let name = chars
                                        .get(&character)
                                        .map(|(r, ..)| r.name.clone())
                                        .unwrap_or_default();
                                    if ui.link(name).clicked() {
                                        view.selected = Some(Selection::Character(character));
                                    }
                                }
                                _ => {
                                    ui.label("No one");
                                }
                            }
                            ui.end_row();
                            if let Some(def) = content.0.provinces.get(&record.key) {
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
                    }
                }
                Some(Selection::Org(id)) => {
                    if let Some((record, resources)) = org_records.get(&id).copied() {
                        let def = content.0.organisations.get(&record.key);
                        ui.strong(def.map(|d| d.name.as_str()).unwrap_or("Unknown"));
                        let standing = match (record.kind, record.tier) {
                            (OrgKind::SanctoraImperim, _) => "Imperial government".to_owned(),
                            (_, Some(HouseTier::Great)) => "Great house".to_owned(),
                            (_, Some(HouseTier::Vassal)) => match record.liege {
                                Some(liege) => format!("Vassal of {}", org_label(liege)),
                                None => "Vassal".to_owned(),
                            },
                            (_, Some(HouseTier::Independent)) => "Independent house".to_owned(),
                            _ => String::new(),
                        };
                        ui.label(standing);
                        if record.defunct {
                            ui.colored_label(egui::Color32::from_rgb(220, 60, 60), "DEFUNCT");
                        }
                        if let Some(resources) = resources {
                            ui.label(format!(
                                "W {}  M {}  S {}  I {}/{}",
                                resources.wealth,
                                resources.manpower,
                                resources.supplies,
                                resources.influence,
                                resources.legitimacy,
                            ));
                        }
                        ui.separator();

                        ui.label("Head:");
                        match record.head.and_then(|h| chars.get(&h)) {
                            Some((head_record, ..)) => {
                                if ui.link(&head_record.name).clicked() {
                                    view.selected = Some(Selection::Character(head_record.id));
                                }
                            }
                            None => {
                                ui.label("None");
                            }
                        }

                        let held = data
                            .titles
                            .iter()
                            .filter(|t| t.holder == TitleHolder::Org(id))
                            .count();
                        ui.label(format!("Titles held: {held}"));

                        ui.separator();
                        ui.label("Members:");
                        egui::ScrollArea::vertical().show(ui, |ui| {
                            for (char_id, (record, ..)) in &chars {
                                if record.organisation != Some(id) || !record.alive() {
                                    continue;
                                }
                                if ui.link(&record.name).clicked() {
                                    view.selected = Some(Selection::Character(*char_id));
                                }
                            }
                        });
                    }
                }
                Some(Selection::Character(id)) => {
                    if let Some((record, skills, char_traits, lineage, _)) = chars.get(&id).copied()
                    {
                        ui.strong(&record.name);
                        match record.death {
                            None => {
                                ui.label(format!("Age {}", record.age_years(date)));
                            }
                            Some(death) => {
                                ui.label(format!("Died {death}"));
                            }
                        }
                        if let Some(org) = record.organisation
                            && ui.link(org_label(org)).clicked()
                        {
                            view.selected = Some(Selection::Org(org));
                        }

                        // Location and travel.
                        let location = politics
                            .characters
                            .get(&id)
                            .and_then(|e| data.locations.get(*e).ok());
                        ui.label(format!("At: {}", location_label(location)));
                        if record.alive()
                            && record.organisation == player_org
                            && let Some(Location::Province(at)) = location.map(|l| l.0)
                        {
                            egui::ComboBox::from_id_salt("travel-to")
                                .selected_text("Travel to...")
                                .show_ui(ui, |ui| {
                                    let mut sorted: Vec<_> = province_names.iter().collect();
                                    sorted.sort_by_key(|(id, _)| **id);
                                    for (province, name) in sorted {
                                        if *province == at {
                                            continue;
                                        }
                                        if ui.selectable_label(false, *name).clicked() {
                                            queue.0.push(PlayerCommand::Travel {
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
                            .filter_map(|key| content.0.traits.get(key))
                            .map(|def| def.name.clone())
                            .collect();
                        if !trait_names.is_empty() {
                            ui.label(format!("Traits: {}", trait_names.join(", ")));
                        }

                        ui.separator();
                        if let Some(spouse) = lineage.spouse
                            && let Some((spouse_record, ..)) = chars.get(&spouse)
                        {
                            ui.horizontal(|ui| {
                                ui.label("Spouse:");
                                if ui.link(&spouse_record.name).clicked() {
                                    view.selected = Some(Selection::Character(spouse));
                                }
                            });
                        }
                        for parent in &lineage.parents {
                            if let Some((parent_record, ..)) = chars.get(parent) {
                                ui.horizontal(|ui| {
                                    ui.label("Parent:");
                                    if ui.link(&parent_record.name).clicked() {
                                        view.selected = Some(Selection::Character(*parent));
                                    }
                                });
                            }
                        }

                        if let Some(head_id) = player_head
                            && head_id != id
                            && let (Some(head), Some(them)) = (chars.get(&head_id), chars.get(&id))
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
                                opinion_of(&content.0, date, as_view(head), as_view(them)),
                            ));
                            ui.label(format!(
                                "Their opinion of your head: {:+}",
                                opinion_of(&content.0, date, as_view(them), as_view(head)),
                            ));
                        }
                    }
                }
            }
        });

    egui::Panel::right("listing")
        .default_size(230.0)
        .show(&mut viewport, |ui| match view.view {
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
                for (org_id, (record, _)) in &org_records {
                    let def = content.0.organisations.get(&record.key);
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
                                ShipLocation::Docked(province) => province_names
                                    .get(&province)
                                    .map(|n| (*n).to_owned())
                                    .unwrap_or_default(),
                                ShipLocation::Transit { to, .. } => format!(
                                    "-> {}",
                                    province_names.get(&to).copied().unwrap_or("...")
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
                                        let mut sorted: Vec<_> = province_names.iter().collect();
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
                            let place =
                                province_names.get(&army.location).copied().unwrap_or("...");
                            ui.horizontal(|ui| {
                                ui.label(format!(
                                    "{} — {} men, {} supplies — {place}",
                                    army.name, army.manpower, army.supplies
                                ));
                                if Some(army.owner) == player_org
                                    && ui.small_button("Disband").clicked()
                                {
                                    queue.0.push(PlayerCommand::DisbandArmy { army: army.id });
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
