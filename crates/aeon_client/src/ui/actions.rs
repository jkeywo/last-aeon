//! Context-sensitive actions: the assignment buttons under a selection, the
//! forecast they expand into, and the slot pickers that fill in whatever
//! the context does not already supply.
//!
//! Issuing stays on the authoritative path throughout: every action ends
//! as a queued `PlayerCommand::StartAssignment`, and nothing here decides whether
//! a assignment is allowed — it renders what the simulation's forecast reports.

use aeon_data::ContentSet;
use aeon_data::model::{AssignmentDef, AssignmentTargetKind};
use aeon_sim::forces::{ArmyRecord, ShipRecord};
use aeon_sim::{
    ArmyId, AssignmentTarget, CharacterId, LeaderAvailability, OrgId, PlayerCommand, PoliticsIndex,
    ProvinceId, ShipId, TextDb,
};
use bevy_egui::egui;

use crate::assignment_ui::{AssignmentForm, ProvinceSlot, UiCommandQueue};
use crate::forecast_view::ForecastCache;
use crate::ui::assignment_popup::AssignmentPopup;
use crate::ui::data::PanelData;
use crate::ui::forecast::{draw_forecast_body, permille_text};
use crate::ui::picker::PickerState;
use crate::ui::theme::{TargetState, UiTheme};

/// What the inspector's context-assignment section is anchored to.
pub enum AssignmentScope {
    /// A living adult member of the player's house, who leads the assignment.
    OwnCharacter(CharacterId),
    /// A character outside the player's house, targeted by the assignment.
    OutsideCharacter(CharacterId),
    /// A province, targeted by assignments that act on ground.
    Province(ProvinceId),
    /// One of the player's armies. Its own orders live here, under it,
    /// rather than under whatever province it happens to be standing in.
    Army(ArmyId),
    /// One of the player's ships.
    Ship(ShipId),
}

/// A tooltip summarising a assignment's effect, costs, and risks.
fn assignment_hover(strings: &TextDb, def: &AssignmentDef) -> String {
    let mut text = def.summary.clone();
    let mut costs = Vec::new();
    for (amount, key) in [
        (def.wealth_cost, "ui.cost.wealth"),
        (def.manpower_cost, "ui.cost.manpower"),
        (def.supplies_cost, "ui.cost.supplies"),
        (def.influence_cost, "ui.cost.influence"),
    ] {
        if amount > 0 {
            costs.push(strings.format(key, &[("amount", &amount.to_string())]));
        }
    }
    if !costs.is_empty() {
        text.push('\n');
        text.push_str(&strings.format("ui.assignment.costs", &[("costs", &costs.join(", "))]));
    }
    if !def.risks.is_empty() {
        let risks: Vec<&str> = def
            .risks
            .iter()
            .map(|risk| strings.text(risk.label_key()))
            .collect();
        text.push('\n');
        text.push_str(&strings.format("ui.assignment.risks", &[("risks", &risks.join(", "))]));
    }
    text
}

/// Draws the assignments this selection offers, as buttons.
///
/// A button no longer expands in place. Choosing who goes, where, and
/// whether the odds are worth it is comparison work that wants room, and
/// an expander inside a narrow docked panel gives it none — so the button
/// opens the assignment popup and the choosing happens there.
///
/// What is offered is asked of the simulation, never decided here.
#[allow(clippy::too_many_arguments)]
pub fn draw_context_assignments(
    ui: &mut egui::Ui,
    scope: AssignmentScope,
    content: &ContentSet,
    data: &PanelData,
    player_org: OrgId,
    player_head: Option<CharacterId>,
    form: &mut AssignmentForm,
    popup: &mut AssignmentPopup,
) {
    let assignments_of =
        |kinds: &[AssignmentTargetKind]| -> Vec<(aeon_data::ContentKey, AssignmentDef)> {
            let mut assignments: Vec<(aeon_data::ContentKey, AssignmentDef)> = content
                .assignments
                .iter()
                .filter(|(_, d)| kinds.contains(&d.target))
                .map(|(k, d)| (k.clone(), d.clone()))
                .collect();
            assignments.sort_by(|a, b| a.1.title.cmp(&b.1.title));
            assignments
        };

    let strings = data.strings.as_deref().expect("a campaign is running");
    ui.separator();
    ui.strong(strings.text("ui.actions.heading"));

    // What this selection may be given, and how the popup should be set up
    // when one is chosen.
    let offered: Vec<(aeon_data::ContentKey, AssignmentDef)> = match scope {
        AssignmentScope::OwnCharacter(_) => assignments_of(&[
            AssignmentTargetKind::None,
            AssignmentTargetKind::Organisation,
            AssignmentTargetKind::Character,
            AssignmentTargetKind::Province,
        ]),
        AssignmentScope::OutsideCharacter(_) => assignments_of(&[AssignmentTargetKind::Character])
            .into_iter()
            .filter(|(key, _)| data.offers.allows(key))
            .collect(),
        AssignmentScope::Province(_) => assignments_of(&[AssignmentTargetKind::Province])
            .into_iter()
            .filter(|(key, _)| data.offers.allows(key))
            .collect(),
        AssignmentScope::Army(_) => assignments_of(&[
            AssignmentTargetKind::OwnArmy,
            AssignmentTargetKind::OwnArmyAndProvince,
        ]),
        AssignmentScope::Ship(_) => assignments_of(&[AssignmentTargetKind::OwnShipAndProvince]),
    };

    // A character of ours who cannot take anything on says why, rather
    // than showing a list of buttons that would all be refused.
    if let AssignmentScope::OwnCharacter(who) = scope
        && let Some(state) = data.availability.of(who)
        && !matches!(
            state,
            LeaderAvailability::Available | LeaderAvailability::Posted(_)
        )
    {
        ui.weak(state.describe(strings, |key| {
            content
                .assignments
                .get(key)
                .map(|def| def.title.clone())
                .unwrap_or_else(|| key.to_string())
        }));
        return;
    }

    if offered.is_empty() {
        ui.weak(strings.text("ui.actions.none-here"));
    }

    for (key, def) in &offered {
        if ui
            .button(&def.title)
            .on_hover_text(assignment_hover(strings, def))
            .clicked()
        {
            form.reset();
            form.assignment = Some(key.clone());
            // Whatever the scope already settles is settled now, so the
            // popup only ever asks for what is genuinely still open.
            match scope {
                AssignmentScope::OwnCharacter(leader) => {
                    form.leader = Some(leader);
                    form.about = Some(leader);
                    if def.target == AssignmentTargetKind::None {
                        form.target = Some(AssignmentTarget::None);
                    }
                }
                AssignmentScope::OutsideCharacter(them) => {
                    form.about = Some(them);
                    form.target = Some(AssignmentTarget::Character(them));
                    form.leader = player_head;
                }
                AssignmentScope::Province(province) => {
                    form.province = Some(province);
                    form.target = Some(AssignmentTarget::Province(province));
                }
                AssignmentScope::Army(army) => {
                    form.army = Some(army);
                    let record = data.armies.iter().find(|a| a.id == army);
                    let (leader, _) = force_leader(record, None);
                    form.leader = leader;
                    if def.target == AssignmentTargetKind::OwnArmy {
                        form.target = Some(AssignmentTarget::OwnArmy(army));
                    }
                }
                AssignmentScope::Ship(ship) => {
                    form.ship = Some(ship);
                    let record = data.ships.iter().find(|s| s.id == ship);
                    let (leader, _) = force_leader(None, record);
                    form.leader = leader;
                }
            }
            let _ = player_org;
            popup.open();
        }
    }

    if let Some(notice) = &form.notice {
        ui.colored_label(
            data.theme.semantics.target(TargetState::IneligibleFixable),
            notice,
        );
    }
}

/// The Confirm button for an expanded action.
pub fn confirm_assignment(
    ui: &mut egui::Ui,
    strings: &TextDb,
    key: &aeon_data::ContentKey,
    cache: &ForecastCache,
    form: &mut AssignmentForm,
    queue: &mut UiCommandQueue,
) {
    // The forecast already knows whether this can be ordered; Confirm must
    // agree with it rather than letting the player press it and be refused.
    let forecast_allows = cache
        .forecast
        .as_ref()
        .map(|view| view.startable())
        .unwrap_or(false);
    let ready = form.leader.is_some() && form.target.is_some() && forecast_allows;
    if ui
        .add_enabled(ready, egui::Button::new(strings.text("ui.actions.confirm")))
        .clicked()
        && let (Some(leader), Some(target)) = (form.leader, form.target)
    {
        queue.0.push(PlayerCommand::StartAssignment {
            assignment: key.clone(),
            leader,
            target,
        });
        form.reset();
        form.notice = None;
    }
}

/// Whether the player picks who leads an action, or the action settles it.
#[derive(Copy, Clone, PartialEq, Eq)]
pub enum LeaderChoice {
    /// Any eligible member of the house may be chosen.
    Free,
    /// Fixed by what is being ordered, holding the key of the reason why.
    ///
    /// A force is led by the character who commands it and nobody else, so
    /// offering a picker for a march would be offering a choice that does
    /// not exist.
    Fixed(&'static str),
}

/// Renders the simulation's forecast for the expanded action, and the way
/// in to choosing who leads it.
///
/// The breakdown itself is drawn by [`draw_forecast_body`], shared with the
/// character picker, so the figures a player compares candidates on are the
/// figures they commit to.
pub fn draw_forecast(
    ui: &mut egui::Ui,
    theme: &UiTheme,
    strings: &TextDb,
    cache: &ForecastCache,
    form: &mut AssignmentForm,
    picker: &mut PickerState,
    choice: LeaderChoice,
) {
    draw_leader_slot(ui, theme, strings, cache, form, picker, choice);

    let Some(view) = &cache.forecast else {
        ui.weak(strings.text("ui.actions.forecast-pending"));
        return;
    };

    egui::Frame::group(ui.style()).show(ui, |ui| {
        draw_forecast_body(ui, theme, strings, view);
    });
}

/// Who leads this action, and the way to change it.
///
/// One control, in one place, for every action: the inline dropdown and the
/// separate "compare leaders" list it used to sit beside were two ways of
/// answering the same question, and they did not agree with each other.
fn draw_leader_slot(
    ui: &mut egui::Ui,
    theme: &UiTheme,
    strings: &TextDb,
    cache: &ForecastCache,
    form: &mut AssignmentForm,
    picker: &mut PickerState,
    choice: LeaderChoice,
) {
    let chosen = form
        .leader
        .and_then(|id| cache.leaders.iter().find(|option| option.id == id));

    ui.horizontal(|ui| {
        ui.label(strings.text("ui.actions.led-by"));
        match chosen {
            Some(option) => {
                ui.colored_label(
                    theme.semantics.target(TargetState::Valid),
                    strings.format(
                        "ui.actions.leader-chosen",
                        &[
                            ("leader", &option.name),
                            ("chance", &permille_text(option.success())),
                        ],
                    ),
                );
            }
            None => {
                ui.colored_label(
                    theme.semantics.target(TargetState::IneligibleFixable),
                    strings.text("ui.actions.no-leader"),
                );
            }
        }
        if let LeaderChoice::Fixed(reason) = choice {
            ui.weak(strings.format(
                "ui.actions.leader-fixed-note",
                &[("reason", strings.text(reason))],
            ));
            return;
        }
        let free = cache
            .leaders
            .iter()
            .filter(|option| option.blocked().is_none())
            .count();
        if ui
            .button(strings.text("ui.actions.choose-leader"))
            .on_hover_text(strings.format(
                "ui.actions.choose-leader.hover",
                &[
                    ("free", &free.to_string()),
                    ("total", &cache.leaders.len().to_string()),
                ],
            ))
            .clicked()
        {
            picker.open();
        }
    });
}

#[allow(clippy::too_many_arguments)]
pub fn pick_target(
    ui: &mut egui::Ui,
    strings: &TextDb,
    kind: AssignmentTargetKind,
    content: &ContentSet,
    politics: &PoliticsIndex,
    player_org: OrgId,
    data: &PanelData,
    form: &mut AssignmentForm,
) {
    match kind {
        AssignmentTargetKind::Organisation => {
            // A house is *great* when it is somebody's liege; that reads into
            // the picker exactly as it does in the lists.
            let great: std::collections::BTreeSet<OrgId> = data
                .orgs
                .iter()
                .filter_map(|(record, _)| record.liege)
                .collect();
            let mut items: Vec<(OrgId, String, Option<egui::Color32>)> = politics
                .orgs
                .iter()
                .filter(|(id, _)| **id != player_org)
                .filter_map(|(id, entity)| {
                    let (record, _) = data.orgs.get(*entity).ok()?;
                    let def = content.organisations.get(&record.key)?;
                    let name = if great.contains(id) {
                        strings.format("ui.house.great", &[("name", &def.name)])
                    } else {
                        def.name.clone()
                    };
                    let colour =
                        crate::ui::lookup::readable_on_dark(def.color.0, def.color.1, def.color.2);
                    Some((*id, name, Some(colour)))
                })
                .collect();
            items.sort_by(|a, b| a.1.cmp(&b.1));
            let current = form.target;
            if let Some(id) = filtered_list(
                ui,
                strings.text("ui.actions.filter-org"),
                &mut form.org_filter,
                "ctx-org",
                &items,
                |id| current == Some(AssignmentTarget::Org(id)),
            ) {
                form.target = Some(AssignmentTarget::Org(id));
            }
        }
        AssignmentTargetKind::Character => {
            let label = match form.target {
                Some(AssignmentTarget::Character(id)) => politics
                    .characters
                    .get(&id)
                    .and_then(|e| data.characters.get(*e).ok())
                    .map(|(r, ..)| r.name.clone())
                    .unwrap_or_default(),
                _ => strings.text("ui.actions.choose-character").to_owned(),
            };
            egui::ComboBox::from_id_salt("ctx-char")
                .selected_text(label)
                .show_ui(ui, |ui| {
                    let mut people: Vec<(CharacterId, String)> = politics
                        .characters
                        .iter()
                        .filter_map(|(id, e)| {
                            let (record, ..) = data.characters.get(*e).ok()?;
                            (record.alive() && record.organisation != Some(player_org))
                                .then(|| (*id, record.name.clone()))
                        })
                        .collect();
                    people.sort_by(|a, b| a.1.cmp(&b.1));
                    for (id, name) in people {
                        if ui
                            .selectable_label(
                                form.target == Some(AssignmentTarget::Character(id)),
                                &name,
                            )
                            .clicked()
                        {
                            form.target = Some(AssignmentTarget::Character(id));
                        }
                    }
                });
        }
        AssignmentTargetKind::Province => {
            let current = form.target;
            let picked = province_picker(
                ui,
                strings,
                data,
                &mut form.province_filter,
                "ctx-prov",
                |id| current == Some(AssignmentTarget::Province(id)),
            );
            if let Some(id) = picked {
                form.target = Some(AssignmentTarget::Province(id));
                form.map_pick = None;
            }
            if map_pick_button(ui, strings, form.map_pick == Some(ProvinceSlot::Target)) {
                form.map_pick = Some(ProvinceSlot::Target);
            }
        }
        _ => {}
    }
}

/// A filter box over a scrolling, selectable list. Returns the id the player
/// clicked this frame, if any.
///
/// The province and organisation lists were dropdowns that showed every
/// entry at once — workable with a handful, unusable with a map's worth. A
/// filter narrows them to what the player is looking for without leaving the
/// popup.
fn filtered_list<T: Copy>(
    ui: &mut egui::Ui,
    hint: &str,
    filter: &mut String,
    id_salt: &str,
    items: &[(T, String, Option<egui::Color32>)],
    selected: impl Fn(T) -> bool,
) -> Option<T> {
    ui.add(
        egui::TextEdit::singleline(filter)
            .hint_text(hint)
            .desired_width(f32::INFINITY),
    );
    let needle = filter.trim().to_lowercase();
    let mut picked = None;
    egui::ScrollArea::vertical()
        .max_height(160.0)
        .id_salt(id_salt)
        .show(ui, |ui| {
            for (id, name, colour) in items {
                if !needle.is_empty() && !name.to_lowercase().contains(&needle) {
                    continue;
                }
                let label = match colour {
                    Some(c) => egui::RichText::new(name).color(*c),
                    None => egui::RichText::new(name),
                };
                if ui.selectable_label(selected(*id), label).clicked() {
                    picked = Some(*id);
                }
            }
        });
    picked
}

/// The filtered province list, shared by the target and destination pickers.
fn province_picker(
    ui: &mut egui::Ui,
    strings: &TextDb,
    data: &PanelData,
    filter: &mut String,
    id_salt: &str,
    selected: impl Fn(ProvinceId) -> bool,
) -> Option<ProvinceId> {
    let mut items: Vec<(ProvinceId, String, Option<egui::Color32>)> = data
        .provinces
        .iter()
        .map(|(record, name, _)| (record.id, name.0.clone(), None))
        .collect();
    items.sort_by(|a, b| a.1.cmp(&b.1));
    filtered_list(
        ui,
        strings.text("ui.actions.filter-province"),
        filter,
        id_salt,
        &items,
        selected,
    )
}

/// The "pick on map" button. Returns true when pressed. It stays lit while a
/// pick is awaiting a click, so the interface's waiting on the player is
/// visible rather than a mode they have silently entered.
fn map_pick_button(ui: &mut egui::Ui, strings: &TextDb, awaiting: bool) -> bool {
    ui.horizontal(|ui| {
        let clicked = ui
            .selectable_label(awaiting, strings.text("ui.actions.pick-on-map"))
            .clicked();
        if awaiting {
            ui.weak(strings.text("ui.actions.pick-on-map.waiting"));
        }
        clicked
    })
    .inner
}

#[cfg(test)]
mod tests {
    use super::*;
    use aeon_sim::forces::{ArmyRecord, ShipLocation, ShipRecord};
    use aeon_sim::warfare::StandingOrders;
    use aeon_sim::{ArmyId, ShipId};

    fn army(general: u64) -> ArmyRecord {
        ArmyRecord {
            id: ArmyId::from_raw(10).unwrap(),
            name: "First Levy".to_owned(),
            owner: OrgId::from_raw(1).unwrap(),
            general: CharacterId::from_raw(general).unwrap(),
            manpower: 500,
            supplies: 100,
            location: ProvinceId::from_raw(3).unwrap(),
            standing_order: StandingOrders::default(),
        }
    }

    fn ship(captain: Option<u64>) -> ShipRecord {
        ShipRecord {
            id: ShipId::from_raw(20).unwrap(),
            key: aeon_data::ContentKey::new("lantern").unwrap(),
            name: "The Lantern".to_owned(),
            class: aeon_data::model::ShipClass::Capital,
            owner: OrgId::from_raw(1).unwrap(),
            captain: captain.map(|c| CharacterId::from_raw(c).unwrap()),
            location: ShipLocation::Docked(ProvinceId::from_raw(3).unwrap()),
            blockading: None,
            route: None,
        }
    }

    #[test]
    fn a_march_is_led_by_the_armys_general() {
        let army = army(42);
        let (leader, obstacle) = force_leader(Some(&army), None);
        assert_eq!(leader, Some(army.general), "and by nobody else");
        assert_eq!(obstacle, None);
    }

    #[test]
    fn a_ship_without_a_captain_has_no_order_to_give() {
        let ship = ship(None);
        let (leader, obstacle) = force_leader(None, Some(&ship));
        assert_eq!(leader, None, "nobody is silently substituted");
        assert_eq!(
            obstacle,
            Some(Obstacle::ShipHasNoCaptain),
            "the obstacle is stated where the choice is made"
        );
    }

    #[test]
    fn a_captained_ship_is_ordered_by_its_captain() {
        let ship = ship(Some(9));
        let (leader, obstacle) = force_leader(None, Some(&ship));
        assert_eq!(leader, ship.captain);
        assert_eq!(obstacle, None);
    }

    #[test]
    fn no_force_at_all_names_nobody_and_blames_nobody() {
        // Reaching here with neither is not a player-visible state, but it
        // must not invent a leader if it happens.
        let (leader, obstacle) = force_leader(None, None);
        assert_eq!(leader, None);
        assert_eq!(obstacle, None);
    }
}

/// What stops a force being ordered.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Obstacle {
    /// The ship has no captain, so there is nobody to give the order to.
    ShipHasNoCaptain,
}

impl Obstacle {
    /// The key of the sentence explaining this obstacle.
    pub fn text_key(self) -> &'static str {
        match self {
            Obstacle::ShipHasNoCaptain => "ui.actions.obstacle.ship-has-no-captain",
        }
    }
}

/// Who gives this force its orders, and what stops it.
///
/// An army is commanded by its general and a ship by its captain; neither
/// post is substitutable, and a vacant one is an obstacle rather than an
/// excuse to fall back on the house head. Kept as a function of its own
/// because it is a rule, and rules are worth testing.
pub fn force_leader(
    army: Option<&ArmyRecord>,
    ship: Option<&ShipRecord>,
) -> (Option<CharacterId>, Option<Obstacle>) {
    if let Some(army) = army {
        return (Some(army.general), None);
    }
    match ship {
        Some(ship) => match ship.captain {
            Some(captain) => (Some(captain), None),
            None => (None, Some(Obstacle::ShipHasNoCaptain)),
        },
        None => (None, None),
    }
}

/// Picks the province a force is being sent to.
pub fn pick_destination(
    ui: &mut egui::Ui,
    strings: &TextDb,
    data: &PanelData,
    form: &mut AssignmentForm,
) {
    let current = form.province;
    let picked = province_picker(
        ui,
        strings,
        data,
        &mut form.province_filter,
        "ctx-destination",
        |id| current == Some(id),
    );
    if let Some(id) = picked {
        form.province = Some(id);
        form.map_pick = None;
    }
    if map_pick_button(
        ui,
        strings,
        form.map_pick == Some(ProvinceSlot::Destination),
    ) {
        form.map_pick = Some(ProvinceSlot::Destination);
    }
}
