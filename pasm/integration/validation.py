from __future__ import annotations

from pasm.core.findings import Finding, FindingCategory, Severity
from pasm.core.model import EntityId, SpecEntity


def validate_cross_domain(entities: tuple[SpecEntity, ...]) -> list[Finding]:
    index = {entity.id: entity for entity in entities}
    findings = []
    findings.extend(_validate_link_targets(entities, index))
    findings.extend(_validate_shipped_architecture_mappings(entities, index))
    findings.extend(_validate_role_action_conformance(entities, index))
    findings.extend(_validate_information_enforcement(entities, index))
    findings.extend(_validate_protected_decision_enforcement(entities, index))
    return findings


def _validate_link_targets(entities, index):
    findings = []
    for entity in entities:
        design = entity.game_design
        if design is None:
            continue
        for field_name, links in (("architecture_links", design.architecture_links), ("enforcement_links", design.enforcement_links)):
            for target in links:
                linked = index.get(target)
                if linked is None:
                    findings.append(_finding(
                        f"unknown-cross-domain-link:{entity.id}:{target}", entity,
                        f"Design entity '{entity.id}' links to unknown architecture entity '{target}'.",
                        "Cross-domain links must resolve to another entity in the loaded PASM model.",
                        "cross-domain.link-target-exists", "Create the target entity or remove the unresolved link.",
                        _location(entity, field_name),
                    ))
                elif linked.architecture is None:
                    findings.append(_finding(
                        f"cross-domain-link-not-architecture:{entity.id}:{target}", entity,
                        f"Design entity '{entity.id}' links to non-architecture entity '{target}'.",
                        "Phase 8 links must identify architecture entities, not another design declaration.",
                        "cross-domain.link-target-is-architecture", "Link to an entity with an architecture section.",
                        _location(entity, field_name),
                    ))
    return findings


def _validate_shipped_architecture_mappings(entities, index):
    findings = []
    for entity in entities:
        design = entity.game_design
        if design is None:
            continue
        for field_name, links in (("architecture_links", design.architecture_links), ("enforcement_links", design.enforcement_links)):
            for target in links:
                linked = index.get(target)
                if linked is None or linked.status.value not in {"implemented", "partially-implemented"} or linked.implementation is not None:
                    continue
                findings.append(_finding(
                    f"design-link-missing-implementation:{entity.id}:{target}", entity,
                    f"Design entity '{entity.id}' links to shipped architecture entity '{target}' without an implementation mapping.",
                    "Phase 8 requires implemented and partially implemented architecture reached from design intent to retain an implementation mapping.",
                    "cross-domain.architecture-link-implementation", "Add an implementation mapping to the architecture entity or correct its lifecycle status.",
                    _location(entity, field_name),
                ))
    return findings


def _validate_role_action_conformance(entities, index):
    findings = []
    for entity in entities:
        design = entity.game_design
        if entity.kind not in {"verb", "action"} or design is None:
            continue
        if not design.architecture_links:
            findings.append(_finding(
                f"role-action-missing-architecture-link:{entity.id}", entity,
                f"Player verb '{entity.id}' has no architecture link.",
                "A player verb must identify the command or interface path that implements it.",
                "cross-domain.role-action-architecture-link-required", "Add 'game_design.architecture_links'.",
                _location(entity, "architecture_links"),
            ))
            continue
        owner = index.get(design.owner_role) if design.owner_role else None
        if owner is None or owner.game_design is None:
            continue
        role_links = set(owner.game_design.architecture_links)
        if role_links and role_links.isdisjoint(design.architecture_links):
            findings.append(_finding(
                f"role-action-no-shared-architecture:{entity.id}:{owner.id}", entity,
                f"Player verb '{entity.id}' does not share an architecture link with owner role '{owner.id}'.",
                "The role and its verb should meet at a declared interface or command path.",
                "cross-domain.role-action-shared-architecture", "Link the role and verb to their shared interface or command entity.",
                _location(entity, "architecture_links"),
            ))
    return findings


def _validate_information_enforcement(entities, index):
    findings = []
    restricted = {"role-visible", "team-visible", "hidden", "partially-known", "delayed", "uncertain"}
    for entity in entities:
        design = entity.game_design
        if entity.kind not in {"information", "information_set"} or design is None or design.visibility is None:
            continue
        if design.visibility.value not in restricted:
            continue
        if not design.enforcement_links:
            findings.append(_finding(
                f"information-missing-enforcement-link:{entity.id}", entity,
                f"Restricted information '{entity.id}' has no architecture enforcement link.",
                "Phase 8 requires restricted information to name the publisher, interface, or state boundary that enforces it.",
                "cross-domain.information-enforcement-required", "Add 'game_design.enforcement_links'.",
                _location(entity, "enforcement_links"),
            ))
    return findings


def _validate_protected_decision_enforcement(entities, index):
    findings = []
    for entity in entities:
        design = entity.game_design
        if entity.kind not in {"decision", "action"} or design is None or not design.protected:
            continue
        authoritative = any(
            index[target].architecture is not None and index[target].architecture.authority == "authoritative"
            for target in design.enforcement_links if target in index
        )
        if authoritative:
            continue
        findings.append(_finding(
            f"protected-decision-missing-authoritative-enforcement:{entity.id}", entity,
            f"Protected decision '{entity.id}' has no authoritative enforcement link.",
            "A protected decision must name an authoritative router or subsystem that prevents bypass.",
            "cross-domain.protected-decision-authoritative-enforcement", "Add an authoritative entity to 'game_design.enforcement_links'.",
            _location(entity, "enforcement_links"),
        ))
    return findings


def _location(entity, *names):
    for name in names:
        location = entity.game_design.field_locations.get(name)
        if location is not None:
            return location
    return entity.source_location


def _finding(identifier, entity, summary, details, rule, resolution, location):
    return Finding(
        id=identifier,
        category=FindingCategory.VIOLATION,
        severity=Severity.ERROR,
        confidence="confirmed",
        summary=summary,
        details=details,
        rule=rule,
        spec_entities=(entity.id,),
        implementation_locations=(location,),
        evidence=(),
        suggested_resolution=resolution,
        requires_decision=False,
    )
