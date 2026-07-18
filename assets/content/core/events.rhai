// Contextual events.
//
// Events are drawn from the state the campaign is actually in, not
// sprinkled at random: each declares the family of situation it belongs
// to and the conditions it needs. Weighty events interrupt with a choice;
// minor ones colour the log without stopping play.
//
// This deck deliberately adds no provinces, houses, or bodies. Its
// variety comes from the state it reads and the choices it offers.

// ---------------------------------------------------------------------------
// Provincial pressure
// ---------------------------------------------------------------------------

define_event(#{
    id: "grain-riot",
    title: "Grain Riot",
    family: "province",
    weight: 120,
    cooldown_days: 540,
    weighty: true,
    requires: #{ player_only: true, max_order: 500 },
    text: "The granaries of {subject} are opened by a crowd that has stopped asking. Your reeve wants soldiers; the local elders want grain.",
    log_text: "A grain riot broke out in {subject}.",
    choices: [
        #{ id: "send-soldiers", label: "Put it down", effect_fn: "riot_suppressed" },
        #{ id: "open-stores", label: "Open the stores", effect_fn: "riot_bought_off" },
    ],
});
fn riot_suppressed(ctx) {
    [#{ kind: "order", scope: "target-province", amount: 120 },
     #{ kind: "log", message: "The riot was broken, and remembered." }]
}
fn riot_bought_off(ctx) {
    [#{ kind: "order", scope: "target-province", amount: 60 },
     #{ kind: "log", message: "The stores were opened, and the crowd went home fed." }]
}

define_event(#{
    id: "occupiers-quarter",
    title: "Soldiers in the Streets",
    family: "province",
    weight: 100,
    cooldown_days: 360,
    weighty: false,
    requires: #{ occupied: true },
    text: "Foreign soldiers are quartered on the households of {subject}.",
    log_text: "Foreign soldiers are quartered on the households of {subject}.",
    effect_fn: "occupation_bites",
});
fn occupation_bites(ctx) {
    [#{ kind: "order", scope: "target-province", amount: -40 }]
}

define_event(#{
    id: "good-harvest",
    title: "A Good Harvest",
    family: "province",
    weight: 80,
    cooldown_days: 720,
    weighty: false,
    requires: #{ min_order: 700 },
    text: "{subject} brings in a harvest worth talking about.",
    log_text: "{subject} brought in a fine harvest.",
    effect_fn: "harvest_kept",
});
fn harvest_kept(ctx) {
    [#{ kind: "order", scope: "target-province", amount: 40 }]
}

// ---------------------------------------------------------------------------
// Political obligation
// ---------------------------------------------------------------------------

define_event(#{
    id: "debt-called-in",
    title: "An Old Debt Remembered",
    family: "political",
    weight: 110,
    cooldown_days: 900,
    weighty: true,
    requires: #{ player_only: true, has_open_obligation: true },
    text: "An envoy arrives to remind your house, courteously, of an obligation it would rather have forgotten. The reminder is public enough that the answer will be too.",
    log_text: "An envoy called on the house about an old obligation.",
    choices: [
        #{ id: "honour-it", label: "Honour it", effect_fn: "debt_honoured" },
        #{ id: "put-them-off", label: "Put them off", effect_fn: "debt_deferred" },
    ],
});
fn debt_honoured(ctx) {
    [#{ kind: "log", message: "The house honoured its obligation, and was seen to." }]
}
fn debt_deferred(ctx) {
    [#{ kind: "log", message: "The envoy left empty-handed, and did not hide it." }]
}

define_event(#{
    id: "whispers-at-court",
    title: "Whispers at Court",
    family: "political",
    weight: 90,
    cooldown_days: 360,
    weighty: false,
    text: "Talk about {subject} runs through the Consulate corridors.",
    log_text: "Talk about {subject} ran through the Consulate corridors.",
});

// ---------------------------------------------------------------------------
// Travel
// ---------------------------------------------------------------------------

define_event(#{
    id: "hard-passage",
    title: "A Hard Passage",
    family: "travel",
    weight: 100,
    cooldown_days: 180,
    weighty: false,
    text: "{subject} makes a rough crossing.",
    log_text: "{subject} made a rough crossing.",
});

define_event(#{
    id: "road-petition",
    title: "A Petition on the Road",
    family: "travel",
    weight: 120,
    cooldown_days: 720,
    weighty: true,
    requires: #{ player_only: true },
    text: "{subject} is stopped on the road by tenants with a grievance against their own steward. They are asking for judgement, here, in the open.",
    log_text: "{subject} was stopped on the road by petitioners.",
    choices: [
        #{ id: "hear-them", label: "Hear them out", effect_fn: "petition_heard" },
        #{ id: "ride-on", label: "Ride on", effect_fn: "petition_ignored" },
    ],
});
fn petition_heard(ctx) {
    [#{ kind: "order", scope: "all-held", amount: 30 },
     #{ kind: "log", message: "The judgement was given on the roadside, and travelled." }]
}
fn petition_ignored(ctx) {
    [#{ kind: "order", scope: "all-held", amount: -20 }]
}

// ---------------------------------------------------------------------------
// Job complications
// ---------------------------------------------------------------------------

define_event(#{
    id: "work-delayed",
    title: "The Work Drags",
    family: "job",
    weight: 100,
    cooldown_days: 240,
    weighty: false,
    text: "The work is going slower than promised.",
    log_text: "Word comes that the work is going slower than promised.",
});
