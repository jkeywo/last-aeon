from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path
from typing import Any

import yaml
from yaml.error import MarkedYAMLError
from yaml.nodes import MappingNode, Node, ScalarNode, SequenceNode

from .findings import Finding, FindingCategory, Severity
from .model import (
    Confidence,
    EntityId,
    EvidenceItem,
    EvidenceKind,
    ExceptionSpec,
    Reference,
    SourceLocation,
    SpecEntity,
    Status,
)
from pasm.architecture.model import ArchitectureSection, PlatformConstraints
from pasm.domains.game_design.model import GameDesignSection, InformationVisibility
from pasm.implementation.model import ImplementationSection, MappingStatus
from pasm.migration.model import MigrationPredicate, MigrationSection, RemovalCondition


ALLOWED_TOP_LEVEL_FIELDS = {"entities"}
ALLOWED_ENTITY_FIELDS = {
    "core",
    "architecture",
    "game_design",
    "implementation",
    "migration_plan",
    "evidence",
    "exceptions",
}
ALLOWED_CORE_FIELDS = {
    "title",
    "status",
    "confidence",
    "summary",
    "goals",
    "rationale",
    "tags",
    "references",
    "assumptions",
    "open_questions",
    "supersedes",
    "conflicts_with",
}
ALLOWED_EXCEPTION_FIELDS = {
    "rule",
    "scope",
    "rationale",
    "temporary",
    "removal_condition",
    "approval_status",
}
ALLOWED_EVIDENCE_FIELDS = {"kind", "reference", "summary"}
ALLOWED_DOMAIN_SECTION_NAMES = {"architecture", "game_design", "implementation", "migration_plan"}
ALLOWED_ARCHITECTURE_FIELDS = {
    "kind",
    "classification",
    "owner",
    "owns",
    "reads",
    "readers",
    "writes",
    "produces",
    "consumes",
    "exposes",
    "receives",
    "transforms",
    "coordinates",
    "validates",
    "persists",
    "renders",
    "accepts",
    "sends",
    "depends_on",
    "may_depend_on",
    "must_not_depend_on",
    "runtime_depends_on",
    "build_depends_on",
    "optional_dependency",
    "temporary_dependency",
    "classification",
    "authority",
    "writers",
    "replicas",
    "derived_from",
    "reveal_conditions",
    "runs_in",
    "producer",
    "consumer",
    "payload",
    "validator",
    "version",
    "replacement",
    "trust_boundary",
    "platforms",
}
ALLOWED_PLATFORM_FIELDS = {"allowed", "forbidden"}
ALLOWED_IMPLEMENTATION_FIELDS = {
    "paths",
    "symbols",
    "messages",
    "tests",
    "status",
    "legacy_paths",
    "target_paths",
}
ALLOWED_MIGRATION_FIELDS = {
    "legacy_entities",
    "target_entities",
    "approved_legacy_callers",
    "temporary_adapters",
    "legacy_symbols",
    "target_symbols",
    "removal_conditions",
}
ALLOWED_GAME_DESIGN_FIELDS = {
    "architecture_links", "enforcement_links",
    "responsibilities", "player_verbs", "exclusive_verbs", "protected_decisions", "player_role",
    "visible_information", "hidden_information", "coordination_with", "expected_decision_frequency",
    "owner_role", "protected", "must_not_be", "visibility", "permitted_viewers",
    "reveal_conditions", "reveal_condition", "indirect_signals", "architectural_enforcement", "participating_roles",
    "inputs", "reads", "changes", "eligibility", "costs", "resolution", "outputs", "produces_facts",
    "failure", "side_effects", "information_revealed", "information_exchanged",
    "actions_required", "intended_player_effect", "implementation_path", "sources", "sinks",
    "capacity", "pressure_intent", "causes", "consequences", "affected_roles", "visible_to",
    "terminal", "recovery_paths", "affected_mechanics", "intended_directional_effect",
    "bounds", "maturity", "supporting_evidence", "claim", "supports",
}
ALLOWED_REMOVAL_CONDITION_FIELDS = {"predicate", "subject", "allowed_callers"}
ALLOWED_YAML_TAGS = {
    "tag:yaml.org,2002:map",
    "tag:yaml.org,2002:seq",
    "tag:yaml.org,2002:str",
    "tag:yaml.org,2002:bool",
}


@dataclass(frozen=True)
class ParsedFile:
    path: Path
    entities: tuple[SpecEntity, ...]
    findings: tuple[Finding, ...]


def parse_spec_file(path: Path, spec_root: Path) -> ParsedFile:
    try:
        text = path.read_text(encoding="utf-8")
    except OSError as exc:
        location = SourceLocation(path=path.relative_to(spec_root))
        return ParsedFile(
            path=path,
            entities=(),
            findings=(
                _error_finding(
                    finding_id=f"io:{path.name}",
                    summary=f"Could not read PASM file '{location.path.as_posix()}'.",
                    details=str(exc),
                    rule="io.readable",
                    location=location,
                ),
            ),
        )

    try:
        root = yaml.compose(text)
    except MarkedYAMLError as exc:
        line = exc.problem_mark.line + 1 if exc.problem_mark is not None else None
        column = exc.problem_mark.column + 1 if exc.problem_mark is not None else None
        location = SourceLocation(
            path=path.relative_to(spec_root),
            line=line,
            column=column,
        )
        return ParsedFile(
            path=path,
            entities=(),
            findings=(
                _error_finding(
                    finding_id=f"yaml:{path.name}",
                    summary=f"Malformed YAML in '{location.path.as_posix()}'.",
                    details=exc.problem or "The YAML document could not be parsed.",
                    rule="yaml.well-formed",
                    location=location,
                ),
            ),
        )

    if root is None:
        return ParsedFile(path=path, entities=(), findings=())

    findings: list[Finding] = []
    _reject_unsupported_yaml(root, findings, path.relative_to(spec_root))

    if not isinstance(root, MappingNode):
        location = _location_for_node(path, spec_root, root)
        findings.append(
            _error_finding(
                finding_id=f"shape:{path.name}",
                summary=f"Top-level document in '{location.path.as_posix()}' must be a mapping.",
                details="PASM files must start with a mapping containing an 'entities' list.",
                rule="yaml.top-level-mapping",
                location=location,
            )
        )
        return ParsedFile(path=path, entities=(), findings=tuple(findings))

    top_level = _mapping_to_nodes(
        root,
        allowed_fields=ALLOWED_TOP_LEVEL_FIELDS,
        findings=findings,
        path=path,
        spec_root=spec_root,
        section=("document",),
    )
    entity_nodes = top_level.get("entities")
    if entity_nodes is None:
        findings.append(
            _error_finding(
                finding_id=f"missing-entities:{path.name}",
                summary=f"'{path.relative_to(spec_root).as_posix()}' is missing the top-level 'entities' field.",
                details="Each PASM file must contain an 'entities' sequence, even if it currently holds only one declaration.",
                rule="yaml.entities-required",
                location=_location_for_node(path, spec_root, root, "entities"),
            )
        )
        return ParsedFile(path=path, entities=(), findings=tuple(findings))
    if not isinstance(entity_nodes, SequenceNode):
        findings.append(
            _error_finding(
                finding_id=f"shape:entities:{path.name}",
                summary=f"'entities' in '{path.relative_to(spec_root).as_posix()}' must be a list.",
                details="The top-level 'entities' field must be a sequence.",
                rule="yaml.entities-sequence",
                location=_location_for_node(path, spec_root, entity_nodes, "entities"),
            )
        )
        return ParsedFile(path=path, entities=(), findings=tuple(findings))

    entities: list[SpecEntity] = []
    for index, entity_node in enumerate(entity_nodes.value):
        entity = _parse_entity(
            entity_node=entity_node,
            path=path,
            spec_root=spec_root,
            findings=findings,
            index=index,
        )
        if entity is not None:
            entities.append(entity)

    return ParsedFile(path=path, entities=tuple(entities), findings=tuple(findings))


def _parse_entity(
    entity_node: Node,
    path: Path,
    spec_root: Path,
    findings: list[Finding],
    index: int,
) -> SpecEntity | None:
    if not isinstance(entity_node, MappingNode):
        findings.append(
            _error_finding(
                finding_id=f"shape:entity:{path.name}:{index}",
                summary=f"Entity #{index + 1} in '{path.relative_to(spec_root).as_posix()}' must be a mapping.",
                details="Each entity declaration must be a mapping with one declaration key and optional sections.",
                rule="yaml.entity-mapping",
                location=_location_for_node(path, spec_root, entity_node, "entities", str(index)),
            )
        )
        return None

    pairs = _mapping_to_nodes(
        entity_node,
        allowed_fields=None,
        findings=findings,
        path=path,
        spec_root=spec_root,
        section=("entities", str(index)),
    )
    declaration_pairs = [(key, value) for key, value in pairs.items() if key not in ALLOWED_ENTITY_FIELDS]
    if not declaration_pairs:
        findings.append(
            _error_finding(
                finding_id=f"shape:declaration:{path.name}:{index}",
                summary=f"Entity #{index + 1} in '{path.relative_to(spec_root).as_posix()}' must declare a kind/id pair.",
                details="Each entity needs one declaration key such as 'component: engineering-station'.",
                rule="yaml.entity-declaration-required",
                location=_location_for_node(path, spec_root, entity_node, "entities", str(index)),
            )
        )
        return None

    kind, id_node = declaration_pairs[0]
    kind_location = _location_for_node(path, spec_root, id_node, "entities", str(index), kind)
    entity_id_raw = _expect_string(
        id_node,
        path=path,
        spec_root=spec_root,
        findings=findings,
        finding_id=f"shape:entity-id:{path.name}:{index}",
        summary=f"Entity #{index + 1} declaration value must be a string.",
        rule="yaml.entity-id-string",
        section=("entities", str(index), kind),
    )
    if entity_id_raw is None:
        return None

    try:
        entity_id = EntityId(entity_id_raw)
    except ValueError as exc:
        findings.append(
            _error_finding(
                finding_id=f"invalid-entity-id:{entity_id_raw}",
                summary=f"Invalid entity ID '{entity_id_raw}'.",
                details=str(exc),
                rule="core.entity-id-format",
                location=kind_location,
            )
        )
        return None

    unexpected_fields = {
        key for key, _ in declaration_pairs[1:]
    }
    if unexpected_fields:
        for field_name in sorted(unexpected_fields):
            findings.append(
                _error_finding(
                    finding_id=f"unknown-entity-field:{entity_id}:{field_name}",
                    summary=f"Unknown field '{field_name}' on entity '{entity_id}'.",
                    details="Phase 0-2 accepts only the declaration key, 'core', domain sections, 'exceptions', and 'evidence'.",
                    rule="yaml.unknown-entity-field",
                    location=_location_for_node(path, spec_root, pairs[field_name], "entities", str(index), field_name),
                )
            )

    core_node = pairs.get("core")
    if core_node is None or not isinstance(core_node, MappingNode):
        findings.append(
            _error_finding(
                finding_id=f"missing-core:{entity_id}",
                summary=f"Entity '{entity_id}' must define a 'core' mapping.",
                details="The shared core section supplies lifecycle status and other common metadata.",
                rule="core.section-required",
                location=kind_location,
            )
        )
        return None

    core = _mapping_to_nodes(
        core_node,
        allowed_fields=ALLOWED_CORE_FIELDS,
        findings=findings,
        path=path,
        spec_root=spec_root,
        section=("entities", str(index), "core"),
    )
    status_raw = _required_string(
        core,
        field_name="status",
        path=path,
        spec_root=spec_root,
        findings=findings,
        entity_id=entity_id,
        section=("entities", str(index), "core"),
        rule="core.status-required",
    )
    if status_raw is None:
        return None

    try:
        status = Status(status_raw)
    except ValueError:
        findings.append(
            _error_finding(
                finding_id=f"invalid-status:{entity_id}",
                summary=f"Entity '{entity_id}' uses invalid status '{status_raw}'.",
                details="Status must be one of the PASM lifecycle values from the Phase 1 core model.",
                rule="core.status-valid",
                location=_location_for_node(path, spec_root, core["status"], "entities", str(index), "core", "status"),
            )
        )
        return None

    confidence_node = core.get("confidence")
    confidence = Confidence.UNKNOWN
    if confidence_node is not None:
        confidence_raw = _expect_string(
            confidence_node,
            path=path,
            spec_root=spec_root,
            findings=findings,
            finding_id=f"invalid-confidence-shape:{entity_id}",
            summary=f"Entity '{entity_id}' confidence must be a string.",
            rule="core.confidence-string",
            section=("entities", str(index), "core", "confidence"),
        )
        if confidence_raw is not None:
            try:
                confidence = Confidence(confidence_raw)
            except ValueError:
                findings.append(
                    _error_finding(
                        finding_id=f"invalid-confidence:{entity_id}",
                        summary=f"Entity '{entity_id}' uses invalid confidence '{confidence_raw}'.",
                        details="Confidence must be one of the PASM confidence values.",
                        rule="core.confidence-valid",
                        location=_location_for_node(path, spec_root, confidence_node, "entities", str(index), "core", "confidence"),
                    )
                )
                return None

    entity = SpecEntity(
        id=entity_id,
        kind=kind,
        status=status,
        confidence=confidence,
        title=_optional_string(core.get("title"), path, spec_root, findings, entity_id, ("entities", str(index), "core", "title")),
        summary=_optional_string(core.get("summary"), path, spec_root, findings, entity_id, ("entities", str(index), "core", "summary")),
        goals=_string_list(core.get("goals"), path, spec_root, findings, entity_id, ("entities", str(index), "core", "goals")),
        rationale=_string_list(core.get("rationale"), path, spec_root, findings, entity_id, ("entities", str(index), "core", "rationale")),
        tags=_string_list(core.get("tags"), path, spec_root, findings, entity_id, ("entities", str(index), "core", "tags")),
        references=_references(core.get("references"), path, spec_root, findings, entity_id, ("entities", str(index), "core", "references")),
        assumptions=_string_list(core.get("assumptions"), path, spec_root, findings, entity_id, ("entities", str(index), "core", "assumptions")),
        open_questions=_string_list(core.get("open_questions"), path, spec_root, findings, entity_id, ("entities", str(index), "core", "open_questions")),
        supersedes=_entity_id_list(core.get("supersedes"), path, spec_root, findings, entity_id, ("entities", str(index), "core", "supersedes")),
        conflicts_with=_entity_id_list(core.get("conflicts_with"), path, spec_root, findings, entity_id, ("entities", str(index), "core", "conflicts_with")),
        exceptions=_exceptions(pairs.get("exceptions"), path, spec_root, findings, entity_id, ("entities", str(index), "exceptions")),
        evidence=_evidence(pairs.get("evidence"), path, spec_root, findings, entity_id, ("entities", str(index), "evidence")),
        architecture=_architecture_section(
            pairs.get("architecture"),
            path=path,
            spec_root=spec_root,
            findings=findings,
            entity_id=entity_id,
            section=("entities", str(index), "architecture"),
        ),
        game_design=_game_design_section(
            pairs.get("game_design"),
            path=path,
            spec_root=spec_root,
            findings=findings,
            entity_id=entity_id,
            section=("entities", str(index), "game_design"),
        ),
        implementation=_implementation_section(
            pairs.get("implementation"),
            path=path,
            spec_root=spec_root,
            findings=findings,
            entity_id=entity_id,
            section=("entities", str(index), "implementation"),
        ),
        migration=_migration_section(
            pairs.get("migration_plan"),
            path=path,
            spec_root=spec_root,
            findings=findings,
            entity_id=entity_id,
            section=("entities", str(index), "migration_plan"),
        ),
        domain_sections=_raw_domain_sections(pairs, path, spec_root, findings, entity_id, ("entities", str(index))),
        source_location=kind_location,
    )
    return entity


def _raw_domain_sections(
    pairs: dict[str, Node],
    path: Path,
    spec_root: Path,
    findings: list[Finding],
    entity_id: EntityId,
    base_section: tuple[str, ...],
) -> dict[str, Any]:
    sections: dict[str, Any] = {}
    for section_name in ALLOWED_DOMAIN_SECTION_NAMES:
        if section_name in {"architecture", "game_design", "implementation", "migration_plan"}:
            continue
        node = pairs.get(section_name)
        if node is None:
            continue
        if not isinstance(node, MappingNode):
            findings.append(
                _error_finding(
                    finding_id=f"invalid-domain-section:{entity_id}:{section_name}",
                    summary=f"Entity '{entity_id}' section '{section_name}' must be a mapping.",
                    details="Phase 0-2 keeps domain sections as raw structured data for later phases.",
                    rule="yaml.domain-section-mapping",
                    location=_location_for_node(path, spec_root, node, *base_section, section_name),
                )
            )
            continue
        sections[section_name] = _node_to_plain_value(
            node,
            path=path,
            spec_root=spec_root,
            findings=findings,
            section=base_section + (section_name,),
        )
    return sections


def _game_design_section(
    node: Node | None,
    path: Path,
    spec_root: Path,
    findings: list[Finding],
    entity_id: EntityId,
    section: tuple[str, ...],
) -> GameDesignSection | None:
    if node is None:
        return None
    if not isinstance(node, MappingNode):
        findings.append(
            _error_finding(
                finding_id=f"invalid-game-design-section:{entity_id}",
                summary=f"Entity '{entity_id}' section 'game_design' must be a mapping.",
                details="Phase 7 game-design declarations use a restricted mapping schema.",
                rule="yaml.game-design-section-mapping",
                location=_location_for_node(path, spec_root, node, *section),
            )
        )
        return None
    values = _mapping_to_nodes(
        node,
        allowed_fields=ALLOWED_GAME_DESIGN_FIELDS,
        findings=findings,
        path=path,
        spec_root=spec_root,
        section=section,
    )
    visibility = _game_design_visibility(
        values.get("visibility"), path, spec_root, findings, entity_id, section + ("visibility",)
    )
    return GameDesignSection(
        field_locations={
            name: _location_for_node(path, spec_root, field_node, *section, name)
            for name, field_node in values.items()
        },
        architecture_links=_entity_id_list(values.get("architecture_links"), path, spec_root, findings, entity_id, section + ("architecture_links",)),
        enforcement_links=_entity_id_list(values.get("enforcement_links"), path, spec_root, findings, entity_id, section + ("enforcement_links",)),
        responsibilities=_string_list(values.get("responsibilities"), path, spec_root, findings, entity_id, section + ("responsibilities",)),
        player_verbs=_entity_id_list(values.get("player_verbs"), path, spec_root, findings, entity_id, section + ("player_verbs",)),
        exclusive_verbs=_entity_id_list(values.get("exclusive_verbs"), path, spec_root, findings, entity_id, section + ("exclusive_verbs",)),
        protected_decisions=_entity_id_list(values.get("protected_decisions"), path, spec_root, findings, entity_id, section + ("protected_decisions",)),
        visible_information=_entity_id_list(values.get("visible_information"), path, spec_root, findings, entity_id, section + ("visible_information",)),
        hidden_information=_entity_id_list(values.get("hidden_information"), path, spec_root, findings, entity_id, section + ("hidden_information",)),
        coordination_with=_entity_id_list(values.get("coordination_with"), path, spec_root, findings, entity_id, section + ("coordination_with",)),
        expected_decision_frequency=_optional_string(values.get("expected_decision_frequency"), path, spec_root, findings, entity_id, section + ("expected_decision_frequency",)),
        owner_role=_game_design_owner_role(values, path, spec_root, findings, entity_id, section),
        protected=_optional_bool(values.get("protected"), path, spec_root, findings, entity_id, section + ("protected",)),
        must_not_be=_string_list(values.get("must_not_be"), path, spec_root, findings, entity_id, section + ("must_not_be",)),
        visibility=visibility,
        permitted_viewers=_entity_id_list(values.get("permitted_viewers"), path, spec_root, findings, entity_id, section + ("permitted_viewers",)),
        reveal_conditions=_string_list(values.get("reveal_conditions"), path, spec_root, findings, entity_id, section + ("reveal_conditions",)) + _string_list(values.get("reveal_condition"), path, spec_root, findings, entity_id, section + ("reveal_condition",)),
        indirect_signals=_string_list(values.get("indirect_signals"), path, spec_root, findings, entity_id, section + ("indirect_signals",)),
        architectural_enforcement=_string_list(values.get("architectural_enforcement"), path, spec_root, findings, entity_id, section + ("architectural_enforcement",)),
        participating_roles=_entity_id_list(values.get("participating_roles"), path, spec_root, findings, entity_id, section + ("participating_roles",)),
        inputs=_string_list(values.get("inputs"), path, spec_root, findings, entity_id, section + ("inputs",)),
        reads=_entity_id_list(values.get("reads"), path, spec_root, findings, entity_id, section + ("reads",)),
        changes=_entity_id_list(values.get("changes"), path, spec_root, findings, entity_id, section + ("changes",)),
        eligibility=_string_list(values.get("eligibility"), path, spec_root, findings, entity_id, section + ("eligibility",)),
        costs=_string_list(values.get("costs"), path, spec_root, findings, entity_id, section + ("costs",)),
        resolution=_optional_string(values.get("resolution"), path, spec_root, findings, entity_id, section + ("resolution",)),
        outputs=_string_list(values.get("outputs"), path, spec_root, findings, entity_id, section + ("outputs",)),
        produces_facts=_string_list(values.get("produces_facts"), path, spec_root, findings, entity_id, section + ("produces_facts",)),
        failure=_entity_id_list(values.get("failure"), path, spec_root, findings, entity_id, section + ("failure",)),
        side_effects=_string_list(values.get("side_effects"), path, spec_root, findings, entity_id, section + ("side_effects",)),
        information_revealed=_entity_id_list(values.get("information_revealed"), path, spec_root, findings, entity_id, section + ("information_revealed",)),
        information_exchanged=_entity_id_list(values.get("information_exchanged"), path, spec_root, findings, entity_id, section + ("information_exchanged",)),
        actions_required=_entity_id_list(values.get("actions_required"), path, spec_root, findings, entity_id, section + ("actions_required",)),
        intended_player_effect=_optional_string(values.get("intended_player_effect"), path, spec_root, findings, entity_id, section + ("intended_player_effect",)),
        implementation_path=_string_list(values.get("implementation_path"), path, spec_root, findings, entity_id, section + ("implementation_path",)),
        sources=_string_list(values.get("sources"), path, spec_root, findings, entity_id, section + ("sources",)),
        sinks=_string_list(values.get("sinks"), path, spec_root, findings, entity_id, section + ("sinks",)),
        capacity=_optional_string(values.get("capacity"), path, spec_root, findings, entity_id, section + ("capacity",)),
        pressure_intent=_string_list(values.get("pressure_intent"), path, spec_root, findings, entity_id, section + ("pressure_intent",)),
        causes=_string_list(values.get("causes"), path, spec_root, findings, entity_id, section + ("causes",)),
        consequences=_string_list(values.get("consequences"), path, spec_root, findings, entity_id, section + ("consequences",)),
        affected_roles=_entity_id_list(values.get("affected_roles"), path, spec_root, findings, entity_id, section + ("affected_roles",)),
        visible_to=_entity_id_list(values.get("visible_to"), path, spec_root, findings, entity_id, section + ("visible_to",)),
        terminal=_optional_bool(values.get("terminal"), path, spec_root, findings, entity_id, section + ("terminal",)),
        recovery_paths=_string_list(values.get("recovery_paths"), path, spec_root, findings, entity_id, section + ("recovery_paths",)),
        affected_mechanics=_entity_id_list(values.get("affected_mechanics"), path, spec_root, findings, entity_id, section + ("affected_mechanics",)),
        intended_directional_effect=_optional_string(values.get("intended_directional_effect"), path, spec_root, findings, entity_id, section + ("intended_directional_effect",)),
        bounds=_optional_string(values.get("bounds"), path, spec_root, findings, entity_id, section + ("bounds",)),
        maturity=_optional_string(values.get("maturity"), path, spec_root, findings, entity_id, section + ("maturity",)),
        supporting_evidence=_string_list(values.get("supporting_evidence"), path, spec_root, findings, entity_id, section + ("supporting_evidence",)),
        claim=_optional_string(values.get("claim"), path, spec_root, findings, entity_id, section + ("claim",)),
        supports=_entity_id_list(values.get("supports"), path, spec_root, findings, entity_id, section + ("supports",)),
    )


def _game_design_visibility(node, path, spec_root, findings, entity_id, section):
    raw = _optional_string(node, path, spec_root, findings, entity_id, section)
    if raw is None:
        return None
    try:
        return InformationVisibility(raw)
    except ValueError:
        findings.append(
            _error_finding(
                finding_id=f"invalid-information-visibility:{entity_id}:{raw}",
                summary=f"Entity '{entity_id}' uses unknown information visibility '{raw}'.",
                details="Visibility must use a fixed Phase 7 information-visibility value.",
                rule="game-design.information-visibility-valid",
                location=_location_for_node(path, spec_root, node, *section),
            )
        )
        return None


def _game_design_owner_role(values, path, spec_root, findings, entity_id, section):
    owner_node = values.get("owner_role")
    player_role_node = values.get("player_role")
    if owner_node is not None and player_role_node is not None:
        findings.append(
            _error_finding(
                finding_id=f"duplicate-game-design-owner-role:{entity_id}",
                summary=f"Entity '{entity_id}' declares both 'owner_role' and legacy 'player_role'.",
                details="Use one role-owner spelling in a game-design declaration.",
                rule="game-design.owner-role-single-spelling",
                location=_location_for_node(path, spec_root, player_role_node, *section, "player_role"),
            )
        )
    node = owner_node or player_role_node
    field_name = "owner_role" if owner_node is not None else "player_role"
    return _optional_entity_id(node, path, spec_root, findings, entity_id, section + (field_name,))


def _architecture_section(
    node: Node | None,
    path: Path,
    spec_root: Path,
    findings: list[Finding],
    entity_id: EntityId,
    section: tuple[str, ...],
) -> ArchitectureSection | None:
    if node is None:
        return None
    if not isinstance(node, MappingNode):
        findings.append(
            _error_finding(
                finding_id=f"invalid-architecture-section:{entity_id}",
                summary=f"Entity '{entity_id}' section 'architecture' must be a mapping.",
                details="Phase 3 architecture declarations use a restricted mapping schema.",
                rule="yaml.architecture-section-mapping",
                location=_location_for_node(path, spec_root, node, *section),
            )
        )
        return None

    values = _mapping_to_nodes(
        node,
        allowed_fields=ALLOWED_ARCHITECTURE_FIELDS,
        findings=findings,
        path=path,
        spec_root=spec_root,
        section=section,
    )
    return ArchitectureSection(
        kind=_optional_string(values.get("kind"), path, spec_root, findings, entity_id, section + ("kind",)),
        classification=_optional_string(values.get("classification"), path, spec_root, findings, entity_id, section + ("classification",)),
        authority=_optional_string(values.get("authority"), path, spec_root, findings, entity_id, section + ("authority",)),
        owner=_optional_entity_id(values.get("owner"), path, spec_root, findings, entity_id, section + ("owner",)),
        owns=_entity_id_list(values.get("owns"), path, spec_root, findings, entity_id, section + ("owns",)),
        reads=_entity_id_list(values.get("reads"), path, spec_root, findings, entity_id, section + ("reads",)),
        readers=_entity_id_list(values.get("readers"), path, spec_root, findings, entity_id, section + ("readers",)),
        writes=_entity_id_list(values.get("writes"), path, spec_root, findings, entity_id, section + ("writes",)),
        produces=_entity_id_list(values.get("produces"), path, spec_root, findings, entity_id, section + ("produces",)),
        consumes=_entity_id_list(values.get("consumes"), path, spec_root, findings, entity_id, section + ("consumes",)),
        exposes=_entity_id_list(values.get("exposes"), path, spec_root, findings, entity_id, section + ("exposes",)),
        receives=_entity_id_list(values.get("receives"), path, spec_root, findings, entity_id, section + ("receives",)),
        transforms=_entity_id_list(values.get("transforms"), path, spec_root, findings, entity_id, section + ("transforms",)),
        coordinates=_entity_id_list(values.get("coordinates"), path, spec_root, findings, entity_id, section + ("coordinates",)),
        validates=_entity_id_list(values.get("validates"), path, spec_root, findings, entity_id, section + ("validates",)),
        persists=_entity_id_list(values.get("persists"), path, spec_root, findings, entity_id, section + ("persists",)),
        renders=_entity_id_list(values.get("renders"), path, spec_root, findings, entity_id, section + ("renders",)),
        accepts=_entity_id_list(values.get("accepts"), path, spec_root, findings, entity_id, section + ("accepts",)),
        sends=_entity_id_list(values.get("sends"), path, spec_root, findings, entity_id, section + ("sends",)),
        depends_on=_entity_id_list(values.get("depends_on"), path, spec_root, findings, entity_id, section + ("depends_on",)),
        may_depend_on=_entity_id_list(values.get("may_depend_on"), path, spec_root, findings, entity_id, section + ("may_depend_on",)),
        must_not_depend_on=_entity_id_list(values.get("must_not_depend_on"), path, spec_root, findings, entity_id, section + ("must_not_depend_on",)),
        runtime_depends_on=_entity_id_list(values.get("runtime_depends_on"), path, spec_root, findings, entity_id, section + ("runtime_depends_on",)),
        build_depends_on=_entity_id_list(values.get("build_depends_on"), path, spec_root, findings, entity_id, section + ("build_depends_on",)),
        optional_dependency=_entity_id_list(values.get("optional_dependency"), path, spec_root, findings, entity_id, section + ("optional_dependency",)),
        temporary_dependency=_entity_id_list(values.get("temporary_dependency"), path, spec_root, findings, entity_id, section + ("temporary_dependency",)),
        writers=_entity_id_list(values.get("writers"), path, spec_root, findings, entity_id, section + ("writers",)),
        replicas=_entity_id_list(values.get("replicas"), path, spec_root, findings, entity_id, section + ("replicas",)),
        derived_from=_entity_id_list(values.get("derived_from"), path, spec_root, findings, entity_id, section + ("derived_from",)),
        reveal_conditions=_entity_id_list(values.get("reveal_conditions"), path, spec_root, findings, entity_id, section + ("reveal_conditions",)),
        runs_in=_entity_id_list(values.get("runs_in"), path, spec_root, findings, entity_id, section + ("runs_in",)),
        producer=_entity_id_list(values.get("producer"), path, spec_root, findings, entity_id, section + ("producer",)),
        consumer=_entity_id_list(values.get("consumer"), path, spec_root, findings, entity_id, section + ("consumer",)),
        validator=_entity_id_list(values.get("validator"), path, spec_root, findings, entity_id, section + ("validator",)),
        payload=_optional_string(values.get("payload"), path, spec_root, findings, entity_id, section + ("payload",)),
        version=_optional_string(values.get("version"), path, spec_root, findings, entity_id, section + ("version",)),
        replacement=_optional_entity_id(values.get("replacement"), path, spec_root, findings, entity_id, section + ("replacement",)),
        trust_boundary=_optional_string(values.get("trust_boundary"), path, spec_root, findings, entity_id, section + ("trust_boundary",)),
        platforms=_platform_constraints(values.get("platforms"), path, spec_root, findings, entity_id, section + ("platforms",)),
    )


def _implementation_section(
    node: Node | None,
    path: Path,
    spec_root: Path,
    findings: list[Finding],
    entity_id: EntityId,
    section: tuple[str, ...],
) -> ImplementationSection | None:
    if node is None:
        return None
    if not isinstance(node, MappingNode):
        findings.append(
            _error_finding(
                finding_id=f"invalid-implementation-section:{entity_id}",
                summary=f"Entity '{entity_id}' section 'implementation' must be a mapping.",
                details="Phase 4 implementation declarations use a restricted mapping schema.",
                rule="yaml.implementation-section-mapping",
                location=_location_for_node(path, spec_root, node, *section),
            )
        )
        return None
    values = _mapping_to_nodes(
        node,
        allowed_fields=ALLOWED_IMPLEMENTATION_FIELDS,
        findings=findings,
        path=path,
        spec_root=spec_root,
        section=section,
    )
    status = None
    status_raw = _optional_string(values.get("status"), path, spec_root, findings, entity_id, section + ("status",))
    if status_raw is not None:
        try:
            status = MappingStatus(status_raw)
        except ValueError:
            findings.append(
                _error_finding(
                    finding_id=f"invalid-implementation-status:{entity_id}",
                    summary=f"Entity '{entity_id}' uses invalid implementation status '{status_raw}'.",
                    details="Implementation status must be one of declared, observed, confirmed, suspected, stale, or removed.",
                    rule="implementation.status-valid",
                    location=_location_for_node(path, spec_root, values["status"], *section, "status"),
                )
            )
            return None
    return ImplementationSection(
        paths=_path_list(values.get("paths"), path, spec_root, findings, entity_id, section + ("paths",)),
        symbols=_string_list(values.get("symbols"), path, spec_root, findings, entity_id, section + ("symbols",)),
        messages=_string_list(values.get("messages"), path, spec_root, findings, entity_id, section + ("messages",)),
        tests=_string_list(values.get("tests"), path, spec_root, findings, entity_id, section + ("tests",)),
        status=status,
        legacy_paths=_path_list(values.get("legacy_paths"), path, spec_root, findings, entity_id, section + ("legacy_paths",)),
        target_paths=_path_list(values.get("target_paths"), path, spec_root, findings, entity_id, section + ("target_paths",)),
    )


def _migration_section(
    node: Node | None,
    path: Path,
    spec_root: Path,
    findings: list[Finding],
    entity_id: EntityId,
    section: tuple[str, ...],
) -> MigrationSection | None:
    if node is None:
        return None
    if not isinstance(node, MappingNode):
        findings.append(
            _error_finding(
                finding_id=f"invalid-migration-section:{entity_id}",
                summary=f"Entity '{entity_id}' section 'migration' must be a mapping.",
                details="Phase 6 migration declarations use a restricted mapping schema.",
                rule="yaml.migration-section-mapping",
                location=_location_for_node(path, spec_root, node, *section),
            )
        )
        return None
    values = _mapping_to_nodes(
        node,
        allowed_fields=ALLOWED_MIGRATION_FIELDS,
        findings=findings,
        path=path,
        spec_root=spec_root,
        section=section,
    )
    return MigrationSection(
        legacy_entities=_entity_id_list(
            values.get("legacy_entities"), path, spec_root, findings, entity_id, section + ("legacy_entities",)
        ),
        target_entities=_entity_id_list(
            values.get("target_entities"), path, spec_root, findings, entity_id, section + ("target_entities",)
        ),
        approved_legacy_callers=_entity_id_list(
            values.get("approved_legacy_callers"), path, spec_root, findings, entity_id, section + ("approved_legacy_callers",)
        ),
        temporary_adapters=_entity_id_list(
            values.get("temporary_adapters"), path, spec_root, findings, entity_id, section + ("temporary_adapters",)
        ),
        legacy_symbols=_string_list(
            values.get("legacy_symbols"), path, spec_root, findings, entity_id, section + ("legacy_symbols",)
        ),
        target_symbols=_string_list(
            values.get("target_symbols"), path, spec_root, findings, entity_id, section + ("target_symbols",)
        ),
        removal_conditions=_removal_conditions(
            values.get("removal_conditions"), path, spec_root, findings, entity_id, section + ("removal_conditions",)
        ),
    )


def _removal_conditions(
    node: Node | None,
    path: Path,
    spec_root: Path,
    findings: list[Finding],
    entity_id: EntityId,
    section: tuple[str, ...],
) -> tuple[RemovalCondition, ...]:
    if node is None:
        return ()
    if not isinstance(node, SequenceNode):
        findings.append(
            _error_finding(
                finding_id=f"invalid-removal-conditions:{entity_id}",
                summary=f"Entity '{entity_id}' field 'removal_conditions' must be a list.",
                details="Migration removal conditions must be declared as a sequence of mappings.",
                rule="yaml.removal-conditions-sequence",
                location=_location_for_node(path, spec_root, node, *section),
            )
        )
        return ()

    conditions: list[RemovalCondition] = []
    for index, item in enumerate(node.value):
        if not isinstance(item, MappingNode):
            findings.append(
                _error_finding(
                    finding_id=f"invalid-removal-condition:{entity_id}:{index}",
                    summary=f"Entity '{entity_id}' removal condition #{index + 1} must be a mapping.",
                    details="Each removal condition must declare a predicate and a subject.",
                    rule="yaml.removal-condition-mapping",
                    location=_location_for_node(path, spec_root, item, *section, str(index)),
                )
            )
            continue
        values = _mapping_to_nodes(
            item,
            allowed_fields=ALLOWED_REMOVAL_CONDITION_FIELDS,
            findings=findings,
            path=path,
            spec_root=spec_root,
            section=section + (str(index),),
        )
        predicate_raw = _required_string(
            values,
            field_name="predicate",
            path=path,
            spec_root=spec_root,
            findings=findings,
            entity_id=entity_id,
            section=section + (str(index),),
            rule="migration.removal-condition-predicate-required",
        )
        subject = _required_string(
            values,
            field_name="subject",
            path=path,
            spec_root=spec_root,
            findings=findings,
            entity_id=entity_id,
            section=section + (str(index),),
            rule="migration.removal-condition-subject-required",
        )
        if predicate_raw is None or subject is None:
            continue
        try:
            predicate = MigrationPredicate(predicate_raw)
        except ValueError:
            findings.append(
                _error_finding(
                    finding_id=f"invalid-removal-condition-predicate:{entity_id}:{predicate_raw}",
                    summary=f"Entity '{entity_id}' uses unknown migration predicate '{predicate_raw}'.",
                    details="Migration removal predicates must use one of the fixed Phase 6 predicate names.",
                    rule="migration.removal-condition-predicate-valid",
                    location=_location_for_node(path, spec_root, values["predicate"], *section, str(index), "predicate"),
                )
            )
            continue
        conditions.append(
            RemovalCondition(
                predicate=predicate,
                subject=subject,
                allowed_callers=_entity_id_list(
                    values.get("allowed_callers"),
                    path,
                    spec_root,
                    findings,
                    entity_id,
                    section + (str(index), "allowed_callers"),
                ),
                source_location=_location_for_node(path, spec_root, item, *section, str(index)),
            )
        )
    return tuple(conditions)


def _exceptions(
    node: Node | None,
    path: Path,
    spec_root: Path,
    findings: list[Finding],
    entity_id: EntityId,
    section: tuple[str, ...],
) -> tuple[ExceptionSpec, ...]:
    if node is None:
        return ()
    if not isinstance(node, SequenceNode):
        findings.append(
            _error_finding(
                finding_id=f"invalid-exceptions:{entity_id}",
                summary=f"Entity '{entity_id}' exceptions must be a list.",
                details="The 'exceptions' section must be a sequence of exception mappings.",
                rule="yaml.exceptions-sequence",
                location=_location_for_node(path, spec_root, node, *section),
            )
        )
        return ()

    exceptions: list[ExceptionSpec] = []
    for index, item in enumerate(node.value):
        if not isinstance(item, MappingNode):
            findings.append(
                _error_finding(
                    finding_id=f"invalid-exception-item:{entity_id}:{index}",
                    summary=f"Entity '{entity_id}' exception #{index + 1} must be a mapping.",
                    details="Each exception must declare rule, scope, rationale, temporary, and related metadata.",
                    rule="yaml.exception-mapping",
                    location=_location_for_node(path, spec_root, item, *section, str(index)),
                )
            )
            continue
        values = _mapping_to_nodes(
            item,
            allowed_fields=ALLOWED_EXCEPTION_FIELDS,
            findings=findings,
            path=path,
            spec_root=spec_root,
            section=section + (str(index),),
        )
        rule = _required_scalar_value(values, "rule", path, spec_root, findings, entity_id, section + (str(index),), "yaml.exception-rule-required")
        rationale = _required_scalar_value(values, "rationale", path, spec_root, findings, entity_id, section + (str(index),), "yaml.exception-rationale-required")
        scope = _string_list(values.get("scope"), path, spec_root, findings, entity_id, section + (str(index), "scope"))
        temporary = _required_bool(values, "temporary", path, spec_root, findings, entity_id, section + (str(index),), "yaml.exception-temporary-required")
        removal_condition = _string_list(values.get("removal_condition"), path, spec_root, findings, entity_id, section + (str(index), "removal_condition"))
        approval_status = _optional_string(values.get("approval_status"), path, spec_root, findings, entity_id, section + (str(index), "approval_status"))
        if rule is None or rationale is None or temporary is None:
            continue
        exceptions.append(
            ExceptionSpec(
                rule=rule,
                scope=scope,
                rationale=rationale,
                temporary=temporary,
                removal_condition=removal_condition,
                approval_status=approval_status,
                source_location=_location_for_node(path, spec_root, item, *section, str(index)),
            )
        )
    return tuple(exceptions)


def _evidence(
    node: Node | None,
    path: Path,
    spec_root: Path,
    findings: list[Finding],
    entity_id: EntityId,
    section: tuple[str, ...],
) -> tuple[EvidenceItem, ...]:
    if node is None:
        return ()
    if not isinstance(node, SequenceNode):
        findings.append(
            _error_finding(
                finding_id=f"invalid-evidence:{entity_id}",
                summary=f"Entity '{entity_id}' evidence must be a list.",
                details="The 'evidence' section must be a sequence of evidence mappings.",
                rule="yaml.evidence-sequence",
                location=_location_for_node(path, spec_root, node, *section),
            )
        )
        return ()

    items: list[EvidenceItem] = []
    for index, item in enumerate(node.value):
        if not isinstance(item, MappingNode):
            findings.append(
                _error_finding(
                    finding_id=f"invalid-evidence-item:{entity_id}:{index}",
                    summary=f"Entity '{entity_id}' evidence item #{index + 1} must be a mapping.",
                    details="Each evidence item must declare a kind and optional reference/summary.",
                    rule="yaml.evidence-mapping",
                    location=_location_for_node(path, spec_root, item, *section, str(index)),
                )
            )
            continue
        values = _mapping_to_nodes(
            item,
            allowed_fields=ALLOWED_EVIDENCE_FIELDS,
            findings=findings,
            path=path,
            spec_root=spec_root,
            section=section + (str(index),),
        )
        kind_raw = _required_scalar_value(values, "kind", path, spec_root, findings, entity_id, section + (str(index),), "yaml.evidence-kind-required")
        if kind_raw is None:
            continue
        try:
            kind = EvidenceKind(kind_raw)
        except ValueError:
            kind = EvidenceKind.OTHER
        items.append(
            EvidenceItem(
                kind=kind,
                reference=_optional_string(values.get("reference"), path, spec_root, findings, entity_id, section + (str(index), "reference")),
                summary=_optional_string(values.get("summary"), path, spec_root, findings, entity_id, section + (str(index), "summary")),
                source_location=_location_for_node(path, spec_root, item, *section, str(index)),
            )
        )
    return tuple(items)


def _references(
    node: Node | None,
    path: Path,
    spec_root: Path,
    findings: list[Finding],
    entity_id: EntityId,
    section: tuple[str, ...],
) -> tuple[Reference, ...]:
    refs = _entity_id_list(node, path, spec_root, findings, entity_id, section)
    return tuple(
        Reference(
            target=entity_id_value,
            source_location=_location_for_node(path, spec_root, node, *section) if node is not None else SourceLocation(path.relative_to(spec_root)),
        )
        for entity_id_value in refs
    )


def _entity_id_list(
    node: Node | None,
    path: Path,
    spec_root: Path,
    findings: list[Finding],
    entity_id: EntityId,
    section: tuple[str, ...],
) -> tuple[EntityId, ...]:
    values = _string_list(node, path, spec_root, findings, entity_id, section)
    ids: list[EntityId] = []
    for raw in values:
        try:
            ids.append(EntityId(raw))
        except ValueError as exc:
            findings.append(
                _error_finding(
                    finding_id=f"invalid-related-id:{entity_id}:{raw}",
                    summary=f"Entity '{entity_id}' contains invalid entity reference '{raw}'.",
                    details=str(exc),
                    rule="core.related-entity-id-format",
                    location=_location_for_node(path, spec_root, node, *section) if node is not None else SourceLocation(path.relative_to(spec_root)),
                )
            )
    return tuple(ids)


def _optional_entity_id(
    node: Node | None,
    path: Path,
    spec_root: Path,
    findings: list[Finding],
    entity_id: EntityId,
    section: tuple[str, ...],
) -> EntityId | None:
    if node is None:
        return None
    raw = _expect_string(
        node,
        path=path,
        spec_root=spec_root,
        findings=findings,
        finding_id=f"invalid-entity-id-field:{entity_id}:{'.'.join(section)}",
        summary=f"Entity '{entity_id}' field '{section[-1]}' must be an entity ID string.",
        rule="yaml.entity-id-field",
        section=section,
    )
    if raw is None:
        return None
    try:
        return EntityId(raw)
    except ValueError as exc:
        findings.append(
            _error_finding(
                finding_id=f"invalid-entity-id-value:{entity_id}:{raw}",
                summary=f"Entity '{entity_id}' contains invalid entity ID '{raw}' in field '{section[-1]}'.",
                details=str(exc),
                rule="core.entity-id-format",
                location=_location_for_node(path, spec_root, node, *section),
            )
        )
        return None


def _platform_constraints(
    node: Node | None,
    path: Path,
    spec_root: Path,
    findings: list[Finding],
    entity_id: EntityId,
    section: tuple[str, ...],
) -> PlatformConstraints | None:
    if node is None:
        return None
    if not isinstance(node, MappingNode):
        findings.append(
            _error_finding(
                finding_id=f"invalid-platforms:{entity_id}",
                summary=f"Entity '{entity_id}' field 'platforms' must be a mapping.",
                details="Platform constraints use a nested mapping with 'allowed' and/or 'forbidden' string lists.",
                rule="yaml.platforms-mapping",
                location=_location_for_node(path, spec_root, node, *section),
            )
        )
        return None
    values = _mapping_to_nodes(
        node,
        allowed_fields=ALLOWED_PLATFORM_FIELDS,
        findings=findings,
        path=path,
        spec_root=spec_root,
        section=section,
    )
    return PlatformConstraints(
        allowed=_string_list(values.get("allowed"), path, spec_root, findings, entity_id, section + ("allowed",)),
        forbidden=_string_list(values.get("forbidden"), path, spec_root, findings, entity_id, section + ("forbidden",)),
    )


def _path_list(
    node: Node | None,
    path: Path,
    spec_root: Path,
    findings: list[Finding],
    entity_id: EntityId,
    section: tuple[str, ...],
) -> tuple[Path, ...]:
    return tuple(Path(item) for item in _string_list(node, path, spec_root, findings, entity_id, section))


def _string_list(
    node: Node | None,
    path: Path,
    spec_root: Path,
    findings: list[Finding],
    entity_id: EntityId,
    section: tuple[str, ...],
) -> tuple[str, ...]:
    if node is None:
        return ()
    if not isinstance(node, SequenceNode):
        findings.append(
            _error_finding(
                finding_id=f"invalid-list:{entity_id}:{'.'.join(section)}",
                summary=f"Entity '{entity_id}' field '{section[-1]}' must be a list of strings.",
                details="The field uses the restricted YAML list form for repeated strings.",
                rule="yaml.string-list",
                location=_location_for_node(path, spec_root, node, *section),
            )
        )
        return ()
    values: list[str] = []
    for index, item in enumerate(node.value):
        raw = _expect_string(
            item,
            path=path,
            spec_root=spec_root,
            findings=findings,
            finding_id=f"invalid-list-item:{entity_id}:{'.'.join(section)}:{index}",
            summary=f"Entity '{entity_id}' field '{section[-1]}' item #{index + 1} must be a string.",
            rule="yaml.string-list-item",
            section=section + (str(index),),
        )
        if raw is not None:
            values.append(raw)
    return tuple(values)


def _required_bool(
    values: dict[str, Node],
    field_name: str,
    path: Path,
    spec_root: Path,
    findings: list[Finding],
    entity_id: EntityId,
    section: tuple[str, ...],
    rule: str,
) -> bool | None:
    node = values.get(field_name)
    if node is None:
        findings.append(
            _error_finding(
                finding_id=f"missing-field:{entity_id}:{field_name}",
                summary=f"Entity '{entity_id}' is missing required field '{field_name}'.",
                details="The exception schema requires this boolean field.",
                rule=rule,
                location=SourceLocation(path.relative_to(spec_root), section=section),
            )
        )
        return None
    if not isinstance(node, ScalarNode) or node.tag != "tag:yaml.org,2002:bool":
        findings.append(
            _error_finding(
                finding_id=f"invalid-bool:{entity_id}:{field_name}",
                summary=f"Entity '{entity_id}' field '{field_name}' must be true or false.",
                details="Restricted YAML accepts booleans only where the schema explicitly requires them.",
                rule="yaml.boolean-field",
                location=_location_for_node(path, spec_root, node, *section, field_name),
            )
        )
        return None
    return node.value.lower() == "true"


def _optional_bool(
    node: Node | None,
    path: Path,
    spec_root: Path,
    findings: list[Finding],
    entity_id: EntityId,
    section: tuple[str, ...],
) -> bool | None:
    if node is None:
        return None
    if not isinstance(node, ScalarNode) or node.tag != "tag:yaml.org,2002:bool":
        findings.append(
            _error_finding(
                finding_id=f"invalid-optional-bool:{entity_id}:{'.'.join(section)}",
                summary=f"Entity '{entity_id}' field '{section[-1]}' must be true or false.",
                details="Restricted YAML accepts booleans only where the schema explicitly permits them.",
                rule="yaml.optional-boolean-field",
                location=_location_for_node(path, spec_root, node, *section),
            )
        )
        return None
    return node.value.lower() == "true"


def _required_scalar_value(
    values: dict[str, Node],
    field_name: str,
    path: Path,
    spec_root: Path,
    findings: list[Finding],
    entity_id: EntityId,
    section: tuple[str, ...],
    rule: str,
) -> str | None:
    node = values.get(field_name)
    if node is None:
        findings.append(
            _error_finding(
                finding_id=f"missing-field:{entity_id}:{field_name}",
                summary=f"Entity '{entity_id}' is missing required field '{field_name}'.",
                details="This field is required by the restricted PASM schema.",
                rule=rule,
                location=SourceLocation(path.relative_to(spec_root), section=section),
            )
        )
        return None
    return _expect_string(
        node,
        path=path,
        spec_root=spec_root,
        findings=findings,
        finding_id=f"invalid-string:{entity_id}:{field_name}",
        summary=f"Entity '{entity_id}' field '{field_name}' must be a string.",
        rule="yaml.string-field",
        section=section + (field_name,),
    )


def _required_string(
    values: dict[str, Node],
    field_name: str,
    path: Path,
    spec_root: Path,
    findings: list[Finding],
    entity_id: EntityId,
    section: tuple[str, ...],
    rule: str,
) -> str | None:
    return _required_scalar_value(values, field_name, path, spec_root, findings, entity_id, section, rule)


def _optional_string(
    node: Node | None,
    path: Path,
    spec_root: Path,
    findings: list[Finding],
    entity_id: EntityId,
    section: tuple[str, ...],
) -> str | None:
    if node is None:
        return None
    return _expect_string(
        node,
        path=path,
        spec_root=spec_root,
        findings=findings,
        finding_id=f"invalid-optional-string:{entity_id}:{'.'.join(section)}",
        summary=f"Entity '{entity_id}' field '{section[-1]}' must be a string.",
        rule="yaml.optional-string-field",
        section=section,
    )


def _expect_string(
    node: Node,
    path: Path,
    spec_root: Path,
    findings: list[Finding],
    finding_id: str,
    summary: str,
    rule: str,
    section: tuple[str, ...],
) -> str | None:
    if not isinstance(node, ScalarNode) or node.tag != "tag:yaml.org,2002:str":
        findings.append(
            _error_finding(
                finding_id=finding_id,
                summary=summary,
                details="Restricted YAML accepts plain strings for this field.",
                rule=rule,
                location=_location_for_node(path, spec_root, node, *section),
            )
        )
        return None
    return node.value


def _mapping_to_nodes(
    node: MappingNode,
    allowed_fields: set[str] | None,
    findings: list[Finding],
    path: Path,
    spec_root: Path,
    section: tuple[str, ...],
) -> dict[str, Node]:
    pairs: dict[str, Node] = {}
    for key_node, value_node in node.value:
        key = _expect_string(
            key_node,
            path=path,
            spec_root=spec_root,
            findings=findings,
            finding_id=f"invalid-key:{path.name}:{'.'.join(section)}",
            summary="Mapping keys in PASM must be strings.",
            rule="yaml.string-key",
            section=section,
        )
        if key is None:
            continue
        if key in pairs:
            findings.append(
                _error_finding(
                    finding_id=f"duplicate-key:{path.name}:{'.'.join(section)}:{key}",
                    summary=f"Duplicate key '{key}' in '{path.relative_to(spec_root).as_posix()}'.",
                    details="Restricted PASM mappings do not allow duplicate keys.",
                    rule="yaml.duplicate-key",
                    location=_location_for_node(path, spec_root, key_node, *section, key),
                )
            )
            continue
        if allowed_fields is not None and key not in allowed_fields:
            findings.append(
                _error_finding(
                    finding_id=f"unknown-field:{path.name}:{'.'.join(section)}:{key}",
                    summary=f"Unknown field '{key}' in '{path.relative_to(spec_root).as_posix()}'.",
                    details="Phase 0-2 rejects fields that are not part of the restricted PASM schema.",
                    rule="yaml.unknown-field",
                    location=_location_for_node(path, spec_root, key_node, *section, key),
                )
            )
        pairs[key] = value_node
    return pairs


def _node_to_plain_value(
    node: Node,
    path: Path,
    spec_root: Path,
    findings: list[Finding],
    section: tuple[str, ...],
) -> Any:
    if isinstance(node, ScalarNode):
        if node.tag == "tag:yaml.org,2002:str":
            return node.value
        if node.tag == "tag:yaml.org,2002:bool":
            return node.value.lower() == "true"
        findings.append(
            _error_finding(
                finding_id=f"unsupported-scalar:{path.name}:{'.'.join(section)}",
                summary=f"Unsupported scalar type at '{'.'.join(section)}'.",
                details="Phase 0-2 supports only strings and explicit booleans in YAML scalars.",
                rule="yaml.scalar-type-supported",
                location=_location_for_node(path, spec_root, node, *section),
            )
        )
        return None
    if isinstance(node, SequenceNode):
        return [
            _node_to_plain_value(item, path=path, spec_root=spec_root, findings=findings, section=section + (str(index),))
            for index, item in enumerate(node.value)
        ]
    if isinstance(node, MappingNode):
        result: dict[str, Any] = {}
        for key_node, value_node in node.value:
            key = _expect_string(
                key_node,
                path=path,
                spec_root=spec_root,
                findings=findings,
                finding_id=f"invalid-domain-key:{path.name}:{'.'.join(section)}",
                summary="Domain-section mapping keys must be strings.",
                rule="yaml.domain-string-key",
                section=section,
            )
            if key is not None:
                result[key] = _node_to_plain_value(
                    value_node,
                    path=path,
                    spec_root=spec_root,
                    findings=findings,
                    section=section + (key,),
                )
        return result
    findings.append(
        _error_finding(
            finding_id=f"unsupported-node:{path.name}:{'.'.join(section)}",
            summary=f"Unsupported YAML node at '{'.'.join(section)}'.",
            details="Phase 0-2 supports only mappings, sequences, strings, and booleans.",
            rule="yaml.node-supported",
            location=_location_for_node(path, spec_root, node, *section),
        )
    )
    return None


def _reject_unsupported_yaml(node: Node, findings: list[Finding], relative_path: Path) -> None:
    anchor = getattr(node, "anchor", None)
    if anchor:
        findings.append(
            _error_finding(
                finding_id=f"yaml-anchor:{relative_path.name}:{anchor}",
                summary=f"Anchors and aliases are not supported in '{relative_path.as_posix()}'.",
                details="Restricted PASM YAML rejects semantically significant anchors to keep models explicit.",
                rule="yaml.no-anchors",
                location=SourceLocation(path=relative_path, line=node.start_mark.line + 1, column=node.start_mark.column + 1),
            )
        )
    if node.tag not in ALLOWED_YAML_TAGS:
        findings.append(
            _error_finding(
                finding_id=f"yaml-tag:{relative_path.name}:{node.tag}",
                summary=f"Unsupported YAML tag '{node.tag}' in '{relative_path.as_posix()}'.",
                details="Restricted PASM YAML rejects custom tags and unsupported implicit scalar types.",
                rule="yaml.allowed-tags",
                location=SourceLocation(path=relative_path, line=node.start_mark.line + 1, column=node.start_mark.column + 1),
            )
        )
    child_nodes: list[Node] = []
    if isinstance(node, MappingNode):
        for key_node, value_node in node.value:
            child_nodes.extend([key_node, value_node])
    elif isinstance(node, SequenceNode):
        child_nodes.extend(node.value)
    for child in child_nodes:
        _reject_unsupported_yaml(child, findings, relative_path)


def _location_for_node(path: Path, spec_root: Path, node: Node, *section: str) -> SourceLocation:
    return SourceLocation(
        path=path.relative_to(spec_root),
        line=node.start_mark.line + 1,
        column=node.start_mark.column + 1,
        section=tuple(section),
    )


def _error_finding(
    finding_id: str,
    summary: str,
    details: str,
    rule: str,
    location: SourceLocation,
) -> Finding:
    return Finding(
        id=finding_id,
        category=FindingCategory.VIOLATION,
        severity=Severity.ERROR,
        confidence="confirmed",
        summary=summary,
        details=details,
        rule=rule,
        spec_entities=(),
        implementation_locations=(location,),
        evidence=(),
        suggested_resolution=None,
        requires_decision=False,
    )
