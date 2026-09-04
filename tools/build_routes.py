"""Generate Last Aeon's committed initial route graph.

The runtime never invokes this file. It turns the authored province coordinates
into the surface network which is then reviewed and committed as content.

Surface adjacency is not a matter of taste: the map draws each province as the
Voronoi cell of its coordinate, so two provinces share a border exactly when
their cells do. That set is the spherical Delaunay triangulation, which for
points on a sphere is the convex hull of their unit vectors. Requires numpy and
scipy, which only a maintainer regenerating this file needs.
"""

from __future__ import annotations

import math
import re
from pathlib import Path

import numpy as np
from scipy.spatial import ConvexHull


ROOT = Path(__file__).resolve().parents[1]
PROVINCES = ROOT / "assets/content/system/provinces.rhai"
OUTPUT = ROOT / "assets/content/system/routes.rhai"
# One block per province. Reading the fields separately keeps an optional line
# such as `starport: true` from hiding a province from the network entirely.
BLOCK = re.compile(r"define_province\(#\{(.*?)\}\);", re.S)
FIELDS = {
    name: re.compile(pattern)
    for name, pattern in (
        ("key", r'id: "([^"]+)"'),
        ("body", r'body: "([^"]+)"'),
        ("latitude", r"latitude_mdeg: (-?\d+)"),
        ("longitude", r"longitude_mdeg: (-?\d+)"),
    )
}


def unit_vectors(provinces: list[tuple[str, str, int, int]]) -> np.ndarray:
    """Province coordinates as points on the unit sphere."""
    latitude = np.radians(np.array([p[2] for p in provinces]) / 1000.0)
    longitude = np.radians(np.array([p[3] for p in provinces]) / 1000.0)
    return np.stack(
        [
            np.cos(latitude) * np.cos(longitude),
            np.cos(latitude) * np.sin(longitude),
            np.sin(latitude),
        ],
        axis=1,
    )


def surface_edges(provinces: list[tuple[str, str, int, int]]) -> set[tuple[int, int]]:
    """Every pair of provinces whose drawn cells share a border.

    The convex hull of points on a sphere is their Delaunay triangulation, and
    Delaunay is the dual of the Voronoi partition the map paints: an edge here
    is a border a player can see and walk across. Fewer than four points cannot
    form a hull, and a lone province has no neighbour to reach.
    """
    if len(provinces) < 4:
        return {
            (i, j)
            for i in range(len(provinces))
            for j in range(i + 1, len(provinces))
        }
    edges: set[tuple[int, int]] = set()
    for simplex in ConvexHull(unit_vectors(provinces)).simplices:
        for i in range(3):
            edges.add(tuple(sorted((int(simplex[i]), int(simplex[(i + 1) % 3])))))
    return edges


def main() -> None:
    provinces = []
    for block in BLOCK.findall(PROVINCES.read_text(encoding="utf-8")):
        found = {name: pattern.search(block) for name, pattern in FIELDS.items()}
        if not all(found.values()):
            continue
        provinces.append(
            (
                found["key"].group(1),
                found["body"].group(1),
                int(found["latitude"].group(1)),
                int(found["longitude"].group(1)),
            )
        )
    lines = [
        "// Generated from the authored province coordinates by",
        "// tools/build_routes.py. Surface routes are every pair of provinces",
        "// whose drawn Voronoi cells share a border; space routes join the",
        "// starports. Runtime pathfinding uses only these committed edges.",
        "",
    ]
    for body in sorted({province[1] for province in provinces}):
        body_provinces = [province for province in provinces if province[1] == body]
        lines.append(f"// --- {body}: surface routes ---")
        for i, j in sorted(
            surface_edges(body_provinces),
            key=lambda pair: (body_provinces[pair[0]][0], body_provinces[pair[1]][0]),
        ):
            a, b = sorted((body_provinces[i][0], body_provinces[j][0]))
            lines.append(
                f'define_route(#{{ id: "surface-{a}-{b}", kind: "surface", '
                f'a: "{a}", b: "{b}", travel_days: 1 }});'
            )
        lines.append("")
    starports = {
        "karvessa": "ashkarr",
        "old-anchorage": "ashkarr",
        "redwater": "ashkarr",
        "tolmaz": "ashkarr",
        "port-vesk": "vesk",
        "spire-decks": "aurelian-spire",
    }
    interbody_days = {
        frozenset(("ashkarr", "aurelian-spire")): 2,
        frozenset(("ashkarr", "vesk")): 7,
        frozenset(("aurelian-spire", "vesk")): 6,
    }
    lines.append("// --- Complete local-system starport graph ---")
    keys = sorted(starports)
    for i, a in enumerate(keys):
        for b in keys[i + 1 :]:
            days = 2 if starports[a] == starports[b] else interbody_days[frozenset((starports[a], starports[b]))]
            lines.append(
                f'define_route(#{{ id: "space-{a}-{b}", kind: "space", '
                f'a: "{a}", b: "{b}", travel_days: {days} }});'
            )
    OUTPUT.write_text("\n".join(lines) + "\n", encoding="utf-8")


if __name__ == "__main__":
    main()
