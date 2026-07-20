// Plans: the campaigns autonomous characters may set their minds to.
//
// A plan answers a heavy pressure with an ordered sequence of ordinary
// assignments where the reactive scorer would answer it with one. Steps
// may name assignments the scorer cannot reach (ai_available: false);
// the gate that matters is validate_start, which every step still
// passes. Conditions here are data, never script.

// Pressing the paramountcy claim becomes a campaign: fund it, raise
// troops behind it, win the Consul's ear, then declare — rather than a
// single declaration made cold.
define_plan(#{
    id: "press-the-claim",
    goal: "claim",
    cooldown_days: 360,
    max_days: 540,
    max_step_retries: 2,
    methods: [
        #{ id: "the-long-road",
           requires: #{ dominant_claimant: true },
           steps: [
               #{ id: "fund", start: "collect-tithes", skip_if: #{ min_wealth: 150 } },
               #{ id: "muster", start: "muster", skip_if: #{ has_army: true } },
               // A declaration invites answers; the claimant's own force
               // stands ready for them before the claim is voiced.
               #{ id: "arm", orders: ["respond", "patrol"], army: "own" },
               #{ id: "endorse", start: "seek-endorsement" },
               #{ id: "declare", start: "press-claim" },
           ] },
    ],
});

// Occupied ground gets an armed answer: raise a levy if none stands,
// then point the head's own force at the doctrine — march to whatever
// holding is threatened, and stand guard between alarms.
define_plan(#{
    id: "ready-the-levies",
    goal: "muster",
    cooldown_days: 180,
    max_days: 240,
    max_step_retries: 1,
    methods: [
        #{ id: "call-and-answer",
           steps: [
               #{ id: "raise", start: "muster", skip_if: #{ has_army: true } },
               #{ id: "stand-to", orders: ["respond", "patrol"], army: "own" },
           ] },
    ],
});

// A household ambition: an idle courtier takes the realm's disquiet
// personally, holding court and touring the holdings — work that costs
// the house nothing but their time.
define_plan(#{
    id: "steady-the-shires",
    goal: "order",
    cooldown_days: 120,
    max_days: 200,
    max_step_retries: 1,
    methods: [
        #{ id: "in-person",
           steps: [
               #{ id: "court", start: "hold-court" },
               #{ id: "tour", start: "tour-holdings" },
           ] },
    ],
});

// A house that has wronged another answers the grievance over time:
// from strength, it simply courts the aggrieved house; from weakness,
// it shores up its own standing first.
define_plan(#{
    id: "answer-a-grievance",
    goal: "standing",
    target: "organisation",
    score_bonus: 40,
    cooldown_days: 240,
    max_days: 360,
    max_step_retries: 1,
    methods: [
        #{ id: "from-strength",
           requires: #{ min_legitimacy: 40 },
           steps: [
               #{ id: "make-amends", start: "court", target: "plan" },
           ] },
        #{ id: "from-weakness",
           steps: [
               #{ plan: "shore-up-standing" },
               #{ id: "make-amends", start: "court", target: "plan" },
           ] },
    ],
});

// Rebuilding thin standing, usable on its own or expanded inside
// another plan.
define_plan(#{
    id: "shore-up-standing",
    goal: "standing",
    cooldown_days: 180,
    max_days: 300,
    max_step_retries: 1,
    methods: [
        #{ id: "win-friends",
           steps: [
               #{ id: "flatter", start: "curry-favour" },
               #{ id: "endorse", start: "seek-endorsement" },
           ] },
    ],
});
