from __future__ import annotations

from dataclasses import dataclass, field
from enum import Enum

class InformationVisibility(str, Enum):
    PUBLIC = "public"
    ROLE_VISIBLE = "role-visible"
    TEAM_VISIBLE = "team-visible"
    HIDDEN = "hidden"
    PARTIALLY_KNOWN = "partially-known"
    DERIVED = "derived"
    DELAYED = "delayed"
    UNCERTAIN = "uncertain"


@dataclass(frozen=True)
class GameDesignSection:
    field_locations: dict[str, object] = field(default_factory=dict)
    architecture_links: tuple[EntityId, ...] = ()
    enforcement_links: tuple[EntityId, ...] = ()
    responsibilities: tuple[str, ...] = ()
    player_verbs: tuple[EntityId, ...] = ()
    exclusive_verbs: tuple[EntityId, ...] = ()
    protected_decisions: tuple[EntityId, ...] = ()
    visible_information: tuple[EntityId, ...] = ()
    hidden_information: tuple[EntityId, ...] = ()
    coordination_with: tuple[EntityId, ...] = ()
    expected_decision_frequency: str | None = None
    owner_role: EntityId | None = None
    protected: bool | None = None
    must_not_be: tuple[str, ...] = ()
    visibility: InformationVisibility | None = None
    permitted_viewers: tuple[EntityId, ...] = ()
    reveal_conditions: tuple[str, ...] = ()
    indirect_signals: tuple[str, ...] = ()
    architectural_enforcement: tuple[str, ...] = ()
    participating_roles: tuple[EntityId, ...] = ()
    inputs: tuple[str, ...] = ()
    reads: tuple[EntityId, ...] = ()
    changes: tuple[EntityId, ...] = ()
    eligibility: tuple[str, ...] = ()
    costs: tuple[str, ...] = ()
    resolution: str | None = None
    outputs: tuple[str, ...] = ()
    produces_facts: tuple[str, ...] = ()
    failure: tuple[EntityId, ...] = ()
    side_effects: tuple[str, ...] = ()
    information_revealed: tuple[EntityId, ...] = ()
    information_exchanged: tuple[EntityId, ...] = ()
    actions_required: tuple[EntityId, ...] = ()
    intended_player_effect: str | None = None
    implementation_path: tuple[str, ...] = ()
    sources: tuple[str, ...] = ()
    sinks: tuple[str, ...] = ()
    capacity: str | None = None
    pressure_intent: tuple[str, ...] = ()
    causes: tuple[str, ...] = ()
    consequences: tuple[str, ...] = ()
    affected_roles: tuple[EntityId, ...] = ()
    visible_to: tuple[EntityId, ...] = ()
    terminal: bool | None = None
    recovery_paths: tuple[str, ...] = ()
    affected_mechanics: tuple[EntityId, ...] = ()
    intended_directional_effect: str | None = None
    bounds: str | None = None
    maturity: str | None = None
    supporting_evidence: tuple[str, ...] = ()
    claim: str | None = None
    supports: tuple[EntityId, ...] = ()
