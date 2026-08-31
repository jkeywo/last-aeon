"""Generate Last Aeon's committed initial route graph.

The runtime never invokes this file. It turns the authored province coordinates
into a stable bootstrap network which is then reviewed and committed as content.
"""

from __future__ import annotations

import math
import re
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
PROVINCES = ROOT / "assets/content/system/provinces.rhai"
OUTPUT = ROOT / "assets/content/system/routes.rhai"
PATTERN = re.compile(
    r'id: "([^"]+)", body: "([^"]+)",\s*'
    r'latitude_mdeg: (-?\d+), longitude_mdeg: (-?\d+)'
)


def angular_distance(a: tuple[str, str, int, int], b: tuple[str, str, int, int]) -> float:
    lat_a, lat_b = math.radians(a[2] / 1000), math.radians(b[2] / 1000)
    longitude = math.radians((a[3] - b[3]) / 1000)
    cosine = math.sin(lat_a) * math.sin(lat_b) + math.cos(lat_a) * math.cos(lat_b) * math.cos(longitude)
    return math.acos(max(-1.0, min(1.0, cosine)))


def surface_edges(provinces: list[tuple[str, str, int, int]]) -> set[tuple[int, int]]:
    distances = {
        (i, j): angular_distance(a, b)
        for i, a in enumerate(provinces)
        for j, b in enumerate(provinces[i + 1 :], i + 1)
    }
    edges: set[tuple[int, int]] = set()
    seen = {0}
    while len(seen) < len(provinces):
        _, i, j = min(
            (distance, i, j)
            for (i, j), distance in distances.items()
            if (i in seen) != (j in seen)
        )
        edges.add(tuple(sorted((i, j))))
        seen.update((i, j))
    for i in range(len(provinces)):
        neighbours = sorted(
            (
                (distance, j if i == k else k)
                for (k, j), distance in distances.items()
                if i in (k, j)
            ),
            key=lambda item: (item[0], provinces[item[1]][0]),
        )[:3]
        edges.update(tuple(sorted((i, j))) for _, j in neighbours)
    return edges


def main() -> None:
    provinces = [
        (key, body, int(latitude), int(longitude))
        for key, body, latitude, longitude in PATTERN.findall(PROVINCES.read_text(encoding="utf-8"))
    ]
    lines = [
        "// Generated once from authored coordinates by tools/build_routes.py.",
        "// Runtime pathfinding uses only these committed explicit edges.",
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
