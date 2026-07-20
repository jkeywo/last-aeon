// Core assignments available to every house.
//
// Authoring notes: definitions are data via define_* calls; behaviour is
// named functions in this same file, referenced by effect_fn. Functions
// receive a read-only context map and return an array of effect maps.
// Text templates may use {leader}, {target}, and {assignment}.

define_assignment(#{
    id: "manage-estates",
    ai_intent: "resources",
    category: "routine",
    duration_days: 30,
    skill: "stewardship",
    difficulty: 6,
    results: #{
        success: #{ weight: 850, effect_fn: "estates_kept" },
        failure: #{ weight: 150 },
    },
});
fn estates_kept(ctx) {
    [#{ kind: "order", scope: "all-held", amount: 25 }]
}

define_assignment(#{
    id: "hold-court",
    ai_intent: "order",
    category: "routine",
    duration_days: 20,
    skill: "diplomacy",
    difficulty: 5,
    results: #{
        success: #{ weight: 800, effect_fn: "court_settled" },
        failure: #{ weight: 200 },
    },
});
fn court_settled(ctx) {
    [#{ kind: "order", scope: "target-province", amount: 60 }]
}

define_assignment(#{
    id: "court-a-rival-house",
    ai_intent: "standing",
    category: "consequential",
    duration_days: 45,
    wealth_cost: 15,
    skill: "diplomacy",
    difficulty: 10,
    target: "organisation",
    risks: ["scandal"],
    results: #{
        critical_success: #{
            weight: 100, popup: true, log: true,
            effect_fn: "courting_crit",
        },
        success: #{
            weight: 500, log: true,
            effect_fn: "courting_success",
        },
        failure: #{ weight: 300 },
        disaster: #{
            weight: 100, popup: true, log: true,
            effect_fn: "courting_disaster",
        },
    },
});

fn courting_crit(ctx) {
    [#{ kind: "opinion", from: "target-head", toward: "leader", amount: 25, days: 1800, reason: "courted" },
     #{ kind: "opinion", from: "target-head", toward: "owner-head", amount: 15, days: 1800, reason: "courted-house" }]
}

fn courting_success(ctx) {
    [#{ kind: "opinion", from: "target-head", toward: "leader", amount: 10, days: 1080, reason: "courted" },
     #{ kind: "opinion", from: "target-head", toward: "owner-head", amount: 10, days: 1080, reason: "courted-house" }]
}

fn courting_disaster(ctx) {
    [#{ kind: "opinion", from: "target-head", toward: "leader", amount: -20, days: 1800, reason: "insulted" },
     #{ kind: "opinion", from: "target-head", toward: "owner-head", amount: -15, days: 1800, reason: "insulted-house" }]
}

define_assignment(#{
    id: "curry-favour-with-the-sanctora",
    ai_intent: "standing",
    category: "consequential",
    duration_days: 60,
    wealth_cost: 25,
    skill: "diplomacy",
    difficulty: 12,
    risks: ["scandal"],
    results: #{
        critical_success: #{
            weight: 100, log: true,
            effect_fn: "sanctora_crit",
        },
        success: #{
            weight: 450, log: true,
            effect_fn: "sanctora_success",
        },
        failure: #{ weight: 350 },
        disaster: #{
            weight: 100, popup: true, log: true,
            effect_fn: "sanctora_disaster",
        },
    },
});

fn sanctora_crit(ctx) {
    [#{ kind: "opinion", from: "sanctora", toward: "leader", amount: 20, days: 1800, reason: "favour" }]
}

fn sanctora_success(ctx) {
    [#{ kind: "opinion", from: "sanctora", toward: "leader", amount: 10, days: 1080, reason: "favour" }]
}

fn sanctora_disaster(ctx) {
    [#{ kind: "opinion", from: "sanctora", toward: "leader", amount: -15, days: 1800, reason: "gaffe" }]
}

define_assignment(#{
    id: "spread-whispers",
    category: "consequential",
    duration_days: 40,
    skill: "intrigue",
    difficulty: 12,
    target: "character",
    risks: ["scandal", "capture"],
    results: #{
        critical_success: #{
            weight: 100, log: true,
            effect_fn: "whispers_crit",
        },
        success: #{
            weight: 450,
            effect_fn: "whispers_success",
        },
        failure: #{ weight: 350 },
        disaster: #{
            weight: 100, popup: true, log: true,
            effect_fn: "whispers_disaster",
        },
    },
});

fn whispers_crit(ctx) {
    [#{ kind: "opinion", from: "liege-head", toward: "target", amount: -20, days: 1440, reason: "whispers" },
     #{ kind: "opinion", from: "sanctora", toward: "target", amount: -10, days: 1440, reason: "whispers" }]
}

fn whispers_success(ctx) {
    [#{ kind: "opinion", from: "liege-head", toward: "target", amount: -10, days: 1080, reason: "whispers" }]
}

fn whispers_disaster(ctx) {
    [#{ kind: "opinion", from: "target", toward: "leader", amount: -30, days: 1800, reason: "exposed-whispers" }]
}

define_assignment(#{
    id: "tour-the-holdings",
    ai_intent: "order",
    category: "consequential",
    duration_days: 50,
    skill: "diplomacy",
    difficulty: 8,
    risks: ["injury", "incapacity"],
    results: #{
        critical_success: #{
            weight: 150, log: true,
            effect_fn: "tour_triumph",
        },
        success: #{ weight: 550, effect_fn: "tour_done" },
        failure: #{ weight: 250 },
        disaster: #{
            weight: 50, popup: true, log: true,
            effect_fn: "tour_soured",
        },
    },
});
fn tour_triumph(ctx) {
    [#{ kind: "order", scope: "all-held", amount: 120 }]
}
fn tour_done(ctx) {
    [#{ kind: "order", scope: "all-held", amount: 60 }]
}
fn tour_soured(ctx) {
    [#{ kind: "order", scope: "all-held", amount: -40 }]
}

define_assignment(#{
    id: "petition-the-consul",
    category: "consequential",
    duration_days: 30,
    influence_cost: 10,
    skill: "diplomacy",
    difficulty: 14,
    risks: ["scandal"],
    results: #{
        success: #{
            weight: 450, log: true,
            effect_fn: "petition_success",
        },
        failure: #{ weight: 450 },
        disaster: #{
            weight: 100, log: true,
            effect_fn: "petition_disaster",
        },
    },
});

fn petition_success(ctx) {
    [#{ kind: "opinion", from: "consul", toward: "owner-head", amount: 10, days: 1080, reason: "petition" }]
}

fn petition_disaster(ctx) {
    [#{ kind: "opinion", from: "consul", toward: "owner-head", amount: -10, days: 1080, reason: "petition" }]
}

define_assignment(#{
    id: "muster-the-levies",
    ai_intent: "muster",
    category: "consequential",
    duration_days: 40,
    skill: "command",
    difficulty: 8,
    wealth_cost: 50,
    influence_cost: 10,
    risks: ["injury"],
    ai_available: false,
    results: #{
        critical_success: #{
            weight: 150, log: true,
            effect_fn: "muster_crit",
        },
        success: #{
            weight: 600, log: true,
            effect_fn: "muster_success",
        },
        failure: #{ weight: 250 },
    },
});

fn muster_crit(ctx) {
    [#{ kind: "form-army", manpower: 700, supplies: 160 }]
}

fn muster_success(ctx) {
    [#{ kind: "form-army", manpower: 500, supplies: 100 }]
}

// --- The planetary succession crisis ---

define_assignment(#{
    id: "press-claim-to-the-paramountcy",
    ai_intent: "claim",
    category: "consequential",
    duration_days: 90,
    skill: "diplomacy",
    difficulty: 12,
    influence_cost: 40,
    risks: ["scandal"],
    ai_available: true,
    results: #{
        critical_success: #{
            weight: 200, popup: true, log: true,
            effect_fn: "press_claim",
        },
        success: #{
            weight: 500, log: true,
            effect_fn: "press_claim",
        },
        failure: #{ weight: 300, log: true, },
    },
});

fn press_claim(ctx) {
    [#{ kind: "claim-paramountcy" }]
}

define_assignment(#{
    id: "collect-imperial-tithes",
    category: "consequential",
    duration_days: 60,
    skill: "stewardship",
    difficulty: 10,
    ai_available: true,
    results: #{
        success: #{
            weight: 700, log: true,
            effect_fn: "levy_tithes",
        },
        failure: #{ weight: 300 },
    },
});

fn levy_tithes(ctx) {
    [#{ kind: "collect-tithes" }]
}

define_assignment(#{
    id: "seek-consular-endorsement",
    ai_intent: "standing",
    category: "consequential",
    duration_days: 45,
    skill: "diplomacy",
    difficulty: 13,
    influence_cost: 15,
    risks: ["scandal"],
    ai_available: true,
    results: #{
        critical_success: #{
            weight: 100, popup: true, log: true,
            effect_fn: "endorsement_crit",
        },
        success: #{
            weight: 450, log: true,
            effect_fn: "endorsement_success",
        },
        failure: #{ weight: 350 },
        disaster: #{
            weight: 100, log: true,
            effect_fn: "endorsement_disaster",
        },
    },
});

fn endorsement_crit(ctx) {
    [#{ kind: "opinion", from: "consul", toward: "owner-head", amount: 25, days: 1440, reason: "endorsed" },
     #{ kind: "opinion", from: "sanctora", toward: "owner-head", amount: 10, days: 1440, reason: "endorsed" }]
}

fn endorsement_success(ctx) {
    [#{ kind: "opinion", from: "consul", toward: "owner-head", amount: 12, days: 1080, reason: "endorsed" }]
}

fn endorsement_disaster(ctx) {
    [#{ kind: "opinion", from: "consul", toward: "owner-head", amount: -15, days: 1440, reason: "rebuffed" }]
}

define_assignment(#{
    id: "call-in-a-favour",
    ai_intent: "obligation",
    category: "consequential",
    duration_days: 25,
    skill: "diplomacy",
    difficulty: 8,
    target: "organisation",
    ai_available: true,
    influence_cost: 5,
    results: #{
        critical_success: #{
            weight: 200, log: true,
            effect_fn: "favour_called_well",
        },
        success: #{
            weight: 500, log: true,
            effect_fn: "favour_called",
        },
        failure: #{
            weight: 250, log: true,
        },
        disaster: #{
            weight: 50, popup: true, log: true,
            effect_fn: "favour_repudiated",
        },
    },
});
fn favour_called_well(ctx) {
    [#{ kind: "obligation", action: "fulfil", obligation: "favour",
        debtor: "target-head", creditor: "owner-head" },
     #{ kind: "opinion", from: "target-head", toward: "owner-head",
        amount: 10, days: 720, reason: "debt-honoured" }]
}
fn favour_called(ctx) {
    [#{ kind: "obligation", action: "fulfil", obligation: "favour",
        debtor: "target-head", creditor: "owner-head" }]
}
fn favour_repudiated(ctx) {
    [#{ kind: "obligation", action: "break", obligation: "favour",
        debtor: "target-head", creditor: "owner-head" },
     #{ kind: "obligation", action: "create", obligation: "grievance",
        debtor: "target-head", creditor: "owner-head", weight: 25,
        origin: "a debt publicly repudiated" }]
}
