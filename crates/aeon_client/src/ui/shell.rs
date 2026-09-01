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
use bevy_egui::{EguiContext, EguiContexts, PrimaryEguiContext, egui};

use crate::assignment_ui::UiCommandQueue;
use crate::sim_driver::TimeControl;
use crate::ui::data::{AssignmentUi, MapUi, PanelData};
use crate::ui::dock::{DockSide, PanelKind};
use crate::ui::layout::{BottomPresentation, LayoutPlan};
use crate::ui::lookup::Lookup;
use crate::ui::overlays::draw_overlays;
use crate::ui::panel::{HeaderAction, PanelCtx, PanelOut, draw_header, draw_panel_body};
use crate::ui::search::draw_search_results;
use crate::ui::top_bar::draw_top_bar;
use crate::view::{SearchState, ViewState};

/// One physical Escape may be owned by exactly one local presentation layer.
#[derive(Resource, Default)]
pub struct LocalEscapeClaim {
    /// The current press was handled before strategic view hotkeys.
    pub claimed: bool,
}

/// Claims physical Escape for the nearest open local surface before the
/// strategic Body -> System fallback runs in `selection::view_hotkeys`.
pub fn claim_local_escape(world: &mut World) {
    let pressed = world
        .resource::<ButtonInput<KeyCode>>()
        .just_pressed(KeyCode::Escape);
    world.resource_mut::<LocalEscapeClaim>().claimed = false;
    if !pressed {
        return;
    }
    // Pinned explanations are the topmost local layer. Their earlier Update
    // claim closes help and reserves both the physical and egui copies of this
    // press, so no lower surface may also unwind.
    if world
        .resource::<crate::ui::explanations::ExplanationState>()
        .escape_claimed()
    {
        return;
    }
    let ctx = {
        let mut query = world.query_filtered::<&mut EguiContext, With<PrimaryEguiContext>>();
        let Ok(mut context) = query.single_mut(world) else {
            return;
        };
        context.get_mut().clone()
    };
    let claimed = if world.resource::<crate::ui::picker::PickerState>().open {
        world
            .resource_mut::<crate::ui::picker::PickerState>()
            .close(&ctx);
        true
    } else if world
        .resource::<crate::ui::assignment_popup::AssignmentPopup>()
        .open
    {
        world
            .resource_mut::<crate::ui::assignment_popup::AssignmentPopup>()
            .cancel(&ctx);
        world
            .resource_mut::<crate::assignment_ui::AssignmentForm>()
            .reset();
        true
    } else if world.resource::<crate::preferences::SettingsUi>().open {
        world
            .resource_mut::<crate::preferences::SettingsUi>()
            .close(&ctx);
        true
    } else if !world.resource::<SearchState>().query.is_empty() {
        world.resource_mut::<SearchState>().query.clear();
        true
    } else {
        false
    };
    world.resource_mut::<LocalEscapeClaim>().claimed = claimed;
}

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
        mut popup,
    } = assignment_ui;
    let MapUi {
        mut mode,
        mut dock,
        mut situation_ui,
        mut explanations,
        mut preferences,
        mut settings,
        escape_claim,
    } = map_ui;
    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };
    crate::ui::keyboard::begin_frame(ctx);
    // The Update-stage semantic claim already closed exactly one layer. Eat
    // the matching egui event before any popup/window/widget can react too.
    if escape_claim.claimed {
        ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
    }
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
    let mut layout = LayoutPlan::new(ctx.viewport_rect().size(), &dock);

    let measured_top = draw_top_bar(
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
        &mut settings,
        layout,
    );
    layout = layout.with_measured_top(measured_top);

    crate::preferences::draw_campaign_settings(ctx, strings, &mut preferences, &mut settings);

    draw_search_results(ctx, &lookup, &data, &mut view, &mut search, layout);
    draw_overlays(
        ctx,
        theme,
        strings,
        &data.readout,
        &mut view,
        &mut dock,
        &mut situation_ui,
        layout,
    );

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
        plans: data.plans.as_deref(),
        goals: data.goals.as_deref(),
        issued_directives: data.issued_directives.as_deref(),
        situations: &data.situations,
    };
    let mut out = PanelOut {
        view: &mut view,
        form: &mut form,
        queue: &mut queue,
        popup: &mut popup,
        filter: &mut filter,
        situation_ui: &mut situation_ui,
        explanations: &mut explanations,
    };

    // Header verbs are collected and applied after drawing: a panel cannot
    // move itself out from under the loop that is drawing it.
    let mut moves: Vec<(PanelKind, Option<DockSide>)> = Vec::new();

    for side in DockSide::ALL {
        let kinds = dock.panels_on(*side).to_vec();
        if kinds.is_empty() {
            continue;
        }
        let size = match side {
            DockSide::Bottom => layout.bottom_height,
            DockSide::Left | DockSide::Right => layout.side_width(*side),
        };
        let mut bottom_selected = dock.bottom_selected();
        let mut draw = |ui: &mut egui::Ui, moves: &mut Vec<_>, out: &mut PanelOut| {
            draw_side(
                ui,
                *side,
                &kinds,
                &panel_ctx,
                out,
                moves,
                layout.bottom_presentation,
                &mut bottom_selected,
            );
        };
        match side {
            DockSide::Bottom => {
                egui::Panel::bottom("dock-bottom")
                    .exact_size(size)
                    .show(&mut viewport, |ui| draw(ui, &mut moves, &mut out));
            }
            DockSide::Left => {
                egui::Panel::left("dock-left")
                    .exact_size(size)
                    .show(&mut viewport, |ui| draw(ui, &mut moves, &mut out));
            }
            DockSide::Right => {
                egui::Panel::right("dock-right")
                    .exact_size(size)
                    .show(&mut viewport, |ui| draw(ui, &mut moves, &mut out));
            }
        }
        if let Some(selected) = bottom_selected {
            dock.select_bottom(selected);
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
#[allow(clippy::too_many_arguments)]
fn draw_side(
    ui: &mut egui::Ui,
    side: DockSide,
    kinds: &[PanelKind],
    ctx: &PanelCtx,
    out: &mut PanelOut,
    moves: &mut Vec<(PanelKind, Option<DockSide>)>,
    bottom_presentation: BottomPresentation,
    bottom_selected: &mut Option<PanelKind>,
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
        DockSide::Bottom if bottom_presentation == BottomPresentation::Tabs => {
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().interact_size.y = ui.spacing().interact_size.y.max(24.0);
                for kind in kinds {
                    let response = ui.selectable_label(
                        *bottom_selected == Some(*kind),
                        ctx.strings.text(kind.title_key()),
                    );
                    crate::ui::keyboard::capture_action(
                        ui,
                        crate::ui::keyboard::LogicalFocus::new(format!("bottom-tab:{kind:?}")),
                        "compact-tab",
                        crate::ui::keyboard::FocusBand::BottomTabs,
                        &response,
                    )
                    .register();
                    #[cfg(test)]
                    crate::ui::rendered_state::record_response(ui, "compact-tab", &response);
                    if response.clicked() {
                        *bottom_selected = Some(*kind);
                    }
                }
            });
            ui.separator();
            if let Some(kind) = (*bottom_selected).or_else(|| kinds.first().copied()) {
                one(ui, kind, out);
            }
        }
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
