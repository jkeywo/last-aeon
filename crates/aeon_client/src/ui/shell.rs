//! Read-only 2D information panels plus presence and forces controls.
//!
//! A top bar (campaign, date, resources, time control, view breadcrumb),
//! a left inspector for the current selection (body, province, house, or
//! character, including location and travel), and a right listing panel
//! (bodies, houses, and the player's forces). Mutations travel through
//! the UI command queue into the authoritative command pipeline.

use aeon_sim::state::{CampaignMeta, ContentDb};
use aeon_sim::{
    CampaignClock, CampaignOver, CharacterId, OrgId, PlayerHouse, PoliticsIndex, ProvinceId,
};
use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};

use crate::jobs_ui::UiCommandQueue;
use crate::sim_driver::TimeControl;
use crate::ui::data::{JobUi, MapUi, PanelData};
use crate::ui::inspector::{Inspector, draw_inspector};
use crate::ui::listing::draw_listing;
use crate::ui::lookup::Lookup;
use crate::ui::overlays::draw_overlays;
use crate::ui::search::draw_search_results;
use crate::ui::top_bar::draw_top_bar;
use crate::view::{SearchState, ViewState};

/// One global-search result.
enum SearchHit {
    Character(CharacterId),
    Org(OrgId),
    /// A province and the body it sits on (so the view can focus it).
    Province(ProvinceId, aeon_sim::BodyId),
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
    job_ui: JobUi,
    log: Option<Res<aeon_sim::MessageLog>>,
    mut filter: ResMut<crate::jobs_ui::LogFilter>,
    data: PanelData,
) {
    let JobUi {
        mut form,
        mut picker,
    } = job_ui;
    let MapUi {
        mut mode,
        mut ledger,
    } = map_ui;
    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };
    let (Some(clock), Some(meta), Some(content), Some(politics)) = (clock, meta, content, politics)
    else {
        return;
    };
    let date = clock.date;
    let theme = &data.theme;
    let player_org = player.as_ref().and_then(|p| p.0);

    // Every name, label and hover summary the panels need, built once.
    let lookup = Lookup::build(&data, &content.0, date);
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
        &meta,
        date,
        over.as_deref(),
        player_org,
        player_head,
        &mut control,
        &mut view,
        &mut mode,
        &mut ledger,
        &mut search,
    );

    draw_search_results(ctx, &lookup, &data, &mut view, &mut search);

    draw_overlays(ctx, theme, &data.readout, *mode, &ledger, &mut view);

    // The bottom bar is declared before the side panels so they stop
    // above it rather than running underneath.
    if let Some(log) = &log {
        crate::jobs_ui::draw_bottom_bar(
            &mut viewport,
            &content,
            &politics,
            date,
            player_org,
            log,
            &mut filter,
            &mut view,
            &mut queue,
            &data.active_jobs,
            &data.character_records,
            &data.province_records,
        );
    }

    // Everything the inspector reads, gathered once rather than by each
    // arm for itself.
    let inspector = Inspector {
        lookup: &lookup,
        data: &data,
        content: &content.0,
        politics: &politics,
        date,
        mode: *mode,
        player_org,
        player_head,
    };

    egui::Panel::left("inspector")
        .default_size(260.0)
        .show(&mut viewport, |ui| {
            ui.heading("Inspector");
            ui.separator();
            // A forecast and its leader comparison can outgrow the panel,
            // so the whole inspector scrolls.
            egui::ScrollArea::vertical()
                .id_salt("inspector-scroll")
                .show(ui, |ui| {
                    draw_inspector(
                        ui,
                        &inspector,
                        &mut view,
                        &mut form,
                        &mut queue,
                        &mut picker,
                    );
                });
        });

    draw_listing(
        &mut viewport,
        &lookup,
        &data,
        &content.0,
        player_org,
        player_head,
        &mut view,
        &mut queue,
    );
}
