// The Ashkarr Succession - the hand-authored MVP scenario.
//
// The planet's paramount recently died without an accepted successor;
// three great houses (Veyrin, Draksha, Meloch) hold five planetary
// provinces apiece and none can yet claim the vacant Paramountcy. Eight
// vassal houses (the player's House Harrow among them), two independents,
// and the Sanctora Imperim on the Aurelian Spire complete the field.
// The player begins as a Veyrin vassal with a credible path to grow,
// press a claim, or break for independence.

define_scenario(#{
    id: "ashkarr-succession",
    start_year: 411, start_month: 1, start_day: 1,
    player_house: "harrow",
});

define_name_pool(#{
    id: "ashkarri-names",
    male: [
        "Kaelen", "Dziemir", "Ostrev", "Vandric", "Tolvan", "Maruk",
        "Chalko", "Ilyar", "Sorven", "Radomil", "Ashter", "Kirov",
        "Vael", "Perrin", "Casold", "Emeric",
    ],
    female: [
        "Veska", "Irenna", "Maritsa", "Odella", "Tsarina", "Halvenna",
        "Zorya", "Kessarin", "Ludmila", "Ashkara", "Nerezza", "Sable",
        "Yelena", "Mirsa", "Doriane", "Valka",
    ],
});

// --- Traits: four opposed pairs ---
define_trait(#{ id: "bold",
    opinion_same: 10, opinion_opposed: 10, opposites: ["wary"] });
define_trait(#{ id: "wary",
    opinion_same: 5, opinion_opposed: 10, opposites: ["bold"] });
define_trait(#{ id: "forthright",
    opinion_same: 15, opinion_opposed: 20, opposites: ["scheming"] });
define_trait(#{ id: "scheming",
    opinion_same: 5, opinion_opposed: 20, opposites: ["forthright"] });
define_trait(#{ id: "magnanimous",
    opinion_same: 15, opinion_opposed: 15, opposites: ["grasping"] });
define_trait(#{ id: "grasping",
    opinion_same: 0, opinion_opposed: 15, opposites: ["magnanimous"] });
define_trait(#{ id: "devout",
    opinion_same: 15, opinion_opposed: 15, opposites: ["profane"] });
define_trait(#{ id: "profane",
    opinion_same: 5, opinion_opposed: 15, opposites: ["devout"] });

// --- Great houses (contest the planet) ---
define_house(#{
    id: "veyrin",
    tier: "great", head: "casimir-veyrin", color: [178, 34, 52],
    provinces: ["redwater", "sable-steppe", "consulate-fields", "vantar-deep", "emberwash"],
    wealth: 320, manpower: 3200, supplies: 520, legitimacy: 58,
});
define_house(#{
    id: "draksha",
    tier: "great", head: "zorka-draksha", color: [46, 92, 158],
    provinces: ["karvessa", "veyrmark", "sallow-march", "thassik", "greyspire"],
    wealth: 300, manpower: 3000, supplies: 500, legitimacy: 55,
});
define_house(#{
    id: "meloch",
    tier: "great", head: "avram-meloch", color: [58, 122, 62],
    provinces: ["koszevny-flats", "old-anchorage", "pellamir", "virek-sound", "shattered-coast"],
    wealth: 310, manpower: 2900, supplies: 540, legitimacy: 56,
});

// --- Vassal houses (the player leads House Harrow) ---
define_house(#{
    id: "harrow",
    tier: "vassal", liege: "veyrin", head: "edrun-harrow", color: [188, 148, 60],
    provinces: ["tolmaz", "ostragard", "vhorruk", "mournhollow"],
    wealth: 160, manpower: 1400, supplies: 240, legitimacy: 48,
});
define_house(#{
    id: "vantar",
    tier: "vassal", liege: "veyrin", head: "perrin-vantar", color: [120, 150, 170],
    provinces: ["cindral", "ulmgorn"],
    wealth: 110, manpower: 900, supplies: 180, legitimacy: 40,
});
define_house(#{
    id: "koszev",
    tier: "vassal", liege: "draksha", head: "milena-koszev", color: [150, 120, 90],
    provinces: ["dreghal", "palensk"],
    wealth: 90, manpower: 850, supplies: 150, legitimacy: 38,
});
define_house(#{
    id: "rusk",
    tier: "vassal", liege: "draksha", head: "tomasz-rusk", color: [90, 110, 120],
    provinces: ["brakkenveld", "charhold"],
    wealth: 70, manpower: 700, supplies: 120, legitimacy: 34,
});
define_house(#{
    id: "emberlyn",
    tier: "vassal", liege: "meloch", head: "sorel-emberlyn", color: [200, 110, 60],
    provinces: ["rustholt", "calders-folly"],
    wealth: 120, manpower: 950, supplies: 200, legitimacy: 42,
});
define_house(#{
    id: "skarn",
    tier: "vassal", liege: "meloch", head: "vael-skarn", color: [100, 100, 110],
    provinces: ["hyperions-rest", "halveth"],
    wealth: 100, manpower: 900, supplies: 190, legitimacy: 40,
});
define_house(#{
    id: "okonn",
    tier: "vassal", liege: "draksha", head: "petra-okonn", color: [122, 82, 148],
    provinces: ["port-vesk", "tithe-pits", "stillmere"],
    wealth: 130, manpower: 1000, supplies: 210, legitimacy: 44,
});
define_house(#{
    id: "calder",
    tier: "vassal", liege: "veyrin", head: "ludo-calder", color: [80, 90, 100],
    provinces: ["regolith-verge", "silent-quarter"],
    wealth: 60, manpower: 600, supplies: 110, legitimacy: 32,
});

// --- Independent houses ---
define_house(#{
    id: "pell",
    tier: "independent", head: "casold-pell", color: [150, 150, 60],
    provinces: ["tsarovka", "drossmark", "skarnos"],
    wealth: 140, manpower: 1200, supplies: 230, legitimacy: 46,
});
define_house(#{
    id: "szel",
    tier: "independent", head: "janos-szel", color: [96, 108, 116],
    provinces: ["cryon-basin", "lanternfall", "hollow-crown"],
    wealth: 110, manpower: 1000, supplies: 200, legitimacy: 42,
});

// --- The Sanctora Imperim, on the Aurelian Spire ---
define_organisation(#{
    id: "sanctora-imperim",
    kind: "sanctora-imperim", head: "consul-aurelia-veth",
    color: [212, 175, 55],
    provinces: ["spire-decks"],
    wealth: 260, manpower: 1500, supplies: 360, legitimacy: 70,
});

// --- Titles and offices ---
define_title(#{
    id: "paramountcy-of-ashkarr",
    kind: "paramount", body: "ashkarr",
    // Vacant and contested: the last paramount died without an heir, and
    // no great house yet holds more of the planet than its rivals.
});
define_title(#{
    id: "consul-of-the-sector",
    kind: "consul", holder_character: "consul-aurelia-veth",
});
define_office(#{
    id: "starbase-commander",
    organisation: "sanctora-imperim", province: "spire-decks",
    holder: "commander-dray",
});

// --- Characters ---
define_character(#{
    id: "casimir-veyrin", gender: "male",
    birth_year: 366, organisation: "veyrin",
    spouse: "irenna-veyrin", traits: ["bold", "grasping"],
    skills: #{ command: 12, diplomacy: 8, intrigue: 6, stewardship: 9 },
});
define_character(#{
    id: "irenna-veyrin", gender: "female",
    birth_year: 370, organisation: "veyrin",
    traits: ["scheming", "devout"],
    skills: #{ command: 3, diplomacy: 11, intrigue: 13, stewardship: 7 },
});
define_character(#{
    id: "aldric-veyrin", gender: "male",
    birth_year: 392, organisation: "veyrin",
    parents: ["casimir-veyrin", "irenna-veyrin"], traits: ["bold", "forthright"],
    skills: #{ command: 9, diplomacy: 5, intrigue: 3, stewardship: 4 },
});
define_character(#{
    id: "nerezza-veyrin", gender: "female",
    birth_year: 396, organisation: "veyrin",
    parents: ["casimir-veyrin", "irenna-veyrin"], traits: ["scheming"],
    skills: #{ command: 4, diplomacy: 7, intrigue: 9, stewardship: 5 },
});
define_character(#{
    id: "zorka-draksha", gender: "female",
    birth_year: 371, organisation: "draksha",
    spouse: "maruk-draksha", traits: ["wary", "scheming"],
    skills: #{ command: 7, diplomacy: 10, intrigue: 14, stewardship: 8 },
});
define_character(#{
    id: "maruk-draksha", gender: "male",
    birth_year: 368, organisation: "draksha",
    traits: ["bold", "profane"],
    skills: #{ command: 13, diplomacy: 4, intrigue: 5, stewardship: 3 },
});
define_character(#{
    id: "vessa-draksha", gender: "female",
    birth_year: 396, organisation: "draksha",
    parents: ["zorka-draksha", "maruk-draksha"], traits: ["wary"],
    skills: #{ command: 4, diplomacy: 6, intrigue: 8, stewardship: 5 },
});
define_character(#{
    id: "avram-meloch", gender: "male",
    birth_year: 359, organisation: "meloch",
    spouse: "doriane-meloch", traits: ["magnanimous", "devout", "wary"],
    skills: #{ command: 6, diplomacy: 12, intrigue: 4, stewardship: 13 },
});
define_character(#{
    id: "doriane-meloch", gender: "female",
    birth_year: 363, organisation: "meloch",
    traits: ["magnanimous", "devout"],
    skills: #{ command: 2, diplomacy: 10, intrigue: 5, stewardship: 11 },
});
define_character(#{
    id: "tomasz-meloch", gender: "male",
    birth_year: 388, organisation: "meloch",
    parents: ["avram-meloch", "doriane-meloch"], traits: ["forthright", "bold"],
    skills: #{ command: 8, diplomacy: 7, intrigue: 2, stewardship: 6 },
});
define_character(#{
    id: "edrun-harrow", gender: "male",
    birth_year: 375, organisation: "harrow",
    parents: ["torvald-harrow"],
    spouse: "kessarin-harrow", traits: ["forthright", "magnanimous"],
    skills: #{ command: 7, diplomacy: 9, intrigue: 5, stewardship: 10 },
});
define_character(#{
    id: "kessarin-harrow", gender: "female",
    birth_year: 379, organisation: "harrow",
    traits: ["wary", "devout"],
    skills: #{ command: 2, diplomacy: 8, intrigue: 9, stewardship: 11 },
});
define_character(#{
    id: "senna-harrow", gender: "female",
    birth_year: 399, organisation: "harrow",
    parents: ["edrun-harrow", "kessarin-harrow"], traits: ["bold"],
    skills: #{ command: 5, diplomacy: 4, intrigue: 4, stewardship: 3 },
});
define_character(#{
    id: "brant-harrow", gender: "male",
    birth_year: 402, organisation: "harrow",
    parents: ["edrun-harrow", "kessarin-harrow"], traits: ["forthright"],
    skills: #{ command: 3, diplomacy: 3, intrigue: 2, stewardship: 5 },
});
define_character(#{
    id: "aleyn-harrow", gender: "female",
    birth_year: 378, organisation: "harrow",
    parents: ["torvald-harrow"], spouse: "reyn-harrow",
    traits: ["forthright", "bold"],
    skills: #{ command: 9, diplomacy: 6, intrigue: 4, stewardship: 6 },
});
define_character(#{
    id: "reyn-harrow", gender: "male",
    birth_year: 376, organisation: "harrow",
    spouse: "aleyn-harrow", traits: ["wary"],
    skills: #{ command: 5, diplomacy: 10, intrigue: 6, stewardship: 7 },
});
define_character(#{
    id: "mikael-harrow", gender: "male",
    birth_year: 386, organisation: "harrow",
    traits: ["scheming"],
    skills: #{ command: 4, diplomacy: 7, intrigue: 12, stewardship: 5 },
});
define_character(#{
    id: "torvald-harrow", gender: "male",
    birth_year: 348, organisation: "harrow",
    traits: ["devout", "magnanimous"],
    skills: #{ command: 4, diplomacy: 9, intrigue: 5, stewardship: 13 },
});
define_character(#{
    id: "captain-lira", gender: "female",
    birth_year: 383, organisation: "harrow",
    traits: ["bold"],
    skills: #{ command: 11, diplomacy: 4, intrigue: 5, stewardship: 4 },
});
define_character(#{
    id: "perrin-vantar", gender: "male",
    birth_year: 377, organisation: "vantar",
    spouse: "valka-vantar", traits: ["wary", "grasping"],
    skills: #{ command: 6, diplomacy: 6, intrigue: 7, stewardship: 8 },
});
define_character(#{
    id: "valka-vantar", gender: "female",
    birth_year: 381, organisation: "vantar",
    traits: ["scheming"],
    skills: #{ command: 3, diplomacy: 7, intrigue: 9, stewardship: 6 },
});
define_character(#{
    id: "milena-koszev", gender: "female",
    birth_year: 373, organisation: "koszev",
    traits: ["grasping", "wary"],
    skills: #{ command: 5, diplomacy: 7, intrigue: 10, stewardship: 12 },
});
define_character(#{
    id: "emeric-koszev", gender: "male",
    birth_year: 397, organisation: "koszev",
    parents: ["milena-koszev"], traits: ["scheming"],
    skills: #{ command: 4, diplomacy: 5, intrigue: 8, stewardship: 4 },
});
define_character(#{
    id: "tomasz-rusk", gender: "male",
    birth_year: 380, organisation: "rusk",
    spouse: "mirsa-rusk", traits: ["bold", "profane"],
    skills: #{ command: 9, diplomacy: 4, intrigue: 5, stewardship: 6 },
});
define_character(#{
    id: "mirsa-rusk", gender: "female",
    birth_year: 384, organisation: "rusk",
    traits: ["forthright"],
    skills: #{ command: 3, diplomacy: 7, intrigue: 5, stewardship: 8 },
});
define_character(#{
    id: "sorel-emberlyn", gender: "male",
    birth_year: 372, organisation: "emberlyn",
    spouse: "yelena-emberlyn", traits: ["magnanimous", "forthright"],
    skills: #{ command: 7, diplomacy: 10, intrigue: 4, stewardship: 9 },
});
define_character(#{
    id: "yelena-emberlyn", gender: "female",
    birth_year: 376, organisation: "emberlyn",
    traits: ["devout"],
    skills: #{ command: 2, diplomacy: 9, intrigue: 6, stewardship: 10 },
});
define_character(#{
    id: "vael-skarn", gender: "male",
    birth_year: 369, organisation: "skarn",
    traits: ["bold", "grasping"],
    skills: #{ command: 10, diplomacy: 5, intrigue: 6, stewardship: 7 },
});
define_character(#{
    id: "petra-okonn", gender: "female",
    birth_year: 373, organisation: "okonn",
    traits: ["grasping", "wary"],
    skills: #{ command: 5, diplomacy: 7, intrigue: 10, stewardship: 12 },
});
define_character(#{
    id: "milo-okonn", gender: "male",
    birth_year: 398, organisation: "okonn",
    parents: ["petra-okonn"], traits: ["scheming"],
    skills: #{ command: 4, diplomacy: 5, intrigue: 9, stewardship: 4 },
});
define_character(#{
    id: "ludo-calder", gender: "male",
    birth_year: 382, organisation: "calder",
    traits: ["forthright", "magnanimous"],
    skills: #{ command: 6, diplomacy: 6, intrigue: 4, stewardship: 7 },
});
define_character(#{
    id: "casold-pell", gender: "male",
    birth_year: 364, organisation: "pell",
    spouse: "sable-pell", traits: ["magnanimous", "bold"],
    skills: #{ command: 9, diplomacy: 8, intrigue: 5, stewardship: 10 },
});
define_character(#{
    id: "sable-pell", gender: "female",
    birth_year: 368, organisation: "pell",
    traits: ["wary", "devout"],
    skills: #{ command: 3, diplomacy: 9, intrigue: 7, stewardship: 9 },
});
define_character(#{
    id: "vandric-pell", gender: "male",
    birth_year: 394, organisation: "pell",
    parents: ["casold-pell", "sable-pell"], traits: ["bold"],
    skills: #{ command: 7, diplomacy: 5, intrigue: 4, stewardship: 5 },
});
define_character(#{
    id: "janos-szel", gender: "male",
    birth_year: 381, organisation: "szel",
    traits: ["bold", "profane"],
    skills: #{ command: 10, diplomacy: 3, intrigue: 7, stewardship: 5 },
});
define_character(#{
    id: "halvenna-szel", gender: "female",
    birth_year: 385, organisation: "szel",
    traits: ["scheming", "grasping"],
    skills: #{ command: 4, diplomacy: 6, intrigue: 9, stewardship: 6 },
});
define_character(#{
    id: "consul-aurelia-veth", gender: "female",
    birth_year: 362, organisation: "sanctora-imperim",
    traits: ["devout", "scheming", "magnanimous"],
    skills: #{ command: 4, diplomacy: 14, intrigue: 11, stewardship: 12 },
});
define_character(#{
    id: "commander-dray", gender: "male",
    birth_year: 377, organisation: "sanctora-imperim",
    traits: ["forthright", "wary"],
    skills: #{ command: 11, diplomacy: 5, intrigue: 6, stewardship: 8 },
});
define_character(#{
    id: "adept-solen", gender: "female",
    birth_year: 385, organisation: "sanctora-imperim",
    traits: ["devout", "forthright"],
    skills: #{ command: 2, diplomacy: 9, intrigue: 7, stewardship: 10 },
});
define_character(#{
    id: "prefect-halden", gender: "male",
    birth_year: 379, organisation: "sanctora-imperim",
    traits: ["wary", "devout"],
    skills: #{ command: 8, diplomacy: 6, intrigue: 8, stewardship: 9 },
});
define_character(#{
    id: "captain-vashk", gender: "male",
    birth_year: 378, organisation: "veyrin",
    traits: ["bold"],
    skills: #{ command: 10, diplomacy: 4, intrigue: 3, stewardship: 6 },
});
define_character(#{
    id: "captain-thessaly", gender: "female",
    birth_year: 382, organisation: "sanctora-imperim",
    traits: ["wary", "devout"],
    skills: #{ command: 12, diplomacy: 5, intrigue: 4, stewardship: 7 },
});
define_character(#{
    id: "captain-drozny", gender: "male",
    birth_year: 376, organisation: "draksha",
    traits: ["bold", "grasping"],
    skills: #{ command: 11, diplomacy: 4, intrigue: 5, stewardship: 5 },
});

// --- Ships (capital ships carry simulated captains) ---

// One more adult in each peer house, so every player of this map has a
// comparable bench and autonomous houses have someone to send.
define_character(#{
    id: "iven-vantar", gender: "male",
    birth_year: 384, organisation: "vantar", traits: ["wary"],
    skills: #{ command: 7, diplomacy: 5, intrigue: 6, stewardship: 6 },
});
define_character(#{
    id: "sella-calder", gender: "female",
    birth_year: 382, organisation: "calder", traits: ["bold"],
    skills: #{ command: 8, diplomacy: 6, intrigue: 4, stewardship: 5 },
});
define_character(#{
    id: "juri-koszev", gender: "male",
    birth_year: 380, organisation: "koszev", traits: ["grasping"],
    skills: #{ command: 6, diplomacy: 7, intrigue: 8, stewardship: 6 },
});
define_character(#{
    id: "nadia-okonn", gender: "female",
    birth_year: 381, organisation: "okonn", traits: ["scheming"],
    skills: #{ command: 5, diplomacy: 8, intrigue: 9, stewardship: 6 },
});
define_character(#{
    id: "captain-okonn-marek", gender: "male",
    birth_year: 379, organisation: "okonn", traits: ["forthright"],
    skills: #{ command: 10, diplomacy: 4, intrigue: 5, stewardship: 5 },
});
define_character(#{
    id: "halden-rusk", gender: "male",
    birth_year: 383, organisation: "rusk", traits: ["forthright"],
    skills: #{ command: 7, diplomacy: 6, intrigue: 5, stewardship: 7 },
});
define_character(#{
    id: "orsa-emberlyn", gender: "female",
    birth_year: 385, organisation: "emberlyn", traits: ["devout"],
    skills: #{ command: 5, diplomacy: 9, intrigue: 5, stewardship: 8 },
});
define_character(#{
    id: "brik-skarn", gender: "male",
    birth_year: 379, organisation: "skarn", traits: ["wary", "grasping"],
    skills: #{ command: 8, diplomacy: 5, intrigue: 7, stewardship: 5 },
});
define_character(#{
    id: "captain-sanctora-imre", gender: "female",
    birth_year: 380, organisation: "sanctora-imperim", traits: ["devout"],
    skills: #{ command: 9, diplomacy: 6, intrigue: 4, stewardship: 6 },
});

define_ship(#{
    id: "ember-crown", class: "capital",
    owner: "veyrin", captain: "captain-vashk", location: "redwater",
});
define_ship(#{
    id: "aurelias-writ", class: "capital",
    owner: "sanctora-imperim", captain: "captain-thessaly", location: "spire-decks",
});
define_ship(#{
    id: "draksha-lance", class: "capital",
    owner: "draksha", captain: "captain-drozny", location: "karvessa",
});
define_ship(#{
    id: "redwater-runner", class: "transport",
    owner: "harrow", captain: "captain-lira", location: "tolmaz",
});
define_ship(#{
    id: "karvess-hauler", class: "transport",
    owner: "veyrin", location: "karvessa",
});
define_ship(#{
    id: "pale-lantern", class: "patrol",
    owner: "okonn", captain: "captain-okonn-marek", location: "port-vesk",
});
define_ship(#{
    id: "spire-picket", class: "patrol",
    owner: "sanctora-imperim", captain: "captain-sanctora-imre", location: "spire-decks",
});

// --- Starting armies (every faction fields a standing force) ---
define_army(#{
    id: "veyrin-first",
    owner: "veyrin", general: "casimir-veyrin", province: "redwater",
    manpower: 1200, supplies: 260,
});
define_army(#{
    id: "veyrin-second",
    owner: "veyrin", general: "aldric-veyrin", province: "consulate-fields",
    manpower: 800, supplies: 170,
});
define_army(#{
    id: "draksha-first",
    owner: "draksha", general: "zorka-draksha", province: "karvessa",
    manpower: 1100, supplies: 240,
});
define_army(#{
    id: "draksha-second",
    owner: "draksha", general: "maruk-draksha", province: "veyrmark",
    manpower: 900, supplies: 190,
});
define_army(#{
    id: "meloch-first",
    owner: "meloch", general: "avram-meloch", province: "koszevny-flats",
    manpower: 1000, supplies: 220,
});
define_army(#{
    id: "meloch-second",
    owner: "meloch", general: "tomasz-meloch", province: "old-anchorage",
    manpower: 800, supplies: 170,
});
define_army(#{
    id: "harrow-guard",
    owner: "harrow", general: "edrun-harrow", province: "ostragard",
    manpower: 600, supplies: 140,
});
define_army(#{
    id: "vantar-levy",
    owner: "vantar", general: "perrin-vantar", province: "cindral",
    manpower: 450, supplies: 100,
});
define_army(#{
    id: "koszev-levy",
    owner: "koszev", general: "milena-koszev", province: "dreghal",
    manpower: 450, supplies: 100,
});
define_army(#{
    id: "rusk-levy",
    owner: "rusk", general: "tomasz-rusk", province: "brakkenveld",
    manpower: 400, supplies: 90,
});
define_army(#{
    id: "emberlyn-levy",
    owner: "emberlyn", general: "sorel-emberlyn", province: "rustholt",
    manpower: 500, supplies: 110,
});
define_army(#{
    id: "skarn-levy",
    owner: "skarn", general: "vael-skarn", province: "hyperions-rest",
    manpower: 500, supplies: 110,
});
define_army(#{
    id: "okonn-levy",
    owner: "okonn", general: "petra-okonn", province: "port-vesk",
    manpower: 500, supplies: 110,
});
define_army(#{
    id: "calder-levy",
    owner: "calder", general: "ludo-calder", province: "regolith-verge",
    manpower: 350, supplies: 80,
});
define_army(#{
    id: "pell-host",
    owner: "pell", general: "casold-pell", province: "tsarovka",
    manpower: 650, supplies: 150,
});
define_army(#{
    id: "szel-reavers",
    owner: "szel", general: "janos-szel", province: "cryon-basin",
    manpower: 550, supplies: 120,
});
define_army(#{
    id: "spire-garrison",
    owner: "sanctora-imperim", general: "commander-dray", province: "spire-decks",
    manpower: 700, supplies: 180,
});

// ---------------------------------------------------------------------------
// Standing obligations
//
// The succession does not begin from nothing: the houses arrive owing and
// resenting one another. These are political facts, separate from opinion —
// a house may loathe its creditor and still be bound.
// ---------------------------------------------------------------------------

define_obligation(#{
    id: "harrow-owes-veyrin",
    kind: "favour", debtor: "harrow", creditor: "veyrin", weight: 30,
    origin: "Veyrin gold paid the Harrow levies through the last dearth",
});
define_obligation(#{
    id: "veyrin-owes-calder",
    kind: "favour", debtor: "veyrin", creditor: "calder", weight: 25,
    origin: "Calder ships carried the Veyrin household through the blockade of 402",
});
define_obligation(#{
    id: "draksha-promise-koszev",
    kind: "promise", debtor: "draksha", creditor: "koszev", weight: 35, days: 1440,
    origin: "Draksha swore Koszev a seat at the Consulate table",
});
define_obligation(#{
    id: "meloch-promise-emberlyn",
    kind: "promise", debtor: "meloch", creditor: "emberlyn", weight: 30, days: 1080,
    origin: "Meloch promised Emberlyn the revenues of the Ashen Reach",
});
define_obligation(#{
    id: "rusk-resents-draksha",
    kind: "grievance", debtor: "draksha", creditor: "rusk", weight: 40,
    origin: "Draksha hanged two Rusk cousins after the Thassik affair",
});
define_obligation(#{
    id: "skarn-resents-meloch",
    kind: "grievance", debtor: "meloch", creditor: "skarn", weight: 30,
    origin: "Meloch took Ulmgorn from Skarn hands within living memory",
});
define_obligation(#{
    id: "pell-resents-veyrin",
    kind: "grievance", debtor: "veyrin", creditor: "pell", weight: 35,
    origin: "Veyrin barred Pell from the Consular lists for a generation",
});
define_obligation(#{
    id: "szel-owes-sanctora",
    kind: "favour", debtor: "szel", creditor: "sanctora-imperim", weight: 20,
    origin: "The Sanctora forgave the Szel tithe arrears of 407",
});
define_obligation(#{
    id: "okonn-resents-vantar",
    kind: "grievance", debtor: "vantar", creditor: "okonn", weight: 25,
    origin: "A Vantar marriage suit to Okonn was broken off in insult",
});
