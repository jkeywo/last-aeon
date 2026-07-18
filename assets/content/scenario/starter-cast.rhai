// Starter political cast for the Ashkarr system.
//
// A reduced but complete political setup so every system runs against
// real data: three great houses, two vassals (House Harrow is the
// player), one independent, and the Sanctora Imperim on the Spire.
// The authored-scenario milestone expands this to the full fourteen
// organisations and complete province allocation.

define_scenario(#{
    id: "ashkarr-succession",
    name: "The Ashkarr Succession",
    start_year: 411, start_month: 1, start_day: 1,
    player_house: "harrow",
});

define_name_pool(#{
    id: "ashkarri-names",
    male: [
        "Kaelen", "Dziemir", "Ostrev", "Vandric", "Tolvan", "Maruk",
        "Chalko", "Ilyar", "Sorven", "Radomil", "Ashter", "Kirov",
    ],
    female: [
        "Veska", "Irenna", "Maritsa", "Odella", "Tsarina", "Halvenna",
        "Zorya", "Kessarin", "Ludmila", "Ashkara", "Nerezza", "Sable",
    ],
});

// --- Traits: four opposed pairs ---

define_trait(#{
    id: "bold", name: "Bold", summary: "Meets danger head on.",
    opinion_same: 10, opinion_opposed: 10, opposites: ["wary"],
});
define_trait(#{
    id: "wary", name: "Wary", summary: "Trusts slowly and watches exits.",
    opinion_same: 5, opinion_opposed: 10, opposites: ["bold"],
});
define_trait(#{
    id: "forthright", name: "Forthright", summary: "Says what they mean.",
    opinion_same: 15, opinion_opposed: 20, opposites: ["scheming"],
});
define_trait(#{
    id: "scheming", name: "Scheming", summary: "Every word is a move.",
    opinion_same: 5, opinion_opposed: 20, opposites: ["forthright"],
});
define_trait(#{
    id: "magnanimous", name: "Magnanimous", summary: "Generous with victory.",
    opinion_same: 15, opinion_opposed: 15, opposites: ["grasping"],
});
define_trait(#{
    id: "grasping", name: "Grasping", summary: "Never enough.",
    opinion_same: 0, opinion_opposed: 15, opposites: ["magnanimous"],
});
define_trait(#{
    id: "devout", name: "Devout", summary: "Keeps the Tsarist observances.",
    opinion_same: 15, opinion_opposed: 15, opposites: ["profane"],
});
define_trait(#{
    id: "profane", name: "Profane", summary: "Honours nothing above appetite.",
    opinion_same: 5, opinion_opposed: 15, opposites: ["devout"],
});

// --- Great houses ---

define_house(#{
    id: "veyrin", name: "House Veyrin", surname: "Veyrin",
    tier: "great", head: "casimir-veyrin", color: [178, 34, 52],
    provinces: ["karvessa", "dreghal", "tolmaz", "veyrmark", "cindral", "ostragard"],
});
define_house(#{
    id: "draksha", name: "House Draksha", surname: "Draksha",
    tier: "great", head: "zorka-draksha", color: [46, 92, 158],
    provinces: ["palensk", "vhorruk", "sallow-march", "brakkenveld", "mournhollow"],
});
define_house(#{
    id: "meloch", name: "House Meloch", surname: "Meloch",
    tier: "great", head: "avram-meloch", color: [58, 122, 62],
    provinces: ["thassik", "ulmgorn", "redwater", "charhold", "sable-steppe"],
});

// --- Vassals and independents ---

define_house(#{
    id: "harrow", name: "House Harrow", surname: "Harrow",
    tier: "vassal", liege: "veyrin", head: "edrun-harrow", color: [188, 148, 60],
    provinces: ["hyperions-rest", "tsarovka", "consulate-fields"],
});
define_house(#{
    id: "okonn", name: "House Okonn", surname: "Okonn",
    tier: "vassal", liege: "draksha", head: "petra-okonn", color: [122, 82, 148],
    provinces: ["port-vesk", "tithe-pits", "cryon-basin"],
});
define_house(#{
    id: "szel", name: "House Szel", surname: "Szel",
    tier: "independent", head: "janos-szel", color: [96, 108, 116],
    provinces: ["stillmere", "lanternfall"],
});

// --- The Sanctora Imperim ---

define_organisation(#{
    id: "sanctora-imperim", name: "The Sanctora Imperim",
    kind: "sanctora-imperim", head: "consul-aurelia-veth",
    color: [212, 175, 55],
    provinces: ["spire-decks"],
});

define_title(#{
    id: "paramountcy-of-ashkarr", name: "Paramount of Ashkarr",
    kind: "paramount", body: "ashkarr",
    // Vacant: the previous paramount died without an accepted successor.
});

define_title(#{
    id: "consul-of-the-sector", name: "Consul of the Sector",
    kind: "consul", holder_character: "consul-aurelia-veth",
});

define_office(#{
    id: "starbase-commander", name: "Commander of the Aurelian Spire",
    organisation: "sanctora-imperim", province: "spire-decks",
    holder: "commander-dray",
});

// --- Characters ---
// House Veyrin
define_character(#{
    id: "casimir-veyrin", name: "Casimir Veyrin", gender: "male",
    birth_year: 366, birth_month: 3, birth_day: 12, organisation: "veyrin",
    spouse: "irenna-veyrin", traits: ["bold", "grasping"],
    skills: #{ command: 12, diplomacy: 8, intrigue: 6, stewardship: 9 },
});
define_character(#{
    id: "irenna-veyrin", name: "Irenna Veyrin", gender: "female",
    birth_year: 370, birth_month: 7, birth_day: 4, organisation: "veyrin",
    traits: ["scheming", "devout"],
    skills: #{ command: 3, diplomacy: 11, intrigue: 13, stewardship: 7 },
});
define_character(#{
    id: "aldric-veyrin", name: "Aldric Veyrin", gender: "male",
    birth_year: 392, birth_month: 1, birth_day: 20, organisation: "veyrin",
    parents: ["casimir-veyrin", "irenna-veyrin"], traits: ["bold", "forthright"],
    skills: #{ command: 9, diplomacy: 5, intrigue: 3, stewardship: 4 },
});

// House Draksha
define_character(#{
    id: "zorka-draksha", name: "Zorka Draksha", gender: "female",
    birth_year: 371, birth_month: 11, birth_day: 2, organisation: "draksha",
    spouse: "maruk-draksha", traits: ["wary", "scheming"],
    skills: #{ command: 7, diplomacy: 10, intrigue: 14, stewardship: 8 },
});
define_character(#{
    id: "maruk-draksha", name: "Maruk Draksha", gender: "male",
    birth_year: 368, birth_month: 5, birth_day: 27, organisation: "draksha",
    traits: ["bold", "profane"],
    skills: #{ command: 13, diplomacy: 4, intrigue: 5, stewardship: 3 },
});
define_character(#{
    id: "vessa-draksha", name: "Vessa Draksha", gender: "female",
    birth_year: 396, birth_month: 9, birth_day: 15, organisation: "draksha",
    parents: ["zorka-draksha", "maruk-draksha"], traits: ["wary"],
    skills: #{ command: 4, diplomacy: 6, intrigue: 8, stewardship: 5 },
});

// House Meloch
define_character(#{
    id: "avram-meloch", name: "Avram Meloch", gender: "male",
    birth_year: 359, birth_month: 2, birth_day: 8, organisation: "meloch",
    traits: ["magnanimous", "devout", "wary"],
    skills: #{ command: 6, diplomacy: 12, intrigue: 4, stewardship: 13 },
});
define_character(#{
    id: "tomasz-meloch", name: "Tomasz Meloch", gender: "male",
    birth_year: 388, birth_month: 6, birth_day: 30, organisation: "meloch",
    parents: ["avram-meloch"], traits: ["forthright", "bold"],
    skills: #{ command: 8, diplomacy: 7, intrigue: 2, stewardship: 6 },
});

// House Harrow — the player's house
define_character(#{
    id: "edrun-harrow", name: "Edrun Harrow", gender: "male",
    birth_year: 375, birth_month: 4, birth_day: 18, organisation: "harrow",
    spouse: "kessarin-harrow", traits: ["forthright", "magnanimous"],
    skills: #{ command: 7, diplomacy: 9, intrigue: 5, stewardship: 10 },
});
define_character(#{
    id: "kessarin-harrow", name: "Kessarin Harrow", gender: "female",
    birth_year: 379, birth_month: 10, birth_day: 9, organisation: "harrow",
    traits: ["wary", "devout"],
    skills: #{ command: 2, diplomacy: 8, intrigue: 9, stewardship: 11 },
});
define_character(#{
    id: "senna-harrow", name: "Senna Harrow", gender: "female",
    birth_year: 399, birth_month: 12, birth_day: 3, organisation: "harrow",
    parents: ["edrun-harrow", "kessarin-harrow"], traits: ["bold"],
    skills: #{ command: 5, diplomacy: 4, intrigue: 4, stewardship: 3 },
});
define_character(#{
    id: "brant-harrow", name: "Brant Harrow", gender: "male",
    birth_year: 402, birth_month: 8, birth_day: 22, organisation: "harrow",
    parents: ["edrun-harrow", "kessarin-harrow"], traits: ["forthright"],
    skills: #{ command: 3, diplomacy: 3, intrigue: 2, stewardship: 5 },
});

// House Okonn
define_character(#{
    id: "petra-okonn", name: "Petra Okonn", gender: "female",
    birth_year: 373, birth_month: 1, birth_day: 25, organisation: "okonn",
    traits: ["grasping", "wary"],
    skills: #{ command: 5, diplomacy: 7, intrigue: 10, stewardship: 12 },
});
define_character(#{
    id: "milo-okonn", name: "Milo Okonn", gender: "male",
    birth_year: 398, birth_month: 3, birth_day: 11, organisation: "okonn",
    parents: ["petra-okonn"], traits: ["scheming"],
    skills: #{ command: 4, diplomacy: 5, intrigue: 9, stewardship: 4 },
});

// House Szel
define_character(#{
    id: "janos-szel", name: "Janos Szel", gender: "male",
    birth_year: 381, birth_month: 7, birth_day: 7, organisation: "szel",
    traits: ["bold", "profane"],
    skills: #{ command: 10, diplomacy: 3, intrigue: 7, stewardship: 5 },
});

// The Sanctora Imperim
define_character(#{
    id: "consul-aurelia-veth", name: "Consul Aurelia Veth", gender: "female",
    birth_year: 362, birth_month: 9, birth_day: 1, organisation: "sanctora-imperim",
    traits: ["devout", "scheming", "magnanimous"],
    skills: #{ command: 4, diplomacy: 14, intrigue: 11, stewardship: 12 },
});
define_character(#{
    id: "commander-dray", name: "Commander Ilex Dray", gender: "male",
    birth_year: 377, birth_month: 12, birth_day: 14, organisation: "sanctora-imperim",
    traits: ["forthright", "wary"],
    skills: #{ command: 11, diplomacy: 5, intrigue: 6, stewardship: 8 },
});
define_character(#{
    id: "adept-solen", name: "Adept Marva Solen", gender: "female",
    birth_year: 385, birth_month: 5, birth_day: 19, organisation: "sanctora-imperim",
    traits: ["devout", "forthright"],
    skills: #{ command: 2, diplomacy: 9, intrigue: 7, stewardship: 10 },
});
