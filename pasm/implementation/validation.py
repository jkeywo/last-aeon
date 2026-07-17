from __future__ import annotations

from pathlib import Path

from pasm.core.findings import Finding, FindingCategory, Severity
from pasm.core.model import SourceLocation, SpecEntity
from pasm.implementation.observation import observe_entity_implementation, observe_repository


def validate_implementation(
    entities: tuple[SpecEntity, ...],
    workspace_root: Path,
) -> list[Finding]:
    findings: list[Finding] = []
    inventory = observe_repository(workspace_root) if _requires_repository_observation(entities) else None
    for entity in entities:
        implementation = entity.implementation
        if implementation is not None:
            findings.extend(_validate_declared_paths(entity, implementation, workspace_root))
            findings.extend(_validate_nonempty_mapping(entity, implementation))
            findings.extend(_validate_observed_mapping(entity, workspace_root))
        findings.extend(_validate_required_mapping(entity))
    if inventory is not None:
        findings.extend(_validate_observed_dependency_drift(entities, workspace_root, inventory))
    return findings


def _validate_declared_paths(entity: SpecEntity, implementation, workspace_root: Path) -> list[Finding]:
    findings: list[Finding] = []
    for path in implementation.paths + implementation.legacy_paths + implementation.target_paths:
        absolute_path = workspace_root / path
        if not absolute_path.exists():
            findings.append(
                Finding(
                    id=f"missing-implementation-path:{entity.id}:{path.as_posix()}",
                    category=FindingCategory.UNIMPLEMENTED_SPECIFICATION,
                    severity=Severity.ERROR,
                    confidence="confirmed",
                    summary=f"Entity '{entity.id}' declares missing implementation path '{path.as_posix()}'.",
                    details="Phase 4 validates authored implementation mappings against the current workspace layout.",
                    rule="implementation.path-exists",
                    spec_entities=(entity.id,),
                    implementation_locations=(
                        entity.source_location,
                        SourceLocation(path=path),
                    ),
                    evidence=(),
                    suggested_resolution="Update the PASM path mapping or restore the intended file/directory.",
                    requires_decision=False,
                )
            )
    return findings


def _validate_nonempty_mapping(entity: SpecEntity, implementation) -> list[Finding]:
    if (
        implementation.paths
        or implementation.symbols
        or implementation.messages
        or implementation.tests
        or implementation.legacy_paths
        or implementation.target_paths
    ):
        return []
    return [
        Finding(
            id=f"empty-implementation-mapping:{entity.id}",
            category=FindingCategory.UNIMPLEMENTED_SPECIFICATION,
            severity=Severity.ERROR,
            confidence="confirmed",
            summary=f"Entity '{entity.id}' has an empty implementation section.",
            details="An implementation section should declare at least one path, symbol, message, test, or migration path.",
            rule="implementation.nonempty",
            spec_entities=(entity.id,),
            implementation_locations=(entity.source_location,),
            evidence=(),
            suggested_resolution="Add declared implementation mappings or remove the empty implementation section.",
            requires_decision=False,
        )
    ]


def _validate_required_mapping(entity: SpecEntity) -> list[Finding]:
    if entity.status.value not in {"implemented", "partially-implemented"}:
        return []
    if entity.kind in {"runtime"}:
        return []
    if entity.implementation is not None:
        return []
    return [
        Finding(
            id=f"missing-implementation-mapping:{entity.id}",
            category=FindingCategory.UNIMPLEMENTED_SPECIFICATION,
            severity=Severity.WARNING,
            confidence="confirmed",
            summary=f"Implemented entity '{entity.id}' has no declared implementation mapping.",
            details="Phase 4 expects shipped or partially shipped entities to point at their intended code paths.",
            rule="implementation.mapping-required-for-shipped-entity",
            spec_entities=(entity.id,),
            implementation_locations=(entity.source_location,),
            evidence=(),
            suggested_resolution="Add an implementation section with the real paths, symbols, or tests for this entity.",
            requires_decision=False,
        )
    ]


def _validate_observed_mapping(entity: SpecEntity, workspace_root: Path) -> list[Finding]:
    implementation = entity.implementation
    if implementation is None:
        return []

    observed = observe_entity_implementation(entity, workspace_root)
    findings: list[Finding] = []

    for symbol_name in implementation.symbols:
        observed_symbol = observed.find_symbol(symbol_name)
        if observed_symbol is not None:
            continue
        findings.append(
            Finding(
                id=f"missing-observed-symbol:{entity.id}:{symbol_name}",
                category=FindingCategory.STALE_SPECIFICATION,
                severity=Severity.WARNING,
                confidence="confirmed",
                summary=f"Entity '{entity.id}' declares symbol '{symbol_name}' but the observed files do not expose it.",
                details="Phase 5 scans only the files already declared in PASM and compares their observed symbols to the declared mapping.",
                rule="implementation.declared-symbol-observed",
                spec_entities=(entity.id,),
                implementation_locations=(entity.source_location,),
                evidence=tuple(file.path.as_posix() for file in observed.files),
                suggested_resolution="Update the PASM symbol mapping or rename the implementation symbol to match.",
                requires_decision=False,
            )
        )

    for message_name in implementation.messages:
        if observed.contains_text(message_name):
            continue
        findings.append(
            Finding(
                id=f"missing-observed-message:{entity.id}:{message_name}",
                category=FindingCategory.STALE_SPECIFICATION,
                severity=Severity.WARNING,
                confidence="confirmed",
                summary=f"Entity '{entity.id}' declares message '{message_name}' but the observed files do not reference that text.",
                details="Phase 5 performs a literal text search for declared implementation messages inside the entity's declared files.",
                rule="implementation.declared-message-observed",
                spec_entities=(entity.id,),
                implementation_locations=(entity.source_location,),
                evidence=tuple(file.path.as_posix() for file in observed.files),
                suggested_resolution="Update the PASM message mapping or point the entity at the file where that message is implemented.",
                requires_decision=False,
            )
        )

    return findings


def _requires_repository_observation(entities: tuple[SpecEntity, ...]) -> bool:
    return any(
        entity.implementation is not None
        and entity.architecture is not None
        and (
            entity.architecture.depends_on
            or entity.architecture.may_depend_on
            or entity.architecture.runtime_depends_on
            or entity.architecture.build_depends_on
            or entity.architecture.optional_dependency
            or entity.architecture.temporary_dependency
            or entity.architecture.must_not_depend_on
        )
        for entity in entities
    )


def _validate_observed_dependency_drift(entities, workspace_root: Path, inventory) -> list[Finding]:
    entities_by_id = {entity.id.value: entity for entity in entities}
    files_by_entity = {
        entity.id.value: {file.path for file in observe_entity_implementation(entity, workspace_root).files}
        for entity in entities
        if entity.implementation is not None
    }
    entity_ids_by_file: dict[Path, set[str]] = {}
    for entity_id, paths in files_by_entity.items():
        for path in paths:
            entity_ids_by_file.setdefault(path, set()).add(entity_id)

    # A file can legitimately implement several PASM entities. A lightweight
    # scanner cannot assign an import to one of those entities, so only use
    # unambiguous file ownership for entity-level conformance findings.
    unambiguous_entity_by_file = {
        path: next(iter(entity_ids))
        for path, entity_ids in entity_ids_by_file.items()
        if len(entity_ids) == 1
    }

    observed_edges: dict[tuple[str, str], list] = {}
    for edge in inventory.dependencies:
        source_id = unambiguous_entity_by_file.get(edge.source)
        target_id = unambiguous_entity_by_file.get(edge.target)
        if source_id is not None and target_id is not None and source_id != target_id:
            observed_edges.setdefault((source_id, target_id), []).append(edge)

    findings: list[Finding] = []
    for entity_id, entity in entities_by_id.items():
        architecture = entity.architecture
        if architecture is None:
            continue
        required = {reference.value for reference in architecture.depends_on}
        allowed = required | {
            reference.value
            for reference in (
                architecture.may_depend_on
                + architecture.runtime_depends_on
                + architecture.build_depends_on
                + architecture.optional_dependency
                + architecture.temporary_dependency
            )
        }
        forbidden = {reference.value for reference in architecture.must_not_depend_on}
        observed_targets = {
            target_id
            for (source_id, target_id) in observed_edges
            if source_id == entity_id
        }

        for target_id in sorted(observed_targets & forbidden):
            findings.append(
                _dependency_finding(
                    "observed-forbidden-dependency",
                    FindingCategory.IMPLEMENTATION_DEFECT,
                    Severity.ERROR,
                    entity,
                    target_id,
                    observed_edges[(entity_id, target_id)],
                    "Repository observation found a direct local dependency that PASM forbids.",
                    "implementation.observed-must-not-depend",
                    "Remove the import/module edge or revise the architecture rule with an explicit decision.",
                )
            )

        for target_id in sorted(observed_targets - allowed - forbidden):
            findings.append(
                _dependency_finding(
                    "undeclared-observed-dependency",
                    FindingCategory.STALE_SPECIFICATION,
                    Severity.WARNING,
                    entity,
                    target_id,
                    observed_edges[(entity_id, target_id)],
                    "Repository observation found a direct local dependency missing from PASM.",
                    "implementation.observed-dependency-declared",
                    "Declare the dependency, classify it as an allowed exception, or remove the code edge.",
                )
            )

        for target_id in sorted(required - observed_targets):
            if not files_by_entity.get(entity_id) or not files_by_entity.get(target_id):
                continue
            findings.append(
                Finding(
                    id=f"missing-observed-dependency:{entity_id}:{target_id}",
                    category=FindingCategory.STALE_SPECIFICATION,
                    severity=Severity.WARNING,
                    confidence="inferred",
                    summary=f"Entity '{entity_id}' declares a dependency on '{target_id}' but no direct local file edge was observed.",
                    details="The Phase 5 scanner observes only direct resolvable source-file edges; indirect runtime relationships may need an explicit exception.",
                    rule="implementation.declared-dependency-observed",
                    spec_entities=(entity.id, entities_by_id[target_id].id),
                    implementation_locations=(entity.source_location, entities_by_id[target_id].source_location),
                    evidence=tuple(sorted(path.as_posix() for path in files_by_entity[entity_id])),
                    suggested_resolution="Add the missing direct dependency, classify the relationship as non-local/indirect, or update the PASM declaration.",
                    requires_decision=False,
                )
            )
    return findings


def _dependency_finding(
    prefix,
    category,
    severity,
    entity,
    target_id,
    edges,
    details,
    rule,
    suggested_resolution,
) -> Finding:
    evidence = tuple(
        sorted(
            f"{edge.source.as_posix()}:{edge.location.line} -> {edge.target.as_posix()}"
            for edge in edges
        )
    )
    return Finding(
        id=f"{prefix}:{entity.id}:{target_id}",
        category=category,
        severity=severity,
        confidence="confirmed",
        summary=f"Entity '{entity.id}' has an observed dependency on '{target_id}'.",
        details=details,
        rule=rule,
        spec_entities=(entity.id,),
        implementation_locations=tuple(edge.location for edge in edges),
        evidence=evidence,
        suggested_resolution=suggested_resolution,
        requires_decision=False,
    )
