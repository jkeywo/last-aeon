// Core jobs available to every house.
//
// Authoring notes: definitions are data via define_* calls; behaviour is
// named functions in this same file, referenced by effect_fn. Functions
// receive a read-only context map and return an array of effect maps.
// Text templates may use {leader}, {target}, and {job}.

define_job(#{
    id: "manage-estates",
    title: "Manage the Estates",
    summary: "Routine administration of the house's provincial holdings.",
    category: "routine",
    duration_days: 30,
    skill: "stewardship",
    difficulty: 6,
    results: #{
        success: #{ weight: 850 },
        failure: #{ weight: 150 },
    },
});

define_job(#{
    id: "hold-court",
    title: "Hold Court",
    summary: "Hear petitions and settle disputes among your household.",
    category: "routine",
    duration_days: 20,
    skill: "diplomacy",
    difficulty: 5,
    results: #{
        success: #{ weight: 800 },
        failure: #{ weight: 200 },
    },
});

define_job(#{
    id: "court-a-rival-house",
    title: "Court a Rival House",
    summary: "Send an envoy to soften another house's standing against you.",
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
            popup_text: "{leader} returns from {target} with warm assurances and an open door.",
            log_text: "{leader} won unexpected goodwill from {target}.",
            effect_fn: "courting_crit",
        },
        success: #{
            weight: 500, log: true,
            log_text: "{leader} improved relations with {target}.",
            effect_fn: "courting_success",
        },
        failure: #{ weight: 300 },
        disaster: #{
            weight: 100, popup: true, log: true,
            popup_text: "{leader} gave insult at the court of {target}; doors close across the region.",
            log_text: "{leader} insulted {target}; relations sour.",
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

define_job(#{
    id: "curry-favour-with-the-sanctora",
    title: "Curry Favour with the Sanctora",
    summary: "Pay respects, tithes, and flattery aboard the Spire.",
    category: "consequential",
    duration_days: 60,
    wealth_cost: 25,
    skill: "diplomacy",
    difficulty: 12,
    risks: ["scandal"],
    results: #{
        critical_success: #{
            weight: 100, log: true,
            log_text: "{leader} is spoken of warmly throughout the Spire.",
            effect_fn: "sanctora_crit",
        },
        success: #{
            weight: 450, log: true,
            log_text: "{leader} made a good impression on the Sanctora.",
            effect_fn: "sanctora_success",
        },
        failure: #{ weight: 350 },
        disaster: #{
            weight: 100, popup: true, log: true,
            popup_text: "{leader} misjudged the Spire's etiquette badly. The adepts remember.",
            log_text: "{leader} embarrassed themselves before the Sanctora.",
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

define_job(#{
    id: "spread-whispers",
    title: "Spread Whispers",
    summary: "Undermine a rival with rumour and insinuation.",
    category: "consequential",
    duration_days: 40,
    skill: "intrigue",
    difficulty: 12,
    target: "character",
    risks: ["scandal", "capture"],
    results: #{
        critical_success: #{
            weight: 100, log: true,
            log_text: "Whispers against {target} take root everywhere at once.",
            effect_fn: "whispers_crit",
        },
        success: #{
            weight: 450,
            effect_fn: "whispers_success",
        },
        failure: #{ weight: 350 },
        disaster: #{
            weight: 100, popup: true, log: true,
            popup_text: "The whisper campaign is traced back to {leader}. {target} knows.",
            log_text: "{leader} was caught spreading whispers against {target}.",
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

define_job(#{
    id: "tour-the-holdings",
    title: "Tour the Holdings",
    summary: "Show the household's face across its provinces.",
    category: "consequential",
    duration_days: 50,
    skill: "diplomacy",
    difficulty: 8,
    risks: ["injury", "incapacity"],
    results: #{
        critical_success: #{
            weight: 150, log: true,
            log_text: "{leader}'s tour of the holdings is met with genuine warmth.",
        },
        success: #{ weight: 550 },
        failure: #{ weight: 250 },
        disaster: #{
            weight: 50, popup: true, log: true,
            popup_text: "An accident on the road; {leader} is hurt, and the tour is remembered badly.",
            log_text: "{leader}'s tour ended in an ugly accident.",
        },
    },
});

define_job(#{
    id: "petition-the-consul",
    title: "Petition the Consul",
    summary: "Seek the Consul's ear for your house's interests.",
    category: "consequential",
    duration_days: 30,
    influence_cost: 10,
    skill: "diplomacy",
    difficulty: 14,
    risks: ["scandal"],
    results: #{
        success: #{
            weight: 450, log: true,
            log_text: "The Consul received {leader}'s petition favourably.",
            effect_fn: "petition_success",
        },
        failure: #{ weight: 450 },
        disaster: #{
            weight: 100, log: true,
            log_text: "{leader}'s petition irritated the Consulate.",
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

define_job(#{
    id: "muster-the-levies",
    title: "Muster the Levies",
    summary: "Raise a standing army under a general, with manpower and supplies from the house pool.",
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
            log_text: "{leader} mustered an eager and well-found levy.",
            effect_fn: "muster_crit",
        },
        success: #{
            weight: 600, log: true,
            log_text: "{leader} mustered the levies.",
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
