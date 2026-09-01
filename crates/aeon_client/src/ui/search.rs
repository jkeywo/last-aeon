//! The global search box's results.
//!
//! Floats over the map rather than taking layout space, since it exists
//! only while a query is typed.

use bevy_egui::egui;

use crate::ui::data::PanelData;
use crate::ui::layout::LayoutPlan;
use crate::ui::lookup::Lookup;
use crate::view::{MapView, SearchState, Selection, ViewState};

/// One global-search result.
#[derive(Debug, PartialEq, Eq)]
enum SearchHit {
    Character(aeon_sim::CharacterId),
    Org(aeon_sim::OrgId),
    /// A province and the body it sits on (so the view can focus it).
    Province(aeon_sim::ProvinceId, aeon_sim::BodyId),
}

/// The matches for a query: case-insensitive substring over the names,
/// sorted by name, capped so the popup stays readable.
///
/// Separated from the drawing so the matching itself is testable; the
/// caller supplies every searchable (name, hit) pair.
fn matching(query: &str, candidates: Vec<(String, SearchHit)>) -> Vec<(String, SearchHit)> {
    let needle = query.trim().to_lowercase();
    let mut hits: Vec<(String, SearchHit)> = candidates
        .into_iter()
        .filter(|(name, _)| name.to_lowercase().contains(&needle))
        .collect();
    hits.sort_by(|a, b| a.0.cmp(&b.0));
    hits.truncate(30);
    hits
}

/// Draws the search results over the map while a query is set.
pub fn draw_search_results(
    ctx: &egui::Context,
    lookup: &Lookup,
    data: &PanelData,
    view: &mut ViewState,
    search: &mut SearchState,
    layout: LayoutPlan,
) {
    let strings = lookup.strings;
    let theme = &data.theme;
    // Search results, floating below the top bar while the query is set.
    let query = search.query.trim().to_lowercase();
    if !query.is_empty() {
        let mut candidates: Vec<(String, SearchHit)> = Vec::new();
        for (id, (record, ..)) in &lookup.chars {
            candidates.push((record.name.clone(), SearchHit::Character(*id)));
        }
        for id in lookup.orgs.keys() {
            candidates.push((lookup.org_name(*id), SearchHit::Org(*id)));
        }
        for (record, name, _) in &data.provinces {
            candidates.push((name.0.clone(), SearchHit::Province(record.id, record.body)));
        }
        let hits = matching(&query, candidates);

        let viewport = ctx.viewport_rect();
        let width = f32::from(theme.components.search_width)
            .min((viewport.width() - layout.left_width - layout.right_width - 16.0).max(180.0));
        let x = (viewport.right() - layout.right_width - width - 8.0).max(viewport.left() + 8.0);
        egui::Area::new("search-results".into())
            .fixed_pos(egui::pos2(x, layout.overlay_top))
            .constrain_to(viewport)
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.set_width(width);
                    if hits.is_empty() {
                        ui.label(strings.text("ui.search.no-matches"));
                    }
                    egui::ScrollArea::vertical()
                        .max_height(f32::from(theme.components.search_max_height))
                        .show(ui, |ui| {
                            for (index, (label, hit)) in hits.iter().enumerate() {
                                let tag = match hit {
                                    SearchHit::Character(_) => "ui.search.tag.character",
                                    SearchHit::Org(_) => "ui.search.tag.house",
                                    SearchHit::Province(..) => "ui.search.tag.province",
                                };
                                let response =
                                    ui.selectable_label(false, format!("{label}  ({tag})"));
                                crate::ui::keyboard::capture_action(
                                    ui,
                                    crate::ui::keyboard::LogicalFocus::new(format!(
                                        "search-result:{index}:{label}"
                                    )),
                                    "search-result",
                                    crate::ui::keyboard::FocusBand::Overlay,
                                    &response,
                                )
                                .register();
                                if response.clicked() {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn character(raw: u64) -> SearchHit {
        SearchHit::Character(aeon_sim::CharacterId::from_raw(raw).unwrap())
    }

    fn named(names: &[&str]) -> Vec<(String, SearchHit)> {
        names
            .iter()
            .enumerate()
            .map(|(i, name)| (name.to_string(), character(i as u64 + 1)))
            .collect()
    }

    #[test]
    fn matching_is_case_insensitive_and_sorted() {
        let hits = matching("ar", named(&["Mara Calder", "Aron Veyrin", "Pell"]));
        let names: Vec<&str> = hits.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(names, vec!["Aron Veyrin", "Mara Calder"]);
    }

    #[test]
    fn the_result_list_is_capped() {
        let many: Vec<(String, SearchHit)> = (0..100)
            .map(|i| (format!("Province {i:03}"), character(i + 1)))
            .collect();
        assert_eq!(matching("province", many).len(), 30);
    }

    #[test]
    fn an_unmatched_query_matches_nothing() {
        assert!(matching("zzz", named(&["Aron Veyrin"])).is_empty());
    }
}
