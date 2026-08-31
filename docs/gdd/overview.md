# Last Aeon — Game Design Overview

| Field | Value |
| --- | --- |
| Document status | Draft 0.1 |
| Scope | High-level master GDD overview |
| Game title | *Last Aeon* |
| Current playable scenario | The Ashkarr Succession |
| Primary design authority | `pasm/spec/` |
| Setting authority | `the_last_aeons/` |

**GDD navigation:** [Index](README.md) · [First detailed section](01-player-experience-campaign-and-onboarding.md)

This document is the front door to the game design, not a replacement for the
PASM model. It states the player-facing proposition and the relationships
between the game's major systems. Detailed rules, data schemas, edge cases,
implementation mappings, and accepted decisions remain in PASM and the linked
feature specifications indexed below.

## The proposition

*Last Aeon* is a pauseable-real-time, character-led grand strategy game
about personal rule in a far-future, science-fantasy society. The player guides
an enduring house—or, in the finished game, another eligible organisation—by
acting through its current head and household. They pursue political,
territorial, military, and personal ambitions through relationships, titles,
offices, assignments, armies, ships, and succession.

The central promise is that power belongs to people who must act somewhere,
through someone, over time. The player does not command an abstract state with
instantaneous reach. Leaders travel, orders can be delayed, capable characters
are finite, delegation matters, and every reign eventually ends.

## Game at a glance

| Dimension | Overview |
| --- | --- |
| Genre | Character-led grand strategy; political and dynastic simulation |
| Setting | Far-future science fantasy and space opera in *Last Aeon* |
| Player identity | An organisation that persists across successive leaders |
| Acting unit | Characters exercising the authority, resources, and relationships available to them |
| Time | Pauseable real-time presentation over an authoritative daily simulation |
| Strategic space | Worlds, moons, orbital habitats, provinces, ships, and scarce routes between star systems |
| Primary interaction | Read the political situation, choose an assignment, choose who will lead it, weigh the forecast, and commit |
| Campaign shape | Persistent and open-ended, driven by player ambitions and systemic consequences rather than a fixed mission chain |
| Current delivery | Native desktop and browser clients using the same authoritative simulation |
| Current play modes | Player-led campaign and autonomous spectator simulation |

## Player fantasy

The player is the person at the centre of a vulnerable political household:
powerful enough to shape events, but never powerful enough to ignore distance,
loyalty, legitimacy, obligation, or mortality. They should feel that they are:

- ruling through a particular leader whose abilities, location, relationships,
  and lifespan materially constrain what can be done;
- building a house that can outlast that leader through heirs, offices,
  holdings, alliances, servants, ships, and armies;
- reading rivals as purposeful people with pressures and ambitions of their
  own, rather than as passive bonuses or arbitrary event generators;
- choosing which risks to take after seeing their likely costs, delays, and
  consequences;
- turning a precarious place in the political order into security, influence,
  independence, or supremacy by means of their own choosing.

## Experience pillars

These are a synthesis of the accepted PASM vision and the behaviour already
present in the game. They describe the intended player experience; the
repository's separate engineering pillars are summarised later.

### 1. Power is personal

Organisations persist, but characters act. A house's current head bears its
authority, household members can be delegated work, and succession changes the
person through whom the player rules. Titles, offices, claims, favours, and
grievances attach to the appropriate person or organisation rather than being
collapsed into a single national score.

### 2. Intent becomes action through assignments

Assignments are the game's main verbs. Political courtship, administration,
intrigue, travel, mustering, warfare, negotiation, and claims all use the same
broad pattern: choose an action, its target, and an eligible leader; understand
the requirements and forecast; then let it unfold through authored phases and
outcomes.

### 3. Distance and delegation matter

Characters, armies, and ships have physical presence. Travel consumes time,
orders can arrive late, and one person cannot be everywhere. Strategic reach
therefore depends on preparation, trusted delegates, transport, local forces,
and control of scarce routes.

### 4. Consequences are legible

Before committing, the player should be able to see the known costs, delays,
risks, and result distribution. Afterwards, the map, Situations, ledgers, and
chronicle should show what changed and why. Uncertainty may remain, but the
rules should not be opaque.

### 5. The world acts back

Other houses and institutions operate through the same characters, resources,
assignments, plans, goals, and political constraints as the player. Their
actions create a continuing field of opportunities, obligations, threats, and
conflicts without waiting for a scripted quest sequence.

## Core play loop

**Observe → choose an aim → assign a person → commit → advance time → absorb the
consequences → reassess.**

1. **Observe.** Read the strategic map, active Situations, political and
   economic ledgers, current assignments, plans, goals, and recent results.
2. **Choose an aim.** Decide which opportunity, obligation, threat, or longer
   ambition deserves scarce attention and resources.
3. **Assign a person.** Select an eligible character and target. Compare skill,
   presence, availability, cost, delay, and personal risk.
4. **Commit.** Issue a validated order. Important choices enter the
   authoritative command log and may take effect only after an order delay.
5. **Advance time.** Assignments progress while every other organisation also
   acts. Travel, economy, politics, plans, Situations, and warfare continue to
   develop.
6. **Resolve and adapt.** Outcomes alter relationships, obligations, resources,
   order, control, health, military position, and future opportunities. The
   player responds, delegates again, or changes direction.
7. **Survive succession.** Death or replacement changes the acting leader. The
   organisation endures, but personal claims, relationships, capabilities, and
   priorities may not.

The loop operates at several connected scales: day-to-day assignments,
month-to-month political and economic pressures, multi-step plans, ambitions
that can span a reign, and an organisation's survival across generations.

## Strategic structure

The finished campaign is envisioned as a compact network of fewer than ten
star systems linked by strategically scarce Maelstrom Gates. Systems contain
worlds and orbital habitats; their politically meaningful territory is divided
into provinces. Characters, armies, and individually tracked ships move through
this geography rather than existing only as values attached to a faction.

The setting is the far-future Milky Way of *Last Aeon*: humanity and its
descendants live among successor polities, ancient infrastructure, posthuman
peoples, alien powers, and the entities or divinities associated with the
Maelstrom. Setting truth belongs to the worldspec; the GDD defines how selected
parts of that canon become play.

## The current playable slice: The Ashkarr Succession

The current game deliberately proves the central political loop inside one
hand-authored local system before expanding to the full interstellar vision.

The player leads **House Harrow**, a legitimate minor dynastic house and vassal
of Great House Veyrin. The previous Paramount of Ashkarr has died without an
accepted successor. Three Great Houses—Veyrin, Draksha, and Meloch—contest the
planet, while their vassals, two independent houses, and the rules-distinct
Sanctora Imperim pursue their own interests. House Harrow begins with a credible
path to grow within the hierarchy, break for independence, or eventually press
a personal claim to the vacant Paramountcy.

| Current slice | Scope |
| --- | --- |
| Map | One local system: a 32-province world, an 8-province moon, and the single-province Aurelian Spire starbase |
| Political field | Three Great Houses, eight vassal houses, two independent houses, and the Sanctora Imperim |
| Playable identity | Fixed House Harrow and its current head |
| Central crisis | A vacant, personally claimed, non-inherited planetary Paramountcy |
| Major pressures | Succession, allegiance, legitimacy, relationships, favours and grievances, provincial order, resources, travel, formal war, and Consular politics |
| Military model | Persistent armies and ships; strategic operations resolve from campaign state rather than tactical battles |
| Campaign objective | Open-ended; no scripted victory condition in the current slice |

## Current slice and finished-game vision

| Dimension | Current Ashkarr slice | Finished-game direction |
| --- | --- | --- |
| Campaign map | One world, moon, and starbase in one local system | Fewer than ten star systems linked by Maelstrom Gates |
| Player organisation | Fixed minor dynastic House Harrow | A chosen house or other eligible organisation with its own political and succession rules |
| Travel | Provincial and local-space movement | Local and interstellar movement shaped by scarce gate routes |
| Setting systems | A grounded succession crisis | Wider use of Precursor technology, Maelstrom infrastructure, and the setting's cosmic scale where appropriate |
| Political horizon | Survival, advancement, independence, and the Paramountcy | Open-ended personal, organisational, territorial, political, and military ambitions across systems |
| Content breadth | One fixed authored scenario | Multiple organisations, places, situations, assignments, and campaign possibilities supported by reusable data schemas |

## Major system relationships

The game is organised around a reinforcing chain rather than a collection of
independent features:

**Map, ledgers, and Situations** reveal pressure → **the player or AI chooses an
aim** → **a character leads an assignment or plan** → **presence, skill,
relationships, resources, and forces shape the forecast** → **time and authored
rules resolve the action** → **effects change people, politics, territory,
economy, order, and war** → **those changes create new Situations and aims**.

Succession cuts across the whole chain. It preserves the organisation while
changing the leader, invalidating some personal claims and relationships, and
reframing which actions are possible or desirable.

## Presentation and information

The strategic view moves between a star-system map, rotatable political globes,
and two-dimensional information panels. Provinces are readable through map
modes for ownership and other strategic facts; armies and ships appear where
they are physically present. The interface combines persistent identity and
time controls with configurable panels for inspection, assignments,
Situations, forecasts, ledgers, plans, goals, and the campaign log.

The presentation should preserve three information promises:

- the player can tell **who they are**, **where their people and forces are**,
  and **what currently demands attention**;
- every available action explains eligibility, cost, delay, forecast, and risk
  from the same authoritative rules that will resolve it;
- important consequences remain inspectable after they occur rather than
  disappearing as transient notifications.

## Scope boundaries

The overview intentionally does not make detailed rules for individual
assignments, warfare, succession, economy, AI, UI, content, or balance. Those
belong in linked system specifications where rules, data, edge cases,
dependencies, presentation, and acceptance criteria can remain precise.

The current Ashkarr slice specifically excludes:

- Maelstrom Gates and interstellar travel;
- Precursor technology and cosmic entities as gameplay systems;
- tactical battle control;
- a scripted victory condition;
- additional maps, factions, or scenarios merely to increase breadth before
  the existing simulation is sufficiently deep and legible.

## Technical and production pillars

These constraints are part of the game's design contract because they govern
what content can promise and how campaign behaviour is verified.

- **Deterministic.** A campaign seed, validated authored data, and ordered
  player commands reproduce the campaign. Saves combine versioned snapshots
  with an append-only command log and can be verified by state hash.
- **Headless-authoritative.** The simulation owns the rules and runs without a
  renderer. Native and browser clients present the same campaign model.
- **Data-driven.** Authored Rhai content reads validated context and emits typed
  effects. Scripts do not mutate simulation state directly.

## Overview-level questions still open

The current sources do not yet settle the following product-level questions.
They should be decided before this overview is treated as final:

- the primary audience and the level of prior grand-strategy familiarity
  assumed by onboarding;
- the intended session rhythm and typical campaign length;
- whether the finished game retains a purely open-ended campaign or adds
  optional ambitions, scoring, retirement, or other end states;
- final platform targets beyond the existing native desktop and browser builds;
- accessibility targets and minimum supported input, display, and localisation
  requirements;
- how many authored starting scenarios and playable organisation types define
  the first full release.

## Source map

- `pasm/spec/core/game-vision.yaml` — accepted finished-game and MVP vision,
  player identity, campaign map, simulation, persistence, and content model.
- `pasm/spec/roadmap/` — implemented and planned vertical slices that deepen
  the playable scenario.
- `assets/content/scenario/ashkarr-succession.rhai` — the current scenario's
  authored political field, characters, forces, titles, and obligations.
- `assets/content/core/` — the assignment, Situation, plan, goal, economy,
  intrigue, and warfare catalogues used by the simulation.
- `crates/aeon_sim/` — the authoritative behaviour against which player-facing
  claims in this document can be checked.
- `crates/aeon_client/` — the native and browser presentation of the same
  simulation.
- `the_last_aeons/` — setting canon and provenance.

## Detailed GDD sections

This overview should remain short as the design grows. Detailed rules and
evidence live in these linked documents:

1. [Player experience, campaign structure, and onboarding](01-player-experience-campaign-and-onboarding.md)
2. [Characters, organisations, relationships, titles, offices, and succession](02-characters-organisations-politics-and-succession.md)
3. [Assignments, forecasts, phases, results, plans, and goals](03-assignments-forecasts-plans-and-goals.md)
4. [Map, presence, travel, orders, ships, and armies](04-map-presence-travel-ships-and-armies.md)
5. [Economy, provincial order, obligations, Situations, intrigue, and warfare](05-economy-order-obligations-situations-intrigue-and-warfare.md)
6. [AI agency and information rules](06-ai-agency-and-information-rules.md)
7. [Interface, accessibility, art, audio, and narrative presentation](07-interface-accessibility-art-audio-and-narrative.md)
8. [Content schemas, balance, verification, and acceptance](08-content-balance-verification-and-acceptance.md)

Each system specification follows the chain recommended by the GDD
guide: **purpose → player experience → rules → data → edge cases → feedback →
dependencies → acceptance criteria → evidence**.
