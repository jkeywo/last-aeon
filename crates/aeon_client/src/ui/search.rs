//! The global search box's results.
//!
//! Floats over the map rather than taking layout space, since it exists
//! only while a query is typed.

use bevy_egui::egui;

use crate::ui::data::PanelData;
use crate::ui::lookup::Lookup;
use crate::view::{MapView, SearchState, Selection, ViewState};

/// One global-search result.
enum SearchHit {
    Character(aeon_sim::CharacterId),
    Org(aeon_sim::OrgId),
    /// A province and the body it sits on (so the view can focus it).
    Province(aeon_sim::ProvinceId, aeon_sim::BodyId),
}

/// Draws the search results over the map while a query is set.
pub fn draw_search_results(
    ctx: &egui::Context,
    lookup: &Lookup,
    data: &PanelData,
    view: &mut ViewState,
    search: &mut SearchState,
) {
    // Search results, floating below the top bar while the query is set.
    let query = search.query.trim().to_lowercase();
    if !query.is_empty() {
        let mut hits: Vec<(String, SearchHit)> = Vec::new();
        for (id, (record, ..)) in &lookup.chars {
            if record.name.to_lowercase().contains(&query) {
                hits.push((record.name.clone(), SearchHit::Character(*id)));
            }
        }
        for (id, (record, _)) in &lookup.orgs {
            let name = lookup.org_name(*id);
            if name.to_lowercase().contains(&query) {
                let _ = record;
                hits.push((name, SearchHit::Org(*id)));
            }
        }
        for (record, name, _) in &data.provinces {
            if name.0.to_lowercase().contains(&query) {
                hits.push((name.0.clone(), SearchHit::Province(record.id, record.body)));
            }
        }
        hits.sort_by(|a, b| a.0.cmp(&b.0));
        hits.truncate(30);

        egui::Area::new("search-results".into())
            .fixed_pos(egui::pos2(ctx.viewport_rect().width() - 260.0, 34.0))
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.set_min_width(240.0);
                    if hits.is_empty() {
                        ui.label("No matches.");
                    }
                    egui::ScrollArea::vertical()
                        .max_height(320.0)
                        .show(ui, |ui| {
                            for (label, hit) in &hits {
                                let tag = match hit {
                                    SearchHit::Character(_) => "character",
                                    SearchHit::Org(_) => "house",
                                    SearchHit::Province(..) => "province",
                                };
                                if ui
                                    .selectable_label(false, format!("{label}  ({tag})"))
                                    .clicked()
                                {
                                    match hit {
                                        SearchHit::Character(id) => {
                                            view.selected = Some(Selection::Character(*id));
                                        }
                                        SearchHit::Org(id) => {
                                            view.selected = Some(Selection::Org(*id));
                                        }
                                        SearchHit::Province(id, body) => {
                                            view.view = MapView::Body(*body);
                                            view.selected = Some(Selection::Province(*id));
                                        }
                                    }
                                    search.query.clear();
                                }
                            }
                        });
                });
            });
    }
}
