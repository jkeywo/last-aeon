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

// --- Torvald's Standing ------------------------------------------------------
//
// Torvald asks that the house stand well with its liege: the liege's head
// must think no ill of the head of House Harrow — derived opinion at or
// above the authored target — by the shared deadline. The goal is a live
// derived-opinion predicate over authoritative relationship state: any
// legitimate relationship effect satisfies it, and third parties can move
// the same number in either direction while the demand is open. The
// liege's head is bound structurally: the demand is the bound man's
// regard, so a changed liege, a dead liege head, or a deposed one ends
// the lifecycle passed-on, and any further expectation is judged afresh
// from live state — a successor's warmer heart never quietly pays the
// achievement tier.

fn torvald_opinion_target() { 0 }

// The living head of the house's liege, or 0 when the house has no
// liege, the liege has no head, or that head is no longer alive.
fn torvald_liege_head(world, house) {
    let liege = 0;
    for organisation in world.organisations {
        if organisation.id == house {
            if type_of(organisation.liege) == "i64" { liege = organisation.liege; }
        }
    }
    if liege == 0 { return 0; }
    let head = 0;
    for organisation in world.organisations {
        if organisation.id == liege {
            if type_of(organisation.head) == "i64" { head = organisation.head; }
        }
    }
    if head == 0 { return 0; }
    for character in world.characters {
        if character.id == head && character.alive { return head; }
    }
    0
}

// One character's derived net opinion of another, read from the shared
// world view's authoritative opinion facts.
fn torvald_opinion(world, from, to) {
    for opinion in world.opinions {
        if opinion.from == from && opinion.to == to { return opinion.value; }
    }
    0
}

fn torvald_house_head(world, house) {
    for organisation in world.organisations {
        if organisation.id == house {
            if type_of(organisation.head) == "i64" { return organisation.head; }
        }
    }
    0
}

// Whether the bound liege head still is what the demand bound: alive and
// the head of the house's current liege. The achieved outcome and the
// passed-on rule share this one test, so a liege succession can never
// pay a relationship tier in the pass that should pass the demand on.
fn torvald_liege_head_stands(world, house, liege_head) {
    liege_head != 0 && torvald_liege_head(world, house) == liege_head
}

// The goal, judged from the bound liege head toward the house's current
// head. The demand follows the office of head-of-house — Torvald's
// concern is the house's standing, whoever leads it — while the judging
// liege head stays the exact bound person.
fn torvald_goal_met(world, liege_head, house) {
    let head = torvald_house_head(world, house);
    if head == 0 { return false; }
    torvald_opinion(world, liege_head, head) >= torvald_opinion_target()
}

define_situation(#{
    id: "torvald-standing",
    source: "scenario",
    bindings: #{
        house: "organisation",
        requester: "character",
        "liege-head": "character",
    },
    owner_binding: "house",
    subject_binding: "requester",
    trigger_fn: "torvald_standing_instances",
    projection_fn: "torvald_standing_projection",
    priority: 108,
    log_activation: true,
    audience: ["house"],
    stages: ["open", "promised", "refused"],
    actions: [
        #{ id: "court", assignment: "court" },
    ],
    responses: [
        #{ id: "promise" },
        #{ id: "refuse" },
    ],
    outcomes: [
        #{ id: "achieved", when_fn: "torvald_goal_reached", effects_fn: "torvald_achieved_effects" },
        #{ id: "passed-on", when_fn: "torvald_demand_passed_on" },
        #{ id: "refused", when_fn: "torvald_was_refused", effects_fn: "torvald_refused_effects" },
        #{ id: "broken", when_fn: "torvald_promise_broken", effects_fn: "torvald_broken_effects" },
        #{ id: "ignored", fallback: true, effects_fn: "torvald_ignored_effects" },
    ],
});

fn torvald_standing_instances(ctx) {
    let house = first_reign_house(ctx.world);
    if house == 0 { return []; }
    if ctx.world.date >= household_deadline(ctx.world) { return []; }
    let liege_head = torvald_liege_head(ctx.world, house);
    if liege_head == 0 { return []; }
    if torvald_goal_met(ctx.world, liege_head, house) { return []; }
    let requester = household_requester(ctx.world, "torvald-harrow", house);
    if requester == 0 { return []; }
    if type_of(ctx.activated) != "i64" && !household_demands_open(ctx.world, house) {
        return [];
    }
    [#{ house: house, requester: requester, "liege-head": liege_head }]
}

fn torvald_standing_projection(ctx) {
    let house = ctx.bindings.house;
    let requester = ctx.bindings.requester;
    let liege_head = ctx.bindings["liege-head"];
    let deadline = household_deadline(ctx.world);
    let head = torvald_house_head(ctx.world, house);
    let stage = "open";
    if type_of(ctx.answer) == "string" {
        if ctx.answer == "promise" { stage = "promised"; }
        if ctx.answer == "refuse" { stage = "refused"; }
    }
    let current = 0;
    if head != 0 { current = torvald_opinion(ctx.world, liege_head, head); }
    let links = [];
    let metrics = [
        #{ label_key: "situation.metric.opinion-target", value: torvald_opinion_target() },
        #{ label_key: "situation.metric.days-left", value: deadline - ctx.world.date },
        #{
            label_key: "situation.metric.liege-opinion",
            value: `${first_reign_character_name(ctx.world, liege_head)} — ${current}`,
        },
    ];
    let shortfall = torvald_opinion_target() - current;
    if shortfall < 0 { shortfall = 0; }
    metrics.push(#{ label_key: "situation.metric.opinion-shortfall", value: shortfall });
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
    if current < torvald_opinion_target() {
        links.push(#{ kind: "character", id: liege_head });
    }
    let liege = 0;
    for organisation in ctx.world.organisations {
        if organisation.id == house {
            if type_of(organisation.liege) == "i64" { liege = organisation.liege; }
        }
    }
    let actions = [];
    if head > 0 && liege > 0 {
        actions.push(#{
            id: "court", leader: head,
            target_kind: "organisation", target_a: liege,
        });
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
            liege: first_reign_character_name(ctx.world, liege_head),
            target: `${torvald_opinion_target()}`,
        },
    }
}

// Achievement is judged against the exact bound liege head, and only
// while that binding still stands: if the liege changed or the bound
// head died or was deposed, this lifecycle can only pass on — the
// successor's own regard is a new demand's business.
fn torvald_goal_reached(ctx) {
    let house = ctx.bindings.house;
    let liege_head = ctx.bindings["liege-head"];
    if !torvald_liege_head_stands(ctx.world, house, liege_head) { return false; }
    torvald_goal_met(ctx.world, liege_head, house)
}

// Before the shared deadline, a lifecycle that ended because its
// requester died, left, or was replaced — or because the house is gone,
// the liege changed, or the bound liege head died or was deposed —
// carries no relationship consequence; whatever expectation remains is
// judged afresh from authoritative state.
fn torvald_demand_passed_on(ctx) {
    if ctx.world.date >= household_deadline(ctx.world) { return false; }
    let house = first_reign_house(ctx.world);
    if house == 0 { return true; }
    if !torvald_liege_head_stands(ctx.world, house, ctx.bindings["liege-head"]) {
        return true;
    }
    household_requester(ctx.world, "torvald-harrow", house) != ctx.bindings.requester
}

fn torvald_was_refused(ctx) {
    type_of(ctx.answer) == "string" && ctx.answer == "refuse"
}

fn torvald_promise_broken(ctx) {
    type_of(ctx.answer) == "string" && ctx.answer == "promise"
}

// The four authored relationship tiers, each under its own stable reason so
// no two tiers can ever stack on one lifecycle.
fn torvald_achieved_effects(ctx) {
    [#{ kind: "opinion", from: "target", toward: "owner-head",
        amount: household_achieved_opinion(), days: household_achieved_days(),
        reason: "torvald-standing-achieved" }]
}

fn torvald_refused_effects(ctx) {
    [#{ kind: "opinion", from: "target", toward: "owner-head",
        amount: household_refused_opinion(), days: household_refused_days(),
        reason: "torvald-standing-refused" }]
}

fn torvald_ignored_effects(ctx) {
    [#{ kind: "opinion", from: "target", toward: "owner-head",
        amount: household_ignored_opinion(), days: household_ignored_days(),
        reason: "torvald-standing-ignored" }]
}

fn torvald_broken_effects(ctx) {
    [#{ kind: "opinion", from: "target", toward: "owner-head",
        amount: household_broken_opinion(), days: household_broken_days(),
        reason: "torvald-standing-broken" }]
}

// --- The Liege's Visit -------------------------------------------------------
//
// In a deterministic first-year window the liege's head comes to see the
// house he granted its ground — the visit Torvald warned would test the
// liege's regard in person. The Situation activates on any settled day in
// the window on which the house stands, the liege's living head exists,
// and a route from his location to a held province exists; it adapts to a
// changed liege or a succession by binding the live head, and it never
// activates for a liege who cannot come. Hosting is an ordinary hosted
// assignment at one of three authored hospitality tiers with distinct
// costs, difficulties, and durations, led by any eligible host the player
// chooses. Each tier reads the liege head's CURRENT opinion of the house's
// head into its authoritative forecast through its authored
// opinion_modifier — earlier diplomacy moves these odds — while spending
// (an easier tier) or a capable host (a better diplomat) mitigates a poor
// relationship. Every gameplay number below is authored data.

fn visit_opens() { 140 }
fn visit_closes() { 180 }
fn visit_warning_days() { 10 }
fn visit_slighted_opinion() { -20 }
fn visit_slighted_days() { 1440 }

fn visit_window_open(world) { world.start_date + visit_opens() }
fn visit_window_close(world) { world.start_date + visit_closes() }

// Whether a hosting assignment was accepted inside the window. Acceptance
// is what counts, exactly as the court's test counted it, and the closing
// day itself still counts — an order given on the last open day lands on
// the boundary, and the court's deadline rule reads that boundary one way.
// Every tier's authored duration is at least the window's length, so
// accepted work is still running whenever this is asked inside the window
// — the visit can never be hosted twice.
fn visit_hosted(world, house) {
    for work in world.assignments {
        if work.owner == house
                && (work.key == "host-visit-restrained"
                    || work.key == "host-visit-proper"
                    || work.key == "host-visit-lavish")
                && work.started >= visit_window_open(world)
                && work.started <= visit_window_close(world) {
            return true;
        }
    }
    false
}

// Whether the liege's head can physically reach any province the house
// holds: breadth-first search over the authored route graph, surface and
// space legs alike, from his current province. A head aboard a ship, a
// landless house, or a severed graph all mean the visit cannot happen —
// the same simulation facts that govern real travel, not a hardcoded
// assumption of connectivity.
fn visit_reachable(world, liege_head, house) {
    let from = 0;
    for character in world.characters {
        if character.id == liege_head {
            if type_of(character.location_kind) == "string"
                    && character.location_kind == "province" {
                from = character.location;
            }
        }
    }
    if from == 0 { return false; }
    let held = [];
    for province in world.provinces {
        if type_of(province.holder) == "i64" && province.holder == house {
            held.push(province.id);
        }
    }
    if held.len() == 0 { return false; }
    let seen = [from];
    let frontier = [from];
    while frontier.len() > 0 {
        let next = [];
        for at in frontier {
            for leg in world.routes {
                let to = 0;
                if leg.a == at { to = leg.b; }
                if leg.b == at { to = leg.a; }
                if to != 0 && !seen.contains(to) {
                    seen.push(to);
                    next.push(to);
                }
            }
        }
        frontier = next;
    }
    for target in held {
        if seen.contains(target) { return true; }
    }
    false
}

define_situation(#{
    id: "casimir-visit",
    source: "scenario",
    bindings: #{ house: "organisation", "liege-head": "character" },
    owner_binding: "house",
    subject_binding: "liege-head",
    trigger_fn: "casimir_visit_instances",
    projection_fn: "casimir_visit_projection",
    priority: 115,
    log_activation: true,
    audience: ["house"],
    stages: ["awaiting"],
    actions: [
        #{ id: "host-restrained", assignment: "host-visit-restrained" },
        #{ id: "host-proper", assignment: "host-visit-proper" },
        #{ id: "host-lavish", assignment: "host-visit-lavish" },
    ],
    outcomes: [
        #{ id: "hosted", when_fn: "casimir_visit_was_hosted" },
        #{ id: "slighted", when_fn: "casimir_visit_was_slighted",
           effects_fn: "casimir_visit_slighted_effects" },
        #{ id: "passed-on", when_fn: "casimir_visit_passed_on" },
        #{ id: "closed", fallback: true },
    ],
});

// One body serves fresh activation and the live lifecycle alike: the
// window must be open, the house must stand, the liege's living head must
// exist and be able to travel, and no hospitality may already stand
// accepted. A live lifecycle whose facts change simply stops being
// returned — the outcomes below say what its ending means — and a changed
// liege head is a different binding, so the successor's visit is a new
// lifecycle of its own.
fn casimir_visit_instances(ctx) {
    let world = ctx.world;
    let house = first_reign_house(world);
    if house == 0 { return []; }
    if world.date < visit_window_open(world) { return []; }
    if world.date >= visit_window_close(world) { return []; }
    if visit_hosted(world, house) { return []; }
    let liege_head = torvald_liege_head(world, house);
    if liege_head == 0 { return []; }
    if !visit_reachable(world, liege_head, house) { return []; }
    [#{ house: house, "liege-head": liege_head }]
}

fn casimir_visit_projection(ctx) {
    let world = ctx.world;
    let house = ctx.bindings.house;
    let liege_head = ctx.bindings["liege-head"];
    let close = visit_window_close(world);
    let head = torvald_house_head(world, house);
    let current = 0;
    if head != 0 { current = torvald_opinion(world, liege_head, head); }
    #{
        stage: "awaiting",
        deadline: close,
        warning: close - world.date <= visit_warning_days(),
        metrics: [
            #{ label_key: "situation.metric.days-left", value: close - world.date },
            #{
                label_key: "situation.metric.liege-opinion",
                value: `${first_reign_character_name(world, liege_head)} — ${current}`,
            },
        ],
        participants: [
            #{ kind: "character", id: liege_head },
            #{ kind: "organisation", id: house },
        ],
        // The three hospitality tiers deliberately pin no leader: hosting
        // is a choice of host, and the ordinary composition popup offers
        // the whole eligible household with a forecast apiece.
        actions: [
            #{ id: "host-restrained" },
            #{ id: "host-proper" },
            #{ id: "host-lavish" },
        ],
        resolution_values: #{
            house: court_house_name(world, house),
            liege: first_reign_character_name(world, liege_head),
        },
    }
}

// An ended lifecycle judges its outcomes with its OLD bindings against
// the LIVE world, so every consequential outcome re-asserts that the
// bound liege head still stands and can still come: the same evaluation
// that ends a lifecycle for a dead, deposed, or replaced liege must pass
// it on, never pay hospitality's dues to a man who no longer exists.
fn casimir_visit_was_hosted(ctx) {
    let house = ctx.bindings.house;
    if !torvald_liege_head_stands(ctx.world, house, ctx.bindings["liege-head"]) {
        return false;
    }
    visit_hosted(ctx.world, house)
}

fn casimir_visit_was_slighted(ctx) {
    let house = ctx.bindings.house;
    let liege_head = ctx.bindings["liege-head"];
    if !torvald_liege_head_stands(ctx.world, house, liege_head) { return false; }
    if !visit_reachable(ctx.world, liege_head, house) { return false; }
    ctx.world.date >= visit_window_close(ctx.world)
}

fn casimir_visit_passed_on(ctx) {
    let house = first_reign_house(ctx.world);
    if house == 0 { return true; }
    if !torvald_liege_head_stands(ctx.world, house, ctx.bindings["liege-head"]) {
        return true;
    }
    !visit_reachable(ctx.world, ctx.bindings["liege-head"], house)
}

// The stated cost of an unanswered window: the bound liege head — the
// subject binding stands behind the `target` role — thinks less of the
// house's head, once, for the authored term.
fn casimir_visit_slighted_effects(ctx) {
    [#{ kind: "opinion", from: "target", toward: "owner-head",
        amount: visit_slighted_opinion(), days: visit_slighted_days(),
        reason: "casimir-visit-slighted" }]
}

// --- The three hospitality tiers --------------------------------------------
//
// Ordinary hosted assignments, started from the visit card (or, like any
// assignment, on their own — outside the window they are merely an
// expensive way to fete the liege's court). Authored trade-offs:
// restrained is cheap but hard and its outcomes are mild; lavish is
// expensive, slow, easiest — spending mitigates a poor relationship —
// and swings the liege's regard hardest in both directions. All three
// read the liege head's live opinion of the owner's head into their
// effectiveness at fifty hundredths per point; the clamp is the tier's
// own, and lavish cushions the downside deepest. Durations are at least
// the window's forty days, which is what makes the visit one-shot: an
// accepted tier is still running whenever the window could re-ask.

define_assignment(#{
    id: "host-visit-restrained",
    category: "consequential",
    duration_days: 40,
    wealth_cost: 10,
    skill: "diplomacy",
    difficulty: 12,
    risks: ["scandal"],
    ai_available: false,
    opinion_modifier: #{
        from: "liege-head", toward: "owner-head",
        per_point: 50, min: -9, max: 9,
    },
    results: #{
        critical_success: #{ weight: 100, popup: true, log: true,
                             effect_fn: "visit_restrained_crit" },
        success: #{ weight: 450, log: true, effect_fn: "visit_restrained_success" },
        failure: #{ weight: 350, log: true, effect_fn: "visit_restrained_failure" },
        disaster: #{ weight: 100, popup: true, log: true,
                     effect_fn: "visit_restrained_disaster" },
    },
});

fn visit_restrained_crit(ctx) {
    [#{ kind: "opinion", from: "liege-head", toward: "owner-head",
        amount: 10, days: 1080, reason: "visit-restrained-crit" }]
}
fn visit_restrained_success(ctx) {
    [#{ kind: "opinion", from: "liege-head", toward: "owner-head",
        amount: 5, days: 1080, reason: "visit-restrained-success" }]
}
fn visit_restrained_failure(ctx) {
    [#{ kind: "opinion", from: "liege-head", toward: "owner-head",
        amount: -5, days: 1080, reason: "visit-restrained-failure" }]
}
fn visit_restrained_disaster(ctx) {
    [#{ kind: "opinion", from: "liege-head", toward: "owner-head",
        amount: -10, days: 1440, reason: "visit-restrained-disaster" }]
}

define_assignment(#{
    id: "host-visit-proper",
    category: "consequential",
    duration_days: 45,
    wealth_cost: 30,
    skill: "diplomacy",
    difficulty: 10,
    risks: ["scandal"],
    ai_available: false,
    opinion_modifier: #{
        from: "liege-head", toward: "owner-head",
        per_point: 50, min: -8, max: 8,
    },
    results: #{
        critical_success: #{ weight: 100, popup: true, log: true,
                             effect_fn: "visit_proper_crit" },
        success: #{ weight: 450, log: true, effect_fn: "visit_proper_success" },
        failure: #{ weight: 350, log: true, effect_fn: "visit_proper_failure" },
        disaster: #{ weight: 100, popup: true, log: true,
                     effect_fn: "visit_proper_disaster" },
    },
});

fn visit_proper_crit(ctx) {
    [#{ kind: "opinion", from: "liege-head", toward: "owner-head",
        amount: 15, days: 1440, reason: "visit-proper-crit" }]
}
fn visit_proper_success(ctx) {
    [#{ kind: "opinion", from: "liege-head", toward: "owner-head",
        amount: 8, days: 1440, reason: "visit-proper-success" }]
}
fn visit_proper_failure(ctx) {
    [#{ kind: "opinion", from: "liege-head", toward: "owner-head",
        amount: -8, days: 1440, reason: "visit-proper-failure" }]
}
fn visit_proper_disaster(ctx) {
    [#{ kind: "opinion", from: "liege-head", toward: "owner-head",
        amount: -15, days: 1440, reason: "visit-proper-disaster" }]
}

define_assignment(#{
    id: "host-visit-lavish",
    category: "consequential",
    duration_days: 50,
    wealth_cost: 60,
    skill: "diplomacy",
    difficulty: 8,
    risks: ["scandal"],
    ai_available: false,
    opinion_modifier: #{
        from: "liege-head", toward: "owner-head",
        per_point: 50, min: -6, max: 9,
    },
    results: #{
        critical_success: #{ weight: 100, popup: true, log: true,
                             effect_fn: "visit_lavish_crit" },
        success: #{ weight: 450, log: true, effect_fn: "visit_lavish_success" },
        failure: #{ weight: 350, log: true, effect_fn: "visit_lavish_failure" },
        disaster: #{ weight: 100, popup: true, log: true,
                     effect_fn: "visit_lavish_disaster" },
    },
});

fn visit_lavish_crit(ctx) {
    [#{ kind: "opinion", from: "liege-head", toward: "owner-head",
        amount: 20, days: 1800, reason: "visit-lavish-crit" }]
}
fn visit_lavish_success(ctx) {
    [#{ kind: "opinion", from: "liege-head", toward: "owner-head",
        amount: 12, days: 1440, reason: "visit-lavish-success" }]
}
fn visit_lavish_failure(ctx) {
    [#{ kind: "opinion", from: "liege-head", toward: "owner-head",
        amount: -10, days: 1440, reason: "visit-lavish-failure" }]
}
fn visit_lavish_disaster(ctx) {
    [#{ kind: "opinion", from: "liege-head", toward: "owner-head",
        amount: -20, days: 1800, reason: "visit-lavish-disaster" }]
}

// --- The shadows ------------------------------------------------------------
//
// The quiet arm of first-year politics: a hostile, capable house may set
// its mind to undermining a neighbour by deniable means. The ambition and
// the campaign below are generic — any vassal with the coin, a shared
// border, and genuine ill will (or an open grievance owed to it) may
// adopt them inside the authored window — but in the Ashkarr Succession
// their natural bearer is House Vantar: informants, disruption, and
// pressure no seal was ever set to, aimed across the Ulmgorn border at
// whichever Harrow ground sits weakest. Everything is data: the window,
// the hostility floor, the capability gate, and the covertness. The
// engine contributes only the shared mechanisms — the border selector,
// the opinion, grievance, and war predicates, and the owner-only audience
// that keeps provenance from every ordinary player surface while
// spectators and replay retain it all.
//
// Reconciliation is the same data read the other way, and its whole
// contract is authored here:
//
//   * Hostile planning may BEGIN while the neighbour's head regards the
//     house's head at or below the hostility floor, or while the
//     neighbour owes the house an open grievance (the goal's selector and
//     the plan's two method gates).
//   * Regard lifted ABOVE the floor suppresses new escalation: the method
//     gate is rechecked before every new step, so no further step begins
//     and an uncommitted campaign is let go.
//   * Regard at or above the reconciliation line, with no grievance owed
//     and no war between the houses, abandons what is not yet committed:
//     the plan's `abandon_when` and the ambition's `set_aside_when` read
//     the same three predicates, and the ambition is set aside with no
//     cooldown — the grounds may return, and a successor of a different
//     temper reads the relationship afresh and may adopt it anew.
//   * Nothing already underway is erased by a change of heart. A step
//     already accepted runs to its ordinary resolution (the plan's
//     committed guard), and a formal war ends only by peace or collapse.
//
// A grievance is cleared by nothing here: no authored effect settles one,
// so a house that has been wronged keeps its grounds until the ledger
// says otherwise. The two numbers are shared with the open Cold Border
// card below, so what the player reads is exactly what the AI weighs.

fn shadow_hostility_floor() { -10 }
fn shadow_reconciliation_line() { 20 }

define_goal(#{
    id: "undermine-a-neighbour",
    priority: 95,
    favours: ["subvert"],
    favour_bonus: 60,
    target: "organisation",
    target_selector: #{
        kind: "hostile-border-neighbour",
        max_head_opinion: shadow_hostility_floor(),
        with_grievance: true,
    },
    covert: true,
    trigger: #{
        is_vassal: true, min_wealth: 40,
        min_campaign_day: 180, max_campaign_day: 260,
    },
    set_aside_when: #{
        min_target_head_opinion: shadow_reconciliation_line(),
        target_owes_no_grievance: true,
        at_war_with_target: false,
    },
    max_days: 120,
    cooldown_days: 720,
});

define_plan(#{
    id: "deniable-pressure",
    goal: "subvert",
    target: "organisation",
    covert: true,
    score_bonus: 40,
    cooldown_days: 360,
    max_days: 120,
    max_step_retries: 1,
    abandon_when: #{
        min_target_head_opinion: shadow_reconciliation_line(),
        target_owes_no_grievance: true,
        at_war_with_target: false,
    },
    methods: [
        #{ id: "from-ill-will",
           requires: #{ min_wealth: 40, max_target_head_opinion: shadow_hostility_floor() },
           steps: [
               #{ id: "stir", start: "foment-unrest", target: "target-border-province" },
           ] },
        #{ id: "from-grievance",
           requires: #{ min_wealth: 40, target_owes_grievance: true },
           steps: [
               #{ id: "stir", start: "foment-unrest", target: "target-border-province" },
           ] },
    ],
});

// --- Unquiet Holdings -------------------------------------------------------
//
// What the covert work looks like from the other side: the moment a
// covert province-aimed operation runs against ground a house holds, its
// holder sees a Situation naming the province, its live Order, the
// resistance that Order is offering, and the time remaining — and
// nothing else. The culprit organisation IS structurally bound (the
// `actor` binding), which is what gives investigation, spectators, and
// replay their authoritative provenance; it is simply never in the
// audience and never emitted by the projection. The trigger may read the
// culprit because triggers are simulation-side; the card the player sees
// is the projection, and the projection stays silent.

fn unquiet_struck_line() { 700 }

// --- Tracing the hand -------------------------------------------------------
//
// The ordinary answer to an unnamed hand. Investigation is an assignment
// like any other: authored cost, duration, difficulty, and skill; no AI
// use; aimed at the holder's own troubled ground rather than at a suspect,
// because there is no suspect to aim at. It pins no leader, so the player
// compares every eligible investigator against one authoritative forecast
// before ordering it.
//
// It is an answer to the alarm, not a free-standing order: the second
// requirement holds only while somebody else's covert work is actually
// running against that holding AND this house has not yet proved whose
// hand it is — exactly when the card below offers its `investigate`
// action, and no longer. On quiet ground there is nothing to trace, and
// once the hand is proved there is nothing left to prove, so in both
// cases the enquiry is neither offered on the province nor accepted from
// the household — the same gate the button, the forecast, and the
// command all pass through — and every enquiry that does start attaches
// to the live card whose action it is.
//
// It has exactly two answers. A proved trail exposes the organisation the
// Situation already bound as `actor` — the true hand, structurally, from
// the operation itself — and nothing else can ever be proved, because the
// effect reads a binding rather than choosing a candidate. A cold or
// botched trail carries no effect at all. There is no false suspect to
// author here, and no field in which one could be authored.
define_assignment(#{
    id: "trace-the-hand",
    category: "consequential",
    duration_days: 25,
    skill: "intrigue",
    difficulty: 13,
    target: "province",
    ai_available: false,
    wealth_cost: 25,
    requires: #{ target_holder: "own", target_under_covert_work: true },
    results: #{
        critical_success: #{
            weight: 120, popup: true, log: true, effect_fn: "the_hand_traced",
        },
        success: #{ weight: 330, popup: true, log: true, effect_fn: "the_hand_traced" },
        failure: #{ weight: 400, log: true },
        disaster: #{ weight: 150, log: true },
    },
});

// The revelation: the culprit is the organisation this exact lifecycle
// bound, named by binding rather than by guess.
fn the_hand_traced(ctx) {
    [#{ kind: "expose", binding: "actor" }]
}

// Whether this holder has proved this culprit. Discovery is per knower:
// another house's investigation teaches this card nothing.
fn unquiet_hand_known(world, house, culprit) {
    for record in world.exposures {
        if record.culprit == culprit && record.knower == house { return true; }
    }
    false
}

// The live covert operation behind one bound instance, or an empty map
// once it has ended. The projection and the trigger share this read.
fn unquiet_live_work(world, province, culprit) {
    for work in world.assignments {
        if work.covert && work.target.kind == "province"
                && work.target.a == province && work.owner == culprit {
            return work;
        }
    }
    #{}
}

fn unquiet_holdings_instances(ctx) {
    let found = [];
    for work in ctx.world.assignments {
        if !work.covert { continue; }
        if work.target.kind != "province" { continue; }
        let province = work.target.a;
        let holder = 0;
        for p in ctx.world.provinces {
            if p.id == province {
                if type_of(p.holder) == "i64" { holder = p.holder; }
            }
        }
        if holder == 0 { continue; }
        // A house's own covert work on its own ground raises no alarm.
        if holder == work.owner { continue; }
        found.push(#{ house: holder, province: province, actor: work.owner });
    }
    found
}

fn unquiet_holdings_projection(ctx) {
    let world = ctx.world;
    let house = ctx.bindings.house;
    let province = ctx.bindings.province;
    let name = "";
    let order = 0;
    for p in world.provinces {
        if p.id == province { name = p.name; order = p.order; }
    }
    let head = 0;
    for organisation in world.organisations {
        if organisation.id == house {
            if type_of(organisation.head) == "i64" { head = organisation.head; }
        }
    }
    // The live operation, while it runs: its completion day is the card's
    // deadline and its exact live Order shift is the resistance the roll
    // will use. Once the work has ended (this same projection freezes the
    // resolution) both rows simply drop away.
    let work = unquiet_live_work(world, province, ctx.bindings.actor);
    let live = work.len() > 0;
    // What this holder has proved. Before it is proved this reads false
    // for everyone but the culprit itself, and the whole disclosure block
    // below is simply absent from the card.
    let known = unquiet_hand_known(world, house, ctx.bindings.actor);
    let culprit_name = "";
    let culprit_head = 0;
    if known {
        for organisation in world.organisations {
            if organisation.id == ctx.bindings.actor {
                culprit_name = organisation.name;
                if type_of(organisation.head) == "i64" { culprit_head = organisation.head; }
            }
        }
    }
    let metrics = [];
    if live {
        metrics.push(#{
            label_key: "situation.metric.days-left",
            value: work.completes - world.date,
        });
    }
    metrics.push(#{
        label_key: "situation.metric.province-order",
        value: `${name} — ${order}`,
    });
    if live {
        metrics.push(#{
            label_key: "situation.metric.order-resistance",
            value: work.order_shift,
        });
    }
    if known {
        metrics.push(#{
            label_key: "situation.metric.proved-hand",
            value: culprit_name,
        });
    }
    let actions = [];
    if head > 0 {
        actions.push(#{ id: "hold-court", leader: head });
        actions.push(#{ id: "tour-holdings", leader: head });
    }
    // Investigation pins NO leader: the player compares every eligible
    // investigator on one authoritative forecast and picks. Once the hand
    // is proved there is nothing left to trace, so the action goes away.
    if !known {
        actions.push(#{ id: "investigate", target_kind: "province", target_a: province });
    }
    // The province, and — until this holder has PROVED it — only the
    // province: no culprit, organisation, leader, or source plan may ever
    // be emitted here otherwise. Proof is the one thing that opens it, and
    // what it opens is the operation's own true provenance: the
    // organisation behind it, the head who ordered it, and the very work
    // in flight.
    let participants = [#{ kind: "province", id: province }];
    let links = [];
    if known {
        participants.push(#{ kind: "organisation", id: ctx.bindings.actor });
        if culprit_head > 0 {
            links.push(#{ kind: "character", id: culprit_head });
        }
        if live {
            links.push(#{ kind: "assignment", id: work.id });
        }
    }
    let stage = "unrest";
    if known { stage = "traced"; }
    let out = #{
        stage: stage,
        warning: live,
        metrics: metrics,
        participants: participants,
        links: links,
        actions: actions,
        resolution_values: #{ province: name, house: culprit_name },
    };
    if live { out.deadline = work.completes; }
    out
}

// An ended lifecycle judges its outcomes with its OLD bindings against
// the LIVE world: a province no longer held as bound passes on with no
// further reading, however the operation went.
fn unquiet_passed_on(ctx) {
    let holder = 0;
    for p in ctx.world.provinces {
        if p.id == ctx.bindings.province {
            if type_of(p.holder) == "i64" { holder = p.holder; }
        }
    }
    holder != ctx.bindings.house
}

// Struck or weathered is a pure live-state reading, not a peek at the
// hidden roll: the holdings were struck when their Order now stands
// below the authored line, and weathered when it held above it — which
// is exactly what "high Order resists sabotage" means at the outcome
// level. A province kept strong absorbs even a landed blow.
fn unquiet_struck(ctx) {
    let order = 0;
    for p in ctx.world.provinces {
        if p.id == ctx.bindings.province { order = p.order; }
    }
    order < unquiet_struck_line()
}

// Whether this holder proved the hand before the matter closed. It splits
// each live-Order outcome in two so the frozen sentence tells the truth
// about what is and is not known — the reading of the ground is unchanged
// either way, because proof is knowledge, never protection.
fn unquiet_traced(ctx) {
    unquiet_hand_known(ctx.world, ctx.bindings.house, ctx.bindings.actor)
}
fn unquiet_struck_traced(ctx) {
    unquiet_struck(ctx) && unquiet_traced(ctx)
}

define_situation(#{
    id: "unquiet-holdings",
    source: "scenario",
    bindings: #{ house: "organisation", province: "province", actor: "organisation" },
    trigger_fn: "unquiet_holdings_instances",
    projection_fn: "unquiet_holdings_projection",
    priority: 112,
    log_activation: true,
    audience: ["house"],
    stages: ["unrest", "traced"],
    actions: [
        #{ id: "hold-court", assignment: "hold-court" },
        #{ id: "tour-holdings", assignment: "tour-holdings" },
        #{ id: "investigate", assignment: "trace-the-hand" },
    ],
    outcomes: [
        #{ id: "passed-on", when_fn: "unquiet_passed_on" },
        #{ id: "struck-traced", when_fn: "unquiet_struck_traced" },
        #{ id: "struck", when_fn: "unquiet_struck" },
        #{ id: "weathered-traced", when_fn: "unquiet_traced" },
        #{ id: "weathered", fallback: true },
    ],
});

// --- A Cold Border ----------------------------------------------------------
//
// The open half of the shadow arc: the reasoning a player may inspect
// without being told anything the simulation keeps covert. A house that
// shares a surface border with the protagonist and whose head regards the
// protagonist's head at or below the hostility floor — or that is owed an
// open grievance by the protagonist — is a cold neighbour, and this card
// says so, in exactly the terms the AI weighs: the live head-to-head
// regard, the floor at which hostile planning may begin, the line at
// which uncommitted designs are let go, the grievances that keep grounds
// open whatever the regard, and any formal war already between the
// houses. It reads the shared world view alone — opinions, obligations,
// wars, provinces, routes — never a plan, a goal, an assignment, or an
// exposure record, so it names a neighbour as cold and never as a
// culprit: it explains why a house MIGHT scheme, never that it IS.
//
// Its actions are the ordinary levers. Courting the house is the general
// answer; sending gifts across the border is a second, distinct lever, so
// that reaching the reconciliation line from the reign's opening standing
// is a strategy of two ordinary successes rather than one lucky triumph.
// Both are ordinary assignments with ordinary forecasts, and both are
// offered against any cold neighbour, not any particular house.

define_assignment(#{
    id: "send-gifts",
    category: "consequential",
    duration_days: 30,
    wealth_cost: 20,
    skill: "diplomacy",
    difficulty: 8,
    target: "organisation",
    ai_available: false,
    risks: ["scandal"],
    results: #{
        critical_success: #{
            weight: 100, popup: true, log: true, effect_fn: "gifts_welcomed",
        },
        success: #{ weight: 500, log: true, effect_fn: "gifts_received" },
        failure: #{ weight: 300, log: true },
        disaster: #{ weight: 100, popup: true, log: true, effect_fn: "gifts_spurned" },
    },
});

fn gifts_welcomed(ctx) {
    [#{ kind: "opinion", from: "target-head", toward: "owner-head",
        amount: 25, days: 1800, reason: "gifted" }]
}
fn gifts_received(ctx) {
    [#{ kind: "opinion", from: "target-head", toward: "owner-head",
        amount: 15, days: 1080, reason: "gifted" }]
}
fn gifts_spurned(ctx) {
    [#{ kind: "opinion", from: "target-head", toward: "owner-head",
        amount: -10, days: 1440, reason: "gift-spurned" }]
}

fn cold_border_head(world, org) {
    for organisation in world.organisations {
        if organisation.id == org {
            if type_of(organisation.head) == "i64" { return organisation.head; }
        }
    }
    0
}

fn cold_border_holder(world, province) {
    for p in world.provinces {
        if p.id == province {
            if type_of(p.holder) == "i64" { return p.holder; }
        }
    }
    0
}

// Whether `house` answers, directly or through its lieges, to `liege`.
fn cold_border_answers_to(world, house, liege) {
    let current = house;
    let steps = 0;
    while steps < 16 {
        let next = 0;
        for organisation in world.organisations {
            if organisation.id == current {
                if type_of(organisation.liege) == "i64" { next = organisation.liege; }
            }
        }
        if next == 0 { return false; }
        if next == liege { return true; }
        current = next;
        steps += 1;
    }
    false
}

// Every standing organisation outside the house's chain of command that
// holds a province sharing a surface route with a province the house
// holds — the same neighbourhood the hostile-border selector walks —
// lowest stable ID first.
fn cold_border_neighbours(world, house) {
    let found = [];
    for route in world.routes {
        if route.kind != "surface" { continue; }
        let a = cold_border_holder(world, route.a);
        let b = cold_border_holder(world, route.b);
        if a == 0 || b == 0 || a == b { continue; }
        let other = 0;
        if a == house { other = b; }
        if b == house { other = a; }
        if other == 0 { continue; }
        if found.contains(other) { continue; }
        if cold_border_answers_to(world, house, other) { continue; }
        if cold_border_answers_to(world, other, house) { continue; }
        found.push(other);
    }
    found.sort();
    found
}

// The live derived opinion of one character about another, from the
// shared world view.
fn cold_border_regard(world, from, to) {
    for opinion in world.opinions {
        if opinion.from == from && opinion.to == to { return opinion.value; }
    }
    0
}

// Open grievances `debtor` owes `creditor`: the grounds a wronged house
// keeps whatever the regard between the heads.
fn cold_border_grievances(world, debtor, creditor) {
    let count = 0;
    for entry in world.obligations {
        if entry.kind == "grievance" && entry.status == "open"
                && entry.debtor == debtor && entry.creditor == creditor {
            count += 1;
        }
    }
    count
}

// The active formal war placing the two houses on opposing sides, or an
// empty map when there is none.
fn cold_border_war(world, house, neighbour) {
    for war in world.wars {
        if !war.active { continue; }
        let first = war.sides[0].members;
        let second = war.sides[1].members;
        if (first.contains(house) && second.contains(neighbour))
                || (first.contains(neighbour) && second.contains(house)) {
            return war;
        }
    }
    #{}
}

// A neighbour is cold when its head regards the house's head at or below
// the floor, or when the house owes it an open grievance — the two
// authored grounds on which hostile planning may begin.
fn cold_border_is_cold(world, house, neighbour) {
    let own = cold_border_head(world, house);
    let theirs = cold_border_head(world, neighbour);
    if own == 0 || theirs == 0 { return false; }
    if cold_border_regard(world, theirs, own) <= shadow_hostility_floor() { return true; }
    cold_border_grievances(world, house, neighbour) > 0
}

fn cold_border_instances(ctx) {
    let house = first_reign_house(ctx.world);
    if house == 0 { return []; }
    let found = [];
    for neighbour in cold_border_neighbours(ctx.world, house) {
        if cold_border_is_cold(ctx.world, house, neighbour) {
            found.push(#{ house: house, neighbour: neighbour });
        }
    }
    found
}

fn cold_border_projection(ctx) {
    let world = ctx.world;
    let house = ctx.bindings.house;
    let neighbour = ctx.bindings.neighbour;
    let own = cold_border_head(world, house);
    let theirs = cold_border_head(world, neighbour);
    let regard = 0;
    if own != 0 && theirs != 0 { regard = cold_border_regard(world, theirs, own); }
    let grievances = cold_border_grievances(world, house, neighbour);
    let war = cold_border_war(world, house, neighbour);
    let stage = "cold";
    if grievances > 0 { stage = "aggrieved"; }
    let metrics = [
        #{ label_key: "situation.metric.neighbour-regard", value: regard },
        #{ label_key: "situation.metric.hostility-floor", value: shadow_hostility_floor() },
        #{ label_key: "situation.metric.reconciliation-line", value: shadow_reconciliation_line() },
        #{ label_key: "situation.metric.open-grievances", value: grievances },
    ];
    let links = [];
    if theirs != 0 { links.push(#{ kind: "character", id: theirs }); }
    if war.len() > 0 {
        metrics.push(#{ label_key: "situation.metric.war-days", value: world.date - war.declared });
        links.push(#{ kind: "war", id: war.id });
    }
    // The ordinary levers, aimed at the neighbour. Courting pins the head,
    // because led by the head it lifts both the personal and the house
    // regard; gifts pin nobody, so the player compares envoys.
    let actions = [];
    if own != 0 {
        actions.push(#{
            id: "court", leader: own,
            target_kind: "organisation", target_a: neighbour,
        });
        actions.push(#{ id: "send-gifts", target_kind: "organisation", target_a: neighbour });
    }
    #{
        stage: stage,
        warning: false,
        metrics: metrics,
        participants: [
            #{ kind: "organisation", id: house },
            #{ kind: "organisation", id: neighbour },
        ],
        links: links,
        actions: actions,
        resolution_values: #{
            house: court_house_name(world, house),
            neighbour: court_house_name(world, neighbour),
        },
    }
}

// The border is no longer the matter it was: a head gone on either side,
// or the houses no longer neighbours outside each other's chain of
// command. Judged with the old bindings against the live world.
fn cold_border_passed_on(ctx) {
    let house = first_reign_house(ctx.world);
    if house != ctx.bindings.house { return true; }
    if cold_border_head(ctx.world, house) == 0 { return true; }
    if cold_border_head(ctx.world, ctx.bindings.neighbour) == 0 { return true; }
    !cold_border_neighbours(ctx.world, house).contains(ctx.bindings.neighbour)
}

// Reconciled is the full line — the same three facts the ambition's
// `set_aside_when` and the campaign's `abandon_when` judge: regard at or
// above the line, no grievance owed, and no formal war between the
// houses. A war already declared keeps every design its grounds until
// peace is made, so a card closing under one is merely eased.
fn cold_border_reconciled(ctx) {
    let own = cold_border_head(ctx.world, ctx.bindings.house);
    let theirs = cold_border_head(ctx.world, ctx.bindings.neighbour);
    if own == 0 || theirs == 0 { return false; }
    cold_border_regard(ctx.world, theirs, own) >= shadow_reconciliation_line()
        && cold_border_grievances(ctx.world, ctx.bindings.house, ctx.bindings.neighbour) == 0
        && cold_border_war(ctx.world, ctx.bindings.house, ctx.bindings.neighbour).len() == 0
}

define_situation(#{
    id: "cold-border",
    source: "scenario",
    bindings: #{ house: "organisation", neighbour: "organisation" },
    trigger_fn: "cold_border_instances",
    projection_fn: "cold_border_projection",
    priority: 90,
    log_activation: true,
    audience: ["house"],
    stages: ["cold", "aggrieved"],
    actions: [
        #{ id: "court", assignment: "court" },
        #{ id: "send-gifts", assignment: "send-gifts" },
    ],
    outcomes: [
        #{ id: "passed-on", when_fn: "cold_border_passed_on" },
        #{ id: "reconciled", when_fn: "cold_border_reconciled" },
        #{ id: "eased", fallback: true },
    ],
});
