from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path

import yaml

from pasm.core.findings import Finding, FindingCategory, Severity
from pasm.core.model import EntityId, SourceLocation, SpecEntity


@dataclass(frozen=True)
class ScenarioStep:
    kind: str
    value: str
    actor: str | None = None
    requires_facts: tuple[str, ...] = ()
    line: int | None = None


@dataclass(frozen=True)
class Scenario:
    id: str
    initial_facts: tuple[str, ...]
    steps: tuple[ScenarioStep, ...]


def load_scenario(path: Path) -> Scenario:
    text = path.read_text(encoding="utf-8")
    raw = yaml.safe_load(text)
    node = yaml.compose(text)
    if not isinstance(raw, dict) or set(raw) != {"scenario"} or not isinstance(raw["scenario"], dict):
        raise ValueError("Scenario YAML must contain exactly one 'scenario' mapping.")
    data = raw["scenario"]
    if set(data) - {"id", "initial_facts", "steps"} or not isinstance(data.get("id"), str) or not isinstance(data.get("steps"), list) or not data["steps"]:
        raise ValueError("Scenario requires an id and a non-empty steps list.")
    initial_facts = data.get("initial_facts", [])
    if not _string_list(initial_facts):
        raise ValueError("Scenario initial_facts must be a list of strings.")
    steps = []
    scenario_node = node.value[0][1] if node is not None else None
    step_nodes = next(
        (value_node.value for key_node, value_node in scenario_node.value if key_node.value == "steps"),
        [],
    ) if scenario_node is not None else []
    for item, item_node in zip(data["steps"], step_nodes):
        if not isinstance(item, dict) or set(item) - {"kind", "value", "actor", "requires_facts"} or item.get("kind") not in {"action", "reveal", "fail", "recover"} or not isinstance(item.get("value"), str) or not _string_list(item.get("requires_facts", [])):
            raise ValueError("Each scenario step must be a known kind with a string value.")
        steps.append(ScenarioStep(kind=item["kind"], value=item["value"], actor=item.get("actor"), requires_facts=tuple(item.get("requires_facts", [])), line=item_node.start_mark.line + 1))
    return Scenario(id=data["id"], initial_facts=tuple(initial_facts), steps=tuple(steps))


def validate_scenario(scenario: Scenario, entities: tuple[SpecEntity, ...], source: Path) -> list[Finding]:
    index = {entity.id.value: entity for entity in entities}
    facts, failed, findings = set(scenario.initial_facts), set(), []
    for position, step in enumerate(scenario.steps, start=1):
        location = SourceLocation(source, line=step.line, section=("scenario", "steps", str(position - 1)))
        entity = index.get(step.value)
        if step.kind == "action":
            if entity is None or entity.kind not in {"verb", "action"} or entity.game_design is None or entity.game_design.owner_role is None or entity.game_design.owner_role.value != step.actor:
                findings.append(_finding(f"scenario-wrong-role-action:{scenario.id}:{step.value}:{position}", f"Actor '{step.actor}' cannot perform action '{step.value}'.", "scenario.role-access", location))
            else:
                missing = set(step.requires_facts) - facts
                if missing:
                    findings.append(_finding(f"scenario-action-preconditions:{scenario.id}:{step.value}:{position}", f"Action '{step.value}' occurs before its conditions: {', '.join(sorted(missing))}.", "scenario.action-preconditions", location))
                else:
                    facts.update(entity.game_design.produces_facts)
        elif step.kind == "reveal":
            if entity is None or entity.kind not in {"information", "information_set"} or entity.game_design is None:
                findings.append(_finding(f"scenario-unknown-information:{scenario.id}:{step.value}:{position}", f"Unknown information '{step.value}'.", "scenario.information-exists", location))
                continue
            missing = set(entity.game_design.reveal_conditions) - facts
            if missing:
                findings.append(_finding(f"scenario-premature-reveal:{scenario.id}:{step.value}:{position}", f"Information '{step.value}' is revealed before its conditions: {', '.join(sorted(missing))}.", "scenario.information-reveal", location))
        elif step.kind == "fail":
            if entity is None or entity.kind not in {"failure", "failure_state"}:
                findings.append(_finding(f"scenario-unknown-failure:{scenario.id}:{step.value}:{position}", f"Unknown failure '{step.value}'.", "scenario.failure-exists", location))
            else:
                failed.add(step.value)
        elif step.kind == "recover":
            if entity is None or entity.kind not in {"failure", "failure_state"}:
                findings.append(_finding(f"scenario-unknown-failure:{scenario.id}:{step.value}:{position}", f"Unknown failure '{step.value}'.", "scenario.failure-exists", location))
            elif step.value not in failed:
                findings.append(_finding(f"scenario-recovery-without-failure:{scenario.id}:{step.value}:{position}", f"Recovery for '{step.value}' occurs before the failure.", "scenario.failure-recovery", location))
            else:
                failed.remove(step.value)
    findings.extend(_reachability_findings(scenario, index, source))
    return findings


def _reachability_findings(scenario: Scenario, index: dict[str, SpecEntity], source: Path) -> list[Finding]:
    """Check monotonic authored-fact reachability without relying on step order."""
    reachable = set(scenario.initial_facts)
    actions = [
        (position, step, index.get(step.value))
        for position, step in enumerate(scenario.steps, start=1)
        if step.kind == "action"
    ]
    changed = True
    while changed:
        changed = False
        for _, step, entity in actions:
            if entity is not None and entity.game_design is not None and entity.game_design.owner_role is not None and entity.game_design.owner_role.value == step.actor and set(step.requires_facts) <= reachable:
                before = len(reachable)
                reachable.update(entity.game_design.produces_facts)
                changed = changed or len(reachable) != before

    findings = []
    for position, step, entity in actions:
        missing = set(step.requires_facts) - reachable
        if missing:
            location = SourceLocation(source, line=step.line, section=("scenario", "steps", str(position - 1)))
            findings.append(_finding(
                f"scenario-unreachable-action:{scenario.id}:{step.value}:{position}",
                f"Action '{step.value}' is unreachable from declared initial facts: {', '.join(sorted(missing))}.",
                "scenario.fact-reachability",
                location,
            ))
    return findings


def _string_list(value: object) -> bool:
    return isinstance(value, list) and all(isinstance(item, str) and item for item in value)


def _finding(identifier, summary, rule, location):
    return Finding(id=identifier, category=FindingCategory.VIOLATION, severity=Severity.ERROR, confidence="confirmed", summary=summary, details="Scenario checks use declared roles, facts, information, failures, and recovery only.", rule=rule, spec_entities=(), implementation_locations=(location,), evidence=(), suggested_resolution="Adjust the scenario facts, order, or declared actor.", requires_decision=False)
