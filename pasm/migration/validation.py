from __future__ import annotations

from pathlib import Path

from pasm.core.findings import Finding, FindingCategory, Severity
from pasm.core.model import EntityId, SourceLocation, SpecEntity
from pasm.implementation.observation import (
    ObservedImplementation,
    find_repository_symbol_references,
    observe_entity_implementation,
    observe_repository,
)
from pasm.migration.model import MigrationPredicate, MigrationSection, RemovalCondition


def validate_migrations(
    entities: tuple[SpecEntity, ...],
    workspace_root: Path,
) -> list[Finding]:
    findings: list[Finding] = []
    entity_map = {entity.id.value: entity for entity in entities}
    observations = {
        entity.id.value: observe_entity_implementation(entity, workspace_root)
        for entity in entities
        if entity.implementation is not None
    }
    inventory = observe_repository(workspace_root)

    for entity in entities:
        migration = entity.migration
        if migration is None:
            continue
        findings.extend(_validate_migration_shape(entity, migration))
        findings.extend(_validate_removal_conditions(entity, migration, workspace_root, entity_map, observations))
        findings.extend(_validate_legacy_callers(entity, migration, entity_map, observations))
        findings.extend(_report_unmodelled_legacy_references(entity, migration, entity_map, inventory))
        findings.extend(_validate_duplicate_writers(entity, migration, entity_map))
        findings.extend(_validate_target_legacy_residue(entity, migration, entity_map, observations))
    return findings


def _validate_migration_shape(entity: SpecEntity, migration: MigrationSection) -> list[Finding]:
    findings: list[Finding] = []
    if not migration.legacy_entities:
        findings.append(
            Finding(
                id=f"migration-missing-legacy-entities:{entity.id}",
                category=FindingCategory.INCOMPLETE_MIGRATION,
                severity=Severity.ERROR,
                confidence="confirmed",
                summary=f"Migration '{entity.id}' declares no legacy entities.",
                details="Phase 6 migration declarations must identify at least one legacy entity being retired or replaced.",
                rule="migration.legacy-entities-required",
                spec_entities=(entity.id,),
                implementation_locations=(entity.source_location,),
                evidence=(),
                suggested_resolution="Add one or more legacy_entities to the migration section.",
                requires_decision=False,
            )
        )
    if not migration.target_entities:
        findings.append(
            Finding(
                id=f"migration-missing-target-entities:{entity.id}",
                category=FindingCategory.INCOMPLETE_MIGRATION,
                severity=Severity.ERROR,
                confidence="confirmed",
                summary=f"Migration '{entity.id}' declares no target entities.",
                details="Phase 6 migration declarations must identify at least one target entity taking over responsibility.",
                rule="migration.target-entities-required",
                spec_entities=(entity.id,),
                implementation_locations=(entity.source_location,),
                evidence=(),
                suggested_resolution="Add one or more target_entities to the migration section.",
                requires_decision=False,
            )
        )
    return findings


def _validate_removal_conditions(
    entity: SpecEntity,
    migration: MigrationSection,
    workspace_root: Path,
    entity_map: dict[str, SpecEntity],
    observations: dict[str, ObservedImplementation],
) -> list[Finding]:
    findings: list[Finding] = []
    for index, condition in enumerate(migration.removal_conditions):
        satisfied = _evaluate_condition(condition, migration, workspace_root, entity_map, observations)
        if satisfied:
            continue
        location = condition.source_location or entity.source_location
        findings.append(
            Finding(
                id=f"migration-removal-condition-pending:{entity.id}:{index}",
                category=FindingCategory.INCOMPLETE_MIGRATION,
                severity=Severity.WARNING,
                confidence="confirmed",
                summary=f"Migration '{entity.id}' removal condition '{condition.predicate.value}' for '{condition.subject}' is not yet satisfied.",
                details="Phase 6 evaluates declared migration removal conditions against the current observed implementation surface.",
                rule="migration.removal-condition-evaluates",
                spec_entities=(entity.id,),
                implementation_locations=(location,),
                evidence=(condition.subject,),
                suggested_resolution="Complete the migration work or update the removal condition if the intended milestone changed.",
                requires_decision=False,
            )
        )
    return findings


def _evaluate_condition(
    condition: RemovalCondition,
    migration: MigrationSection,
    workspace_root: Path,
    entity_map: dict[str, SpecEntity],
    observations: dict[str, ObservedImplementation],
) -> bool:
    if condition.predicate == MigrationPredicate.PATH_DOES_NOT_EXIST:
        return not (workspace_root / condition.subject).exists()
    if condition.predicate == MigrationPredicate.SYMBOL_DOES_NOT_EXIST:
        return not any(
            observations.get(legacy_id.value, ObservedImplementation(legacy_id.value, ())).has_symbol(condition.subject)
            for legacy_id in migration.legacy_entities
        )
    if condition.predicate == MigrationPredicate.NO_OBSERVED_IMPORTS:
        return not any(
            _entity_references_symbol(observations.get(entity_id.value), condition.subject)
            for entity_id in migration.approved_legacy_callers + migration.target_entities + migration.temporary_adapters
        )
    if condition.predicate == MigrationPredicate.ALL_CALLERS_ARE:
        allowed = {caller.value for caller in condition.allowed_callers} or {
            caller.value for caller in migration.approved_legacy_callers
        }
        callers = _find_calling_entities(condition.subject, entity_map, observations, exclude=migration.legacy_entities)
        return set(callers) <= allowed
    if condition.predicate == MigrationPredicate.TEST_PASSES:
        return False
    return False


def _validate_legacy_callers(
    entity: SpecEntity,
    migration: MigrationSection,
    entity_map: dict[str, SpecEntity],
    observations: dict[str, ObservedImplementation],
) -> list[Finding]:
    findings: list[Finding] = []
    approved = {
        caller.value for caller in migration.approved_legacy_callers + migration.temporary_adapters
    }
    approved.update(legacy.value for legacy in migration.legacy_entities)
    approved.update(target.value for target in migration.target_entities)
    excluded = migration.legacy_entities

    for symbol in migration.legacy_symbols:
        call_sites = _find_calling_entities(symbol, entity_map, observations, exclude=excluded)
        for caller_id in sorted(set(call_sites) - approved):
            caller = entity_map[caller_id]
            findings.append(
                Finding(
                    id=f"undeclared-legacy-caller:{entity.id}:{caller_id}:{symbol}",
                    category=FindingCategory.INCOMPLETE_MIGRATION,
                    severity=Severity.ERROR,
                    confidence="confirmed",
                    summary=f"Migration '{entity.id}' found undeclared legacy caller '{caller_id}' for symbol '{symbol}'.",
                    details="Phase 6 checks that only approved callers continue to reference legacy migration symbols.",
                    rule="migration.legacy-callers-declared",
                    spec_entities=(entity.id, caller.id),
                    implementation_locations=call_sites[caller_id],
                    evidence=(symbol,),
                    suggested_resolution="Add the caller to approved_legacy_callers or remove its dependency on the legacy symbol.",
                    requires_decision=False,
                )
            )
    return findings


def _report_unmodelled_legacy_references(
    entity: SpecEntity,
    migration: MigrationSection,
    entity_map: dict[str, SpecEntity],
    inventory,
) -> list[Finding]:
    """Surface repository-wide lexical evidence not owned by a PASM entity."""
    declared_paths = {
        path
        for candidate in entity_map.values()
        if candidate.implementation is not None
        for path in candidate.implementation.paths + candidate.implementation.legacy_paths + candidate.implementation.target_paths
    }
    findings: list[Finding] = []
    for symbol in migration.legacy_symbols:
        locations = tuple(
            location
            for location in find_repository_symbol_references(inventory, symbol)
            if location.path not in declared_paths
        )
        if not locations:
            continue
        findings.append(
            Finding(
                id=f"unmodelled-legacy-reference:{entity.id}:{symbol}",
                category=FindingCategory.INCOMPLETE_MIGRATION,
                severity=Severity.WARNING,
                confidence="inferred",
                summary=f"Migration '{entity.id}' found repository-wide lexical references to legacy symbol '{symbol}' outside PASM-mapped files.",
                details="This bounded Phase 5/6 audit reports candidate references only; it does not prove a call, runtime reachability, actor identity, or data flow.",
                rule="migration.repository-wide-legacy-reference-candidate",
                spec_entities=(entity.id,),
                implementation_locations=locations,
                evidence=(symbol,),
                suggested_resolution="Map the relevant file to a PASM entity, confirm it is not a caller, or remove the legacy reference.",
                requires_decision=False,
            )
        )
    return findings


def _validate_duplicate_writers(
    entity: SpecEntity,
    migration: MigrationSection,
    entity_map: dict[str, SpecEntity],
) -> list[Finding]:
    findings: list[Finding] = []
    legacy_written = _written_states(migration.legacy_entities, entity_map)
    target_written = _written_states(migration.target_entities, entity_map)
    overlapping = sorted(legacy_written & target_written)
    for state_id in overlapping:
        findings.append(
            Finding(
                id=f"migration-overlapping-writers:{entity.id}:{state_id}",
                category=FindingCategory.INCOMPLETE_MIGRATION,
                severity=Severity.WARNING,
                confidence="confirmed",
                summary=f"Migration '{entity.id}' has both legacy and target entities writing '{state_id}'.",
                details="This is a deterministic Phase 6 signal that old and new implementations may both still be mutating the same authoritative state.",
                rule="migration.no-overlapping-legacy-target-writers",
                spec_entities=(entity.id,),
                implementation_locations=(entity.source_location,),
                evidence=(state_id,),
                suggested_resolution="Declare an explicit temporary adapter strategy or finish moving writes to the target implementation.",
                requires_decision=False,
            )
        )
    return findings


def _validate_target_legacy_residue(
    entity: SpecEntity,
    migration: MigrationSection,
    entity_map: dict[str, SpecEntity],
    observations: dict[str, ObservedImplementation],
) -> list[Finding]:
    findings: list[Finding] = []
    temporary = {adapter.value for adapter in migration.temporary_adapters}
    for target_id in migration.target_entities:
        if target_id.value in temporary:
            continue
        observation = observations.get(target_id.value)
        if observation is None:
            continue
        for symbol in migration.legacy_symbols:
            locations = _reference_locations(observation, symbol)
            if not locations:
                continue
            target = entity_map.get(target_id.value)
            if target is None:
                continue
            findings.append(
                Finding(
                    id=f"migration-target-still-references-legacy:{entity.id}:{target_id.value}:{symbol}",
                    category=FindingCategory.PROBABLE_VIOLATION,
                    severity=Severity.CONCERN,
                    confidence="inferred",
                    summary=f"Migration target '{target_id.value}' still references legacy symbol '{symbol}'.",
                    details="Phase 6 treats target-side references to legacy symbols as a partial-conversion heuristic unless the target is declared as a temporary adapter.",
                    rule="migration.target-legacy-residue-heuristic",
                    spec_entities=(entity.id, target.id),
                    implementation_locations=locations,
                    evidence=(symbol,),
                    suggested_resolution="Remove the target's dependency on the legacy symbol or mark it as a temporary adapter while the bridge remains necessary.",
                    requires_decision=False,
                )
            )
    return findings


def _written_states(entity_ids: tuple[EntityId, ...], entity_map: dict[str, SpecEntity]) -> set[str]:
    written: set[str] = set()
    for entity_id in entity_ids:
        entity = entity_map.get(entity_id.value)
        if entity is None or entity.architecture is None:
            continue
        written.update(state.value for state in entity.architecture.writes)
    return written


def _find_calling_entities(
    symbol: str,
    entity_map: dict[str, SpecEntity],
    observations: dict[str, ObservedImplementation],
    exclude: tuple[EntityId, ...] = (),
) -> dict[str, tuple[SourceLocation, ...]]:
    excluded = {entity_id.value for entity_id in exclude}
    callers: dict[str, tuple[SourceLocation, ...]] = {}
    for entity_id, observation in observations.items():
        if entity_id in excluded:
            continue
        entity = entity_map.get(entity_id)
        if entity is None or entity.kind == "migration":
            continue
        locations = _reference_locations(observation, symbol)
        if locations:
            callers[entity_id] = locations
    return callers


def _entity_references_symbol(observation: ObservedImplementation | None, symbol: str) -> bool:
    return bool(_reference_locations(observation, symbol))


def _reference_locations(
    observation: ObservedImplementation | None,
    symbol: str,
) -> tuple[SourceLocation, ...]:
    if observation is None:
        return ()
    locations: list[SourceLocation] = []
    for observed_file in observation.files:
        if observed_file.has_symbol(symbol):
            continue
        if observed_file.contains_text(symbol):
            locations.append(SourceLocation(path=observed_file.path))
    return tuple(locations)
