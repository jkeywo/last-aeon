//! The character picker: choosing who leads an action.
//!
//! Deliberately **not** a modal. Choosing a leader is comparison work —
//! read a candidate, weigh them against another, change your mind — and a
//! window that closes the moment you touch a name forces that comparison to
//! be redone from scratch each time. Selecting a candidate here writes the
//! choice and leaves the window standing.
//!
//! Every adult of the house appears, including those who cannot take the
//! job on. They are listed below a divider rather than mixed in among those
//! who can, each with the simulation's own account of where they are and
//! when they will be free — a household member who is merely busy should
//! never look like one who does not exist.
//!
//! Nothing is computed here. The candidate list, the odds, and the reasons
//! all arrive in [`ForecastCache`], which is filled by the simulation.

use aeon_sim::TextDb;
use aeon_sim::state::ContentDb;
use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};

use crate::forecast_view::{ForecastCache, LeaderOption};
use crate::jobs_ui::JobForm;
use crate::ui::forecast::{draw_forecast_body, permille_text};
use crate::ui::theme::{TargetState, UiTheme};

/// Whether the picker is up.
///
/// A resource rather than a flag inside [`JobForm`] because the window
/// outlives any single expanded action: closing an action should not leave
/// a stale window open, but changing which action is expanded while the
/// picker is up should just re-list the candidates for the new one.
#[derive(Resource, Default)]
pub struct PickerState {
    /// Whether the window is showing.
    pub open: bool,
}

impl PickerState {
    /// Opens the picker.
    pub fn open(&mut self) {
        self.open = true;
    }
}

/// Draws the picker window when it is open and an action is expanded.
pub fn draw_picker(
    mut contexts: EguiContexts,
    mut picker: ResMut<PickerState>,
    mut form: ResMut<JobForm>,
    cache: Res<ForecastCache>,
    content: Option<Res<ContentDb>>,
    theme: Res<UiTheme>,
    strings: Option<Res<TextDb>>,
) {
    if !picker.open {
        return;
    }
    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };
    // The picker is about an expanded action; with none expanded it has
    // nothing to be about, so it closes itself rather than showing an
    // empty frame.
    if form.job.is_none() {
        picker.open = false;
        return;
    }
    let (Some(content), Some(strings)) = (content, strings) else {
        return;
    };
    let strings = &*strings;

    let title = cache
        .forecast
        .as_ref()
        .map(|view| strings.format("ui.picker.title", &[("job", &view.title)]))
        .unwrap_or_else(|| strings.text("ui.picker.title.generic").to_owned());

    let mut open = true;
    egui::Window::new(title)
        .id(egui::Id::new("character-picker"))
        .open(&mut open)
        .resizable(true)
        .default_width(360.0)
        .show(ctx, |ui| {
            let (free, committed): (Vec<&LeaderOption>, Vec<&LeaderOption>) =
                cache.leaders.iter().partition(|o| o.blocked().is_none());

            let job_title = |key: &aeon_data::ContentKey| -> String {
                content
                    .0
                    .jobs
                    .get(key)
                    .map(|def| def.title.clone())
                    .unwrap_or_else(|| key.to_string())
            };

            egui::ScrollArea::vertical()
                .max_height(420.0)
                .show(ui, |ui| {
                    if free.is_empty() {
                        draw_empty_state(ui, &theme, strings, &committed);
                    }
                    for option in &free {
                        draw_candidate(ui, &theme, strings, &mut form, option);
                    }

                    if !committed.is_empty() {
                        ui.separator();
                        ui.weak(strings.text("ui.picker.committed"));
                        for option in &committed {
                            draw_committed(ui, &theme, strings, option, &job_title);
                        }
                    }
                });
        });
    if !open {
        picker.open = false;
    }
}

/// One candidate who could take the job on now.
fn draw_candidate(
    ui: &mut egui::Ui,
    theme: &UiTheme,
    strings: &TextDb,
    form: &mut JobForm,
    option: &LeaderOption,
) {
    let chosen = form.leader == Some(option.id);
    let mut label = strings.format(
        "ui.picker.candidate",
        &[
            ("name", &option.name),
            ("skill", &option.skill_value().to_string()),
            ("chance", &permille_text(option.success())),
        ],
    );
    if let Some(assignment) = &option.assignment {
        label.push_str(&strings.format(
            "ui.picker.candidate.assigned",
            &[("assignment", assignment)],
        ));
    }
    let state = if chosen {
        TargetState::AlreadyDoing
    } else {
        TargetState::Valid
    };
    let text = egui::RichText::new(label).color(theme.semantics.target(state));
    let response = ui
        .selectable_label(chosen, text)
        // The full breakdown is one hover away from the summary, and is the
        // same calculation the job will resolve with.
        .on_hover_ui(|ui| {
            ui.set_max_width(340.0);
            ui.strong(&option.name);
            if let Some(assignment) = &option.assignment {
                ui.weak(assignment);
            }
            ui.separator();
            draw_forecast_body(ui, theme, strings, &option.forecast);
        });
    // Selecting writes the choice and leaves the window open, so the next
    // candidate can be weighed against this one without reopening it.
    if response.clicked() {
        form.leader = Some(option.id);
    }
}

/// One household member who cannot take this job on.
fn draw_committed(
    ui: &mut egui::Ui,
    theme: &UiTheme,
    strings: &TextDb,
    option: &LeaderOption,
    job_title: &impl Fn(&aeon_data::ContentKey) -> String,
) {
    // The simulation's own account of where they are, which names the job
    // and when it ends rather than saying only that they are unavailable.
    let reason = option.availability.describe(job_title);
    // A refusal the player can lift reads differently from one they cannot.
    let state = match option.availability {
        aeon_sim::LeaderAvailability::Ineligible(_) => TargetState::StructurallyIneligible,
        _ => TargetState::IneligibleFixable,
    };
    ui.add_enabled(
        false,
        egui::Button::new(
            egui::RichText::new(strings.format(
                "ui.picker.committed.row",
                &[("name", &option.name), ("reason", &reason)],
            ))
            .color(theme.semantics.target(state)),
        )
        .frame(false),
    )
    .on_disabled_hover_text(strings.format(
        "ui.picker.committed.hover",
        &[
            ("name", &option.name),
            (
                "reason",
                option
                    .blocked()
                    .as_deref()
                    .unwrap_or_else(|| strings.text("ui.actions.unavailable")),
            ),
        ],
    ));
}

/// What to say when nobody is free.
///
/// An empty list must explain itself and offer a way out, so it names the
/// soonest date the household frees up rather than leaving the player to
/// work out whether waiting would even help.
fn draw_empty_state(
    ui: &mut egui::Ui,
    theme: &UiTheme,
    strings: &TextDb,
    committed: &[&LeaderOption],
) {
    if committed.is_empty() {
        ui.colored_label(
            theme.semantics.target(TargetState::StructurallyIneligible),
            strings.text("ui.picker.empty.nobody"),
        );
        return;
    }

    // The soonest a busy member comes free. Only Busy carries a date;
    // someone barred for good is no reason to wait.
    let soonest = committed
        .iter()
        .filter_map(|option| match option.availability {
            aeon_sim::LeaderAvailability::Busy { completes, .. } => Some(completes),
            aeon_sim::LeaderAvailability::Indisposed { until } => until,
            _ => None,
        })
        .min();

    ui.colored_label(
        theme.semantics.target(TargetState::IneligibleFixable),
        strings.text("ui.picker.empty.all-committed"),
    );
    match soonest {
        Some(date) => {
            ui.weak(strings.format(
                "ui.picker.empty.wait-until",
                &[("date", &date.to_string())],
            ));
        }
        None => {
            ui.weak(strings.text("ui.picker.empty.cancel-one"));
        }
    }
}
