//! The message log, and the filters that make it readable.
//!
//! Was half of a fixed bottom bar; it is now a panel like any other, which
//! is why it no longer sizes itself. Whatever space its side gives it, it
//! fills — so the same code serves a wide strip along the bottom and a
//! narrow column down an edge.

use aeon_sim::map::ProvinceRecord;
use aeon_sim::{LogChannel, LogEntry, LogSubject, MessageLog, OrgId};
use bevy::prelude::Query;
use bevy_egui::egui;

use crate::jobs_ui::LogFilter;
use crate::view::{MapView, Selection, ViewState};

/// Draws the log's filter row and its entries.
pub fn draw_log_panel(
    ui: &mut egui::Ui,
    log: &MessageLog,
    filter: &mut LogFilter,
    player_org: Option<OrgId>,
    view: &mut ViewState,
    provinces: &Query<&ProvinceRecord>,
) {
    ui.horizontal_wrapped(|ui| {
        for channel in LogChannel::ALL {
            let mut on = filter.channels.contains(&channel);
            if ui.toggle_value(&mut on, channel.label()).changed() {
                if on {
                    filter.channels.insert(channel);
                } else {
                    filter.channels.remove(&channel);
                }
            }
        }
        ui.toggle_value(&mut filter.mine_only, "Mine")
            .on_hover_text("Show only entries concerning your own house.");
        ui.add(
            egui::TextEdit::singleline(&mut filter.text)
                .hint_text("Filter…")
                .desired_width(90.0),
        );
    });

    egui::ScrollArea::vertical()
        .id_salt("log-scroll")
        .stick_to_bottom(true)
        .show(ui, |ui| {
            let visible: Vec<&LogEntry> = log
                .entries
                .iter()
                .filter(|entry| filter.admits(entry, player_org))
                .rev()
                .take(200)
                .collect();
            if visible.is_empty() {
                ui.weak("Nothing matches this filter.");
            }
            for entry in visible.into_iter().rev() {
                ui.horizontal_wrapped(|ui| {
                    ui.weak(entry.date.to_string());
                    match entry.subject {
                        // A subject makes the entry a way in.
                        Some(subject) => {
                            if ui
                                .link(&entry.text)
                                .on_hover_text("Show what this is about")
                                .clicked()
                            {
                                match subject {
                                    LogSubject::Character(id) => {
                                        view.selected = Some(Selection::Character(id));
                                    }
                                    LogSubject::Org(id) => {
                                        view.selected = Some(Selection::Org(id));
                                    }
                                    LogSubject::Province(id) => {
                                        view.selected = Some(Selection::Province(id));
                                        if let Some(body) = provinces
                                            .iter()
                                            .find(|record| record.id == id)
                                            .map(|record| record.body)
                                        {
                                            view.view = MapView::Body(body);
                                        }
                                    }
                                }
                            }
                        }
                        None => {
                            ui.label(&entry.text);
                        }
                    }
                });
            }
        });
}
