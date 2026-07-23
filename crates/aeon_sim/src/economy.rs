//! Organisation resources and the provincial economy.
//!
//! Wealth, manpower, and supplies accrue monthly from held province
//! titles. Influence is the spendable political resource; legitimacy is
//! the non-spendable standing that caps it and drives its recharge, with
//! a bonus for holding a paramount title.

use bevy::app::App;
use bevy::prelude::{Component, IntoScheduleConfigs, World};
use serde::{Deserialize, Serialize};

use crate::clock::MonthlyPulse;
use crate::ids::OrgId;
use crate::politics::{PoliticsIndex, TitleHolder, TitleKind, TitleRecord};
use crate::state::ContentDb;

/// Effective-legitimacy bonus for holding a paramount title.
pub const PARAMOUNT_LEGITIMACY_BONUS: i32 = 20;

/// An organisation's strategic resources.
#[derive(Component, Copy, Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct OrgResources {
    /// Fungible economic capacity.
    pub wealth: i64,
    /// People available to staff assignments, armies, and holdings.
    pub manpower: i64,
    /// Material support for sustained operations.
    pub supplies: i64,
    /// Spendable political capital.
    pub influence: i64,
    /// Non-spendable standing, 0..=100, from content at spawn.
    pub legitimacy: i32,
}

impl OrgResources {
    /// Whether the organisation can afford the given costs.
    pub fn can_afford(&self, wealth: i64, manpower: i64, supplies: i64, influence: i64) -> bool {
        self.wealth >= wealth
            && self.manpower >= manpower
            && self.supplies >= supplies
            && self.influence >= influence
    }

    /// Deducts the given costs. Callers must have checked affordability.
    pub fn spend(&mut self, wealth: i64, manpower: i64, supplies: i64, influence: i64) {
        self.wealth -= wealth;
        self.manpower -= manpower;
        self.supplies -= supplies;
        self.influence -= influence;
    }
}

/// Effective legitimacy: authored standing plus title bonuses.
pub fn effective_legitimacy(world: &World, org: OrgId) -> i32 {
    let index = world.resource::<PoliticsIndex>();
    let base = crate::access::org_entity(world, org)
        .and_then(|e| world.get::<OrgResources>(e))
        .map(|r| r.legitimacy)
        .unwrap_or(0);
    let holds_paramountcy = index.titles.values().any(|entity| {
        world.get::<TitleRecord>(*entity).is_some_and(|t| {
            matches!(t.kind, TitleKind::Paramount(_)) && t.holder == TitleHolder::Org(org)
        })
    });
    let bonus = if holds_paramountcy {
        PARAMOUNT_LEGITIMACY_BONUS
    } else {
        0
    };
    (base + bonus).clamp(0, 120)
}

/// Monthly: provincial production accrues to title holders, and
/// legitimacy recharges influence up to its cap.
pub fn monthly_economy(world: &mut World) {
    if world.get_resource::<PoliticsIndex>().is_none() {
        return;
    }
    let content = world.resource::<ContentDb>().0.clone();

    // Production by held province titles, accumulated per organisation.
    let mut income: std::collections::BTreeMap<OrgId, (i64, i64, i64)> =
        std::collections::BTreeMap::new();
    {
        let index = world.resource::<PoliticsIndex>();
        let map_index = world.resource::<crate::map::MapIndex>();
        for entity in index.titles.values() {
            let Some(title) = world.get::<TitleRecord>(*entity) else {
                continue;
            };
            let (TitleKind::Province(province), TitleHolder::Org(org)) = (title.kind, title.holder)
            else {
                continue;
            };
            let Some(key) = map_index.province_ids.get(&province) else {
                continue;
            };
            let Some(def) = content.provinces.get(key) else {
                continue;
            };
            // A blockaded province yields half its wealth.
            let blockaded = world
                .get_resource::<crate::forces::ForcesIndex>()
                .is_some_and(|forces| {
                    forces.ships.values().any(|ship_entity| {
                        world
                            .get::<crate::forces::ShipRecord>(*ship_entity)
                            .is_some_and(|ship| ship.blockading == Some(province))
                    })
                });
            // A province yields in proportion to its order: a compliant
            // population pays and musters, a resentful one does neither.
            let factor = crate::order::output_factor_permille(
                crate::order::province_order(world, province).order,
            );
            let scaled = |amount: i64| -> i64 { amount * factor / 1000 };
            // A province's plain wealth, plus whatever its buildings add.
            let base_wealth =
                def.wealth_output + crate::trade::building_wealth_bonus(world, province);
            let entry = income.entry(org).or_default();
            entry.0 += scaled(if blockaded {
                base_wealth / 2
            } else {
                base_wealth
            });
            entry.1 += scaled(def.manpower_output);
            entry.2 += scaled(def.supplies_output);
        }
    }

    for org in crate::access::org_ids(world) {
        let cap = i64::from(effective_legitimacy(world, org));
        let entity = crate::access::org_entity(world, org).expect("indexed");
        let Some(mut resources) = world.get_mut::<OrgResources>(entity) else {
            continue;
        };
        if let Some((wealth, manpower, supplies)) = income.get(&org) {
            resources.wealth += wealth;
            resources.manpower += manpower;
            resources.supplies += supplies;
        }
        // Influence recharges by a tenth of effective legitimacy, capped.
        resources.influence = (resources.influence + cap / 10).min(cap);
    }
}

pub(crate) fn install(app: &mut App) {
    app.add_systems(
        MonthlyPulse,
        monthly_economy.before(crate::politics::expire_opinion_modifiers),
    );
}
