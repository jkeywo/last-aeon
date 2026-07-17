from __future__ import annotations

from dataclasses import dataclass


@dataclass(frozen=True)
class PlatformConstraints:
    allowed: tuple[str, ...] = ()
    forbidden: tuple[str, ...] = ()


@dataclass(frozen=True)
class ArchitectureSection:
    kind: str | None = None
    classification: str | None = None
    authority: str | None = None
    owner: EntityId | None = None
    owns: tuple[EntityId, ...] = ()
    reads: tuple[EntityId, ...] = ()
    readers: tuple[EntityId, ...] = ()
    writes: tuple[EntityId, ...] = ()
    produces: tuple[EntityId, ...] = ()
    consumes: tuple[EntityId, ...] = ()
    exposes: tuple[EntityId, ...] = ()
    receives: tuple[EntityId, ...] = ()
    transforms: tuple[EntityId, ...] = ()
    coordinates: tuple[EntityId, ...] = ()
    validates: tuple[EntityId, ...] = ()
    persists: tuple[EntityId, ...] = ()
    renders: tuple[EntityId, ...] = ()
    accepts: tuple[EntityId, ...] = ()
    sends: tuple[EntityId, ...] = ()
    depends_on: tuple[EntityId, ...] = ()
    may_depend_on: tuple[EntityId, ...] = ()
    must_not_depend_on: tuple[EntityId, ...] = ()
    runtime_depends_on: tuple[EntityId, ...] = ()
    build_depends_on: tuple[EntityId, ...] = ()
    optional_dependency: tuple[EntityId, ...] = ()
    temporary_dependency: tuple[EntityId, ...] = ()
    writers: tuple[EntityId, ...] = ()
    replicas: tuple[EntityId, ...] = ()
    derived_from: tuple[EntityId, ...] = ()
    reveal_conditions: tuple[EntityId, ...] = ()
    runs_in: tuple[EntityId, ...] = ()
    producer: tuple[EntityId, ...] = ()
    consumer: tuple[EntityId, ...] = ()
    validator: tuple[EntityId, ...] = ()
    payload: str | None = None
    version: str | None = None
    replacement: EntityId | None = None
    trust_boundary: str | None = None
    platforms: PlatformConstraints | None = None
