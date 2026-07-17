from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path

from .findings import Finding, FindingCategory, Severity
from .model import SourceLocation, SpecEntity
from .parser import parse_spec_file
from .references import validate_references
from pasm.architecture.validation import validate_architecture
from pasm.implementation.validation import validate_implementation
from pasm.migration.validation import validate_migrations
from pasm.domains.game_design.validation import validate_game_design
from pasm.integration.validation import validate_cross_domain


@dataclass(frozen=True)
class PasmModel:
    spec_root: Path
    entities: tuple[SpecEntity, ...]

    def entity_by_id(self, entity_id: str) -> SpecEntity | None:
        for entity in self.entities:
            if entity.id.value == entity_id:
                return entity
        return None


@dataclass(frozen=True)
class ValidationResult:
    model: PasmModel
    findings: tuple[Finding, ...]

    @property
    def ok(self) -> bool:
        return not any(finding.severity == Severity.ERROR for finding in self.findings)

    @property
    def exit_code(self) -> int:
        return 0 if self.ok else 1


def validate_spec_root(
    spec_root: Path,
    workspace_root: Path | None = None,
) -> ValidationResult:
    spec_root = spec_root.resolve()
    # Production specs live at <workspace>/pasm/spec. Fixture and external
    # spec roots can be nested differently, so callers may opt in explicitly.
    workspace_root = (
        workspace_root.resolve()
        if workspace_root is not None
        else spec_root.parent.parent.resolve()
    )
    entities: list[SpecEntity] = []
    findings: list[Finding] = []

    yaml_paths = sorted(
        path for path in spec_root.rglob("*") if path.suffix.lower() in {".yaml", ".yml"} and "scenarios" not in path.relative_to(spec_root).parts
    )

    if not yaml_paths:
        findings.append(
            Finding(
                id="spec-root-empty",
                category=FindingCategory.UNIMPLEMENTED_SPECIFICATION,
                severity=Severity.ERROR,
                confidence="confirmed",
                summary=f"No PASM YAML files were found under '{spec_root.as_posix()}'.",
                details="Point 'pasm validate' at a spec root containing one or more .yaml or .yml files.",
                rule="io.spec-root-has-yaml",
                spec_entities=(),
                implementation_locations=(SourceLocation(path=spec_root),),
                evidence=(),
                suggested_resolution="Create a spec directory with PASM YAML files or choose the correct --spec-root.",
                requires_decision=False,
            )
        )
        return ValidationResult(model=PasmModel(spec_root=spec_root, entities=()), findings=tuple(findings))

    for yaml_path in yaml_paths:
        parsed = parse_spec_file(yaml_path, spec_root)
        findings.extend(parsed.findings)
        entities.extend(parsed.entities)

    findings.extend(_validate_duplicate_entities(tuple(entities)))
    findings.extend(_validate_temporary_exceptions(tuple(entities)))
    findings.extend(validate_references(tuple(entities)))
    findings.extend(validate_architecture(tuple(entities)))
    findings.extend(validate_implementation(tuple(entities), workspace_root))
    findings.extend(validate_migrations(tuple(entities), workspace_root))
    findings.extend(validate_game_design(tuple(entities)))
    findings.extend(validate_cross_domain(tuple(entities)))

    return ValidationResult(
        model=PasmModel(spec_root=spec_root, entities=tuple(entities)),
        findings=tuple(findings),
    )


def _validate_duplicate_entities(entities: tuple[SpecEntity, ...]) -> list[Finding]:
    seen: dict[tuple[str, str], SpecEntity] = {}
    findings: list[Finding] = []
    for entity in entities:
        key = (entity.kind, entity.id.value)
        original = seen.get(key)
        if original is None:
            seen[key] = entity
            continue
        findings.append(
            Finding(
                id=f"duplicate-entity:{entity.kind}:{entity.id}",
                category=FindingCategory.CONFLICTING_INTENT,
                severity=Severity.ERROR,
                confidence="confirmed",
                summary=f"Duplicate entity declaration for '{entity.kind}: {entity.id}'.",
                details="Entity IDs must be globally unique across the loaded PASM model.",
                rule="core.entity-id-unique",
                spec_entities=(entity.id, original.id),
                implementation_locations=(original.source_location, entity.source_location),
                evidence=(),
                suggested_resolution="Merge the declarations or rename one entity to a distinct semantic ID.",
                requires_decision=False,
            )
        )
    return findings


def _validate_temporary_exceptions(entities: tuple[SpecEntity, ...]) -> list[Finding]:
    findings: list[Finding] = []
    for entity in entities:
        for index, exception in enumerate(entity.exceptions):
            if exception.temporary and not exception.removal_condition:
                findings.append(
                    Finding(
                        id=f"temporary-exception-without-removal:{entity.id}:{index}",
                        category=FindingCategory.ARCHITECTURE_RISK,
                        severity=Severity.ERROR,
                        confidence="confirmed",
                        summary=f"Temporary exception '{exception.rule}' on '{entity.id}' has no removal condition.",
                        details="Temporary exceptions must declare how the project will know when they can be removed.",
                        rule="core.temporary-exception-removal-condition",
                        spec_entities=(entity.id,),
                        implementation_locations=(exception.source_location,),
                        evidence=(),
                        suggested_resolution="Add one or more removal conditions or mark the exception non-temporary.",
                        requires_decision=False,
                    )
                )
    return findings
