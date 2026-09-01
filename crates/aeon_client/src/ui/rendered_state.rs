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
use bevy::prelude::{IntoScheduleConfigs, Schedule};
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
            refresh_situation_panel_view(world);
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
                    crate::ui::shell::draw_panels,
                    crate::ui::assignment_popup::draw_assignment_popup,
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
    fn production_situation_controls_emit_and_resolve_authoritative_command() {
        let mut exercised_vertical_overflow = false;
        for spec in MATRIX {
            let viewport = egui::vec2(spec.width_px / spec.scale, spec.height_px / spec.scale);
            let viewport_rect = egui::Rect::from_min_size(egui::Pos2::ZERO, viewport);
            let mut fixture = ProductionFixture::new();
            fixture.prepare_full_shell();
            fixture.render_full_shell(viewport, Vec::new());
            let initial = fixture.render_full_shell(viewport, Vec::new());
            for required in [
                "Situations",
                "Inspector",
                "Log",
                "Assignments",
                "Attention overlay evidence",
                "Veyrin",
            ] {
                assert!(
                    materially_visible_text(&initial, viewport_rect, required).is_some(),
                    "complete production shell component '{required}' at {spec:?}"
                );
            }
            let clip_failures = horizontal_clip_failures(&initial);
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
            let mut output = fixture.render_full_shell(viewport, Vec::new());
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
            let action = recorded_situation_action(&fixture.full_egui_context())
                .filter(|rect| {
                    egui::Rect::from_min_size(egui::Pos2::ZERO, viewport).contains(rect.center())
                })
                .unwrap_or_else(|| panic!("production action reachable at {spec:?}"));
            assert_response_roles(
                &fixture.full_egui_context(),
                viewport_rect,
                spec,
                &["situation-action"],
            );
            fixture.render_full_shell(viewport, vec![egui::Event::PointerMoved(action.center())]);
            let mut forecast = fixture
                .render_full_shell(viewport, vec![egui::Event::PointerMoved(action.center())]);
            for _ in 0..8 {
                if recorded_situation_forecast(&fixture.full_egui_context())
                    && materially_visible_text(&forecast, viewport_rect, "Takes").is_some()
                {
                    break;
                }
                forecast = fixture.render_full_shell(viewport, Vec::new());
            }
            let forecast_drawn = recorded_situation_forecast(&fixture.full_egui_context());
            let action_hovered = recorded_situation_action_hovered(&fixture.full_egui_context());
            let forecast_body = recorded_forecast_body(&fixture.full_egui_context())
                .expect("production forecast renderer records its laid-out body");
            assert!(
                action_hovered
                    && forecast_drawn
                    && forecast_body.width() > 0.0
                    && forecast_body.height() > 0.0
                    && egui::Rect::from_min_size(egui::Pos2::ZERO, viewport)
                        .intersects(forecast_body)
                    && materially_visible_text(&forecast, viewport_rect, "Takes").is_some(),
                "production forecast tooltip is visibly laid out at {spec:?}: hovered={action_hovered}, drawn={forecast_drawn}, action={action:?}, forecast={forecast_body:?}, painted={:?}",
                painted_texts(&forecast),
            );
            fixture.render_full_shell(viewport, press_at(action.center()));
            fixture.render_full_shell(viewport, release_at(action.center()));
            assert!(
                fixture.host.world_mut().resource::<AssignmentPopup>().open,
                "Situation action opens production assignment popup at {spec:?}"
            );
            let mut popup = fixture.render_full_shell(viewport, Vec::new());
            let mut confirm = recorded_confirm(&fixture.full_egui_context())
                .expect("Situation popup renders Confirm");
            for _ in 0..6 {
                if viewport_rect.contains(confirm.center())
                    && confirm.width() >= 24.0
                    && confirm.height() >= 24.0
                    && materially_visible_text(&popup, viewport_rect, "Takes").is_some()
                {
                    break;
                }
                popup = fixture.render_full_shell(
                    viewport,
                    vec![
                        egui::Event::PointerMoved(viewport_rect.center()),
                        egui::Event::MouseWheel {
                            unit: egui::MouseWheelUnit::Point,
                            delta: egui::vec2(0.0, -400.0),
                            modifiers: egui::Modifiers::NONE,
                            phase: egui::TouchPhase::Move,
                        },
                    ],
                );
                confirm = recorded_confirm(&fixture.full_egui_context())
                    .expect("Situation Confirm remains rendered while scrolling");
            }
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
            fixture.submit_queued_and_resolve();
            let mut resolved = fixture.render_full_shell(viewport, Vec::new());
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
        }
        assert!(
            exercised_vertical_overflow,
            "the accepted matrix exercises real vertical-only overflow"
        );
    }
}
