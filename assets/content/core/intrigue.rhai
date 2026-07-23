// The shadows: work aimed at a person, to have them killed or taken.
//
// These are not reached by the reactive scorer — a house does not murder
// on a whim — but a plan may call for them, and the player may order
// them by hand. The work is hard and the one who leads it risks capture,
// scandal, and, at worst, their own life; a house exposed in the attempt
// earns a lasting grievance from the house it struck at.

define_assignment(#{
    id: "assassinate",
    category: "consequential",
    duration_days: 45,
    skill: "intrigue",
    difficulty: 16,
    target: "character",
    risks: ["capture", "scandal", "death"],
    ai_available: false,
    requires: #{ target_house: "other" },
    results: #{
        critical_success: #{
            weight: 150, popup: true, log: true,
            effect_fn: "assassination_done",
        },
        success: #{
            weight: 250, popup: true, log: true,
            effect_fn: "assassination_done",
        },
        failure: #{ weight: 400 },
        disaster: #{
            weight: 200, popup: true, log: true,
            effect_fn: "assassination_exposed",
        },
    },
});
fn assassination_done(ctx) {
    [#{ kind: "condition", target: "target", tag: "death" }]
}
fn assassination_exposed(ctx) {
    [#{ kind: "obligation", action: "create", obligation: "grievance",
        debtor: "owner-head", creditor: "target-head", weight: 40,
        origin: "an attempt on their life, laid bare" }]
}

define_assignment(#{
    id: "abduct",
    category: "consequential",
    duration_days: 30,
    skill: "intrigue",
    difficulty: 12,
    target: "character",
    risks: ["capture", "scandal"],
    ai_available: false,
    requires: #{ target_house: "other" },
    results: #{
        success: #{
            weight: 450, popup: true, log: true,
            effect_fn: "abduction_done",
        },
        failure: #{ weight: 350 },
        disaster: #{
            weight: 200, popup: true, log: true,
            effect_fn: "abduction_exposed",
        },
    },
});
fn abduction_done(ctx) {
    [#{ kind: "condition", target: "target", tag: "capture" }]
}
fn abduction_exposed(ctx) {
    [#{ kind: "obligation", action: "create", obligation: "grievance",
        debtor: "owner-head", creditor: "target-head", weight: 25,
        origin: "a kidnapping, botched and exposed" }]
}
