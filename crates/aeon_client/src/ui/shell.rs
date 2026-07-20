//! The shell: one root `Ui`, and the order the panels claim space in.
//!
//! This is deliberately the only place that knows about layout. It builds
//! the frame's shared lookups, draws the top bar and the map overlays, and
//! then walks the dock — for each side, in the order egui claims space,
//! drawing whatever panels the player has put there.
//!
//! No panel is named here. Which panel is where is data, held in
//! [`DockState`], so a panel moves without this file changing.

use aeon_sim::state::{CampaignMeta, ContentDb};
use aeon_sim::{CampaignClock, CampaignOver, CharacterId, PlayerHouse, PoliticsIndex};
use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};

use crate::assignment_ui::UiCommandQueue;
use crate::sim_driver::TimeControl;
use crate::ui::data::{AssignmentUi, MapUi, PanelData};
use crate::ui::dock::{DockSide, PanelKind};
use crate::ui::lookup::Lookup;
use crate::ui::overlays::draw_overlays;
use crate::ui::panel::{HeaderAction, PanelCtx, PanelOut, draw_header, draw_panel_body};
use crate::ui::search::draw_search_results;
use crate::ui::top_bar::draw_top_bar;
use crate::view::{SearchState, ViewState};

#[allow(clippy::too_many_arguments)]
pub fn draw_panels(
    mut contexts: EguiContexts,
    clock: Option<Res<CampaignClock>>,
    meta: Option<Res<CampaignMeta>>,
    content: Option<Res<ContentDb>>,
    politics: Option<Res<PoliticsIndex>>,
    player: Option<Res<PlayerHouse>>,
    over: Option<Res<CampaignOver>>,
    mut control: ResMut<TimeControl>,
    mut view: ResMut<ViewState>,
    mut queue: ResMut<UiCommandQueue>,
    mut search: ResMut<SearchState>,
    map_ui: MapUi,
    assignment_ui: AssignmentUi,
    log: Option<Res<aeon_sim::MessageLog>>,
    mut filter: ResMut<crate::assignment_ui::LogFilter>,
    data: PanelData,
) {
    let AssignmentUi {
        mut form,
        mut picker,
    } = assignment_ui;
    let MapUi { mut mode, mut dock } = map_ui;
    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };
    let (Some(clock), Some(meta), Some(content), Some(politics)) = (clock, meta, content, politics)
    else {
        return;
    };
    let Some(strings) = data.strings.as_deref() else {
        return;
    };
    let date = clock.date;
    let theme = &data.theme;
    let player_org = player.as_ref().and_then(|p| p.0);

    // Every name, label and hover summary the panels need, built once.
    let lookup = Lookup::build(&data, &content.0, strings, date);
    let player_head: Option<CharacterId> =
        player_org.and_then(|org| lookup.orgs.get(&org).and_then(|(r, _)| r.head));

    let mut viewport = egui::Ui::new(
        ctx.clone(),
        "viewport".into(),
        egui::UiBuilder::new()
            .layer_id(egui::LayerId::background())
            .max_rect(ctx.viewport_rect()),
    );

    draw_top_bar(
        &mut viewport,
        &lookup,
        &content.0,
        theme,
        strings,
        &meta,
        date,
        over.as_deref(),
        player_org,
        player_head,
        &mut control,
        &mut view,
        &mut mode,
        &mut dock,
        &mut search,
    );

    draw_search_results(ctx, &lookup, &data, &mut view, &mut search);
    draw_overlays(ctx, theme, strings, &data.readout, &mut view);

    let panel_ctx = PanelCtx {
        lookup: &lookup,
        data: &data,
        content: &content.0,
        content_db: &content,
        politics: &politics,
        strings,
        date,
        mode: *mode,
        player_org,
        player_head,
        log: log.as_deref(),
    };
    let mut out = PanelOut {
        view: &mut view,
        form: &mut form,
        queue: &mut queue,
        picker: &mut picker,
        filter: &mut filter,
    };

    // Header verbs are collected and applied after drawing: a panel cannot
    // move itself out from under the loop that is drawing it.
    let mut moves: Vec<(PanelKind, Option<DockSide>)> = Vec::new();

    for side in DockSide::ALL {
        let kinds = dock.panels_on(*side).to_vec();
        if kinds.is_empty() {
            continue;
        }
        let size = dock.size_of(*side);
        let draw = |ui: &mut egui::Ui, moves: &mut Vec<_>, out: &mut PanelOut| {
            draw_side(ui, *side, &kinds, &panel_ctx, out, moves);
        };
        match side {
            DockSide::Bottom => {
                egui::Panel::bottom("dock-bottom")
                    .exact_size(size)
                    .show(&mut viewport, |ui| draw(ui, &mut moves, &mut out));
            }
            DockSide::Left => {
                egui::Panel::left("dock-left")
                    .default_size(size)
                    .show(&mut viewport, |ui| draw(ui, &mut moves, &mut out));
            }
            DockSide::Right => {
                egui::Panel::right("dock-right")
                    .default_size(size)
                    .show(&mut viewport, |ui| draw(ui, &mut moves, &mut out));
            }
        }
    }

    for (kind, target) in moves {
        match target {
            Some(side) => dock.dock(kind, side),
            None => dock.close(kind),
        }
    }
}

/// Draws every panel on one side.
///
/// The bottom lays its panels out side by side and the edges stack theirs,
/// because that is the shape each has room for — a wide short strip suits
/// a list of messages, a tall narrow one suits an inspector.
fn draw_side(
    ui: &mut egui::Ui,
    side: DockSide,
    kinds: &[PanelKind],
    ctx: &PanelCtx,
    out: &mut PanelOut,
    moves: &mut Vec<(PanelKind, Option<DockSide>)>,
) {
    let mut one = |ui: &mut egui::Ui, kind: PanelKind, out: &mut PanelOut| {
        if let Some(action) = draw_header(ui, ctx.strings, kind, side) {
            moves.push(match action {
                HeaderAction::Dock(target) => (kind, Some(target)),
                HeaderAction::Close => (kind, None),
            });
        }
        ui.separator();
        draw_panel_body(ui, kind, ctx, out);
    };

    match side {
        DockSide::Bottom => {
            ui.columns(kinds.len(), |columns| {
                for (index, kind) in kinds.iter().enumerate() {
                    one(&mut columns[index], *kind, out);
                }
            });
        }
        _ => {
            for kind in kinds {
                one(ui, *kind, out);
            }
        }
    }
}
