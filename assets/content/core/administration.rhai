// Core administrative jobs available to every house.
//
// Authoring notes: definitions are data via define_* calls; behaviour is
// named functions in this same file, referenced by effect_fn. Functions
// receive a read-only context map and return an array of effect maps.

define_job(#{
    id: "manage-estates",
    title: "Manage the Estates",
    summary: "Routine administration of the house's provincial holdings.",
    category: "routine",
    duration_days: 30,
    results: #{
        success: #{ weight: 850 },
        failure: #{ weight: 150 },
    },
});

define_job(#{
    id: "court-a-rival-house",
    title: "Court a Rival House",
    summary: "Send an envoy to soften a rival house's standing against you.",
    category: "consequential",
    duration_days: 45,
    results: #{
        critical_success: #{ weight: 100, popup: true, log: true },
        success: #{ weight: 500, log: true },
        failure: #{ weight: 300 },
        disaster: #{ weight: 100, popup: true, log: true, effect_fn: "courting_disaster" },
    },
});

fn courting_disaster(ctx) {
    [#{
        kind: "log",
        message: "The envoy gave insult at " + ctx.location + "; doors close across the court.",
    }]
}
