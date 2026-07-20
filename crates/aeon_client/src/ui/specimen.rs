//! Every design token, drawn in context.
//!
//! The other half of the design loop: with hot-reload watching the file,
//! this panel is where a change to `theme.ron` becomes visible without
//! hunting through the game for a widget in the right state.
//!
//! Native only, like the reload it pairs with. It is a tool for authoring
//! the theme, not part of the game, and the web build is what players get.
//!
//! **Everything here is labelled with its token path** — `palette.hovered`,
//! `shape.stroke_hover` — rather than with prose. That is deliberate twice
//! over: it tells whoever is looking exactly which line to edit, and it
//! keeps the panel out of the translated string table, where a developer
//! tool's headings would be waste for a translator to carry.

use bevy_egui::egui;

use crate::ui::theme::{TargetState, UiTheme};
use crate::ui::widgets::swatch;

/// Draws the specimen: every token, under the name you would set it by.
pub fn draw_specimen_panel(ui: &mut egui::Ui, theme: &UiTheme) {
    egui::ScrollArea::vertical()
        .id_salt("specimen-scroll")
        .show(ui, |ui| {
            widget_states(ui);
            ui.add_space(8.0);
            text_roles(ui, theme);
            ui.add_space(8.0);
            colour_group(ui, theme, "semantics — target", &target_colours(theme));
            colour_group(ui, theme, "semantics — outcome", &outcome_colours(theme));
            colour_group(ui, theme, "semantics — urgency", &urgency_colours(theme));
            colour_group(ui, theme, "semantics — resource", &resource_colours(theme));
            colour_group(ui, theme, "semantics — map", &map_colours(theme));
            colour_group(ui, theme, "palette", &palette_colours(theme));
            ui.add_space(8.0);
            measures(ui, theme);
        });
}

/// Every state a control can be in, as a control actually in it.
///
/// Hover and press these rather than reading their values: expansion and
/// stroke weight are things you can only judge in motion.
fn widget_states(ui: &mut egui::Ui) {
    ui.strong("widget states");
    ui.weak("hover and press these — expansion and stroke only read in motion");
    ui.horizontal_wrapped(|ui| {
        let _ = ui.button("inactive → hovered → active");
        let _ = ui.button("another");
        ui.add_enabled(false, egui::Button::new("disabled"));
    });
    ui.horizontal_wrapped(|ui| {
        let mut on = true;
        ui.toggle_value(&mut on, "open");
        let mut off = false;
        ui.toggle_value(&mut off, "not open");
        let mut checked = true;
        ui.checkbox(&mut checked, "spacing.icon_width");
    });
    ui.horizontal_wrapped(|ui| {
        let mut value = 0.5f32;
        // Labelled beside rather than through `Slider::text`, whose shape
        // is indistinguishable from a string-table lookup to the scanner
        // that checks every key resolves.
        ui.add(egui::Slider::new(&mut value, 0.0..=1.0));
        ui.label("spacing.slider_width");
    });
    ui.horizontal_wrapped(|ui| {
        let mut text = String::from("spacing.text_edit_width");
        ui.text_edit_singleline(&mut text);
    });
    ui.horizontal_wrapped(|ui| {
        ui.label("palette.text");
        ui.strong("palette.text_strong");
        ui.weak("palette.text_weak");
        ui.hyperlink_to("palette.link", "https://example.invalid");
        ui.monospace("typography.monospace");
    });
    ui.horizontal_wrapped(|ui| {
        ui.colored_label(ui.visuals().error_fg_color, "palette.error");
        ui.colored_label(ui.visuals().warn_fg_color, "palette.warn");
    });
    ui.collapsing("chrome.collapsing_header_frame", |ui| {
        ui.label("chrome.indent_left_vline");
    });
}

/// The type scale, each role at its own size.
fn text_roles(ui: &mut egui::Ui, theme: &UiTheme) {
    ui.strong("typography");
    let t = &theme.typography;
    for (name, size, style) in [
        ("heading", t.heading, egui::TextStyle::Heading),
        ("body", t.body, egui::TextStyle::Body),
        ("button", t.button, egui::TextStyle::Button),
        ("small", t.small, egui::TextStyle::Small),
        ("monospace", t.monospace, egui::TextStyle::Monospace),
    ] {
        ui.label(egui::RichText::new(format!("typography.{name} — {size}px")).text_style(style));
    }
    ui.label(
        egui::RichText::new(format!(
            "typography.map_label {} / map_label_selected {} — drawn on the globe",
            t.map_label, t.map_label_selected
        ))
        .text_style(egui::TextStyle::Small),
    );
}

/// A row of swatches under one heading.
///
/// Drawn with the same `swatch` widget the legend and identity block use,
/// so its own tokens are on show here alongside the colours.
fn colour_group(
    ui: &mut egui::Ui,
    theme: &UiTheme,
    heading: &str,
    entries: &[(String, egui::Color32)],
) {
    ui.strong(heading);
    for (name, colour) in entries {
        ui.horizontal(|ui| {
            swatch(ui, theme, *colour);
            ui.label(name);
        });
    }
}

fn target_colours(theme: &UiTheme) -> Vec<(String, egui::Color32)> {
    [
        ("valid", TargetState::Valid),
        ("already_doing", TargetState::AlreadyDoing),
        ("ineligible_fixable", TargetState::IneligibleFixable),
        ("ineligible_structural", TargetState::StructurallyIneligible),
        ("not_interactable", TargetState::NotInteractable),
    ]
    .into_iter()
    .map(|(name, state)| (format!("semantics.{name}"), theme.semantics.target(state)))
    .collect()
}

fn outcome_colours(theme: &UiTheme) -> Vec<(String, egui::Color32)> {
    aeon_data::model::JobResultKind::ALL
        .iter()
        .map(|kind| {
            (
                format!("semantics.{}", format!("{kind:?}").to_lowercase()),
                theme.semantics.outcome(*kind),
            )
        })
        .collect()
}

fn urgency_colours(theme: &UiTheme) -> Vec<(String, egui::Color32)> {
    let s = &theme.semantics;
    vec![
        ("semantics.urgent".to_owned(), s.urgent.into()),
        ("semantics.notable".to_owned(), s.notable.into()),
        ("semantics.calm".to_owned(), s.calm.into()),
    ]
}

fn resource_colours(theme: &UiTheme) -> Vec<(String, egui::Color32)> {
    let s = &theme.semantics;
    vec![
        ("semantics.wealth".to_owned(), s.wealth.into()),
        ("semantics.manpower".to_owned(), s.manpower.into()),
        ("semantics.supplies".to_owned(), s.supplies.into()),
        ("semantics.influence".to_owned(), s.influence.into()),
    ]
}

fn map_colours(theme: &UiTheme) -> Vec<(String, egui::Color32)> {
    let s = &theme.semantics;
    vec![
        ("semantics.map_neutral".to_owned(), s.map_neutral.into()),
        ("semantics.map_border".to_owned(), s.map_border.into()),
        ("semantics.map_label".to_owned(), s.map_label.into()),
        (
            "semantics.map_label_selected".to_owned(),
            s.map_label_selected.into(),
        ),
        (
            "semantics.map_label_alert".to_owned(),
            s.map_label_alert.into(),
        ),
    ]
}

fn palette_colours(theme: &UiTheme) -> Vec<(String, egui::Color32)> {
    let p = &theme.palette;
    vec![
        ("palette.window".to_owned(), p.window.into()),
        ("palette.panel".to_owned(), p.panel.into()),
        ("palette.panel_alt".to_owned(), p.panel_alt.into()),
        ("palette.popup".to_owned(), p.popup.into()),
        ("palette.inactive".to_owned(), p.inactive.into()),
        ("palette.hovered".to_owned(), p.hovered.into()),
        ("palette.active".to_owned(), p.active.into()),
        ("palette.open".to_owned(), p.open.into()),
        ("palette.inactive_weak".to_owned(), p.inactive_weak.into()),
        ("palette.hovered_weak".to_owned(), p.hovered_weak.into()),
        ("palette.active_weak".to_owned(), p.active_weak.into()),
        ("palette.open_weak".to_owned(), p.open_weak.into()),
        ("palette.border".to_owned(), p.border.into()),
        ("palette.border_strong".to_owned(), p.border_strong.into()),
        ("palette.selection".to_owned(), p.selection.into()),
        ("palette.code_bg".to_owned(), p.code_bg.into()),
    ]
}

/// The numbers that have no colour: spacing, shape, interaction, chrome.
fn measures(ui: &mut egui::Ui, theme: &UiTheme) {
    let s = &theme.spacing;
    let shape = &theme.shape;
    let i = &theme.interaction;
    let c = &theme.chrome;
    let comp = &theme.components;

    ui.strong("shape");
    for (name, value) in [
        ("radius_small", shape.radius_small),
        ("radius_large", shape.radius_large),
        ("radius_menu", shape.radius_menu),
    ] {
        ui.horizontal(|ui| {
            let (rect, _) = ui.allocate_exact_size(egui::vec2(28.0, 14.0), egui::Sense::hover());
            ui.painter()
                .rect_filled(rect, value as f32, ui.visuals().widgets.inactive.bg_fill);
            ui.label(format!("shape.{name} — {value}"));
        });
    }
    for (name, value) in [
        ("stroke_rest", shape.stroke_rest),
        ("stroke_hover", shape.stroke_hover),
        ("stroke_active", shape.stroke_active),
        ("stroke_open", shape.stroke_open),
        ("stroke_noninteractive", shape.stroke_noninteractive),
        ("text_stroke", shape.text_stroke),
        ("window_stroke", shape.window_stroke),
    ] {
        ui.label(format!("shape.{name} — {value}"));
    }
    ui.label(format!(
        "shape.window_shadow — offset {},{} blur {} spread {}",
        shape.window_shadow.offset_x,
        shape.window_shadow.offset_y,
        shape.window_shadow.blur,
        shape.window_shadow.spread
    ));
    ui.label(format!(
        "shape.popup_shadow — offset {},{} blur {} spread {}",
        shape.popup_shadow.offset_x,
        shape.popup_shadow.offset_y,
        shape.popup_shadow.blur,
        shape.popup_shadow.spread
    ));

    ui.add_space(6.0);
    ui.strong("interaction");
    ui.label(format!(
        "interaction.expansion_hover — {}",
        i.expansion_hover
    ));
    ui.label(format!(
        "interaction.expansion_active — {}",
        i.expansion_active
    ));
    ui.label(format!("interaction.animation_time — {}", i.animation_time));
    ui.label(format!("interaction.disabled_alpha — {}", i.disabled_alpha));
    ui.label(format!(
        "interaction.weak_text_alpha — {}",
        i.weak_text_alpha
    ));

    ui.add_space(6.0);
    ui.strong("chrome");
    for (name, value) in [
        ("button_frame", c.button_frame),
        ("collapsing_header_frame", c.collapsing_header_frame),
        ("striped", c.striped),
        ("indent_left_vline", c.indent_left_vline),
        ("indent_ends_with_line", c.indent_ends_with_line),
        ("slider_trailing_fill", c.slider_trailing_fill),
        ("scroll_floating", c.scroll_floating),
        ("window_highlight_topmost", c.window_highlight_topmost),
    ] {
        ui.label(format!("chrome.{name} — {value}"));
    }

    ui.add_space(6.0);
    ui.strong("spacing");
    // A ruler: each gap drawn at the width it actually is.
    for (name, value) in [
        ("item_gap_x", s.item_gap_x),
        ("item_gap_y", s.item_gap_y),
        ("panel_margin", s.panel_margin),
        ("menu_margin", s.menu_margin),
        ("indent", s.indent),
        ("button_pad_x", s.button_pad_x),
        ("button_pad_y", s.button_pad_y),
        ("row_height", s.row_height),
        ("row_min_width", s.row_min_width),
        ("icon_width", s.icon_width),
        ("icon_inner_width", s.icon_inner_width),
        ("icon_gap", s.icon_gap),
        ("scroll_bar_width", s.scroll_bar_width),
        ("scroll_handle_min", s.scroll_handle_min),
        ("clip_margin", s.clip_margin),
        ("resize_corner", s.resize_corner),
    ] {
        ui.horizontal(|ui| {
            let (rect, _) = ui.allocate_exact_size(
                egui::vec2(f32::from(value).max(1.0), 8.0),
                egui::Sense::hover(),
            );
            ui.painter()
                .rect_filled(rect, 0.0, ui.visuals().widgets.active.bg_fill);
            ui.label(format!("spacing.{name} — {value}"));
        });
    }

    ui.add_space(6.0);
    ui.strong("components");
    ui.label(format!("components.swatch_size — {}", comp.swatch_size));
    ui.label(format!("components.icon_button — {}", comp.icon_button));
    ui.label(format!("components.picker_width — {}", comp.picker_width));
    ui.label(format!("components.search_width — {}", comp.search_width));
    ui.label(format!(
        "components.log_max_entries — {}",
        comp.log_max_entries
    ));
    ui.horizontal(|ui| {
        swatch(ui, theme, ui.visuals().widgets.active.bg_fill);
        ui.label("components.swatch_size / swatch_radius");
    });
}
