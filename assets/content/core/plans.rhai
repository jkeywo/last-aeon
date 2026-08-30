// Plans: the campaigns autonomous characters may set their minds to.
//
// A plan answers a heavy pressure with an ordered sequence of ordinary
// assignments where the reactive scorer would answer it with one. Steps
// may name assignments the scorer cannot reach (ai_available: false);
// the gate that matters is validate_start, which every step still
// passes. Conditions here are data, never script.

// The personal claim is declared before dominance and pressed only once the
// claimant's complete realm leads and no rival claimant remains at war.
define_plan(#{
    id: "press-the-claim",
    goal: "claim",
    cooldown_days: 0,
    max_days: 540,
    max_step_retries: 2,
    methods: [
        #{ id: "declare",
           requires: #{ has_paramount_claim: false },
           steps: [
               #{ id: "declare", start: "declare-paramount-claim" },
           ] },
        #{ id: "press-after-peace",
           requires: #{ has_paramount_claim: true, dominant_claimant: true },
           steps: [
               #{ id: "press", start: "press-claim" },
           ] },
    ],
});

// A trailing claimant below the challenge floor raises another force. The
// comparison is against the complete political branches a declaration would
// freeze, not only the two leaders' household troops.
define_plan(#{
    id: "prepare-claimant-challenge",
    goal: "muster",
    target: "organisation",
    cooldown_days: 0,
    max_days: 180,
    max_step_retries: 2,
    methods: [
        #{ id: "raise-strength",
           requires: #{
               has_paramount_claim: true,
               max_target_branch_manpower_permille: 749,
           },
           steps: [
               #{ id: "muster", start: "muster" },
           ] },
    ],
});

// Once the prospective branch reaches three quarters of the leading
// claimant's raised manpower, the head issues the ordinary seven-day war
// declaration.
define_plan(#{
    id: "challenge-leading-claimant",
    goal: "claim",
    target: "organisation",
    cooldown_days: 0,
    max_days: 90,
    max_step_retries: 1,
    methods: [
        #{ id: "challenge",
           requires: #{
               has_paramount_claim: true,
               min_target_branch_manpower_permille: 750,
           },
           steps: [
               #{ id: "declare-war", start: "declare-formal-war", target: "plan" },
           ] },
    ],
});

// An exact claimant WarId is prosecuted one province at a time. Each fresh
// zero-cooldown adoption selects today's lowest-order enemy province. Once
// dominance is achieved, or the head has no army left to continue, the same
// authored campaign turns to probabilistic whole-war peace.
define_plan(#{
    id: "prosecute-claimant-war",
    goal: "claim",
    target: "war",
    cooldown_days: 0,
    max_days: 540,
    max_step_retries: 2,
    methods: [
        #{ id: "settle-from-strength",
           requires: #{ dominant_claimant: true },
           steps: [
               #{ id: "peace", start: "negotiate", target: "plan" },
           ] },
        #{ id: "settle-when-spent",
           requires: #{ dominant_claimant: false, has_army: false },
           steps: [
               #{ id: "peace", start: "negotiate", target: "plan" },
           ] },
        #{ id: "settle-with-no-ground-left",
           requires: #{
               dominant_claimant: false,
               has_army: true,
               war_has_enemy_province: false,
           },
           steps: [
               #{ id: "peace", start: "negotiate", target: "plan" },
           ] },
        #{ id: "take-ground",
           requires: #{
               dominant_claimant: false,
               has_army: true,
               war_has_enemy_province: true,
           },
           steps: [
               #{ id: "guard-the-realm", orders: ["respond", "patrol"], army: "own" },
               #{ id: "siege", start: "besiege", target: "lowest-enemy-province-in-war" },
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
        // Or answer the grievance in blood: a house with the stomach and
        // the standing to spend has the rival's head removed.
        #{ id: "from-the-shadows",
           requires: #{ min_influence: 30, min_legitimacy: 55 },
           steps: [
               #{ id: "remove", start: "assassinate", target: "target-head" },
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
