//! Cross-reference validation, run once every file has been loaded.
//!
//! Builders validate each definition in isolation; this pass validates the
//! references *between* them — a assignment's effect functions exist in its own
//! file, a moon's parent is a defined planet, a vassal's liege is a great
//! house, a ship's captain belongs to its owner. Findings accumulate so a
//! single load reports every broken reference in a content set.

use std::collections::{BTreeMap, BTreeSet};

use crate::key::ContentKey;
use crate::model::{
    AssignmentDef, AssignmentTargetKind, BodyKind, HouseTier, OrgKind, OutcomeKind, PlanStepAction,
    PlanTargetSelector, ShipClass, TitleKindDef,
};

use super::builders::BuilderState;

/// Post-pass validation once every file has run.
pub(super) fn validate_cross_references(
    builder: &mut BuilderState,
    fn_names: &BTreeMap<String, BTreeSet<String>>,
) {
    // Assignments: mandatory results, effect functions must exist in their file.
    let mut findings: Vec<(String, Option<String>, String)> = Vec::new();
    for (key, assignment) in &builder.assignments {
        for required in [OutcomeKind::Success, OutcomeKind::Failure] {
            if !assignment.results.contains_key(&required) {
                findings.push((
                    fn_ref_path(assignment),
                    Some(key.to_string()),
                    format!("assignments must define a {required:?} result"),
                ));
            }
        }
        for result in assignment.results.values() {
            let fn_refs = result
                .effect_fn
                .iter()
                .chain(result.choices.iter().filter_map(|c| c.effect_fn.as_ref()));
            for fn_ref in fn_refs {
                let exists = fn_names
                    .get(&fn_ref.path)
                    .is_some_and(|names| names.contains(&fn_ref.name));
                if !exists {
                    findings.push((
                        fn_ref.path.clone(),
                        Some(key.to_string()),
                        format!(
                            "effect_fn '{}' is not defined in this file (function references are file-local)",
                            fn_ref.name
                        ),
                    ));
                }
            }
            // Whether a popup has body text is the string table's to
            // answer, and it is not consulted until after this pass; the
            // fill reports a popup result whose row is missing.
        }
    }

    // Events: effect functions are file-local, like a assignment's.
    for (key, event) in &builder.events {
        let fn_refs = event
            .effect_fn
            .iter()
            .chain(event.choices.iter().filter_map(|c| c.effect_fn.as_ref()));
        for fn_ref in fn_refs {
            let exists = fn_names
                .get(&fn_ref.path)
                .is_some_and(|names| names.contains(&fn_ref.name));
            if !exists {
                findings.push((
                    fn_ref.path.clone(),
                    Some(key.to_string()),
                    format!(
                        "effect_fn '{}' is not defined in this file (function references are file-local)",
                        fn_ref.name
                    ),
                ));
            }
        }
    }

    // Bodies: parent structure.
    for (key, body) in &builder.bodies {
        match (&body.kind, &body.parent) {
            (BodyKind::Planet, Some(_)) => findings.push((
                String::new(),
                Some(key.to_string()),
                "planets must not declare a parent".to_owned(),
            )),
            (BodyKind::Planet, None) => {}
            (_, None) => findings.push((
                String::new(),
                Some(key.to_string()),
                "moons and starbases must declare a parent".to_owned(),
            )),
            (_, Some(parent)) => match builder.bodies.get(parent) {
                None => findings.push((
                    String::new(),
                    Some(key.to_string()),
                    format!("parent body '{parent}' is not defined"),
                )),
                Some(parent_body) if parent_body.kind != BodyKind::Planet => findings.push((
                    String::new(),
                    Some(key.to_string()),
                    format!("parent body '{parent}' must be a planet"),
                )),
                Some(_) if body.orbit_days == 0 => findings.push((
                    String::new(),
                    Some(key.to_string()),
                    "orbiting bodies must declare orbit_days".to_owned(),
                )),
                Some(_) => {}
            },
        }
    }

    // Provinces: bodies must exist, and every good produced or consumed
    // must be a defined good.
    for (key, province) in &builder.provinces {
        if !builder.bodies.contains_key(&province.body) {
            findings.push((
                String::new(),
                Some(key.to_string()),
                format!("body '{}' is not defined", province.body),
            ));
        }
        for good in province.produces.keys().chain(province.consumes.keys()) {
            if !builder.goods.contains_key(good) {
                findings.push((
                    String::new(),
                    Some(key.to_string()),
                    format!("good '{good}' is not defined"),
                ));
            }
        }
    }

    // Buildings: every good they produce or consume must be defined.
    for (key, building) in &builder.buildings {
        for good in building.produces.keys().chain(building.consumes.keys()) {
            if !builder.goods.contains_key(good) {
                findings.push((
                    String::new(),
                    Some(key.to_string()),
                    format!("good '{good}' is not defined"),
                ));
            }
        }
    }

    validate_political_references(builder, &mut findings);
    validate_plans(builder, &mut findings);

    for (path, key, message) in findings {
        builder.report.error(&path, key.as_deref(), message);
    }
}

/// Cross-reference validation for traits, characters, organisations,
/// titles, and offices.
fn validate_political_references(
    builder: &BuilderState,
    findings: &mut Vec<(String, Option<String>, String)>,
) {
    if !builder.characters.is_empty() && builder.name_pools.is_empty() {
        findings.push((
            String::new(),
            None,
            "content with characters must define a name pool (births need names)".to_owned(),
        ));
    }

    let mut err = |key: &ContentKey, message: String| {
        findings.push((String::new(), Some(key.to_string()), message));
    };

    for (key, trait_def) in &builder.traits {
        for opposite in &trait_def.opposites {
            if !builder.traits.contains_key(opposite) {
                err(key, format!("opposite trait '{opposite}' is not defined"));
            }
        }
    }

    for (key, character) in &builder.characters {
        if let Some(org) = &character.organisation
            && !builder.organisations.contains_key(org)
        {
            err(key, format!("organisation '{org}' is not defined"));
        }
        for parent in &character.parents {
            if !builder.characters.contains_key(parent) {
                err(key, format!("parent '{parent}' is not defined"));
            }
        }
        if let Some(spouse) = &character.spouse {
            match builder.characters.get(spouse) {
                None => err(key, format!("spouse '{spouse}' is not defined")),
                Some(_) if spouse == key => {
                    err(key, "characters cannot marry themselves".to_owned());
                }
                Some(other) => {
                    // Marriage is symmetric; the simulation mirrors
                    // one-sided declarations, but a conflicting
                    // declaration is an authoring error.
                    if let Some(their_spouse) = &other.spouse
                        && their_spouse != key
                    {
                        err(
                            key,
                            format!(
                                "spouse '{spouse}' declares a different spouse '{their_spouse}'"
                            ),
                        );
                    }
                }
            }
        }
        for trait_key in &character.traits {
            if !builder.traits.contains_key(trait_key) {
                err(key, format!("trait '{trait_key}' is not defined"));
            }
        }
    }

    let mut province_holders: BTreeMap<&ContentKey, &ContentKey> = BTreeMap::new();
    for (key, org) in &builder.organisations {
        match org.kind {
            OrgKind::DynasticHouse => match (org.tier, &org.liege) {
                (Some(HouseTier::Vassal), None) => {
                    err(key, "vassal houses must declare a liege".to_owned());
                }
                (Some(HouseTier::Vassal), Some(liege)) => match builder.organisations.get(liege) {
                    None => err(key, format!("liege '{liege}' is not defined")),
                    Some(liege_org) if liege_org.tier != Some(HouseTier::Great) => {
                        err(key, format!("liege '{liege}' must be a great house"));
                    }
                    Some(_) => {}
                },
                (Some(_), Some(_)) => {
                    err(key, "only vassal houses declare a liege".to_owned());
                }
                (Some(_), None) => {}
                (None, _) => err(key, "houses must declare a tier".to_owned()),
            },
            OrgKind::SanctoraImperim => {
                if org.tier.is_some() || org.liege.is_some() {
                    err(
                        key,
                        "the Sanctora Imperim has neither tier nor liege".to_owned(),
                    );
                }
            }
        }
        if let Some(head) = &org.head {
            match builder.characters.get(head) {
                None => err(key, format!("head '{head}' is not defined")),
                Some(character) if character.organisation.as_ref() != Some(key) => {
                    err(
                        key,
                        format!("head '{head}' does not belong to this organisation"),
                    );
                }
                Some(_) => {}
            }
        }
        for province in &org.provinces {
            if !builder.provinces.contains_key(province) {
                err(key, format!("province '{province}' is not defined"));
            }
            if let Some(other) = province_holders.insert(province, key) {
                err(
                    key,
                    format!("province '{province}' is already held by '{other}'"),
                );
            }
        }
    }

    for (key, title) in &builder.titles {
        if let TitleKindDef::Paramount { body } = &title.kind
            && !builder.bodies.contains_key(body)
        {
            err(key, format!("body '{body}' is not defined"));
        }
        match &title.holder {
            crate::model::TitleHolderDef::Organisation(org) => {
                if !builder.organisations.contains_key(org) {
                    err(key, format!("holder organisation '{org}' is not defined"));
                }
            }
            crate::model::TitleHolderDef::Character(character) => {
                if !builder.characters.contains_key(character) {
                    err(
                        key,
                        format!("holder character '{character}' is not defined"),
                    );
                }
            }
            crate::model::TitleHolderDef::Vacant => {}
        }
        if matches!(title.kind, TitleKindDef::Consul)
            && matches!(title.holder, crate::model::TitleHolderDef::Organisation(_))
        {
            err(
                key,
                "the Consul title is held personally, not by an organisation".to_owned(),
            );
        }
    }

    for (key, office) in &builder.offices {
        if !builder.organisations.contains_key(&office.organisation) {
            err(
                key,
                format!("organisation '{}' is not defined", office.organisation),
            );
        }
        if let Some(province) = &office.province
            && !builder.provinces.contains_key(province)
        {
            err(key, format!("province '{province}' is not defined"));
        }
        if let Some(holder) = &office.holder
            && !builder.characters.contains_key(holder)
        {
            err(key, format!("holder '{holder}' is not defined"));
        }
    }

    for (key, ship) in &builder.ships {
        if !builder.organisations.contains_key(&ship.owner) {
            err(key, format!("owner '{}' is not defined", ship.owner));
        }
        if !builder.provinces.contains_key(&ship.location) {
            err(key, format!("location '{}' is not defined", ship.location));
        }
        match (&ship.class, &ship.captain) {
            (ShipClass::Capital, None) => {
                err(key, "capital ships must have a captain".to_owned());
            }
            (_, Some(captain)) if !builder.characters.contains_key(captain) => {
                err(key, format!("captain '{captain}' is not defined"));
            }
            // A ship is commanded by one of its owner's own officers, the
            // same rule an army's general has always had.
            (_, Some(captain))
                if builder
                    .characters
                    .get(captain)
                    .is_some_and(|c| c.organisation.as_ref() != Some(&ship.owner)) =>
            {
                err(
                    key,
                    format!("captain '{captain}' does not belong to the owning house"),
                );
            }
            _ => {}
        }
    }

    for (key, army) in &builder.armies {
        if !builder.organisations.contains_key(&army.owner) {
            err(key, format!("owner '{}' is not defined", army.owner));
        }
        if !builder.provinces.contains_key(&army.province) {
            err(key, format!("province '{}' is not defined", army.province));
        }
        match builder.characters.get(&army.general) {
            None => err(key, format!("general '{}' is not defined", army.general)),
            Some(general) if general.organisation.as_ref() != Some(&army.owner) => {
                err(
                    key,
                    format!(
                        "general '{}' does not belong to the owning house",
                        army.general
                    ),
                );
            }
            Some(_) => {}
        }
    }

    for (key, obligation) in &builder.obligations {
        if !builder.organisations.contains_key(&obligation.debtor) {
            err(
                key,
                format!("debtor '{}' is not defined", obligation.debtor),
            );
        }
        if !builder.organisations.contains_key(&obligation.creditor) {
            err(
                key,
                format!("creditor '{}' is not defined", obligation.creditor),
            );
        }
    }

    if let Some(scenario) = &builder.scenario
        && let Some(player_house) = &scenario.player_house
    {
        match builder.organisations.get(player_house) {
            None => {
                findings.push((
                    String::new(),
                    Some(scenario.key.to_string()),
                    format!("player_house '{player_house}' is not defined"),
                ));
            }
            Some(org) if org.kind != OrgKind::DynasticHouse => {
                findings.push((
                    String::new(),
                    Some(scenario.key.to_string()),
                    "player_house must be a dynastic house".to_owned(),
                ));
            }
            Some(org) if org.head.is_none() => {
                findings.push((
                    String::new(),
                    Some(scenario.key.to_string()),
                    "player_house must have an authored head".to_owned(),
                ));
            }
            Some(_) => {}
        }
    }
}

/// How deep sub-plans may nest. A plan counting only itself has depth 1.
const MAX_PLAN_DEPTH: usize = 3;

/// Cross-reference validation for plans: every step's key exists, step
/// targets are compatible with what they name, and sub-plans compose
/// acyclically to a shallow depth.
fn validate_plans(builder: &BuilderState, findings: &mut Vec<(String, Option<String>, String)>) {
    let mut err = |key: &ContentKey, message: String| {
        findings.push((String::new(), Some(key.to_string()), message));
    };

    for (key, plan) in &builder.plans {
        for method in &plan.methods {
            for step in &method.steps {
                match &step.action {
                    PlanStepAction::Assignment {
                        key: assignment,
                        target,
                    } => match builder.assignments.get(assignment) {
                        None => err(
                            key,
                            format!(
                                "step '{}': assignment '{assignment}' is not defined",
                                step.id
                            ),
                        ),
                        Some(def) => {
                            // The step's selector must produce exactly the
                            // target kind the assignment demands, so a plan
                            // cannot author a step its runtime could never
                            // start.
                            let produced = match target {
                                PlanTargetSelector::None => AssignmentTargetKind::None,
                                PlanTargetSelector::PlanTarget => plan.target,
                                PlanTargetSelector::WorstHolding => AssignmentTargetKind::Province,
                            };
                            if *target == PlanTargetSelector::PlanTarget
                                && plan.target == AssignmentTargetKind::None
                            {
                                err(
                                    key,
                                    format!(
                                        "step '{}' aims at the plan's target, but the plan has none",
                                        step.id
                                    ),
                                );
                            } else if def.target != produced {
                                err(
                                    key,
                                    format!(
                                        "step '{}': assignment '{assignment}' wants a {:?} target, \
                                         but the step provides {:?}",
                                        step.id, def.target, produced
                                    ),
                                );
                            }
                        }
                    },
                    PlanStepAction::Orders { orders, .. } => {
                        for order in orders {
                            match builder.assignments.get(order) {
                                None => err(
                                    key,
                                    format!(
                                        "step '{}': standing order '{order}' is not defined",
                                        step.id
                                    ),
                                ),
                                // Only orders a force can actually take up:
                                // the standing-orders pass targets the army
                                // itself or the army and a province, and an
                                // assignment of any other kind would sit in
                                // the list doing nothing forever.
                                Some(def)
                                    if !matches!(
                                        def.target,
                                        AssignmentTargetKind::OwnArmy
                                            | AssignmentTargetKind::OwnArmyAndProvince
                                    ) =>
                                {
                                    err(
                                        key,
                                        format!(
                                            "step '{}': '{order}' cannot be a standing order; \
                                             it does not target an army",
                                            step.id
                                        ),
                                    );
                                }
                                Some(_) => {}
                            }
                        }
                    }
                    PlanStepAction::SubPlan(sub) => match builder.plans.get(sub) {
                        None => err(
                            key,
                            format!("step '{}': sub-plan '{sub}' is not defined", step.id),
                        ),
                        // A sub-plan expands inside another plan's target
                        // context; giving it a target of its own would
                        // leave two answers to one question.
                        Some(def) if def.target != AssignmentTargetKind::None => err(
                            key,
                            format!(
                                "step '{}': sub-plan '{sub}' declares a target; \
                                 only targetless plans may be sub-plans",
                                step.id
                            ),
                        ),
                        Some(_) => {}
                    },
                }
            }
        }
    }

    // Sub-plan composition: acyclic, and no deeper than MAX_PLAN_DEPTH.
    for key in builder.plans.keys() {
        let mut trail: Vec<&ContentKey> = Vec::new();
        if let Some(message) = plan_depth_problem(builder, key, &mut trail) {
            err(key, message);
        }
    }
}

/// Walks a plan's sub-plans depth-first, reporting a cycle or excessive
/// depth. `trail` carries the path walked so a cycle can be named.
fn plan_depth_problem<'a>(
    builder: &'a BuilderState,
    key: &'a ContentKey,
    trail: &mut Vec<&'a ContentKey>,
) -> Option<String> {
    if trail.contains(&key) {
        let named: Vec<String> = trail.iter().map(|k| k.to_string()).collect();
        return Some(format!(
            "sub-plans form a cycle: {} -> {key}",
            named.join(" -> ")
        ));
    }
    trail.push(key);
    if trail.len() > MAX_PLAN_DEPTH {
        let named: Vec<String> = trail.iter().map(|k| k.to_string()).collect();
        trail.pop();
        return Some(format!(
            "sub-plans nest deeper than {MAX_PLAN_DEPTH}: {}",
            named.join(" -> ")
        ));
    }
    let problem = builder.plans.get(key).and_then(|plan| {
        plan.methods.iter().find_map(|method| {
            method.steps.iter().find_map(|step| match &step.action {
                PlanStepAction::SubPlan(sub) => plan_depth_problem(builder, sub, trail),
                PlanStepAction::Assignment { .. } | PlanStepAction::Orders { .. } => None,
            })
        })
    });
    trail.pop();
    problem
}

fn fn_ref_path(assignment: &AssignmentDef) -> String {
    assignment
        .results
        .values()
        .find_map(|r| r.effect_fn.as_ref().map(|f| f.path.clone()))
        .unwrap_or_default()
}
