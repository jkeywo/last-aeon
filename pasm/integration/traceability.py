from __future__ import annotations

from dataclasses import dataclass

from pasm.core.model import EntityId, SpecEntity


@dataclass(frozen=True)
class TraceabilityRow:
    design_entity: EntityId
    design_kind: str
    architecture_links: tuple[EntityId, ...]
    enforcement_links: tuple[EntityId, ...]
    implementation_paths: tuple[str, ...]
    implementation_status: str


def build_traceability_rows(entities: tuple[SpecEntity, ...]) -> tuple[TraceabilityRow, ...]:
    index = {entity.id: entity for entity in entities}
    rows = []
    for entity in entities:
        design = entity.game_design
        if design is None:
            continue
        linked_ids = design.architecture_links + design.enforcement_links
        linked_entities = [index[link] for link in linked_ids if link in index]
        mappings = [
            linked.implementation
            for linked in linked_entities
            if linked.implementation is not None
        ]
        paths = tuple(
            path.as_posix()
            for mapping in mappings
            for path in mapping.paths
        )
        statuses = {mapping.status.value if mapping.status is not None else "unspecified" for mapping in mappings}
        status = "declared-design-only" if not mappings else statuses.pop() if len(statuses) == 1 else "mixed"
        rows.append(
            TraceabilityRow(
                design_entity=entity.id,
                design_kind=entity.kind,
                architecture_links=design.architecture_links,
                enforcement_links=design.enforcement_links,
                implementation_paths=paths,
                implementation_status=status,
            )
        )
    return tuple(rows)
