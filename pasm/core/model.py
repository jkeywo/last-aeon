from __future__ import annotations

from dataclasses import dataclass, field
from enum import Enum
from pathlib import Path
import re

from pasm.architecture.model import ArchitectureSection
from pasm.domains.game_design.model import GameDesignSection
from pasm.implementation.model import ImplementationSection
from pasm.migration.model import MigrationSection


ENTITY_ID_RE = re.compile(r"^[a-z0-9]+(?:-[a-z0-9]+)*$")


class Status(str, Enum):
    PROPOSED = "proposed"
    PROVISIONAL = "provisional"
    ACCEPTED = "accepted"
    PARTIALLY_IMPLEMENTED = "partially-implemented"
    IMPLEMENTED = "implemented"
    DEPRECATED = "deprecated"
    REMOVED = "removed"
    REJECTED = "rejected"


class Confidence(str, Enum):
    CONFIRMED = "confirmed"
    INFERRED = "inferred"
    PROVISIONAL = "provisional"
    DISPUTED = "disputed"
    UNKNOWN = "unknown"


class EvidenceKind(str, Enum):
    TEST = "test"
    MANUAL_REVIEW = "manual-review"
    PLAYTEST = "playtest"
    RUNTIME_OBSERVATION = "runtime-observation"
    TELEMETRY = "telemetry"
    DECISION_RECORD = "decision-record"
    BENCHMARK = "benchmark"
    OTHER = "other"


@dataclass(frozen=True, order=True)
class EntityId:
    value: str

    def __post_init__(self) -> None:
        if not self.value:
            raise ValueError("Entity IDs must be non-empty.")
        if not ENTITY_ID_RE.fullmatch(self.value):
            raise ValueError(
                "Entity IDs must be lowercase kebab-case using only a-z, 0-9, and '-'."
            )

    def __str__(self) -> str:
        return self.value


@dataclass(frozen=True)
class SourceLocation:
    path: Path
    line: int | None = None
    column: int | None = None
    section: tuple[str, ...] = ()

    def with_section(self, *parts: str) -> "SourceLocation":
        return SourceLocation(
            path=self.path,
            line=self.line,
            column=self.column,
            section=self.section + tuple(parts),
        )

    def render(self) -> str:
        position = []
        if self.line is not None:
            position.append(str(self.line))
        if self.column is not None:
            position.append(str(self.column))
        suffix = ""
        if position:
            suffix = ":" + ":".join(position)
        if self.section:
            suffix += " [" + ".".join(self.section) + "]"
        return f"{self.path.as_posix()}{suffix}"


@dataclass(frozen=True)
class Reference:
    target: EntityId
    source_location: SourceLocation


@dataclass(frozen=True)
class ExceptionSpec:
    rule: str
    scope: tuple[str, ...]
    rationale: str
    temporary: bool
    removal_condition: tuple[str, ...]
    approval_status: str | None
    source_location: SourceLocation


@dataclass(frozen=True)
class EvidenceItem:
    kind: EvidenceKind
    reference: str | None
    summary: str | None
    source_location: SourceLocation


@dataclass(frozen=True)
class SpecEntity:
    id: EntityId
    kind: str
    status: Status
    confidence: Confidence
    title: str | None
    summary: str | None
    goals: tuple[str, ...] = ()
    rationale: tuple[str, ...] = ()
    tags: tuple[str, ...] = ()
    references: tuple[Reference, ...] = ()
    assumptions: tuple[str, ...] = ()
    open_questions: tuple[str, ...] = ()
    supersedes: tuple[EntityId, ...] = ()
    conflicts_with: tuple[EntityId, ...] = ()
    exceptions: tuple[ExceptionSpec, ...] = ()
    evidence: tuple[EvidenceItem, ...] = ()
    architecture: ArchitectureSection | None = None
    game_design: GameDesignSection | None = None
    implementation: ImplementationSection | None = None
    migration: MigrationSection | None = None
    domain_sections: dict[str, object] = field(default_factory=dict)
    source_location: SourceLocation = field(
        default_factory=lambda: SourceLocation(path=Path("<unknown>"))
    )
