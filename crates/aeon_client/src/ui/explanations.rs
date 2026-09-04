//! Focusable, pinnable explanations for consequential interface meaning.
//!
//! An explanation owns a snapshot of the text and authoritative forecast it
//! was opened from.  It is presentation state only: changing selection,
//! responsive reflow, or even resolution of the originating Situation cannot
//! silently replace what the player was reading.

use aeon_sim::TextDb;
use aeon_sim::forecast::AssignmentForecast;
use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};

use crate::ui::forecast::draw_forecast_body;
use crate::ui::theme::UiTheme;

#[cfg(test)]
const EXPLANATION_TRIGGER: &str = "production-explanation-trigger";
#[cfg(test)]
const EXPLANATION_TRIGGER_ID: &str = "production-explanation-trigger-id";
#[cfg(test)]
const EXPLANATION_PREVIEW: &str = "production-explanation-preview";
#[cfg(test)]
const PINNED_EXPLANATION: &str = "production-pinned-explanation";
#[cfg(test)]
const EXPLANATION_DISMISS: &str = "production-explanation-dismiss";

/// An immutable explanation captured at the point where it was requested.
///
/// A forecast-bearing topic explains a concrete action; a prose-only topic
/// (`forecast: None`) carries guidance or strategic meaning with no
/// simulation numbers attached.
#[derive(Clone, Debug)]
pub struct ExplanationTopic {
    pub title: String,
    /// A stable, non-identifying discriminator of *what* is being explained:
    /// an authored content key or a fixed interface role, prefixed by the
    /// kind of surface it was raised from. Never a display name, never
    /// authored prose, never anything the player typed — `title` is the
    /// player-facing copy and may carry a character's name, so measurement
    /// reads this instead.
    pub subject: String,
    pub summary: String,
    pub forecast: Option<AssignmentForecast>,
}

/// Client-only explanation state. It is neither saved nor sent to the sim.
#[derive(Resource, Default)]
pub struct ExplanationState {
    pub pinned: Option<ExplanationTopic>,
    /// Stable control that pinned the snapshot, used for close/Escape return.
    pub invoker: Option<crate::ui::keyboard::LogicalFocus>,
    /// The current physical Escape press belongs to pinned help. Update
    /// systems consult this before applying any lower-priority navigation.
    escape_claimed: bool,
}

impl ExplanationState {
    /// Whether pinned help owns this frame's Escape press.
    pub fn escape_claimed(&self) -> bool {
        self.escape_claimed
    }

    fn close(&mut self, ctx: &egui::Context) {
        self.pinned = None;
        if let Some(invoker) = self.invoker.take() {
            crate::ui::keyboard::request_logical(ctx, invoker);
        }
    }
}

/// Claims physical Escape before map and other client navigation systems.
///
/// Bevy's keyboard input and egui's event stream describe the same press but
/// run in different schedules. This shared claim joins them: the Update
/// schedule closes help first and lower-priority handlers stand down; the
/// egui pass then consumes its copy of the event.
pub fn claim_escape_for_pinned_help(world: &mut World) {
    let pressed = world
        .resource::<ButtonInput<KeyCode>>()
        .just_pressed(KeyCode::Escape);
    world.resource_mut::<ExplanationState>().escape_claimed = false;
    if !pressed {
        return;
    }
    let invoker = {
        let mut state = world.resource_mut::<ExplanationState>();
        state.escape_claimed = state.pinned.take().is_some();
        state.escape_claimed.then(|| state.invoker.take()).flatten()
    };
    if let Some(invoker) = invoker {
        let mut query = world
            .query_filtered::<&mut bevy_egui::EguiContext, With<bevy_egui::PrimaryEguiContext>>();
        if let Ok(mut context) = query.single_mut(world) {
            crate::ui::keyboard::request_logical(context.get_mut(), invoker);
        }
    }
}

/// Consumes the egui copy of an already-claimed Escape before any floating
/// campaign surface is allowed to interpret it.
pub fn consume_claimed_escape(mut contexts: EguiContexts, mut state: ResMut<ExplanationState>) {
    if !state.escape_claimed {
        return;
    }
    if let Ok(ctx) = contexts.ctx_mut() {
        ctx.input_mut(|input| {
            input.consume_key(egui::Modifiers::NONE, egui::Key::Escape);
        });
    }
    state.escape_claimed = false;
}

#[cfg(test)]
pub(crate) fn recorded_explanation_trigger(ctx: &egui::Context) -> Option<egui::Rect> {
    ctx.data(|data| data.get_temp(egui::Id::new(EXPLANATION_TRIGGER)))
}

#[cfg(test)]
pub(crate) fn recorded_explanation_trigger_id(ctx: &egui::Context) -> Option<egui::Id> {
    ctx.data(|data| data.get_temp(egui::Id::new(EXPLANATION_TRIGGER_ID)))
}

#[cfg(test)]
pub(crate) fn recorded_explanation_preview(ctx: &egui::Context) -> bool {
    ctx.data(|data| {
        data.get_temp(egui::Id::new(EXPLANATION_PREVIEW))
            .unwrap_or(false)
    })
}

#[cfg(test)]
pub(crate) fn recorded_pinned_explanation(ctx: &egui::Context) -> Option<egui::Rect> {
    ctx.data(|data| data.get_temp(egui::Id::new(PINNED_EXPLANATION)))
}

#[cfg(test)]
pub(crate) fn recorded_explanation_dismiss(ctx: &egui::Context) -> Option<egui::Rect> {
    ctx.data(|data| data.get_temp(egui::Id::new(EXPLANATION_DISMISS)))
}

#[cfg(test)]
pub(crate) fn clear_explanation_frame_evidence(ctx: &egui::Context) {
    ctx.data_mut(|data| {
        data.insert_temp(egui::Id::new(EXPLANATION_PREVIEW), false);
        data.remove::<egui::Rect>(egui::Id::new(EXPLANATION_TRIGGER));
        data.remove::<egui::Id>(egui::Id::new(EXPLANATION_TRIGGER_ID));
        data.remove::<egui::Rect>(egui::Id::new(PINNED_EXPLANATION));
        data.remove::<egui::Rect>(egui::Id::new(EXPLANATION_DISMISS));
    });
}

fn draw_topic(ui: &mut egui::Ui, theme: &UiTheme, strings: &TextDb, topic: &ExplanationTopic) {
    ui.strong(&topic.title);
    ui.add(egui::Label::new(&topic.summary).wrap());
    if let Some(forecast) = &topic.forecast {
        ui.separator();
        draw_forecast_body(ui, theme, strings, forecast);
    }
}

/// Gives an existing control identical hover and keyboard-focus preview.
pub fn preview_for_response(
    mut response: egui::Response,
    theme: &UiTheme,
    strings: &TextDb,
    topic: &ExplanationTopic,
) -> egui::Response {
    if response.hovered() {
        #[cfg(test)]
        response.ctx.data_mut(|data| {
            data.insert_temp(egui::Id::new(EXPLANATION_PREVIEW), true);
        });
        // Let egui own pointer-hover placement relative to the response. This
        // keeps the popup from covering or displacing the control that owns it.
        response = response.on_hover_ui(|ui| draw_topic(ui, theme, strings, topic));
    } else if response.has_focus() {
        #[cfg(test)]
        response.ctx.data_mut(|data| {
            data.insert_temp(egui::Id::new(EXPLANATION_PREVIEW), true);
        });
        // Keyboard focus has no pointer anchor, so it needs an explicit
        // response-anchored tooltip. The body is exactly the hover body above.
        egui::Tooltip::for_widget(&response)
            .width(response.ctx.global_style().spacing.tooltip_width)
            .show(|ui| draw_topic(ui, theme, strings, topic));
    }
    response
}

/// A visible, focusable route to the same preview; activation pins a snapshot.
pub fn explanation_trigger(
    ui: &mut egui::Ui,
    theme: &UiTheme,
    strings: &TextDb,
    topic: &ExplanationTopic,
    state: &mut ExplanationState,
    logical: crate::ui::keyboard::LogicalFocus,
    band: crate::ui::keyboard::FocusBand,
) -> egui::Response {
    labelled_explanation_trigger(
        ui,
        theme,
        strings,
        strings.text("ui.explanation.pin"),
        topic,
        state,
        logical,
        band,
    )
}

/// The same preview-and-pin control under a caller-supplied label, for
/// named help routes such as the guidance "Show me how" and "Why this
/// matters" triggers.
#[allow(clippy::too_many_arguments)]
pub fn labelled_explanation_trigger(
    ui: &mut egui::Ui,
    theme: &UiTheme,
    strings: &TextDb,
    label: &str,
    topic: &ExplanationTopic,
    state: &mut ExplanationState,
    logical: crate::ui::keyboard::LogicalFocus,
    band: crate::ui::keyboard::FocusBand,
) -> egui::Response {
    let response = ui.add(
        egui::Button::new(label)
            .min_size(egui::vec2(24.0, 24.0))
            .wrap(),
    );
    crate::ui::keyboard::capture_action(ui, logical.clone(), "explanation-pin", band, &response)
        .register();
    #[cfg(test)]
    crate::ui::rendered_state::record_response(ui, "explanation", &response);
    #[cfg(test)]
    ui.ctx().data_mut(|data| {
        data.insert_temp(egui::Id::new(EXPLANATION_TRIGGER), response.rect);
        data.insert_temp(egui::Id::new(EXPLANATION_TRIGGER_ID), response.id);
    });
    let response = preview_for_response(response, theme, strings, topic);
    if response.clicked() {
        state.pinned = Some(topic.clone());
        state.invoker = Some(logical);
        response.surrender_focus();
    }
    response
}

/// Draws the one pinned explanation above the rest of the campaign UI.
pub fn draw_pinned_explanation(
    mut contexts: EguiContexts,
    mut state: ResMut<ExplanationState>,
    theme: Res<UiTheme>,
    strings: Option<Res<TextDb>>,
) {
    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };
    let Some(strings) = strings else {
        return;
    };
    if state.pinned.is_some()
        && ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
    {
        state.pinned = None;
        return;
    }
    let Some(topic) = state.pinned.clone() else {
        return;
    };

    let viewport = ctx.viewport_rect().shrink(8.0);
    let width = (viewport.width() * 0.42)
        .clamp(180.0, 480.0)
        .min(viewport.width());
    let _shown = egui::Window::new(strings.text("ui.explanation.title"))
        .id(egui::Id::new("pinned-explanation"))
        .resizable(true)
        .default_width(width)
        .max_width(viewport.width())
        .max_height(viewport.height())
        .constrain_to(viewport)
        .show(ctx, |ui| {
            ui.set_max_width(width);
            egui::ScrollArea::vertical()
                .id_salt("pinned-explanation-scroll")
                .auto_shrink([false, true])
                .max_height((viewport.height() - 120.0).clamp(120.0, 360.0))
                .show(ui, |ui| {
                    draw_topic(ui, &theme, &strings, &topic);
                });
            ui.separator();
            let dismiss = ui.add(
                egui::Button::new(strings.text("ui.explanation.dismiss"))
                    .min_size(egui::vec2(24.0, 24.0))
                    .wrap(),
            );
            crate::ui::keyboard::capture_action(
                ui,
                crate::ui::keyboard::LogicalFocus::new("explanation-dismiss"),
                "explanation-dismiss",
                crate::ui::keyboard::FocusBand::Floating,
                &dismiss,
            )
            .register();
            #[cfg(test)]
            {
                crate::ui::rendered_state::record_response(ui, "explanation", &dismiss);
                ui.ctx().data_mut(|data| {
                    data.insert_temp(egui::Id::new(EXPLANATION_DISMISS), dismiss.rect);
                });
            }
            if dismiss.clicked() {
                state.close(ui.ctx());
            }
        });
    #[cfg(test)]
    if let Some(shown) = &_shown {
        ctx.data_mut(|data| {
            data.insert_temp(egui::Id::new(PINNED_EXPLANATION), shown.response.rect);
        });
    }
}
