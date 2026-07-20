// Military operations: army- and ship-led jobs whose operational
// semantics (movement, engagements, conquest, loot, blockade) are
// engine-owned. Content declares pacing, costs, risks, and flavour.

define_job(#{
    id: "march-the-army",
    category: "consequential",
    duration_days: 5,
    skill: "command",
    difficulty: 4,
    target: "own-army-and-province",
    military_op: "move",
    ai_available: false,
    results: #{
        success: #{ weight: 950 },
        failure: #{
            weight: 50, log: true,
        },
    },
});

define_job(#{
    id: "resupply-the-army",
    category: "consequential",
    duration_days: 15,
    skill: "stewardship",
    difficulty: 6,
    target: "own-army",
    military_op: "resupply",
    ai_available: false,
    results: #{
        success: #{ weight: 900 },
        failure: #{ weight: 100 },
    },
});

define_job(#{
    id: "patrol-the-province",
    category: "routine",
    duration_days: 30,
    skill: "command",
    difficulty: 5,
    target: "own-army",
    military_op: "patrol",
    ai_available: false,
    results: #{
        success: #{ weight: 900, effect_fn: "patrol_kept" },
        failure: #{ weight: 100 },
    },
});
fn patrol_kept(ctx) {
    [#{ kind: "order", scope: "target-province", amount: 80 }]
}

define_job(#{
    id: "lay-siege",
    category: "consequential",
    duration_days: 60,
    skill: "command",
    difficulty: 14,
    target: "own-army-and-province",
    military_op: "besiege",
    risks: ["injury", "death"],
    supplies_cost: 40,
    influence_cost: 20,
    ai_available: false,
    results: #{
        critical_success: #{
            weight: 100, popup: true, log: true,
        },
        success: #{
            weight: 500, popup: true, log: true,
        },
        failure: #{
            weight: 350, log: true,
        },
        disaster: #{
            weight: 50, popup: true, log: true,
        },
    },
});

define_job(#{
    id: "raid-the-province",
    category: "consequential",
    duration_days: 25,
    skill: "command",
    difficulty: 10,
    target: "own-army-and-province",
    military_op: "raid",
    risks: ["injury", "capture"],
    supplies_cost: 15,
    ai_available: false,
    results: #{
        critical_success: #{
            weight: 150, log: true,
        },
        success: #{
            weight: 500, log: true,
        },
        failure: #{
            weight: 300, log: true,
        },
        disaster: #{
            weight: 50, log: true,
        },
    },
});

define_job(#{
    id: "blockade-the-port",
    category: "consequential",
    duration_days: 20,
    skill: "command",
    difficulty: 8,
    target: "own-ship-and-province",
    military_op: "blockade",
    supplies_cost: 10,
    ai_available: false,
    results: #{
        success: #{
            weight: 800, log: true,
        },
        failure: #{ weight: 200 },
    },
});

// Standing orders start this when a defended holding is threatened.
define_job(#{
    id: "answer-the-alarm",
    category: "consequential",
    duration_days: 3,
    skill: "command",
    difficulty: 6,
    target: "own-army-and-province",
    military_op: "move",
    ai_available: false,
    results: #{
        success: #{ weight: 950 },
        failure: #{ weight: 50 },
    },
});

define_job(#{
    id: "negotiate-standdown",
    category: "consequential",
    duration_days: 20,
    skill: "diplomacy",
    difficulty: 12,
    target: "organisation",
    influence_cost: 10,
    ai_available: false,
    results: #{
        success: #{
            weight: 550, log: true,
            effect_fn: "standdown_success",
        },
        failure: #{ weight: 400 },
        disaster: #{
            weight: 50, log: true,
            effect_fn: "standdown_disaster",
        },
    },
});

fn standdown_success(ctx) {
    [#{ kind: "opinion", from: "target-head", toward: "owner-head", amount: 15, days: 720, reason: "standdown" },
     #{ kind: "opinion", from: "owner-head", toward: "target-head", amount: 15, days: 720, reason: "standdown" }]
}

fn standdown_disaster(ctx) {
    [#{ kind: "opinion", from: "target-head", toward: "owner-head", amount: -10, days: 720, reason: "spurned-overture" }]
}
