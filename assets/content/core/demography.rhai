// Demography: how often characters die, marry, and bear children.
//
// These are tuning values, so they are authored here rather than compiled
// into the simulation. They are set so a campaign's population holds
// roughly steady — about 1.2x over a century — while dying often enough
// that succession, heirs and the death-driven arcs still fire.
//
// The numbers are a deliberate middle. Realistic infant mortality alone,
// or a fertility trim alone, each land near steady; applying both
// aggressively collapses the world to about a third of its people over a
// century, which empties the political board.

define_demography(#{
    id: "standard",

    // Yearly chance an unmarried adult in an organisation seeks a match.
    marriage_permille: 300,

    // Childbearing runs from the age of majority (a simulation rule) to
    // this age inclusive. Each child already born lowers the yearly
    // chance by the step, never below the floor.
    fertile_to_age: 45,
    birth_base_permille: 230,
    birth_step_permille: 42,
    birth_floor_permille: 40,

    // Yearly chance of death, by inclusive upper age. Bands ascend; the
    // first infant band carries the pre-modern childhood toll that keeps
    // the population from compounding.
    mortality: [
        #{ through_age: 4, permille: 60 },
        #{ through_age: 17, permille: 6 },
        #{ through_age: 49, permille: 4 },
        #{ through_age: 59, permille: 12 },
        #{ through_age: 69, permille: 25 },
        #{ through_age: 79, permille: 70 },
        #{ through_age: 89, permille: 160 },
    ],
    mortality_beyond_permille: 350,
});
