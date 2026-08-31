# Last Aeon — Map, Presence, Travel, Ships, and Armies

| Field | Value |
| --- | --- |
| Document status | Draft 0.1 |
| Scope | Strategic geography, physical presence, movement, forces, and operational warfare |
| Current scenario | The Ashkarr Succession |
| Primary design authority | `pasm/spec/` |
| Implementation evidence | `crates/aeon_sim/`, `crates/aeon_client/`, and `assets/content/` |

**GDD navigation:** [Index](README.md) · [Overview](overview.md) · [Previous: Assignments, Forecasts, Plans, and Goals](03-assignments-forecasts-plans-and-goals.md) · [Next: Economy, Order, Obligations, Situations, Intrigue, and Warfare](05-economy-order-obligations-situations-intrigue-and-warfare.md)

This section explains how place constrains power in *Last Aeon*. It is a
player-facing design account, not a replacement for PASM or the authoritative
simulation. Where the sources do not yet agree completely, the difference is
called out rather than silently resolved here.

## Reading the status labels

- **Implemented now** describes behaviour present in the current simulation or
  client and covered by code or tests.
- **Accepted design** describes a protected direction or decision in PASM. It
  may be broader than the current implementation.
- **Finished-game direction** describes accepted scope beyond the Ashkarr
  slice. It is not a commitment that the feature belongs in the current slice.
- **Open question** identifies a decision still needing design work. It does
  not establish a new rule.

## Purpose and player experience

The map is the material limit on personal rule. It answers four linked
questions: where political authority applies, where people and forces actually
are, how long it takes them to move, and what can happen before an instruction
reaches its agent.

The intended experience is operational rather than tactical. The player should
read a political geography, decide where an important person or force must be,
commit scarce time and leadership, and live with the consequences while the
journey or operation unfolds. A leader on Vesk cannot act as if they were on
Ashkarr; an army without supplies is not its paper strength; and a ship without
an effective captain should not be equivalent to a properly commanded asset.

The resulting loop is:

**Inspect place and pressure → position a person or force → issue a delayed,
validated order → advance time → resolve movement or an operation → inspect the
changed territorial and military situation.**

## Campaign geography

### Map hierarchy

The accepted hierarchy is:

1. a campaign contains a compact network of star systems;
2. each system contains worlds and orbital infrastructure;
3. politically meaningful surfaces and habitats are divided into provinces;
4. characters, armies, and ships occupy or move between those places.

A province is the basic territorial holding. Its title determines its legal
holder; its body and authored latitude/longitude determine where it appears;
and its economy, buildings, order, characters, and forces give the place its
strategic meaning. A starbase may be one province rather than a special map
exception.

**Implemented now.** The Ashkarr scenario contains three authored bodies and
41 stable provinces: 32 on Ashkarr, eight on its moon Vesk, and the single
province of Spire Decks on the Aurelian Spire. Ashkarr is the primary; Vesk
orbits it at an authored distance and period; the Aurelian Spire is a smaller,
faster orbital body. Bodies and provinces spawn from content in content-key
order, receive stable IDs, and restore from snapshots against hash-verified
content.

**Accepted design.** Provinces are legal holdings. Their territorial titles
are held by organisations and exercised through the current head, while
personal offices and superior titles remain attached to characters. This lets
a house persist through succession without retitling all of its land.

**Finished-game direction.** The complete campaign is a network of fewer than
ten star systems linked by scarce Maelstrom Gates. Large worlds may contain
around one hundred provinces. Gates determine the meaningful interstellar
routes. The MVP explicitly excludes gates, Maelstrom travel, Precursor
technology, and cosmic entities.

### What the map currently shows

**Implemented now.** The client has a local-system view for selecting orbiting
bodies and a focused body view that can be shown as either a rotatable globe or
an equirectangular flat projection. Both projections use the same authored
surface directions, political texture, labels, selection pin, and click
targets. Province regions are baked from the nearest authored province centre;
the focused map overlays province names and force badges and links selections
to detailed inspectors.

Eight map modes answer distinct strategic questions:

| Mode | Question answered |
| --- | --- |
| Holder | Who directly holds each province? |
| Great House | Which great-house realm sits above each holder? |
| My Control | Which land answers directly or transitively to the player? |
| Order | How governable is each province, and which are in unrest? |
| Wealth | What is each province's authored wealth output? |
| Military | How much manpower is physically garrisoned there? |
| Player Relations | How does the holder's head regard the player's head? |
| Claim Pressure | How much does the holder's territorial share matter to the Paramountcy contest? |

Graded modes print a value as well as using colour. Legends, hover explanations,
attention marks, and province inspectors all derive from the same readout. The
political and military answers come from authoritative simulation functions,
not client-side approximations.

## Physical presence

**Accepted design.** Every simulated character has a concrete location: in a
province, aboard a tracked ship, or on an explicit route segment through the
vehicle carrying them. There is no abstract transit location. Surface travellers
move through successive provinces; people and embarked armies inherit the
location of their ship. Presence should make delegation, travel, interception,
and local authority matter.

**Implemented now.** A character's serialized location is either a province or
aboard a tracked ship; route and segment progress are persisted separately.
Surface travel advances one authored edge at a time. Interworld travellers walk
to a starport, board a visible system-controlled personal transport, cross the
authored space graph, disembark, and permanently retire that vessel's stable ID.
Arrival is processed daily before due assignments resolve.

Presence is a commitment because a person already in transit cannot begin a
second journey, and assignment eligibility, leadership posts, indisposition,
and existing work constrain who can act. Exact job-specific presence rules and
forecasting belong in [Assignments, Forecasts, Plans, and Goals](03-assignments-forecasts-plans-and-goals.md).

## Travel and order delay

### Personal travel

**Implemented now.** Personal travel, ships, trade shuttles, marching armies,
and military operations use deterministic fastest paths over the same authored
route graph. Equal paths break ties by stable route and province identity. Each
edge carries duration and persisted risk; risk has no gameplay effect yet.

**Accepted design.** All movement uses one authored route graph. Same-body
journeys advance province by province. Route edges record separate time and risk
costs; the initial route choice is the fastest valid deterministic path, while
the data supports later safety-versus-speed choices based on route danger,
world state, and political relationships.

Interworld civilian travel must pass through an authored starport at each end.
At the departure starport the system spawns a temporary personal transport ship;
the traveller boards it, follows the space route, disembarks at the destination
starport, and the transport despawns. Surface legs connect the traveller's real
origin and destination provinces to those ports. Armies use the same route graph
but cannot create these personal transports.

### Command latency

Every player command is queued for at
least the next simulation day. When a command names an acting character on a
different body from the player's head, it gains half the fastest valid route
time, with a minimum of one additional day. If the head is aboard a moving ship,
every player order is delayed until the day after that ship next docks. Surface
travel creates no blanket delay because the head remains in successive concrete
provinces. The delay is computed when the order is submitted and stored in its
command envelope, so a replay does not recalculate history from later positions.

The delay applies most visibly to assignments and character travel because
those commands identify an actor. Commands without a character actor still
inherit the head-in-transit delay, but do not currently gain cross-body
head-to-agent latency. This is an implementation fact, not a broader statement
that all direct force administration should remain instantaneous.

The interface contract is that order delay appears in the same simulation-
derived forecast the player sees before commitment. The player should be able
to tell the difference between assignment duration, physical travel time, and
the time before an order starts.

### Interstellar travel

**Finished-game direction only.** Maelstrom Gates are accepted as scarce links
between systems, but gate traversal, interstellar pathfinding, gate control,
interception, expedition logistics, and inter-system order latency are not
implemented in the Ashkarr slice. Nothing in the local orbital-distance formula
should be read as the future gate-travel rule.

## Ships and captains

**Implemented now.** Ships are persistent, individually tracked assets with a
stable ID, authored name and class, owner, Captain, optional First Officer,
troop capacity, occupants, route progress, retreat state, and any active
blockade or trade route. Ordinary destinations are authored starports. Direct
movement and operations advance through explicit space-route segments. Each
persistent ship draws one supply from its owner on the monthly pulse; temporary
personal transports do not.

The current scenario authors seven ships: three capital ships, two transports,
and two patrol ships. The wider economy also gives transports standing trade
routes, discussed in [Economy, Order, Obligations, Situations, Intrigue, and Warfare](05-economy-order-obligations-situations-intrigue-and-warfare.md).

**Accepted design.** A ship may have a Captain and an optional First Officer,
both physically aboard. Either post can be assigned remotely, but appointment is
a timed job: the candidate travels to the ship and only assumes the post after
boarding. The incumbent Captain remains in command until a replacement arrives.
If the Captain dies or leaves, an eligible First Officer is promoted immediately
and seamlessly, preserving the ship's active orders and route progress. A
replaced Captain automatically fills an open First Officer post unless they
already have another validated active or queued assignment.

A commanded ship can receive ordinary orders. A ship with neither Captain nor a
promotable First Officer accepts no ordinary orders and automatically seeks an
acceptable starport at half speed. Candidate tiers are checked in order:

1. ports owned by the ship's organisation;
2. ports owned one hierarchy level at a time above it, beginning with its direct
   liege;
3. ports whose owner's head has positive opinion of the ship-owner's head.

Within a tier, shortest valid route time wins, with stable starport ID breaking
ties. The destination is retained unless it becomes invalid or unreachable. A
captainless ship already docked at any acceptable port stays there; if no port
qualifies, it remains where it is. Half speed doubles each remaining route
segment duration, rounding partial days up. Passengers, cargo, and an embarked
army remain aboard. A newly arrived Captain ends the automatic retreat
immediately without discarding current route progress.

These command, succession, suspension, and retreat rules are implemented through
the typed appointment command path; the legacy Captain command delegates to the
same travel-and-handover logic.

## Armies and generals

**Implemented now.** An army is a persistent force with an owner, optional
General and Lieutenant, manpower, its own supply train, concrete province or
embarked location, route and retreat progress, and an ordered standing-order
list. The Ashkarr scenario begins with 17 authored armies: two
for each great house, one for every vassal and independent house, and the
Sanctora garrison on the Aurelian Spire. These starting forces are separate
from each organisation's recruitable manpower pool.

New armies form through the 40-day Muster assignment. Wealth and influence are
paid at commitment; on a successful result, the effect draws the specified
manpower and supplies from the owner's pools, clamped to what remains, and
musters the army where its general stands. The authored current results form
500 men with 100 supplies on success or 700 men with 160 supplies on critical
success. Failure spends the initial political and financial commitment but
does not draw the levy.

Monthly, an army consumes `1 + manpower / 1000` supplies from its own train. If
the train cannot cover that amount, supplies fall to zero and the army loses
five percent of its manpower, with at least one casualty. A force that wastes
away is removed. Voluntary disbandment is allowed only while the army is not
committed to an assignment and returns its surviving soldiers, but not its
remaining supplies, to the owner's manpower pool.

The general is not interchangeable with a generic household leader. Army-led
operations use that army's general, command skill affects field strength, and
one officer cannot hold multiple standing commands. The force inspector links
the army to its province and general, exposes manpower and supplies, and offers
the assignments and standing orders applicable to that army.

Armies mirror the ship command system in all respects. An
army has a General and may have a Lieutenant. Remote appointments require the
candidate to travel to the army; an eligible Lieutenant succeeds a vacant
General seamlessly and preserves orders; and a replaced General fills an open
Lieutenant post unless already ordered elsewhere. An army without either officer
accepts no ordinary orders and retreats at half speed toward the closest
acceptable province using the same ownership, hierarchy, positive-opinion,
route-time, and stable-ID rules as ships. It cannot arrange new interworld
transport while leaderless. If already aboard a ship, it remains aboard and
defers its own retreat until legal disembarkation is possible.

### Army transport

An army crossing between worlds requires a persistent owned ship at an authored
starport. Any ship may carry troops if it has positive authored troop capacity;
current transport classes should normally carry capacity while capital and
patrol ships default to zero unless deliberately authored otherwise. One army
must fit wholly within one vessel according to its current manpower. Supplies
and equipment are included, armies are never split implicitly, and authored army
sizes and capacity bands should align cleanly.

Embarkation and disembarkation are logged commands producing one-day jobs. They
require an eligible Captain and General, an uncommitted same-owner persistent
ship, a whole-army capacity fit, and a common authored starport. The General and
Lieutenant move with the army. A future specialised assault-landing system may use
distinct ships, risks, opposition, and non-starport destinations.

### Movement and standing orders

Army movement is an assignment, not instantaneous dragging. Travel happens
first over surface-route edges, followed by the authored work duration. March
adds a one-day arrival and reorganisation stage; siege, raid, blockade, and
response retain their authored work time after reaching the objective.

Standing orders are an ordered list of authored assignments a force may start
while idle. Each day, the army attempts the first entry whose ordinary
requirements are satisfied. Manual assignments take precedence. In the
current content, `respond` can answer a threatened holding and `patrol` can
improve local order. Foreign presence by itself is not hostile: formal-war
sides or the operation-local hostility of a raid determine threats.

## Strategic operations, not tactical combat

**Accepted design.** The player selects objectives, forces, commanders, and
preparation through the assignment system. There is no separate tactical
battle layer. Move, resupply, patrol, siege, raid, blockade, war declaration,
war adoption, and negotiation remain visible strategic actions in the same
time-and-consequence pipeline as political work.

**Implemented now.** Content declares operation pacing, costs, phases, risks,
and result weights; engine code owns movement, engagement, conquest, loot, and
blockade mutations that cross multiple authoritative systems.

This section owns force presence, movement, commanders, and supply state. The
exact hostility requirements, operation consequences, engagement constants,
retreat, conquest, loot, blockade, and peace rules are owned by
[Economy, Order, Obligations, Situations, Intrigue, and Warfare](05-economy-order-obligations-situations-intrigue-and-warfare.md#formal-war-and-operational-warfare).
The assignment's rolled success cannot overrule defeat in the field.

PASM's high-level military component also names terrain or orbit,
intelligence, morale, and local support among intended inputs. Local support is
present through provincial order, but there are no separate terrain,
intelligence, or morale fields in the current engagement input. Those words
should be treated as accepted design breadth, not as claims about the present
formula.

## Rules and edge cases

- All map entities, forces, command sequencing, standing-order iteration, and
  engagement streams use stable identities and deterministic ordering.
- A journey cannot target the traveller's current province, and a traveller
  cannot be given another travel order until they arrive.
- A ship must be docked, owned by the player, aimed at another known province,
  and free of an active assignment before direct movement.
- An army or ship committed to an assignment cannot be disbanded, moved by the
  direct command path, or have its command contract changed in ways that would
  invalidate the operation.
- Siege and blockade validate the exact active war and opposing sides both at
  commitment and resolution. Peace or changed ownership can therefore make a
  previously valid operation fail safely.
- A raid is deliberately not proof of a formal war; its hostility belongs to
  that operation.
- Peaceful co-location does not create occupation pressure. Military map alerts
  use formal-war hostility, not “different owner means enemy.”
- Empty or missing armies cannot conquer. A force that changes owner during an
  operation no longer satisfies the initiating owner's contract.
- Captain death vacates the ship command. Replacing the captain does not let
  the replacement claim an operation accepted by the previous captain.
- Snapshot restore preserves map bindings, character locations, pending
  commands, ships, captains, armies, standing orders, blockades, and travel
  arrival dates.

## Feedback and information requirements

The player must be able to answer, without reconstructing simulation rules:

- which body and province a person, army, or docked ship occupies;
- where a traveller or ship is going and when it arrives;
- when an issued order will begin, separately from how long the work takes;
- who owns and commands each force, and why a force or officer is unavailable;
- current manpower, supplies, garrison strength, blockade, war side, and
  standing orders;
- the known costs, phases, interruption points, result odds, risks, and
  strategic modifiers before an operation is committed;
- what changed after movement, battle, retreat, conquest, raid, blockade,
  disbandment, or arrival, with links back to the people and places involved.

**Implemented now.** Province, army, ship, character, and body inspectors are
linked; armies and docked ships appear under their province; map badges and the
Military mode expose force presence; selection starts force-specific
assignments under the force being ordered; and important operation results are
written to the campaign log. Wider presentation, accessibility, and narrative
standards are specified in [Interface, Accessibility, Art, Audio, and Narrative](07-interface-accessibility-art-audio-and-narrative.md).

## Dependencies

- **Assignments and forecasts:** operations, mustering, leaders, phases,
  cancellation, outcomes, and displayed delay use the common assignment
  pipeline.
- **Characters and organisations:** heads issue orders; generals and captains
  are persistent posts; ownership and vassalage determine control.
- **Titles and succession:** province conquest changes an organisation-held
  title, while command posts and claims remain personal.
- **Economy and order:** provinces fund organisations; ships and armies consume
  supplies; raids, blockades, patrols, conquest, and garrisons alter output or
  governability.
- **Formal wars and Situations:** exact war occurrences determine hostility and
  legal operations; war Situations surface fronts and actions.
- **AI plans and goals:** AI and standing forces use the same authored
  assignments and authoritative validation rather than a separate movement or
  combat ruleset.
- **Persistence and replay:** stable IDs, command envelopes, derived RNG
  streams, content hashes, and snapshots reproduce the same geography and
  outcomes.

## Acceptance criteria

This section is satisfied for the current slice when the following remain true:

1. The authored campaign always spawns the same three bodies, 41 provinces,
   seven ships, and 17 starting armies with deterministic stable bindings.
2. Saving and restoring preserves all map, presence, travel, force, blockade,
   standing-order, and pending-command state and produces the same subsequent
   state hash.
3. Character, ship, and army movement advances over authored route segments,
   arrives on the forecast date, and command envelopes retain the delay quoted
   at submission.
4. The map's holder, realm, order, wealth, military, relationship, and claim
   answers match the authoritative simulation used by validation and
   resolution.
5. Every visible force can be inspected through its province or map badge, and
   every player-owned force offers orders under that force rather than under an
   unrelated destination.
6. Force ownership, leader eligibility, commitment, destination, and exact-war
   requirements reject invalid commands without partial mutation.
7. Mustering, upkeep, starvation, disbandment, march, resupply, patrol, siege,
   raid, blockade, engagement losses, retreat, and standing-order precedence
   behave deterministically and are covered by tests.
8. No current-slice interface or content implies that Maelstrom Gates or
   interstellar travel are playable.
9. Captain/First Officer and General/Lieutenant appointments, seamless
   succession, replacement, and leaderless retreat preserve deterministic
   orders and physical occupants.
10. Interworld civilian travel spawns and retires personal transports only at
    starports; an army embarks whole only when one persistent ship's authored
    troop capacity covers its current manpower.

## Evidence

- `pasm/spec/core/game-vision.yaml` — campaign map, local-system scope,
  provinces, presence, ships, jobs, operational warfare, formal war, and map
  intelligence modes.
- `pasm/spec/architecture/implementation-decisions.yaml` — local layout,
  liner and ship travel, order-delay formula, mustering and upkeep, engagement
  formula, authored armies, captains, force-scoped orders, standing orders, and
  map authority.
- `crates/aeon_sim/src/map.rs`, `presence.rs`, `command.rs`, `forces.rs`,
  `forecast.rs`, `warfare.rs`, and `wars.rs` — current authoritative rules.
- `crates/aeon_sim/tests/content_binding.rs`, `logistics.rs`, `warfare.rs`,
  `order.rs`, and `crisis_wars.rs` — deterministic spawn/restore, travel,
  delay, force lifecycle, operations, combat, order, and exact-war contracts.
- `crates/aeon_client/src/scene.rs`, `camera.rs`, `view.rs`, `map_modes.rs`,
  `map_overlay.rs`, and `ui/inspector.rs` — system/body views, projections,
  map questions, selection, force overlays, and linked inspectors.
- `assets/content/system/bodies.rhai`, `assets/content/system/provinces.rhai`,
  `assets/content/scenario/ashkarr-succession.rhai`,
  `assets/content/core/administration.rhai`, and
  `assets/content/core/warfare.rhai` — current geography, forces, muster, and
  operation catalogue.

## Open questions

- How should future route danger combine path conditions, relationships, escort,
  and the player's safety-versus-speed preference?
- Which of terrain, orbit, intelligence, morale, and local support should
  become explicit engagement data, and which should remain folded into
  command, supply, order, and authored assignment difficulty?
- What information and actions are available for forces in transit, including
  cancellation, rerouting, interception, and estimated arrival?
- How do gate ownership, pathfinding, expedition supply, and inter-system order
  delay work in the finished campaign without making distance opaque?
- What naval conflict exists beyond blockade and trade carriage: interception,
  escort, ship damage, capture, destruction, and repair are not yet specified
  here or implemented in the current force model.

## Related GDD sections

- [Assignments, Forecasts, Plans, and Goals](03-assignments-forecasts-plans-and-goals.md)
- [Economy, Order, Obligations, Situations, Intrigue, and Warfare](05-economy-order-obligations-situations-intrigue-and-warfare.md)
- [Interface, Accessibility, Art, Audio, and Narrative](07-interface-accessibility-art-audio-and-narrative.md)
