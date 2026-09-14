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

// The Consulship by other means: a strong, well-placed house that wants
// the seat does not wait on a natural death but makes the vacancy it
// means to win. The ambition is aimed at the house whose member holds
// the Consulship — nobody's, while the seat is vacant — and it lifts the
// deniable subvert pressure, so the campaign that answers it is an
// ordinary covert plan and the knife an ordinary intrigue assignment,
// with all the risk of exposure that carries. The ambition is set aside
// the moment the seat has passed from that house's head, whether by the
// knife or by nature; the contest that follows is the become-consul
// ambition's to work toward.
define_goal(#{
    id: "unseat-the-consul",
    priority: 10,
    favours: ["subvert"],
    favour_bonus: 60,
    target: "organisation",
    target_selector: #{ kind: "consul-house" },
    covert: true,
    trigger: #{ is_vassal: false, min_legitimacy: 55, min_influence: 30 },
    set_aside_when: #{ target_head_lacks_title: "consul" },
    max_days: 720,
    cooldown_days: 1440,
});

// The planet itself. Any living adult independent dynastic head may set this
// ambition while the Paramountcy is vacant; dominance is something the
// ensuing authored plans must win, not a prerequisite for wanting it.
define_goal(#{
    id: "take-the-planet",
    priority: 100,
    favours: ["claim", "muster"],
    favour_bonus: 60,
    max_days: 5400,
    cooldown_days: 1080,
    trigger: #{ can_claim_paramountcy: true },
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
