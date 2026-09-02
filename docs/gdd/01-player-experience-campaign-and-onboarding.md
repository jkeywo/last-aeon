# Last Aeon — Player Experience, Campaign, and Onboarding

| Field | Value |
| --- | --- |
| Document status | Draft 0.1 |
| Scope | Player experience, campaign shape, onboarding, rhythm, and recovery |
| Current playable scenario | The Ashkarr Succession |
| Primary design authority | `pasm/spec/` |
| Parent document | [Game Design Overview](overview.md) |

**GDD navigation:** [Index](README.md) · [Overview](overview.md) · [Next: Characters, Organisations, Politics, and Succession](02-characters-organisations-politics-and-succession.md)

This document describes what it should feel like to begin, understand, and
sustain a campaign. It does not replace the authoritative rules in PASM or the
system specifications linked below.

Status labels are deliberate:

- **Implemented/current** describes behaviour present in the repository now.
- **Accepted design** restates an accepted PASM decision, including work that
  may not yet be complete.
- **Proposal** is a GDD recommendation for later review. It is not a project
  commitment and does not alter an unmarked PASM decision.

## Purpose

The campaign experience should turn the game's central promise—power belongs
to people who must act somewhere, through someone, over time—into an arc the
player can read and direct. The player should leave an opening session knowing
who they are, what threatens or binds them, which people can act for them, and
how to make one informed decision without needing to understand every ledger.

Thereafter, the campaign should support repeated cycles of observation,
commitment, time, consequence, and reassessment at three connected scales:

- assignments measured in days or months;
- political and economic shifts measured across months and years;
- succession and house ambitions measured across reigns.

The purpose is not to lead the player through a fixed mission sequence. The
Ashkarr Succession is an open political field: survival, loyal service,
independence, the Paramountcy, military dominance, and other ambitions emerge
from the same simulation rather than from a prescribed campaign solution.

## Player experience

### Campaign entry

**Implemented/current.** The client opens on a title screen before any campaign
simulation exists. New Game starts the authored Ashkarr scenario with a fresh
seed. Native builds offer Continue only when a monthly autosave exists, parses,
and matches the embedded content; the browser build currently offers no save
or Continue path. A spectator option starts with no player house, makes House
Harrow autonomous like every other house, and leaves the observational
interface available without player-authority actions.

A new campaign begins paused. The player can therefore inspect the opening
state before the first daily tick. The default layout places the inspector on
the left, Situations on the right, and the campaign log and assignments along
the bottom. Date, House Harrow's resources, pause, three speed settings, map
navigation, panel controls, and search remain accessible in the campaign
shell.

### The authored opening

**Implemented/current.** The player leads House Harrow through its head, Edrun
Harrow, from 1 January 411. Harrow is a legitimate but subordinate House Veyrin
vassal holding four provinces, with an authored household, starting resources,
and a live favour debt to Veyrin. The planet's Paramountcy is vacant. Three
Great Houses contest Ashkarr; eight vassals, two independent houses, and the
Sanctora Imperim complete the opening political field.

This opening should convey four truths quickly:

1. The player is House Harrow across reigns, but acts through particular
   people—initially Edrun and his household.
2. Harrow is constrained by allegiance and obligation rather than starting as
   a sovereign great power.
3. The vacant Paramountcy is a long-horizon opportunity, not a compulsory
   first objective.
4. The world moves when time resumes; rival leaders pursue their own pressures,
   plans, goals, and wars.

### Onboarding experience

**Implemented/current.** There is no separate tutorial mode or first-run
campaign flow in the current client. Learning support is contextual:
the game starts paused; Situation cards expose live strategic problems and
available responses; unavailable actions give authoritative reasons; forecasts
show cost, delay, duration, skill contest, current result odds, effects, recall
limits, and personal risk; panel and map-mode tooltips explain their readings;
the filterable log preserves consequences and links to subjects.

**Implemented/current.** [ai] The First Reign arc now gives that support an
authored day-one shape. The Ashkarr scenario opens with **The Court Awaits**,
an ordinary urgent Situation: the assembled court demands a substantive order,
any accepted ordinary House Harrow assignment answers it within seven days,
and letting the deadline lapse forfeits exactly 10 Influence with a durable
resolution and the campaign continuing. The activation raises a pausing
announcement popup that states the deadline and consequence in advance;
invalid or unaffordable attempts are refused by ordinary command validation
and do not count. A default-on, client-owned **First Reign guidance**
preference (editable on the title screen and in campaign settings) adds an
objective line and optional "Show me how" / "Why this matters" help to guided
cards; the Situation, its deadline, and its consequence exist identically
with guidance disabled, and equal seed and commands produce equal
authoritative hashes either way. [ai] The resolved persistence rule: guidance
shows only facts derived from authoritative state, and the sole persisted
onboarding datum is the preference itself, stored in the client's versioned
local preferences document rather than in any campaign save. The wider
guided-sequence design below remains proposal.

**Implemented/current.** [ai] The first household demand follows the court's
test. Once The Court Awaits is behind the reign — answered, or lapsed —
**Kessarin's Order** opens as an ordinary urgent Situation: Kessarin asks
that every Harrow-held province reach at least 850 Order by a shared
120-day household deadline anchored to the campaign start (the day the
court's window would have closed, plus 120 days — the anchor the two
remaining household demands will share). The goal is a live-state
predicate over current holders and Order; the card shows the live
per-province metric, the target, the deadline, the known ordinary routes
(estate management, holding court, touring the holdings), and the exact
four-tier relationship consequences. The player may promise, refuse,
remain silent, or simply deliver the provinces: achievement resolves the
demand whatever was said (+10 opinion for 1,440 days, Kessarin of Edrun),
an explicit refusal left to stand costs -5 for 1,080 days, silence costs
-10 for 1,440, and a broken promise -20 for 1,800 — every magnitude
authored in scenario content. Promising and refusing are pure recorded
answers through an ordinary logged command, durable in saves and replays;
completion is history while provincial Order keeps moving normally; and a
requester who dies or is replaced passes the demand on without penalty to
a deterministically chosen successor.

**Implemented/current.** [ai] The second household demand, **Aleyn's
Levies**, opens beside Kessarin's on the same shared anchor and runs the
same accepted lifecycle: Aleyn asks that House Harrow field at least
1,000 total army manpower by the shared 120-day household deadline. The
goal is a live military-strength predicate summed over the armies the
house actually owns — any legitimate route to the strength satisfies it,
and losing or disbanding soldiers counts against it while the demand is
open. The card offers the one honest ordinary route, the muster
assignment, whose wealth and Influence costs are paid on acceptance and
whose roll can genuinely fail: a failed muster refunds nothing, forms
nothing, carries no tutorial protection, and leaves the same open
lifecycle awaiting an ordinary retry. Promise, refusal, silence,
achievement, and requester replacement carry the same four authored
relationship tiers and passed-on rule as Kessarin's demand, under
Aleyn's own stable opinion reasons.

**Proposal.** Onboarding should use a short, dismissible sequence of goals over
the live campaign rather than a separate rules sandbox. It should teach the
normal interface and issue ordinary logged commands, without changing odds,
granting resources, freezing rivals, or scripting outcomes. A possible first
session sequence is:

1. **Orient:** identify House Harrow, Edrun, holdings, liege, resources, and the
   two opening Situations.
2. **Read pressure:** open the Veyrin favour and the Empty Paramountcy; compare
   obligation with ambition.
3. **Delegate:** choose an available assignment, compare at least two eligible
   leaders, and read the forecast.
4. **Commit and observe:** issue one ordinary command, resume at a chosen speed,
   and follow its delayed start and resolution through assignments and the log.
5. **Reassess:** inspect the changed subject and choose whether to respond,
   continue the original aim, or let time run.

The proposal intentionally teaches a grammar rather than prescribing a
strategy. It should never imply that declaring for the Paramountcy is the
correct opening, or conceal the possibility of loyalty, independence,
administration, intrigue, or military preparation.

### Session and campaign rhythm

**Accepted design.** Campaign time is pauseable real time resolved as discrete
daily ticks, with slower systems using monthly and yearly pulses. The campaign
is open-ended and has no scripted victory condition. Succession is expected to
change the player's constraints rather than normally end play.

**Implemented/current.** The presentation offers pause and three speeds of one,
three, and ten campaign days per real-time second. Assignment and Situation
interaction automatically pauses when it requires focused player attention.
The client autosaves every thirty advanced campaign days on native builds.
The simulation's shared daily entry point advances commands, travel,
assignments, economy, politics, events, and settled presentation state in a
fixed order.

The resulting intended rhythm is:

- **Pause and read** when a warning, result, death, war, vacancy, resource
  shortage, or new opportunity changes priorities.
- **Compose decisions** by choosing the aim, target, and person, then examining
  the authoritative forecast.
- **Run time** while orders travel and work unfolds, using higher speeds only
  while the current position is understood.
- **Return to the strategic horizon** through plans, house goals, liege
  directives, succession, and the changing balance of realms.

**Open question.** The target length of a play session, the expected number of
reigns in a campaign, and any intended soft retirement or score cadence are not
settled in the reviewed sources. They should not be inferred from the current
simulation speeds or autosave interval.

## Rules

1. **The organisation is the enduring player identity.** In the current
   scenario that organisation is fixed as House Harrow; the current head is the
   principal bearer of its authority, not the campaign's persistent identity.
2. **Campaign actions use authoritative commands.** Meaningful player
   decisions are validated, ordered, and recorded. The UI may explain or
   compose a choice, but does not own its rules.
3. **Time advances only by complete days.** Pause and speed are presentation
   controls; the simulation receives the same ordered daily tick regardless of
   frame rate or client.
4. **The opening does not suspend simulation rules.** AI houses act through the
   shared assignment, plan, and goal machinery. A future onboarding layer must
   not create a second action model.
5. **Succession normally continues play.** For a dynastic house, the legal heir
   is selected from living members in the accepted order: children by age,
   siblings by age, then other living members by age. A weak, unpopular,
   underage, distant, or compromised heir is still the new head.
6. **Personal facts do not automatically survive succession.** In particular,
   a Paramount claim belongs to its claimant, ends when their eligibility
   ends, and must be declared afresh by a later independent head.
7. **Failure is part of play.** Accepted design distinguishes routine failures
   that retry at the cost of time, consequential setbacks with credible
   recovery, and disasters that create a new problem or irreversible loss.
8. **The MVP has no victory trigger.** Securing the Paramountcy is a major
   systemic achievement, not a scripted end screen.
9. **Terminal failure is exceptional.** Accepted design ends the campaign when
   the house has neither a viable successor nor a meaningful territorial
   foothold. The current implementation creates Campaign Over when the player
   house has no viable successor; the territorial-footing terminal rule is not
   evidenced in the present code and remains an implementation gap.

## Data

The campaign experience is assembled from existing authoritative and derived
data rather than from a separate campaign script:

| Data | Role in the experience | Current source |
| --- | --- | --- |
| Scenario identity, start date, player house | Establishes the fixed opening | `assets/content/scenario/ashkarr-succession.rhai` |
| Houses, heads, family, holdings, resources | Defines who the player is and what they can use | Authored scenario content loaded into simulation records |
| Titles, offices, obligations, Situations | Defines opening pressures and opportunities | Scenario and `assets/content/core/` |
| Campaign seed | Makes each New Game differ while remaining deterministic thereafter | Campaign configuration recorded in state |
| Date and pulse boundaries | Establishes daily, monthly, yearly rhythm | `CampaignClock` and the clock schedules |
| Player house, Campaign Over, active work | Controls authority, continuity, and terminal state | Authoritative simulation resources |
| Snapshot and command log | Supports continuation, replay, and diagnosis | Versioned persistence model |
| Text and explanation | Presents mechanics without duplicating them | `assets/text/strings.csv` and derived client views |

**Proposal.** If guided onboarding is accepted, its progress should be explicit,
versioned campaign or profile data with stable objective identities. It should
observe authoritative state and submitted commands, never infer completion
from transient UI clicks alone. [ai] The First Reign slice resolves this for
its own scope: guided objectives are read directly from authoritative
Situation lifecycles and resolutions, so no separate progress store exists,
and the guidance preference persists in the client's versioned interface
preferences document — a local profile datum, never campaign state.

## Edge cases and recovery

- **Missing, unreadable, unparseable, state-hash-invalid, or content-mismatched
  autosave:** leaves Continue unavailable. Full integrity verification is part
  of deciding whether Continue can be offered, not a failure deferred until
  after the click.
- **Browser play:** currently has no persistence. Closing or refreshing loses
  the session; onboarding and release messaging must state this honestly until
  browser storage exists.
- **Spectator mode:** persists as part of campaign political state and restores
  without a player organisation. Every player-only action must remain absent or
  authoritatively unavailable.
- **Leader dies with an heir:** succession changes the acting head and removes
  ineligible personal claims. The player continues with the inherited house
  position, including its problems.
- **No legal successor:** any living character selected by the organisation's
  legal succession rule is viable regardless of age, popularity, health,
  location, captivity, or incapacity. If none exists at the end of a fully
  settled day, Campaign Over records the reason and later player commands fail.
- **Territory is lost:** the dynastic MVP requires the player organisation to
  directly hold at least one province title. Vassal-held provinces and personal
  titles, armies, or ships do not count. The first fully settled day ending with
  no directly held province ends the campaign; there is no grace period.
- **Assignment cannot begin:** the authoritative reason remains visible; the
  player can change leader or target, recover resources, wait for availability
  or order delivery, or choose another approach.
- **Routine failure:** accepted design calls for automatic retry with time as
  its cost. Consequential work instead resolves into authored setbacks or
  disasters; recovery must come through ordinary assignments and state changes,
  not a reload-only solution.
- **Campaign over while viewing:** the current top bar displays the reason.
  Return-to-title, post-campaign review, export, and deliberate continuation as
  spectator are not evidenced and remain open interface decisions.

## Feedback

The campaign should answer four questions at every stage:

| Player question | Current feedback |
| --- | --- |
| What needs attention? | Prioritised Situation cards, warnings, resolution cards, map attention, and the log |
| What can I do? | Context-sensitive actions, leader and target pickers, and authoritative refusal reasons |
| What will it cost and risk? | Shared-simulation forecasts for costs, delay, duration, odds, effects, recall, and personal risks |
| What changed and why? | Result popups where authored, persistent Situation history, subject-linked log entries, inspectors, ledgers, and map modes |

**Proposal.** A guided objective should explain why it appeared, show how to
reach the relevant normal surface, and disappear without penalty when skipped.
It should never obscure a higher-priority Situation warning. [ai] The
implemented day-one guidance meets this contract: it renders inside the
ordinary Situation card beneath the card's own warning, its "Show me how" and
"Why this matters" help uses the shared focusable pinnable-explanation
surface, and disabling the preference removes only the guidance prose. On succession, a
brief reign transition summary could gather the new head, inherited position,
lost personal claims, active work, and urgent Situations; this is not currently
an accepted or implemented feature.

## Dependencies

- [Characters, organisations, politics, and succession](02-characters-organisations-politics-and-succession.md)
  defines the enduring house, acting head, inheritance, relationships, titles,
  offices, and terminal dynastic failure.
- Assignments, forecasts, plans, and goals define the decision grammar and the
  short-, medium-, and long-horizon rhythm.
- Map, presence, travel, orders, ships, and armies establish the delays and
  spatial constraints the opening must teach.
- Economy, order, obligations, Situations, intrigue, and warfare create the
  pressures, failures, and recovery paths that sustain the campaign.
- AI agency ensures the political field acts during onboarding and thereafter
  through the same rules available to the player.
- [Interface, accessibility, art, audio, and narrative](07-interface-accessibility-art-audio-and-narrative.md)
  owns presentation, input, readable guidance, alert hierarchy, and any future
  tutorial or reign-transition treatment.
- Determinism, snapshot verification, command logging, content validation, and
  native/web parity constrain every campaign-flow feature.

## Acceptance criteria

### Current and accepted experience

1. Starting the client shows the title screen, with no campaign resources
   created before New Game or Continue.
2. New Game starts the authored Ashkarr Succession as House Harrow on the
   authored date, paused, with the day-one political field and applicable
   Situations present.
3. A submitted player action uses the same authoritative eligibility and
   forecast calculations that later apply and resolve it.
4. Pause and each speed setting advance only through complete ordered daily
   ticks; a seeded campaign with the same commands reproduces the same state.
5. Native Continue restores a valid matching autosave exactly. Missing,
   unparseable, state-hash-invalid, or content-mismatched files do not enable
   Continue. The web build does not promise persistence it lacks.
6. Spectator mode begins before the first tick, gives every house autonomous
   agency, exposes no player-authority action, and remains spectator after
   snapshot restore.
7. Death of a house head with a legal heir continues the same player
   organisation under that heir and does not transfer a personal Paramount
   claim.
8. At the end of each settled day, either exhaustion of legal living successors
   or loss of the player's final directly held province produces an inspectable
   Campaign Over reason and later player commands are refused.
9. The campaign may continue after political achievements, including settling
   the Paramountcy; no scripted victory condition ends the MVP.

### Proposed onboarding, if accepted

1. A new player can identify their house, head, liege, holdings, resources, and
   opening Situations before unpausing.
2. The guidance leads to one ordinary forecasted and logged command without
   changing simulation state, odds, costs, AI agency, or result selection.
3. Guidance can be skipped, dismissed, and resumed according to an explicitly
   chosen persistence rule.
4. Every guided step is operable through the eventual accessibility baseline
   and remains correct in both native and browser clients.
5. Automated tests prove objective completion from authoritative state or
   command evidence; manual review proves that wording does not prescribe a
   single Ashkarr strategy.

[ai] The Court Awaits slice satisfies criteria 2 and 3 for its scope — the
guidance preference is presentation-only with an explicit persistence rule,
and any ordinary forecasted, logged command answers the demand — and its
external-behaviour tests prove resolution, the day-seven boundary, and the
10-Influence forfeit from authoritative state and command evidence alone.
Criteria 1, 4, and the manual wording review remain open for the wider arc.

## Evidence

- `pasm/spec/core/game-vision.yaml` — accepted campaign identity, open-ended MVP,
  pauseable-real-time cadence, failure model, succession, persistence, and
  fixed-house opening.
- `pasm/spec/roadmap/milestone-7-the-front-door.yaml` — accepted title, New
  Game, spectator, autosave, and Continue behaviour.
- `pasm/spec/roadmap/mvp-implementation.yaml` — implemented MVP scenario and
  deterministic end-to-end acceptance intent.
- `docs/gdd/overview.md` — proposition, experience pillars, core loop, current
  slice, and scope boundaries.
- `README.md` — current player-facing description and native/web delivery.
- `assets/content/scenario/ashkarr-succession.rhai` — authored date, House
  Harrow, household, resources, holdings, hierarchy, obligations, and crisis.
- `assets/content/core/` and `assets/text/strings.csv` — current actions,
  Situations, goals, plans, results, warnings, explanations, and feedback text.
- `crates/aeon_client/src/title.rs` and `sim_driver.rs` — implemented campaign
  entry, pause/speed, spectator start, autosave, and native Continue.
- `crates/aeon_client/src/ui/` — implemented campaign shell, Situations,
  forecasts, logs, inspectors, and explanatory tooltips.
- `crates/aeon_sim/src/clock.rs`, `politics.rs`, `command.rs`, `snapshot.rs`, and
  `persistence.rs` — authoritative daily rhythm, succession and Campaign Over,
  commands, restore, and persistence.
- `crates/aeon_sim/tests/acceptance.rs`, `politics.rs`, `plans.rs`,
  `determinism.rs`, and `situations.rs` — deterministic continuation,
  succession, spectator agency, and day-one Situation evidence.

## Open questions

- Who is the primary audience, and how much prior grand-strategy literacy may
  onboarding assume?
- What session length and campaign horizon should UI pacing and content density
  target?
- Does the finished game remain purely open-ended, or add optional ambitions,
  scoring, retirement, or chronicle-based end states?
- How should the interface forecast imminent loss of the final directly held
  province before the settled-day terminal check?
- Should a terminal house failure permit post-campaign inspection, spectator
  continuation, export, or an immediate return to title?
- What browser persistence and migration guarantees are required before the
  web build can offer Continue?

## Related GDD sections

- [Game Design Overview](overview.md)
- [02 — Characters, Organisations, Politics, and Succession](02-characters-organisations-politics-and-succession.md)
- [07 — Interface, Accessibility, Art, Audio, and Narrative](07-interface-accessibility-art-audio-and-narrative.md)
