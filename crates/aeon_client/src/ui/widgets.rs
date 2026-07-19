//! Small drawing helpers shared by every panel.
//!
//! Nothing here knows what panel it is in. If a helper needs to know that,
//! it belongs in the panel instead.

use aeon_data::model::BodyKind;
use aeon_sim::economy::OrgResources;
use aeon_sim::{CharacterId, OrgId};
use bevy_egui::egui;

use crate::ui::lookup::Lookup;
use crate::view::Selection;

/// The player-facing name of a kind of celestial body.
pub fn kind_label(kind: BodyKind) -> &'static str {
    match kind {
        BodyKind::Planet => "Planet",
        BodyKind::Moon => "Moon",
        BodyKind::Starbase => "Starbase",
    }
}

/// Renders a link with a hover summary, returning whether it was clicked.
pub fn linked(ui: &mut egui::Ui, label: &str, summary: &str) -> bool {
    ui.link(label).on_hover_text(summary).clicked()
}

/// Renders the W/M/S/I resource readout, each value with its own tooltip.
pub fn resource_readout(ui: &mut egui::Ui, r: &OrgResources) {
    ui.label(format!("W {}", r.wealth))
        .on_hover_text("Wealth — funds jobs, personnel, and construction");
    ui.label(format!("M {}", r.manpower))
        .on_hover_text("Manpower — people to staff jobs, garrisons, and armies");
    ui.label(format!("S {}", r.supplies))
        .on_hover_text("Supplies — sustains armies, fleets, and expeditions");
    ui.label(format!("I {}/{}", r.influence, r.legitimacy))
        .on_hover_text(
            "Influence / Legitimacy — spendable political capital, capped and \
             recharged by your standing",
        );
}

/// Draws who the player is, and returns whatever they clicked on.
///
/// Kept at the head of the top bar so it is on screen in every view and
/// every selection: the one thing that should never need looking up is
/// whose house this is and who currently leads it. Both are links, so the
/// block doubles as the way back to yourself after wandering the map.
///
/// The liege line is not printed — it is already in the house's hover
/// summary, and a bar that is always visible should spend its width on
/// what changes rather than on what does not.
pub fn draw_identity(
    ui: &mut egui::Ui,
    lookup: &Lookup,
    org: Option<OrgId>,
    head: Option<CharacterId>,
) -> Option<Selection> {
    let Some(org) = org else {
        ui.weak("No house");
        return None;
    };
    let mut hit = None;

    // The house's own colour, the same one it is painted in on the map.
    let colour = lookup
        .orgs
        .get(&org)
        .and_then(|(record, _)| lookup.content.organisations.get(&record.key))
        .map(|def| egui::Color32::from_rgb(def.color.0, def.color.1, def.color.2))
        .unwrap_or(egui::Color32::GRAY);
    let (rect, _) = ui.allocate_exact_size(egui::vec2(12.0, 12.0), egui::Sense::hover());
    ui.painter().rect_filled(rect, 2.0, colour);

    if linked(ui, &lookup.org_name(org), &lookup.org_hover(org)) {
        hit = Some(Selection::Org(org));
    }
    match head.filter(|id| lookup.chars.contains_key(id)) {
        Some(id) => {
            if linked(ui, &lookup.char_name(id), &lookup.char_hover(id)) {
                hit = Some(Selection::Character(id));
            }
        }
        // A house between heads still says so, rather than showing a gap.
        None => {
            ui.weak("leaderless");
        }
    }
    hit
}
