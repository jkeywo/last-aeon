// The goods economy: the commodities worlds make and want, the
// buildings that change what a place makes, and the work of raising one.

define_good(#{ id: "grain", value: 3 });
define_good(#{ id: "ore", value: 5 });

// A granary lifts a province's grain: the answer to a hungry world that
// a house can build rather than ship for.
define_building(#{
    id: "granary",
    build_days: 120,
    wealth_cost: 60,
    supplies_cost: 20,
    adds_wealth: 3,
    produces: #{ grain: 12 },
});

// Raising a granary is stewardship over a season, aimed at one of the
// house's own provinces; success stands the building.
define_assignment(#{
    id: "raise-a-granary",
    category: "consequential",
    duration_days: 120,
    skill: "stewardship",
    difficulty: 8,
    wealth_cost: 60,
    supplies_cost: 20,
    target: "province",
    ai_available: false,
    requires: #{ target_holder: "own" },
    results: #{
        success: #{ weight: 700, log: true, effect_fn: "granary_built" },
        failure: #{ weight: 300 },
    },
});
fn granary_built(ctx) {
    [#{ kind: "construct", building: "granary" }]
}
