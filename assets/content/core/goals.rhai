// Grand-strategy goals: the standing ambitions a house head may adopt.
//
// A goal does not act. While it is held it lifts the pressures it
// favours in the head's own scoring, so the plans and assignments
// already in place carry it out, and it presses advisory directives on
// the house's vassals. Conditions are data, never script.

// The Consulship, worked toward rather than fallen into: a house shores
// up its standing against the day a vacancy opens, and asks its vassals
// to lend their own weight to the effort.
define_goal(#{
    id: "become-consul",
    favours: ["standing"],
    favour_bonus: 40,
    max_days: 5400,
    cooldown_days: 1080,
    trigger: #{ is_vassal: false, min_legitimacy: 40 },
    directives: [ #{ intent: "standing" } ],
});

// The planet itself. A house that already holds more of it than any
// rival bends everything toward pressing the claim, and calls its
// vassals to arms behind the attempt.
define_goal(#{
    id: "take-the-planet",
    favours: ["claim", "muster"],
    favour_bonus: 60,
    max_days: 5400,
    cooldown_days: 1080,
    trigger: #{ dominant_claimant: true },
    directives: [ #{ intent: "muster" } ],
});

// A weaker neighbour, marked for conquest: the house arms and steadies
// itself, and directs its vassals to muster against the same rival.
define_goal(#{
    id: "conquer-a-neighbour",
    favours: ["muster", "order"],
    favour_bonus: 30,
    target: "organisation",
    max_days: 5400,
    cooldown_days: 1080,
    trigger: #{ is_vassal: false, has_army: true, has_vassals: true },
    directives: [ #{ intent: "muster", target: "goal" } ],
});
