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
    family: "province",
    weight: 120,
    cooldown_days: 540,
    weighty: true,
    requires: #{ player_only: true, max_order: 500 },
    choices: [
        #{ id: "send-soldiers", effect_fn: "riot_suppressed" },
        #{ id: "open-stores", effect_fn: "riot_bought_off" },
    ],
});
fn riot_suppressed(ctx) {
    [#{ kind: "order", scope: "target-province", amount: 120 },
     #{ kind: "log", message_key: "event.riot-suppressed.log" }]
}
fn riot_bought_off(ctx) {
    [#{ kind: "order", scope: "target-province", amount: 60 },
     #{ kind: "log", message_key: "event.riot-bought-off.log" }]
}

define_event(#{
    id: "occupiers-quarter",
    family: "province",
    weight: 100,
    cooldown_days: 360,
    weighty: false,
    requires: #{ occupied: true },
    effect_fn: "occupation_bites",
});
fn occupation_bites(ctx) {
    [#{ kind: "order", scope: "target-province", amount: -40 }]
}

define_event(#{
    id: "good-harvest",
    family: "province",
    weight: 80,
    cooldown_days: 720,
    weighty: false,
    requires: #{ min_order: 700 },
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
    family: "political",
    weight: 110,
    cooldown_days: 900,
    weighty: true,
    requires: #{ player_only: true, has_open_obligation: true },
    choices: [
        #{ id: "honour-it", effect_fn: "debt_honoured" },
        #{ id: "put-them-off", effect_fn: "debt_deferred" },
    ],
});
fn debt_honoured(ctx) {
    [#{ kind: "log", message_key: "event.debt-honoured.log" }]
}
fn debt_deferred(ctx) {
    [#{ kind: "log", message_key: "event.debt-deferred.log" }]
}

define_event(#{
    id: "whispers-at-court",
    family: "political",
    weight: 90,
    cooldown_days: 360,
    weighty: false,
});

// ---------------------------------------------------------------------------
// Travel
// ---------------------------------------------------------------------------

define_event(#{
    id: "hard-passage",
    family: "travel",
    weight: 100,
    cooldown_days: 180,
    weighty: false,
});

define_event(#{
    id: "road-petition",
    family: "travel",
    weight: 120,
    cooldown_days: 720,
    weighty: true,
    requires: #{ player_only: true },
    choices: [
        #{ id: "hear-them", effect_fn: "petition_heard" },
        #{ id: "ride-on", effect_fn: "petition_ignored" },
    ],
});
fn petition_heard(ctx) {
    [#{ kind: "order", scope: "all-held", amount: 30 },
     #{ kind: "log", message_key: "event.petition-heard.log" }]
}
fn petition_ignored(ctx) {
    [#{ kind: "order", scope: "all-held", amount: -20 }]
}

// ---------------------------------------------------------------------------
// Assignment complications
// ---------------------------------------------------------------------------

define_event(#{
    id: "work-delayed",
    family: "assignment",
    weight: 100,
    cooldown_days: 240,
    weighty: false,
});
