//! Rendering for a single [`JobForecast`].
//!
//! The simulation's forecast is shown in two places — expanded under an
//! action in the inspector, and under the cursor when comparing candidates
//! in the character picker. Both call [`draw_forecast_body`], so the
//! figures a player compares candidates on are drawn by the same code, from
//! the same object, as the figures they finally commit to.
//!
//! Nothing here computes anything: every number arrives in the forecast.

use aeon_data::model::JobResultKind;
use aeon_sim::forecast::{JobForecast, Permille};
use bevy_egui::egui;

use crate::ui::theme::{TargetState, UiTheme};

/// A permille figure as a percentage with one decimal place.
pub fn permille_text(value: Permille) -> String {
    format!("{}.{}%", value / 10, value % 10)
}

/// The name of a graded outcome; its colour comes from the theme.
pub fn result_label(kind: JobResultKind) -> &'static str {
    match kind {
        JobResultKind::CriticalSuccess => "Critical success",
        JobResultKind::Success => "Success",
        JobResultKind::Failure => "Failure",
        JobResultKind::Disaster => "Disaster",
    }
}

/// Draws what an action costs, how long it takes, the exact odds it would
/// roll now, what each outcome does, and what the leader personally risks.
///
/// Draws bare, without a frame of its own, so the caller decides whether it
/// sits in a group box or in a tooltip.
pub fn draw_forecast_body(ui: &mut egui::Ui, theme: &UiTheme, view: &JobForecast) {
    // Timing.
    ui.horizontal_wrapped(|ui| {
        ui.label(format!("Takes {} days", view.duration_days))
            .on_hover_text(
                "Days from the order taking effect until it resolves. A march \
                 takes at least as long as the army needs to reach its objective.",
            );
        if view.order_delay_days > 0 {
            ui.label(format!("· begins in {}d", view.order_delay_days))
                .on_hover_text(
                    "Your order has to physically reach the leader first. \
                     Distance from your head, and any travel in progress, add \
                     this delay before the job even starts.",
                );
        }
    });

    // Immediate costs.
    let mut costs = Vec::new();
    if view.wealth_cost > 0 {
        costs.push(format!("W {}", view.wealth_cost));
    }
    if view.manpower_cost > 0 {
        costs.push(format!("M {}", view.manpower_cost));
    }
    if view.supplies_cost > 0 {
        costs.push(format!("S {}", view.supplies_cost));
    }
    if view.influence_cost > 0 {
        costs.push(format!("I {}", view.influence_cost));
    }
    if !costs.is_empty() {
        ui.label(format!("Costs {}", costs.join(" · ")))
            .on_hover_text(
                "Taken from your house's stores the moment the job begins. \
                 This is spent whatever the outcome — a guaranteed cost, not \
                 a risk.",
            );
    }

    // The skill contest behind the odds.
    ui.label(format!(
        "{:?} {} vs difficulty {} → {:+}",
        view.skill, view.skill_value, view.difficulty, view.effectiveness
    ))
    .on_hover_text(
        "The leader's governing skill against the job's authored difficulty. \
         Each point of advantage shifts weight out of the bad outcomes and \
         into the good ones; each point of deficit does the reverse.",
    );

    ui.separator();
    ui.label("If ordered now").on_hover_text(
        "The exact outcome distribution the simulation would roll against \
         today. It moves with the leader you choose and their skill.",
    );
    for result in &view.results {
        let label = result_label(result.kind);
        let colour = theme.semantics.outcome(result.kind);
        ui.horizontal(|ui| {
            ui.colored_label(colour, permille_text(result.chance));
            let mut text = label.to_owned();
            if result.popup {
                text.push_str("  (asks you)");
            }
            let response = ui.label(text);
            match &result.text {
                Some(detail) => {
                    response.on_hover_text(detail);
                }
                None => {
                    response.on_hover_text("No authored consequence for this outcome.");
                }
            }
        });
    }

    // Personal risks: conditional on a bad outcome, not on the order.
    for risk in &view.risks {
        ui.colored_label(
            theme.semantics.target(TargetState::NotInteractable),
            format!(
                "{:?} risk — {} on a failure, {} on a disaster",
                risk.tag,
                permille_text(risk.on_failure),
                permille_text(risk.on_disaster)
            ),
        )
        .on_hover_text(
            "A personal consequence for the leader, rolled only if the job \
             goes badly. It is conditional on the outcome above, not an \
             additional chance on the order itself.",
        );
    }

    // A military operation is settled after the roll, not by it.
    if let Some(op) = view.military_op {
        ui.colored_label(
            theme.semantics.target(TargetState::AlreadyDoing),
            format!("Then contested in the field ({op:?})"),
        )
        .on_hover_text(
            "These chances cover the order itself. Even a successful order is \
             then decided by the operation — the strength, supply and order of \
             the forces present settle it, and that contest is deliberately \
             not folded into the percentages above.",
        );
    }

    if let Some(reason) = &view.blocked {
        ui.colored_label(
            theme.semantics.target(TargetState::IneligibleFixable),
            format!("Cannot start: {reason}"),
        );
    }
}
