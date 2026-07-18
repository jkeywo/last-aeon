// Military operations: army- and ship-led jobs whose operational
// semantics (movement, engagements, conquest, loot, blockade) are
// engine-owned. Content declares pacing, costs, risks, and flavour.

define_job(#{
    id: "march-the-army",
    title: "March the Army",
    summary: "Move an army to another province. Marches take as long as the road demands.",
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
            log_text: "{leader}'s march on {target} dissolved in confusion.",
        },
    },
});

define_job(#{
    id: "resupply-the-army",
    title: "Resupply the Army",
    summary: "Refill an army's supply train from the house stores.",
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
    title: "Patrol",
    summary: "Stand guard where the army is; a visible deterrent to raiders.",
    category: "routine",
    duration_days: 30,
    skill: "command",
    difficulty: 5,
    target: "own-army",
    military_op: "patrol",
    ai_available: false,
    results: #{
        success: #{ weight: 900 },
        failure: #{ weight: 100 },
    },
});

define_job(#{
    id: "lay-siege",
    title: "Lay Siege",
    summary: "March on a province and take it. Its garrison must be beaten first.",
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
            popup_text: "{target} has fallen to {leader}, and quickly enough that the granaries are intact.",
            log_text: "{target} fell swiftly to {leader}.",
        },
        success: #{
            weight: 500, popup: true, log: true,
            popup_text: "After a grinding investment, {target} has fallen to {leader}.",
            log_text: "{target} fell to {leader} after a siege.",
        },
        failure: #{
            weight: 350, log: true,
            log_text: "The siege of {target} was broken.",
        },
        disaster: #{
            weight: 50, popup: true, log: true,
            popup_text: "The siege of {target} collapsed in recrimination and blood.",
            log_text: "The siege of {target} ended in disaster.",
        },
    },
});

define_job(#{
    id: "raid-the-province",
    title: "Raid",
    summary: "Strike a province for loot and be gone before the response forms.",
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
            log_text: "{leader}'s raid on {target} came home heavy with plunder.",
        },
        success: #{
            weight: 500, log: true,
            log_text: "{leader} raided {target}.",
        },
        failure: #{
            weight: 300, log: true,
            log_text: "{leader}'s raid on {target} was driven off.",
        },
        disaster: #{
            weight: 50, log: true,
            log_text: "{leader}'s raiders were cut apart at {target}.",
        },
    },
});

define_job(#{
    id: "blockade-the-port",
    title: "Blockade",
    summary: "Station a ship over a province and choke its trade.",
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
            log_text: "{target} is under blockade.",
        },
        failure: #{ weight: 200 },
    },
});

// Standing orders start this when a defended holding is threatened.
define_job(#{
    id: "answer-the-alarm",
    title: "Answer the Alarm",
    summary: "March to relieve a threatened holding.",
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
    title: "Negotiate a Stand-Down",
    summary: "Talk a rival back from the brink before more blood is spilt.",
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
            log_text: "{leader} negotiated a stand-down with {target}.",
            effect_fn: "standdown_success",
        },
        failure: #{ weight: 400 },
        disaster: #{
            weight: 50, log: true,
            log_text: "{leader}'s overture to {target} was thrown back in their face.",
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
