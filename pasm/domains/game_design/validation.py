from __future__ import annotations

from pasm.core.findings import Finding, FindingCategory, Severity
from pasm.core.model import EntityId, SpecEntity


def validate_game_design(entities: tuple[SpecEntity, ...]) -> list[Finding]:
    findings: list[Finding] = []
    index = {entity.id: entity for entity in entities}
    findings.extend(_validate_references(entities, index))
    findings.extend(_validate_verb_owners(entities, index))
    findings.extend(_validate_protected_decisions(entities, index))
    findings.extend(_validate_information_visibility(entities, index))
    findings.extend(_validate_resources(entities))
    findings.extend(_validate_failures(entities))
    findings.extend(_validate_tuning_and_playtest_claims(entities))
    return findings


def _validate_references(entities, index):
    findings = []
    for entity in entities:
        design = entity.game_design
        if design is None:
            continue
        for field_name, target in _design_refs(design):
            if target in index:
                continue
            findings.append(_finding(
                f"unknown-game-design-reference:{entity.id}:{target}", entity,
                f"Entity '{entity.id}' references unknown game-design entity '{target}'.",
                "Game-design relationships must resolve after all PASM files are loaded.",
                "game-design.reference-target-exists",
                "Create the referenced entity or remove the unresolved design link.",
                _location(entity, field_name),
            ))
    return findings


def _validate_verb_owners(entities, index):
    findings = []
    for entity in entities:
        if entity.kind not in {"verb", "action"} or entity.game_design is None:
            continue
        owner = entity.game_design.owner_role
        if owner is None:
            findings.append(_finding(
                f"player-verb-missing-owner:{entity.id}", entity,
                f"Player verb '{entity.id}' has no owner role.",
                "Phase 7 requires every player verb to name the role that may perform it.",
                "game-design.player-verb-owner-required",
                "Add 'game_design.owner_role' referencing a role entity.", _location(entity, "owner_role", "player_role"),
            ))
        elif owner in index and not _is_role(index[owner]):
            findings.append(_finding(
                f"player-verb-owner-not-role:{entity.id}:{owner}", entity,
                f"Player verb '{entity.id}' names non-role entity '{owner}' as owner.",
                "Verb ownership must be assigned to a role entity.",
                "game-design.player-verb-owner-is-role",
                "Point 'owner_role' at an entity declared with kind 'role'.", _location(entity, "owner_role", "player_role"),
            ))
    return findings


def _validate_protected_decisions(entities, index):
    findings = []
    for entity in entities:
        if entity.kind not in {"decision", "action"} or entity.game_design is None or not entity.game_design.protected:
            continue
        owner = entity.game_design.owner_role
        if owner is None:
            findings.append(_finding(
                f"protected-decision-missing-owner:{entity.id}", entity,
                f"Protected decision '{entity.id}' has no owner role.",
                "Protected decisions must identify the role whose decision may not be bypassed.",
                "game-design.protected-decision-owner-required",
                "Add 'game_design.owner_role' referencing the responsible role.", _location(entity, "owner_role", "player_role"),
            ))
        elif owner in index and not _is_role(index[owner]):
            findings.append(_finding(
                f"protected-decision-owner-not-role:{entity.id}:{owner}", entity,
                f"Protected decision '{entity.id}' names non-role entity '{owner}' as owner.",
                "Protected decision ownership must be assigned to a role entity.",
                "game-design.protected-decision-owner-is-role",
                "Point 'owner_role' at an entity declared with kind 'role'.", _location(entity, "owner_role", "player_role"),
            ))
        if not entity.game_design.must_not_be:
            findings.append(_finding(
                f"protected-decision-missing-bypass-policy:{entity.id}", entity,
                f"Protected decision '{entity.id}' has no prohibited bypass declaration.",
                "A protected decision should state what must not resolve or commit it instead of its owner.",
                "game-design.protected-decision-bypass-policy-required",
                "Add one or more 'game_design.must_not_be' values.", _location(entity, "protected"),
            ))
    return findings


def _validate_information_visibility(entities, index):
    findings = []
    restricted = {"hidden", "partially-known", "delayed", "uncertain"}
    for entity in entities:
        if entity.kind not in {"information", "information_set"} or entity.game_design is None:
            continue
        design = entity.game_design
        if design.visibility is None:
            findings.append(_finding(
                f"information-missing-visibility:{entity.id}", entity,
                f"Information set '{entity.id}' has no visibility classification.",
                "Information entities must declare their visibility in the Phase 7 model.",
                "game-design.information-visibility-required",
                "Add 'game_design.visibility'.", _location(entity, "visibility"),
            ))
            continue
        if design.visibility.value in restricted and not design.reveal_conditions:
            findings.append(_finding(
                f"hidden-information-missing-reveal-condition:{entity.id}", entity,
                f"Restricted information '{entity.id}' has no reveal condition.",
                "Hidden, partially-known, delayed, and uncertain information must declare how it becomes available.",
                "game-design.hidden-information-reveal-condition-required",
                "Add one or more 'game_design.reveal_conditions'.", _location(entity, "visibility"),
            ))
        for viewer in design.permitted_viewers:
            if viewer in index and not _is_role(index[viewer]):
                findings.append(_finding(
                    f"information-viewer-not-role:{entity.id}:{viewer}", entity,
                    f"Information set '{entity.id}' permits non-role entity '{viewer}' to view it.",
                    "Permitted viewers in the design model must be role entities.",
                    "game-design.information-viewer-is-role",
                    "Point 'permitted_viewers' at entities declared with kind 'role'.", _location(entity, "permitted_viewers"),
                ))
    return findings


def _validate_resources(entities):
    findings = []
    for entity in entities:
        if entity.kind != "resource" or entity.game_design is None:
            continue
        design = entity.game_design
        if not design.sources:
            findings.append(_finding(
                f"resource-missing-source:{entity.id}", entity,
                f"Resource '{entity.id}' has no source.",
                "Resources in the Phase 7 model require an origin.",
                "game-design.resource-source-required",
                "Add one or more 'game_design.sources'.", _location(entity, "sources"),
            ))
        if not design.sinks:
            findings.append(_finding(
                f"resource-missing-sink:{entity.id}", entity,
                f"Resource '{entity.id}' has no sink.",
                "Resources in the Phase 7 model require at least one consumer.",
                "game-design.resource-sink-required",
                "Add one or more 'game_design.sinks'.", _location(entity, "sinks"),
            ))
    return findings


def _validate_failures(entities):
    findings = []
    for entity in entities:
        if entity.kind not in {"failure", "failure_state"} or entity.game_design is None:
            continue
        design = entity.game_design
        if not design.consequences:
            findings.append(_finding(
                f"failure-missing-consequence:{entity.id}", entity,
                f"Failure state '{entity.id}' has no consequence.",
                "Failure states must state their gameplay consequence.",
                "game-design.failure-consequence-required",
                "Add one or more 'game_design.consequences'.", _location(entity, "consequences"),
            ))
        if design.terminal is False and not design.recovery_paths:
            findings.append(_finding(
                f"nonterminal-failure-missing-recovery:{entity.id}", entity,
                f"Non-terminal failure state '{entity.id}' has no recovery path.",
                "Non-terminal failures must describe how players can recover.",
                "game-design.nonterminal-failure-recovery-required",
                "Add one or more 'game_design.recovery_paths'.", _location(entity, "recovery_paths"),
            ))
    return findings


def _validate_tuning_and_playtest_claims(entities):
    findings = []
    for entity in entities:
        if entity.game_design is None:
            continue
        design = entity.game_design
        if entity.kind == "tuning":
            for field_name, summary, details, resolution in (
                ("affected_mechanics", f"Tuning '{entity.id}' has no affected mechanic.", "Tuning declarations must name the mechanic they tune.", "Add 'game_design.affected_mechanics'."),
                ("intended_directional_effect", f"Tuning '{entity.id}' has no intended directional effect.", "Tuning declarations must state how changing the value should affect play.", "Add 'game_design.intended_directional_effect'."),
                ("bounds", f"Tuning '{entity.id}' has no bounds.", "Tuning declarations must state their permitted or intended range.", "Add 'game_design.bounds'."),
                ("maturity", f"Tuning '{entity.id}' has no maturity declaration.", "Tuning declarations must distinguish tentative parameters from established values.", "Add 'game_design.maturity'."),
            ):
                if getattr(design, field_name):
                    continue
                findings.append(_finding(
                    f"tuning-missing-{field_name.replace('_', '-')}:{entity.id}", entity, summary,
                    details, f"game-design.tuning-{field_name.replace('_', '-')}-required", resolution,
                    _location(entity, field_name),
                ))
        if entity.kind == "playtest-claim":
            if not design.claim:
                findings.append(_finding(
                    f"playtest-claim-missing-claim:{entity.id}", entity,
                    f"Playtest claim '{entity.id}' has no claim.",
                    "A playtest claim must state the experience or outcome it expects to verify.",
                    "game-design.playtest-claim-required", "Add 'game_design.claim'.", _location(entity, "claim"),
                ))
            if not design.supports:
                findings.append(_finding(
                    f"playtest-claim-missing-support:{entity.id}", entity,
                    f"Playtest claim '{entity.id}' is not linked to any design entity.",
                    "A playtest claim must identify the design it is intended to test.",
                    "game-design.playtest-claim-support-required", "Add 'game_design.supports'.", _location(entity, "supports"),
                ))
    return findings


def _design_refs(design) -> tuple[EntityId, ...]:
    refs = []
    for name in (
        "player_verbs", "exclusive_verbs", "protected_decisions", "visible_information",
        "hidden_information", "coordination_with", "permitted_viewers", "participating_roles",
        "reads", "changes", "failure", "information_revealed", "information_exchanged",
        "actions_required", "affected_roles", "visible_to", "affected_mechanics", "supports",
    ):
        refs.extend((name, target) for target in getattr(design, name))
    if design.owner_role is not None:
        refs.append(("owner_role", design.owner_role))
    return tuple(refs)


def _location(entity, *field_names):
    for field_name in field_names:
        location = entity.game_design.field_locations.get(field_name)
        if location is not None:
            return location
    return entity.source_location


def _is_role(entity):
    return entity.kind in {"role", "player_role"}


def _finding(identifier, entity, summary, details, rule, resolution, location=None):
    return Finding(
        id=identifier,
        category=FindingCategory.VIOLATION,
        severity=Severity.ERROR,
        confidence="confirmed",
        summary=summary,
        details=details,
        rule=rule,
        spec_entities=(entity.id,),
        implementation_locations=(location or entity.source_location,),
        evidence=(),
        suggested_resolution=resolution,
        requires_decision=False,
    )
