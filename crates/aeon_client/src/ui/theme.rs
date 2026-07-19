//! The interface's design tokens, and how they reach egui.
//!
//! Everything about how the interface *looks* lives in one serialisable
//! token set rather than being scattered through the panels as literals.
//! That makes the appearance something a designer can specify — a palette,
//! a type scale, a spacing grid — and the code's job merely to apply it.
//!
//! The tokens are deliberately shaped like egui rather than like CSS.
//! egui's style is keyed by widget *state* (inactive, hovered, active,
//! open) rather than cascading, so every interactive colour is given for
//! every state, and there is exactly one popup shadow because egui draws
//! exactly one.
//!
//! [`Semantics`] is the part that earns its keep: it carries a fixed
//! vocabulary for what a thing *means* — a valid target, something already
//! under way, a refusal that can be fixed, a refusal that cannot — so the
//! same idea is never given two different colours in two different panels.

use bevy::prelude::Resource;
use bevy_egui::egui;
use serde::{Deserialize, Serialize};

/// The theme shipped with the build.
const EMBEDDED_THEME: &str = include_str!("../../assets/theme.ron");

/// An sRGB colour, authored as `(r, g, b)` or `(r, g, b, a)`.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rgba {
    /// Red.
    pub r: u8,
    /// Green.
    pub g: u8,
    /// Blue.
    pub b: u8,
    /// Alpha; 255 is opaque.
    #[serde(default = "opaque")]
    pub a: u8,
}

fn opaque() -> u8 {
    255
}

impl From<Rgba> for egui::Color32 {
    fn from(value: Rgba) -> Self {
        egui::Color32::from_rgba_unmultiplied(value.r, value.g, value.b, value.a)
    }
}

/// Surfaces and text, given per widget state because egui asks for them
/// that way.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Palette {
    /// The window behind everything.
    pub window: Rgba,
    /// A panel's own ground.
    pub panel: Rgba,
    /// A second panel tone, for headers and banding.
    pub panel_alt: Rgba,
    /// Popups and tooltips.
    pub popup: Rgba,
    /// A control at rest.
    pub inactive: Rgba,
    /// A control under the pointer.
    pub hovered: Rgba,
    /// A control being pressed.
    pub active: Rgba,
    /// A control holding something open.
    pub open: Rgba,
    /// A control that cannot be used.
    pub disabled: Rgba,
    /// Ordinary text.
    pub text: Rgba,
    /// Text of lesser importance.
    pub text_weak: Rgba,
    /// Headings.
    pub heading: Rgba,
    /// Something that can be followed.
    pub link: Rgba,
    /// A quiet dividing line.
    pub border: Rgba,
    /// A line that should be noticed.
    pub border_strong: Rgba,
    /// The selection.
    pub selection: Rgba,
}

/// Text sizes by role. One family, so hierarchy comes from size and
/// colour rather than weight.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Typography {
    /// Panel headings.
    pub heading: u8,
    /// Ordinary text.
    pub body: u8,
    /// Secondary and annotation text.
    pub small: u8,
    /// Figures that should line up.
    pub monospace: u8,
}

/// The spacing grid.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Spacing {
    /// Between items in a row.
    pub item_gap_x: u8,
    /// Between stacked items.
    pub item_gap_y: u8,
    /// Inside a panel's edge.
    pub panel_margin: u8,
    /// One step of indentation.
    pub indent: u8,
    /// Inside a button.
    pub button_padding: u8,
    /// A standard row.
    pub row_height: u8,
}

/// Corners and lines.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Shape {
    /// Corner radius on small controls.
    pub radius_small: u8,
    /// Corner radius on panels and popups.
    pub radius_large: u8,
    /// A thin line.
    pub stroke_hairline: u8,
    /// A line meant to be seen.
    pub stroke_normal: u8,
}

/// What a possible target of an interaction currently *is*.
///
/// One vocabulary across every interaction in the game, so the meaning is
/// learned once. The distinction between the two refusals matters: one is
/// a state the player can change, the other is not.
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum TargetState {
    /// A legal target for what is being offered.
    Valid,
    /// Already doing this, here.
    AlreadyDoing,
    /// Refused for now, but the player could change that.
    IneligibleFixable,
    /// Refused by something the player cannot change.
    StructurallyIneligible,
    /// Not a participant in this interaction at all.
    NotInteractable,
}

/// Colours that carry meaning rather than decoration.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Semantics {
    /// A legal target.
    pub valid: Rgba,
    /// Already under way here.
    pub already_doing: Rgba,
    /// Refused, but fixable.
    pub ineligible_fixable: Rgba,
    /// Refused for good.
    pub ineligible_structural: Rgba,
    /// Not part of this interaction.
    pub not_interactable: Rgba,

    /// A job's four graded outcomes.
    pub critical_success: Rgba,
    /// A plain success.
    pub success: Rgba,
    /// A failure.
    pub failure: Rgba,
    /// A disaster.
    pub disaster: Rgba,

    /// Something needing attention now.
    pub urgent: Rgba,
    /// Something worth noting.
    pub notable: Rgba,
    /// Nothing wrong.
    pub calm: Rgba,

    /// Wealth.
    pub wealth: Rgba,
    /// Manpower.
    pub manpower: Rgba,
    /// Supplies.
    pub supplies: Rgba,
    /// Influence and legitimacy.
    pub influence: Rgba,

    /// A province nobody holds.
    pub map_neutral: Rgba,
    /// The line between provinces on the baked map.
    pub map_border: Rgba,
    /// A province label.
    pub map_label: Rgba,
    /// The shadow that keeps a label readable over any colour.
    pub map_label_shadow: Rgba,
    /// The label of the selected province.
    pub map_label_selected: Rgba,
    /// The label of a province wanting attention.
    pub map_label_alert: Rgba,
}

impl Semantics {
    /// The colour for what a target currently is.
    pub fn target(&self, state: TargetState) -> egui::Color32 {
        match state {
            TargetState::Valid => self.valid,
            TargetState::AlreadyDoing => self.already_doing,
            TargetState::IneligibleFixable => self.ineligible_fixable,
            TargetState::StructurallyIneligible => self.ineligible_structural,
            TargetState::NotInteractable => self.not_interactable,
        }
        .into()
    }

    /// The colour for one of a job's graded outcomes.
    pub fn outcome(&self, kind: aeon_data::model::JobResultKind) -> egui::Color32 {
        use aeon_data::model::JobResultKind as K;
        match kind {
            K::CriticalSuccess => self.critical_success,
            K::Success => self.success,
            K::Failure => self.failure,
            K::Disaster => self.disaster,
        }
        .into()
    }
}

/// The whole appearance of the interface.
#[derive(Resource, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct UiTheme {
    /// Surfaces and text.
    pub palette: Palette,
    /// Text sizes.
    pub typography: Typography,
    /// The spacing grid.
    pub spacing: Spacing,
    /// Corners and lines.
    pub shape: Shape,
    /// Colours that mean something.
    pub semantics: Semantics,
}

impl Default for UiTheme {
    fn default() -> Self {
        Self::embedded()
    }
}

impl UiTheme {
    /// The theme shipped with this build.
    ///
    /// # Panics
    /// Panics if the embedded theme does not parse. It is checked by a
    /// test and by CI, so this only fires on a broken local edit — and
    /// should fire loudly rather than silently falling back.
    pub fn embedded() -> Self {
        ron::from_str(EMBEDDED_THEME).expect("the embedded theme parses")
    }

    /// Flattens the tokens onto an egui style.
    ///
    /// Every widget state is written, because egui resolves appearance by
    /// state rather than by cascade: anything left unset would keep
    /// egui's own default and quietly break the palette.
    pub fn apply(&self, style: &mut egui::Style) {
        let p = &self.palette;
        let radius_small = egui::CornerRadius::same(self.shape.radius_small);
        let radius_large = egui::CornerRadius::same(self.shape.radius_large);
        let hairline = f32::from(self.shape.stroke_hairline);
        let normal = f32::from(self.shape.stroke_normal);

        style.visuals.dark_mode = true;
        style.visuals.panel_fill = p.panel.into();
        style.visuals.window_fill = p.popup.into();
        style.visuals.extreme_bg_color = p.window.into();
        style.visuals.faint_bg_color = p.panel_alt.into();
        style.visuals.window_corner_radius = radius_large;
        style.visuals.window_stroke = egui::Stroke::new(hairline, p.border);
        style.visuals.selection.bg_fill = p.selection.into();
        style.visuals.selection.stroke = egui::Stroke::new(hairline, p.text);
        style.visuals.hyperlink_color = p.link.into();

        let widget = |fill: Rgba, stroke: Rgba, text: Rgba| egui::style::WidgetVisuals {
            bg_fill: fill.into(),
            weak_bg_fill: fill.into(),
            bg_stroke: egui::Stroke::new(hairline, stroke),
            fg_stroke: egui::Stroke::new(normal, text),
            corner_radius: radius_small,
            expansion: 0.0,
        };
        // Non-interactive surfaces carry no outline of their own.
        style.visuals.widgets.noninteractive = widget(p.panel, p.border, p.text);
        style.visuals.widgets.inactive = widget(p.inactive, p.border, p.text);
        style.visuals.widgets.hovered = widget(p.hovered, p.border_strong, p.text);
        style.visuals.widgets.active = widget(p.active, p.border_strong, p.text);
        style.visuals.widgets.open = widget(p.open, p.border_strong, p.text);
        // Disabled controls read as absent rather than as another state.
        style.visuals.widgets.noninteractive.fg_stroke = egui::Stroke::new(normal, p.text_weak);

        style.spacing.item_spacing = egui::vec2(
            f32::from(self.spacing.item_gap_x),
            f32::from(self.spacing.item_gap_y),
        );
        style.spacing.window_margin = egui::Margin::same(self.spacing.panel_margin as i8);
        style.spacing.button_padding = egui::vec2(
            f32::from(self.spacing.button_padding),
            f32::from(self.spacing.button_padding) / 2.0,
        );
        style.spacing.indent = f32::from(self.spacing.indent);
        style.spacing.interact_size.y = f32::from(self.spacing.row_height);

        use egui::FontFamily::{Monospace, Proportional};
        use egui::TextStyle as T;
        style.text_styles = [
            (
                T::Heading,
                egui::FontId::new(f32::from(self.typography.heading), Proportional),
            ),
            (
                T::Body,
                egui::FontId::new(f32::from(self.typography.body), Proportional),
            ),
            (
                T::Button,
                egui::FontId::new(f32::from(self.typography.body), Proportional),
            ),
            (
                T::Small,
                egui::FontId::new(f32::from(self.typography.small), Proportional),
            ),
            (
                T::Monospace,
                egui::FontId::new(f32::from(self.typography.monospace), Monospace),
            ),
        ]
        .into();
    }
}

/// Applies the theme once at startup, and again whenever it changes.
pub fn apply_theme(
    mut contexts: bevy_egui::EguiContexts,
    theme: bevy::prelude::Res<UiTheme>,
    mut applied: bevy::prelude::Local<bool>,
) {
    use bevy::prelude::DetectChanges;
    if *applied && !theme.is_changed() {
        return;
    }
    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };
    // Written into every theme egui keeps, so the appearance does not
    // depend on which one the context happens to be using.
    ctx.all_styles_mut(|style| theme.apply(style));
    *applied = true;
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn the_shipped_theme_parses() {
        // `embedded()` panics on a broken file; this is the test that turns
        // that into a build failure rather than a runtime one.
        let theme = UiTheme::embedded();
        assert!(theme.typography.body > 0, "body text has a size");
        assert!(theme.spacing.row_height > 0, "rows have a height");
    }

    #[test]
    fn a_theme_survives_a_round_trip() {
        // A designer's token file has to come back out of the loader exactly
        // as it went in, or successive edits will drift.
        let theme = UiTheme::embedded();
        let text = ron::ser::to_string_pretty(&theme, ron::ser::PrettyConfig::default())
            .expect("a theme serialises");
        let parsed: UiTheme = ron::from_str(&text).expect("and parses back");
        assert_eq!(theme, parsed);
    }

    #[test]
    fn every_target_state_has_its_own_colour() {
        // The five states are the interface's whole vocabulary for "can I do
        // this here". If a designer collapses two of them into one colour the
        // distinction silently stops being visible — most damagingly the one
        // between a refusal the player can fix and one they cannot.
        let semantics = UiTheme::embedded().semantics;
        let states = [
            TargetState::Valid,
            TargetState::AlreadyDoing,
            TargetState::IneligibleFixable,
            TargetState::StructurallyIneligible,
            TargetState::NotInteractable,
        ];
        let colours: Vec<_> = states
            .iter()
            .map(|state| semantics.target(*state))
            .collect();

        for (i, a) in colours.iter().enumerate() {
            for (j, b) in colours.iter().enumerate() {
                if i != j {
                    assert_ne!(a, b, "{:?} and {:?} share a colour", states[i], states[j]);
                }
            }
        }
    }

    #[test]
    fn every_job_outcome_has_its_own_colour() {
        use aeon_data::model::JobResultKind;

        let semantics = UiTheme::embedded().semantics;
        let colours: Vec<_> = JobResultKind::ALL
            .iter()
            .map(|kind| semantics.outcome(*kind))
            .collect();
        for (i, a) in colours.iter().enumerate() {
            for (j, b) in colours.iter().enumerate() {
                if i != j {
                    assert_ne!(a, b, "two graded outcomes share a colour");
                }
            }
        }
    }

    #[test]
    fn applying_a_theme_writes_every_widget_state() {
        // egui resolves appearance by widget state rather than by cascade, so
        // any state left unwritten keeps egui's own default and quietly
        // breaks the palette.
        let theme = UiTheme::embedded();
        let mut style = egui::Style::default();
        theme.apply(&mut style);

        let widgets = &style.visuals.widgets;
        let fills = [
            widgets.inactive.bg_fill,
            widgets.hovered.bg_fill,
            widgets.active.bg_fill,
            widgets.open.bg_fill,
        ];
        let default_style = egui::Style::default();
        let defaults = [
            default_style.visuals.widgets.inactive.bg_fill,
            default_style.visuals.widgets.hovered.bg_fill,
            default_style.visuals.widgets.active.bg_fill,
            default_style.visuals.widgets.open.bg_fill,
        ];
        for (applied, default) in fills.iter().zip(defaults.iter()) {
            assert_ne!(applied, default, "a widget state kept egui's default fill");
        }

        assert_eq!(
            style.visuals.panel_fill,
            egui::Color32::from(theme.palette.panel),
            "panels take the themed surface"
        );
        assert!(
            style.text_styles.len() >= 5,
            "every text role is given a size"
        );
    }
}
