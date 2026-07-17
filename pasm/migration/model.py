from __future__ import annotations

from dataclasses import dataclass
from enum import Enum
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from pasm.core.model import EntityId, SourceLocation


class MigrationPredicate(str, Enum):
    PATH_DOES_NOT_EXIST = "path-does-not-exist"
    SYMBOL_DOES_NOT_EXIST = "symbol-does-not-exist"
    NO_OBSERVED_IMPORTS = "no-observed-imports"
    ALL_CALLERS_ARE = "all-callers-are"
    TEST_PASSES = "test-passes"


@dataclass(frozen=True)
class RemovalCondition:
    predicate: MigrationPredicate
    subject: str
    allowed_callers: tuple["EntityId", ...] = ()
    source_location: "SourceLocation | None" = None


@dataclass(frozen=True)
class MigrationSection:
    legacy_entities: tuple["EntityId", ...] = ()
    target_entities: tuple["EntityId", ...] = ()
    approved_legacy_callers: tuple["EntityId", ...] = ()
    temporary_adapters: tuple["EntityId", ...] = ()
    legacy_symbols: tuple[str, ...] = ()
    target_symbols: tuple[str, ...] = ()
    removal_conditions: tuple[RemovalCondition, ...] = ()
