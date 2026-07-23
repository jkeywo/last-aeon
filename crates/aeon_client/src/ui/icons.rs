//! Icons, drawn as painter primitives rather than loaded as assets.
//!
//! The client ships no image assets, and bevy_egui needs every texture
//! registered before it can be drawn — so an icon set made of files would
//! cost a loading path, a registration step, and a wasm packaging concern,
//! all before the first triangle. Glyphs from the font were the other
//! option and are worse: coverage varies by platform, and a missing glyph
//! is a silent box rather than a build error.
//!
//! Shapes drawn from primitives have none of those problems. They are
//! resolution-independent, tinted by the theme like everything else, and
//! identical on native and web.
//!
//! Each icon is drawn inside a unit square and scaled to whatever rect it
//! is given, so the same shape serves a toolbar button and a legend swatch.

use aeon_sim::TextDb;
use bevy_egui::egui;

use crate::ui::dock::PanelKind;
use crate::ui::theme::UiTheme;
use crate::view::MapMode;

/// Draws the row of map-mode buttons, and returns the mode picked this
/// frame if one was.
///
/// A row of icons rather than a control that cycles: cycling makes the
/// eighth mode seven clicks from the first, gives no sense of what is
/// available, and cannot show which mode you are in without reading text.
/// Every mode is one click from every other here, and each says what
/// question it answers on hover.
pub fn draw_mode_bar(
    ui: &mut egui::Ui,
    theme: &UiTheme,
    strings: &TextDb,
    active: MapMode,
) -> Option<MapMode> {
    let mut picked = None;
    let button = f32::from(theme.components.icon_button);
    for mode in MapMode::ALL {
        let (rect, response) =
            ui.allocate_exact_size(egui::vec2(button, button), egui::Sense::click());
        let selected = *mode == active;

        // The button's own surface, taken from the same widget states the
        // rest of the interface uses, so an icon button and a text button
        // respond identically.
        let visuals = ui.style().interact_selectable(&response, selected);
        if selected || response.hovered() {
            ui.painter()
                .rect_filled(rect, theme.shape.radius_small as f32, visuals.bg_fill);
        }
        draw_mode_icon(ui.painter(), theme, rect, *mode, visuals.fg_stroke.color);

        if response
            .on_hover_text(format!(
                "{}\n{}",
                strings.text(&mode.label_key()),
                strings.text(&mode.description_key())
            ))
            .clicked()
        {
            picked = Some(*mode);
        }
    }
    picked
}

/// Draws `mode`'s icon inside `rect`, in `colour`.
///
/// The shapes are chosen to differ in *silhouette*, not only in detail: at
/// toolbar size a player picks them out by outline long before they can
/// resolve what is inside them.
pub fn draw_mode_icon(
    painter: &egui::Painter,
    theme: &UiTheme,
    rect: egui::Rect,
    mode: MapMode,
    colour: egui::Color32,
) {
    // Inset, so no icon touches its button's edge.
    let r = rect.shrink(rect.width() * theme.components.icon_inset as f32 / 100.0);
    let at = |x: f32, y: f32| egui::pos2(r.left() + r.width() * x, r.top() + r.height() * y);
    let width = (r.width() * theme.components.icon_stroke as f32 / 100.0).max(1.0);
    let stroke = egui::Stroke::new(width, colour);
    let line = |a: egui::Pos2, b: egui::Pos2| painter.line_segment([a, b], stroke);

    match mode {
        // One holding: a single filled block.
        MapMode::Holder => {
            painter.rect_filled(
                egui::Rect::from_min_max(at(0.1, 0.1), at(0.9, 0.9)),
                r.width() * 0.1,
                colour,
            );
        }
        // A holding inside a greater one: the liege chain, one level up.
        MapMode::GreatHouse => {
            painter.rect_stroke(
                egui::Rect::from_min_max(at(0.05, 0.05), at(0.95, 0.95)),
                r.width() * 0.1,
                stroke,
                egui::StrokeKind::Inside,
            );
            painter.rect_filled(
                egui::Rect::from_min_max(at(0.32, 0.32), at(0.68, 0.68)),
                r.width() * 0.06,
                colour,
            );
        }
        // You at the centre, with what answers to you around you.
        MapMode::MyControl => {
            painter.circle_filled(r.center(), r.width() * 0.18, colour);
            painter.circle_stroke(r.center(), r.width() * 0.42, stroke);
        }
        // A balance: order is a thing that tips.
        MapMode::Order => {
            line(at(0.1, 0.35), at(0.9, 0.35));
            line(at(0.5, 0.35), at(0.5, 0.8));
            line(at(0.28, 0.8), at(0.72, 0.8));
        }
        // A coin.
        MapMode::Wealth => {
            painter.circle_stroke(r.center(), r.width() * 0.4, stroke);
            painter.circle_filled(r.center(), r.width() * 0.14, colour);
        }
        // Crossed blades.
        MapMode::Military => {
            line(at(0.15, 0.15), at(0.85, 0.85));
            line(at(0.85, 0.15), at(0.15, 0.85));
        }
        // Two parties, and the line between them.
        MapMode::PlayerRelations => {
            painter.circle_filled(at(0.22, 0.5), r.width() * 0.16, colour);
            painter.circle_filled(at(0.78, 0.5), r.width() * 0.16, colour);
            line(at(0.38, 0.5), at(0.62, 0.5));
        }
        // A claim staked: a pennant on a pole.
        MapMode::ClaimPressure => {
            line(at(0.3, 0.1), at(0.3, 0.9));
            painter.add(egui::Shape::convex_polygon(
                vec![at(0.3, 0.12), at(0.85, 0.32), at(0.3, 0.52)],
                colour,
                egui::Stroke::NONE,
            ));
        }
    }
}

/// Draws `kind`'s icon inside `rect`, in `colour`.
///
/// As with the map modes, these differ by silhouette: a magnifier, a
/// stack of rows, lines of text, a clock, and a column of swatches.
pub fn draw_panel_icon(
    painter: &egui::Painter,
    theme: &UiTheme,
    rect: egui::Rect,
    kind: PanelKind,
    colour: egui::Color32,
) {
    let r = rect.shrink(rect.width() * theme.components.icon_inset as f32 / 100.0);
    let at = |x: f32, y: f32| egui::pos2(r.left() + r.width() * x, r.top() + r.height() * y);
    let width = (r.width() * theme.components.icon_stroke as f32 / 100.0).max(1.0);
    let stroke = egui::Stroke::new(width, colour);
    let line = |a: egui::Pos2, b: egui::Pos2| painter.line_segment([a, b], stroke);

    match kind {
        // A magnifier: looking closely at one thing.
        PanelKind::Inspector => {
            painter.circle_stroke(at(0.42, 0.42), r.width() * 0.3, stroke);
            line(at(0.64, 0.64), at(0.9, 0.9));
        }
        // Stacked rows: many things, listed.
        PanelKind::Listing => {
            for y in [0.2, 0.5, 0.8] {
                line(at(0.1, y), at(0.9, y));
            }
        }
        // Lines of text, ragged like a log.
        PanelKind::Log => {
            line(at(0.1, 0.22), at(0.9, 0.22));
            line(at(0.1, 0.5), at(0.7, 0.5));
            line(at(0.1, 0.78), at(0.82, 0.78));
        }
        // A clock: work with time left to run.
        PanelKind::Assignments => {
            painter.circle_stroke(r.center(), r.width() * 0.4, stroke);
            line(r.center(), at(0.5, 0.2));
            line(r.center(), at(0.75, 0.6));
        }
        // A row of paint chips: the tokens themselves.
        #[cfg(not(target_arch = "wasm32"))]
        PanelKind::Specimen => {
            for x in [0.12, 0.42, 0.72] {
                painter.rect_filled(
                    egui::Rect::from_min_max(at(x, 0.15), at(x + 0.18, 0.85)),
                    1.0,
                    colour,
                );
            }
        }
        // A figure: a person waiting to be put to work.
        PanelKind::Idle => {
            painter.circle_stroke(at(0.5, 0.3), r.width() * 0.16, stroke);
            line(at(0.24, 0.85), at(0.5, 0.52));
            line(at(0.76, 0.85), at(0.5, 0.52));
        }
        // A column of swatches: the colour key.
        PanelKind::Ledger => {
            for y in [0.15, 0.45, 0.75] {
                painter.rect_filled(
                    egui::Rect::from_min_max(at(0.12, y), at(0.36, y + 0.16)),
                    1.0,
                    colour,
                );
                line(at(0.48, y + 0.08), at(0.88, y + 0.08));
            }
        }
    }
}
