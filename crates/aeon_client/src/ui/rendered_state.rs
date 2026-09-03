//! Native and browser rendered-frame acceptance tests for the production shell.

use aeon_sim::TextDb;
use bevy_egui::egui;

use crate::ui::dock::DockState;
#[cfg(test)]
use crate::ui::forecast::recorded_forecast_body;
use crate::ui::theme::UiTheme;

#[cfg(test)]
use crate::assignment_ui::flush_ui_commands;
#[cfg(test)]
use crate::assignment_ui::{AssignmentForm, LogFilter, UiCommandQueue};
#[cfg(test)]
use crate::forecast_view::{AvailabilityView, ForecastCache};
#[cfg(test)]
use crate::map_modes::{AttentionItem, AttentionTarget, MapReadout};
#[cfg(test)]
use crate::offer_view::OfferView;
#[cfg(test)]
use crate::preferences::{SettingsUi, UiPreferences};
#[cfg(test)]
use crate::sim_driver::TimeControl;
#[cfg(test)]
use crate::sim_driver::advance_for_elapsed;
#[cfg(test)]
use crate::ui::actions::recorded_confirm;
#[cfg(test)]
use crate::ui::assignment_popup::AssignmentPopup;
#[cfg(test)]
use crate::ui::explanations::{
    ExplanationState, recorded_explanation_dismiss, recorded_explanation_preview,
    recorded_explanation_trigger, recorded_explanation_trigger_id, recorded_pinned_explanation,
};
#[cfg(test)]
use crate::ui::picker::PickerState;
#[cfg(test)]
use crate::ui::situations_panel::{
    SituationPanelView, SituationUiState, recorded_situation_action,
    recorded_situation_action_hovered, recorded_situation_forecast, refresh_situation_panel_view,
};
#[cfg(test)]
use crate::ui::top_bar::recorded_time_control;
#[cfg(test)]
use crate::ui::widgets::recorded_link;
#[cfg(test)]
use crate::view::{MapMode, MapView, SearchState, ViewState};
#[cfg(test)]
use aeon_core::calendar::CalendarDate;
#[cfg(test)]
use aeon_sim::assignments::CharacterCondition;
#[cfg(test)]
use aeon_sim::command::PlayerCommand;
#[cfg(test)]
use aeon_sim::config::CampaignConfig;
#[cfg(test)]
use aeon_sim::host::SimHost;
#[cfg(test)]
use aeon_sim::politics::PlayerHouse;
#[cfg(test)]
use aeon_sim::situations::{SituationSubject, active_cards};
#[cfg(test)]
use aeon_sim::{CampaignClock, LeaderAvailability, PoliticsIndex};
#[cfg(test)]
use bevy::prelude::{Assets, ButtonInput, Image, IntoScheduleConfigs, KeyCode, Schedule};
#[cfg(test)]
use bevy::window::PrimaryWindow;
#[cfg(test)]
use bevy_egui::{EguiContext, EguiUserTextures, PrimaryEguiContext};
#[cfg(test)]
use std::sync::Arc;

/// One physical viewport and whole-interface scale from the acceptance matrix.
#[derive(Copy, Clone, Debug)]
pub struct DisplaySpec {
    pub width_px: f32,
    pub height_px: f32,
    pub scale: f32,
}

#[derive(Clone, Debug)]
pub(crate) struct SemanticResponse {
    pub role: &'static str,
    pub rect: egui::Rect,
    pub clip: egui::Rect,
}

#[derive(Clone, Debug)]
pub(crate) struct ScrollGeometry {
    pub role: &'static str,
    pub inner: egui::Rect,
    pub content: egui::Vec2,
}

const SEMANTIC_RESPONSES: &str = "production-semantic-responses";
const SCROLL_GEOMETRY: &str = "production-scroll-geometry";

pub(crate) fn clear_frame_evidence(ctx: &egui::Context) {
    crate::ui::explanations::clear_explanation_frame_evidence(ctx);
    crate::ui::forecast::clear_forecast_frame_evidence(ctx);
    crate::ui::situations_panel::clear_situation_frame_evidence(ctx);
    ctx.data_mut(|data| {
        data.insert_temp(
            egui::Id::new(SEMANTIC_RESPONSES),
            Vec::<SemanticResponse>::new(),
        );
        data.insert_temp(egui::Id::new(SCROLL_GEOMETRY), Vec::<ScrollGeometry>::new());
    });
}

pub(crate) fn record_response(ui: &egui::Ui, role: &'static str, response: &egui::Response) {
    ui.ctx().data_mut(|data| {
        let id = egui::Id::new(SEMANTIC_RESPONSES);
        let mut entries = data
            .get_temp::<Vec<SemanticResponse>>(id)
            .unwrap_or_default();
        entries.push(SemanticResponse {
            role,
            rect: response.rect,
            clip: ui.clip_rect(),
        });
        data.insert_temp(id, entries);
    });
}

pub(crate) fn record_scroll(
    ctx: &egui::Context,
    role: &'static str,
    inner: egui::Rect,
    content: egui::Vec2,
) {
    ctx.data_mut(|data| {
        let id = egui::Id::new(SCROLL_GEOMETRY);
        let mut entries = data.get_temp::<Vec<ScrollGeometry>>(id).unwrap_or_default();
        entries.push(ScrollGeometry {
            role,
            inner,
            content,
        });
        data.insert_temp(id, entries);
    });
}

pub(crate) fn semantic_responses(ctx: &egui::Context) -> Vec<SemanticResponse> {
    ctx.data(|data| {
        data.get_temp(egui::Id::new(SEMANTIC_RESPONSES))
            .unwrap_or_default()
    })
}

pub(crate) fn scroll_geometry(ctx: &egui::Context) -> Vec<ScrollGeometry> {
    ctx.data(|data| {
        data.get_temp(egui::Id::new(SCROLL_GEOMETRY))
            .unwrap_or_default()
    })
}

fn relative_luminance(colour: egui::Color32) -> f32 {
    fn channel(value: u8) -> f32 {
        let value = f32::from(value) / 255.0;
        if value <= 0.04045 {
            value / 12.92
        } else {
            ((value + 0.055) / 1.055).powf(2.4)
        }
    }
    0.2126 * channel(colour.r()) + 0.7152 * channel(colour.g()) + 0.0722 * channel(colour.b())
}

fn contrast(a: egui::Color32, b: egui::Color32) -> f32 {
    let a = relative_luminance(a);
    let b = relative_luminance(b);
    let (light, dark) = if a >= b { (a, b) } else { (b, a) };
    (light + 0.05) / (dark + 0.05)
}

fn over(foreground: egui::Color32, background: egui::Color32) -> egui::Color32 {
    let alpha = f32::from(foreground.a()) / 255.0;
    let blend = |front: u8, back: u8| {
        (f32::from(front) * alpha + f32::from(back) * (1.0 - alpha)).round() as u8
    };
    egui::Color32::from_rgb(
        blend(foreground.r(), background.r()),
        blend(foreground.g(), background.g()),
        blend(foreground.b(), background.b()),
    )
}

fn resolved_visuals_contrast_passes(visuals: &egui::Visuals, theme: &UiTheme) -> bool {
    let panel = over(visuals.panel_fill, visuals.extreme_bg_color);
    let window = over(visuals.window_fill, visuals.extreme_bg_color);
    let text_colours = [
        visuals.text_color(),
        visuals.weak_text_color(),
        visuals.strong_text_color(),
        visuals.hyperlink_color,
        visuals.widgets.noninteractive.fg_stroke.color,
        visuals.widgets.inactive.fg_stroke.color,
        visuals.widgets.hovered.fg_stroke.color,
        visuals.widgets.active.fg_stroke.color,
        visuals.widgets.open.fg_stroke.color,
        theme.semantics.valid.into(),
        theme.semantics.ineligible_fixable.into(),
        theme.semantics.urgent.into(),
    ];
    let boundaries = [
        visuals.widgets.inactive.bg_stroke.color,
        visuals.widgets.hovered.bg_stroke.color,
        visuals.widgets.active.bg_stroke.color,
        visuals.widgets.open.bg_stroke.color,
        visuals.selection.bg_fill,
    ];
    text_colours.iter().all(|foreground| {
        contrast(*foreground, panel) >= 4.5 && contrast(*foreground, window) >= 4.5
    }) && boundaries
        .iter()
        .all(|boundary| contrast(*boundary, panel) >= 3.0 && contrast(*boundary, window) >= 3.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(target_arch = "wasm32")]
    wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_browser);

    struct ProductionFixture {
        host: SimHost,
        time: f64,
    }

    impl ProductionFixture {
        fn new() -> Self {
            let content = crate::content::load_embedded();
            let scenario = content.scenario.clone().expect("embedded scenario");
            let start_date = CalendarDate {
                year: scenario.start_year,
                month: scenario.start_month,
                day: scenario.start_day,
            }
            .to_date()
            .expect("scenario date");
            let mut host = SimHost::new_with_content(
                CampaignConfig {
                    name: scenario.name,
                    seed: 201,
                    start_date,
                },
                Arc::clone(&content),
            );
            let creditor = active_cards(host.world_mut())
                .into_iter()
                .find(|card| card.active.key.definition.as_str() == "favour-debt")
                .and_then(|card| match card.active.key.bindings.get("creditor") {
                    Some(SituationSubject::Organisation(org)) => Some(*org),
                    _ => None,
                })
                .expect("opening creditor");
            host.world_mut().resource_mut::<PlayerHouse>().0 = Some(creditor);
            // The day-one Court Awaits announcement popup addresses the
            // scenario protagonist; this fixture pins the creditor's view,
            // so the popup would only float over the geometry under test.
            host.world_mut()
                .resource_mut::<aeon_sim::PendingPopups>()
                .popups
                .clear();
            // Keep one genuine household candidate unavailable so the real
            // picker exercises a mixed enabled/disabled row. This is
            // simulation state, not a presentation-only fake response.
            let blocked = {
                let world = host.world_mut();
                let date = world.resource::<CampaignClock>().date;
                world
                    .resource::<PoliticsIndex>()
                    .characters
                    .iter()
                    .find_map(|(id, entity)| {
                        matches!(
                            aeon_sim::leader_availability(world, creditor, *id, date),
                            LeaderAvailability::Available
                        )
                        .then_some((*entity, date.add_days(30)))
                    })
                    .expect("an available household leader for disabled picker evidence")
            };
            host.world_mut()
                .entity_mut(blocked.0)
                .insert(CharacterCondition {
                    injured_until: Some(blocked.1),
                    ..Default::default()
                });
            let world = host.world_mut();
            world.insert_resource(UiTheme::embedded());
            world.insert_resource(AvailabilityView::default());
            world.insert_resource(OfferView::default());
            world.insert_resource(ForecastCache::default());
            world.insert_resource(MapReadout::default());
            world.insert_resource(SituationPanelView::default());
            world.insert_resource(TextDb::embedded());
            world.insert_resource(TimeControl::default());
            world.insert_resource(UiCommandQueue::default());
            world.insert_resource(AssignmentForm::default());
            world.insert_resource(ViewState::default());
            world.insert_resource(SearchState::default());
            world.insert_resource(MapMode::default());
            world.insert_resource(DockState::default());
            world.insert_resource(SituationUiState::default());
            world.insert_resource(UiPreferences::default());
            world.insert_resource(SettingsUi::default());
            world.insert_resource(AssignmentPopup::default());
            world.insert_resource(LogFilter::default());
            world.insert_resource(PickerState::default());
            world.insert_resource(crate::ui::shell::LocalEscapeClaim::default());
            world.insert_resource(ExplanationState::default());
            world.insert_resource(ButtonInput::<KeyCode>::default());
            world.insert_resource(Assets::<Image>::default());
            world.insert_resource(EguiUserTextures::default());
            world.spawn((EguiContext::default(), PrimaryEguiContext, PrimaryWindow));
            refresh_situation_panel_view(world);
            Self { host, time: 0.0 }
        }

        fn prepare_full_shell(&mut self) {
            let card = active_cards(self.host.world_mut())
                .into_iter()
                .find(|card| card.active.key.definition.as_str() == "favour-debt")
                .expect("opening Situation card");
            let situation = card.active.key.clone();
            let world = self.host.world_mut();
            world.resource_mut::<SearchState>().query = "Veyrin".to_owned();
            let body = *world
                .resource::<aeon_sim::map::MapIndex>()
                .bodies
                .keys()
                .next()
                .expect("opening body");
            world.resource_mut::<ViewState>().view = MapView::Body(body);
            world
                .resource_mut::<MapReadout>()
                .attention
                .push(AttentionItem {
                    target: AttentionTarget::Situation(situation),
                    headline: "Attention overlay evidence".to_owned(),
                    detail: "A production overlay item in the shared rendered matrix.".to_owned(),
                    urgent: true,
                });
        }

        fn render_full_shell(
            &mut self,
            viewport: egui::Vec2,
            events: Vec<egui::Event>,
        ) -> egui::FullOutput {
            self.time += 0.6;
            let world = self.host.world_mut();
            let physical_escape = events.iter().any(|event| {
                matches!(
                    event,
                    egui::Event::Key {
                        key: egui::Key::Escape,
                        pressed: true,
                        ..
                    }
                )
            });
            refresh_situation_panel_view(world);
            {
                let mut keys = world.resource_mut::<ButtonInput<KeyCode>>();
                keys.reset_all();
                if physical_escape {
                    keys.press(KeyCode::Escape);
                }
            }
            // Exercise the same ordered production Escape route as main's
            // Update schedule before rendering the egui pass.
            let mut hotkey_schedule = Schedule::default();
            hotkey_schedule.add_systems(
                (
                    crate::ui::explanations::claim_escape_for_pinned_help,
                    crate::ui::shell::claim_local_escape,
                    crate::selection::view_hotkeys,
                )
                    .chain(),
            );
            hotkey_schedule.run(world);
            let ctx = {
                let mut query = world
                    .query_filtered::<&mut EguiContext, bevy::prelude::With<PrimaryEguiContext>>();
                query
                    .single_mut(world)
                    .expect("one primary egui context")
                    .get_mut()
                    .clone()
            };
            clear_frame_evidence(&ctx);
            ctx.begin_pass(egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, viewport)),
                events,
                time: Some(self.time),
                ..Default::default()
            });
            let mut schedule = Schedule::default();
            schedule.add_systems(
                (
                    crate::ui::theme::apply_theme,
                    crate::forecast_view::refresh_forecast,
                    crate::ui::explanations::consume_claimed_escape,
                    crate::map_overlay::draw_map_overlay,
                    crate::ui::shell::draw_panels,
                    crate::ui::assignment_popup::draw_assignment_popup,
                    crate::ui::picker::draw_picker,
                    crate::assignment_ui::draw_popups,
                    crate::ui::explanations::draw_pinned_explanation,
                    crate::ui::keyboard::finish_frame,
                )
                    .chain(),
            );
            schedule.run(world);
            ctx.end_pass()
        }

        fn full_egui_context(&mut self) -> egui::Context {
            let world = self.host.world_mut();
            let mut query =
                world.query_filtered::<&mut EguiContext, bevy::prelude::With<PrimaryEguiContext>>();
            query
                .single_mut(world)
                .expect("one primary egui context")
                .get_mut()
                .clone()
        }

        fn submit_queued_and_resolve(&mut self) {
            let command = self
                .host
                .world_mut()
                .resource::<UiCommandQueue>()
                .0
                .last()
                .cloned()
                .expect("production action queued a command");
            let situation = match &command {
                PlayerCommand::StartSituationAssignment { situation, .. } => situation.clone(),
                other => panic!("unexpected UI command: {other:?}"),
            };
            flush_ui_commands(self.host.world_mut());
            assert!(
                self.host
                    .world_mut()
                    .resource::<aeon_sim::command::PendingCommands>()
                    .entries()
                    .iter()
                    .any(|envelope| envelope.command == command)
            );

            let mut resolved = false;
            for _ in 0..120 {
                assert_eq!(advance_for_elapsed(self.host.world_mut(), 1.0), 1);
                let world = self.host.world_mut();
                resolved = world
                    .resource::<aeon_sim::situations::SituationState>()
                    .resolutions
                    .iter()
                    .any(|notice| notice.situation == situation);
                if resolved {
                    break;
                }
            }
            assert!(
                resolved,
                "authored assignment naturally resolves its Situation"
            );
            let applied = self
                .host
                .world_mut()
                .resource::<aeon_sim::command::CommandLog>()
                .applied
                .iter()
                .find(|envelope| envelope.command == command)
                .expect("UI command applied on the normal clock tick");
            assert_eq!(applied.command, command);
            refresh_situation_panel_view(self.host.world_mut());
            let view = self.host.world_mut().resource::<SituationPanelView>();
            assert!(
                !view.resolutions.is_empty(),
                "authoritative resolution projected"
            );
            assert!(
                view.resolutions
                    .iter()
                    .any(|resolution| !resolution.history.is_empty()),
                "tagged history projected"
            );
            assert!(
                view.resolutions.iter().any(|resolution| {
                    resolution.resolution.situation == situation
                        && resolution.history.iter().any(|entry| {
                            entry.channel == aeon_sim::LogChannel::Assignments
                                && entry
                                    .situations
                                    .contains(&resolution.resolution.occurrence())
                        })
                }),
                "natural assignment outcome is logged and tagged to the resolution"
            );
        }
    }

    fn materially_visible_text(
        output: &egui::FullOutput,
        viewport: egui::Rect,
        needle: &str,
    ) -> Option<(egui::Rect, egui::Rect)> {
        fn find(
            shape: &egui::Shape,
            clip: egui::Rect,
            viewport: egui::Rect,
            needle: &str,
        ) -> Option<(egui::Rect, egui::Rect)> {
            match shape {
                egui::Shape::Text(text) if text.galley.job.text.contains(needle) => {
                    let bounds = text.visual_bounding_rect();
                    let visible = bounds.intersect(clip).intersect(viewport);
                    let area = bounds.width().max(0.0) * bounds.height().max(0.0);
                    let visible_area = visible.width().max(0.0) * visible.height().max(0.0);
                    (visible.width() >= 1.0
                        && visible.height() >= 1.0
                        && visible_area >= area * 0.5)
                        .then_some((bounds, clip))
                }
                egui::Shape::Vec(shapes) => shapes
                    .iter()
                    .find_map(|shape| find(shape, clip, viewport, needle)),
                _ => None,
            }
        }
        output
            .shapes
            .iter()
            .find_map(|shape| find(&shape.shape, shape.clip_rect, viewport, needle))
    }

    fn painted_texts(output: &egui::FullOutput) -> Vec<String> {
        output
            .shapes
            .iter()
            .filter_map(|clipped| match &clipped.shape {
                egui::Shape::Text(text) => Some(text.galley.text().to_owned()),
                _ => None,
            })
            .collect()
    }

    fn horizontal_clip_failures(output: &egui::FullOutput) -> Vec<String> {
        fn collect(shape: &egui::Shape, clip: egui::Rect, failures: &mut Vec<String>) {
            match shape {
                egui::Shape::Text(text) => {
                    let bounds = text.visual_bounding_rect();
                    if bounds.min.x < clip.min.x - 1.0 || bounds.max.x > clip.max.x + 1.0 {
                        failures.push(format!(
                            "{:?} bounds={bounds:?} clip={clip:?}",
                            text.galley.job.text
                        ));
                    }
                }
                egui::Shape::Vec(shapes) => {
                    for shape in shapes {
                        collect(shape, clip, failures);
                    }
                }
                _ => {}
            }
        }
        let mut failures = Vec::new();
        for shape in &output.shapes {
            collect(&shape.shape, shape.clip_rect, &mut failures);
        }
        failures
    }

    fn assert_response_roles(
        ctx: &egui::Context,
        viewport: egui::Rect,
        spec: DisplaySpec,
        roles: &[&'static str],
    ) {
        let contains_rect = |outer: egui::Rect, inner: egui::Rect| {
            inner.min.x >= outer.min.x - 0.5
                && inner.max.x <= outer.max.x + 0.5
                && inner.min.y >= outer.min.y - 0.5
                && inner.max.y <= outer.max.y + 0.5
        };
        let responses = semantic_responses(ctx);
        for role in roles {
            let matching: Vec<_> = responses
                .iter()
                .filter(|entry| entry.role == *role)
                .collect();
            assert!(
                !matching.is_empty(),
                "production response '{role}' at {spec:?}"
            );
            assert!(
                matching.iter().all(|entry| {
                    contains_rect(entry.clip, entry.rect)
                        && contains_rect(viewport, entry.rect)
                        && entry.rect.width() >= 24.0
                        && entry.rect.height() >= 24.0
                }),
                "fully contained raw >=24 production responses '{role}' at {spec:?}: {matching:#?}"
            );
        }
    }

    fn visible_response_center(
        ctx: &egui::Context,
        viewport: egui::Rect,
        role: &'static str,
    ) -> egui::Pos2 {
        semantic_responses(ctx)
            .into_iter()
            .rev()
            .find_map(|response| {
                if response.role != role {
                    return None;
                }
                let visible = response.rect.intersect(response.clip).intersect(viewport);
                (visible.width() >= 24.0 && visible.height() >= 24.0).then_some(visible.center())
            })
            .unwrap_or_else(|| panic!("visible raw production response for {role}"))
    }

    fn assert_vertical_only_scroll(ctx: &egui::Context, spec: DisplaySpec) -> bool {
        let scrolls = scroll_geometry(ctx);
        assert!(!scrolls.is_empty(), "production scroll areas at {spec:?}");
        assert!(
            scrolls
                .iter()
                .all(|scroll| scroll.content.x <= scroll.inner.width() + 2.0),
            "no production scroll requires horizontal movement at {spec:?}: {scrolls:#?}"
        );
        assert!(
            scrolls.iter().all(|scroll| !scroll.role.is_empty()),
            "production scroll roles at {spec:?}"
        );
        scrolls
            .iter()
            .any(|scroll| scroll.content.y > scroll.inner.height() + 1.0)
    }

    fn press_at(pos: egui::Pos2) -> Vec<egui::Event> {
        vec![
            egui::Event::PointerMoved(pos),
            egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: egui::Modifiers::NONE,
            },
        ]
    }

    fn release_at(pos: egui::Pos2) -> Vec<egui::Event> {
        vec![
            egui::Event::PointerMoved(pos),
            egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            },
        ]
    }

    fn click_at(pos: egui::Pos2) -> Vec<egui::Event> {
        let mut events = press_at(pos);
        events.extend(release_at(pos));
        events
    }

    fn key_event(key: egui::Key, shift: bool) -> Vec<egui::Event> {
        vec![egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers {
                shift,
                ..Default::default()
            },
        }]
    }

    fn registry_focus(ctx: &egui::Context) -> Option<crate::ui::keyboard::FocusEntry> {
        let logical = crate::ui::keyboard::logical_focus(ctx)?;
        let entry = crate::ui::keyboard::completed_registry(ctx)
            .into_iter()
            .find(|entry| entry.logical == logical)?;
        (ctx.memory(|memory| memory.focused()) == Some(entry.id)).then_some(entry)
    }

    fn tab_until(
        fixture: &mut ProductionFixture,
        viewport: egui::Vec2,
        role: &str,
        backwards: bool,
    ) -> (egui::FullOutput, Vec<String>) {
        let mut visited = Vec::new();
        for _ in 0..512 {
            let ctx = fixture.full_egui_context();
            let expected_sequence = independent_visual_sequence(&ctx);
            let current = crate::ui::keyboard::logical_focus(&ctx);
            let current_index = current.as_ref().and_then(|logical| {
                expected_sequence
                    .iter()
                    .position(|entry| &entry.logical == logical)
            });
            let current_is_floating = current_index.is_some_and(|index| {
                expected_sequence[index].band == crate::ui::keyboard::FocusBand::Floating
            });
            let mut floating = expected_sequence
                .iter()
                .enumerate()
                .filter(|(_, entry)| entry.band == crate::ui::keyboard::FocusBand::Floating)
                .map(|(index, _)| index);
            let next = if !current_is_floating {
                if backwards {
                    floating.next_back()
                } else {
                    floating.next()
                }
            } else {
                None
            }
            .unwrap_or_else(|| match (current_index, backwards) {
                (Some(index), true) => {
                    (index + expected_sequence.len() - 1) % expected_sequence.len()
                }
                (Some(index), false) => (index + 1) % expected_sequence.len(),
                (None, true) => expected_sequence.len() - 1,
                (None, false) => 0,
            });
            let expected_target = expected_sequence[next].logical.clone();
            fixture.render_full_shell(viewport, key_event(egui::Key::Tab, backwards));
            for _ in 0..12 {
                fixture.render_full_shell(viewport, Vec::new());
                if let Some(entry) = registry_focus(&fixture.full_egui_context()) {
                    assert_eq!(
                        entry.logical,
                        expected_target,
                        "exact independent {} Tab target; sequence={:?}",
                        if backwards { "reverse" } else { "forward" },
                        expected_sequence
                            .iter()
                            .map(|entry| &entry.logical)
                            .collect::<Vec<_>>()
                    );
                    visited.push(entry.logical.0.clone());
                    if entry.role == role {
                        let viewport_rect = egui::Rect::from_min_size(egui::Pos2::ZERO, viewport);
                        for _ in 0..8 {
                            let painted = fixture.render_full_shell(viewport, Vec::new());
                            let settled = registry_focus(&fixture.full_egui_context())
                                .expect("target remains focused for paint");
                            assert_eq!(settled.logical, entry.logical);
                            if settled.clip.contains_rect(settled.rect)
                                && viewport_rect.contains_rect(settled.rect)
                            {
                                return (painted, visited);
                            }
                        }
                        panic!("focused target did not scroll fully into view: {entry:?}");
                    }
                    break;
                }
            }
        }
        panic!("Tab traversal did not reach role {role}; visited={visited:?}");
    }

    fn independent_visual_sequence(
        ctx: &egui::Context,
    ) -> Vec<crate::ui::keyboard::AuditedResponse> {
        let mut audited = crate::ui::keyboard::audited_responses(ctx);
        audited.retain(|entry| entry.enabled);
        audited.sort_by(|left, right| {
            left.band
                .cmp(&right.band)
                .then_with(|| (left.layer.order as u8).cmp(&(right.layer.order as u8)))
                .then_with(|| left.rect.top().round().total_cmp(&right.rect.top().round()))
                .then_with(|| left.rect.left().total_cmp(&right.rect.left()))
                .then_with(|| left.rect.bottom().total_cmp(&right.rect.bottom()))
                .then_with(|| left.role.cmp(right.role))
                .then_with(|| left.logical.cmp(&right.logical))
        });
        audited
    }

    fn independent_adjacent(
        ctx: &egui::Context,
        backwards: bool,
    ) -> crate::ui::keyboard::LogicalFocus {
        let sequence = independent_visual_sequence(ctx);
        let current = crate::ui::keyboard::logical_focus(ctx).expect("logical focus before Tab");
        let index = sequence
            .iter()
            .position(|entry| entry.logical == current)
            .expect("focused logical ID in independent response oracle");
        let next = if backwards {
            (index + sequence.len() - 1) % sequence.len()
        } else {
            (index + 1) % sequence.len()
        };
        sequence[next].logical.clone()
    }

    fn role_sequence(ctx: &egui::Context, role: &str) -> Vec<crate::ui::keyboard::LogicalFocus> {
        independent_visual_sequence(ctx)
            .into_iter()
            .filter(|entry| entry.enabled && entry.role == role)
            .map(|entry| entry.logical)
            .collect()
    }

    fn draw_duplicate_explanation_controls(
        mut contexts: bevy_egui::EguiContexts,
        mut explanations: bevy::prelude::ResMut<ExplanationState>,
        theme: bevy::prelude::Res<UiTheme>,
        strings: bevy::prelude::Res<TextDb>,
        situations: bevy::prelude::Res<SituationPanelView>,
    ) {
        let Ok(ctx) = contexts.ctx_mut() else {
            return;
        };
        crate::ui::keyboard::begin_frame(ctx);
        let forecast = situations
            .active
            .iter()
            .flat_map(|card| &card.actions)
            .find_map(|action| action.forecast.clone())
            .expect("fixture has a consequential Situation forecast");
        let topic = crate::ui::explanations::ExplanationTopic {
            // Deliberately identical display copy: semantic caller identity,
            // never this title, must distinguish the controls.
            title: "The same displayed title".to_owned(),
            summary: crate::ui::forecast::forecast_summary(&strings, &forecast),
            forecast: Some(forecast),
        };
        egui::Area::new(egui::Id::new("duplicate-explanation-fixture")).show(ctx, |ui| {
            for logical in ["duplicate-source:first", "duplicate-source:second"] {
                crate::ui::explanations::explanation_trigger(
                    ui,
                    &theme,
                    &strings,
                    &topic,
                    &mut explanations,
                    crate::ui::keyboard::LogicalFocus::new(logical),
                    crate::ui::keyboard::FocusBand::Center,
                );
            }
        });
    }

    fn render_duplicate_explanations(
        fixture: &mut ProductionFixture,
        viewport: egui::Vec2,
        events: Vec<egui::Event>,
    ) -> egui::FullOutput {
        fixture.time += 0.6;
        let world = fixture.host.world_mut();
        let physical_escape = events.iter().any(|event| {
            matches!(
                event,
                egui::Event::Key {
                    key: egui::Key::Escape,
                    pressed: true,
                    ..
                }
            )
        });
        {
            let mut keys = world.resource_mut::<ButtonInput<KeyCode>>();
            keys.reset_all();
            if physical_escape {
                keys.press(KeyCode::Escape);
            }
        }
        let mut hotkeys = Schedule::default();
        hotkeys.add_systems(crate::ui::explanations::claim_escape_for_pinned_help);
        hotkeys.run(world);
        let ctx = {
            let mut query =
                world.query_filtered::<&mut EguiContext, bevy::prelude::With<PrimaryEguiContext>>();
            query
                .single_mut(world)
                .expect("one primary egui context")
                .get_mut()
                .clone()
        };
        clear_frame_evidence(&ctx);
        ctx.begin_pass(egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, viewport)),
            events,
            time: Some(fixture.time),
            ..Default::default()
        });
        let mut schedule = Schedule::default();
        schedule.add_systems(
            (
                crate::ui::explanations::consume_claimed_escape,
                draw_duplicate_explanation_controls,
                crate::ui::explanations::draw_pinned_explanation,
                crate::ui::keyboard::finish_frame,
            )
                .chain(),
        );
        schedule.run(world);
        ctx.end_pass()
    }

    fn settle_registry_focus(
        fixture: &mut ProductionFixture,
        viewport: egui::Vec2,
    ) -> crate::ui::keyboard::FocusEntry {
        for _ in 0..8 {
            fixture.render_full_shell(viewport, Vec::new());
            if let Some(entry) = registry_focus(&fixture.full_egui_context()) {
                return entry;
            }
        }
        panic!("keyboard focus did not settle")
    }

    fn assert_real_group_wrap(
        fixture: &mut ProductionFixture,
        viewport: egui::Vec2,
        role: &str,
        spec: DisplaySpec,
    ) {
        let _ = tab_until(fixture, viewport, role, false);
        let before = registry_focus(&fixture.full_egui_context()).expect("group focus");
        let sequence = role_sequence(&fixture.full_egui_context(), role);
        assert!(sequence.len() >= 2, "real {role} group at {spec:?}");
        assert_eq!(before.logical, sequence[0], "Tab reaches first {role}");
        fixture.render_full_shell(viewport, key_event(egui::Key::ArrowLeft, false));
        let wrapped = settle_registry_focus(fixture, viewport);
        assert_eq!(
            wrapped.logical,
            *sequence.last().expect("group tail"),
            "ArrowLeft wraps real {role} row at {spec:?}"
        );
        fixture.render_full_shell(viewport, key_event(egui::Key::ArrowRight, false));
        let restored = settle_registry_focus(fixture, viewport);
        assert_eq!(
            restored.logical, sequence[0],
            "ArrowRight wraps real {role} row at {spec:?}"
        );
    }

    fn assert_registry_visual_order(ctx: &egui::Context, spec: DisplaySpec) {
        let entries = crate::ui::keyboard::completed_registry(ctx);
        let mut audited = crate::ui::keyboard::audited_responses(ctx);
        audited.retain(|entry| entry.enabled);
        assert!(!entries.is_empty(), "rendered focus registry at {spec:?}");
        assert!(
            crate::ui::keyboard::audit_gaps(ctx).is_empty(),
            "enabled production responses missing from registry at {spec:?}: {:?}",
            crate::ui::keyboard::audit_gaps(ctx)
        );
        audited.sort_by(|left, right| {
            left.band
                .cmp(&right.band)
                .then_with(|| (left.layer.order as u8).cmp(&(right.layer.order as u8)))
                .then_with(|| left.rect.top().round().total_cmp(&right.rect.top().round()))
                .then_with(|| left.rect.left().total_cmp(&right.rect.left()))
                .then_with(|| left.rect.bottom().total_cmp(&right.rect.bottom()))
                .then_with(|| left.role.cmp(right.role))
                .then_with(|| left.logical.cmp(&right.logical))
        });
        let expected = audited
            .iter()
            .map(|response| response.logical.0.as_str())
            .collect::<Vec<_>>();
        let actual = entries
            .iter()
            .map(|entry| entry.logical.0.as_str())
            .collect::<Vec<_>>();
        assert_eq!(
            actual, expected,
            "exact independent visual logical-ID sequence at {spec:?}"
        );
        for pair in entries.windows(2) {
            let [left, right] = pair else { unreachable!() };
            assert!(
                left.band <= right.band,
                "surface order at {spec:?}: {pair:?}"
            );
            if left.band == right.band && left.layer.order == right.layer.order {
                let rows_advance = left.rect.top().round() <= right.rect.top().round();
                let same_row_advances = left.rect.top().round() != right.rect.top().round()
                    || left.rect.left() <= right.rect.left();
                assert!(
                    rows_advance && same_row_advances,
                    "rectangle order at {spec:?}: {pair:?}"
                );
            }
        }
        let top = entries
            .iter()
            .filter(|entry| entry.band == crate::ui::keyboard::FocusBand::TopChrome)
            .collect::<Vec<_>>();
        assert!(
            top.windows(2).all(
                |pair| pair[0].rect.top().round() < pair[1].rect.top().round()
                    || pair[0].rect.left() <= pair[1].rect.left()
            ),
            "right-to-left top chrome is traversed visually LTR at {spec:?}: {top:?}"
        );
    }

    fn assert_focused_boundary(
        output: &egui::FullOutput,
        entry: &crate::ui::keyboard::FocusEntry,
        viewport: egui::Rect,
        spec: DisplaySpec,
    ) {
        assert!(
            entry.clip.contains_rect(entry.rect),
            "focus clip at {spec:?}: {entry:?}"
        );
        assert!(
            viewport.contains_rect(entry.rect),
            "focus viewport at {spec:?}: {entry:?}"
        );
        fn matching_focus(
            shape: &egui::Shape,
            rect: egui::Rect,
        ) -> Option<(egui::Rect, egui::Stroke)> {
            match shape {
                egui::Shape::Rect(shape)
                    if (shape.rect.min - rect.min).length_sq() <= 0.25
                        && (shape.rect.max - rect.max).length_sq() <= 0.25
                        && (shape.stroke.width - 2.0).abs() <= 0.01 =>
                {
                    Some((shape.rect, shape.stroke))
                }
                egui::Shape::Vec(shapes) => {
                    shapes.iter().find_map(|shape| matching_focus(shape, rect))
                }
                _ => None,
            }
        }
        let (painted_rect, observed_stroke, observed_clip) = output
            .shapes
            .iter()
            .find_map(|clipped| {
                matching_focus(&clipped.shape, entry.rect)
                    .map(|(rect, stroke)| (rect, stroke, clipped.clip_rect))
            })
            .unwrap_or_else(|| panic!("exact 2px focus paint at {spec:?}: {entry:?}"));
        assert!((observed_stroke.width - 2.0).abs() <= 0.01);
        assert!(observed_clip.contains_rect(painted_rect));
        assert!(viewport.contains_rect(painted_rect));

        fn painted_rects(shape: &egui::Shape, out: &mut Vec<(egui::Rect, egui::Color32)>) {
            match shape {
                egui::Shape::Rect(rect) if rect.fill.a() > 0 => out.push((rect.rect, rect.fill)),
                egui::Shape::Vec(shapes) => {
                    for shape in shapes {
                        painted_rects(shape, out);
                    }
                }
                _ => {}
            }
        }
        let mut surfaces = Vec::new();
        for clipped in &output.shapes {
            painted_rects(&clipped.shape, &mut surfaces);
        }
        let observed_surface = surfaces
            .into_iter()
            .filter(|(rect, _)| rect.contains(entry.rect.center()))
            .min_by(|(left, _), (right, _)| left.area().total_cmp(&right.area()))
            .map(|(_, fill)| fill)
            .expect("observed adjacent painted surface");

        fn luminance(colour: egui::Color32) -> f32 {
            fn channel(value: u8) -> f32 {
                let value = f32::from(value) / 255.0;
                if value <= 0.04045 {
                    value / 12.92
                } else {
                    ((value + 0.055) / 1.055).powf(2.4)
                }
            }
            0.2126 * channel(colour.r())
                + 0.7152 * channel(colour.g())
                + 0.0722 * channel(colour.b())
        }
        fn contrast(left: egui::Color32, right: egui::Color32) -> f32 {
            let (light, dark) = if luminance(left) >= luminance(right) {
                (luminance(left), luminance(right))
            } else {
                (luminance(right), luminance(left))
            };
            (light + 0.05) / (dark + 0.05)
        }
        assert!(
            contrast(observed_stroke.color, observed_surface) >= 3.0,
            "observed focus/surface contrast at {spec:?}: stroke={:?} surface={:?}",
            observed_stroke.color,
            observed_surface,
        );
    }

    const MATRIX: [DisplaySpec; 5] = [
        DisplaySpec {
            width_px: 1920.0,
            height_px: 1080.0,
            scale: 1.0,
        },
        DisplaySpec {
            width_px: 1920.0,
            height_px: 1080.0,
            scale: 1.5,
        },
        DisplaySpec {
            width_px: 1920.0,
            height_px: 1080.0,
            scale: 2.0,
        },
        DisplaySpec {
            width_px: 1366.0,
            height_px: 768.0,
            scale: 1.0,
        },
        DisplaySpec {
            width_px: 1366.0,
            height_px: 768.0,
            scale: 1.5,
        },
    ];

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn first_reign_guidance_is_optional_presentation_over_the_same_card() {
        let viewport = egui::vec2(1920.0, 1080.0);
        let mut fixture = ProductionFixture::new();
        {
            // Guidance addresses the scenario protagonist; pin that view.
            let world = fixture.host.world_mut();
            let harrow = world.resource::<PoliticsIndex>().org_keys
                [&aeon_data::ContentKey::new("harrow").unwrap()];
            world.resource_mut::<PlayerHouse>().0 = Some(harrow);
            refresh_situation_panel_view(world);
        }
        let hash_before = fixture.host.state_hash();

        // Guidance on (the default): the card renders its help triggers,
        // registered for keyboard traversal like any other control.
        fixture.render_full_shell(viewport, Vec::new());
        let ctx = fixture.full_egui_context();
        let guidance: Vec<_> = semantic_responses(&ctx)
            .into_iter()
            .filter(|entry| entry.role == "situation-guidance")
            .collect();
        assert_eq!(
            guidance.len(),
            2,
            "Show me how and Why this matters render with guidance enabled"
        );
        let registry = crate::ui::keyboard::completed_registry(&ctx);
        assert_eq!(
            registry
                .iter()
                .filter(|entry| entry.logical.0.starts_with("situation-guidance:"))
                .count(),
            2,
            "guidance triggers are keyboard-reachable"
        );

        // Guidance off: the same card renders without guidance, and neither
        // state queued a command or moved authoritative state.
        fixture
            .host
            .world_mut()
            .resource_mut::<UiPreferences>()
            .guidance = false;
        fixture.render_full_shell(viewport, Vec::new());
        let ctx = fixture.full_egui_context();
        assert!(
            semantic_responses(&ctx)
                .into_iter()
                .all(|entry| entry.role != "situation-guidance"),
            "disabling the preference removes guidance and nothing else"
        );
        assert!(
            fixture
                .host
                .world_mut()
                .resource::<UiCommandQueue>()
                .0
                .is_empty(),
            "guidance rendering emitted no commands"
        );
        assert_eq!(
            fixture.host.state_hash(),
            hash_before,
            "the guidance preference is presentation only"
        );
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn household_demand_responses_render_and_are_keyboard_reachable() {
        let viewport = egui::vec2(1920.0, 1080.0);
        let mut fixture = ProductionFixture::new();
        {
            // Open the household demands: the court's window lapses on day
            // seven and Kessarin's demand activates that same settled day.
            fixture.host.advance_days(7);
            let world = fixture.host.world_mut();
            let harrow = world.resource::<PoliticsIndex>().org_keys
                [&aeon_data::ContentKey::new("harrow").unwrap()];
            world.resource_mut::<PlayerHouse>().0 = Some(harrow);
            // Pending popups are authoritative state that would float over
            // the geometry under test; clear them like the fixture does.
            world
                .resource_mut::<aeon_sim::PendingPopups>()
                .popups
                .clear();
        }
        let hash_before = fixture.host.state_hash();

        // The pure responses render as focusable controls registered for
        // keyboard traversal like every other card action.
        fixture.render_full_shell(viewport, Vec::new());
        let ctx = fixture.full_egui_context();
        let responses: Vec<_> = semantic_responses(&ctx)
            .into_iter()
            .filter(|entry| entry.role == "situation-response")
            .collect();
        // Three household demands are open on day seven — Kessarin's Order,
        // Aleyn's Levies, and Torvald's Standing — and each renders its own
        // Promise and Refuse.
        assert_eq!(
            responses.len(),
            6,
            "all three demands' Promise and Refuse render as pure recorded choices"
        );
        let registry = crate::ui::keyboard::completed_registry(&ctx);
        assert_eq!(
            registry
                .iter()
                .filter(|entry| entry.logical.0.starts_with("situation-response:"))
                .count(),
            6,
            "response controls are keyboard-reachable"
        );

        // Rendering the choices records nothing: answering is an ordinary
        // queued command, never a presentation side effect.
        assert!(
            fixture
                .host
                .world_mut()
                .resource::<UiCommandQueue>()
                .0
                .is_empty(),
            "rendering the responses emitted no commands"
        );
        assert_eq!(
            fixture.host.state_hash(),
            hash_before,
            "the response surface is presentation only"
        );
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn a_leaderless_visit_action_is_enabled_and_the_host_choice_changes_the_odds() {
        let viewport = egui::vec2(1920.0, 1080.0);
        let mut fixture = ProductionFixture::new();
        let harrow = {
            let world = fixture.host.world_mut();
            let harrow = world.resource::<PoliticsIndex>().org_keys
                [&aeon_data::ContentKey::new("harrow").unwrap()];
            world.resource_mut::<PlayerHouse>().0 = Some(harrow);
            harrow
        };
        // The liege's visit opens in its authored deterministic window.
        fixture.host.advance_days(140);
        fixture
            .host
            .world_mut()
            .resource_mut::<aeon_sim::PendingPopups>()
            .popups
            .clear();
        refresh_situation_panel_view(fixture.host.world_mut());
        let head = aeon_sim::access::org_head(fixture.host.world_mut(), harrow)
            .expect("the house has a head");

        // The panel view: three tier actions, none pinning a leader, all
        // enabled, each previewing the authoritative forecast for the
        // deterministic default host — the house head — with genuinely
        // distinct odds per tier.
        let (visit_key, tier_chances) = {
            let view = fixture.host.world_mut().resource::<SituationPanelView>();
            let visit = view
                .active
                .iter()
                .find(|card| card.card.active.key.definition.as_str() == "casimir-visit")
                .expect("the visit projects for the player");
            let mut chances = std::collections::BTreeMap::new();
            for action in &visit.actions {
                assert_eq!(action.action.leader, None, "the host is a free choice");
                assert_eq!(
                    action.unavailable, None,
                    "a leaderless action is enabled, not blocked"
                );
                let forecast = action
                    .forecast
                    .as_ref()
                    .expect("the card previews the default host's forecast");
                assert_eq!(forecast.leader, head, "the preview host is the head");
                assert!(
                    forecast.opinion_value.is_some(),
                    "the preview reads the live relationship"
                );
                chances.insert(
                    action.action.id.as_str().to_owned(),
                    forecast.success_chance(),
                );
            }
            (visit.card.active.key.clone(), chances)
        };
        assert_eq!(tier_chances.len(), 3);
        assert!(
            tier_chances["host-lavish"] > tier_chances["host-restrained"],
            "the tiers quote genuinely different odds: {tier_chances:?}"
        );

        // Rendered, the three tier controls are real enabled focusables.
        fixture.render_full_shell(viewport, Vec::new());
        let ctx = fixture.full_egui_context();
        let visit_actions: Vec<_> = crate::ui::keyboard::audited_responses(&ctx)
            .into_iter()
            .filter(|entry| {
                entry.logical.0.starts_with("situation-action:")
                    && entry.logical.0.contains("casimir-visit")
            })
            .collect();
        assert_eq!(visit_actions.len(), 3, "three tier controls render");
        assert!(
            visit_actions.iter().all(|entry| entry.enabled),
            "every leaderless tier control is enabled"
        );

        // Keyboard activation opens the ordinary composition popup with
        // the default host prefilled and the free picker available.
        let lavish = visit_actions
            .iter()
            .find(|entry| entry.logical.0.ends_with(":host-lavish"))
            .expect("the lavish tier control");
        crate::ui::keyboard::request_logical(&ctx, lavish.logical.clone());
        fixture.render_full_shell(viewport, Vec::new());
        fixture.render_full_shell(viewport, key_event(egui::Key::Enter, false));
        fixture.render_full_shell(viewport, Vec::new());
        assert!(
            fixture.host.world_mut().resource::<AssignmentPopup>().open,
            "the leaderless action opens the assignment popup"
        );
        {
            let form = fixture.host.world_mut().resource::<AssignmentForm>();
            assert_eq!(
                form.assignment.as_ref().map(|key| key.as_str()),
                Some("host-visit-lavish")
            );
            assert_eq!(form.leader, Some(head), "the default host is prefilled");
        }

        // The popup's candidate list is the sim's own per-host forecast
        // comparison; choosing a different host changes the reported
        // chance to exactly that candidate's authoritative number.
        let before = {
            let cache = fixture.host.world_mut().resource::<ForecastCache>();
            cache
                .forecast
                .as_ref()
                .expect("the prefilled host has a forecast")
                .success_chance()
        };
        let (other, other_chance) = {
            let cache = fixture.host.world_mut().resource::<ForecastCache>();
            cache
                .leaders
                .iter()
                .find(|option| {
                    option.id != head && option.blocked().is_none() && option.success() != before
                })
                .map(|option| (option.id, option.success()))
                .expect("another free host with different odds exists")
        };
        fixture
            .host
            .world_mut()
            .resource_mut::<AssignmentForm>()
            .leader = Some(other);
        fixture.render_full_shell(viewport, Vec::new());
        let after = {
            let cache = fixture.host.world_mut().resource::<ForecastCache>();
            cache
                .forecast
                .as_ref()
                .expect("the chosen host has a forecast")
                .success_chance()
        };
        assert_eq!(after, other_chance, "the reported chance is the host's own");
        assert_ne!(after, before, "the host choice changed the odds");

        // Confirm queues the ordinary Situation command for the chosen
        // host, and the command round-trips into a running assignment and
        // a durable hosted resolution. The completed keyboard route ends
        // first, so its focus tooltip is not a top-layer hit target over
        // the popup.
        crate::ui::keyboard::clear_focus(&fixture.full_egui_context());
        fixture.render_full_shell(viewport, vec![egui::Event::PointerGone]);
        let confirm =
            recorded_confirm(&fixture.full_egui_context()).expect("the popup renders Confirm");
        fixture.render_full_shell(viewport, press_at(confirm.center()));
        let pressed = recorded_confirm(&fixture.full_egui_context())
            .expect("Confirm remains under the pointer");
        fixture.render_full_shell(viewport, release_at(pressed.center()));
        let queued = fixture
            .host
            .world_mut()
            .resource::<UiCommandQueue>()
            .0
            .last()
            .cloned()
            .expect("Confirm queued a command");
        match &queued {
            PlayerCommand::StartSituationAssignment {
                situation,
                action,
                leader,
                war,
                ..
            } => {
                assert_eq!(situation, &visit_key);
                assert_eq!(action.as_str(), "host-lavish");
                assert_eq!(*leader, other, "the chosen host leads");
                assert_eq!(*war, None);
            }
            other => panic!("unexpected UI command: {other:?}"),
        }
        flush_ui_commands(fixture.host.world_mut());
        fixture.host.advance_days(3);
        let running = {
            let world = fixture.host.world_mut();
            world
                .resource::<aeon_sim::AssignmentsIndex>()
                .assignments
                .values()
                .filter_map(|entity| world.get::<aeon_sim::ActiveAssignment>(*entity))
                .find(|work| work.def.as_str() == "host-visit-lavish")
                .cloned()
                .expect("the hosting assignment runs")
        };
        assert_eq!(running.leader, other);
        assert!(
            fixture
                .host
                .world_mut()
                .resource::<aeon_sim::situations::SituationState>()
                .resolutions
                .iter()
                .any(|notice| notice.situation == visit_key && notice.outcome.as_str() == "hosted"),
            "acceptance resolved the visit hosted"
        );
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn duplicate_explanation_titles_keep_distinct_invokers() {
        let viewport = egui::vec2(960.0, 720.0);
        let mut fixture = ProductionFixture::new();
        render_duplicate_explanations(&mut fixture, viewport, Vec::new());
        render_duplicate_explanations(&mut fixture, viewport, Vec::new());
        let expected = [
            crate::ui::keyboard::LogicalFocus::new("duplicate-source:first"),
            crate::ui::keyboard::LogicalFocus::new("duplicate-source:second"),
        ];
        let registry = crate::ui::keyboard::completed_registry(&fixture.full_egui_context());
        let pins = registry
            .iter()
            .filter(|entry| entry.role == "explanation-pin")
            .collect::<Vec<_>>();
        assert_eq!(pins.len(), 2, "both same-title Pin controls are registered");
        assert!(
            expected
                .iter()
                .all(|logical| pins.iter().any(|entry| &entry.logical == logical))
        );
        assert_ne!(
            pins[0].id, pins[1].id,
            "same display title keeps two widgets"
        );

        let mut traversed = std::collections::BTreeSet::new();
        for _ in 0..4 {
            render_duplicate_explanations(&mut fixture, viewport, key_event(egui::Key::Tab, false));
            if let Some(logical) = crate::ui::keyboard::logical_focus(&fixture.full_egui_context())
                && expected.contains(&logical)
            {
                traversed.insert(logical);
            }
        }
        assert_eq!(
            traversed.len(),
            2,
            "Tab reaches both same-title semantic Pin controls"
        );

        let click_pin = |fixture: &mut ProductionFixture,
                         logical: &crate::ui::keyboard::LogicalFocus| {
            crate::ui::keyboard::clear_focus(&fixture.full_egui_context());
            crate::ui::keyboard::request_logical(&fixture.full_egui_context(), logical.clone());
            render_duplicate_explanations(fixture, viewport, Vec::new());
            assert_eq!(
                crate::ui::keyboard::logical_focus(&fixture.full_egui_context()),
                Some(logical.clone()),
                "requested semantic Pin receives rendered focus"
            );
            render_duplicate_explanations(fixture, viewport, key_event(egui::Key::Enter, false));
        };

        click_pin(&mut fixture, &expected[1]);
        assert_eq!(
            fixture
                .host
                .world_mut()
                .resource::<ExplanationState>()
                .invoker,
            Some(expected[1].clone()),
            "Pin stores the exact second semantic invoker"
        );
        let dismiss = recorded_explanation_dismiss(&fixture.full_egui_context())
            .expect("visible Dismiss for pinned explanation");
        assert!(dismiss.width() >= 24.0 && dismiss.height() >= 24.0);
        crate::ui::keyboard::request_logical(
            &fixture.full_egui_context(),
            crate::ui::keyboard::LogicalFocus::new("explanation-dismiss"),
        );
        render_duplicate_explanations(&mut fixture, viewport, Vec::new());
        render_duplicate_explanations(&mut fixture, viewport, key_event(egui::Key::Enter, false));
        assert!(
            fixture
                .host
                .world_mut()
                .resource::<ExplanationState>()
                .pinned
                .is_none()
        );
        assert_eq!(
            crate::ui::keyboard::logical_focus(&fixture.full_egui_context()),
            Some(expected[1].clone()),
            "visible Dismiss returns to the actual second invoker"
        );

        click_pin(&mut fixture, &expected[0]);
        assert_eq!(
            fixture
                .host
                .world_mut()
                .resource::<ExplanationState>()
                .invoker,
            Some(expected[0].clone()),
            "Pin stores the exact first semantic invoker"
        );
        render_duplicate_explanations(&mut fixture, viewport, key_event(egui::Key::Escape, false));
        render_duplicate_explanations(&mut fixture, viewport, Vec::new());
        assert!(
            fixture
                .host
                .world_mut()
                .resource::<ExplanationState>()
                .pinned
                .is_none()
        );
        assert_eq!(
            crate::ui::keyboard::logical_focus(&fixture.full_egui_context()),
            Some(expected[0].clone()),
            "Escape returns to the actual first invoker"
        );
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn production_situation_controls_emit_and_resolve_authoritative_command() {
        let mut exercised_vertical_overflow = false;
        for spec in MATRIX {
            let viewport = egui::vec2(spec.width_px / spec.scale, spec.height_px / spec.scale);
            let viewport_rect = egui::Rect::from_min_size(egui::Pos2::ZERO, viewport);
            let mut fixture = ProductionFixture::new();
            fixture.prepare_full_shell();
            fixture.render_full_shell(viewport, Vec::new());
            let mut output = fixture.render_full_shell(viewport, Vec::new());
            for required in [
                "Situations",
                "Inspector",
                "Log",
                "Assignments",
                "Attention overlay evidence",
                "Veyrin",
            ] {
                assert!(
                    materially_visible_text(&output, viewport_rect, required).is_some(),
                    "complete production shell component '{required}' at {spec:?}"
                );
            }
            let clip_failures = horizontal_clip_failures(&output);
            assert!(
                clip_failures.is_empty(),
                "initial production shell clips at {spec:?}: {clip_failures:#?}"
            );
            let ctx = fixture.full_egui_context();
            let mut roles = vec!["dock-header", "search", "attention", "time", "subject"];
            if viewport.x < 1_400.0 || viewport.y < 640.0 {
                roles.push("compact-tab");
            }
            assert_response_roles(&ctx, viewport_rect, spec, &roles);
            exercised_vertical_overflow |= assert_vertical_only_scroll(&ctx, spec);
            assert!(
                resolved_visuals_contrast_passes(
                    &ctx.style_of(egui::Theme::Dark).visuals,
                    &UiTheme::embedded()
                ),
                "resolved production contrast at {spec:?}"
            );
            let subject = recorded_link(&ctx)
                .unwrap_or_else(|| panic!("production identity subject recorded at {spec:?}"));
            assert!(
                viewport_rect.contains(subject.center())
                    && subject.width() >= 24.0
                    && subject.height() >= 24.0,
                "production subject lies inside viewport at {spec:?}: {subject:?}"
            );
            fixture.render_full_shell(viewport, press_at(subject.center()));
            let pressed_subject =
                recorded_link(&fixture.full_egui_context()).expect("subject response after press");
            fixture.render_full_shell(viewport, release_at(pressed_subject.center()));
            assert!(
                fixture
                    .host
                    .world_mut()
                    .resource::<ViewState>()
                    .selected
                    .is_some(),
                "production subject routed at {spec:?}"
            );
            let time = recorded_time_control(&fixture.full_egui_context())
                .unwrap_or_else(|| panic!("production time response recorded at {spec:?}"));
            assert!(
                viewport_rect.contains(time.center())
                    && time.width() >= 24.0
                    && time.height() >= 24.0,
                "production time control lies inside viewport at {spec:?}: {time:?}"
            );
            let mut pressed_time = time;
            fixture.render_full_shell(viewport, Vec::new());
            for _ in 0..3 {
                let current = recorded_time_control(&fixture.full_egui_context())
                    .expect("time response before press");
                pressed_time = current;
                output = fixture.render_full_shell(viewport, click_at(current.center()));
                if !fixture.host.world_mut().resource::<TimeControl>().paused {
                    break;
                }
            }
            assert!(
                !fixture.host.world_mut().resource::<TimeControl>().paused,
                "production time control clicked at {spec:?}: {time:?} -> {pressed_time:?}"
            );
            for _ in 0..5 {
                let action_reachable = recorded_situation_action(&fixture.full_egui_context())
                    .is_some_and(|rect| rect.width() >= 24.0 && rect.height() >= 24.0);
                if materially_visible_text(&output, viewport_rect, "Call In the Favour").is_some()
                    && action_reachable
                {
                    break;
                }
                output = fixture.render_full_shell(
                    viewport,
                    vec![
                        egui::Event::PointerMoved(egui::pos2(viewport.x - 40.0, viewport.y * 0.5)),
                        egui::Event::MouseWheel {
                            unit: egui::MouseWheelUnit::Point,
                            delta: egui::vec2(0.0, -400.0),
                            modifiers: egui::Modifiers::NONE,
                            phase: egui::TouchPhase::Move,
                        },
                    ],
                );
            }
            assert!(
                recorded_situation_action(&fixture.full_egui_context()).is_some_and(|rect| {
                    egui::Rect::from_min_size(egui::Pos2::ZERO, viewport).contains(rect.center())
                }),
                "production action reachable at {spec:?}"
            );
            assert_response_roles(
                &fixture.full_egui_context(),
                viewport_rect,
                spec,
                &["situation-action"],
            );
            assert!(
                materially_visible_text(&output, viewport_rect, "favourable outcome").is_some(),
                "Situation consequence has an always-visible summary at {spec:?}"
            );
            assert!(
                recorded_explanation_trigger(&fixture.full_egui_context())
                    .is_some_and(|rect| viewport_rect.contains(rect.center())),
                "focusable Situation detail trigger at {spec:?}"
            );
            let detail_id = recorded_explanation_trigger_id(&fixture.full_egui_context())
                .expect("production detail trigger id");
            fixture
                .full_egui_context()
                .memory_mut(|memory| memory.request_focus(detail_id));
            let mut focused = fixture.render_full_shell(viewport, vec![egui::Event::PointerGone]);
            for _ in 0..3 {
                if recorded_explanation_preview(&fixture.full_egui_context())
                    && materially_visible_text(&focused, viewport_rect, "If ordered now").is_some()
                {
                    break;
                }
                let current_id = recorded_explanation_trigger_id(&fixture.full_egui_context())
                    .expect("stable production detail trigger id");
                fixture
                    .full_egui_context()
                    .memory_mut(|memory| memory.request_focus(current_id));
                focused = fixture.render_full_shell(viewport, vec![egui::Event::PointerGone]);
            }
            assert!(
                recorded_explanation_preview(&fixture.full_egui_context())
                    && materially_visible_text(&focused, viewport_rect, "If ordered now").is_some()
                    && materially_visible_text(&focused, viewport_rect, "Days from the order")
                        .is_some()
                    && materially_visible_text(&focused, viewport_rect, "governing skill against")
                        .is_some()
                    && materially_visible_text(
                        &focused,
                        viewport_rect,
                        "exact outcome distribution"
                    )
                    .is_some(),
                "keyboard focus exposes consequential timing, contest and outcome meaning without a pointer at {spec:?}: {:?}",
                painted_texts(&focused),
            );
            let focused_trigger = recorded_explanation_trigger_id(&fixture.full_egui_context())
                .expect("focused production detail trigger before activation");
            fixture
                .full_egui_context()
                .memory_mut(|memory| memory.request_focus(focused_trigger));
            fixture.render_full_shell(
                viewport,
                vec![egui::Event::Key {
                    key: egui::Key::Enter,
                    physical_key: Some(egui::Key::Enter),
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                }],
            );
            let pinned = fixture.render_full_shell(viewport, vec![egui::Event::PointerGone]);
            let pinned_rect = recorded_pinned_explanation(&fixture.full_egui_context())
                .unwrap_or_else(|| panic!("pinned explanation is laid out at {spec:?}"));
            let dismiss_visible =
                materially_visible_text(&pinned, viewport_rect, "Dismiss explanation").is_some();
            let pinned_clip_failures = horizontal_clip_failures(&pinned);
            assert!(
                viewport_rect.contains(pinned_rect.min)
                    && viewport_rect.contains(pinned_rect.max)
                    && dismiss_visible
                    && pinned_clip_failures.is_empty(),
                "pinned explanation is constrained, wrapped and visibly dismissible at {spec:?}: rect={pinned_rect:?}, dismiss={dismiss_visible}, clips={pinned_clip_failures:#?}, painted={:?}",
                painted_texts(&pinned),
            );
            let owned_topic = fixture
                .host
                .world_mut()
                .resource::<ExplanationState>()
                .pinned
                .clone()
                .expect("visible pin captures an owned topic");
            let queued_before_help_dismiss = fixture
                .host
                .world_mut()
                .resource::<UiCommandQueue>()
                .0
                .len();
            let visible_dismiss = recorded_explanation_dismiss(&fixture.full_egui_context())
                .expect("visible explanation dismissal response");
            fixture.render_full_shell(viewport, press_at(visible_dismiss.center()));
            let dismiss_release = recorded_explanation_dismiss(&fixture.full_egui_context())
                .expect("visible explanation dismissal after press");
            fixture.render_full_shell(viewport, release_at(dismiss_release.center()));
            assert!(
                fixture
                    .host
                    .world_mut()
                    .resource::<ExplanationState>()
                    .pinned
                    .is_none(),
                "visible control dismisses pinned help at {spec:?}"
            );
            assert_eq!(
                fixture
                    .host
                    .world_mut()
                    .resource::<UiCommandQueue>()
                    .0
                    .len(),
                queued_before_help_dismiss,
                "visible help dismissal preserves the command queue at {spec:?}"
            );
            // Visible close correctly restored the logical Pin invoker. End
            // that completed keyboard interaction before the independent
            // stationary-pointer route, or its focus tooltip would remain a
            // real top-layer hit target over the action under test.
            let ctx = fixture.full_egui_context();
            crate::ui::keyboard::clear_focus(&ctx);
            fixture.render_full_shell(viewport, Vec::new());
            let action_center = visible_response_center(
                &fixture.full_egui_context(),
                viewport_rect,
                "situation-action",
            );
            // Prime egui's hover delay once, then hold the pointer stationary.
            // The next two fixed passes allow egui's response-owned popup to
            // complete its sizing/opening lifecycle. Every asserted frame
            // after that must still report the originating action as hovered
            // and paint the same semantics as keyboard focus.
            fixture.render_full_shell(viewport, vec![egui::Event::PointerMoved(action_center)]);
            fixture.render_full_shell(viewport, Vec::new());
            fixture.render_full_shell(viewport, Vec::new());
            for held_frame in 0..4 {
                let forecast = fixture.render_full_shell(viewport, Vec::new());
                let forecast_drawn = recorded_situation_forecast(&fixture.full_egui_context());
                let action_hovered =
                    recorded_situation_action_hovered(&fixture.full_egui_context());
                let action_rect = recorded_situation_action(&fixture.full_egui_context())
                    .expect("stationary-hover action remains in this frame");
                let forecast_body = recorded_forecast_body(&fixture.full_egui_context())
                    .expect("stationary-hover forecast belongs to this frame");
                assert!(
                    action_hovered
                        && action_rect.contains(action_center)
                        && forecast_drawn
                        && forecast_body.width() > 0.0
                        && forecast_body.height() > 0.0
                        && viewport_rect.intersects(forecast_body)
                        && materially_visible_text(&forecast, viewport_rect, "If ordered now")
                            .is_some()
                        && materially_visible_text(&forecast, viewport_rect, "Days from the order")
                            .is_some()
                        && materially_visible_text(
                            &forecast,
                            viewport_rect,
                            "governing skill against",
                        )
                        .is_some()
                        && materially_visible_text(
                            &forecast,
                            viewport_rect,
                            "exact outcome distribution",
                        )
                        .is_some(),
                    "stationary response-owned hover frame {held_frame} remains anchored and paints focus-equivalent meaning at {spec:?}: hovered={action_hovered}, drawn={forecast_drawn}, action={action_rect:?}, forecast={forecast_body:?}, painted={:?}",
                    painted_texts(&forecast),
                );
            }
            fixture.render_full_shell(viewport, vec![egui::Event::PointerGone]);
            let action_press = visible_response_center(
                &fixture.full_egui_context(),
                viewport_rect,
                "situation-action",
            );
            fixture.render_full_shell(viewport, press_at(action_press));
            let action_release = visible_response_center(
                &fixture.full_egui_context(),
                viewport_rect,
                "situation-action",
            );
            fixture.render_full_shell(viewport, release_at(action_release));
            assert!(
                fixture.host.world_mut().resource::<AssignmentPopup>().open,
                "Situation action opens production assignment popup at {spec:?}"
            );
            fixture.host.world_mut().resource_mut::<PickerState>().open = true;
            fixture
                .host
                .world_mut()
                .resource_mut::<ExplanationState>()
                .pinned = Some(owned_topic.clone());
            fixture.render_full_shell(viewport, vec![egui::Event::PointerGone]);
            let view_before_help_escape = *fixture.host.world_mut().resource::<ViewState>();
            let form_before_help_escape = fixture
                .host
                .world_mut()
                .resource::<AssignmentForm>()
                .clone();
            let popup_before_help_escape = fixture
                .host
                .world_mut()
                .resource::<AssignmentPopup>()
                .clone();
            let picker_before_help_escape =
                fixture.host.world_mut().resource::<PickerState>().clone();
            let search_before_help_escape =
                fixture.host.world_mut().resource::<SearchState>().clone();
            let queue_before_help_escape = fixture
                .host
                .world_mut()
                .resource::<UiCommandQueue>()
                .0
                .clone();
            assert!(
                popup_before_help_escape.open
                    && picker_before_help_escape.open
                    && form_before_help_escape.assignment.is_some(),
                "Escape fixture holds a real active assignment composition at {spec:?}"
            );
            fixture.render_full_shell(
                viewport,
                vec![egui::Event::Key {
                    key: egui::Key::Escape,
                    physical_key: Some(egui::Key::Escape),
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                }],
            );
            assert!(
                fixture
                    .host
                    .world_mut()
                    .resource::<ExplanationState>()
                    .pinned
                    .is_none(),
                "claimed Escape closes visible help over active composition at {spec:?}"
            );
            assert_eq!(
                *fixture.host.world_mut().resource::<ViewState>(),
                view_before_help_escape,
                "claimed Escape preserves map view and selection at {spec:?}"
            );
            assert_eq!(
                *fixture.host.world_mut().resource::<AssignmentForm>(),
                form_before_help_escape,
                "claimed Escape preserves active assignment composition at {spec:?}"
            );
            assert_eq!(
                fixture
                    .host
                    .world_mut()
                    .resource::<AssignmentPopup>()
                    .clone(),
                popup_before_help_escape,
                "claimed Escape preserves open assignment popup at {spec:?}"
            );
            assert_eq!(
                fixture.host.world_mut().resource::<PickerState>().clone(),
                picker_before_help_escape,
                "claimed Escape preserves open candidate picker at {spec:?}"
            );
            assert_eq!(
                *fixture.host.world_mut().resource::<SearchState>(),
                search_before_help_escape,
                "claimed Escape preserves search context at {spec:?}"
            );
            assert_eq!(
                fixture.host.world_mut().resource::<UiCommandQueue>().0,
                queue_before_help_escape,
                "claimed Escape emits no authoritative command at {spec:?}"
            );

            // With no pinned help, #8's nearer local surfaces correctly own
            // Escape before the map. Remove those presentation layers to
            // exercise the genuinely unclaimed Body -> System fallback.
            fixture.host.world_mut().resource_mut::<PickerState>().open = false;
            fixture
                .host
                .world_mut()
                .resource_mut::<AssignmentPopup>()
                .open = false;
            fixture
                .host
                .world_mut()
                .resource_mut::<AssignmentForm>()
                .reset();
            fixture
                .host
                .world_mut()
                .resource_mut::<SearchState>()
                .query
                .clear();
            crate::ui::keyboard::clear_focus(&fixture.full_egui_context());
            fixture.render_full_shell(
                viewport,
                vec![egui::Event::Key {
                    key: egui::Key::Escape,
                    physical_key: Some(egui::Key::Escape),
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                }],
            );
            let after_fallthrough = *fixture.host.world_mut().resource::<ViewState>();
            assert_eq!(
                after_fallthrough.view,
                MapView::System,
                "unclaimed Escape retains normal map-navigation behavior at {spec:?}"
            );
            assert_eq!(
                after_fallthrough.selected, view_before_help_escape.selected,
                "normal Escape changes only the map level at {spec:?}"
            );
            assert_eq!(
                fixture.host.world_mut().resource::<UiCommandQueue>().0,
                queue_before_help_escape,
                "unclaimed map Escape emits no authoritative command at {spec:?}"
            );
            *fixture.host.world_mut().resource_mut::<ViewState>() = view_before_help_escape;
            *fixture.host.world_mut().resource_mut::<AssignmentForm>() =
                form_before_help_escape.clone();
            *fixture.host.world_mut().resource_mut::<AssignmentPopup>() =
                popup_before_help_escape.clone();
            *fixture.host.world_mut().resource_mut::<PickerState>() =
                picker_before_help_escape.clone();
            fixture.host.world_mut().resource_mut::<PickerState>().open = false;
            *fixture.host.world_mut().resource_mut::<SearchState>() =
                search_before_help_escape.clone();
            let popup = fixture.render_full_shell(viewport, Vec::new());
            let confirm = recorded_confirm(&fixture.full_egui_context())
                .expect("Situation popup renders Confirm");
            assert!(
                viewport_rect.contains(confirm.center())
                    && confirm.width() >= 24.0
                    && confirm.height() >= 24.0,
                "Situation Confirm is reachable at {spec:?}: {confirm:?}"
            );
            assert_response_roles(
                &fixture.full_egui_context(),
                viewport_rect,
                spec,
                &["confirm"],
            );
            assert!(
                materially_visible_text(&popup, viewport_rect, "Takes").is_some()
                    && horizontal_clip_failures(&popup).is_empty(),
                "production popup forecast is painted inside its clip at {spec:?}"
            );
            fixture.render_full_shell(viewport, press_at(confirm.center()));
            let pressed_confirm = recorded_confirm(&fixture.full_egui_context())
                .expect("Situation Confirm after press");
            fixture.render_full_shell(viewport, release_at(pressed_confirm.center()));
            assert!(
                fixture
                    .host
                    .world_mut()
                    .resource::<UiCommandQueue>()
                    .0
                    .iter()
                    .any(|command| matches!(
                        command,
                        PlayerCommand::StartSituationAssignment { .. }
                    )),
                "popup Confirm emits the Situation command at {spec:?}"
            );
            let queued_command_count = fixture
                .host
                .world_mut()
                .resource::<UiCommandQueue>()
                .0
                .len();
            crate::ui::keyboard::clear_focus(&fixture.full_egui_context());
            let after_confirm = fixture.render_full_shell(viewport, Vec::new());
            let repin = recorded_explanation_trigger(&fixture.full_egui_context())
                .filter(|rect| viewport_rect.contains(rect.center()))
                .unwrap_or_else(|| {
                    panic!(
                        "Situation detail remains reachable after Confirm at {spec:?}: {:?}",
                        painted_texts(&after_confirm)
                    )
                });
            fixture.render_full_shell(viewport, press_at(repin.center()));
            let repin_release = recorded_explanation_trigger(&fixture.full_egui_context())
                .expect("Situation detail remains after press");
            fixture.render_full_shell(viewport, release_at(repin_release.center()));
            let pinned_title = fixture
                .host
                .world_mut()
                .resource::<ExplanationState>()
                .pinned
                .as_ref()
                .map(|topic| topic.title.clone())
                .expect("Situation forecast snapshot pinned before resolution");
            fixture
                .host
                .world_mut()
                .resource_mut::<ViewState>()
                .selected = None;
            let reflowed_viewport = egui::vec2((viewport.x * 0.72).max(480.0), viewport.y);
            let reflowed_rect = egui::Rect::from_min_size(egui::Pos2::ZERO, reflowed_viewport);
            let reflowed =
                fixture.render_full_shell(reflowed_viewport, vec![egui::Event::PointerGone]);
            let reflowed_help = recorded_pinned_explanation(&fixture.full_egui_context())
                .expect("pinned snapshot survives context and responsive reflow");
            assert!(
                reflowed_rect.contains(reflowed_help.min)
                    && reflowed_rect.contains(reflowed_help.max)
                    && materially_visible_text(&reflowed, reflowed_rect, &pinned_title).is_some(),
                "owned explanation remains readable after context/reflow at {spec:?}: {reflowed_help:?}"
            );
            assert_eq!(
                fixture
                    .host
                    .world_mut()
                    .resource::<UiCommandQueue>()
                    .0
                    .len(),
                queued_command_count,
                "context and help changes do not alter the authoritative queue at {spec:?}"
            );
            fixture.submit_queued_and_resolve();
            let mut resolved = fixture.render_full_shell(viewport, Vec::new());
            assert_eq!(
                fixture
                    .host
                    .world_mut()
                    .resource::<ExplanationState>()
                    .pinned
                    .as_ref()
                    .map(|topic| topic.title.as_str()),
                Some(pinned_title.as_str()),
                "owned explanation survives authoritative Situation resolution at {spec:?}"
            );
            for _ in 0..5 {
                if materially_visible_text(&resolved, viewport_rect, "Resolved").is_some() {
                    break;
                }
                resolved = fixture.render_full_shell(
                    viewport,
                    vec![
                        egui::Event::PointerMoved(egui::pos2(viewport.x - 40.0, viewport.y * 0.5)),
                        egui::Event::MouseWheel {
                            unit: egui::MouseWheelUnit::Point,
                            delta: egui::vec2(0.0, 500.0),
                            modifiers: egui::Modifiers::NONE,
                            phase: egui::TouchPhase::Move,
                        },
                    ],
                );
            }
            assert!(
                materially_visible_text(&resolved, viewport_rect, "Resolved").is_some(),
                "resolution rendered at {spec:?}"
            );
            assert!(
                materially_visible_text(&resolved, viewport_rect, "History").is_some(),
                "history rendered at {spec:?}"
            );
            let applied_before_escape = fixture
                .host
                .world_mut()
                .resource::<aeon_sim::command::CommandLog>()
                .applied
                .len();
            fixture.render_full_shell(
                viewport,
                vec![egui::Event::Key {
                    key: egui::Key::Escape,
                    physical_key: Some(egui::Key::Escape),
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                }],
            );
            assert!(
                fixture
                    .host
                    .world_mut()
                    .resource::<ExplanationState>()
                    .pinned
                    .is_none(),
                "Escape dismisses a snapshot after source resolution at {spec:?}"
            );
            assert_eq!(
                fixture
                    .host
                    .world_mut()
                    .resource::<aeon_sim::command::CommandLog>()
                    .applied
                    .len(),
                applied_before_escape,
                "Escape dismissal has no authoritative effect at {spec:?}"
            );
        }
        assert!(
            exercised_vertical_overflow,
            "the accepted matrix exercises real vertical-only overflow"
        );
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn production_situation_path_is_keyboard_complete() {
        for spec in MATRIX {
            let viewport = egui::vec2(spec.width_px / spec.scale, spec.height_px / spec.scale);
            let viewport_rect = egui::Rect::from_min_size(egui::Pos2::ZERO, viewport);
            let mut fixture = ProductionFixture::new();
            fixture.prepare_full_shell();
            fixture.render_full_shell(viewport, Vec::new());
            fixture.render_full_shell(viewport, Vec::new());
            let ctx = fixture.full_egui_context();
            assert_eq!(
                ctx.memory(|memory| memory.focused()),
                None,
                "starts unfocused at {spec:?}"
            );
            assert_registry_visual_order(&ctx, spec);

            // Enter activates the real time control, reached from an empty
            // focus state solely through the visual-order registry.
            let (time_output, time_path) = tab_until(&mut fixture, viewport, "time", false);
            let time = registry_focus(&fixture.full_egui_context()).expect("time focus");
            assert_eq!(time.role, "time");
            assert!(!time_path.is_empty());
            assert_focused_boundary(&time_output, &time, viewport_rect, spec);
            fixture.render_full_shell(viewport, key_event(egui::Key::Enter, false));
            assert!(!fixture.host.world_mut().resource::<TimeControl>().paused);

            // These are production icon rows, reached only by Tab. Their
            // physical ends wrap under real Arrow events.
            assert_real_group_wrap(&mut fixture, viewport, "map-mode", spec);
            assert_real_group_wrap(&mut fixture, viewport, "panel-toggle", spec);

            // Search is the production TextEdit. ArrowLeft edits its cursor;
            // it must not escape into either surrounding roving group.
            let _ = tab_until(&mut fixture, viewport, "search", false);
            let search = registry_focus(&fixture.full_egui_context()).expect("search focus");
            assert_eq!(search.role, "search");
            let before_cursor = egui::TextEdit::load_state(&fixture.full_egui_context(), search.id)
                .and_then(|state| state.cursor.char_range())
                .expect("search cursor before ArrowLeft")
                .primary
                .index
                .0;
            assert!(
                before_cursor > 0,
                "search cursor starts after text at {spec:?}"
            );
            fixture.render_full_shell(viewport, key_event(egui::Key::ArrowLeft, false));
            let search_after = settle_registry_focus(&mut fixture, viewport);
            let after_cursor =
                egui::TextEdit::load_state(&fixture.full_egui_context(), search_after.id)
                    .and_then(|state| state.cursor.char_range())
                    .expect("search cursor after ArrowLeft")
                    .primary
                    .index
                    .0;
            assert_eq!(search_after.logical, search.logical);
            assert_eq!(after_cursor + 1, before_cursor, "TextEdit owns ArrowLeft");

            // Settings opens from its real top-bar action. Reflow while a
            // preference owns focus, then one physical+egui Escape closes
            // only settings and restores the logical invoker. Search remains
            // open and the strategic view/selection are untouched.
            let _ = tab_until(&mut fixture, viewport, "settings", false);
            let settings_invoker = registry_focus(&fixture.full_egui_context())
                .expect("settings action focus")
                .logical;
            fixture.render_full_shell(viewport, key_event(egui::Key::Enter, false));
            fixture.render_full_shell(viewport, Vec::new());
            assert!(fixture.host.world_mut().resource::<SettingsUi>().open);
            assert_eq!(
                fixture
                    .host
                    .world_mut()
                    .resource::<SettingsUi>()
                    .invoker
                    .as_ref(),
                Some(&settings_invoker)
            );
            let _ = tab_until(&mut fixture, viewport, "preference-scale", false);
            let preference_focus = registry_focus(&fixture.full_egui_context())
                .expect("real preference control focus");
            assert_eq!(preference_focus.role, "preference-scale");
            let settings_reflow = if viewport.x > 1_000.0 {
                egui::vec2(900.0, 560.0)
            } else {
                egui::vec2(1_600.0, 800.0)
            };
            fixture.render_full_shell(settings_reflow, Vec::new());
            fixture.render_full_shell(viewport, Vec::new());
            let before_local_escape = *fixture.host.world_mut().resource::<ViewState>();
            let _ = tab_until(&mut fixture, viewport, "settings-close", false);
            fixture.render_full_shell(viewport, key_event(egui::Key::Space, false));
            fixture.render_full_shell(viewport, Vec::new());
            assert!(!fixture.host.world_mut().resource::<SettingsUi>().open);
            assert_eq!(
                registry_focus(&fixture.full_egui_context())
                    .expect("explicit settings Close restores invoker")
                    .logical,
                settings_invoker
            );
            // Reopen and retain the semantic Escape evidence on the same
            // production surface.
            fixture.render_full_shell(viewport, key_event(egui::Key::Enter, false));
            fixture.render_full_shell(viewport, Vec::new());
            assert!(fixture.host.world_mut().resource::<SettingsUi>().open);
            let _ = tab_until(&mut fixture, viewport, "preference-scale", false);
            fixture.render_full_shell(viewport, key_event(egui::Key::Escape, false));
            fixture.render_full_shell(viewport, Vec::new());
            assert!(!fixture.host.world_mut().resource::<SettingsUi>().open);
            assert_eq!(
                fixture.host.world_mut().resource::<SearchState>().query,
                "Veyrin"
            );
            let after_settings_escape = *fixture.host.world_mut().resource::<ViewState>();
            assert_eq!(after_settings_escape.view, before_local_escape.view);
            assert_eq!(after_settings_escape.selected, before_local_escape.selected);
            assert_eq!(
                registry_focus(&fixture.full_egui_context())
                    .expect("settings invoker restoration")
                    .logical,
                settings_invoker
            );

            // Search is the final local layer. Escape clears it without
            // changing the Body view; with no local layer left, the next
            // press falls through to the real strategic Body -> System rule.
            let _ = tab_until(&mut fixture, viewport, "search", false);
            let search_invoker = registry_focus(&fixture.full_egui_context())
                .expect("search focus before Escape")
                .logical;
            fixture.render_full_shell(viewport, key_event(egui::Key::Escape, false));
            fixture.render_full_shell(viewport, Vec::new());
            assert!(
                fixture
                    .host
                    .world_mut()
                    .resource::<SearchState>()
                    .query
                    .is_empty()
            );
            let after_search_escape = *fixture.host.world_mut().resource::<ViewState>();
            assert_eq!(after_search_escape.view, before_local_escape.view);
            assert_eq!(after_search_escape.selected, before_local_escape.selected);
            assert_eq!(
                registry_focus(&fixture.full_egui_context())
                    .expect("search remains focused after clearing")
                    .logical,
                search_invoker
            );
            assert!(matches!(after_search_escape.view, MapView::Body(_)));
            fixture.render_full_shell(viewport, key_event(egui::Key::Escape, false));
            fixture.render_full_shell(viewport, Vec::new());
            let strategic_escape = *fixture.host.world_mut().resource::<ViewState>();
            assert_eq!(strategic_escape.view, MapView::System);
            assert_eq!(strategic_escape.selected, before_local_escape.selected);

            // Reach the actual Situation subject with forward Tab. A single
            // reverse step and forward step must visit the exact adjacent IDs.
            let (subject_output, subject_path) =
                tab_until(&mut fixture, viewport, "situation-subject", false);
            let subject = registry_focus(&fixture.full_egui_context()).expect("subject focus");
            assert_eq!(subject.role, "situation-subject");
            assert!(subject_path.windows(2).all(|pair| pair[0] != pair[1]));
            assert_focused_boundary(&subject_output, &subject, viewport_rect, spec);
            let expected_previous = independent_adjacent(&fixture.full_egui_context(), true);
            fixture.render_full_shell(viewport, key_event(egui::Key::Tab, true));
            let mut previous = None;
            for _ in 0..6 {
                fixture.render_full_shell(viewport, Vec::new());
                previous = registry_focus(&fixture.full_egui_context());
                if previous.is_some() {
                    break;
                }
            }
            let previous = previous.expect("reverse focus");
            assert_eq!(
                previous.logical, expected_previous,
                "exact reverse sequence at {spec:?}"
            );
            let expected_forward = independent_adjacent(&fixture.full_egui_context(), false);
            fixture.render_full_shell(viewport, key_event(egui::Key::Tab, false));
            let mut restored_subject = None;
            for _ in 0..6 {
                fixture.render_full_shell(viewport, Vec::new());
                if let Some(entry) = registry_focus(&fixture.full_egui_context()) {
                    restored_subject = Some(entry);
                    break;
                }
            }
            let forward = restored_subject.expect("forward restore");
            assert_eq!(
                forward.logical, expected_forward,
                "exact forward sequence at {spec:?}"
            );
            let output = fixture.render_full_shell(viewport, Vec::new());
            let forward = registry_focus(&fixture.full_egui_context())
                .expect("forward target remains focused for paint");
            assert_focused_boundary(&output, &forward, viewport_rect, spec);
            let (output, subject) = if forward.role == "situation-subject" {
                (output, forward)
            } else {
                let (output, _) = tab_until(&mut fixture, viewport, "situation-subject", false);
                let subject =
                    registry_focus(&fixture.full_egui_context()).expect("subject refocus");
                (output, subject)
            };
            assert_focused_boundary(&output, &subject, viewport_rect, spec);

            fixture.render_full_shell(viewport, key_event(egui::Key::Space, false));
            assert!(
                fixture
                    .host
                    .world_mut()
                    .resource::<ViewState>()
                    .selected
                    .is_some()
            );

            // Focus is the keyboard equivalent of hover for the real
            // authoritative forecast. Responsive reflow retains its logical
            // target even though the control receives a different egui ID.
            let (mut focused, action_path) =
                tab_until(&mut fixture, viewport, "situation-action", false);
            let action = registry_focus(&fixture.full_egui_context()).expect("action focus");
            assert_eq!(action.role, "situation-action");
            assert!(action_path.windows(2).all(|pair| pair[0] != pair[1]));
            let action_key = action.logical.clone();
            assert_focused_boundary(&focused, &action, viewport_rect, spec);
            for _ in 0..8 {
                if recorded_situation_forecast(&fixture.full_egui_context())
                    && materially_visible_text(&focused, viewport_rect, "Takes").is_some()
                {
                    break;
                }
                focused = fixture.render_full_shell(viewport, Vec::new());
            }
            assert!(recorded_situation_forecast(&fixture.full_egui_context()));
            assert!(materially_visible_text(&focused, viewport_rect, "Takes").is_some());

            let reflow = if viewport.x > 1_000.0 {
                egui::vec2(900.0, 560.0)
            } else {
                egui::vec2(1_600.0, 800.0)
            };
            fixture.render_full_shell(reflow, Vec::new());
            fixture.render_full_shell(reflow, Vec::new());
            let reflowed = registry_focus(&fixture.full_egui_context()).expect("reflow focus");
            assert_eq!(
                reflowed.logical, action_key,
                "logical reflow repair at {spec:?}"
            );
            fixture.render_full_shell(viewport, Vec::new());
            focused = fixture.render_full_shell(viewport, Vec::new());
            let mut action =
                registry_focus(&fixture.full_egui_context()).expect("restored action focus");
            for _ in 0..8 {
                if action.clip.contains_rect(action.rect)
                    && viewport_rect.contains_rect(action.rect)
                {
                    break;
                }
                focused = fixture.render_full_shell(viewport, Vec::new());
                action = registry_focus(&fixture.full_egui_context())
                    .expect("restored action stays focused while scrolling");
            }
            assert_eq!(action.logical, action_key);
            assert_focused_boundary(&focused, &action, viewport_rect, spec);

            fixture.render_full_shell(viewport, key_event(egui::Key::Enter, false));
            assert!(fixture.host.world_mut().resource::<AssignmentPopup>().open);
            let before_popup_escape = *fixture.host.world_mut().resource::<ViewState>();

            let _ = tab_until(&mut fixture, viewport, "assignment-popup-cancel", false);
            fixture.render_full_shell(viewport, key_event(egui::Key::Enter, false));
            fixture.render_full_shell(viewport, Vec::new());
            assert!(!fixture.host.world_mut().resource::<AssignmentPopup>().open);
            assert_eq!(
                registry_focus(&fixture.full_egui_context())
                    .expect("explicit assignment Cancel restores invoker")
                    .logical,
                action_key
            );
            fixture.render_full_shell(viewport, key_event(egui::Key::Enter, false));
            fixture.render_full_shell(viewport, Vec::new());
            assert!(fixture.host.world_mut().resource::<AssignmentPopup>().open);

            // Escape resolves the stable logical invoker against the freshly
            // rendered registry, rather than retaining an obsolete egui ID.
            fixture.render_full_shell(viewport, key_event(egui::Key::Escape, false));
            fixture.render_full_shell(viewport, Vec::new());
            assert!(!fixture.host.world_mut().resource::<AssignmentPopup>().open);
            let after_popup_escape = *fixture.host.world_mut().resource::<ViewState>();
            assert_eq!(after_popup_escape.view, before_popup_escape.view);
            assert_eq!(after_popup_escape.selected, before_popup_escape.selected);
            let restored = registry_focus(&fixture.full_egui_context()).expect("restored invoker");
            assert_eq!(restored.logical, action_key);

            fixture.render_full_shell(viewport, key_event(egui::Key::Enter, false));
            fixture.render_full_shell(viewport, Vec::new());
            assert!(
                fixture.host.world_mut().resource::<AssignmentPopup>().open,
                "keyboard reopened popup at {spec:?}"
            );
            assert!(
                recorded_confirm(&fixture.full_egui_context()).is_some(),
                "production Confirm rendered at {spec:?}"
            );
            {
                let world = fixture.host.world_mut();
                let form = world.resource::<AssignmentForm>();
                assert!(
                    form.assignment.is_some() && form.leader.is_some() && form.target.is_some(),
                    "popup form ready at {spec:?}: assignment={:?} leader={:?} target={:?}",
                    form.assignment,
                    form.leader,
                    form.target
                );
                let cache = world.resource::<ForecastCache>();
                assert!(
                    cache
                        .forecast
                        .as_ref()
                        .is_some_and(|forecast| forecast.startable()),
                    "popup forecast ready at {spec:?}: {:?}",
                    cache.forecast
                );
            }
            fixture.render_full_shell(viewport, Vec::new());
            fixture.render_full_shell(viewport, Vec::new());

            // The real leader picker is entered using only Tab and Enter.
            // Its roving row includes captured disabled committed leaders;
            // ArrowLeft must wrap among enabled candidates without landing on
            // any of them, and Escape restores the logical invoker.
            let _ = tab_until(&mut fixture, viewport, "choose-leader", false);
            let choose_leader = registry_focus(&fixture.full_egui_context())
                .expect("choose-leader focus")
                .logical;
            fixture.render_full_shell(viewport, key_event(egui::Key::Enter, false));
            fixture.render_full_shell(viewport, Vec::new());
            assert!(fixture.host.world_mut().resource::<PickerState>().open);
            let local_view = *fixture.host.world_mut().resource::<ViewState>();
            let _ = tab_until(&mut fixture, viewport, "leader", false);
            let first_leader = registry_focus(&fixture.full_egui_context()).expect("leader focus");
            let enabled_leaders = role_sequence(&fixture.full_egui_context(), "leader");
            let raw_leaders = crate::ui::keyboard::audited_responses(&fixture.full_egui_context())
                .into_iter()
                .filter(|entry| entry.role == "leader")
                .collect::<Vec<_>>();
            assert!(
                raw_leaders.iter().any(|entry| !entry.enabled),
                "production picker captures a disabled member at {spec:?}: {raw_leaders:?}"
            );
            let first_index = enabled_leaders
                .iter()
                .position(|logical| logical == &first_leader.logical)
                .expect("Tab-reached leader belongs to enabled row");
            for expected in enabled_leaders.iter().skip(first_index + 1) {
                fixture.render_full_shell(viewport, key_event(egui::Key::ArrowRight, false));
                assert_eq!(
                    settle_registry_focus(&mut fixture, viewport).logical,
                    *expected,
                    "real picker follows exact visual leader order at {spec:?}"
                );
            }
            // The disabled rows are physically after the enabled candidates.
            // One more ArrowRight crosses them and wraps to the enabled head.
            fixture.render_full_shell(viewport, key_event(egui::Key::ArrowRight, false));
            let wrapped_leader = settle_registry_focus(&mut fixture, viewport);
            assert_eq!(
                wrapped_leader.logical, enabled_leaders[0],
                "real picker wraps while skipping disabled member at {spec:?}"
            );
            assert!(
                raw_leaders
                    .iter()
                    .filter(|entry| !entry.enabled)
                    .all(|entry| entry.logical != wrapped_leader.logical)
            );
            let _ = tab_until(&mut fixture, viewport, "picker-close", false);
            fixture.render_full_shell(viewport, key_event(egui::Key::Space, false));
            fixture.render_full_shell(viewport, Vec::new());
            assert!(!fixture.host.world_mut().resource::<PickerState>().open);
            assert!(fixture.host.world_mut().resource::<AssignmentPopup>().open);
            let after_picker_escape = *fixture.host.world_mut().resource::<ViewState>();
            assert_eq!(after_picker_escape.view, local_view.view);
            assert_eq!(after_picker_escape.selected, local_view.selected);
            assert_eq!(
                registry_focus(&fixture.full_egui_context())
                    .expect("picker invoker restoration")
                    .logical,
                choose_leader
            );
            // Reopen from the restored real invoker and retain the shared
            // semantic Escape evidence too.
            fixture.render_full_shell(viewport, key_event(egui::Key::Enter, false));
            fixture.render_full_shell(viewport, Vec::new());
            assert!(fixture.host.world_mut().resource::<PickerState>().open);
            let _ = tab_until(&mut fixture, viewport, "leader", false);
            fixture.render_full_shell(viewport, key_event(egui::Key::Escape, false));
            fixture.render_full_shell(viewport, Vec::new());
            assert!(!fixture.host.world_mut().resource::<PickerState>().open);
            assert!(fixture.host.world_mut().resource::<AssignmentPopup>().open);
            assert_eq!(
                registry_focus(&fixture.full_egui_context())
                    .expect("picker Escape restores invoker")
                    .logical,
                choose_leader
            );
            assert!(
                crate::ui::keyboard::completed_registry(&fixture.full_egui_context())
                    .iter()
                    .any(|entry| entry.role == "confirm"),
                "enabled Confirm entered registry at {spec:?}: {:?}",
                crate::ui::keyboard::completed_registry(&fixture.full_egui_context())
                    .iter()
                    .map(|entry| (entry.role, &entry.logical))
                    .collect::<Vec<_>>()
            );
            assert!(
                crate::ui::keyboard::traversal_registry(&fixture.full_egui_context())
                    .iter()
                    .any(|entry| entry.role == "confirm"),
                "Confirm promoted into traversal registry at {spec:?}: {:?}",
                crate::ui::keyboard::traversal_registry(&fixture.full_egui_context())
                    .iter()
                    .map(|entry| (entry.role, &entry.logical))
                    .collect::<Vec<_>>()
            );
            let (confirm_output, confirm_path) =
                tab_until(&mut fixture, viewport, "confirm", false);
            let confirm = registry_focus(&fixture.full_egui_context()).expect("confirm focus");
            assert_eq!(confirm.role, "confirm");
            assert!(confirm_path.windows(2).all(|pair| pair[0] != pair[1]));
            assert_focused_boundary(&confirm_output, &confirm, viewport_rect, spec);
            fixture.render_full_shell(viewport, key_event(egui::Key::Space, false));
            assert!(
                fixture
                    .host
                    .world_mut()
                    .resource::<UiCommandQueue>()
                    .0
                    .iter()
                    .any(|command| matches!(
                        command,
                        PlayerCommand::StartSituationAssignment { .. }
                    ))
            );

            // Normal authoritative command and clock seams resolve the action.
            // The focused action then disappears; finish-frame repair must
            // choose the documented result-summary fallback without a test
            // focus request or another Tab event.
            fixture.submit_queued_and_resolve();
            let mut resolved = fixture.render_full_shell(viewport, Vec::new());
            for _ in 0..16 {
                if materially_visible_text(&resolved, viewport_rect, "Resolved").is_some()
                    && materially_visible_text(&resolved, viewport_rect, "History").is_some()
                    && registry_focus(&fixture.full_egui_context())
                        .is_some_and(|entry| entry.role == "resolution-summary")
                {
                    break;
                }
                resolved = fixture.render_full_shell(viewport, Vec::new());
            }
            resolved = fixture.render_full_shell(viewport, Vec::new());
            let repaired = registry_focus(&fixture.full_egui_context()).expect("resolution repair");
            assert_eq!(
                repaired.role, "resolution-summary",
                "action fallback at {spec:?}"
            );
            assert_focused_boundary(&resolved, &repaired, viewport_rect, spec);
            assert_registry_visual_order(&fixture.full_egui_context(), spec);
            assert!(
                materially_visible_text(&resolved, viewport_rect, "Resolved").is_some(),
                "resolution after keyboard focus at {spec:?}: focused={:?}, responses={:?}, painted={:?}",
                fixture
                    .full_egui_context()
                    .memory(|memory| memory.focused()),
                semantic_responses(&fixture.full_egui_context())
                    .into_iter()
                    .filter(|response| response.role == "resolution-dismiss")
                    .collect::<Vec<_>>(),
                painted_texts(&resolved),
            );
            assert!(materially_visible_text(&resolved, viewport_rect, "History").is_some());
        }
    }
}
