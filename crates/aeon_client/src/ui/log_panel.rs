//! The message log, and the filters that make it readable.
//!
//! Was half of a fixed bottom bar; it is now a panel like any other, which
//! is why it no longer sizes itself. Whatever space its side gives it, it
//! fills — so the same code serves a wide strip along the bottom and a
//! narrow column down an edge.

use aeon_sim::{LogChannel, LogEntry, LogSubject};
use bevy_egui::egui;

use crate::ui::panel::{PanelCtx, PanelOut};
use crate::view::{MapView, Selection};

/// Draws the log's filter row and its entries.
pub fn draw_log_panel(ui: &mut egui::Ui, ctx: &PanelCtx, out: &mut PanelOut) {
    let Some(log) = ctx.log else {
        ui.weak(ctx.strings.text("ui.log.no-campaign"));
        return;
    };
    let theme = &ctx.data.theme;
    let strings = ctx.strings;
    let filter = &mut *out.filter;
    let view = &mut *out.view;
    let player_org = ctx.player_org;
    let provinces = &ctx.data.province_records;
    ui.horizontal_wrapped(|ui| {
        for channel in LogChannel::ALL {
            let mut on = filter.channels.contains(&channel);
            if ui
                .toggle_value(&mut on, strings.text(channel.label_key()))
                .changed()
            {
                if on {
                    filter.channels.insert(channel);
                } else {
                    filter.channels.remove(&channel);
                }
            }
        }
        ui.toggle_value(&mut filter.mine_only, strings.text("ui.log.mine-only"))
            .on_hover_text(strings.text("ui.log.mine-only.hover"));
        ui.add(
            egui::TextEdit::singleline(&mut filter.text)
                .hint_text(strings.text("ui.log.filter-hint"))
                .desired_width(f32::from(theme.components.log_filter_width)),
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
                .take(usize::from(theme.components.log_max_entries))
                .collect();
            if visible.is_empty() {
                ui.weak(strings.text("ui.log.no-matches"));
            }
            for entry in visible.into_iter().rev() {
                ui.horizontal_wrapped(|ui| {
                    ui.weak(entry.date.to_string());
                    match entry.subject {
                        // A subject makes the entry a way in.
                        Some(subject) => {
                            if ui
                                .link(&entry.text)
                                .on_hover_text(strings.text("ui.log.go-to-subject"))
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
