// The local system's three bodies: the world, its moon, and the
// Sanctora-held orbital starbase.
//
// Radii are presentation-scale kilometres; orbits are megametres and
// days. The primary (Ashkarr) has no parent and no orbit.

define_body(#{
    id: "ashkarr",
    name: "Ashkarr",
    kind: "planet",
    radius_km: 6400,
});

define_body(#{
    id: "vesk",
    name: "Vesk",
    kind: "moon",
    parent: "ashkarr",
    radius_km: 1700,
    orbit_radius_mm: 384,
    orbit_days: 27,
});

define_body(#{
    id: "aurelian-spire",
    name: "The Aurelian Spire",
    kind: "starbase",
    parent: "ashkarr",
    radius_km: 60,
    orbit_radius_mm: 42,
    orbit_days: 3,
});
