//! Where each panel currently sits.
//!
//! Panels are identified by *what they are*, not by where they are drawn.
//! A panel's side is a lookup rather than a property of its code, so the
//! same drawing function serves the left edge, the right edge and the
//! bottom, and moving one is a data change rather than a rewrite.
//!
//! The central invariant — a panel cannot be in two places at once — is
//! not enforced by checks but by representation: [`DockState::placement`]
//! maps each kind to *one* side, so a second placement necessarily
//! replaces the first. The ordering lists are kept consistent with it.
//!
//! Bottom is a real side rather than a special case for the log. A wide,
//! short panel is the right shape for a message list and for a list of
//! jobs in progress, and the wrong shape for an inspector — so the choice
//! belongs to the player, and the layout has to be able to express it.

use std::collections::BTreeMap;

use bevy::prelude::Resource;

/// An edge of the screen a panel can be docked to.
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum DockSide {
    /// The left edge.
    Left,
    /// The right edge.
    Right,
    /// The bottom edge, where panels sit side by side rather than stacked.
    Bottom,
}

impl DockSide {
    /// Every side, in the order egui claims space: outside in.
    ///
    /// The bottom is declared before the sides so the side panels stop
    /// above it rather than running underneath it.
    pub const ALL: &'static [DockSide] = &[DockSide::Bottom, DockSide::Left, DockSide::Right];

    /// A short player-facing name.
    pub fn label_key(self) -> &'static str {
        match self {
            DockSide::Left => "ui.dock.side.left",
            DockSide::Right => "ui.dock.side.right",
            DockSide::Bottom => "ui.dock.side.bottom",
        }
    }
}

/// A panel, named by what it shows.
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum PanelKind {
    /// The current selection, in detail.
    Inspector,
    /// What exists: bodies, houses, and your own forces.
    Listing,
    /// The message log.
    Log,
    /// Jobs currently under way.
    Jobs,
    /// What the map's colours mean.
    Ledger,
}

impl PanelKind {
    /// Every panel, in the order they appear in the toolbar.
    pub const ALL: &'static [PanelKind] = &[
        PanelKind::Inspector,
        PanelKind::Listing,
        PanelKind::Log,
        PanelKind::Jobs,
        PanelKind::Ledger,
    ];

    /// The panel's title.
    pub fn title_key(self) -> &'static str {
        match self {
            PanelKind::Inspector => "ui.panel.inspector.title",
            PanelKind::Listing => "ui.panel.listing.title",
            PanelKind::Log => "ui.panel.log.title",
            PanelKind::Jobs => "ui.panel.jobs.title",
            PanelKind::Ledger => "ui.panel.ledger.title",
        }
    }

    /// What the panel is for, for its toolbar button's tooltip.
    pub fn description_key(self) -> &'static str {
        match self {
            PanelKind::Inspector => "ui.panel.inspector.description",
            PanelKind::Listing => "ui.panel.listing.description",
            PanelKind::Log => "ui.panel.log.description",
            PanelKind::Jobs => "ui.panel.jobs.description",
            PanelKind::Ledger => "ui.panel.ledger.description",
        }
    }
}

/// Where every panel currently is.
#[derive(Resource, Clone, Debug)]
pub struct DockState {
    /// The side each open panel is on. Absent means closed, and because a
    /// kind maps to exactly one side, a panel cannot be in two places.
    placement: BTreeMap<PanelKind, DockSide>,
    /// Draw order within each side. Kept consistent with `placement`.
    order: BTreeMap<DockSide, Vec<PanelKind>>,
    /// How much room each side takes.
    sizes: BTreeMap<DockSide, f32>,
}

impl Default for DockState {
    /// The layout the game has always opened with: the inspector on the
    /// left, the listing on the right, and the log and jobs sharing the
    /// bottom.
    fn default() -> Self {
        let mut dock = Self {
            placement: BTreeMap::new(),
            order: BTreeMap::new(),
            sizes: BTreeMap::new(),
        };
        dock.sizes.insert(DockSide::Left, 260.0);
        dock.sizes.insert(DockSide::Right, 230.0);
        dock.sizes.insert(DockSide::Bottom, 190.0);
        dock.dock(PanelKind::Inspector, DockSide::Left);
        dock.dock(PanelKind::Listing, DockSide::Right);
        dock.dock(PanelKind::Log, DockSide::Bottom);
        dock.dock(PanelKind::Jobs, DockSide::Bottom);
        dock
    }
}

impl DockState {
    /// Which side a panel is on, or `None` if it is closed.
    pub fn side_of(&self, kind: PanelKind) -> Option<DockSide> {
        self.placement.get(&kind).copied()
    }

    /// The panels on a side, in draw order.
    pub fn panels_on(&self, side: DockSide) -> &[PanelKind] {
        self.order.get(&side).map(|v| v.as_slice()).unwrap_or(&[])
    }

    /// How much room a side takes.
    pub fn size_of(&self, side: DockSide) -> f32 {
        self.sizes.get(&side).copied().unwrap_or(240.0)
    }

    /// Moves a panel to a side, opening it if it was closed.
    ///
    /// Removing it from wherever it was first is what keeps the
    /// ordering lists agreeing with the placement map.
    pub fn dock(&mut self, kind: PanelKind, side: DockSide) {
        self.close(kind);
        self.placement.insert(kind, side);
        self.order.entry(side).or_default().push(kind);
    }

    /// Closes a panel, wherever it is.
    pub fn close(&mut self, kind: PanelKind) {
        if let Some(previous) = self.placement.remove(&kind)
            && let Some(list) = self.order.get_mut(&previous)
        {
            list.retain(|entry| *entry != kind);
        }
    }

    /// What a toolbar click does: dock to `side`, or close if it is
    /// already there.
    ///
    /// Clicking the button for a panel you are already looking at, on the
    /// side you are already looking at it, can only sensibly mean "put it
    /// away".
    pub fn toggle(&mut self, kind: PanelKind, side: DockSide) {
        if self.side_of(kind) == Some(side) {
            self.close(kind);
        } else {
            self.dock(kind, side);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_layout_is_the_one_the_game_has_always_had() {
        let dock = DockState::default();
        assert_eq!(dock.side_of(PanelKind::Inspector), Some(DockSide::Left));
        assert_eq!(dock.side_of(PanelKind::Listing), Some(DockSide::Right));
        assert_eq!(
            dock.panels_on(DockSide::Bottom),
            &[PanelKind::Log, PanelKind::Jobs],
            "the log and jobs share the bottom, log first"
        );
        assert_eq!(
            dock.side_of(PanelKind::Ledger),
            None,
            "the ledger is summoned, not permanent"
        );
    }

    #[test]
    fn a_panel_is_never_on_two_sides() {
        let mut dock = DockState::default();
        dock.dock(PanelKind::Inspector, DockSide::Right);
        assert_eq!(dock.side_of(PanelKind::Inspector), Some(DockSide::Right));
        assert!(
            !dock
                .panels_on(DockSide::Left)
                .contains(&PanelKind::Inspector),
            "moving a panel must remove it from where it was, not copy it"
        );
        // The invariant stated as the player would: it appears once, total.
        let total: usize = DockSide::ALL
            .iter()
            .map(|side| {
                dock.panels_on(*side)
                    .iter()
                    .filter(|k| **k == PanelKind::Inspector)
                    .count()
            })
            .sum();
        assert_eq!(total, 1);
    }

    #[test]
    fn toggling_the_side_it_is_already_on_puts_it_away() {
        let mut dock = DockState::default();
        dock.toggle(PanelKind::Inspector, DockSide::Left);
        assert_eq!(dock.side_of(PanelKind::Inspector), None);
        assert!(dock.panels_on(DockSide::Left).is_empty());
    }

    #[test]
    fn toggling_the_other_side_moves_it_rather_than_closing_it() {
        let mut dock = DockState::default();
        dock.toggle(PanelKind::Inspector, DockSide::Right);
        assert_eq!(dock.side_of(PanelKind::Inspector), Some(DockSide::Right));
    }

    #[test]
    fn order_within_a_side_is_stable_across_unrelated_moves() {
        let mut dock = DockState::default();
        dock.dock(PanelKind::Ledger, DockSide::Bottom);
        assert_eq!(
            dock.panels_on(DockSide::Bottom),
            &[PanelKind::Log, PanelKind::Jobs, PanelKind::Ledger],
            "a new panel joins the end rather than displacing what is there"
        );
        dock.close(PanelKind::Jobs);
        assert_eq!(
            dock.panels_on(DockSide::Bottom),
            &[PanelKind::Log, PanelKind::Ledger],
            "closing one leaves the others in the order they were"
        );
    }

    #[test]
    fn closing_something_already_closed_changes_nothing() {
        let mut dock = DockState::default();
        let before = format!("{dock:?}");
        dock.close(PanelKind::Ledger);
        assert_eq!(format!("{dock:?}"), before);
    }

    #[test]
    fn every_panel_has_a_title_and_a_description() {
        // The toolbar draws ALL and nothing else, so a panel missing from
        // it is one the player cannot reach — and a panel whose rows are
        // missing is one that draws blank.
        let strings = aeon_sim::TextDb::embedded();
        let mut seen: Vec<PanelKind> = Vec::new();
        for kind in PanelKind::ALL {
            assert!(!seen.contains(kind), "{kind:?} is listed twice");
            seen.push(*kind);
            assert!(strings.0.get(kind.title_key()).is_some(), "{kind:?} title");
            assert!(
                strings.0.get(kind.description_key()).is_some(),
                "{kind:?} description"
            );
        }
    }

    #[test]
    fn every_dock_side_has_a_label() {
        let strings = aeon_sim::TextDb::embedded();
        for side in [DockSide::Left, DockSide::Right, DockSide::Bottom] {
            assert!(strings.0.get(side.label_key()).is_some(), "{side:?}");
        }
    }
}
