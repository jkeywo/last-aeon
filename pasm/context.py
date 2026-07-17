"""Deterministic, bounded task-context bundles from PASM relationships."""
from __future__ import annotations

from dataclasses import asdict
from pathlib import Path

from pasm.core.model import SpecEntity

RELATION_FIELDS = (
    "owner", "owns", "reads", "readers", "writes", "produces", "consumes",
    "coordinates", "validates", "depends_on", "runtime_depends_on", "sends",
    "receives", "producer", "consumer", "validator", "derived_from",
)


def build_context_bundle(entities: tuple[SpecEntity, ...], seeds: tuple[str, ...], depth: int) -> dict[str, object]:
    entity_map = {entity.id.value: entity for entity in entities}
    missing = sorted(seed for seed in seeds if seed not in entity_map)
    if missing:
        raise ValueError(f"Unknown context seed entities: {', '.join(missing)}")
    included = set(seeds)
    frontier = set(seeds)
    omitted: set[str] = set()
    for _ in range(depth):
        next_frontier: set[str] = set()
        for entity_id in frontier:
            for target in _targets(entity_map[entity_id]):
                if target in entity_map and target not in included:
                    next_frontier.add(target)
        included.update(next_frontier)
        frontier = next_frontier
    for entity_id in included:
        omitted.update(target for target in _targets(entity_map[entity_id]) if target in entity_map and target not in included)
    selected = tuple(sorted((entity_map[entity_id] for entity_id in included), key=lambda entity: entity.id.value))
    files = sorted({path.as_posix() for entity in selected if entity.implementation for path in entity.implementation.paths + entity.implementation.legacy_paths + entity.implementation.target_paths})
    return {
        "schema_version": 1,
        "seeds": list(seeds),
        "dependency_depth": depth,
        "entities": [asdict(entity) for entity in selected],
        "implementation_paths": files,
        "migrations": [asdict(entity.migration) for entity in selected if entity.migration],
        "evidence": [asdict(item) for entity in selected for item in entity.evidence],
        "omitted_linked_entities": sorted(omitted),
        "limitations": "Only explicit PASM architecture links are traversed; source contents, dynamic flow, and unmodelled relationships are omitted.",
    }


def _targets(entity: SpecEntity) -> set[str]:
    if entity.architecture is None:
        return set()
    targets: set[str] = set()
    for name in RELATION_FIELDS:
        value = getattr(entity.architecture, name)
        if value is None:
            continue
        if isinstance(value, tuple):
            targets.update(item.value for item in value)
        else:
            targets.add(value.value)
    return targets
