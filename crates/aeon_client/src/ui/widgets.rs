//! Small drawing helpers shared by every panel.
//!
//! Nothing here knows what panel it is in. If a helper needs to know that,
//! it belongs in the panel instead.

use aeon_data::model::BodyKind;
use aeon_sim::economy::OrgResources;
use aeon_sim::{CharacterId, OrgId, TextDb};
use bevy_egui::egui;

use crate::ui::lookup::Lookup;
use crate::ui::theme::UiTheme;
use crate::view::Selection;

/// The player-facing name of a kind of celestial body.
pub fn kind_label_key(kind: BodyKind) -> &'static str {
    match kind {
        BodyKind::Planet => "ui.body-kind.planet",
        BodyKind::Moon => "ui.body-kind.moon",
        BodyKind::Starbase => "ui.body-kind.starbase",
    }
}

/// Renders a link with a hover summary, returning whether it was clicked.
///
/// Takes `impl Into<WidgetText>` rather than `&str` so a caller can hand it
/// a coloured [`egui::RichText`] — a house link carries its own colour.
pub fn linked(ui: &mut egui::Ui, label: impl Into<egui::WidgetText>, summary: &str) -> bool {
    ui.link(label).on_hover_text(summary).clicked()
}

/// Renders the W/M/S/I resource readout, each value with its own tooltip.
pub fn resource_readout(ui: &mut egui::Ui, strings: &TextDb, r: &OrgResources) {
    ui.label(strings.format("ui.cost.wealth", &[("amount", &r.wealth.to_string())]))
        .on_hover_text(strings.text("ui.resource.wealth.hover"));
    ui.label(strings.format("ui.cost.manpower", &[("amount", &r.manpower.to_string())]))
        .on_hover_text(strings.text("ui.resource.manpower.hover"));
    ui.label(strings.format("ui.cost.supplies", &[("amount", &r.supplies.to_string())]))
        .on_hover_text(strings.text("ui.resource.supplies.hover"));
    ui.label(strings.format(
        "ui.resource.influence",
        &[
            ("influence", &r.influence.to_string()),
            ("legitimacy", &r.legitimacy.to_string()),
        ],
    ))
    .on_hover_text(
        "Influence / Legitimacy — spendable political capital, capped and \
             recharged by your standing",
    );
}

/// Draws a small filled square in `colour`.
///
/// Both the legend and the identity block need one, and they were drawing
/// it to their own hardcoded measurements — so a designer changing swatch
/// size changed one of them and not the other.
pub fn swatch(ui: &mut egui::Ui, theme: &UiTheme, colour: egui::Color32) {
    let side = f32::from(theme.components.swatch_size);
    let (rect, _) = ui.allocate_exact_size(egui::vec2(side, side), egui::Sense::hover());
    ui.painter()
        .rect_filled(rect, theme.components.swatch_radius as f32, colour);
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
    theme: &UiTheme,
    lookup: &Lookup,
    org: Option<OrgId>,
    head: Option<CharacterId>,
) -> Option<Selection> {
    let strings = lookup.strings;
    let Some(org) = org else {
        ui.weak(strings.text("ui.identity.no-house"));
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
    swatch(ui, theme, colour);

    if linked(ui, lookup.org_rich(org), &lookup.org_hover(org)) {
        hit = Some(Selection::Org(org));
    }
    match head.filter(|id| lookup.chars.contains_key(id)) {
        Some(id) => {
            if linked(ui, lookup.char_name(id), &lookup.char_hover(id)) {
                hit = Some(Selection::Character(id));
            }
        }
        // A house between heads still says so, rather than showing a gap.
        None => {
            ui.weak(strings.text("ui.identity.leaderless"));
        }
    }
    hit
}
