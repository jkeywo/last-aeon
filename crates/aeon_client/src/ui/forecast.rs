//! Rendering for a single [`AssignmentForecast`].
//!
//! The simulation's forecast is shown in two places — expanded under an
//! action in the inspector, and under the cursor when comparing candidates
//! in the character picker. Both call [`draw_forecast_body`], so the
//! figures a player compares candidates on are drawn by the same code, from
//! the same object, as the figures they finally commit to.
//!
//! Nothing here computes anything: every number arrives in the forecast.

use aeon_data::model::OutcomeKind;
use aeon_sim::TextDb;
use aeon_sim::forecast::{AssignmentForecast, Permille};
use bevy_egui::egui;

use crate::ui::theme::{TargetState, UiTheme};

/// A permille figure as a percentage with one decimal place.
pub fn permille_text(value: Permille) -> String {
    format!("{}.{}%", value / 10, value % 10)
}

/// The name of a graded outcome; its colour comes from the theme.
pub fn result_label_key(kind: OutcomeKind) -> &'static str {
    match kind {
        OutcomeKind::CriticalSuccess => "ui.result.critical-success",
        OutcomeKind::Success => "ui.result.success",
        OutcomeKind::Failure => "ui.result.failure",
        OutcomeKind::Disaster => "ui.result.disaster",
    }
}

/// Draws what an action costs, how long it takes, the exact odds it would
/// roll now, what each outcome does, and what the leader personally risks.
///
/// Draws bare, without a frame of its own, so the caller decides whether it
/// sits in a group box or in a tooltip.
pub fn draw_forecast_body(
    ui: &mut egui::Ui,
    theme: &UiTheme,
    strings: &TextDb,
    view: &AssignmentForecast,
) {
    // Timing.
    ui.horizontal_wrapped(|ui| {
        ui.label(strings.format(
            "ui.forecast.duration",
            &[("days", &view.duration_days.to_string())],
        ))
        .on_hover_text(strings.text("ui.forecast.duration.hover"));
        if view.order_delay_days > 0 {
            ui.label(strings.format(
                "ui.forecast.delay",
                &[("days", &view.order_delay_days.to_string())],
            ))
            .on_hover_text(strings.text("ui.forecast.delay.hover"));
        }
    });

    // Immediate costs.
    let mut costs = Vec::new();
    if view.wealth_cost > 0 {
        costs.push(strings.format(
            "ui.cost.wealth",
            &[("amount", &view.wealth_cost.to_string())],
        ));
    }
    if view.manpower_cost > 0 {
        costs.push(strings.format(
            "ui.cost.manpower",
            &[("amount", &view.manpower_cost.to_string())],
        ));
    }
    if view.supplies_cost > 0 {
        costs.push(strings.format(
            "ui.cost.supplies",
            &[("amount", &view.supplies_cost.to_string())],
        ));
    }
    if view.influence_cost > 0 {
        costs.push(strings.format(
            "ui.cost.influence",
            &[("amount", &view.influence_cost.to_string())],
        ));
    }
    if !costs.is_empty() {
        ui.label(strings.format("ui.forecast.costs", &[("costs", &costs.join(" · "))]))
            .on_hover_text(strings.text("ui.forecast.costs.hover"));
    }

    // The skill contest behind the odds.
    ui.label(strings.format(
        "ui.forecast.contest",
        &[
            ("skill", strings.text(view.skill.label_key())),
            ("value", &view.skill_value.to_string()),
            ("difficulty", &view.difficulty.to_string()),
            ("effect", &format!("{:+}", view.effectiveness)),
        ],
    ))
    .on_hover_text(strings.text("ui.forecast.contest.hover"));

    ui.separator();
    ui.label(strings.text("ui.forecast.outcomes"))
        .on_hover_text(strings.text("ui.forecast.outcomes.hover"));
    for result in &view.results {
        let label = strings.text(result_label_key(result.kind));
        let colour = theme.semantics.outcome(result.kind);
        ui.horizontal(|ui| {
            ui.colored_label(colour, permille_text(result.chance));
            let text = if result.popup {
                strings.format("ui.forecast.result.asks-you", &[("result", label)])
            } else {
                label.to_owned()
            };
            let response = ui.label(text);
            match &result.text {
                Some(detail) => {
                    response.on_hover_text(detail);
                }
                None => {
                    response.on_hover_text(strings.text("ui.forecast.result.unauthored"));
                }
            }
        });
    }

    // Personal risks: conditional on a bad outcome, not on the order.
    for risk in &view.risks {
        ui.colored_label(
            theme.semantics.target(TargetState::NotInteractable),
            strings.format(
                "ui.forecast.risk",
                &[
                    ("risk", strings.text(risk.tag.label_key())),
                    ("on_failure", &permille_text(risk.on_failure)),
                    ("on_disaster", &permille_text(risk.on_disaster)),
                ],
            ),
        )
        .on_hover_text(strings.text("ui.forecast.risk.hover"));
    }

    // A military operation is settled after the roll, not by it.
    if let Some(op) = view.military_op {
        ui.colored_label(
            theme.semantics.target(TargetState::AlreadyDoing),
            strings.format(
                "ui.forecast.military-op",
                &[("operation", strings.text(op.label_key()))],
            ),
        )
        .on_hover_text(strings.text("ui.forecast.military-op.hover"));
    }

    // What committing actually commits to.
    match view.point_of_no_return {
        Some(0) => {
            ui.colored_label(
                theme.semantics.target(TargetState::NotInteractable),
                strings.text("ui.forecast.no-recall"),
            );
        }
        Some(day) => {
            ui.label(strings.format("ui.forecast.recall-until", &[("days", &day.to_string())]))
                .on_hover_text(strings.text("ui.forecast.recall-until.hover"));
        }
        None => {}
    }

    if let Some(reason) = &view.blocked {
        ui.colored_label(
            theme.semantics.target(TargetState::IneligibleFixable),
            strings.format("ui.forecast.blocked", &[("reason", &reason.to_string())]),
        );
    }
}
