// The First Reign arc — scenario-authored onboarding pressure for the
// Ashkarr Succession. These are ordinary urgent Situations, framed as
// in-world politics: the simulation rules, deadlines, and consequences
// exist identically whether or not the client's guidance preference is on.
//
// Gameplay values live here, not in Rust: the length of the court's
// patience and the Influence forfeited when it lapses are authored data.
// Script functions cannot see file-scope constants, so the values are
// returned by named functions the trigger, projection, and outcomes share.

fn court_days() { 7 }
fn court_influence_penalty() { 10 }

// --- The Court Awaits -------------------------------------------------------
//
// On the opening day of Edrun Harrow's reign the assembled court demands a
// substantive order. Any accepted ordinary House Harrow assignment within
// seven days answers it; letting the deadline lapse forfeits Influence.
// The Situation activates exactly once, on the campaign's first day, and
// its resolution is durable either way.

define_situation(#{
    id: "court-awaits",
    source: "scenario",
    bindings: #{ house: "organisation" },
    owner_binding: "house",
    trigger_fn: "court_awaits_instances",
    projection_fn: "court_awaits_projection",
    priority: 120,
    log_activation: true,
    audience: ["house"],
    stages: ["waiting"],
    actions: [#{ id: "hold-court", assignment: "hold-court" }],
    outcomes: [
        #{ id: "answered", when_fn: "court_was_answered" },
        #{ id: "lapsed", when_fn: "court_demand_lapsed", effects_fn: "court_lapsed_effects" },
        #{ id: "closed", fallback: true },
    ],
});

fn first_reign_house(world) {
    for organisation in world.organisations {
        if organisation.key == "harrow" { return organisation.id; }
    }
    0
}

// Whether an ordinary assignment for the house was accepted inside the
// court's window: started on or after activation, no later than the
// deadline day. Acceptance is what counts — every authored assignment
// outlasts the window, so the accepted work is still running when the
// court's patience would have lapsed.
fn court_demand_answered(world, house, activated) {
    for work in world.assignments {
        if work.owner == house
                && work.started >= activated
                && work.started <= activated + court_days() {
            return true;
        }
    }
    false
}

fn court_awaits_instances(ctx) {
    let house = first_reign_house(ctx.world);
    if house == 0 { return []; }
    if type_of(ctx.activated) == "i64" {
        // An open demand continues until it is answered or lapses.
        if court_demand_answered(ctx.world, house, ctx.activated) { return []; }
        if ctx.world.date >= ctx.activated + court_days() { return []; }
        return [#{ house: house }];
    }
    // The court gathers once, on the opening day of the reign. A resolved
    // demand never reactivates: after day one this trigger stays silent.
    if ctx.world.date == ctx.world.start_date {
        return [#{ house: house }];
    }
    []
}

fn court_awaits_projection(ctx) {
    let house = ctx.bindings.house;
    let deadline = ctx.activated + court_days();
    let head = 0;
    let influence = 0;
    for organisation in ctx.world.organisations {
        if organisation.id == house {
            head = organisation.head;
            influence = organisation.influence;
        }
    }
    let actions = [];
    if type_of(head) == "i64" && head > 0 {
        actions.push(#{ id: "hold-court", leader: head });
    }
    #{
        stage: "waiting",
        deadline: deadline,
        warning: true,
        metrics: [
            #{ label_key: "situation.metric.days-left", value: deadline - ctx.world.date },
            #{ label_key: "situation.metric.influence-at-stake", value: court_influence_penalty() },
            #{ label_key: "situation.metric.house-influence", value: influence },
        ],
        participants: [#{ kind: "organisation", id: house }],
        actions: actions,
        resolution_values: #{
            house: court_house_name(ctx.world, house),
            penalty: `${court_influence_penalty()}`,
        },
    }
}

fn court_was_answered(ctx) {
    court_demand_answered(ctx.world, ctx.bindings.house, ctx.activated)
}

fn court_demand_lapsed(ctx) {
    ctx.world.date >= ctx.activated + court_days()
}

// The stated consequence, applied exactly once when the demand lapses:
// the house forfeits the authored Influence, no more and no less.
fn court_lapsed_effects(ctx) {
    [#{ kind: "resources", influence: -court_influence_penalty() }]
}

fn court_house_name(world, id) {
    for organisation in world.organisations {
        if organisation.id == id { return organisation.name; }
    }
    "Unknown organisation"
}

fn first_reign_character_name(world, id) {
    for character in world.characters {
        if character.id == id { return character.name; }
    }
    "Unknown character"
}

// --- The household demands ---------------------------------------------------
//
// Once the court's opening test is behind the reign — answered, or lapsed —
// the household presents its expectations. All demands share one deadline,
// anchored to pure campaign facts rather than to any activation quirk: the
// day the court's patience would have ended, plus the authored household
// window. The four-tier relationship consequences are likewise shared
// authored data: achieving a goal always earns the good tier, whatever was
// said; a promise only changes the price of failure.

fn household_days() { 120 }

fn household_achieved_opinion() { 10 }
fn household_achieved_days() { 1440 }
fn household_refused_opinion() { -5 }
fn household_refused_days() { 1080 }
fn household_ignored_opinion() { -10 }
fn household_ignored_days() { 1440 }
fn household_broken_opinion() { -20 }
fn household_broken_days() { 1800 }

// The one shared deadline, a pure function of campaign facts.
fn household_deadline(world) {
    world.start_date + court_days() + household_days()
}

// The demands open the day the court is satisfied or its window closes.
// Only activation checks this: a live demand never re-tests it, because the
// accepted work that answered the court eventually completes and leaves the
// assignment list.
fn household_demands_open(world, house) {
    if court_demand_answered(world, house, world.start_date) { return true; }
    world.date >= world.start_date + court_days()
}

// Who presses a household demand: the named family member while they live
// in the house and do not head it; otherwise the first living adult
// non-head member in stable ID order. Trigger, projection, and outcomes
// all use this one pure rule, so death and replacement resolve identically
// everywhere.
fn household_requester(world, preferred, house) {
    let head = 0;
    for organisation in world.organisations {
        if organisation.id == house {
            if type_of(organisation.head) == "i64" { head = organisation.head; }
        }
    }
    for character in world.characters {
        if type_of(character.key) == "string" && character.key == preferred
                && character.alive && character.organisation == house
                && character.id != head {
            return character.id;
        }
    }
    for character in world.characters {
        if character.alive && character.adult && character.organisation == house
                && character.id != head {
            return character.id;
        }
    }
    0
}

// --- Kessarin's Order --------------------------------------------------------
//
// Kessarin asks that every province the house holds stand at or above the
// authored Order target by the shared deadline. The goal is a live-state
// predicate: it is satisfied by the provinces themselves, however the
// player got them there, and whatever was promised. The demand resolves
// the first settled day the goal holds; if the target later slips before
// the shared deadline, the household may raise the demand again as a new
// lifecycle.

fn kessarin_order_target() { 850 }

fn kessarin_goal_met(world, house) {
    let held = 0;
    for province in world.provinces {
        if type_of(province.holder) == "i64" && province.holder == house {
            held += 1;
            if province.order < kessarin_order_target() { return false; }
        }
    }
    held > 0
}

define_situation(#{
    id: "kessarin-order",
    source: "scenario",
    bindings: #{ house: "organisation", requester: "character" },
    owner_binding: "house",
    subject_binding: "requester",
    trigger_fn: "kessarin_order_instances",
    projection_fn: "kessarin_order_projection",
    priority: 110,
    log_activation: true,
    audience: ["house"],
    stages: ["open", "promised", "refused"],
    actions: [
        #{ id: "manage-estates", assignment: "manage-estates" },
        #{ id: "hold-court", assignment: "hold-court" },
        #{ id: "tour-holdings", assignment: "tour-holdings" },
    ],
    responses: [
        #{ id: "promise" },
        #{ id: "refuse" },
    ],
    outcomes: [
        #{ id: "achieved", when_fn: "kessarin_goal_reached", effects_fn: "kessarin_achieved_effects" },
        #{ id: "passed-on", when_fn: "kessarin_demand_passed_on" },
        #{ id: "refused", when_fn: "kessarin_was_refused", effects_fn: "kessarin_refused_effects" },
        #{ id: "broken", when_fn: "kessarin_promise_broken", effects_fn: "kessarin_broken_effects" },
        #{ id: "ignored", fallback: true, effects_fn: "kessarin_ignored_effects" },
    ],
});

fn kessarin_order_instances(ctx) {
    let house = first_reign_house(ctx.world);
    if house == 0 { return []; }
    if ctx.world.date >= household_deadline(ctx.world) { return []; }
    if kessarin_goal_met(ctx.world, house) { return []; }
    let requester = household_requester(ctx.world, "kessarin-harrow", house);
    if requester == 0 { return []; }
    if type_of(ctx.activated) != "i64" && !household_demands_open(ctx.world, house) {
        return [];
    }
    [#{ house: house, requester: requester }]
}

fn kessarin_order_projection(ctx) {
    let house = ctx.bindings.house;
    let requester = ctx.bindings.requester;
    let deadline = household_deadline(ctx.world);
    let head = 0;
    for organisation in ctx.world.organisations {
        if organisation.id == house {
            if type_of(organisation.head) == "i64" { head = organisation.head; }
        }
    }
    let stage = "open";
    if type_of(ctx.answer) == "string" {
        if ctx.answer == "promise" { stage = "promised"; }
        if ctx.answer == "refuse" { stage = "refused"; }
    }
    let below = 0;
    let links = [];
    let metrics = [
        #{ label_key: "situation.metric.order-target", value: kessarin_order_target() },
        #{ label_key: "situation.metric.days-left", value: deadline - ctx.world.date },
    ];
    for province in ctx.world.provinces {
        if type_of(province.holder) == "i64" && province.holder == house {
            metrics.push(#{
                label_key: "situation.metric.province-order",
                value: `${province.name} — ${province.order}`,
            });
            if province.order < kessarin_order_target() {
                below += 1;
                links.push(#{ kind: "province", id: province.id });
            }
        }
    }
    metrics.push(#{ label_key: "situation.metric.provinces-below-target", value: below });
    metrics.push(#{
        label_key: "situation.metric.on-achieved",
        value: `+${household_achieved_opinion()} / ${household_achieved_days()}`,
    });
    metrics.push(#{
        label_key: "situation.metric.on-refused",
        value: `${household_refused_opinion()} / ${household_refused_days()}`,
    });
    metrics.push(#{
        label_key: "situation.metric.on-ignored",
        value: `${household_ignored_opinion()} / ${household_ignored_days()}`,
    });
    metrics.push(#{
        label_key: "situation.metric.on-broken",
        value: `${household_broken_opinion()} / ${household_broken_days()}`,
    });
    let actions = [];
    if head > 0 {
        actions.push(#{ id: "manage-estates", leader: head });
        actions.push(#{ id: "hold-court", leader: head });
        actions.push(#{ id: "tour-holdings", leader: head });
    }
    // A refusal quiets the card; an open or promised demand raises the
    // shared attention strip once the window narrows.
    let warning = stage != "refused" && deadline - ctx.world.date <= 30;
    #{
        stage: stage,
        deadline: deadline,
        warning: warning,
        metrics: metrics,
        participants: [
            #{ kind: "character", id: requester },
            #{ kind: "organisation", id: house },
        ],
        links: links,
        actions: actions,
        resolution_values: #{
            requester: first_reign_character_name(ctx.world, requester),
            house: court_house_name(ctx.world, house),
            target: `${kessarin_order_target()}`,
        },
    }
}

fn kessarin_goal_reached(ctx) {
    kessarin_goal_met(ctx.world, ctx.bindings.house)
}

// Before the shared deadline, a lifecycle that ended because its requester
// died, left, or was replaced — or because the house itself is gone —
// carries no relationship consequence; the successor's own lifecycle
// takes over from authoritative state.
fn kessarin_demand_passed_on(ctx) {
    if ctx.world.date >= household_deadline(ctx.world) { return false; }
    let house = first_reign_house(ctx.world);
    if house == 0 { return true; }
    household_requester(ctx.world, "kessarin-harrow", house) != ctx.bindings.requester
}

fn kessarin_was_refused(ctx) {
    type_of(ctx.answer) == "string" && ctx.answer == "refuse"
}

fn kessarin_promise_broken(ctx) {
    type_of(ctx.answer) == "string" && ctx.answer == "promise"
}

// The four authored relationship tiers, each under its own stable reason so
// no two tiers can ever stack on one lifecycle.
fn kessarin_achieved_effects(ctx) {
    [#{ kind: "opinion", from: "target", toward: "owner-head",
        amount: household_achieved_opinion(), days: household_achieved_days(),
        reason: "kessarin-order-achieved" }]
}

fn kessarin_refused_effects(ctx) {
    [#{ kind: "opinion", from: "target", toward: "owner-head",
        amount: household_refused_opinion(), days: household_refused_days(),
        reason: "kessarin-order-refused" }]
}

fn kessarin_ignored_effects(ctx) {
    [#{ kind: "opinion", from: "target", toward: "owner-head",
        amount: household_ignored_opinion(), days: household_ignored_days(),
        reason: "kessarin-order-ignored" }]
}

fn kessarin_broken_effects(ctx) {
    [#{ kind: "opinion", from: "target", toward: "owner-head",
        amount: household_broken_opinion(), days: household_broken_days(),
        reason: "kessarin-order-broken" }]
}

// --- Aleyn's Levies ----------------------------------------------------------
//
// Aleyn asks that the house field at least the authored total army
// manpower by the shared deadline. The goal is a live-state predicate
// over the armies the house actually owns — any legitimate route to the
// strength satisfies it, mustered levies or anything the future adds —
// and losses count against an open demand exactly as gains count for it.

fn aleyn_manpower_target() { 1000 }

fn aleyn_fielded_manpower(world, house) {
    let total = 0;
    for army in world.armies {
        if army.owner == house { total += army.manpower; }
    }
    total
}

fn aleyn_goal_met(world, house) {
    aleyn_fielded_manpower(world, house) >= aleyn_manpower_target()
}

define_situation(#{
    id: "aleyn-levies",
    source: "scenario",
    bindings: #{ house: "organisation", requester: "character" },
    owner_binding: "house",
    subject_binding: "requester",
    trigger_fn: "aleyn_levies_instances",
    projection_fn: "aleyn_levies_projection",
    priority: 109,
    log_activation: true,
    audience: ["house"],
    stages: ["open", "promised", "refused"],
    actions: [
        #{ id: "muster", assignment: "muster" },
    ],
    responses: [
        #{ id: "promise" },
        #{ id: "refuse" },
    ],
    outcomes: [
        #{ id: "achieved", when_fn: "aleyn_goal_reached", effects_fn: "aleyn_achieved_effects" },
        #{ id: "passed-on", when_fn: "aleyn_demand_passed_on" },
        #{ id: "refused", when_fn: "aleyn_was_refused", effects_fn: "aleyn_refused_effects" },
        #{ id: "broken", when_fn: "aleyn_promise_broken", effects_fn: "aleyn_broken_effects" },
        #{ id: "ignored", fallback: true, effects_fn: "aleyn_ignored_effects" },
    ],
});

fn aleyn_levies_instances(ctx) {
    let house = first_reign_house(ctx.world);
    if house == 0 { return []; }
    if ctx.world.date >= household_deadline(ctx.world) { return []; }
    if aleyn_goal_met(ctx.world, house) { return []; }
    let requester = household_requester(ctx.world, "aleyn-harrow", house);
    if requester == 0 { return []; }
    if type_of(ctx.activated) != "i64" && !household_demands_open(ctx.world, house) {
        return [];
    }
    [#{ house: house, requester: requester }]
}

fn aleyn_levies_projection(ctx) {
    let house = ctx.bindings.house;
    let requester = ctx.bindings.requester;
    let deadline = household_deadline(ctx.world);
    let head = 0;
    for organisation in ctx.world.organisations {
        if organisation.id == house {
            if type_of(organisation.head) == "i64" { head = organisation.head; }
        }
    }
    let stage = "open";
    if type_of(ctx.answer) == "string" {
        if ctx.answer == "promise" { stage = "promised"; }
        if ctx.answer == "refuse" { stage = "refused"; }
    }
    let total = 0;
    let links = [];
    let metrics = [
        #{ label_key: "situation.metric.manpower-target", value: aleyn_manpower_target() },
        #{ label_key: "situation.metric.days-left", value: deadline - ctx.world.date },
    ];
    for army in ctx.world.armies {
        if army.owner == house {
            total += army.manpower;
            metrics.push(#{
                label_key: "situation.metric.army-strength",
                value: `${army.name} — ${army.manpower}`,
            });
            links.push(#{ kind: "army", id: army.id });
        }
    }
    metrics.push(#{ label_key: "situation.metric.army-manpower", value: total });
    let shortfall = aleyn_manpower_target() - total;
    if shortfall < 0 { shortfall = 0; }
    metrics.push(#{ label_key: "situation.metric.manpower-shortfall", value: shortfall });
    metrics.push(#{
        label_key: "situation.metric.on-achieved",
        value: `+${household_achieved_opinion()} / ${household_achieved_days()}`,
    });
    metrics.push(#{
        label_key: "situation.metric.on-refused",
        value: `${household_refused_opinion()} / ${household_refused_days()}`,
    });
    metrics.push(#{
        label_key: "situation.metric.on-ignored",
        value: `${household_ignored_opinion()} / ${household_ignored_days()}`,
    });
    metrics.push(#{
        label_key: "situation.metric.on-broken",
        value: `${household_broken_opinion()} / ${household_broken_days()}`,
    });
    let actions = [];
    if head > 0 {
        actions.push(#{ id: "muster", leader: head });
    }
    // A refusal quiets the card; an open or promised demand raises the
    // shared attention strip once the window narrows.
    let warning = stage != "refused" && deadline - ctx.world.date <= 30;
    #{
        stage: stage,
        deadline: deadline,
        warning: warning,
        metrics: metrics,
        participants: [
            #{ kind: "character", id: requester },
            #{ kind: "organisation", id: house },
        ],
        links: links,
        actions: actions,
        resolution_values: #{
            requester: first_reign_character_name(ctx.world, requester),
            house: court_house_name(ctx.world, house),
            target: `${aleyn_manpower_target()}`,
        },
    }
}

fn aleyn_goal_reached(ctx) {
    aleyn_goal_met(ctx.world, ctx.bindings.house)
}

// Before the shared deadline, a lifecycle that ended because its requester
// died, left, or was replaced — or because the house itself is gone —
// carries no relationship consequence; the successor's own lifecycle
// takes over from authoritative state.
fn aleyn_demand_passed_on(ctx) {
    if ctx.world.date >= household_deadline(ctx.world) { return false; }
    let house = first_reign_house(ctx.world);
    if house == 0 { return true; }
    household_requester(ctx.world, "aleyn-harrow", house) != ctx.bindings.requester
}

fn aleyn_was_refused(ctx) {
    type_of(ctx.answer) == "string" && ctx.answer == "refuse"
}

fn aleyn_promise_broken(ctx) {
    type_of(ctx.answer) == "string" && ctx.answer == "promise"
}

// The four authored relationship tiers, each under its own stable reason so
// no two tiers can ever stack on one lifecycle.
fn aleyn_achieved_effects(ctx) {
    [#{ kind: "opinion", from: "target", toward: "owner-head",
        amount: household_achieved_opinion(), days: household_achieved_days(),
        reason: "aleyn-levies-achieved" }]
}

fn aleyn_refused_effects(ctx) {
    [#{ kind: "opinion", from: "target", toward: "owner-head",
        amount: household_refused_opinion(), days: household_refused_days(),
        reason: "aleyn-levies-refused" }]
}

fn aleyn_ignored_effects(ctx) {
    [#{ kind: "opinion", from: "target", toward: "owner-head",
        amount: household_ignored_opinion(), days: household_ignored_days(),
        reason: "aleyn-levies-ignored" }]
}

fn aleyn_broken_effects(ctx) {
    [#{ kind: "opinion", from: "target", toward: "owner-head",
        amount: household_broken_opinion(), days: household_broken_days(),
        reason: "aleyn-levies-broken" }]
}
