# Last Aeon — AI Agency and Information Rules

| Field | Value |
| --- | --- |
| Document status | Draft 0.1 |
| Scope | Autonomous character decisions, organisational intent, and the information boundary presented to players and spectators |
| Primary design authority | `pasm/spec/` |
| Current implementation | `crates/aeon_sim/`, `crates/aeon_client/`, and `assets/content/core/` |

**GDD navigation:** [Index](README.md) · [Overview](overview.md) · [Previous: Economy, Order, Obligations, Situations, Intrigue, and Warfare](05-economy-order-obligations-situations-intrigue-and-warfare.md) · [Next: Interface, Accessibility, Art, Audio, and Narrative](07-interface-accessibility-art-audio-and-narrative.md)

This section describes how the world acts without the player and what the
player is allowed to learn about that activity in *Last Aeon*. It is a
player-facing account of the current design, not a replacement for PASM or the
authoritative simulation.

## Status language

- **Implemented** describes behaviour present in the current code and supported
  by tests or direct implementation evidence.
- **Accepted design** describes a confirmed PASM decision. In this section,
  most accepted rules are also implemented unless stated otherwise.
- **Proposal** or **open question** describes an option for later discussion.
  It is not a commitment and does not amend PASM.

## Purpose and experience promise

Autonomous organisations should feel like political actors with comprehensible
needs, persistent ambitions, and limited people, not like event generators or
an opponent with a private command language. The player should be able to
recognise why another house acted, anticipate some of what it may do next, and
trust that it paid the same costs and passed the same rules they would have.

The intended experience follows four promises:

1. **A person acts.** An organisation supplies authority and resources, but a
   living character makes and carries out the decision.
2. **Pressure becomes purpose.** Observable political, territorial, economic,
   and military facts create scored pressures; plans and goals turn the
   strongest pressures into sustained action.
3. **The rules are symmetrical.** Autonomous action uses the authored
   assignment catalogue, leader availability, targets, costs, and validation
   used by player orders.
4. **Intent is legible, but not falsely certain.** Significant reasoning,
   plans, Situations, and consequences are exposed through inspectors and
   permanent history. The random tie-break between plausible choices is not
   narrated as inevitability.

The current information model is deliberately open. It contains authored
private Situation audiences and limited hostile-plan rumours, but it is not a
general fog-of-war, secrecy, espionage, or plot-detection simulation.
[ai] One narrow, reusable exception now exists: authored **covert
provenance**. A plan, goal, or assignment definition may declare
`covert: true`, and before exposure every ordinary player surface narrows
that work's provenance to its owning organisation. This is a visibility
capability carried on ordinary log audiences, not a fog-of-war system:
spectators and replay retain everything, and exposure is a single future
read reserved for the investigation slice.

## Who acts, and with whose authority

**Implemented / accepted design.** The autonomous pass walks living characters
in stable ID order. The head of a non-player organisation may spend that
organisation's resources, adopt a goal, adopt or advance a plan, or begin a
single assignment. A head already pursuing a plan is not given an unrelated
reactive action, and a busy character cannot begin a new commitment.

Other household members also possess agency, but within a narrower leash. They
may take up free, untargeted work and may adopt only plans proven to spend no
wealth, manpower, supplies, or influence and to issue no army orders. They
cannot silently consume the house's stores. A duplicate household plan is
also suppressed when another member of the same organisation already pursues
it.

The player house is excluded from autonomous head action during ordinary play:
the player supplies its strategic direction. Household members may still take
up the limited free work allowed by the household rules. In spectator mode
there is no player house, so the former protagonist and every other eligible
organisation act autonomously.

This division preserves the central fiction: houses do not take action in the
abstract. Their heads wield organisational authority, while other characters
have personal initiative within what they can legitimately undertake.

## The pressure scorer

**Implemented / accepted design.** Each autonomous character scores concrete
`AiIntent` pressures over current authoritative state. The current vocabulary
is muster, order, standing, resources, obligation, claim, and routine.
[ai] The First Year intrigue slice adds **subvert**: undermining a rival by
indirect, deniable means. Like the claim pressure, it is exposed head-only
and only while the house's active goal favours it, aimed at the goal's own
resolved target; the engine names no content key, and the authored plan
requirements decide whether a campaign actually mounts. The
engine does not name a preferred assignment for a pressure. Instead, authored
assignments declare whether AI may use them, which intent they answer, and
which target kind they require. A new assignment therefore joins the
repertoire through content rather than an engine exception.

Current scored signals include:

| Pressure | Current evidence and target |
| --- | --- |
| Order | The worst held province by order shortfall; unrest increases urgency |
| Muster | Occupied holdings, or the next force-building step in the Paramount claim campaign |
| Obligation | The heaviest favour owed to the organisation |
| Standing | The heaviest grievance held against it, or low effective legitimacy |
| Resources | Wealth below the current operating floor |
| Claim | The next legal stage of the authored Paramountcy ambition: declaration, challenge, war prosecution, or press |
| Subvert | [ai] The active covert ambition's resolved hostile border neighbour, head-only, carried out by an authored covert plan |
| Routine | Authored untargeted upkeep when nothing more urgent wins |

Scores use integer arithmetic. Candidates are sorted by score and then stable
content key. A one-shot action must meet the current threshold; only the best
three qualifying candidates form the shortlist. One derived random roll,
weighted by score, chooses among that shortlist, so the strongest need usually
wins without making every house mechanically predictable. A separate monthly
check means an idle character does not necessarily start new work every month.

Goals and directives modify the same list. A house goal raises the pressures
it favours; a liege's directive gives a smaller bonus to the matching pressure
felt by its vassal head. Neither creates a candidate that content and current
state did not support, and neither changes assignment validation or outcome
resolution.

The fixed thresholds, shortlist size, cadence, and bonuses are current
implementation values rather than promises that every future balance pass must
preserve. Their design contract is the observable pipeline: state produces
pressures, authored intent maps them to available actions, and deterministic
selection chooses among legal candidates.

## Plans and goals

The full rules for their structure and lifecycle are in
[Assignments, Forecasts, Plans, and Goals](03-assignments-forecasts-plans-and-goals.md).
For agency and information, the important distinction is ownership and
horizon.

**Implemented / accepted design.** A plan belongs to one character and answers
a heavily scored pressure with an authored sequence of assignments or standing
orders. Plan conditions are declarative integer facts over current state.
Dynamic target selectors resolve when a step starts. Each assignment step
still passes ordinary start validation; `ai_available: false` can keep an
action out of the reactive scorer without making it unavailable to a
deliberate authored plan. Death, lost authority, an invalid target, an ended
war, a failed method gate, or expiry abandons the plan and logs why. There is no
hidden replanning search.

A goal belongs to an organisation and may survive succession. It is an
ambition, not another executor. On its monthly cadence an eligible autonomous
head may adopt an authored goal; the goal then biases ordinary pressures so
existing plans and assignments pursue it. Current content provides ambitions
to become Consul, take the planet, and conquer a neighbour. Their triggers,
favoured pressures, targets, priorities, horizons, cooldowns, and directives
are data in `assets/content/core/goals.rhai`.

Plans and goals are therefore different layers of intent:

**world fact → scored pressure → character plan or single action → ordinary
validated assignment**, with an organisational goal acting as a lens over the
pressure score.

## Directives and vassal autonomy

**Implemented / accepted design.** A directive is an advisory wish from a
liege to a direct vassal. It raises one named pressure in the vassal head's own
scoring and never compels an assignment, selects a method, supplies resources,
or bypasses validation.

There are two sources:

- a goal-derived directive exists exactly while the liege holds the goal that
  authors it and is recomputed from that state;
- a manually issued directive is stored because no goal exists from which to
  derive it, with at most one such directive per vassal.

The simulation merges both sources for scoring and presentation. Goal-derived
directives lapse when the goal ends. A stored directive is ignored unless its
recorded issuer remains the recipient's liege.

Player issue and clear commands are authorised by direct chain of command,
not by ownership. They are revalidated when applied. The organisation
inspector shows what the player's liege asks of them and lets the player press
or withdraw the currently supported untargeted wishes on a direct vassal. The
vassal remains free to choose a stronger pressure or to do nothing.

## Shared rules and player symmetry

**Implemented / accepted design.** Autonomous single actions call the same
`validate_start` and `start_assignment` functions as player-issued work. Plan
steps call the same validation, including the exact formal-war context where
needed. Household work does likewise. As a result, AI actors share the
player's rules for:

- membership, adulthood, life, health, availability, command posts, and
  presence;
- authored target kinds and relational requirements;
- wealth, manpower, supplies, and influence costs;
- order delay, duration, phases, risks, outcomes, effects, and strategic
  revalidation;
- formal-war identity, standing orders, and authoritative state changes.

The AI receives no alternate outcome table and no privileged write path into
simulation state. Rhai result content receives validated context and returns
typed effects for the simulation to apply. This symmetry is about rules, not
identical interfaces: the player deliberately composes orders, while
autonomous actors use pressure scoring and authored plans to decide what to
attempt.

## Knowledge, visibility, and audiences

### Current authoritative knowledge boundary

**Implemented.** Agency, plan conditions, and goal triggers read current
authoritative campaign facts through deterministic, read-only accessors. Rhai
Situations receive a stable semantic world view rather than raw ECS access.
This is a content and determinism boundary, not a per-character memory or
belief model. The current AI does not maintain uncertain estimates, stale
reports, discovered secrets, or separate knowledge states for different
characters.

Forecasts likewise describe the authoritative current inputs used by the
simulation. They may distinguish a later conditional military contest whose
state can change before resolution, but the current system does not degrade a
rival forecast according to intelligence quality.

### Situation visibility

**Implemented / accepted design.** A Situation definition authors either a
public audience or a bound audience. A bound audience resolves named bindings
such as source, debtor, creditor, or another participant to concrete
organisations for that exact lifecycle. The current favour-debt Situation is
private to its two parties. Situation cards, warnings, resolutions, and
actions are hidden from an ordinary player outside the audience.

The audience is captured onto Situation-linked log entries when they are
written. Ending the Situation or later changing participants therefore does
not turn private history public. Assignment and engine-operation lines derived
from that Situation inherit its occurrence provenance, audience, and exact war
where applicable.

### Logs, inspectors, and explanation

**Implemented / accepted design.** Significant autonomous behaviour explains
itself through the same reasons that caused it:

- a pressure-driven one-shot action logs the organisation, assignment, scored
  reason, and navigable subject;
- routine upkeep normally speaks through its result rather than adding a
  second reasoning line;
- plan adoption, completion, abandonment, goal adoption, achievement, and
  setting-aside are logged;
- the character inspector openly names an autonomous character's active plan;
- a plan aimed at the player organisation or one of its holdings creates a
  deterministic rumour naming the acting house and broad target, without
  disclosing a probabilistic detection roll or full method;
- the organisation inspector exposes liege and vassal directives relevant to
  the player;
- notable outcomes enter dated, filterable channels and may link back to their
  character, organisation, or province.

The explanation deliberately states the pressure and chosen action, not the
hidden random roll that broke a close choice. The result is an honest reason
without pretending the selected response was the only possible one.

[ai] **The covert exception.** Two of the rules above — the character
inspector openly naming an autonomous character's active plan, and the
guaranteed rumour when a plan is aimed at the player — are hereby
qualified, not repealed, for authored covert work before exposure:

- a plan, goal, or assignment authored `covert: true` writes its adoption,
  progress, result, abandonment, and completion lines with an owner-only
  audience through the same recorded-audience mechanism private Situations
  use — the history is complete and spectator-visible, never rewritten;
- a covert plan produces **no** rumour, however squarely it is aimed at the
  player: deniability is the category's meaning, and discovery belongs to
  the investigation slice rather than to a free whisper;
- the inspector renders no pursuing line for a covert plan to any ordinary
  player; spectators still read it openly;
- what the targeted player receives instead is the ordinary **Unquiet
  Holdings** Situation: the targeted province, its live Order, the exact
  resistance that Order applies to the hostile work, and the time
  remaining — with the culprit organisation structurally bound into the
  lifecycle (for spectators, replay, and future investigation) but never
  in the audience and never emitted by the projection;
- exposure is one deliberate seam (`covert::is_exposed`), a stub that
  returns false until investigation gives it authoritative state. Every
  *simulation-side* covertness read passes through that seam — the plan
  lines in `plans.rs`, the goal lines in `goals.rs`, every
  assignment-derived line in `assignments.rs`, and the covert flag on the
  semantic world view scripts read in `script_world.rs` — so exposure
  flips all of those at once. The client's inspector pursuing-line gate is
  the one surface that does not: its panel context holds no `World`, so it
  gates on the authored `covert` flag directly. Its behaviour before
  exposure is identical, and the investigation issue that gives exposure
  authoritative state must also project that state client-side and rewire
  the gate to read it.

## Spectator rules

**Implemented / accepted design.** Spectator mode is stored as the absence of
a player organisation in campaign political state. It survives snapshot and
restore. The simulation then treats every eligible house, including House
Harrow, as autonomous.

A spectator may observe every Situation and every log entry, including
authored private audiences. The Situations surface still displays projected
cards and history, but supplies no actionable forecast or order control. More
generally, commands that require player authority fail because no player
organisation exists. Spectator omniscience is an observation and debugging
rule, not evidence that all organisations know all private information.

## Data and persistence

| Data | Owner and lifetime | Persistence and ordering |
| --- | --- | --- |
| Scored intent | Derived for one character and agency pass | Not stored; rebuilt from current state in stable order |
| Active plan | One character | Stored with flattened steps, target, reason, progress, retries, and cooldowns in character-keyed `BTreeMap`s |
| Active goal | One organisation | Stored with adopting head, resolved target, start date, and cooldowns in organisation-keyed `BTreeMap`s |
| Goal directive | Liege goal | Derived, not stored; ends with the goal |
| Manual directive | Direct liege-to-vassal wish | Stored one per vassal and rechecked against current hierarchy |
| Situation audience | Authored bindings resolved for one lifecycle | Captured as stable organisation IDs on permanent log entries |
| Covert audience | [ai] Derived from the authored `covert` flag when a covert line is written | [ai] Captured as an owner-only organisation audience on the entry itself; no separate snapshot state, spectators and replay read all |
| Explanation log | Campaign history | Stored chronologically with date, channel, organisation, subject, Situation occurrence, war, and audience |
| Spectator identity | Campaign political state | Stored as no player organisation and restored as such |

Agency randomness uses frozen derived stream identities, including
`"character-agency"`, `"plan-method"`, and `"grand-goal"`, with stable subject
IDs and monthly epochs. Stable entity and content ordering plus integer scoring
make equivalent campaigns replay identically. Stream labels are historical
identities and must not be renamed as a balance or copy-editing change.

## Edge cases and failure rules

- A dead character, defunct organisation, busy leader, or character already
  pursuing a plan takes no incompatible new action.
- A candidate below the one-shot threshold is ignored; no legal candidate
  means no action.
- A chosen assignment that fails current validation does not start. The AI
  receives no bypass or substitute effect.
- A plan may wait on a temporary block until its maximum lifetime, but a
  strategic permission failure abandons it rather than preserving obsolete
  authority.
- A character plan ends with the character or their authority; an organisation
  goal normally survives succession.
- A goal or directive can only bias a pressure that exists in the current
  candidate list. Bias is not compulsion.
- An organisation with no liege receives no directive. A manual directive from
  a former liege is ignored.
- A malformed Situation fails content validation. A runtime projection error
  leaves an unavailable, non-actionable card and a deterministic log-once
  diagnostic rather than silently hiding the underlying conflict.
- An empty bound Situation audience is hidden from all ordinary players but
  remains visible to spectators and replay inspection.
- Private log visibility is fixed when the entry is written; a later political
  change does not rewrite history's audience.
- A hostile plan rumour is a guaranteed, coarse notice when its target concerns
  the player. It is not a successful detection check and reveals no espionage
  statistic.
- [ai] A covert plan whispers no rumour and its lines confide only in their
  owner; the targeted holder learns of the work through the Unquiet
  Holdings Situation, which names the ground and the resistance but never
  the hand.
- [ai] A covert operation whose agent dies, whose plan is abandoned by
  reconciliation, or whose target province changes hands ends through the
  ordinary assignment, plan, and Situation-lifecycle rules — and every
  line those endings write keeps the owner-only audience, so even a failed
  covert operation names nobody on the way out.
- [ai] A covert campaign's method gate is rechecked before each step like any
  plan's: a relationship lifted above the authored hostility floor abandons
  an uncommitted covert campaign, while work already accepted runs to its
  ordinary resolution.

## Feedback requirements

**Implemented baseline.** The player can inspect autonomous plans, read
pressure-based reasoning and goal lifecycle lines, follow notable outcomes by
channel and subject, see relevant directives, and receive a coarse rumour when
a plan concerns them. Situation cards expose authored stages, metrics,
participants, warnings, links, related history, actions, and authoritative
forecasts only to their audience. Runtime content faults remain visibly
unavailable rather than disappearing.

**Proposal, not accepted.** A future explanation view could show a compact
causal chain from goal and directive bonuses through the winning pressure to
the plan or assignment actually chosen. If pursued, it should disclose exact
rules without exposing a random roll as destiny or overwhelming the player
with every rejected candidate.

## Dependencies

- Characters, organisation membership, heads, hierarchy, succession,
  relationships, titles, and offices determine who acts and what authority
  they wield.
- Assignment validation, forecasts, phases, outcomes, plans, and goals provide
  the only ordinary path from selected intent to simulation change. See
  [Assignments, Forecasts, Plans, and Goals](03-assignments-forecasts-plans-and-goals.md).
- Order, resources, obligations, Situations, intrigue, formal war, and claims
  supply the pressures and facts the scorer reads. See
  [Economy, Order, Obligations, Situations, Intrigue, and Warfare](05-economy-order-obligations-situations-intrigue-and-warfare.md).
- Presence and forces constrain leader availability, order delay, military
  targets, and standing orders.
- The client inspector, Situations panel, forecast views, attention links, and
  message log present information without owning simulation truth. See
  [Interface, Accessibility, Art, Audio, and Narrative](07-interface-accessibility-art-audio-and-narrative.md).
- Validated Rhai content defines assignment intent, plans, goals, Situation
  audiences, projections, and effects; the string table supplies displayed
  reasons and labels.
- Stable IDs, campaign time, derived RNG, snapshots, the command log, and state
  hashes provide deterministic replay.

## Acceptance criteria

The current design contract is met when:

1. every autonomous deed is attributed to a living character acting within
   their organisational or household authority;
2. pressures are derived from authoritative state with integer arithmetic,
   stable ordering, authored intent mappings, and replay-stable random streams;
3. heads may spend and direct forces only through ordinary rules, while
   non-head autonomy is structurally spend-free and cannot command armies;
4. autonomous assignments, plan steps, Situation actions, and standing orders
   pass the same validation and resolution paths used by the player;
5. goals steer existing scoring without becoming a second executor and remain
   organisation-owned across succession;
6. directives are advisory, direct-hierarchy-aware, merged from derived and
   manual sources, and never make an illegal act possible;
7. a significant action's explanation uses the actual scored reason, while
   routine noise and random tie-breaks are not misrepresented;
8. active plans, hostile-plan rumours, goals, directives, and notable results
   remain inspectable through their implemented surfaces;
9. public and bound Situation audiences gate cards, actions, resolutions, and
   permanent related history consistently;
10. spectators restore as spectators, activate every eligible organisation,
    see all audiences, and cannot issue gameplay actions;
11. snapshots and replays preserve plans, goals, manual directives, audiences,
    log history, spectator identity, and frozen RNG stream identities;
12. no current feature is described as fog of war, secrecy, espionage, or plot
    detection when the simulation does not implement that model;
13. [ai] authored covert work keeps its provenance — culprit, organisation,
    leader, and source plan — out of every ordinary player surface (cards,
    inspector, plan naming, logs, card history, notifications) before
    exposure, while spectators, snapshots, and replay verification retain
    it completely and the targeted holder still receives the authored
    Situation with target, live Order, resistance, and remaining time.

## Explicit exclusions and open questions

### Excluded from the current design

Milestones 5 and 6 explicitly exclude an espionage or plot-detection system.
Plans speak openly in inspectors, and a plan concerning the player produces a
coarse guaranteed rumour. [ai] (Both statements now carry the covert
exception recorded above: an authored covert plan is neither named nor
rumoured before exposure.) There is no general fog of war, secret-action
detection chance, actor-specific belief state, misinformation, or gradual
intelligence collection in the current implementation. [ai] Covert provenance
is deliberately none of those: it hides who is behind authored covert work,
never that the work's consequences are happening. The mere mention of
`secrets` as a possible future specific relationship fact in PASM does not
define such a system.

### Open questions, not commitments

- Should a future campaign retain the current broadly open political model, or
  introduce actor-specific knowledge and stale reports at larger map scales?
- If secrecy is added, which facts may be hidden without making authoritative
  forecasts misleading or autonomous decisions appear to cheat?
- Should hostile-plan rumours remain guaranteed and coarse, become conditional
  on an explicit information system, or coexist with both public and covert
  plan categories?
  [ai] **Resolved:** they coexist. Ordinary hostile plans keep the
  guaranteed coarse rumour unchanged; authored covert plans produce no
  rumour at all, and their discovery is the investigation slice's work.
  See the covert-provenance decision in
  `pasm/spec/architecture/implementation-decisions.yaml`.
- Which actions, if any, should be intrinsically private outside a Situation's
  authored audience, and how should their later consequences enter public
  history?
  [ai] **Resolved for the current slice:** privacy is authored per
  definition (`covert: true`), not intrinsic to an action kind, and the
  consequences were never private — the Order drop, the Unquiet Holdings
  card, and any exposure grievance enter ordinary history normally; only
  the provenance lines are owner-confided, and they enter public view
  when exposure later flips the one covert seam.
- How much of pressure scoring should the inspector expose: the selected
  reason only, ranked factors, or exact bonuses from goals and directives?
- Should ordinary players ever receive access to an omniscient replay or
  post-campaign history view, clearly separated from in-campaign knowledge?
- As the number of organisations grows, what log and notification limits keep
  autonomous intent legible without turning every monthly choice into noise?

Any answer that creates an epistemic model, covert action rules, or a new
information economy requires a PASM decision before implementation. This GDD
does not choose among those options.

## Evidence

### PASM

- `pasm/spec/core/game-vision.yaml` — organisation agency, shared assignment
  rules, forecasts, notable results, and the authored Situation system.
- `pasm/spec/architecture/implementation-decisions.yaml` — authored AI
  intents, characters as acting units, data-only plans, validation symmetry,
  goals as scoring lenses, derived directives, chain-of-command authority,
  Situation audiences, and spectator presentation.
- `pasm/spec/roadmap/milestone-5-persons-and-plans.yaml` — character agency,
  plan runtime, explanations, inspectors, and hostile-plan rumours.
- `pasm/spec/roadmap/milestone-6-plans-reach-the-field.yaml` — household plans,
  force orders, target selectors, and the explicit exclusion of espionage and
  plot detection.
- `pasm/spec/roadmap/milestone-7-the-front-door.yaml` and
  `milestone-8-grand-strategy.yaml` — spectator identity, goals, directives,
  and player-vassal authority.

### Simulation and tests

- `crates/aeon_sim/src/agency.rs`, `plans.rs`, and `goals.rs` — pressure
  construction, selection, household limits, plans, ambitions, directives,
  explanations, persistence, and deterministic streams.
- [ai] `crates/aeon_sim/src/covert.rs` — the covert-provenance reads: the
  authored flag, the owner-only audience, and the `is_exposed` seam the
  investigation slice will fill.
- `crates/aeon_sim/src/assignments.rs`, `forecast.rs`, `command.rs`, and
  `access.rs` — shared validation, resolution, log audiences, forecast rules,
  command authority, stable reads, and dated history.
- `crates/aeon_sim/src/situations.rs` — visibility, bound audience resolution,
  provenance, action gating, spectator visibility, and runtime failures.
- `crates/aeon_sim/tests/plans.rs`, `goals.rs`, `situations.rs`, and
  `assignments.rs` — rumours, spectator autonomy and persistence, scoring bias,
  directives, private audiences, audience persistence, validation symmetry,
  and deterministic restore.

### Content and client

- `assets/content/core/plans.rhai`, `goals.rhai`, and `situations.rhai` — the
  current authored campaigns, ambitions, directive wishes, public Situations,
  and private favour-debt audience.
- `crates/aeon_client/src/ui/inspector.rs`, `situations_panel.rs`, and
  `log_panel.rs`, plus `assignment_ui.rs` — visible plans and directives,
  audience-filtered Situations and history, spectator observation, navigable
  log channels, and private-log filtering.

## Related GDD sections

- [Game Design Overview](overview.md)
- [Assignments, Forecasts, Plans, and Goals](03-assignments-forecasts-plans-and-goals.md)
- [Economy, Order, Obligations, Situations, Intrigue, and Warfare](05-economy-order-obligations-situations-intrigue-and-warfare.md)
- [Interface, Accessibility, Art, Audio, and Narrative](07-interface-accessibility-art-audio-and-narrative.md)
