from __future__ import annotations

from .findings import Finding, FindingCategory, Severity
from .model import SpecEntity


def validate_references(entities: tuple[SpecEntity, ...]) -> list[Finding]:
    known_ids = {entity.id for entity in entities}
    findings: list[Finding] = []

    for entity in entities:
        for reference in entity.references:
            if reference.target not in known_ids:
                findings.append(
                    Finding(
                        id=f"unknown-reference:{entity.id}:{reference.target}",
                        category=FindingCategory.VIOLATION,
                        severity=Severity.ERROR,
                        confidence="confirmed",
                        summary=f"Entity '{entity.id}' references unknown entity '{reference.target}'.",
                        details="Every core reference must resolve after all PASM files are loaded.",
                        rule="core.references-target-exists",
                        spec_entities=(entity.id,),
                        implementation_locations=(reference.source_location,),
                        evidence=(),
                        suggested_resolution="Create the referenced entity or remove the reference.",
                        requires_decision=False,
                    )
                )

        for related in entity.supersedes + entity.conflicts_with:
            if related not in known_ids:
                findings.append(
                    Finding(
                        id=f"unknown-related-entity:{entity.id}:{related}",
                        category=FindingCategory.VIOLATION,
                        severity=Severity.ERROR,
                        confidence="confirmed",
                        summary=f"Entity '{entity.id}' points at unknown entity '{related}'.",
                        details="Related-entity links must resolve across the loaded PASM model.",
                        rule="core.related-entity-exists",
                        spec_entities=(entity.id,),
                        implementation_locations=(entity.source_location,),
                        evidence=(),
                        suggested_resolution="Create the related entity or remove the unresolved link.",
                        requires_decision=False,
                    )
                )

    return findings

