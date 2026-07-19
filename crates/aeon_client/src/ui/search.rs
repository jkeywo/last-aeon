//! The global search box's results.
//!
//! Floats over the map rather than taking layout space, since it exists
//! only while a query is typed.

use bevy_egui::egui;

use crate::ui::data::PanelData;
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
) {
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
