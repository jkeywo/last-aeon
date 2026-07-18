//! Job UI: the bottom panel (message log + active jobs + start form),
//! result popups, and the queue that carries UI intents into the
//! authoritative command pipeline.

use aeon_data::ContentKey;
use aeon_data::model::JobTargetKind;
use aeon_sim::command::submit_command;
use aeon_sim::jobs::CharacterCondition;
use aeon_sim::map::DisplayName;
use aeon_sim::politics::ADULT_AGE;
use aeon_sim::state::ContentDb;
use aeon_sim::{
    ActiveJob, CampaignClock, CharacterId, CharacterRecord, JobTarget, MessageLog, OrgRecord,
    PendingPopups, PlayerCommand, PlayerHouse, PoliticsIndex, ProvinceRecord,
};
use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};

use crate::sim_driver::TimeControl;

/// Player intents collected by UI systems this frame, submitted as
/// commands by [`flush_ui_commands`].
#[derive(Resource, Default)]
pub struct UiCommandQueue(pub Vec<PlayerCommand>);

/// The start-job form's in-progress choices.
#[derive(Resource, Default)]
pub struct JobForm {
    /// Chosen job definition.
    pub job: Option<ContentKey>,
    /// Chosen leader.
    pub leader: Option<CharacterId>,
    /// Chosen target.
    pub target: Option<JobTarget>,
    /// Last rejection message, shown until the next attempt.
    pub notice: Option<String>,
}

/// Submits queued UI commands through the shared command pipeline.
pub fn flush_ui_commands(world: &mut World) {
    let queued: Vec<PlayerCommand> = std::mem::take(&mut world.resource_mut::<UiCommandQueue>().0);
    for command in queued {
        if let Err(rejection) = submit_command(world, command) {
            world.resource_mut::<JobForm>().notice = Some(rejection.to_string());
        }
    }
}

/// Pauses the campaign when a new popup arrives.
pub fn auto_pause_on_popups(
    popups: Option<Res<PendingPopups>>,
    mut control: ResMut<TimeControl>,
    mut last_seen: Local<u64>,
) {
    let Some(popups) = popups else {
        return;
    };
    if popups.next_id != *last_seen {
        *last_seen = popups.next_id;
        if !popups.popups.is_empty() {
            control.paused = true;
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub fn draw_jobs_ui(
    mut contexts: EguiContexts,
    clock: Option<Res<CampaignClock>>,
    content: Option<Res<ContentDb>>,
    politics: Option<Res<PoliticsIndex>>,
    player: Option<Res<PlayerHouse>>,
    popups: Option<Res<PendingPopups>>,
    log: Option<Res<MessageLog>>,
    mut queue: ResMut<UiCommandQueue>,
    mut form: ResMut<JobForm>,
    jobs: Query<&ActiveJob>,
    characters: Query<(&CharacterRecord, &CharacterCondition)>,
    orgs: Query<&OrgRecord>,
    provinces: Query<(&ProvinceRecord, &DisplayName)>,
) {
    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };
    let (Some(clock), Some(content), Some(politics), Some(player), Some(popups), Some(log)) =
        (clock, content, politics, player, popups, log)
    else {
        return;
    };
    let date = clock.date;
    let Some(player_org) = player.0 else {
        return;
    };

    let mut viewport = egui::Ui::new(
        ctx.clone(),
        "jobs-viewport".into(),
        egui::UiBuilder::new()
            .layer_id(egui::LayerId::background())
            .max_rect(ctx.viewport_rect()),
    );

    // ------------------------------------------------------------------
    // Bottom panel: message log | active jobs | start form.
    // ------------------------------------------------------------------
    egui::Panel::bottom("jobs-bar")
        .default_size(170.0)
        .show(&mut viewport, |ui| {
            ui.columns(3, |columns| {
                // Message log.
                columns[0].heading("Log");
                egui::ScrollArea::vertical()
                    .id_salt("log-scroll")
                    .stick_to_bottom(true)
                    .show(&mut columns[0], |ui| {
                        for entry in log.entries.iter().rev().take(100).rev() {
                            ui.label(format!("{}  {}", entry.date, entry.text));
                        }
                    });

                // Active jobs.
                columns[1].heading("Active Jobs");
                egui::ScrollArea::vertical()
                    .id_salt("jobs-scroll")
                    .show(&mut columns[1], |ui| {
                        let mut any = false;
                        let mut sorted: Vec<&ActiveJob> =
                            jobs.iter().filter(|j| j.owner == player_org).collect();
                        sorted.sort_by_key(|j| j.id);
                        for job in sorted {
                            any = true;
                            let title = content
                                .0
                                .jobs
                                .get(&job.def)
                                .map(|d| d.title.as_str())
                                .unwrap_or("Unknown");
                            let leader = politics
                                .characters
                                .get(&job.leader)
                                .and_then(|e| characters.get(*e).ok())
                                .map(|(r, _)| r.name.clone())
                                .unwrap_or_default();
                            let remaining = date.days_until(job.completes).max(0);
                            ui.horizontal(|ui| {
                                ui.label(format!("{title} — {leader} ({remaining}d left)"));
                                if ui.small_button("Cancel").clicked() {
                                    queue.0.push(PlayerCommand::CancelJob { job: job.id });
                                }
                            });
                        }
                        if !any {
                            ui.label("No jobs under way.");
                        }
                    });

                // Start form.
                columns[2].heading("Start a Job");
                draw_start_form(
                    &mut columns[2],
                    &content,
                    &politics,
                    player_org,
                    date,
                    &mut form,
                    &mut queue,
                    &jobs,
                    &characters,
                    &orgs,
                    &provinces,
                );
            });
        });

    // ------------------------------------------------------------------
    // Result popups: modal-style windows, oldest first.
    // ------------------------------------------------------------------
    if let Some(popup) = popups.popups.first() {
        egui::Window::new("A Matter Requires Your Attention")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.label(&popup.text);
                ui.separator();
                ui.horizontal(|ui| {
                    for (choice_id, label) in &popup.choices {
                        if ui.button(label).clicked() {
                            queue.0.push(PlayerCommand::AnswerPopup {
                                popup: popup.id,
                                choice: choice_id.clone(),
                            });
                        }
                    }
                });
            });
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_start_form(
    ui: &mut egui::Ui,
    content: &ContentDb,
    politics: &PoliticsIndex,
    player_org: aeon_sim::OrgId,
    date: aeon_core::calendar::GameDate,
    form: &mut JobForm,
    queue: &mut UiCommandQueue,
    jobs: &Query<&ActiveJob>,
    characters: &Query<(&CharacterRecord, &CharacterCondition)>,
    orgs: &Query<&OrgRecord>,
    provinces: &Query<(&ProvinceRecord, &DisplayName)>,
) {
    // Job picker.
    let job_label = form
        .job
        .as_ref()
        .and_then(|k| content.0.jobs.get(k))
        .map(|d| d.title.clone())
        .unwrap_or_else(|| "Choose a job".to_owned());
    egui::ComboBox::from_label("Job")
        .selected_text(job_label)
        .show_ui(ui, |ui| {
            for (key, def) in &content.0.jobs {
                if ui
                    .selectable_label(form.job.as_ref() == Some(key), &def.title)
                    .clicked()
                {
                    form.job = Some(key.clone());
                    form.target = None;
                }
            }
        });
    let Some(def) = form.job.as_ref().and_then(|k| content.0.jobs.get(k)) else {
        return;
    };
    ui.label(&def.summary);
    if !def.risks.is_empty() {
        let risks: Vec<String> = def.risks.iter().map(|r| format!("{r:?}")).collect();
        ui.colored_label(
            egui::Color32::from_rgb(200, 140, 60),
            format!("Risks: {}", risks.join(", ")),
        );
    }

    // Leader picker: living adult members not already leading.
    let busy: Vec<CharacterId> = jobs.iter().map(|j| j.leader).collect();
    let leader_label = form
        .leader
        .and_then(|id| politics.characters.get(&id))
        .and_then(|e| characters.get(*e).ok())
        .map(|(r, _)| r.name.clone())
        .unwrap_or_else(|| "Choose a leader".to_owned());
    egui::ComboBox::from_label("Leader")
        .selected_text(leader_label)
        .show_ui(ui, |ui| {
            for (id, entity) in &politics.characters {
                let Ok((record, condition)) = characters.get(*entity) else {
                    continue;
                };
                if !record.alive()
                    || record.organisation != Some(player_org)
                    || record.age_years(date) < ADULT_AGE
                    || busy.contains(id)
                    || !condition.can_lead(date)
                {
                    continue;
                }
                if ui
                    .selectable_label(form.leader == Some(*id), &record.name)
                    .clicked()
                {
                    form.leader = Some(*id);
                }
            }
        });

    // Target picker, when the definition needs one.
    match def.target {
        JobTargetKind::None => {
            form.target = Some(JobTarget::None);
        }
        JobTargetKind::Organisation => {
            let label = match form.target {
                Some(JobTarget::Org(org)) => politics
                    .orgs
                    .get(&org)
                    .and_then(|e| orgs.get(*e).ok())
                    .and_then(|r| content.0.organisations.get(&r.key))
                    .map(|d| d.name.clone())
                    .unwrap_or_default(),
                _ => "Choose an organisation".to_owned(),
            };
            egui::ComboBox::from_label("Target")
                .selected_text(label)
                .show_ui(ui, |ui| {
                    for (id, entity) in &politics.orgs {
                        if *id == player_org {
                            continue;
                        }
                        let Ok(record) = orgs.get(*entity) else {
                            continue;
                        };
                        let Some(def) = content.0.organisations.get(&record.key) else {
                            continue;
                        };
                        if ui
                            .selectable_label(form.target == Some(JobTarget::Org(*id)), &def.name)
                            .clicked()
                        {
                            form.target = Some(JobTarget::Org(*id));
                        }
                    }
                });
        }
        JobTargetKind::Character => {
            let label = match form.target {
                Some(JobTarget::Character(id)) => politics
                    .characters
                    .get(&id)
                    .and_then(|e| characters.get(*e).ok())
                    .map(|(r, _)| r.name.clone())
                    .unwrap_or_default(),
                _ => "Choose a character".to_owned(),
            };
            egui::ComboBox::from_label("Target")
                .selected_text(label)
                .show_ui(ui, |ui| {
                    for (id, entity) in &politics.characters {
                        let Ok((record, _)) = characters.get(*entity) else {
                            continue;
                        };
                        if !record.alive() || record.organisation == Some(player_org) {
                            continue;
                        }
                        if ui
                            .selectable_label(
                                form.target == Some(JobTarget::Character(*id)),
                                &record.name,
                            )
                            .clicked()
                        {
                            form.target = Some(JobTarget::Character(*id));
                        }
                    }
                });
        }
        JobTargetKind::Province => {
            let label = match form.target {
                Some(JobTarget::Province(id)) => provinces
                    .iter()
                    .find(|(r, _)| r.id == id)
                    .map(|(_, n)| n.0.clone())
                    .unwrap_or_default(),
                _ => "Choose a province".to_owned(),
            };
            egui::ComboBox::from_label("Target")
                .selected_text(label)
                .show_ui(ui, |ui| {
                    let mut sorted: Vec<_> = provinces.iter().collect();
                    sorted.sort_by_key(|(r, _)| r.id);
                    for (record, name) in sorted {
                        if ui
                            .selectable_label(
                                form.target == Some(JobTarget::Province(record.id)),
                                &name.0,
                            )
                            .clicked()
                        {
                            form.target = Some(JobTarget::Province(record.id));
                        }
                    }
                });
        }
    }

    let ready = form.leader.is_some() && form.target.is_some();
    if ui.add_enabled(ready, egui::Button::new("Start")).clicked()
        && let (Some(job), Some(leader), Some(target)) =
            (form.job.clone(), form.leader, form.target)
    {
        queue.0.push(PlayerCommand::StartJob {
            job,
            leader,
            target,
        });
        form.notice = None;
        form.leader = None;
        form.target = None;
    }
    if let Some(notice) = &form.notice {
        ui.colored_label(egui::Color32::from_rgb(220, 60, 60), notice);
    }
}
