//! Fixed presentation for authored continuing Situations.
//!
//! The client derives no gameplay rule here. It asks the authoritative
//! simulation for projected cards and assignment forecasts, then renders the
//! returned stages, warnings, facts, participants, and actions. The only
//! client-owned state is a requested focus and optimistic hiding of a
//! resolution whose logged dismissal has already been queued.

use std::collections::BTreeSet;

use aeon_data::model::SituationSubjectKind;
use aeon_sim::forecast::AssignmentForecast;
use aeon_sim::politics::PlayerHouse;
use aeon_sim::situations::{
    SituationAction, SituationCard, SituationInstanceKey, SituationLink, SituationMetricValue,
    SituationOccurrence, SituationParticipantGroup, SituationResolution, SituationState,
    active_cards, forecast_for_action, recorded_answer, situation_war, visible_to_player,
};
use aeon_sim::state::ContentDb;
use aeon_sim::{
    ArmyId, AssignmentId, BodyId, CharacterId, LogEntry, MessageLog, OfficeId, OrgId,
    PlayerCommand, ProvinceId, ShipId, TitleId,
};
use bevy::prelude::{Resource, World};
use bevy_egui::egui;

use crate::ui::explanations::{ExplanationTopic, explanation_trigger, preview_for_response};
use crate::ui::forecast::forecast_summary;
use crate::ui::layout::{draw_vertical_scroll, draw_wrapped_action, draw_wrapped_fact};
use crate::ui::panel::{PanelCtx, PanelOut};
use crate::view::{MapView, Selection};

/// Enough exact history to explain a card without turning it into another
/// full log panel.
const HISTORY_LIMIT: usize = 6;

#[cfg(test)]
const SITUATION_ACTION_RESPONSE: &str = "production-situation-action";
#[cfg(test)]
const SITUATION_FORECAST_VISIBLE: &str = "production-situation-forecast-visible";
#[cfg(test)]
const SITUATION_ACTION_HOVERED: &str = "production-situation-action-hovered";

#[cfg(test)]
pub(crate) fn recorded_situation_action(ctx: &egui::Context) -> Option<egui::Rect> {
    ctx.data(|data| data.get_temp(egui::Id::new(SITUATION_ACTION_RESPONSE)))
}

#[cfg(test)]
pub(crate) fn recorded_situation_forecast(ctx: &egui::Context) -> bool {
    ctx.data(|data| {
        data.get_temp(egui::Id::new(SITUATION_FORECAST_VISIBLE))
            .unwrap_or(false)
    })
}

#[cfg(test)]
pub(crate) fn recorded_situation_action_hovered(ctx: &egui::Context) -> bool {
    ctx.data(|data| {
        data.get_temp(egui::Id::new(SITUATION_ACTION_HOVERED))
            .unwrap_or(false)
    })
}

#[cfg(test)]
pub(crate) fn clear_situation_frame_evidence(ctx: &egui::Context) {
    ctx.data_mut(|data| {
        data.remove::<egui::Rect>(egui::Id::new(SITUATION_ACTION_RESPONSE));
        data.insert_temp(egui::Id::new(SITUATION_FORECAST_VISIBLE), false);
        data.insert_temp(egui::Id::new(SITUATION_ACTION_HOVERED), false);
    });
}

/// One projected action paired with the simulation's current forecast.
#[derive(Clone, Debug)]
pub struct SituationActionView {
    pub action: SituationAction,
    pub assignment: aeon_data::ContentKey,
    pub label: String,
    pub forecast: Option<AssignmentForecast>,
    pub unavailable: Option<String>,
}

/// One pure authored response the player may still record on a card.
#[derive(Clone, Debug)]
pub struct SituationResponseView {
    pub id: aeon_data::ContentKey,
    pub label: String,
}

/// One active card with its authored stage copy and forecasted actions.
#[derive(Clone, Debug)]
pub struct ActiveSituationView {
    pub card: SituationCard,
    pub stage_title: Option<String>,
    pub stage_summary: Option<String>,
    pub warning: Option<String>,
    pub actions: Vec<SituationActionView>,
    /// Authored responses still open to the player; empty once answered,
    /// for spectators, and while the card is unavailable.
    pub responses: Vec<SituationResponseView>,
    /// The label of the durable answer already recorded, when any.
    pub answer: Option<String>,
    pub history: Vec<LogEntry>,
}

/// One completed notice with the permanent history of its exact lifecycle.
#[derive(Clone, Debug)]
pub struct SituationResolutionView {
    pub resolution: SituationResolution,
    pub history: Vec<LogEntry>,
}

impl ActiveSituationView {
    fn warns(&self) -> bool {
        self.card
            .projection
            .as_ref()
            .is_some_and(|projection| projection.warning)
    }
}

/// The immutable, frame-ready Situation data read by the panel.
#[derive(Resource, Default)]
pub struct SituationPanelView {
    pub active: Vec<ActiveSituationView>,
    pub resolutions: Vec<SituationResolutionView>,
}

/// Presentation-only interaction state for the Situation surface.
#[derive(Resource, Default)]
pub struct SituationUiState {
    /// Card requested by an attention-strip warning.
    pub focused: Option<SituationInstanceKey>,
    /// Resolution notices hidden while their dismissal awaits its tick.
    pub dismissed: BTreeSet<u64>,
}

/// Refreshes the panel from the authoritative Situation and forecast APIs.
pub fn refresh_situation_panel_view(world: &mut World) {
    let player = world
        .get_resource::<PlayerHouse>()
        .and_then(|house| house.0);
    let Some(content_db) = world.get_resource::<ContentDb>() else {
        return;
    };
    let log_entries = world
        .get_resource::<MessageLog>()
        .map(|log| log.entries.as_slice())
        .unwrap_or(&[]);

    let mut active = Vec::new();
    for card in active_cards(world) {
        if !visible_to_player(world, &card.active.key) {
            continue;
        }
        let Some(def) = content_db.0.situations.get(&card.active.key.definition) else {
            continue;
        };
        let stage = card.projection.as_ref().and_then(|projection| {
            def.stages
                .iter()
                .find(|stage| stage.key == projection.stage)
        });
        let mut actions = Vec::new();
        if let Some(projection) = &card.projection {
            for action in &projection.actions {
                let Some(authored) = def
                    .actions
                    .iter()
                    .find(|candidate| candidate.key == action.id)
                else {
                    continue;
                };
                let (view, unavailable) = match (player, action.leader) {
                    (Some(_), Some(leader)) => {
                        match forecast_for_action(
                            world,
                            &card.active.key,
                            &action.id,
                            leader,
                            action.target,
                        ) {
                            Ok(view) => {
                                let unavailable = view.blocked.as_ref().map(ToString::to_string);
                                (Some(view), unavailable)
                            }
                            Err(error) => (None, Some(error.to_string())),
                        }
                    }
                    (Some(_), None) => (
                        None,
                        Some(
                            world
                                .resource::<aeon_sim::TextDb>()
                                .text("ui.situations.no-leader")
                                .to_owned(),
                        ),
                    ),
                    // Spectators see the authored card but cannot issue orders.
                    (None, _) => (None, None),
                };
                actions.push(SituationActionView {
                    action: action.clone(),
                    assignment: authored.assignment.clone(),
                    label: authored.label.clone(),
                    forecast: view,
                    unavailable,
                });
            }
        }
        // A recorded answer is durable authoritative state; the open
        // responses are the authored choices minus that possibility. Both
        // read the simulation — the client owns no answer rule of its own.
        let answer = recorded_answer(world, &card.active.key).map(|answer| {
            def.responses
                .iter()
                .find(|declared| declared.key == answer)
                .map(|declared| declared.label.clone())
                .unwrap_or_else(|| answer.to_string())
        });
        let responses = if answer.is_none() && player.is_some() && card.unavailable.is_none() {
            def.responses
                .iter()
                .map(|declared| SituationResponseView {
                    id: declared.key.clone(),
                    label: declared.label.clone(),
                })
                .collect()
        } else {
            Vec::new()
        };
        active.push(ActiveSituationView {
            stage_title: stage.map(|stage| stage.title.clone()),
            stage_summary: stage.map(|stage| stage.summary.clone()),
            warning: stage.and_then(|stage| stage.warning.clone()),
            history: tagged_history(log_entries, &card.active.occurrence()),
            card,
            actions,
            responses,
            answer,
        });
    }

    let resolutions: Vec<SituationResolutionView> = world
        .get_resource::<SituationState>()
        .map(|state| {
            newest_resolutions(&state.resolutions)
                .filter(|notice| visible_to_player(world, &notice.situation))
                .map(|notice| SituationResolutionView {
                    history: tagged_history(log_entries, &notice.occurrence()),
                    resolution: notice.clone(),
                })
                .collect()
        })
        .unwrap_or_default();
    let live_resolution_ids: BTreeSet<u64> = resolutions
        .iter()
        .map(|notice| notice.resolution.id)
        .collect();

    world.insert_resource(SituationPanelView {
        active,
        resolutions,
    });
    if let Some(mut ui) = world.get_resource_mut::<SituationUiState>() {
        ui.dismissed
            .retain(|notice| live_resolution_ids.contains(notice));
    }
}

/// Draws completed resolutions first, then warnings, then all other cards.
pub fn draw_situations_panel(ui: &mut egui::Ui, ctx: &PanelCtx, out: &mut PanelOut) {
    let has_resolution = ctx
        .situations
        .resolutions
        .iter()
        .any(|notice| !out.situation_ui.dismissed.contains(&notice.resolution.id));
    if !has_resolution && ctx.situations.active.is_empty() {
        ui.weak(ctx.strings.text("ui.situations.empty"));
        return;
    }

    draw_vertical_scroll(ui, "situations-scroll", |ui| {
        for notice in &ctx.situations.resolutions {
            if out.situation_ui.dismissed.contains(&notice.resolution.id) {
                continue;
            }
            draw_resolution(ui, notice, ctx, out);
            ui.add_space(8.0);
        }
        for card in &ctx.situations.active {
            draw_active_card(ui, card, ctx, out);
            ui.add_space(8.0);
        }
    });
}

fn draw_resolution(
    ui: &mut egui::Ui,
    view: &SituationResolutionView,
    ctx: &PanelCtx,
    out: &mut PanelOut,
) {
    let notice = &view.resolution;
    let resolved = ctx.strings.format(
        "ui.situations.resolved",
        &[("date", &notice.resolved.to_string())],
    );
    let title = ctx
        .content
        .situations
        .get(&notice.situation.definition)
        .map(|def| def.title.clone())
        .unwrap_or_else(|| resolved.clone());
    egui::Frame::group(ui.style()).show(ui, |ui| {
        let heading = ui.add(
            egui::Label::new(egui::RichText::new(&title).strong())
                .sense(egui::Sense::focusable_noninteractive()),
        );
        let occurrence = format!("{:?}@{}", notice.situation, notice.activated);
        crate::ui::keyboard::situation_resolution(
            ui,
            crate::ui::keyboard::LogicalFocus::new(format!(
                "resolution-summary:{occurrence}:{}",
                notice.id
            )),
            occurrence,
            crate::ui::keyboard::FocusBand::Right,
            &heading,
        )
        .register();
        #[cfg(test)]
        crate::ui::rendered_state::record_response(ui, "resolution-summary", &heading);
        ui.weak(resolved);
        ui.separator();
        ui.label(&notice.text);
        draw_links(
            ui,
            ctx,
            out,
            &notice.participants,
            "ui.situations.participants",
        );
        draw_participant_groups(ui, ctx, out, &notice.participant_groups);
        draw_links(ui, ctx, out, &notice.links, "ui.situations.links");
        draw_history(ui, ctx, &view.history);
        let dismiss = ctx.player_org.is_some().then(|| {
            let response = draw_wrapped_action(ui, true, ctx.strings.text("ui.situations.dismiss"));
            crate::ui::keyboard::capture_action(
                ui,
                crate::ui::keyboard::LogicalFocus::new(format!("resolution-dismiss:{}", notice.id)),
                "resolution-dismiss",
                crate::ui::keyboard::FocusBand::Right,
                &response,
            )
            .register();
            #[cfg(test)]
            crate::ui::rendered_state::record_response(ui, "resolution-dismiss", &response);
            response
        });
        if dismiss.is_some_and(|response| response.clicked()) {
            out.situation_ui.dismissed.insert(notice.id);
            out.queue.0.push(PlayerCommand::DismissSituationResolution {
                resolution: notice.id,
            });
        }
    });
}

fn draw_active_card(
    ui: &mut egui::Ui,
    view: &ActiveSituationView,
    ctx: &PanelCtx,
    out: &mut PanelOut,
) {
    let focused = out.situation_ui.focused.as_ref() == Some(&view.card.active.key);
    let warns = view.warns();
    let mut frame = egui::Frame::group(ui.style());
    if warns || focused {
        frame = frame.stroke(egui::Stroke::new(
            if focused { 2.0 } else { 1.0 },
            egui::Color32::from(ctx.data.theme.semantics.urgent),
        ));
    }
    let response = frame
        .show(ui, |ui| {
            ui.strong(&view.card.title);
            ui.label(&view.card.summary);
            if let Some(stage) = &view.stage_title {
                ui.separator();
                ui.strong(stage);
            }
            if let Some(summary) = &view.stage_summary {
                ui.label(summary);
            }
            if warns {
                let warning = view.warning.as_deref().unwrap_or(&view.card.summary);
                ui.colored_label(
                    egui::Color32::from(ctx.data.theme.semantics.urgent),
                    warning,
                );
            }
            draw_guidance(ui, view, ctx, out);
            if let Some(reason) = &view.card.unavailable {
                ui.colored_label(
                    egui::Color32::from(ctx.data.theme.semantics.urgent),
                    ctx.strings
                        .format("ui.situations.unavailable", &[("reason", reason)]),
                );
                return;
            }
            let Some(projection) = &view.card.projection else {
                return;
            };
            if let Some(deadline) = projection.deadline {
                ui.weak(
                    ctx.strings
                        .format("ui.situations.deadline", &[("date", &deadline.to_string())]),
                );
            }
            for metric in &projection.metrics {
                let value = match &metric.value {
                    SituationMetricValue::Integer(value) => value.to_string(),
                    SituationMetricValue::Text(value) => value.clone(),
                };
                draw_wrapped_fact(ui, ctx.strings.text(&metric.label_key), &value);
            }
            draw_links(
                ui,
                ctx,
                out,
                &projection.participants,
                "ui.situations.participants",
            );
            draw_participant_groups(ui, ctx, out, &projection.participant_groups);
            draw_links(ui, ctx, out, &projection.links, "ui.situations.links");
            draw_history(ui, ctx, &view.history);

            if ctx.player_org.is_some() && !view.actions.is_empty() {
                ui.separator();
                ui.strong(ctx.strings.text("ui.situations.actions"));
                let mut responses = Vec::new();
                for action in &view.actions {
                    responses.push(draw_action(
                        ui,
                        &view.card.active.key,
                        view.card.active.activated,
                        action,
                        ctx,
                        out,
                    ));
                }
                crate::ui::keyboard::roving_group(ui, &responses);
            }
            draw_responses(ui, view, ctx, out);
        })
        .response;
    if focused {
        response.scroll_to_me(Some(egui::Align::Center));
        out.situation_ui.focused = None;
    }
}

/// A card's pure recorded answer surface: either the durable answer already
/// given, or the authored response choices still open.
///
/// Answering queues an ordinary logged command — the client records no
/// choice of its own, and the buttons simply stop being offered once the
/// authoritative answer exists.
fn draw_responses(
    ui: &mut egui::Ui,
    view: &ActiveSituationView,
    ctx: &PanelCtx,
    out: &mut PanelOut,
) {
    if let Some(answer) = &view.answer {
        ui.separator();
        ui.weak(
            ctx.strings
                .format("ui.situations.answered", &[("answer", answer)]),
        );
        return;
    }
    if ctx.player_org.is_none() || view.responses.is_empty() {
        return;
    }
    ui.separator();
    ui.strong(ctx.strings.text("ui.situations.responses"));
    let occurrence = format!("{:?}@{}", view.card.active.key, view.card.active.activated);
    ui.horizontal_wrapped(|ui| {
        let mut responses = Vec::new();
        for response in &view.responses {
            let button = draw_wrapped_action(ui, true, response.label.clone());
            crate::ui::keyboard::capture_action(
                ui,
                crate::ui::keyboard::LogicalFocus::new(format!(
                    "situation-response:{occurrence}:{}",
                    response.id
                )),
                "situation-response",
                crate::ui::keyboard::FocusBand::Right,
                &button,
            )
            .register();
            #[cfg(test)]
            crate::ui::rendered_state::record_response(ui, "situation-response", &button);
            if button.clicked() {
                out.queue.0.push(PlayerCommand::AnswerSituation {
                    situation: view.card.active.key.clone(),
                    response: response.id.clone(),
                });
            }
            responses.push(button);
        }
        crate::ui::keyboard::roving_group(ui, &responses);
    });
}

/// Optional authored guidance under a card: an objective line plus the
/// "Show me how" and "Why this matters" help routes.
///
/// Purely additive presentation gated by the client-owned guidance
/// preference: the prose is authored content on the Situation definition,
/// the triggers reuse the shared pinnable-explanation surface, and nothing
/// here reads or writes simulation rules, commands, or state.
fn draw_guidance(
    ui: &mut egui::Ui,
    view: &ActiveSituationView,
    ctx: &PanelCtx,
    out: &mut PanelOut,
) {
    if !ctx.guidance {
        return;
    }
    let Some(def) = ctx.content.situations.get(&view.card.active.key.definition) else {
        return;
    };
    let has_help = def.guidance_how.is_some() || def.guidance_why.is_some();
    if def.guidance_objective.is_none() && !has_help {
        return;
    }
    ui.separator();
    ui.strong(ctx.strings.text("ui.situations.guidance-heading"));
    if let Some(objective) = &def.guidance_objective {
        ui.label(objective);
    }
    if !has_help {
        return;
    }
    let occurrence = format!("{:?}@{}", view.card.active.key, view.card.active.activated);
    ui.horizontal_wrapped(|ui| {
        for (kind, label_key, prose) in [
            ("how", "ui.situations.show-how", &def.guidance_how),
            ("why", "ui.situations.why-matters", &def.guidance_why),
        ] {
            let Some(prose) = prose else {
                continue;
            };
            let topic = ExplanationTopic {
                title: format!("{} — {}", view.card.title, ctx.strings.text(label_key)),
                summary: prose.clone(),
                forecast: None,
            };
            let response = crate::ui::explanations::labelled_explanation_trigger(
                ui,
                &ctx.data.theme,
                ctx.strings,
                ctx.strings.text(label_key),
                &topic,
                out.explanations,
                crate::ui::keyboard::LogicalFocus::new(format!(
                    "situation-guidance:{occurrence}:{kind}"
                )),
                crate::ui::keyboard::FocusBand::Right,
            );
            #[cfg(test)]
            crate::ui::rendered_state::record_response(ui, "situation-guidance", &response);
            #[cfg(not(test))]
            let _ = response;
        }
    });
}

/// Exact structural tags are the only membership test. Iterating the
/// chronological log backwards gives newest-first presentation without a
/// second sort, and the cap keeps every card compact.
fn newest_resolutions(
    resolutions: &[SituationResolution],
) -> impl Iterator<Item = &SituationResolution> {
    resolutions.iter().rev()
}

fn tagged_history(entries: &[LogEntry], occurrence: &SituationOccurrence) -> Vec<LogEntry> {
    let war = situation_war(&occurrence.situation);
    entries
        .iter()
        .rev()
        .filter(|entry| {
            entry.situations.iter().any(|tag| tag == occurrence)
                || war.is_some_and(|war| entry.war == Some(war))
        })
        .take(HISTORY_LIMIT)
        .cloned()
        .collect()
}

fn draw_history(ui: &mut egui::Ui, ctx: &PanelCtx, entries: &[LogEntry]) {
    if entries.is_empty() {
        return;
    }
    ui.separator();
    ui.strong(ctx.strings.text("ui.situations.history"));
    for entry in entries {
        ui.horizontal_wrapped(|ui| {
            ui.weak(entry.date.to_string());
            ui.label(&entry.text);
        });
    }
}

fn draw_action(
    ui: &mut egui::Ui,
    situation: &SituationInstanceKey,
    activated: aeon_core::calendar::GameDate,
    view: &SituationActionView,
    ctx: &PanelCtx,
    out: &mut PanelOut,
) -> egui::Response {
    let enabled = view.unavailable.is_none() && view.action.leader.is_some();
    let label = action_label(ctx, view);
    let mut response = draw_wrapped_action(ui, enabled, label.clone());
    let occurrence = format!("{situation:?}@{activated}");
    crate::ui::keyboard::situation_action(
        ui,
        crate::ui::keyboard::LogicalFocus::new(format!(
            "situation-action:{occurrence}:{}",
            view.action.id
        )),
        occurrence.clone(),
        view.action.id.to_string(),
        crate::ui::keyboard::FocusBand::Right,
        &response,
    )
    .register();
    #[cfg(test)]
    if view.action.id.as_str() == "call-favour" {
        crate::ui::rendered_state::record_response(ui, "situation-action", &response);
    }
    #[cfg(test)]
    if view.action.id.as_str() == "call-favour" {
        ui.ctx().data_mut(|data| {
            data.insert_temp(egui::Id::new(SITUATION_ACTION_RESPONSE), response.rect);
            data.insert_temp(egui::Id::new(SITUATION_ACTION_HOVERED), response.hovered());
        });
    }
    if let Some(reason) = &view.unavailable {
        response = response.on_disabled_hover_text(reason);
        ui.weak(
            ctx.strings
                .format("ui.situations.unavailable", &[("reason", reason)]),
        );
    } else if let Some(forecast) = &view.forecast {
        let topic = ExplanationTopic {
            title: label,
            summary: forecast_summary(ctx.strings, forecast),
            forecast: Some(forecast.clone()),
        };
        response = preview_for_response(response, &ctx.data.theme, ctx.strings, &topic);
        #[cfg(test)]
        if view.action.id.as_str() == "call-favour" && (response.hovered() || response.has_focus())
        {
            ui.ctx().data_mut(|data| {
                data.insert_temp(egui::Id::new(SITUATION_FORECAST_VISIBLE), true);
            });
        }
        ui.horizontal_wrapped(|ui| {
            ui.weak(forecast_summary(ctx.strings, forecast));
            explanation_trigger(
                ui,
                &ctx.data.theme,
                ctx.strings,
                &topic,
                out.explanations,
                crate::ui::keyboard::LogicalFocus::new(format!(
                    "situation-explanation:{occurrence}:{}",
                    view.action.id
                )),
                crate::ui::keyboard::FocusBand::Right,
            );
        });
    }
    if response.clicked()
        && let Some(leader) = view.action.leader
    {
        out.form.reset();
        out.form.assignment = Some(view.assignment.clone());
        out.form.leader = Some(leader);
        out.form.target = Some(view.action.target);
        out.form.situation = Some(crate::assignment_ui::SituationAssignmentContext {
            situation: situation.clone(),
            action: view.action.id.clone(),
            war: aeon_sim::situations::action_war(situation, view.action.target),
        });
        out.popup
            .open_from(crate::ui::keyboard::LogicalFocus::new(format!(
                "situation-action:{situation:?}@{activated}:{}",
                view.action.id
            )));
    }
    response
}

fn action_label(ctx: &PanelCtx, view: &SituationActionView) -> String {
    if view.action.context.is_empty() {
        return view.label.clone();
    }
    let context = view
        .action
        .context
        .iter()
        .map(|link| link_label(ctx, link))
        .collect::<Vec<_>>();
    contextual_action_label(ctx.strings, &view.label, &context)
}

fn contextual_action_label(strings: &aeon_sim::TextDb, action: &str, context: &[String]) -> String {
    let context = context.join(" → ");
    strings.format(
        "ui.situations.action-context",
        &[("action", action), ("context", &context)],
    )
}

fn draw_participant_groups(
    ui: &mut egui::Ui,
    ctx: &PanelCtx,
    out: &mut PanelOut,
    groups: &[SituationParticipantGroup],
) {
    for group in groups {
        draw_links(ui, ctx, out, &group.participants, &group.label_key);
    }
}

fn draw_links(
    ui: &mut egui::Ui,
    ctx: &PanelCtx,
    out: &mut PanelOut,
    links: &[SituationLink],
    heading_key: &str,
) {
    if links.is_empty() {
        return;
    }
    ui.horizontal_wrapped(|ui| {
        ui.weak(ctx.strings.text(heading_key));
        let mut responses = Vec::new();
        for link in links {
            let label = link_label(ctx, link);
            if let Some(selection) = selection_for(link) {
                let response = ui.add(egui::Button::new(label).wrap().frame(false));
                crate::ui::keyboard::capture_action(
                    ui,
                    crate::ui::keyboard::LogicalFocus::new(format!(
                        "situation-subject:{:?}:{}",
                        link.kind, link.id
                    )),
                    "situation-subject",
                    crate::ui::keyboard::FocusBand::Right,
                    &response,
                )
                .register();
                responses.push(response.clone());
                if response.clicked() {
                    if let Selection::Province(province) = selection
                        && let Some((record, ..)) = ctx
                            .data
                            .provinces
                            .iter()
                            .find(|(record, ..)| record.id == province)
                    {
                        out.view.view = MapView::Body(record.body);
                    }
                    out.view.selected = Some(selection);
                }
            } else {
                ui.label(label);
            }
        }
        crate::ui::keyboard::roving_group(ui, &responses);
    });
}

fn link_label(ctx: &PanelCtx, link: &SituationLink) -> String {
    if let Some(key) = &link.label_key
        && let Some(label) = ctx.strings.0.get(key)
    {
        return label.to_owned();
    }
    match link.kind {
        SituationSubjectKind::Scenario => link.id.to_string(),
        SituationSubjectKind::Body => BodyId::from_raw(link.id)
            .map(|id| ctx.lookup.body_name(id).to_owned())
            .unwrap_or_else(|| link.id.to_string()),
        SituationSubjectKind::Province => ProvinceId::from_raw(link.id)
            .map(|id| ctx.lookup.province_name(id))
            .unwrap_or_else(|| link.id.to_string()),
        SituationSubjectKind::Character => CharacterId::from_raw(link.id)
            .map(|id| ctx.lookup.char_name(id))
            .unwrap_or_else(|| link.id.to_string()),
        SituationSubjectKind::Organisation => OrgId::from_raw(link.id)
            .map(|id| ctx.lookup.org_display(id))
            .unwrap_or_else(|| link.id.to_string()),
        SituationSubjectKind::Title => TitleId::from_raw(link.id)
            .and_then(|id| ctx.data.titles.iter().find(|record| record.id == id))
            .map(|record| record.name.clone())
            .unwrap_or_else(|| link.id.to_string()),
        SituationSubjectKind::Office => OfficeId::from_raw(link.id)
            .and_then(|id| ctx.data.offices.iter().find(|record| record.id == id))
            .map(|record| record.name.clone())
            .unwrap_or_else(|| link.id.to_string()),
        SituationSubjectKind::Army => ArmyId::from_raw(link.id)
            .and_then(|id| ctx.data.armies.iter().find(|record| record.id == id))
            .map(|record| record.name.clone())
            .unwrap_or_else(|| link.id.to_string()),
        SituationSubjectKind::Ship => ShipId::from_raw(link.id)
            .and_then(|id| ctx.data.ships.iter().find(|record| record.id == id))
            .map(|record| record.name.clone())
            .unwrap_or_else(|| link.id.to_string()),
        SituationSubjectKind::Assignment => AssignmentId::from_raw(link.id)
            .and_then(|id| {
                ctx.data
                    .active_assignments
                    .iter()
                    .find(|assignment| assignment.id == id)
            })
            .and_then(|assignment| ctx.content.assignments.get(&assignment.def))
            .map(|def| def.title.clone())
            .unwrap_or_else(|| link.id.to_string()),
        SituationSubjectKind::Obligation | SituationSubjectKind::War => link.id.to_string(),
    }
}

fn selection_for(link: &SituationLink) -> Option<Selection> {
    match link.kind {
        SituationSubjectKind::Body => BodyId::from_raw(link.id).map(Selection::Body),
        SituationSubjectKind::Province => ProvinceId::from_raw(link.id).map(Selection::Province),
        SituationSubjectKind::Character => CharacterId::from_raw(link.id).map(Selection::Character),
        SituationSubjectKind::Organisation => OrgId::from_raw(link.id).map(Selection::Org),
        SituationSubjectKind::Army => ArmyId::from_raw(link.id).map(Selection::Army),
        SituationSubjectKind::Ship => ShipId::from_raw(link.id).map(Selection::Ship),
        SituationSubjectKind::Scenario
        | SituationSubjectKind::Title
        | SituationSubjectKind::Office
        | SituationSubjectKind::Assignment
        | SituationSubjectKind::Obligation
        | SituationSubjectKind::War => None,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use aeon_core::calendar::GameDate;
    use aeon_data::ContentKey;
    use aeon_sim::situations::{SituationSource, SituationSubject};
    use aeon_sim::{LogChannel, WarId};

    use super::*;

    fn key(source: &str) -> SituationInstanceKey {
        SituationInstanceKey {
            definition: ContentKey::new("formal-war").unwrap(),
            source: SituationSource {
                kind: SituationSubjectKind::Scenario,
                key: ContentKey::new(source).unwrap(),
                id: None,
            },
            bindings: BTreeMap::new(),
        }
    }

    fn occurrence(situation: SituationInstanceKey, activated: i64) -> SituationOccurrence {
        SituationOccurrence {
            situation,
            activated: GameDate::from_days(activated),
        }
    }

    #[test]
    fn history_is_exact_newest_first_and_compact() {
        let structural_key = key("ashkarr-succession");
        let wanted = occurrence(structural_key.clone(), 1);
        let repeated_key = occurrence(structural_key, 20);
        let other_lifecycle = occurrence(key("another-scenario"), 1);
        let mut entries = vec![
            LogEntry::new(
                GameDate::from_days(1),
                "not this lifecycle",
                LogChannel::Military,
            )
            .for_situation(other_lifecycle),
            LogEntry::new(
                GameDate::from_days(20),
                "same structural key, later activation",
                LogChannel::Military,
            )
            .for_situation(repeated_key),
        ];
        for day in 1..=8 {
            entries.push(
                LogEntry::new(
                    GameDate::from_days(day + 1),
                    format!("wanted-{day}"),
                    LogChannel::Military,
                )
                .for_situation(wanted.clone()),
            );
        }

        let history = tagged_history(&entries, &wanted);
        assert_eq!(history.len(), HISTORY_LIMIT);
        assert_eq!(
            history
                .iter()
                .map(|entry| entry.text.as_str())
                .collect::<Vec<_>>(),
            [
                "wanted-8", "wanted-7", "wanted-6", "wanted-5", "wanted-4", "wanted-3",
            ]
        );
        assert!(
            history
                .iter()
                .all(|entry| entry.situations.iter().any(|tag| tag == &wanted)),
            "another activation or structural key must never leak into the card"
        );
    }

    #[test]
    fn formal_war_history_includes_exact_war_logs_without_cross_war_bleed() {
        let first_war = WarId::from_raw(41).unwrap();
        let second_war = WarId::from_raw(42).unwrap();
        let mut first_key = key("ashkarr-succession");
        first_key
            .bindings
            .insert("war".to_owned(), SituationSubject::War(first_war));
        let first = occurrence(first_key, 1);
        let mut second_key = key("ashkarr-succession");
        second_key
            .bindings
            .insert("war".to_owned(), SituationSubject::War(second_war));
        let second = occurrence(second_key, 20);
        let entries = vec![
            LogEntry::new(
                GameDate::from_days(2),
                "first AI siege",
                LogChannel::Military,
            )
            .for_war(first_war),
            LogEntry::new(
                GameDate::from_days(3),
                "second AI siege",
                LogChannel::Military,
            )
            .for_war(second_war),
            LogEntry::new(
                GameDate::from_days(4),
                "first resolution",
                LogChannel::Events,
            )
            .for_situation(first.clone()),
            LogEntry::new(
                GameDate::from_days(21),
                "second resolution",
                LogChannel::Events,
            )
            .for_situation(second),
        ];

        assert_eq!(
            tagged_history(&entries, &first)
                .iter()
                .map(|entry| entry.text.as_str())
                .collect::<Vec<_>>(),
            ["first resolution", "first AI siege"]
        );
    }

    #[test]
    fn resolutions_are_presented_newest_first() {
        let situation = key("ashkarr-succession");
        let notice = |id, activated, resolved| SituationResolution {
            id,
            situation: situation.clone(),
            activated: GameDate::from_days(activated),
            resolved: GameDate::from_days(resolved),
            outcome: ContentKey::new("ended").unwrap(),
            text: format!("notice-{id}"),
            participants: Vec::new(),
            participant_groups: Vec::new(),
            links: Vec::new(),
        };
        let notices = vec![notice(1, 1, 5), notice(2, 10, 15), notice(3, 20, 25)];

        assert_eq!(
            newest_resolutions(&notices)
                .map(|notice| notice.id)
                .collect::<Vec<_>>(),
            vec![3, 2, 1]
        );
    }

    #[test]
    fn repeated_action_labels_name_force_and_objective() {
        let label = contextual_action_label(
            &aeon_sim::TextDb::embedded(),
            "Besiege",
            &["First Levy".to_owned(), "Cindral".to_owned()],
        );

        assert!(label.contains("Besiege"));
        assert!(label.contains("First Levy → Cindral"));
    }
}
