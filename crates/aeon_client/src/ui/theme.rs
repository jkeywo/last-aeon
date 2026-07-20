//! The interface's design tokens, and how they reach egui.
//!
//! Everything about how the interface *looks* lives in one serialisable
//! token set rather than being scattered through the panels as literals.
//! That makes the appearance something a designer can specify — a palette,
//! a type scale, a spacing grid, how a button behaves under the pointer —
//! and the code's job merely to apply it.
//!
//! The tokens are deliberately shaped like egui rather than like CSS.
//! egui's style is keyed by widget *state* (inactive, hovered, active,
//! open) rather than cascading, so every interactive surface is given for
//! every state.
//!
//! **The rule this file lives by: every token is read.** A token nothing
//! reads is worse than no token at all, because it lies to whoever sets it
//! — they change a value, see nothing happen, and lose confidence in the
//! whole file. [`tests::every_token_changes_something`] enforces this by
//! name, and it exists because three tokens had already gone bad without
//! anyone noticing.
//!
//! There is deliberately no disabled *colour*. egui has no disabled widget
//! state: it fades whatever state a control is in by
//! [`Interaction::disabled_alpha`]. A `palette.disabled` entry would have
//! nowhere to go — which is precisely why the one that used to be here was
//! dead.
//!
//! Two things else are deliberately *not* tokens. There is no font-family
//! token, because the build ships a single face and egui cannot select a
//! family that was never registered — it would be a token that does
//! nothing. There is no separate heading colour, because egui derives
//! emphasis from the active widget state rather than from the text style;
//! [`Palette::text_strong`] names that mechanism honestly instead of
//! pretending to a control egui does not offer.
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

/// A cast shadow. egui draws one per surface, so this is offset, blur,
/// spread and colour and nothing more.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Shadow {
    /// Horizontal offset.
    pub offset_x: i8,
    /// Vertical offset.
    pub offset_y: i8,
    /// How far the edge is softened.
    pub blur: u8,
    /// How far the shadow grows before blurring.
    pub spread: u8,
    /// Its colour, usually mostly transparent.
    pub colour: Rgba,
}

impl From<Shadow> for egui::epaint::Shadow {
    fn from(value: Shadow) -> Self {
        egui::epaint::Shadow {
            offset: [value.offset_x, value.offset_y],
            blur: value.blur,
            spread: value.spread,
            color: value.colour.into(),
        }
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
    /// A *frameless* control at rest.
    ///
    /// egui distinguishes the fill of a control that has an obvious box
    /// from one that does not — a button versus a bare clickable label.
    /// Collapsing the two, as this file used to, means one of them is
    /// always drawn wrong.
    pub inactive_weak: Rgba,
    /// A frameless control under the pointer.
    pub hovered_weak: Rgba,
    /// A frameless control being pressed.
    pub active_weak: Rgba,
    /// A frameless control holding something open.
    pub open_weak: Rgba,

    /// Ordinary text.
    pub text: Rgba,
    /// Text of lesser importance.
    pub text_weak: Rgba,
    /// Emphasised text. egui takes this from the active widget state, so
    /// it is also the colour of a pressed control's label.
    pub text_strong: Rgba,
    /// Something that can be followed.
    pub link: Rgba,
    /// A quiet dividing line.
    pub border: Rgba,
    /// A line that should be noticed.
    pub border_strong: Rgba,
    /// The selection.
    pub selection: Rgba,

    /// Something has gone wrong.
    pub error: Rgba,
    /// Something deserves care.
    pub warn: Rgba,
    /// The ground behind monospaced figures.
    pub code_bg: Rgba,
}

/// Text sizes by role. One family, so hierarchy comes from size and
/// colour rather than weight.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Typography {
    /// Panel headings.
    pub heading: u8,
    /// Ordinary text.
    pub body: u8,
    /// Text on controls.
    pub button: u8,
    /// Secondary and annotation text.
    pub small: u8,
    /// Figures that should line up.
    pub monospace: u8,
    /// A province name on the map.
    pub map_label: u8,
    /// The selected province's name, which is drawn larger.
    pub map_label_selected: u8,
}

/// The spacing grid, and the size of every control egui measures for us.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Spacing {
    /// Between items in a row.
    pub item_gap_x: u8,
    /// Between stacked items.
    pub item_gap_y: u8,
    /// Inside a panel's edge.
    pub panel_margin: u8,
    /// Inside a menu's edge.
    pub menu_margin: u8,
    /// One step of indentation.
    pub indent: u8,
    /// Left and right of a button's label.
    pub button_pad_x: u8,
    /// Above and below it. Its own token rather than half the horizontal
    /// one, which is a proportion a designer should be able to argue with.
    pub button_pad_y: u8,
    /// The height a row is guaranteed.
    pub row_height: u8,
    /// The width a control is guaranteed.
    pub row_min_width: u8,
    /// A checkbox or radio.
    pub icon_width: u8,
    /// The mark inside it.
    pub icon_inner_width: u8,
    /// Between the mark and its label.
    pub icon_gap: u8,
    /// A slider's track.
    pub slider_width: u8,
    /// How thick that track is.
    pub slider_rail_height: u8,
    /// A dropdown.
    pub combo_width: u8,
    /// How far a dropdown may open before scrolling.
    pub combo_height: u8,
    /// A text field.
    pub text_edit_width: u8,
    /// How wide a tooltip may grow before wrapping.
    pub tooltip_width: u16,
    /// A menu.
    pub menu_width: u16,
    /// Between menu entries.
    pub menu_gap: u8,
    /// A scrollbar.
    pub scroll_bar_width: u8,
    /// The shortest a scroll handle may become.
    pub scroll_handle_min: u8,
    /// Inside the scrollbar's own track.
    pub scroll_bar_inner_margin: u8,
    /// Between the scrollbar and the content.
    pub scroll_bar_outer_margin: u8,
    /// How far drawing may spill past a clip edge.
    pub clip_margin: u8,
    /// The drag handle on a resizable window.
    pub resize_corner: u8,
}

/// Corners, lines, and cast shadows.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShapeTokens {
    /// Corner radius on small controls.
    pub radius_small: u8,
    /// Corner radius on panels and popups.
    pub radius_large: u8,
    /// Corner radius on menus.
    pub radius_menu: u8,

    /// The outline of a control at rest.
    pub stroke_rest: u8,
    /// Under the pointer. Thickening on hover is one of the cheapest ways
    /// to make a control feel like it is answering.
    pub stroke_hover: u8,
    /// While pressed.
    pub stroke_active: u8,
    /// While holding something open.
    pub stroke_open: u8,
    /// On a surface that is not a control at all.
    pub stroke_noninteractive: u8,
    /// The weight text is drawn at.
    pub text_stroke: u8,
    /// A window's own outline.
    pub window_stroke: u8,

    /// What a window casts.
    pub window_shadow: Shadow,
    /// What a popup or tooltip casts.
    pub popup_shadow: Shadow,
}

/// How the interface answers the pointer.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Interaction {
    /// How far a control grows under the pointer, in points.
    pub expansion_hover: f32,
    /// How far it grows while pressed. Smaller than the hover value reads
    /// as the control being pushed in.
    pub expansion_active: f32,
    /// How long a state change takes to play out, in seconds.
    pub animation_time: f32,
    /// How much of its colour a disabled control keeps.
    pub disabled_alpha: f32,
    /// How much of its colour secondary text keeps, where no explicit
    /// weak colour is given.
    pub weak_text_alpha: f32,
}

/// Structural choices that are on or off.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Chrome {
    /// Whether buttons draw a box at all.
    pub button_frame: bool,
    /// Whether a collapsing section draws one.
    pub collapsing_header_frame: bool,
    /// Whether grids band their rows.
    pub striped: bool,
    /// Whether an indent draws the line that shows what it belongs to.
    pub indent_left_vline: bool,
    /// Whether that line finishes with a foot.
    pub indent_ends_with_line: bool,
    /// Whether a slider fills the track behind its handle.
    pub slider_trailing_fill: bool,
    /// Whether scrollbars float over the content instead of taking room.
    pub scroll_floating: bool,
    /// Whether the front window is drawn differently from those behind.
    pub window_highlight_topmost: bool,
}

/// Sizes for the parts the client draws itself, which egui knows nothing
/// about and so cannot be styled through [`egui::Style`].
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Components {
    /// A colour swatch beside a legend entry or a house name.
    pub swatch_size: u8,
    /// Its corner radius.
    pub swatch_radius: u8,
    /// A square icon button in the top bar.
    pub icon_button: u8,
    /// How far an icon is inset inside its button, in hundredths.
    pub icon_inset: u8,
    /// How thick an icon's strokes are, in hundredths of its box.
    pub icon_stroke: u8,
    /// How wide the character picker opens.
    pub picker_width: u16,
    /// How far it scrolls before clipping.
    pub picker_max_height: u16,
    /// How wide a candidate's hovered breakdown may grow.
    pub picker_hover_width: u16,
    /// How wide the search results are.
    pub search_width: u16,
    /// How far they scroll before clipping.
    pub search_max_height: u16,
    /// How far below the top bar they sit.
    pub search_offset_y: u8,
    /// How far in from the left the situation strip sits.
    pub strip_offset_x: u16,
    /// How far down from the top.
    pub strip_offset_y: u8,
    /// The log's free-text filter box.
    pub log_filter_width: u16,
    /// How many log entries are kept on screen.
    pub log_max_entries: u16,
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
#[derive(Resource, Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct UiTheme {
    /// Surfaces and text.
    pub palette: Palette,
    /// Text sizes.
    pub typography: Typography,
    /// The spacing grid and control sizes.
    pub spacing: Spacing,
    /// Corners, lines and shadows.
    pub shape: ShapeTokens,
    /// How the interface answers the pointer.
    pub interaction: Interaction,
    /// Structural choices that are on or off.
    pub chrome: Chrome,
    /// Sizes for the parts the client draws itself.
    pub components: Components,
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
        self.apply_visuals(&mut style.visuals);
        self.apply_spacing(&mut style.spacing);
        self.apply_text(style);
        style.animation_time = self.interaction.animation_time;
    }

    fn apply_visuals(&self, visuals: &mut egui::Visuals) {
        let p = &self.palette;
        let s = &self.shape;

        visuals.dark_mode = true;
        visuals.panel_fill = p.panel.into();
        visuals.window_fill = p.popup.into();
        visuals.extreme_bg_color = p.window.into();
        visuals.faint_bg_color = p.panel_alt.into();
        visuals.code_bg_color = p.code_bg.into();
        // Left at egui's stock red and yellow, these fight any palette.
        visuals.error_fg_color = p.error.into();
        visuals.warn_fg_color = p.warn.into();

        visuals.window_corner_radius = egui::CornerRadius::same(s.radius_large);
        visuals.menu_corner_radius = egui::CornerRadius::same(s.radius_menu);
        visuals.window_stroke = egui::Stroke::new(f32::from(s.window_stroke), p.border);
        visuals.window_shadow = s.window_shadow.into();
        visuals.popup_shadow = s.popup_shadow.into();

        visuals.selection.bg_fill = p.selection.into();
        visuals.selection.stroke = egui::Stroke::new(f32::from(s.text_stroke), p.text_strong);
        visuals.hyperlink_color = p.link.into();
        visuals.weak_text_color = Some(p.text_weak.into());
        visuals.weak_text_alpha = self.interaction.weak_text_alpha;
        visuals.disabled_alpha = self.interaction.disabled_alpha;

        let radius = egui::CornerRadius::same(s.radius_small);
        let text = f32::from(s.text_stroke);
        let widget = |fill: Rgba, weak: Rgba, stroke: Rgba, width: u8, fg: Rgba, grow: f32| {
            egui::style::WidgetVisuals {
                bg_fill: fill.into(),
                weak_bg_fill: weak.into(),
                bg_stroke: egui::Stroke::new(f32::from(width), stroke),
                fg_stroke: egui::Stroke::new(text, fg),
                corner_radius: radius,
                expansion: grow,
            }
        };

        // `noninteractive` is where egui reads ordinary label colour from,
        // not just where it draws inert surfaces — so this state carries
        // `text`, and secondary text is asked for explicitly via `weak`.
        visuals.widgets.noninteractive = widget(
            p.panel,
            p.panel,
            p.border,
            s.stroke_noninteractive,
            p.text,
            0.0,
        );
        visuals.widgets.inactive = widget(
            p.inactive,
            p.inactive_weak,
            p.border,
            s.stroke_rest,
            p.text,
            0.0,
        );
        visuals.widgets.hovered = widget(
            p.hovered,
            p.hovered_weak,
            p.border_strong,
            s.stroke_hover,
            p.text,
            self.interaction.expansion_hover,
        );
        // egui takes emphasised text from the active state, so this is
        // also where `ui.strong` and headings get their colour.
        visuals.widgets.active = widget(
            p.active,
            p.active_weak,
            p.border_strong,
            s.stroke_active,
            p.text_strong,
            self.interaction.expansion_active,
        );
        visuals.widgets.open = widget(
            p.open,
            p.open_weak,
            p.border_strong,
            s.stroke_open,
            p.text,
            0.0,
        );

        visuals.button_frame = self.chrome.button_frame;
        visuals.collapsing_header_frame = self.chrome.collapsing_header_frame;
        visuals.striped = self.chrome.striped;
        visuals.indent_has_left_vline = self.chrome.indent_left_vline;
        visuals.slider_trailing_fill = self.chrome.slider_trailing_fill;
        visuals.window_highlight_topmost = self.chrome.window_highlight_topmost;
        visuals.resize_corner_size = f32::from(self.spacing.resize_corner);
        visuals.clip_rect_margin = f32::from(self.spacing.clip_margin);
    }

    fn apply_spacing(&self, spacing: &mut egui::style::Spacing) {
        let s = &self.spacing;
        spacing.item_spacing = egui::vec2(f32::from(s.item_gap_x), f32::from(s.item_gap_y));
        spacing.window_margin = egui::Margin::same(s.panel_margin as i8);
        spacing.menu_margin = egui::Margin::same(s.menu_margin as i8);
        spacing.button_padding = egui::vec2(f32::from(s.button_pad_x), f32::from(s.button_pad_y));
        spacing.indent = f32::from(s.indent);
        spacing.interact_size = egui::vec2(f32::from(s.row_min_width), f32::from(s.row_height));
        spacing.icon_width = f32::from(s.icon_width);
        spacing.icon_width_inner = f32::from(s.icon_inner_width);
        spacing.icon_spacing = f32::from(s.icon_gap);
        spacing.slider_width = f32::from(s.slider_width);
        spacing.slider_rail_height = f32::from(s.slider_rail_height);
        spacing.combo_width = f32::from(s.combo_width);
        spacing.combo_height = f32::from(s.combo_height);
        spacing.text_edit_width = f32::from(s.text_edit_width);
        spacing.tooltip_width = f32::from(s.tooltip_width);
        spacing.menu_width = f32::from(s.menu_width);
        spacing.menu_spacing = f32::from(s.menu_gap);
        spacing.indent_ends_with_horizontal_line = self.chrome.indent_ends_with_line;

        spacing.scroll.floating = self.chrome.scroll_floating;
        spacing.scroll.bar_width = f32::from(s.scroll_bar_width);
        spacing.scroll.handle_min_length = f32::from(s.scroll_handle_min);
        spacing.scroll.bar_inner_margin = f32::from(s.scroll_bar_inner_margin);
        spacing.scroll.bar_outer_margin = f32::from(s.scroll_bar_outer_margin);
    }

    fn apply_text(&self, style: &mut egui::Style) {
        use egui::FontFamily::{Monospace, Proportional};
        use egui::TextStyle as T;
        let t = &self.typography;
        style.text_styles = [
            (
                T::Heading,
                egui::FontId::new(f32::from(t.heading), Proportional),
            ),
            (T::Body, egui::FontId::new(f32::from(t.body), Proportional)),
            (
                T::Button,
                egui::FontId::new(f32::from(t.button), Proportional),
            ),
            (
                T::Small,
                egui::FontId::new(f32::from(t.small), Proportional),
            ),
            (
                T::Monospace,
                egui::FontId::new(f32::from(t.monospace), Monospace),
            ),
        ]
        .into();
    }
}

/// Reloads the token file from disk while the game is running.
///
/// Native only, and deliberately so: the point is the design loop — save
/// `theme.ron`, see it — and the web build has no filesystem to watch. The
/// embedded copy remains what ships.
///
/// A broken file logs and is ignored rather than panicking. That is the
/// opposite of [`UiTheme::embedded`], and for a good reason: a token file
/// that fails to parse at build time is a bug, while one that fails to
/// parse mid-edit is just a designer halfway through typing a colour.
/// Taking the game down over it would make the feature useless.
#[cfg(not(target_arch = "wasm32"))]
pub fn reload_theme_from_disk(
    mut theme: bevy::prelude::ResMut<UiTheme>,
    time: bevy::prelude::Res<bevy::prelude::Time>,
    mut next_check: bevy::prelude::Local<f32>,
    mut last_seen: bevy::prelude::Local<Option<std::time::SystemTime>>,
) {
    // A file a human edits does not need checking every frame.
    const INTERVAL_SECONDS: f32 = 0.5;
    let now = time.elapsed_secs();
    if now < *next_check {
        return;
    }
    *next_check = now + INTERVAL_SECONDS;

    let Some(path) = theme_path() else {
        return;
    };
    let Ok(modified) = std::fs::metadata(&path).and_then(|meta| meta.modified()) else {
        return;
    };
    // The first sighting establishes the baseline rather than reloading:
    // what is on disk at startup is what was compiled in.
    if last_seen.is_none() {
        *last_seen = Some(modified);
        return;
    }
    if *last_seen == Some(modified) {
        return;
    }
    *last_seen = Some(modified);

    match std::fs::read_to_string(&path) {
        Ok(text) => match ron::from_str::<UiTheme>(&text) {
            Ok(parsed) => {
                bevy::log::info!("theme reloaded from {}", path.display());
                *theme = parsed;
            }
            Err(error) => bevy::log::warn!("theme.ron not applied — {error}"),
        },
        Err(error) => bevy::log::warn!("theme.ron not readable — {error}"),
    }
}

/// Where the live token file is, if it can be found.
///
/// Tried relative to the working directory first, which is how the game is
/// launched from the repository root, then relative to the crate as it was
/// built. Neither existing simply means no hot-reload.
#[cfg(not(target_arch = "wasm32"))]
fn theme_path() -> Option<std::path::PathBuf> {
    const RELATIVE: &str = "crates/aeon_client/assets/theme.ron";
    let candidates = [
        std::path::PathBuf::from(RELATIVE),
        std::path::PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/theme.ron")),
    ];
    candidates.into_iter().find(|path| path.is_file())
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

    /// Applies a theme and returns the resulting style, comparably.
    ///
    /// `egui::Style` is not `PartialEq`, so the rendered debug form stands
    /// in for it. That is enough: if two styles print identically, no token
    /// distinguished them.
    fn applied(theme: &UiTheme) -> String {
        let mut style = egui::Style::default();
        theme.apply(&mut style);
        format!("{style:?}")
    }

    #[test]
    fn every_token_changes_something() {
        // The rule this file lives by. A token that alters nothing is a
        // promise to a designer that the code does not keep: they set it,
        // see no change, and stop trusting the file. Three tokens had
        // already gone bad this way — `palette.disabled` and
        // `palette.heading` were never read at all, and ordinary label
        // colour was being taken from `text_weak` — before this test
        // existed to say so.
        //
        // Each row names a token and a way to change it. If applying the
        // changed theme yields the same style, that row fails by name.
        //
        // This covers every token that reaches `egui::Style`. The
        // `components` group and the two map-label sizes do not — they are
        // read at call sites that draw by hand — so they are absent here
        // by design rather than by oversight.
        let base = UiTheme::embedded();
        let reference = applied(&base);

        type Mutate = fn(&mut UiTheme);
        let cases: &[(&str, Mutate)] = &[
            ("palette.window", |t| {
                t.palette.window = Rgba {
                    r: 1,
                    g: 2,
                    b: 3,
                    a: 255,
                }
            }),
            ("palette.panel", |t| {
                t.palette.panel = Rgba {
                    r: 1,
                    g: 2,
                    b: 3,
                    a: 255,
                }
            }),
            ("palette.panel_alt", |t| {
                t.palette.panel_alt = Rgba {
                    r: 1,
                    g: 2,
                    b: 3,
                    a: 255,
                }
            }),
            ("palette.popup", |t| {
                t.palette.popup = Rgba {
                    r: 1,
                    g: 2,
                    b: 3,
                    a: 255,
                }
            }),
            ("palette.inactive", |t| {
                t.palette.inactive = Rgba {
                    r: 1,
                    g: 2,
                    b: 3,
                    a: 255,
                }
            }),
            ("palette.hovered", |t| {
                t.palette.hovered = Rgba {
                    r: 1,
                    g: 2,
                    b: 3,
                    a: 255,
                }
            }),
            ("palette.active", |t| {
                t.palette.active = Rgba {
                    r: 1,
                    g: 2,
                    b: 3,
                    a: 255,
                }
            }),
            ("palette.open", |t| {
                t.palette.open = Rgba {
                    r: 1,
                    g: 2,
                    b: 3,
                    a: 255,
                }
            }),
            ("palette.inactive_weak", |t| {
                t.palette.inactive_weak = Rgba {
                    r: 1,
                    g: 2,
                    b: 3,
                    a: 255,
                }
            }),
            ("palette.hovered_weak", |t| {
                t.palette.hovered_weak = Rgba {
                    r: 1,
                    g: 2,
                    b: 3,
                    a: 255,
                }
            }),
            ("palette.active_weak", |t| {
                t.palette.active_weak = Rgba {
                    r: 1,
                    g: 2,
                    b: 3,
                    a: 255,
                }
            }),
            ("palette.open_weak", |t| {
                t.palette.open_weak = Rgba {
                    r: 1,
                    g: 2,
                    b: 3,
                    a: 255,
                }
            }),
            ("palette.text", |t| {
                t.palette.text = Rgba {
                    r: 1,
                    g: 2,
                    b: 3,
                    a: 255,
                }
            }),
            ("palette.text_weak", |t| {
                t.palette.text_weak = Rgba {
                    r: 1,
                    g: 2,
                    b: 3,
                    a: 255,
                }
            }),
            ("palette.text_strong", |t| {
                t.palette.text_strong = Rgba {
                    r: 1,
                    g: 2,
                    b: 3,
                    a: 255,
                }
            }),
            ("palette.link", |t| {
                t.palette.link = Rgba {
                    r: 1,
                    g: 2,
                    b: 3,
                    a: 255,
                }
            }),
            ("palette.border", |t| {
                t.palette.border = Rgba {
                    r: 1,
                    g: 2,
                    b: 3,
                    a: 255,
                }
            }),
            ("palette.border_strong", |t| {
                t.palette.border_strong = Rgba {
                    r: 1,
                    g: 2,
                    b: 3,
                    a: 255,
                }
            }),
            ("palette.selection", |t| {
                t.palette.selection = Rgba {
                    r: 1,
                    g: 2,
                    b: 3,
                    a: 255,
                }
            }),
            ("palette.error", |t| {
                t.palette.error = Rgba {
                    r: 1,
                    g: 2,
                    b: 3,
                    a: 255,
                }
            }),
            ("palette.warn", |t| {
                t.palette.warn = Rgba {
                    r: 1,
                    g: 2,
                    b: 3,
                    a: 255,
                }
            }),
            ("palette.code_bg", |t| {
                t.palette.code_bg = Rgba {
                    r: 1,
                    g: 2,
                    b: 3,
                    a: 255,
                }
            }),
            ("typography.heading", |t| t.typography.heading += 3),
            ("typography.body", |t| t.typography.body += 3),
            ("typography.button", |t| t.typography.button += 3),
            ("typography.small", |t| t.typography.small += 3),
            ("typography.monospace", |t| t.typography.monospace += 3),
            ("spacing.item_gap_x", |t| t.spacing.item_gap_x += 3),
            ("spacing.item_gap_y", |t| t.spacing.item_gap_y += 3),
            ("spacing.panel_margin", |t| t.spacing.panel_margin += 3),
            ("spacing.menu_margin", |t| t.spacing.menu_margin += 3),
            ("spacing.indent", |t| t.spacing.indent += 3),
            ("spacing.button_pad_x", |t| t.spacing.button_pad_x += 3),
            ("spacing.button_pad_y", |t| t.spacing.button_pad_y += 3),
            ("spacing.row_height", |t| t.spacing.row_height += 3),
            ("spacing.row_min_width", |t| t.spacing.row_min_width += 3),
            ("spacing.icon_width", |t| t.spacing.icon_width += 3),
            ("spacing.icon_inner_width", |t| {
                t.spacing.icon_inner_width += 3
            }),
            ("spacing.icon_gap", |t| t.spacing.icon_gap += 3),
            ("spacing.slider_width", |t| t.spacing.slider_width += 3),
            ("spacing.slider_rail_height", |t| {
                t.spacing.slider_rail_height += 3
            }),
            ("spacing.combo_width", |t| t.spacing.combo_width += 3),
            ("spacing.combo_height", |t| t.spacing.combo_height += 3),
            ("spacing.text_edit_width", |t| {
                t.spacing.text_edit_width += 3
            }),
            ("spacing.tooltip_width", |t| t.spacing.tooltip_width += 3),
            ("spacing.menu_width", |t| t.spacing.menu_width += 3),
            ("spacing.menu_gap", |t| t.spacing.menu_gap += 3),
            ("spacing.scroll_bar_width", |t| {
                t.spacing.scroll_bar_width += 3
            }),
            ("spacing.scroll_handle_min", |t| {
                t.spacing.scroll_handle_min += 3
            }),
            ("spacing.scroll_bar_inner_margin", |t| {
                t.spacing.scroll_bar_inner_margin += 3
            }),
            ("spacing.scroll_bar_outer_margin", |t| {
                t.spacing.scroll_bar_outer_margin += 3
            }),
            ("spacing.clip_margin", |t| t.spacing.clip_margin += 3),
            ("spacing.resize_corner", |t| t.spacing.resize_corner += 3),
            ("shape.radius_small", |t| t.shape.radius_small += 3),
            ("shape.radius_large", |t| t.shape.radius_large += 3),
            ("shape.radius_menu", |t| t.shape.radius_menu += 3),
            ("shape.stroke_rest", |t| t.shape.stroke_rest += 3),
            ("shape.stroke_hover", |t| t.shape.stroke_hover += 3),
            ("shape.stroke_active", |t| t.shape.stroke_active += 3),
            ("shape.stroke_open", |t| t.shape.stroke_open += 3),
            ("shape.stroke_noninteractive", |t| {
                t.shape.stroke_noninteractive += 3
            }),
            ("shape.text_stroke", |t| t.shape.text_stroke += 3),
            ("shape.window_stroke", |t| t.shape.window_stroke += 3),
            ("shape.window_shadow", |t| t.shape.window_shadow.blur += 5),
            ("shape.popup_shadow", |t| t.shape.popup_shadow.blur += 5),
            ("interaction.expansion_hover", |t| {
                t.interaction.expansion_hover += 2.0
            }),
            ("interaction.expansion_active", |t| {
                t.interaction.expansion_active += 2.0
            }),
            ("interaction.animation_time", |t| {
                t.interaction.animation_time += 0.5
            }),
            ("interaction.disabled_alpha", |t| {
                t.interaction.disabled_alpha *= 0.5
            }),
            ("interaction.weak_text_alpha", |t| {
                t.interaction.weak_text_alpha *= 0.5
            }),
            ("chrome.button_frame", |t| {
                t.chrome.button_frame = !t.chrome.button_frame
            }),
            ("chrome.collapsing_header_frame", |t| {
                t.chrome.collapsing_header_frame = !t.chrome.collapsing_header_frame
            }),
            ("chrome.striped", |t| t.chrome.striped = !t.chrome.striped),
            ("chrome.indent_left_vline", |t| {
                t.chrome.indent_left_vline = !t.chrome.indent_left_vline
            }),
            ("chrome.indent_ends_with_line", |t| {
                t.chrome.indent_ends_with_line = !t.chrome.indent_ends_with_line
            }),
            ("chrome.slider_trailing_fill", |t| {
                t.chrome.slider_trailing_fill = !t.chrome.slider_trailing_fill
            }),
            ("chrome.scroll_floating", |t| {
                t.chrome.scroll_floating = !t.chrome.scroll_floating
            }),
            ("chrome.window_highlight_topmost", |t| {
                t.chrome.window_highlight_topmost = !t.chrome.window_highlight_topmost
            }),
        ];

        for (name, mutate) in cases {
            let mut changed = base.clone();
            mutate(&mut changed);
            assert_ne!(
                applied(&changed),
                reference,
                "token `{name}` is declared but changes nothing when applied"
            );
        }
    }

    #[test]
    fn ordinary_text_is_not_drawn_in_the_secondary_colour() {
        // egui reads plain label colour from the *noninteractive* state, so
        // pointing that at `text_weak` — as this file once did — quietly
        // drew every label in the game in the muted tone and left the
        // brighter `text` reaching only button labels.
        let theme = UiTheme::embedded();
        let mut style = egui::Style::default();
        theme.apply(&mut style);
        assert_eq!(
            style.visuals.text_color(),
            egui::Color32::from(theme.palette.text),
            "plain text takes the primary text colour"
        );
        assert_eq!(
            style.visuals.weak_text_color(),
            egui::Color32::from(theme.palette.text_weak),
            "and secondary text is the one that is muted"
        );
        assert_eq!(
            style.visuals.strong_text_color(),
            egui::Color32::from(theme.palette.text_strong),
            "and emphasis is its own colour"
        );
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
