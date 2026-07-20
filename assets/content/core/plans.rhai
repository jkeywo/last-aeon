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
               #{ id: "arm", start: "muster", skip_if: #{ has_army: true } },
               #{ id: "endorse", start: "seek-endorsement" },
               #{ id: "declare", start: "press-claim" },
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
