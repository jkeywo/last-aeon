# Last Aeon — Assignments, Forecasts, Plans, and Goals

| Field | Value |
| --- | --- |
| Document status | Draft 0.1 |
| Scope | The action vocabulary from individual orders to organisational ambitions |
| Primary design authority | `pasm/spec/` |
| Current implementation | `crates/aeon_sim/`, `crates/aeon_client/`, and `assets/content/core/` |

**GDD navigation:** [Index](README.md) · [Overview](overview.md) · [Previous: Characters and Politics](02-characters-organisations-politics-and-succession.md) · [Next: Map, Presence, Travel, Ships, and Armies](04-map-presence-travel-ships-and-armies.md)

This section explains how intent becomes action in *Last Aeon*. It is a
player-facing design account, not a replacement for PASM or the executable
rules. Where those sources differ in level of detail, PASM remains the design
authority and the simulation remains the authority on current behaviour.

## Status language

- **Implemented** describes behaviour present in the current code and covered
  by tests or direct implementation evidence.
- **Accepted design** describes a confirmed PASM decision. In this section,
  accepted designs are also implemented unless explicitly stated otherwise.
- **Proposal** or **open question** identifies an option for later discussion.
  It is not a commitment and does not alter PASM.

## Purpose and player experience

Assignments are the game's verbs. They turn a strategic intention into work
performed by a particular person, against a typed target, over time. The player
should not merely choose an abstract action; they should decide who can be
spared, what the action is aimed at, whether its cost and delay are acceptable,
and whether its likely consequences justify the risk.

The intended experience is:

1. discover an action in the context of the person, organisation, province,
   war, army, or ship it concerns;
2. choose or confirm a legal target;
3. compare eligible household members using authoritative forecasts;
4. commit a delayed, logged order;
5. follow the assignment's current phase and decide whether to recall it;
6. receive a legible outcome through state changes, the message log, and, when
   authored, a result popup;
7. reassess as that result changes later opportunities, plans, and goals.

Plans and goals extend the same structure without creating privileged action
paths. A plan is a character's authored multi-step campaign whose steps are
ordinary assignments or standing-order changes. A goal is a house's longer
ambition: it does not execute actions itself, but makes relevant pressures more
important to the house and its vassals.

## The assignment lifecycle

### 1. Discover and compose

**Implemented / accepted design.** Actions are offered under the thing being
ordered. An army offers army assignments; a province offers actions performed
there; other targets are selected through the assignment composition window.
The assignment declares its target kind and its requirements, so the UI does
not hardcode which action is valid against which subject.

The current target vocabulary is:

| Authored target kind | Runtime target |
| --- | --- |
| None | The acting organisation itself |
| Character | One character |
| Organisation | One organisation |
| Province | One province |
| War | One occurrence-stable formal war |
| War side | One exact side of one formal war |
| Own army | One army belonging to the acting organisation |
| Own army and province | One owned army plus its destination or operational target |
| Own ship and province | One owned ship plus its destination or operational target |

Requirements are declarative and default to "do not care". They can constrain
the target's holder or house, hostile occupation, army presence, title kind,
an owed favour, whether the owner is threatened, and minimum or maximum
provincial order. One simulation gate evaluates these facts for the action
button, forecast, AI, plan, and standing-order paths.

[ai] One further requirement, `target_under_covert_work`, is about what is
being done to the target rather than who holds it: it holds only while
somebody else's covert province-aimed work is live against the target
province *and* the orderer has not yet proved whose hand it is — the same
facts that raise the Unquiet Holdings alarm and keep its investigate action
on offer — and may only be authored on a province-bearing target kind. It
is what keeps an enquiry such as `trace-the-hand` an answer to the alarm
rather than a free-standing order: on quiet ground, and again once the hand
is proved while the work still runs, the enquiry is offered on no province,
in no household list, and is refused as an ordinary bad target by the one
shared gate.

Situation actions are shortcuts into this same flow. They project an authored
assignment and retain the exact Situation occurrence and formal-war identity
that authorised it; they do not create a separate action system.

[ai] The convergence runs both ways. An ordinary order that a live
Situation's current projection offers — the same assignment at the same
target, a leader the action pins or leaves free, and the same formal-war
context — inherits that Situation's occurrence when it starts, so the
province and household buttons place exactly the order the card's shortcut
would have, provenance included. An answer whose effect reads its origin,
such as `trace-the-hand`, is offered by the ordinary paths only while a
live card offers it — while unproved covert work runs against the province
— so it always starts with that card's occurrence and keeps the forecast's
promise whichever way it was ordered; an order no live card offers starts
without an origin, as before.

### 2. Select a leader

**Implemented / accepted design.** A leader must be alive, an adult member of
the acting organisation, able to lead, and free of a conflicting commitment.
The simulation's `leader_availability` function is the single source of truth.
It distinguishes:

- available;
- busy leading a named assignment, with its completion date;
- indisposed through injury, capture, or incapacity, with a recovery date when
  known;
- holding a standing post such as general or captain;
- ineligible because the character is dead, under age, absent from the
  organisation, or unknown.

A standing command does not bar ordinary work by itself. It bars a second
command, while still allowing its holder to lead an action involving the force
they already command. Army and ship operations use their assigned general or
captain rather than presenting the commander as a free choice.

The client lists eligible adults rather than silently hiding busy or
indisposed candidates. It compares candidates in a non-modal composition
window, keeps the window open while selection changes, places available leaders
first, and retains the whole forecast for each option.

### 3. Forecast before commitment

**Implemented / accepted design.** A forecast reports:

- action title and summary;
- leader and target;
- order delay and assignment duration;
- immediate wealth, manpower, supplies, and influence costs;
- governing skill, leader skill value, authored difficulty, and effectiveness;
- [ai] the live opinion an authored relationship modifier read, and the
  clamped effectiveness shift it produced, when the assignment authors one;
- [ai] the target province's live Order an authored `order_modifier` read,
  and the clamped effectiveness shift it produced, when the assignment
  authors one and the target names a province;
- every authored outcome and its exact permille chance;
- personal risks on failure and disaster;
- any separate conditional military contest;
- the first point of no return, if the assignment has a committed phase;
- the specific reason the order is currently blocked.

Effectiveness is the leader's governing skill minus the assignment difficulty.
It shifts favourable and unfavourable authored outcome weights in opposite
directions, with a floor that prevents a non-zero outcome from disappearing.
Largest-remainder apportionment turns the weights into integer permille odds
that total exactly 1,000.

[ai] An assignment may additionally author an `opinion_modifier` block —
`from`, `toward`, `per_point`, `min`, `max` — whose two roles reuse the
closed effect vocabulary, restricted at load to the roles resolvable from
the owner and leader alone (leader, owner-head, liege-head, consul). The
live opinion between the resolved pair, times `per_point` hundredths of an
effectiveness point, truncated toward zero and clamped to the authored
bounds, is added inside the shared effectiveness calculation — so a
relationship shades the contest without ever replacing skill, and the same
number moves the forecast and the roll. The mechanism is simulation code;
every magnitude and both roles are authored data. The forecast reports the
opinion it read and the shift it produced, and resolution reads the live
relationship on its own day exactly as it reads the leader's live skill.

[ai] An assignment whose target names a province may likewise author an
`order_modifier` block — `reference`, `per_hundred`, `min`, `max` — read
against the target province's live Order inside the same shared
effectiveness calculation: the shortfall of Order below the authored
reference, times `per_hundred` hundredths of an effectiveness point,
truncated toward zero and clamped to the authored bounds. Authoring
`max: 0` makes Order pure resistance, which is how the intrigue province
operations use it. Only province-bearing target kinds may author the
block, enforced loudly at load; a target without a province reads as
neutral. The forecast reports the Order it read and the shift it
produced, the semantic world view exposes the same live shift on running
assignments so Situation cards can quote it, and resolution reads the
live province on its own day.

Forecast and resolution share duration, weighting, sampling, and risk
calculations. A military operation is disclosed as a second conditional field
contest rather than folded into the assignment roll, because its state may
change before resolution. This prevents the interface from promising false
precision.

### 4. Validate and dispatch

Starting an assignment is a `PlayerCommand`. Submission
validates the current action, allocates a monotonic sequence number, and queues
it for the current date plus one day plus the leader's order delay. Pending
assignment orders must appear in the Assignments panel as en route, including
orders launched from Situations, so a command issued while paused does not seem
to vanish. Ordinary and Situation-launched starts use the same projection and
identify the target alongside the assignment, leader, and execution date.

Commands execute in deterministic `(day, sequence)` order and are revalidated
when they arrive. If circumstances changed during the delay, the logged attempt
remains but the invalid action does not start. On a legal start, costs are paid
immediately, a stable assignment ID is allocated, and the leader becomes busy
until the assignment ends.

Long-running strategic transitions, including claim and formal-war actions,
are checked again at completion. Permission that no longer exists cannot be
carried forward from the start date. A successful roll whose strategic
prerequisite has vanished becomes a failure. War-bound assignments are also
aborted when the exact authorising war ends.

### 5. Progress through phases and cancel

**Implemented / accepted design.** An assignment contains an ordered list of
authored phases. Each phase has a stable ID, duration, interruption rule, and
optional interruption effect. Content loading verifies that phase durations
sum to the assignment duration. An assignment that authors no phases receives
one interruptible phase covering its full duration, preserving the earlier
behaviour.

The Assignments panel shows the leader, remaining days, current phase when
there is more than one, and whether recall is possible. A cancellation during
an interruptible phase ends the assignment immediately and applies that
phase's interruption effect. A cancellation during a committed phase is
recorded rather than rejected; the panel shows it as pending, and the request
is honoured on the first day an interruptible phase begins. Peace is a stronger
authoritative boundary and immediately aborts work bound to the concluded war,
regardless of its authored phase.

### 6. Resolve, apply consequences, and report

**Implemented.** Due assignments resolve daily in stable assignment-ID order.
A dead or removed leader causes abandonment. Otherwise, guaranteed assignments
resolve as success, while probabilistic assignments draw exactly once from the
forecast's outcome table. The current outcome vocabulary is critical success,
success, failure, and disaster.

Failure and disaster may independently expose the leader to authored risks:
injury, capture, scandal, incapacity, or death. Results call authored Rhai
functions that receive validated context and return typed effects; scripts do
not mutate world state. Effects can address resolved roles such as leader,
target, target head, owner head, liege head, Consul, and Sanctora.

Notable results from all organisations may enter filterable history, subject to
the captured audience and visibility rules of the originating content. An
authored popup is shown only for the player organisation, automatically pauses
time, names the matter and the character concerned, and may offer choices whose
answers are themselves logged player commands. The window dismisses
immediately after the click for responsive feedback; the authoritative answer
applies when its queued command arrives.

Routine failures automatically restart the same assignment and lose time;
other outcomes complete it. A plan waiting on a completed assignment receives
the outcome and either advances, retries, or ends.

## Standing orders

**Implemented / accepted design.** An army's standing orders are an ordered
priority list of assignment keys. Each day an idle army tries the first entry
whose authored requirements and ordinary start validation pass. The player can
edit this list directly, and a plan can write the same list to the same army
record. No separate automated-combat permission exists: a force cannot begin
unbidden what its owner could not legally have ordered by hand.

Standing orders connect strategic plans to operations without making plans
command battles. A plan selects one army associated with its acting character,
sets doctrine, and lets the ordinary daily standing-order system decide which
valid assignment fires. This relationship is expanded in
[Map, Presence, Travel, Ships, and Armies](04-map-presence-travel-ships-and-armies.md).

## Character plans

### Authored structure

**Implemented / accepted design.** A plan is data, not script. It defines a
stable key, title, summary, shared AI intent, score bonus, target kind,
cooldown, maximum lifetime, retry budget, and one or more methods in authored
preference order. Methods have declarative gates and ordered steps. Steps may:

- start an ordinary assignment;
- expand a sub-plan at adoption;
- set one army's standing-order priority list.

Sub-plans are validated as acyclic to a shallow depth and flattened when the
plan is adopted. A running plan therefore does not consult changing sub-plan
definitions. Assignment steps may deliberately use actions marked
`ai_available: false`, but they never bypass target, leader, resource, or
strategic validation.

The current selectors are intentionally narrow: no target, the plan target,
the authority's worst holding, the head of the target organisation, the
lowest-order enemy province in the exact target war, and [ai] the target
organisation's most disordered province sharing a surface route with a held
one (`target-border-province`, lowest stable ID on a tie — how a covert
campaign finds the shared border). Dynamic selectors resolve
when the step starts, so a months-old plan acts on current visible facts.

### Adoption and execution

**Implemented.** On the monthly agency pass, an autonomous character may adopt
one plan answering their highest scored pressure when the pressure plus the
plan's bonus reaches the fixed plan threshold. Content-key order selects the
first eligible plan. When that plan has several eligible methods, one is chosen
from the frozen `"plan-method"` derived stream using the character and month.

Heads may exercise their house's full authority. Other household members may
adopt only plans proven to be entirely spend-free and unable to command an
army; they also do not duplicate a plan somebody else in their organisation is
already pursuing. One character holds at most one plan, and assigned work keeps
a courtier's own ambition from pre-empting the house's needs.

Plans advance daily in stable character-ID order. Satisfied steps are skipped.
An unresolved selector or temporarily blocked start waits, while maximum plan
duration is the single timeout. Method gates are rechecked before every new
step; changed strategic facts abandon the plan rather than invoking hidden
replanning. Success advances; failure or disaster consumes a retry; exceeding
the retry budget abandons the plan. Completion and abandonment both apply the
authored cooldown and write to the log. Death of the actor, fall of their
organisation or target, end of a target war, or expiry also ends the plan.

The current authored catalogue includes campaigns for pressing the Paramount
claim, preparing and challenging a rival claimant, prosecuting a claimant war,
readying levies, steadying holdings, answering a grievance, and shoring up
standing. A plan aimed at the player's organisation or holding produces a
limited rumour naming who is acting and broadly what concerns them; it does not
pretend a full espionage system exists. [ai] The catalogue now also carries
the covert `deniable-pressure` campaign: gated in data on hostility (a
head-to-head opinion floor, or an open grievance owed) and capability, it
whispers no rumour and confides its lines in its owner and in whoever has
proved that owner — the covert
exception recorded in
[AI Agency and Information Rules](06-ai-agency-and-information-rules.md).
[ai] The corresponding answer is an ordinary assignment: `trace-the-hand`
is authored like any other consequential intrigue work, is closed to the
AI, is aimed at the investigator's own troubled province rather than at a
suspect, and is offered by its Situation with no leader pinned so the
player compares candidates on the ordinary per-candidate forecasts. Only
its success results carry an effect, and that effect proves an
organisation the originating lifecycle already bound; nothing in the
authoring vocabulary can name anybody else. It is gated in data on
`target_under_covert_work`, so it can be bought only while a hand is
actually moving against that holding and has not yet been proved — never
on quiet ground, and never against a hand already proved, where either way
it could prove nothing.

## Organisational goals and directives

**Implemented / accepted design.** A goal is a standing ambition keyed by
organisation. Unlike a character's plan, it survives succession. It contains
an adoption trigger, priority, favoured AI intents and bonus, optional target,
derived directives, maximum lifetime, and cooldown.

An eligible autonomous house without a goal gets a monthly 30% adoption check
on the frozen `"grand-goal"` stream, derived from organisation and month. It
then chooses the highest-priority eligible goal, with content key breaking
ties. Current authored ambitions are becoming Consul, taking the planet, and
conquering a neighbour. [ai] The covert `undermine-a-neighbour` ambition
joins them: windowed in data to campaign days 180–260 (`min_campaign_day` /
`max_campaign_day`, new trigger fields any goal may use), gated to vassals
with the authored capability floor, and resolved through the new authored
`hostile-border-neighbour` target selector — a standing organisation
outside the chain of command, holding a surface-adjacent province, hostile
by authored data (opinion at or below the authored floor toward its head,
or an open grievance owed), lowest stable ID first.

A goal has no executor. It biases the head's ordinary scoring so that existing
assignments and plans pursue the ambition. It ends when its house falls, an
organisation target falls, or its time horizon expires. This is intentionally
not a general semantic victory-condition engine: current completion detection
only recognises the states the implementation explicitly models.

A goal may press advisory directives on direct vassals. Derived directives
exist only while the liege's goal exists and are computed from it rather than
stored. A player may also issue one stored directive per direct vassal; a new
one replaces the previous one. Both forms add a modest bonus to the named
pressure in the vassal head's scoring. They never compel an action or bypass
the vassal's own leader, means, target rules, or validation. Manual issue and
clear commands recheck the direct chain of command when they apply.

The limits on what autonomous actors may know and how rumours expose their
intent are detailed in [AI Agency and Information Rules](06-ai-agency-and-information-rules.md).

## Data and persistence model

| Layer | Principal data | Deterministic identity and ordering |
| --- | --- | --- |
| Assignment definition | Target kind, requirements, skill, difficulty, duration, phases, costs, urgency, AI intent, results, risks, military operation, [ai] optional live-opinion modifier (roles, per-point scale, clamp), [ai] optional live-Order modifier (reference, per-hundred scale, clamp; province-bearing targets only), [ai] covert flag (provenance confided to the owner, and to any house that has proved it) | Stable content key; authored phase and outcome order |
| Active assignment | Stable ID, definition, owner, leader, target, war, Situation origin, start/completion dates, cancellation request | Stable assignment ID; daily resolution in ID order |
| Command | Typed player decision, execution day, monotonic sequence | Applied in `(day, sequence)` order and appended to the command log |
| Forecast | Derived timing, costs, contest, odds, risks, block reason, point of no return | Pure integer calculations shared with resolution |
| Plan definition | Intent, target kind, methods, gates ([ai] including campaign-day windows and the hostility predicates), flattened step vocabulary, cooldown and limits, [ai] covert flag | Stable content key; validated acyclic composition |
| Active plan | Actor-keyed definition, method, flattened steps, target, step, dates, current assignment, retries, reason | `BTreeMap` by stable character ID |
| Goal definition | Trigger ([ai] including campaign-day windows), priority, favoured intents, target, [ai] target selector, directives, lifetime and cooldown, [ai] covert flag | Stable content key; priority then key tie-break |
| Active goal | Organisation-keyed definition, adopting head, resolved target, date | `BTreeMap` by stable organisation ID |

Active assignments, pending popups, plans, goals, cooldowns, and manually
issued directives are captured in snapshots. Goal-derived directives are not
stored because they can be reconstructed exactly. Commands remain the ordered
record of player decisions. Content is validated before play and embedded for
byte-identical native, web, and replay behaviour.

The frozen RNG labels used in this area include `"job-resolution"`,
`"job-risk"`, `"plan-method"`, and `"grand-goal"`. Their historic spelling is
part of campaign identity. Renaming any of them would reroll past and future
campaigns and is outside the scope of documentation work.

## Edge cases and failure policy

- A command valid at submission but invalid on arrival remains logged and does
  not start.
- Costs are charged only when an assignment actually starts.
- Missing, dead, under-age, foreign, busy, indisposed, or conflictingly posted
  leaders are rejected by the shared availability and validation path.
- A target may exist but still be illegal because its authored relational
  requirements no longer hold.
- A committed-phase cancellation remains visible and waits; it is not lost.
- Conclusion of an authorising war immediately aborts the associated work.
- A leader's death abandons their assignment and character plan, while an
  organisation goal survives a normal succession.
- A routine failure retries automatically; a routine disaster does not.
- A plan selector that currently resolves to nothing waits. A failed method
  gate, dead actor, fallen target, ended war, or expired horizon abandons.
- Plan step failures use one authored retry budget. There is no hidden search
  for a different method after adoption.
- A goal-derived directive disappears with its source goal. A stored manual
  directive is ignored unless its recorded issuer is still the vassal's liege.
- Missing authored result data falls back safely to a generic failure report
  rather than applying an unrelated success effect.

## Feedback requirements

**Accepted baseline.** The player can see every delayed assignment order en
route regardless of its launch surface, plus its
leader, remaining duration, phase, recall state, authoritative forecast,
blocked reason, result log, and authored popup. Autonomous plan adoption,
completion, abandonment, and goal changes are logged; hostile plans concerning
the player can appear as rumours.

**Proposal, not accepted.** A later UI pass could expose an explicit causal
trail from active goal → favoured pressure → adopted plan → running assignment,
and give queued commands a visible reason when arrival-time revalidation drops
them. This would strengthen the overview's promise that consequences are
legible, but the presentation and information rules should be settled with the
UI and AI GDDs before any PASM decision is proposed.

## Dependencies

- Characters, membership, adulthood, health, succession, titles, obligations,
  hierarchy, and relationships determine who may act and against whom. See
  [Characters, Organisations, Politics, and Succession](02-characters-organisations-politics-and-succession.md).
- Presence supplies order delay; armies and ships supply commanders, force
  targets, standing orders, and conditional operations. See
  [Map, Presence, Travel, Ships, and Armies](04-map-presence-travel-ships-and-armies.md).
- Economy supplies costs and eligibility facts; order, Situations, intrigue,
  and war supply pressures, targets, effects, and revalidation boundaries.
- AI scoring supplies the pressures that adopt plans and receives bias from
  goals and directives. See
  [AI Agency and Information Rules](06-ai-agency-and-information-rules.md).
- Rhai content and the string table supply definitions, effects, player-facing
  prose, and localisation identities.
- The deterministic clock, stable IDs, derived RNG, snapshots, command log,
  and state hashing make campaigns reproducible.

## Acceptance criteria

The system meets the current design contract when:

1. every assignment start path uses the same target, requirement, leader,
   resource, and strategic validation;
2. forecast odds, duration, risk, and operational caveats come from the same
   simulation calculations used at resolution;
3. a player can compare legal and blocked leaders with specific reasons before
   issuing an order;
4. submitted orders are delayed, totally ordered, logged, revalidated, and
   visible while in transit;
5. authored phases progress in order, report commitment, and honour immediate
   or deferred cancellation with the correct interruption effect;
6. results deterministically apply typed effects, risks, logs, popups, and
   popup choices without scripts mutating simulation state;
7. routine retries, leader death, war conclusion, and changed strategic
   permission follow the documented failure rules;
8. standing orders and plan steps can do nothing their owner could not have
   started through ordinary validation;
9. active plans progress, retry, complete, abandon, cool down, snapshot, and
   restore identically;
10. non-head plans cannot spend organisational resources, command armies, or
    duplicate the same house plan;
11. goals remain organisation-level scoring biases across succession, while
    directives remain advisory and obey the direct chain of command;
12. snapshots and replay preserve assignments, popups, plans, goals, manual
    directives, command ordering, and frozen RNG stream identities.

## Open questions

These questions are intentionally unresolved and make no new commitment:

- Should a delayed order that fails arrival-time revalidation produce a
  dedicated player-facing notice, and how much of the failed condition should
  it reveal?
- Should the player receive a first-class plans-and-goals inspector showing
  causal scoring detail, or should some autonomous intent remain inferential?
- Should players ever author personal multi-step plans directly, or remain
  responsible for issuing their own assignments while plans model autonomous
  character persistence?
- Does automatic retry remain desirable for every routine failure once the
  assignment catalogue grows, or should retry policy become authored?
- Which goal-completion predicates are needed beyond a rival organisation
  becoming defunct, and should they remain a small typed vocabulary rather than
  a general condition language?
- How should hidden information affect forecast precision for hostile targets
  without breaking the promise that displayed odds are authoritative for the
  known inputs?
- What notification density keeps AI plans and goals legible without flooding
  the message log in a larger interstellar campaign?

## Evidence

### PASM

- `pasm/spec/architecture/implementation-decisions.yaml` — accepted decisions
  for shared forecasts, leader availability, target requirements, authored
  phases, standing-order priority lists, data-only plans, plan validation,
  plan persistence, goals as scoring lenses, derived directives, chain-of-
  command authority, and frozen RNG identities.
- `pasm/spec/roadmap/milestone-4-the-assignment-pass.yaml` — assignment naming,
  contextual action UI, phases, standing orders, and household autonomy.
- `pasm/spec/roadmap/milestone-5-persons-and-plans.yaml` — character agency,
  authored plan vocabulary, runtime, catalogue, and communication.
- `pasm/spec/roadmap/milestone-6-plans-reach-the-field.yaml` — plans commanding
  forces through standing orders, dynamic target selectors, and non-head plans.

### Simulation and tests

- `crates/aeon_sim/src/assignments.rs`, `forecast.rs`, and `command.rs` — target
  and lifecycle state, availability, validation, dispatch, phases,
  cancellation, resolution, risks, effects, popups, persistence, and shared
  forecast calculations.
- `crates/aeon_sim/src/plans.rs` and `goals.rs` — plan adoption and advancement,
  goal bias, directives, deterministic ordering, cooldowns, and persistence.
- `crates/aeon_sim/tests/assignments.rs` — effects and logs, delegation, routine
  retry, popups, risks, death, AI action, deterministic snapshots, empirical
  forecast agreement, costs and delays, rejection explanations, availability,
  target rules, phases, cancellation, and household autonomy.
- `crates/aeon_sim/tests/plans.rs` — ordered steps, skips, retry abandonment,
  leader death, replay and snapshots, rumours, standing orders, dynamic targets,
  courtier limits, duplicate prevention, and spectator activity.
- `crates/aeon_sim/tests/goals.rs` — adoption, scoring bias, succession,
  deterministic snapshots, derived directives, lapse, manual authority, and
  manual-directive persistence.

### Content and client

- `assets/content/core/administration.rhai`, `economy.rhai`, `intrigue.rhai`,
  and `warfare.rhai` — the current assignment catalogue and authored effects.
- `assets/content/core/plans.rhai` and `goals.rhai` — the current plan and goal
  catalogues.
- `crates/aeon_client/src/ui/assignment_popup.rs`, `assignments_panel.rs`, and
  `forecast.rs`, plus `assignment_ui.rs` and `forecast_view.rs` — composition,
  leader comparison, forecast presentation, pending and active work,
  cancellation, popup display, and automatic pause.

## Related GDD sections

- [Game Design Overview](overview.md)
- [Characters, Organisations, Politics, and Succession](02-characters-organisations-politics-and-succession.md)
- [Map, Presence, Travel, Ships, and Armies](04-map-presence-travel-ships-and-armies.md)
- [AI Agency and Information Rules](06-ai-agency-and-information-rules.md)
