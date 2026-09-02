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
