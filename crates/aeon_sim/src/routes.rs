//! Authored route graph and deterministic journey progress.

use std::cmp::Reverse;
use std::collections::{BTreeMap, BinaryHeap};

use aeon_core::calendar::GameDate;
use aeon_data::ContentKey;
use aeon_data::model::{ContentSet, RouteKind};
use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::ids::ProvinceId;
use crate::map::MapIndex;

/// One resolved authored edge.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct RouteLeg {
    pub route: ContentKey,
    pub kind: RouteKind,
    pub from: ProvinceId,
    pub to: ProvinceId,
    pub travel_days: u32,
    pub risk: u16,
}

/// Deterministic adjacency index over authored routes.
#[derive(Resource, Clone, Debug, Default)]
pub struct RouteGraph {
    adjacency: BTreeMap<ProvinceId, Vec<RouteLeg>>,
}

impl RouteGraph {
    /// Canonical authored edges, one per undirected connection.
    pub fn routes(&self) -> Vec<&RouteLeg> {
        self.adjacency
            .values()
            .flatten()
            .filter(|leg| leg.from < leg.to)
            .collect()
    }
    /// Fastest path of one route kind, with the route-key sequence breaking ties.
    pub fn fastest_path(
        &self,
        kind: RouteKind,
        from: ProvinceId,
        to: ProvinceId,
    ) -> Option<Vec<RouteLeg>> {
        self.fastest_path_where(from, to, |leg| leg.kind == kind)
    }

    /// Fastest route across surface and space edges. Space edges only join
    /// starports in validated content, so a mixed path naturally walks to a
    /// port, crosses space, and walks on from the destination port.
    pub fn fastest_mixed_path(&self, from: ProvinceId, to: ProvinceId) -> Option<Vec<RouteLeg>> {
        self.fastest_path_where(from, to, |_| true)
    }

    fn fastest_path_where(
        &self,
        from: ProvinceId,
        to: ProvinceId,
        permitted: impl Fn(&RouteLeg) -> bool,
    ) -> Option<Vec<RouteLeg>> {
        if from == to {
            return Some(Vec::new());
        }
        let mut best: BTreeMap<ProvinceId, (u64, Vec<RouteLeg>)> = BTreeMap::new();
        let mut heap = BinaryHeap::new();
        heap.push(Reverse((0_u64, Vec::<RouteLeg>::new(), from)));
        best.insert(from, (0, Vec::new()));
        while let Some(Reverse((cost, path, at))) = heap.pop() {
            if at == to {
                return Some(path);
            }
            if best.get(&at) != Some(&(cost, path.clone())) {
                continue;
            }
            for leg in self.adjacency.get(&at).into_iter().flatten() {
                if !permitted(leg) {
                    continue;
                }
                let next_cost = cost + u64::from(leg.travel_days);
                let mut next_path = path.clone();
                next_path.push(leg.clone());
                let candidate = (next_cost, next_path.clone());
                if best.get(&leg.to).is_none_or(|current| candidate < *current) {
                    best.insert(leg.to, candidate);
                    heap.push(Reverse((next_cost, next_path, leg.to)));
                }
            }
        }
        None
    }

    pub fn path_days(path: &[RouteLeg]) -> i64 {
        path.iter().map(|leg| i64::from(leg.travel_days)).sum()
    }
}

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum JourneyPurpose {
    #[default]
    Travel,
    Assignment,
    Retreat,
    Appointment,
}

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum JourneySpeed {
    #[default]
    Normal,
    Half,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RouteProgress {
    pub leg: RouteLeg,
    pub arrives: GameDate,
}

/// Persistable route plan attached to a character, ship, or army entity.
#[derive(Component, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Journey {
    pub destination: ProvinceId,
    pub current: Option<RouteProgress>,
    pub remaining: Vec<RouteLeg>,
    pub speed: JourneySpeed,
    pub purpose: JourneyPurpose,
}

impl Journey {
    pub fn new(destination: ProvinceId, remaining: Vec<RouteLeg>, purpose: JourneyPurpose) -> Self {
        Self {
            destination,
            current: None,
            remaining,
            speed: JourneySpeed::Normal,
            purpose,
        }
    }

    pub fn arrival(&self, date: GameDate) -> GameDate {
        let current = self
            .current
            .as_ref()
            .map_or(0, |p| date.days_until(p.arrives).max(0));
        let multiplier = if self.speed == JourneySpeed::Half {
            2
        } else {
            1
        };
        date.add_days(
            current
                + self
                    .remaining
                    .iter()
                    .map(|leg| i64::from(leg.travel_days) * multiplier)
                    .sum::<i64>(),
        )
    }
}

/// Builds the graph after province IDs have been allocated.
pub fn build(world: &mut World, content: &ContentSet) {
    let map = world.resource::<MapIndex>();
    let mut graph = RouteGraph::default();
    for route in content.routes.values() {
        let a = map.province_keys[&route.a];
        let b = map.province_keys[&route.b];
        for (from, to) in [(a, b), (b, a)] {
            graph.adjacency.entry(from).or_default().push(RouteLeg {
                route: route.key.clone(),
                kind: route.kind,
                from,
                to,
                travel_days: route.travel_days,
                risk: route.risk,
            });
        }
    }
    for legs in graph.adjacency.values_mut() {
        legs.sort_by(|a, b| a.route.cmp(&b.route).then(a.to.cmp(&b.to)));
    }
    world.insert_resource(graph);
}
