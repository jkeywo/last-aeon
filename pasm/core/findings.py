from __future__ import annotations

from dataclasses import dataclass
from enum import Enum

from .model import EntityId, SourceLocation


class Severity(str, Enum):
    ERROR = "error"
    WARNING = "warning"
    CONCERN = "concern"
    INFORMATION = "information"


class FindingCategory(str, Enum):
    VIOLATION = "violation"
    PROBABLE_VIOLATION = "probable-violation"
    INCOMPLETE_MIGRATION = "incomplete-migration"
    STALE_SPECIFICATION = "stale-specification"
    UNMAPPED_IMPLEMENTATION = "unmapped-implementation"
    UNIMPLEMENTED_SPECIFICATION = "unimplemented-specification"
    INTENTIONAL_EXCEPTION = "intentional-exception"
    CONFLICTING_INTENT = "conflicting-intent"
    UNVERIFIED = "unverified"
    DESIGN_RISK = "design-risk"
    ARCHITECTURE_RISK = "architecture-risk"


@dataclass(frozen=True)
class Finding:
    id: str
    category: FindingCategory
    severity: Severity
    confidence: str
    summary: str
    details: str
    rule: str
    spec_entities: tuple[EntityId, ...]
    implementation_locations: tuple[SourceLocation, ...]
    evidence: tuple[str, ...]
    suggested_resolution: str | None
    requires_decision: bool
    status: str = "open"

