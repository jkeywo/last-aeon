//! Drawing one panel, whichever panel it is.
//!
//! Every panel gets the same header — its title, the three places it can
//! go, and a way to close it — because a control that appears on one panel
//! and not another has to be learned per panel. Dispatch is by
//! [`PanelKind`], so the shell never names a particular panel.
//!
//! A panel body is handed a read-only [`PanelCtx`] and a mutable
//! [`PanelOut`]. Splitting them that way is what lets several panels be
//! drawn in one frame without the borrow checker objecting: the shared
//! half is borrowed once and the mutable half is threaded through.

use aeon_core::calendar::GameDate;
use aeon_data::ContentSet;
use aeon_sim::state::ContentDb;
use aeon_sim::{CharacterId, MessageLog, OrgId, PoliticsIndex, TextDb};
use bevy_egui::egui;

use crate::jobs_ui::{JobForm, LogFilter, UiCommandQueue};
use crate::ui::data::PanelData;
use crate::ui::dock::{DockSide, PanelKind};
use crate::ui::inspector::draw_inspector;
use crate::ui::jobs_panel::draw_jobs_panel;
use crate::ui::ledger_panel::draw_ledger_panel;
use crate::ui::listing::draw_listing;
use crate::ui::log_panel::draw_log_panel;
use crate::ui::lookup::Lookup;
use crate::ui::picker::PickerState;
use crate::view::{MapMode, ViewState};

/// Everything a panel reads.
pub struct PanelCtx<'a, 'w, 's> {
    /// Names, labels and hover summaries.
    pub lookup: &'a Lookup<'a>,
    /// The frame's world queries.
    pub data: &'a PanelData<'w, 's>,
    /// The authored content.
    pub content: &'a ContentSet,
    /// The content database, for panels that want the whole thing.
    pub content_db: &'a ContentDb,
    /// The political index, for resolving ids to entities.
    pub politics: &'a PoliticsIndex,
    /// Every string the panel draws.
    pub strings: &'a TextDb,
    /// Today.
    pub date: GameDate,
    /// The active map mode.
    pub mode: MapMode,
    /// The player's house, if they have one.
    pub player_org: Option<OrgId>,
    /// Its current head.
    pub player_head: Option<CharacterId>,
    /// The message log, if a campaign is running.
    pub log: Option<&'a MessageLog>,
}

/// Everything a panel writes.
pub struct PanelOut<'a> {
    /// What is selected and which map is showing.
    pub view: &'a mut ViewState,
    /// The action being composed.
    pub form: &'a mut JobForm,
    /// Commands bound for the simulation.
    pub queue: &'a mut UiCommandQueue,
    /// Whether the character picker is up.
    pub picker: &'a mut PickerState,
    /// What the log is showing.
    pub filter: &'a mut LogFilter,
}

/// What a panel's header was asked to do.
pub enum HeaderAction {
    /// Move it to a side.
    Dock(DockSide),
    /// Put it away.
    Close,
}

/// Draws a panel's header, returning whatever the player asked for.
///
/// The three destination buttons are always all shown, with the current
/// one disabled rather than hidden: a control that vanishes when active
/// gives the player nothing to aim at, and no way to see where they are.
pub fn draw_header(
    ui: &mut egui::Ui,
    strings: &TextDb,
    kind: PanelKind,
    side: DockSide,
) -> Option<HeaderAction> {
    let mut action = None;
    ui.horizontal(|ui| {
        ui.strong(strings.text(kind.title_key()))
            .on_hover_text(strings.text(kind.description_key()));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui
                .small_button("✕")
                .on_hover_text(strings.text("ui.panel.close"))
                .clicked()
            {
                action = Some(HeaderAction::Close);
            }
            for target in [DockSide::Right, DockSide::Bottom, DockSide::Left] {
                let glyph = match target {
                    DockSide::Left => "▏",
                    DockSide::Right => "▕",
                    DockSide::Bottom => "▁",
                };
                let here = target == side;
                if ui
                    .add_enabled(!here, egui::Button::new(glyph).small())
                    .on_hover_text(strings.format(
                        "ui.panel.move-to",
                        &[("side", strings.text(target.label_key()))],
                    ))
                    .clicked()
                {
                    action = Some(HeaderAction::Dock(target));
                }
            }
        });
    });
    action
}

/// Draws one panel's contents.
pub fn draw_panel_body(ui: &mut egui::Ui, kind: PanelKind, ctx: &PanelCtx, out: &mut PanelOut) {
    match kind {
        PanelKind::Inspector => {
            // A forecast and its candidate list can outgrow the panel, so
            // the inspector scrolls independently of its side.
            egui::ScrollArea::vertical()
                .id_salt("inspector-scroll")
                .show(ui, |ui| {
                    draw_inspector(ui, ctx, out);
                });
        }
        PanelKind::Listing => draw_listing(ui, ctx, out),
        PanelKind::Log => draw_log_panel(ui, ctx, out),
        PanelKind::Jobs => draw_jobs_panel(
            ui,
            ctx.lookup,
            ctx.content_db,
            ctx.player_org,
            ctx.date,
            &ctx.data.active_jobs,
            out.queue,
        ),
        PanelKind::Ledger => draw_ledger_panel(
            ui,
            &ctx.data.theme,
            ctx.strings,
            &ctx.data.readout,
            ctx.mode,
        ),
    }
}
