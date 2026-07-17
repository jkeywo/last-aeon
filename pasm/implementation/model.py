from __future__ import annotations

from dataclasses import dataclass
from enum import Enum
from pathlib import Path


class MappingStatus(str, Enum):
    DECLARED = "declared"
    OBSERVED = "observed"
    CONFIRMED = "confirmed"
    SUSPECTED = "suspected"
    STALE = "stale"
    REMOVED = "removed"


@dataclass(frozen=True)
class ImplementationSection:
    paths: tuple[Path, ...] = ()
    symbols: tuple[str, ...] = ()
    messages: tuple[str, ...] = ()
    tests: tuple[str, ...] = ()
    status: MappingStatus | None = None
    legacy_paths: tuple[Path, ...] = ()
    target_paths: tuple[Path, ...] = ()
