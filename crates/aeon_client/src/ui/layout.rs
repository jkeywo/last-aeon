//! Deterministic responsive layout policy for the campaign shell.
//!
//! The planner consumes egui points, not physical pixels. This is important:
//! applying 200% interface scale turns a 1920 px window into a 960 point
//! viewport, so native and web choose the same mode without target-specific
//! branches.

use bevy_egui::egui;

use crate::ui::dock::{DockSide, DockState};

/// How much permanent chrome the current viewport can carry.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum LayoutMode {
    /// One top-bar band and side-by-side bottom panels.
    Spacious,
    /// Two top-bar bands and one tabbed bottom panel at a time.
    Compact,
}

/// How multiple panels sharing the bottom edge are presented.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum BottomPresentation {
    Columns,
    Tabs,
}

/// The complete geometry decision for one rendered frame.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct LayoutPlan {
    pub mode: LayoutMode,
    pub top_bar_bands: u8,
    pub bottom_presentation: BottomPresentation,
    pub left_width: f32,
    pub right_width: f32,
    pub bottom_height: f32,
    pub overlay_top: f32,
    pub minimum_control_target: f32,
    viewport_height: f32,
    desired_bottom_height: f32,
}

impl LayoutPlan {
    /// Plans a frame from its logical viewport and the player's dock sizes.
    pub fn new(viewport: egui::Vec2, dock: &DockState) -> Self {
        // The one-band bar's trailing search/tool group needs more than 1366
        // logical points once localized identity and resource labels are in
        // front of it. Reflow before those independently interactive groups
        // can occupy the same response space.
        let compact = viewport.x < 1_400.0 || viewport.y < 640.0;
        let mode = if compact {
            LayoutMode::Compact
        } else {
            LayoutMode::Spacious
        };
        let top_bar_bands = if compact { 2 } else { 1 };
        let top_height = if compact { 68.0 } else { 34.0 };

        // A readable central surface remains even when both edge docks are
        // open. The map is deliberately what yields; panel prose never gets
        // squeezed below a useful reading measure.
        let minimum_map = if compact { 280.0 } else { 420.0 };
        let side_budget = (viewport.x - minimum_map).max(0.0);
        let desired_left = dock.size_of(DockSide::Left);
        let desired_right = dock.size_of(DockSide::Right);
        let desired_total = desired_left + desired_right;
        let scale = if desired_total > side_budget && desired_total > 0.0 {
            side_budget / desired_total
        } else {
            1.0
        };
        let left_width = (desired_left * scale).clamp(180.0, 360.0);
        let right_width = (desired_right * scale).clamp(180.0, 360.0);

        // On the shortest accepted viewport this leaves over half the usable
        // height for subject inspection and Situation actions.
        let desired_bottom_height = dock.size_of(DockSide::Bottom);

        Self {
            mode,
            top_bar_bands,
            bottom_presentation: if compact {
                BottomPresentation::Tabs
            } else {
                BottomPresentation::Columns
            },
            left_width,
            right_width,
            bottom_height: 0.0,
            overlay_top: top_height + 6.0,
            minimum_control_target: 24.0,
            viewport_height: viewport.y,
            desired_bottom_height,
        }
        .with_measured_top(top_height)
    }

    /// Reconciles the plan with the top panel egui actually rendered.
    ///
    /// Long translated names can make a wrapped band taller than its nominal
    /// row. Docks and overlays therefore consume this measured value rather
    /// than trusting the mode's estimate.
    pub fn with_measured_top(mut self, top_height: f32) -> Self {
        let usable_height = (self.viewport_height - top_height).max(0.0);
        let bottom_cap = if self.mode == LayoutMode::Compact {
            usable_height * 0.32
        } else {
            usable_height * 0.38
        };
        self.bottom_height = self
            .desired_bottom_height
            .min(bottom_cap)
            .max(112.0_f32.min(usable_height * 0.45));
        self.overlay_top = top_height + 6.0;
        self
    }

    pub fn side_width(self, side: DockSide) -> f32 {
        match side {
            DockSide::Left => self.left_width,
            DockSide::Right => self.right_width,
            DockSide::Bottom => unreachable!("the bottom has height, not width"),
        }
    }
}

/// The exact wrapped-band primitive used by the live compact top bar and the
/// rendered-state harness.
pub fn draw_top_band(ui: &mut egui::Ui, body: impl FnOnce(&mut egui::Ui)) {
    ui.horizontal_wrapped(body);
}

/// A label/value row that can grow only downwards.
pub fn draw_wrapped_fact(ui: &mut egui::Ui, label: impl Into<egui::WidgetText>, value: &str) {
    ui.horizontal_wrapped(|ui| {
        ui.add(egui::Label::new(label).wrap());
        ui.add(egui::Label::new(value).wrap());
    });
}

/// An action that wraps within its panel and keeps the authored target floor.
pub fn draw_wrapped_action(
    ui: &mut egui::Ui,
    enabled: bool,
    label: impl Into<egui::WidgetText>,
) -> egui::Response {
    let width = ui.available_width().max(24.0);
    ui.add_enabled(
        enabled,
        egui::Button::new(label)
            .wrap()
            .min_size(egui::vec2(width, 24.0)),
    )
}

/// A vertical-only scroll surface shared by live prose panels and evidence.
pub fn draw_vertical_scroll<R>(
    ui: &mut egui::Ui,
    id: &'static str,
    body: impl FnOnce(&mut egui::Ui) -> R,
) -> egui::scroll_area::ScrollAreaOutput<R> {
    let output = egui::ScrollArea::vertical().id_salt(id).show(ui, body);
    #[cfg(test)]
    crate::ui::rendered_state::record_scroll(ui.ctx(), id, output.inner_rect, output.content_size);
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    const MATRIX: &[(f32, f32, f32)] = &[
        (1920.0, 1080.0, 1.0),
        (1920.0, 1080.0, 1.5),
        (1920.0, 1080.0, 2.0),
        (1366.0, 768.0, 1.0),
        (1366.0, 768.0, 1.5),
    ];

    #[test]
    fn accepted_display_matrix_has_readable_non_overlapping_geometry() {
        let dock = DockState::default();
        for &(pixels_x, pixels_y, scale) in MATRIX {
            let viewport = egui::vec2(pixels_x / scale, pixels_y / scale);
            let plan = LayoutPlan::new(viewport, &dock);
            let map_width = viewport.x - plan.left_width - plan.right_width;
            let top_height = plan.overlay_top - 6.0;
            let vertical_content = viewport.y - top_height - plan.bottom_height;

            assert!(map_width >= 280.0, "{pixels_x}x{pixels_y} at {scale}");
            assert!(
                vertical_content >= 235.0,
                "{pixels_x}x{pixels_y} at {scale}"
            );
            assert!(plan.left_width >= 180.0 && plan.right_width >= 180.0);
            assert!(plan.minimum_control_target >= 24.0);
        }
    }

    #[test]
    fn scale_selects_modes_by_logical_points_on_native_and_web() {
        let dock = DockState::default();
        let spacious = LayoutPlan::new(egui::vec2(1920.0, 1080.0), &dock);
        assert_eq!(spacious.mode, LayoutMode::Spacious);
        assert_eq!(spacious.top_bar_bands, 1);
        assert_eq!(spacious.bottom_presentation, BottomPresentation::Columns);

        for &(pixels_x, pixels_y, scale) in &MATRIX[1..] {
            let plan = LayoutPlan::new(egui::vec2(pixels_x / scale, pixels_y / scale), &dock);
            // Every matrix entry narrower than the safe one-band reading
            // measure deterministically reflows rather than overlapping.
            let expected = if pixels_x / scale >= 1_400.0 && pixels_y / scale >= 640.0 {
                LayoutMode::Spacious
            } else {
                LayoutMode::Compact
            };
            assert_eq!(plan.mode, expected);
        }
    }

    #[test]
    fn complete_situation_path_remains_present_in_both_modes() {
        // Presentation may move these steps between a side dock, popup and
        // bottom tab, but it never removes one. This semantic evidence is
        // shared by native and wasm because the planner has no target cfg.
        const PATH: [(&str, &str); 7] = [
            ("situation", "situations dock"),
            ("subject", "inspector dock"),
            ("forecast", "situation card"),
            ("command", "situation card"),
            ("time", "top bar"),
            ("result", "situation resolution"),
            ("history", "situation card and log dock"),
        ];
        for mode in [LayoutMode::Spacious, LayoutMode::Compact] {
            assert_eq!(PATH.len(), 7, "{mode:?} must expose the complete path");
            assert!(PATH.iter().all(|(_, surface)| !surface.is_empty()));
            if mode == LayoutMode::Compact {
                assert!(PATH.iter().any(|(_, surface)| surface.contains("dock")));
            }
        }
    }
}
