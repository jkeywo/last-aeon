# Last Aeon — Content, Balance, Verification, and Acceptance

| Field | Value |
| --- | --- |
| Document status | Draft 0.1 |
| Scope | Authored content, tuning ownership, deterministic verification, saves, replay, and release evidence |
| Primary design authority | `pasm/spec/` |
| Current implementation | `crates/aeon_core/`, `crates/aeon_data/`, `crates/aeon_sim/`, `crates/aeon_tools/`, `assets/`, and `.github/workflows/ci.yml` |

**GDD navigation:** [Index](README.md) · [Overview](overview.md) · [Previous: Interface, Accessibility, Art, Audio, and Narrative](07-interface-accessibility-art-audio-and-narrative.md)

This section defines how *Last Aeon* turns authored material into trustworthy
gameplay. It describes the present content contract and the evidence required
to accept changes; it does not replace PASM, the Rust data model, or executable
tests. PASM remains the authority on accepted design, while the current code
and validated assets remain the authority on implemented behaviour.

## Status language

- **Implemented** describes behaviour present in the current code and supported
  by direct implementation or test evidence.
- **Accepted design** describes a confirmed PASM decision. An accepted design
  is not assumed implemented unless this document says so.
- **Proposal** and **open question** describe possible future practice. They do
  not alter PASM or create a release commitment.

## Purpose and player promise

Content should be expressive enough to create distinct people, places,
political pressures, assignments, plans, goals, events, and Situations without
creating a second simulation hidden in scripts. Balance changes should be
reviewable as deliberate changes to player choices, not opaque changes caused
by file ordering, platform state, or unrelated random rolls.

The player-facing promise is therefore:

1. the rules shown in forecasts are the rules used at resolution;
2. the same seed, content, and ordered commands reproduce the same campaign;
3. malformed or internally inconsistent content is rejected before release;
4. a save never silently resumes against different authored rules;
5. important tuning changes can be explained in terms of their effect on
   decisions, pacing, scarcity, risk, and campaign outcomes.

## Content model and ownership

### Sources of truth

**Implemented / accepted design.** Content and rules are divided by authority,
not merely by file type.

| Concern | Current owner | Contract |
| --- | --- | --- |
| Structural game design and accepted decisions | `pasm/spec/` | Records system responsibilities, relationships, and confirmed choices |
| Valid content shapes and boundary vocabularies | `crates/aeon_data/src/model.rs`, `key.rs`, and `host/` | Converts authored values into ordered, typed Rust definitions and reports invalid input |
| Authoritative gameplay mutation and invariants | `crates/aeon_sim/` | Validates commands and applies effects to campaign state |
| Reusable gameplay catalogue | `assets/content/core/` | Assignments, events, plans, goals, Situations, economy, intrigue, and warfare definitions |
| Geography | `assets/content/system/` | Bodies and provinces |
| Starting political state | `assets/content/scenario/ashkarr-succession.rhai` | Scenario, organisations, characters, titles, offices, forces, and opening obligations |
| Player-facing prose | `assets/text/strings.csv` | One keyed table for content, interface, and simulation text |
| Shipped content set | `crates/aeon_client/build.rs` and embedded assets | Native and web builds carry the content validated for that build |

Rhai uses declaration-by-call: a restricted load-time engine receives a fixed
set of `define_*` builders and produces typed definitions. Runtime function
references are file-local, keeping behaviour self-contained and preventing an
authored file from accidentally depending on a function in another file.

### Current schemas

**Implemented.** `ContentSet` stores each definition category in a `BTreeMap`,
so traversal is stable rather than hash-order-dependent. Its current catalogue
contains:

- assignments and their phases, requirements, outcome weights, risks, costs,
  popup choices, logs, and effect functions;
- celestial bodies, provinces, goods, and buildings;
- name pools, traits, characters, organisations, titles, and offices;
- ships, starting armies, and opening obligations;
- contextual events, character plans, organisational goals, and directives;
- reusable Situation definitions, stages, actions, outcomes, visibility, and
  typed subject bindings;
- one optional scenario definition selecting the start date, player house, and
  enabled Situations.

The schema deliberately exposes game concepts rather than Bevy components.
For example, Situation subjects name characters, organisations, titles,
provinces, forces, assignments, obligations, and wars. Content can therefore
survive internal ECS reorganisation without receiving raw reflection access.

### Stable keys and durable identity

**Implemented / accepted design.** Authored definitions and scenario entities
use lowercase kebab-case `ContentKey` values. Display text uses dot-separated
kebab-case `TextKey` values. Definition prose keys are derived from the
definition key and field, so an ID and its text cannot silently point at
unrelated rows.

Runtime campaign entities use typed, positive, never-reused stable IDs from a
single snapshotted allocator. Saves, commands, UI selection, scripts, and
cross-system links use those IDs; transient Bevy `Entity` handles do not cross
the durable boundary. Ordered keys, stable IDs, and explicitly sorted views are
part of the replay contract, not implementation trivia.

Changing or removing an authored key may invalidate cross-references and can
affect save compatibility. Renaming should therefore be treated as an identity
change unless an explicit, tested migration policy says otherwise.

## Script context and typed effects

**Implemented / accepted design.** Rhai is non-authoritative. Runtime functions
receive an approved, read-only context and return plain values. Rust parses the
result into the type required by that call site before any mutation occurs.

Assignment result, popup-choice, event, and event-choice effect calls receive a
common context containing the source key, result or selected option, leader
display name, and target display label. The semantic world view additionally
provides deterministic, sorted domain records to Situation and other authored
queries. Scripts never receive direct mutable access to ECS state.

The present typed effect vocabulary includes:

- notable log entries and directional opinion modifiers;
- army formation and building construction or wrecking;
- claims, formal-war declaration, adoption, and conclusion;
- Imperial tithe collection;
- creation, fulfilment, or breaking of obligations;
- provincial-order changes; and
- personal conditions such as injury, capture, scandal, incapacity, or death.

Each effect uses a closed vocabulary for roles, scopes, actions, and tags.
Unknown kinds, missing fields, mistyped fields, or unknown roles fail at the
boundary. Rust then applies the effect through authoritative rules; a parsed
effect is a request, not permission to violate simulation invariants.

**Accepted design.** The sandbox is deny-by-default: it excludes nondeterminism,
imports, dynamic evaluation, wall-clock access, unrestricted output, and
unbounded execution. Integer-only authored arithmetic and hard operation limits
keep native, web, tests, and replay on the same footing.

## Validation pipeline

**Implemented.** Validation is cumulative where practical, so authors receive a
report rather than fixing one broken reference per run.

1. **Discover and order sources.** Content files are read under stable relative
   paths; duplicate paths are errors. The content hash covers the source set.
2. **Compile and run declarations.** Every Rhai file compiles and executes once
   in the restricted load-time engine. Parse, runtime, sandbox, and operation
   limit failures are reported against their source path.
3. **Build typed definitions.** Builder functions validate keys, required
   fields, numeric ranges, enum spellings, target types, stages, costs, dates,
   and definition-local invariants. Duplicate IDs are rejected. Unknown fields
   currently warn rather than fail, preserving forward diagnosis without
   silently treating them as supported behaviour.
4. **Validate cross-references.** The post-pass checks referenced definitions,
   local function names, mandatory assignment outcomes, body parentage, goods,
   political membership and hierarchy, title and office holders, force owners
   and locations, plan targets and cycles, goal directives, and Situation
   actions, audiences, functions, attachments, and fallbacks.
5. **Resolve display text.** `assets/text/strings.csv` supplies all
   player-facing strings. Missing or malformed rows are findings. Bracketed
   English remains the accepted marker for prose awaiting human approval, and
   validation reports its count.
6. **Check repeatability.** `aeon validate-content` loads the same sources a
   second time and requires structurally equal data and the same content hash.
7. **Exercise the opening.** The tool starts the authored scenario and validates
   opening Situation evaluation, catching deterministic runtime failures that
   structural loading alone cannot reach.

Content validation proves that a content set is well-formed and can start. It
does not by itself prove that the content is enjoyable, fairly tuned,
comprehensible, or reachable through ordinary play.

## Balance and tunables

### Present ownership

**Implemented.** Many values intended for content iteration are authored in
Rhai: assignment duration, difficulty, resource costs, outcome weights and
risks; event weight and cooldown; plan bonus, horizon, retries, and cooldown;
goal priority, favoured pressures, bonuses, horizon, and cooldown; Situation
priority; opening resources and obligations; and provincial or building
production and consumption.

Invariant mechanics and some global tuning remain in Rust. Current examples
include agency shortlist and threshold values, event occurrence chances,
political age and contest durations, plan adoption threshold, order bounds and
recovery values, trade capacity, tithe divisor, directive bonus, and selected
warfare modifiers. This is the implemented division, not a commitment that all
such values must remain compiled constants.

**Accepted design.** Authoring values as data must not transfer authority to
scripts. Forecast and resolution calculations share authoritative code, and
random decision functions are separated from state application. This makes it
possible to test the distribution promised to the player against the sampler
that actually resolves it.

### Balance-change discipline

**Proposal.** A balance change should record, in its change description or a
linked design note:

- the player decision or campaign pressure it is intended to change;
- the exact authored values or authoritative constants changed;
- representative seeds, starting conditions, and observation horizon;
- before-and-after evidence using state facts, forecasts, or scenario outcomes;
- known effects on AI behaviour, economy, warfare, and pacing; and
- whether the change deliberately changes replay results or save compatibility.

No formal target ranges for campaign length, resource scarcity, action success,
AI goal attainment, revolt frequency, war duration, or house survival are
currently evidenced in the repository. There is also no evidenced telemetry,
player analytics pipeline, or performance benchmark suite. Headless runs and
state hashes provide a foundation for batch analysis, but a multi-seed balance
corpus remains a proposal rather than an implemented gate.

## Determinism and random streams

**Implemented / accepted design.** The campaign seed, authored content, and
ordered player commands determine campaign state. Canonical hashing uses the
same digest definition across native, web, and CI; deterministic collections
and serialisation exclude wall-clock or platform-dependent state.

Randomness uses independently derived PCG32 streams. Each use site derives a
stream from:

`campaign seed + frozen purpose label + stable subject identities`

This prevents one unrelated call from consuming a shared stream and shifting
later outcomes. A purpose label is an identity, not descriptive prose. Changing
it rerolls every outcome produced by that stream, so labels remain frozen even
when the surrounding game concept is renamed. New use sites require a new
deliberate label and stable, sufficiently specific subjects; existing labels
must not be cleaned up cosmetically.

Pure decision functions consume a fixed number of rolls where implemented.
Forecast sampling, assignment resolution, engagements, and autonomous choice
have direct tests, including a statistical check that sampled assignment
outcomes match the displayed odds within tolerance.

## Saves, command logs, replay, and migration

**Implemented.** A campaign snapshot is a complete authoritative state capture
with an explicit format version and canonical state hash. The current format is
version 20. Snapshots include the campaign seed and dates, content hash, stable
ID allocator, map and political state, assignments, forces, obligations,
events, plans, goals, directives, claims, wars, Situations, pending commands,
applied commands, and next command sequence.

Native snapshots are pretty-printed RON. Command logs are append-only JSON
Lines with one envelope per line. Each command carries its scheduled day and a
global monotonic sequence number; pending and replayed commands apply in strict
`(day, sequence)` order. These human-readable formats are a current development
choice. The web path currently uses in-memory serialisation rather than an
implemented browser-storage backend.

Every content-backed snapshot records its content hash and restores only with
an identical content set. Content-free snapshots refuse supplied content, and
content-backed snapshots refuse missing or mismatched content. Snapshot restore
also rejects a corrupt state hash, stale or reordered replay envelopes, and an
unsupported format version.

**Accepted design.** Versioned snapshots plus an append-only command log are the
campaign persistence model, with autosaves at meaningful decisions and calendar
milestones. The present repository proves snapshot, log, and replay mechanics;
this document does not claim that every planned autosave or platform storage
surface is complete.

**Current migration policy.** No release has shipped. Pre-release incompatible
changes have bumped the snapshot version and refused older formats rather than
inventing unsafe state. The fleet RNG and hash adoption deliberately broke old
pre-release saves, and later Situation and formal-war identity changes did the
same. A future shipped release will need explicit migration support or a clearly
communicated compatibility boundary before its save format can be considered a
public promise.

## Verification and QA

### Automated evidence

**Implemented.** The repository provides several complementary levels of proof:

| Evidence | What it demonstrates |
| --- | --- |
| `crates/aeon_data/tests/loading.rs` | Valid loading, stable input ordering, sandbox refusals, schema and cross-reference failures, typed effect parsing, and repository-content validity |
| Focused `crates/aeon_sim/tests/` suites | Rules and edge cases for assignments, events, plans, goals, politics, logistics, economy, order, Situations, intrigue, warfare, and formal wars |
| `crates/aeon_sim/tests/determinism.rs` | Identical-run hashes, seed divergence, command ordering, snapshot equality, continuation, replay, tamper rejection, version rejection, and pulse boundaries |
| `crates/aeon_sim/tests/content_binding.rs` | Exact content-hash restoration and deterministic stable-ID reconstruction |
| `crates/aeon_sim/tests/acceptance.rs` | Scripted scenario play, midpoint snapshot replay, connected Situations and wars, autonomous plans and goals, and trade behaviour |
| `aeon validate-content` | Full repository content load, repeatability, content hash, and opening Situation evaluation |
| `aeon run`, `replay`, and `hash` | Seeded headless execution, log replay, and optional expected-hash verification |
| `aeon accept` | Authored scenario run, midpoint snapshot, restore, continuation, and exact final-hash equality |

CI delegates to the pinned fleet workflow. Its declared gates include PASM
validation and scan, formatting, Clippy with warnings denied, workspace tests,
content validation, replay acceptance, and the Trunk web build; the playable web
build deploys from main. Passing one test layer does not waive the others.

### Manual QA

**Proposal.** Automated equality cannot judge clarity or dramatic quality. A
candidate content slice should also receive a short human pass that checks:

- every intended action is discoverable from its ordinary context;
- eligibility failures, costs, delays, odds, risks, and points of no return are
  understandable before commitment;
- outcome text matches the state change and does not disclose hidden facts;
- Situation stages, warnings, conclusions, and links remain useful throughout
  the occurrence;
- prolonged play produces meaningful alternatives rather than one dominant or
  mandatory loop;
- succession, resource exhaustion, defeat, and missing-target states remain
  recoverable or end cleanly; and
- native and browser presentation expose the same authoritative facts.

There is no evidenced formal manual-QA checklist or sign-off role today. The
list above is proposed acceptance practice, not a claim about the current
release process.

## Acceptance criteria for a content change

A change is ready for acceptance when all applicable statements in criteria
1–10 are supported by evidence. Criterion 11 is proposed balance practice and
does not become a mandatory gate without an accepted design decision:

1. **Design authority:** structural changes update PASM and record any new
   accepted choice; changes do not silently contradict an existing unmarked
   human decision.
2. **Schema:** every authored definition builds into the typed model, uses valid
   stable keys, and has complete references, mandatory outcomes, and text rows.
3. **Authority:** scripts consume only approved context and return values that
   parse into the call site's closed vocabulary; authoritative Rust validation
   remains the mutation boundary.
4. **Determinism:** repeated content loads agree, identical campaign inputs
   produce identical hashes, and no unordered or platform-dependent source has
   entered authoritative state.
5. **RNG identity:** no frozen purpose label changed accidentally; any new label
   and its subjects identify the intended occurrence without coupling unrelated
   outcomes.
6. **Persistence:** snapshot capture and restore include every new authoritative
   section symmetrically; content binding, state hashing, and command replay
   still agree.
7. **Compatibility:** an incompatible snapshot, ID, hash, or random-stream
   change is explicit and uses the accepted migration or version-refusal path.
8. **Rules:** focused tests cover success, rejection, boundary values, and the
   cross-system consequences introduced by the change.
9. **Scenario:** repository content validates and the end-to-end acceptance
   round-trip reproduces the exact final hash.
10. **Delivery:** formatting, Clippy, workspace tests, PASM gates, and the web
    build pass in CI.
11. **Proposed player meaning:** for balance or presentation changes, the intended player
    decision and observed before-and-after effect are documented; where no
    automated measure exists, the limitation is stated rather than guessed.

## Edge cases and failure policy

- Duplicate source paths or content keys are errors; deterministic ordering
  must not decide which duplicate wins.
- A missing definition, bad political or geographic relationship, absent local
  function, plan cycle, incompatible target, or malformed Situation fallback is
  rejected at load.
- A guaranteed assignment may define only success; a non-guaranteed assignment
  must define success and failure.
- A runtime script value that does not match its typed boundary is rejected and
  diagnosed, not coerced into a partial effect.
- Empty or malformed command-log lines are handled deliberately: blank lines
  are ignored, while malformed entries report their line number.
- Snapshot parsing alone is not trust: version, state hash, and content binding
  must all pass before restore.
- Adding a persisted system requires capture and both applicable restoration
  halves to remain symmetric. The replay test is the backstop, not a substitute
  for making that ownership explicit.
- Content edits that are structurally valid may still strand a plan, make an
  action unreachable, collapse an economy, or create a dominant strategy. Those
  are balance and playability failures requiring scenario evidence.
- Bracketed prose is deliberately visible unfinished text. It may validate, but
  should not be mistaken for human-approved release copy.
- A future platform-specific storage failure must not be represented as a
  successful save. Browser persistence remains an unresolved delivery concern.

## Dependencies

This contract depends on:

- PASM remaining current with structural changes and implementation mappings;
- `aeon_core` keeping stable identity, calendar, RNG, and hashing semantics
  deterministic;
- `aeon_data` owning the complete authored schema, sandbox, string table, and
  typed return boundaries;
- `aeon_sim` keeping rules headless, authoritative, canonically captured, and
  replayable;
- `aeon_tools` exposing validation and acceptance without requiring the client;
- the client embedding the same validated assets for native and web; and
- CI continuing to run the full validation, test, acceptance, and delivery
  gates against pinned shared foundations.

## Open questions

These questions do not change current or accepted behaviour until answered and
recorded:

- Which quantitative ranges define a healthy Ashkarr campaign for scarcity,
  assignment outcomes, revolt, war length, succession, and house survival?
- Which representative seeds and scripted decisions should form a future
  multi-seed balance corpus, and which results should be invariants versus
  monitored trends?
- Which currently compiled tuning constants should become authored data, and
  which are foundational rules that should remain code-owned?
- What evidence and human role are required to remove brackets from prose and
  approve a complete scenario for release?
- What save-compatibility period begins with the first public release, and how
  are migrations tested across every supported version?
- Should shipped builds retain human-readable saves and logs, or introduce a
  binary format while keeping diagnostic export tools?
- What browser storage, backup, corruption recovery, and save-export experience
  completes parity with native persistence?
- What performance budgets matter for content load, a daily tick, long
  headless runs, snapshot size, restore time, and web delivery, and how should
  CI measure them without making unstable machines the authority?
- Is opt-in telemetry appropriate for balancing, and if so, what minimal,
  privacy-preserving facts answer design questions that deterministic local
  runs cannot?
- When an accepted balance change deliberately changes hashes, what durable
  baseline record should explain the new result without turning a single golden
  hash into a substitute for behavioural tests?

## Evidence map

### Design and decisions

- `pasm/spec/core/game-vision.yaml` — deterministic campaigns, persistence,
  stable identity, validated content, Rhai, and the typed script boundary.
- `pasm/spec/architecture/implementation-decisions.yaml` — content keys,
  sandbox policy, returned typed values, content-hash binding, embedded content,
  CLI acceptance, strings, frozen RNG labels, and pre-release save breaks.

### Content and validation

- `crates/aeon_data/src/model.rs`, `key.rs`, `effect.rs`, and `host/` — current
  schemas, key rules, effects, loading, and cross-reference validation.
- `crates/aeon_data/tests/loading.rs` — executable content and sandbox contract.
- `assets/content/` and `assets/text/strings.csv` — the current authored game
  and its player-facing language.

### Determinism, persistence, and acceptance

- `crates/aeon_core/src/id.rs`, `rng.rs`, `fixed.rs`, and `hash.rs` — shared
  deterministic primitives.
- `crates/aeon_sim/src/command.rs`, `snapshot.rs`, and `persistence.rs` — ordered
  commands, canonical campaign capture, content binding, and file formats.
- `crates/aeon_sim/tests/determinism.rs`, `content_binding.rs`, and
  `acceptance.rs` — replay and scenario-level executable evidence.
- `crates/aeon_tools/src/main.rs` — content validation and headless
  run/replay/hash/accept commands.
- `.github/workflows/ci.yml` — pinned CI entry point and extra content and replay
  gates.

## Related GDD sections

- [GDD overview](overview.md)
- [Assignments, Forecasts, Plans, and Goals](03-assignments-forecasts-plans-and-goals.md)
- [Economy, Order, Obligations, Situations, Intrigue, and Warfare](05-economy-order-obligations-situations-intrigue-and-warfare.md)
- [Interface, Accessibility, Art, Audio, and Narrative](07-interface-accessibility-art-audio-and-narrative.md)
