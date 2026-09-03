# Last Aeon — Economy, Order, Obligations, Situations, Intrigue, and Warfare

| Field | Value |
| --- | --- |
| Document status | Draft 0.1 |
| Scope | Interlocking consequence systems in the Ashkarr playable slice |
| Primary design authority | `pasm/spec/` |
| Current implementation | `crates/aeon_sim/`, `crates/aeon_client/`, and `assets/content/` |
| Setting authority | `the_last_aeons/` |

**GDD navigation:** [Index](README.md) · [Overview](overview.md) · [Previous: Map, Presence, Travel, Ships, and Armies](04-map-presence-travel-ships-and-armies.md) · [Next: AI Agency and Information Rules](06-ai-agency-and-information-rules.md)

This document specifies the systems through which political and military action
changes the campaign. It does not replace the PASM model or the assignment rules.
Where a number below describes current behaviour, the code remains authoritative
until that number is separately accepted as a design commitment.

## Status language

- **Implemented/current** means the repository presently behaves this way and
  tests exercise it.
- **Accepted design** means PASM records the choice as accepted, whether or not
  every desired presentation surface is complete.
- **Future proposal** means an option for later approval. It is not a commitment
  and must not override an accepted PASM decision.

The Living Economy is both **accepted and implemented**. It extends rather than
replaces the current organisation resource pools. Wealth, manpower, supplies,
influence, and legitimacy remain the strategic budget; typed goods, buildings,
and transport routes make territory and logistics more specific beneath that
budget. Nothing in the accepted model creates floating prices, inventories, or a
general market.

## Purpose

These systems make consequences travel across the game instead of ending at one
assignment result:

- territory generates capacity, but disorder and blockade reduce its yield;
- hunger, military occupation, and blockade erode order;
- low order weakens production and defence and can culminate in a visible revolt;
- favours, promises, and grievances preserve named political reasons independently
  of opinion;
- Situations turn authoritative state into legible, actionable continuing crises;
- intrigue changes people, works, order, and obligations without becoming a
  second action model;
- warfare uses characters, forces, supplies, territory, formal political sides,
  and the same assignment pipeline as peaceful work;
- goods and trade give transports an economic role and let formal blockade cut
  an inter-world supply line.

The intended feeling is a tightly connected political ecology. A ruler should be
able to trace a hungry moon to a blocked route, a blocked route to an exact war,
disorder to lost output, and a hostile choice to the grievance or ambition that
made it worthwhile.

## Player experience

The player reads pressure, chooses a response, commits a suitable character or
force, and watches the effects propagate over daily and monthly time. They should
be able to answer five questions before acting:

1. What is wrong, wanted, owed, or threatened?
2. Which person, province, organisation, force, obligation, or war occurrence is
   involved?
3. Which actions are valid, what do they cost, and what risk do they carry?
4. Which consequences are immediate, and which depend on later daily or monthly
   pulses?
5. Where can the result be inspected afterwards?

The systems deliberately offer several forms of power. Administration can repair
order; construction can answer want; shipping can bridge it; political work can
create or discharge obligations; intrigue can undermine a rival; an army can raid
without declaring a war; and an exact formal war can authorise siege, occupation,
and blockade. These are connected alternatives, not interchangeable buttons.

## System relationship at a glance

| Input or action | Immediate authoritative effect | Continuing consequence |
| --- | --- | --- |
| Provincial holding | Monthly wealth, manpower, and supplies | Yield scales with order; wealth is halved by an active blockade |
| Typed production and consumption | A derived per-body goods balance | Want creates privation pressure; surplus creates monthly wealth |
| Building assignment | Adds a validated building on success | Changes goods rates and may add wealth output |
| Transport route | Derived relief between two bodies | Answers want and earns a margin while both docks remain open |
| Garrison or house-member presence | Attendance pressure | Repairs damaged order toward the settled baseline when no harm applies |
| Occupation, blockade, or shortage | Daily order loss | Lower output and defensive reliability; prolonged critical order causes revolt |
| Favour, promise, or grievance | A distinct ledger record | Situation, assignment, plan, or agency reasoning can use its kind and weight |
| Situation trigger | One occurrence-stable continuing card | Projects warnings, facts, participants, links, actions, and a frozen resolution |
| Intrigue result | Condition, order, building, or grievance effect | Removes actors, creates disorder, changes production, or motivates retaliation |
| Deniable raid | Operation-local hostility, loot, and order damage | May trigger reactive defence; does not create formal-war membership |
| Formal war | Two explicit, frozen organisation sides | Authorises exact-war siege, hostile occupation, blockade, adoption, and peace |

## Strategic resources and monthly economy

### Implemented/current

Each organisation holds five strategic values:

| Resource | Current meaning |
| --- | --- |
| Wealth | Fungible economic capacity spent by assignments and gained from holdings, surplus goods, trade, and some operations |
| Manpower | People available for work and armies |
| Supplies | Material support for assignments and military forces |
| Influence | Spendable political capital; monthly recovery is tied to effective legitimacy and capped by it |
| Legitimacy | Non-spendable standing on a 0–100 authored base, modified by authoritative political rules |

On the monthly pulse, each organisation-held province contributes its authored
wealth, manpower, and supplies. Order applies one common output multiplier:
`(200 + order) / 1000`, with order clamped to 0–1000. Current code therefore
produces 20% of authored output at order 0, 100% at the settled start of 800,
and 120% at order 1000. An active, exactly valid blockade halves the province's
wealth before that multiplier; it does not directly halve manpower or supplies.
Building wealth bonuses join the province's base wealth before scaling.

Influence then recovers by one tenth of effective legitimacy, up to effective
legitimacy. Assignment affordability and spending use the same organisation
resource component. Costs are not silently borrowed.

### Accepted boundary

The strategic pools remain abstractions. The goods layer gives location and
logistics meaning but does not replace wealth, manpower, supplies, or influence
with item-by-item accounting. A future change to that boundary requires a new
accepted decision.

## The Living Economy: goods, buildings, and trade

### Implemented/current and accepted

The accepted model is logistics, not a market. All authoritative flows use
integers and stable ordering.

- Goods are authored typed definitions with a fixed value. Current core content
  defines grain at value 3 and ore at value 5.
- Provinces author production and consumption rates. A celestial body's balance
  is derived whenever read by summing its provinces; it is not a stored stockpile.
- Provinces on the same body net freely. Goods never net automatically between
  bodies.
- A body is in want if any native deficit remains after active route relief.
  That one boolean feeds the existing shortage/privation pressure on every
  province of the body.
- A positive body surplus sells monthly at its authored good value. Proceeds are
  split in equal per-province shares among the organisations holding provinces
  there. Integer division is intentional.
- Buildings are authored content and persistent province state. They alter
  production, consumption, and/or wealth only after the corresponding validated
  construction assignment succeeds.
- The current granary costs 60 wealth and 20 supplies, takes 120 days, adds 3
  wealth and 12 grain production, and is raised through the ordinary assignment
  effect pipeline.
- A trade route belongs to a transport ship and names one good, one source dock,
  one sink dock, and a quantity. Its docks must be on different bodies.
- Route relief is derived from valid, running routes. Only delivery that answers a
  real native deficit earns its carrier the good's fixed-value margin.
- A blockade at either dock stops relief and profit. The route itself may remain
  recorded; its economic effect is inactive until the route is valid again.
- Ships visibly shuttle through existing transit and docking machinery, but goods
  are not simulated as cargo entities. Economic relief is derived from the running
  route rather than the ship's momentary position.
- Idle non-player transports can deterministically select an obvious
  surplus-to-deficit route. The player command API can set or clear a route.

The authored Ashkarr slice makes the planet a grain exporter, makes Vesk and the
Aurelian Spire grain consumers, gives Vesk ore production, and provides Harrow and
Veyrin transports. A dedicated client route-management or full goods-balance
surface is not evident in the current UI; this is a presentation gap, not evidence
that the simulation is unimplemented.

### Explicitly outside the accepted model

- floating or negotiated prices;
- price discovery and arbitrage simulation;
- stored body inventories or decaying stockpiles;
- physical cargo-unit entities;
- automatic inter-body netting without a transport route.

These are future proposals only if separately approved.

## Provincial order

### Implemented/current and accepted

Order is a bounded persistent province value, not a separate city-management
game. It begins at 800, is clamped from 0 to 1000, and connects presence,
economy, logistics, warfare, and revolt.

Daily pressures are inspectable and additive when harmful:

| Pressure | Current daily change |
| --- | ---: |
| Hostile formal-war army occupying the province | -6 |
| Active exact-war blockade | -3 |
| Organisation supplies at zero or body in want | -2 |
| No harmful pressure, with holder garrison or living house member present, below 800 | +2 |
| Quiet, unattended province | 0 |

Any harm overrides restorative attendance for that day. Attendance repairs only
toward 800; order above 800 requires deliberate authored effects. Current direct
effects include patrol (+80), foment unrest (-250), sabotage works (-80), a
successful raid (-150), and the immediate bite of a newly established blockade
(-20 plus the captain's command, clamped to 0–20). A successful siege resets the
conquered province to 350.

At or below 200, the visible unrest clock advances. If order rises above 200 the
clock resets. After 120 consecutive critical days the province title becomes
vacant and order resets to 400. Revolt is therefore telegraphed and recoverable;
it is not a hidden irreversible collapse.

Order also changes military defence through a factor of
`600 + floor(order / 2)` permille. The same authoritative order component survives
snapshots and replay.

[ai] The First Reign household demand leans on this shape deliberately:
Kessarin asks for every held province at 850 or better, above the 800
attendance ceiling, so meeting her requires the deliberate authored
routes (estate management, holding court, touring) rather than waiting
out passive recovery.

## Political obligations

### Implemented/current and accepted

Favours, promises, and grievances are explicit bilateral organisation facts,
separate from character opinion. An organisation can dislike a creditor while
still owing a favour, or like another house while still carrying a grievance.

Each record preserves a monotonic ID, optional authored source, kind, debtor,
creditor, plain-language origin, creation date, optional expiry, non-negative
weight, and status. Status is open, fulfilled, broken, or expired. Settled entries
remain in creation order for history and replay; the daily cleanup expires due
entries rather than deleting them.

The summary standing from debtor to creditor adds open favour and promise weights
and subtracts open grievance weights. That summary supports display and agency,
but never replaces the individual facts. When settling by kind and parties, the
oldest matching open entry is settled first. Self-obligations are refused.

The Ashkarr scenario seeds multiple cross-house favours, promises, and grievances.
Intrigue exposure can create a new grievance. The favour-debt Situation exposes a
specific open favour and offers the creditor a contextual call-in-favour action.
The organisation inspector shows open obligations, their direction, origin,
expiry or permanence, and weight.

## Situations

### Implemented/current and accepted

Situations are authored continuing interpretations of authoritative state. They
do not own a parallel copy of political, economic, or military truth.

Each instance has a structural identity: definition, source, and ordered semantic
bindings. Each activation adds its activation date, so a later recurrence of the
same structural crisis is a new lifecycle. Current source and binding vocabulary
can name scenario, body, province, character, organisation, title, office, army,
ship, assignment, obligation, and exact war subjects.

On the daily refresh, validated Rhai triggers discover active instances. A
projection chooses one declared stage and may return a deadline, warning, metrics,
participants and role groups, navigation links, contextual assignment actions,
and frozen values for resolution text. When the trigger ends, a declared outcome
is selected in authored order and a dismissible resolution is retained.

The current reusable deck contains:

| Situation | Source | Main current purpose |
| --- | --- | --- |
| Planetary succession | Paramount title | Claims, rival realms, challenges, and paths to press or renounce |
| Consular vacancy | Consular title | Candidate standings and political actions |
| Favour debt | One obligation | Private, actionable debt between its two parties |
| Formal war | Scenario, bound to one exact war | Sides, participating forces, objectives, adoption, operations, and peace |
| The Court Awaits | Scenario, bound to House Harrow | Day-one authority test: any accepted ordinary assignment within seven days, or a 10-Influence forfeit |
| Kessarin's Order | Scenario, bound to House Harrow and its requester | Household demand: every held province at 850+ Order by the shared 120-day deadline, with four-tier relationship consequences |
| Aleyn's Levies | Scenario, bound to House Harrow and its requester | Household demand: at least 1,000 total fielded army manpower by the same shared deadline, with the same four-tier consequences |
| Torvald's Standing | Scenario, bound to House Harrow, its requester, and the exact liege head | Household demand: the bound liege head's opinion of the house head at 0 or higher by the same shared deadline, with the same four-tier consequences |
| The Liege's Visit | Scenario, bound to House Harrow and the live liege head | [ai] First-year windowed hosted visit (days 140–180): three hospitality tiers whose live forecasts read the liege head's current opinion, a slight for an unanswered window, and passed-on adaptation when the bound head dies, is deposed, is replaced, or cannot travel |
| Unquiet Holdings | Scenario, bound to the targeted holder, the targeted province, and (structurally, outside the audience) the culprit organisation | [ai] The covert-interference alarm: while a covert province-aimed operation runs against another holder's ground, the holder sees the province, its live Order, the exact resistance shift that Order applies, and the remaining time — and the hand only once an ordinary investigation has proved it. The card ends passed-on, struck, or weathered by a pure live-Order reading, each of the last two in a traced and an untraced form |
| A Cold Border | Scenario, bound to House Harrow and one cold surface neighbour | [ai] The open half of the intrigue arc: while a neighbour's head regards the house head at or below the authored -10 floor, or the house owes it an open grievance, the card shows the live regard, the floor, the +20 reconciliation line, the grievances owed, and any formal war between the houses, and offers courting and sending gifts as ordinary actions. It reads public relationship facts only — never a plan, goal, operation, or exposure record — and ends passed-on, reconciled, or eased with no effects. Two open the reign: Draksha's head is as cold toward Edrun as Vantar's |

[ai] The Court Awaits slice added three reusable seams the deck may now use.
Situation call contexts carry the instance's activation date (triggers see
the oldest live activation for their definition and source, or none), and
the shared world view exposes the campaign start date, so authored deadlines
such as "seven days after activation" are computed in content. A definition
may declare an optional table-decided activation announcement: activation
then raises an ordinary pausing acknowledgement popup for audiences the
player may see, stating the demand and its consequence in advance. And an
outcome may declare an `effects_fn` plus a definition-level `owner_binding`:
when that outcome resolves, its typed effects (including the new exact
signed `resources` effect) are applied once for the bound organisation,
through the same effect boundary and provenance tagging as assignment
results. All authored magnitudes — the seven days, the 10 Influence — live
in scenario content, not in Rust.

[ai] Kessarin's Order added two further reusable seams. A definition may
declare pure **responses** — political answers with no assignment behind
them, such as promise and refuse. The player records one through the new
`AnswerSituation` command; the accepted answer is durable authoritative
state keyed by the exact activation, snapshotted, logged as tagged
history, shown on the card, and exposed to scripts as `ctx.answer` beside
`ctx.activated`, so outcome predicates can distinguish achievement,
refusal, silence, and a broken promise months later. The first answer per
activation is final, and a reactivation or requester replacement is a new
activation that starts unanswered. A definition may also declare a
`subject_binding` naming a character binding: when outcome effects or the
activation announcement resolve their roles, that character stands behind
the existing `target` effect role, so content can write directional
personal consequences — the requester's opinion of the head — through the
unchanged seven-role vocabulary. Household goals themselves are
live-state predicates recomputed from current holders and Order, resolved
on the first settled day they hold (achievement beats any recorded
answer) or by answer tier on the shared deadline day; all magnitudes —
the 850 target, the 120 days, the four opinion tiers — are authored in
scenario content.

[ai] Aleyn's Levies, the second household demand, reuses those seams
unchanged — no new Rust was needed. Its predicate aggregates fielded
manpower over the armies the house owns (the shared world view's army
records), its per-army card rows and navigation links use the existing
army subject kind, and its muster action is an ordinary assignment whose
forecast can fail with costs paid up front and never refunded; a failed
attempt receives no protection and a retry is the same ordinary command.
The 1,000-manpower target and every tier live in scenario content beside
Kessarin's.

[ai] Torvald's Standing, the third household demand, adds the
derived-relationship predicate class and one structural seam. The
demand binds the exact liege head beside the house and requester, so a
changed liege or a dead or deposed liege head is a different structural
instance: the old lifecycle ends passed-on with no tier, and any
remaining concern is judged afresh from live state. Its achieved
outcome is judged only against the bound man while he still stands as
the liege's living head — the ended lifecycle's outcome predicates run
against the live world with the old bindings, so without that clause a
liege succession would pay the achievement tier from the successor's
warmer derived affinity in the very pass that should pass the demand
on. The goal itself follows the office of head-of-house, matching the
owner-head role the tiers pay toward; it reads the same derived opinion
facts scripts already see, is satisfied by any legitimate relationship
effect, and its one authored route is the ordinary court assignment,
org-targeted at the liege. With the three demands live together, an
unmet shared deadline resolves all of them in the one evaluate pass, in
stable definition order with distinct tiers on distinct requesters'
ledgers, and each demand otherwise resolves independently with durable
history.

[ai] The Liege's Visit, the First Year arc's first slice, adds the
windowed hosted-Situation shape and consumes the new forecast seam. Its
trigger is live on any settled day in the authored window on which the
house stands and holds ground, the liege's living head exists, and a
breadth-first search over the authored route graph reaches a held
province from his concrete location — reachability is asked of the same
route facts real travel uses, computed in content, never assumed. The
trigger binds the live liege head, so death, deposition, or a changed
liege ends the bound lifecycle passed-on with no effects and, while the
window and travel allow, binds the successor's own visit; the closing
day itself accepts hospitality under the court's deadline rule, and a
window left wholly unanswered while the bound head still stands and can
come resolves slighted with one authored opinion penalty through the
subject binding. Hosting is three ordinary `ai_available: false`
assignments with authored rising cost, falling difficulty, rising
duration, per-tier live-opinion forecast modifiers, and graded opinion
results; acceptance in-window resolves the Situation hosted and the
reception's own results then carry the consequences. Every tier's
duration is at least the window's length, which is what makes the visit
one-shot without any new snapshot state: accepted work is still running
whenever the window could re-ask. The projected tier actions pin no
leader — the host is the player's choice through the ordinary
composition popup and free picker.

Visibility follows authoritative audience rules. In particular, favour debt is
private to its parties in player-led play while spectator mode can inspect it.
Situation-started assignments retain their Situation occurrence, target, and exact
war provenance through queuing, snapshot, result logging, and resolution history.

Malformed trigger, projection, or outcome data does not silently fabricate a
card. The instance becomes unavailable, a deterministic diagnostic is logged once,
and diagnostic state survives snapshots. A later war occurrence cannot inherit a
queued action or history from an earlier war merely because the participants are
similar.

The client orders completed resolutions first, then warnings, then other active
cards. It displays stages, deadlines, metrics, participants, links, action
availability, forecasts, and provenance-tagged history.

## Intrigue

### Implemented/current and accepted

Intrigue is authored assignment content using the same leader, forecast, phase,
risk, validation, effect, log, and persistence rules as every other assignment.
It does not gain a privileged mutation path.

Current core operations are:

| Assignment | Target | Current consequence |
| --- | --- | --- |
| Assassinate | Character | Success or critical success applies death; exposure creates a grievance |
| Abduct | Character | Success applies capture; exposure creates a grievance |
| Foment unrest | Foreign-held province | Success removes 250 order; disaster exposes the operation and creates a grievance |
| Sabotage works | Foreign-held province | Success wrecks a building and removes 80 order; disaster creates a grievance |

These assignments declare intrigue difficulty, duration, explicit personal risks,
and weighted results in Rhai. Current definitions are not available to the simple
reactive AI scorer; authored plans or larger goals can call them. This is an
accepted guard against arbitrary murder or sabotage, not a rule that all future AI
must remain incapable of intrigue.

[ai] The First Year intrigue slice makes the province operations genuinely
contested and, for fomenting unrest, genuinely deniable:

- Both province operations author an `order_modifier` (reference 800,
  4 hundredths per Order point, clamped −8..0): the target province's live
  Order is read into the one shared effectiveness calculation, so a
  well-kept province materially worsens the hostile odds while disorder
  never helps beyond neutral. This is the accepted "Order is resistance"
  rule — no hidden detection statistic exists. The forecast, the Unquiet
  Holdings card, and the resolution roll all quote the same live number.
- `foment-unrest` is authored `covert: true` and answers the new subvert
  pressure. An AI house reaches it only through the covert
  `deniable-pressure` plan, adopted under the covert
  `undermine-a-neighbour` ambition: a vassal head, inside the authored
  day 180–260 window, with the authored capability floor, against a
  hostile border neighbour (head-to-head opinion at or below −10, or an
  open grievance owed). In the Ashkarr scenario that chain resolves to
  House Vantar working Vhorruk, the one Harrow province across its
  border. The player may order the same operation by hand and receives
  the same deniability.
- The targeted holder experiences the operation as the Unquiet Holdings
  Situation; counter-play is ordinary administration (raise Order),
  investigation, retaliation, reconciliation, or accepting the
  risk. Provenance rules are in
  [AI Agency and Information Rules](06-ai-agency-and-information-rules.md).
- [ai] **Investigation** is the second answer, and an ordinary assignment:
  `trace-the-hand` — intrigue, authored cost, difficulty, and duration,
  closed to the AI — aimed at the holder's *own* troubled province rather
  than at any suspect, because there is no suspect to aim at. The card
  offers it pinning no leader, so the player compares every eligible
  investigator on the same authoritative per-candidate forecast the order
  will use. Its two good results prove the organisation the lifecycle
  already bound; its two bad results author nothing whatever. A proved
  card names the house, links its head and the covert work itself, and
  freezes a resolution that says so; the live-Order reading that decides
  struck from weathered is unchanged, because proof is knowledge and never
  protection. Discovery is durable campaign state, recorded per
  discovering house, and is what allows retaliation or a demanded
  settlement to be aimed at anybody at all.
- [ai] **Reconciliation** has explicit authored rules, and they touch only
  what is uncommitted. The ambition's selector and the campaign's method
  gates open at -10 or with a grievance owed; above -10 the method recheck
  starts no new step and lets an uncommitted campaign go; at +20 with no
  grievance owed and no formal war between the houses, the campaign's
  `abandon_when` and the ambition's `set_aside_when` — three predicates in
  the shared plan vocabulary: `min_target_head_opinion`,
  `target_owes_no_grievance`, `at_war_with_target` — let both go, the
  ambition without a cooldown. A sabotage already accepted resolves through
  Unquiet Holdings on its ordinary day, and a war ends only by negotiated
  peace. The A Cold Border card is where the player reads these terms, and
  `send-gifts` is the second ordinary lever beside courting, with its own
  opinion reason, so two ordinary successes reach the line from the
  opening standing. No authored effect settles a grievance: a wronged
  house keeps its grounds until the ledger says otherwise.

Covert intrigue is not formal war. Its hostile consequence is the operation and
its effects; it does not put organisations onto war sides, authorise occupation,
siege, or blockade, or make every foreign force hostile.

## Formal war and operational warfare

### Formal-war authority

A formal war is an authoritative, occurrence-identified conflict. It records one
stable War ID, cause, declaration date, two explicit sides, side leaders,
participating organisation branches, adoption history, active state, and frozen
conclusion. Concluded wars remain historical records.

At declaration, each leader's current transitive vassal branch forms a side. The
two branches must be non-empty and disjoint. In rebellion or an attack down the
same hierarchy, the descendant branch is cut out of the ancestor's side rather
than appearing on both sides. Eligible nonparticipating lieges can explicitly
adopt the side containing their vassal, adding their valid branch. Only a current
side leader may settle the whole war.

Hostility is never inferred from foreignness, proximity, bad opinion, or a
historical war. A siege or blockade must carry one exact active War ID and the
force owner and current target holder must be opposing members of that occurrence.
Peace aborts war-bound work and invalidates blockades. A title transfer that makes
a blockade stale clears its marker so a later holder change cannot reactivate it.

### Deniable operations are different

A raid may target a foreign province without a formal war. Its hostility is
**deniable and operation-local**: it authorises the raid's contest and can trigger
the defender's standing response to that raid, but it does not establish general
hostility, occupation pressure, war membership, siege authority, or blockade
authority. A peaceful foreign army standing in a province is not hostile and does
not reduce order.

This distinction is a hard rule. Implementations and UI labels must not collapse
“a foreign force,” “a deniable hostile operation,” and “an opponent in this exact
formal war” into one broad hostile relation.

### Current operations

| Operation | Rule and result |
| --- | --- |
| March/respond | Moves an army through assignment and travel rules; respond is raised by valid standing defence |
| Resupply | Draws available organisation supplies into the selected army |
| Patrol | Requires presence and can restore provincial order through authored effects |
| Besiege | Requires exact formal-war authority; after its staged assignment, resolves any field defence and transfers the province title on success |
| Raid | Needs no formal war; contests a defender locally, steals 10% of holder wealth up to 100, and removes 150 order |
| Blockade | Requires exact formal-war authority and the selected ship's captain; moves the ship to the dock, damages order, halves wealth, and interdicts routes at that dock |
| Negotiate | Targets one exact war; on success a side leader concludes it in negotiated peace |

Military assignments are led by the army's general or ship's captain where the
operation requires that asset. Engagement strength uses manpower, general command
(+5% per point), supply state (zero supplies reduces strength to 60%), and a 20%
home-ground defence bonus. Defender strength is then scaled by provincial order.
A purpose-derived deterministic stream adds a bounded ±15% swing. Winners lose
5–15% manpower and losers 20–35%; losers retreat to the nearest holding or
disband. There is no tactical battle layer.

Standing defence starts an ordinary visible response assignment only for an idle
army. Bespoke work takes precedence. It answers a raid aimed at the owner's
holding, or siege/blockade/hostile presence involving an opponent on the exact
formal-war side; peaceful foreign presence alone does nothing.

## Data and authoring rules

| Data | Owner and persistence |
| --- | --- |
| Organisation resources | Runtime component; snapshotted |
| Province base outputs and goods rates | Validated content definitions |
| Goods and buildings | Validated authored definitions |
| Built buildings | Ordered runtime province component; snapshotted |
| Trade route | Runtime transport state; snapshotted; relief derived |
| Provincial order and unrest days | Runtime province component; snapshotted |
| Obligations | Append-preserving runtime ledger; snapshotted |
| Situation definitions and attachments | Validated Rhai content |
| Situation active lifecycles, resolutions, and diagnostics | Runtime state; snapshotted |
| Intrigue and warfare pacing/results | Authored assignment definitions |
| Military operation semantics | Authoritative engine code |
| Formal wars | Occurrence-keyed runtime registry; active and concluded records snapshotted |

Content may read validated context and emit typed effects. It may not directly
mutate simulation state. References to goods, buildings, assignments, Situation
stages/actions/outcomes, forces, provinces, and other content must validate before
campaign play. Stable IDs and stable iteration order are gameplay requirements,
not implementation conveniences.

## Edge cases and required behaviour

- A quiet damaged province does not heal without attendance; attended ground does
  not heal while an active harmful pressure remains.
- Order never leaves 0–1000. The unrest clock runs only at or below 200 and resets
  immediately above that threshold.
- A revolt vacates the province title, retains no hidden rebel faction, and leaves
  the province recoverable at order 400.
- Unheld provinces do not pay organisation income. Goods rates still contribute
  to their body's derived balance because the balance belongs to geography, while
  surplus proceeds distribute only through held province shares.
- A body with several deficits is in want if any remains unanswered.
- Route delivery can remove want, but profit is capped by real native scarcity;
  shipping unwanted goods does not print wealth.
- Only a transport can receive a valid route; source and sink must be on different
  bodies and the good must exist.
- A blockade matters only while its ship is docked and its owner opposes the
  current holder in the bound active war.
- A large friendly or peaceful army cannot hide a smaller exact-war hostile army
  when hostile garrison strength is determined.
- An unrelated active war does not authorise an operation. A concluded war does
  not authorise one. A redeclared war is a new occurrence.
- Raids remain deniable even when no formal war exists; their local hostility must
  not leak into general map hostility or occupation order pressure.
- Obligations between identical debtor and creditor are refused; negative authored
  weight is clamped away; expiry settles rather than deletes.
- Repeated Situation structural keys create new dated lifecycles. Resolution and
  log history must not bleed between occurrences.
- Situation action commands revalidate the declared action, target, leader,
  lifecycle, and exact war when they execute; forged or stale identities fail.
- Destroyed, captured, dead, absent, reassigned, or otherwise ineligible leaders
  and forces are handled through assignment validation rather than special UI
  exceptions.

## Feedback and presentation

### Implemented/current

- The top-level ledger panel displays organisation resource totals alongside map
  context.
- Province inspection shows base outputs, current order as a percentage with a
  pressure explanation, revolt countdown when critical, and built buildings.
- Organisation inspection shows open obligations with direction, status context,
  origin, duration, and weight.
- Army inspection exposes standing orders; commands use the authoritative standing
  order vocabulary.
- Map modes expose ownership and hostile military presence using exact formal-war
  predicates.
- Situation cards display warnings, stages, deadlines, metrics, participants,
  links, action availability, forecasts, and tagged history. Resolution notices are
  dismissible through a player command.
- Assignment results and order/revolt changes use the campaign log; selectively
  important results may also use popups.

### Required presentation work before final

The simulation already supports more than the current client makes conveniently
inspectable. A final GDD should settle, and the client should then expose:

- a per-body goods balance explaining production, consumption, route relief,
  remaining want, and surplus value;
- creation, editing, clearing, validity, journey, interdiction, and profit of a
  player's transport routes;
- explicit visual distinction between peaceful foreign presence, a deniable raid,
  and exact formal-war hostility;
- exact war identity, cause, leaders, side membership, adoption history, operations,
  and conclusion without relying only on Situation prose;
- a clearer before/after explanation when order scales monthly output or defence;
- a persistent historical obligation view if settled records are intended to be
  player-readable rather than only replay-preserved.

These are requirements to specify and verify, not approval for a particular UI
layout.

## Dependencies

- Character identity, organisation headship, vassal branches, holdings, titles,
  opinion, legitimacy, succession, and obligation ownership are defined in
  [Characters, Organisations, Politics, and Succession](02-characters-organisations-politics-and-succession.md).
- Assignment composition, forecasts, phases, typed effects, plans, goals, logging,
  and persistence are defined in
  [Assignments, Forecasts, Plans, and Goals](03-assignments-forecasts-plans-and-goals.md).
- Province/body identity, force presence, travel, ships, armies, commanders, and
  order delay are defined in
  [Map, Presence, Travel, Ships, and Armies](04-map-presence-travel-ships-and-armies.md).
- Agency scoring, private information, spectator visibility, and AI use of
  obligations, order, trade, plans, and war are defined in
  [AI Agency and Information Rules](06-ai-agency-and-information-rules.md).
- Client layout, accessibility, sound, and narrative hierarchy belong in GDD 07.
- Schema validation, deterministic replay, balance fixtures, and release evidence
  are defined in
  [Content, Balance, Verification, and Acceptance](08-content-balance-verification-and-acceptance.md).

## Acceptance criteria

### Baseline: current and accepted behaviour

- Monthly held-province output reaches the correct organisation, uses the current
  order multiplier, halves only blockaded wealth, includes building wealth, and
  recovers influence to the correct cap.
- Province order is bounded and replay-stable; each pressure produces the stated
  daily delta; attendance repairs only in the absence of harm; 120 consecutive
  critical days vacate the title and reset order.
- Goods balance is derived per body in stable order. Want applies privation;
  surplus distributes deterministic integer wealth; buildings alter the derived
  balance only after a valid assignment.
- A valid transport route answers a real inter-body deficit, earns only the margin
  corresponding to answered scarcity, survives snapshot, and ceases to relieve or
  profit under an active blockade at either dock.
- Obligations preserve individual origins, weights, parties, dates, and terminal
  status; daily expiry and oldest-first settlement replay identically.
- Every current Situation opens from authoritative triggers, projects valid
  declared content, obeys audience visibility, starts only a revalidated action,
  freezes a resolution, and keeps occurrence-specific history.
- Runtime Situation errors are deterministic, visible, logged once, and preserved
  through restore without crashing or inventing state.
- Intrigue applies effects to the intended target, wrecking updates a real
  building and order, exposed actions create the specified grievance, and affected
  characters obey later eligibility rules.
- Formal war creates two valid disjoint sides, preserves one exact occurrence,
  partitions rebellion branches correctly, controls adoption and peace authority,
  and retains its concluded history.
- Siege, blockade, occupation, standing defence, trade interdiction, and map
  hostility all use shared exact-war predicates. No unrelated or concluded war
  authorises them.
- A raid works without formal war but its hostility remains operation-local. A
  peaceful foreign force creates neither occupation pressure nor reactive defence.
- Engagements are deterministic for the same seed and stable identities, use the
  documented strategic inputs and bounded swing/loss bands, and correctly apply
  retreat or destruction.
- Snapshot, command replay, and state hash reproduce resources, order, buildings,
  routes, obligations, Situations, forces, and formal wars.

### Before this GDD can become final

- The client presentation gaps above have an accepted information design and
  corresponding automated or visual acceptance evidence.
- Balance targets are established for resource abundance, order recovery and
  collapse, building payback, route value, military costs, operation durations,
  and intrigue risk without changing the accepted arithmetic by implication.
- The design owner decides whether historical settled obligations need a dedicated
  player-facing ledger.
- The design owner decides how much goods and route automation the player may
  delegate, while keeping the accepted physical transport and blockade rules.

## Evidence and traceability

### PASM

- `pasm/spec/core/game-vision.yaml` — formal war, operational warfare,
  provincial order, obligations, Situations/contextual events, Shadows, and the
  Goods Economy.
- `pasm/spec/architecture/implementation-decisions.yaml` — exact-war identity,
  standing defence, deterministic arithmetic/ordering, derived goods balance,
  inter-body trade, and related implementation choices.
- `pasm/spec/roadmap/milestone-2-consequences-and-clarity.yaml` — order,
  obligations, Situations, and exact formal-war consequence pass.
- `pasm/spec/roadmap/milestone-9-the-living-economy.yaml` — accepted goods,
  buildings, routes, authored scenario economy, and evidence.
- `pasm/spec/roadmap/milestone-10-shadows.yaml` — accepted intrigue expansion and
  its relationship to plans, goals, buildings, and grievances.

### Simulation and tests

- `crates/aeon_sim/src/economy.rs`, `order.rs`, `trade.rs`, and
  `obligations.rs` — strategic resources, monthly accrual, order, goods, routes,
  buildings, and the political ledger.
- `crates/aeon_sim/src/situations.rs` — occurrence identity, projection,
  visibility, resolution, diagnostics, and persistence.
- `crates/aeon_sim/src/wars.rs` and `warfare.rs` — side authority, adoption,
  conclusion, exact hostility, strategic engagements, operations, and standing
  defence.
- `crates/aeon_sim/tests/order.rs`, `trade.rs`, `intrigue.rs`, `situations.rs`,
  `situation_war_contract.rs`, `warfare.rs`, and `crisis_wars.rs` — executable
  contracts for the rules and edge cases above.
- `crates/aeon_sim/tests/acceptance.rs` — seed-pinned full-scenario goods flow,
  blockade, Situation, and formal-war evidence.

### Content and client

- `assets/content/core/economy.rhai`, `intrigue.rhai`, `situations.rhai`, and
  `warfare.rhai` — current goods/buildings and assignment/Situation catalogues.
- `assets/content/system/provinces.rhai` and
  `assets/content/scenario/ashkarr-succession.rhai` — authored outputs, goods,
  forces, transports, obligations, and the current political field.
- `crates/aeon_client/src/ui/ledger_panel.rs`, `inspector.rs`, and
  `situations_panel.rs`, plus `crates/aeon_client/src/map_modes.rs` — current
  player-facing information surfaces.

## Open questions

These questions do not alter accepted behaviour until answered and recorded:

- What scarcity and recovery ranges should define a healthy early, middle, and
  late Ashkarr economy?
- Should a player transport default to manual routes, suggested routes, or an
  explicit opt-in automation policy?
- How should the interface compare the economic value of building locally with
  the time, opportunity cost, and blockade risk of shipping?
- Should surplus sale and route profit remain silent ledger changes, or receive
  thresholded monthly summaries?
- Which settled obligations deserve permanent player-visible history, and how
  should broken versus expired commitments differ in later political reasoning?
- What information about covert intrigue is hidden before exposure, and what
  evidence can make suspicion legible without revealing authoritative truth?
  [ai] **Resolved:** before exposure the culprit
  organisation, leader, and source plan are hidden from every ordinary
  player surface, while the operation's target province, its live Order,
  the exact resistance shift, and the remaining time are openly shown on
  the Unquiet Holdings card — legible suspicion without fabricated
  evidence. [ai] Investigation then reveals the whole of that hidden
  provenance or none of it, at an authored cost in coin and days and on
  ordinary odds: the organisation, its head, and the covert work itself
  appear on the card and in its frozen resolution, and the discovery is
  recorded per discovering house. Nothing partial, graded, or speculative
  is ever shown, because the only alternative to proof is silence.
- What player-facing term best distinguishes deniable raid hostility from formal
  war without implying that raids are consequence-free?
- How should peace terms grow beyond the current whole-war negotiated conclusion,
  if at all, while retaining exact occurrence and side authority?
- Which additional goods, buildings, and trade patterns are necessary for the
  finished-game systems, and which would only add accounting noise?

## Related GDD sections

- [GDD overview](overview.md)
- [Characters, Organisations, Politics, and Succession](02-characters-organisations-politics-and-succession.md)
- [Assignments, Forecasts, Plans, and Goals](03-assignments-forecasts-plans-and-goals.md)
- [Map, Presence, Travel, Ships, and Armies](04-map-presence-travel-ships-and-armies.md)
- [AI Agency and Information Rules](06-ai-agency-and-information-rules.md)
- [Content, Balance, Verification, and Acceptance](08-content-balance-verification-and-acceptance.md)
