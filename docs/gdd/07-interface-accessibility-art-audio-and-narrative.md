# Last Aeon — Interface, Accessibility, Art, Audio, and Narrative Presentation

| Field | Value |
| --- | --- |
| Document status | Draft 0.1 |
| Scope | Player-facing presentation for the current Ashkarr slice |
| Game title | *Last Aeon* |
| Design authority | `pasm/spec/` |
| Implementation evidence | `crates/aeon_client/`, `assets/`, and client tests |

**GDD navigation:** [Index](README.md) · [Overview](overview.md) · [Previous: AI Agency and Information Rules](06-ai-agency-and-information-rules.md) · [Next: Content, Balance, Verification, and Acceptance](08-content-balance-verification-and-acceptance.md)

This document describes how the authoritative simulation becomes legible to a
player or spectator. It does not move rules into the client, establish an art
or audio brief that the project has not accepted, or claim accessibility and
localisation support that the current build does not provide.

Statements use three labels throughout:

- **Implemented** describes behaviour evidenced in the current code or assets.
- **Accepted design** describes a confirmed PASM commitment, whether or not its
  current presentation is final.
- **Proposal** is a GDD recommendation requiring review before it becomes a
  production requirement or PASM decision.

## Purpose and player experience

The presentation layer should let the player answer four questions without
reverse-engineering the simulation:

1. Who am I acting through, and what do I presently control?
2. Where are the relevant people, forces, places, and pressures?
3. What can I do, who can do it, and what will it cost or risk?
4. What changed, why did it change, and what now demands attention?

The intended rhythm is therefore **orient → inspect → compare → commit → watch
time pass → read the consequence**. The interface is a strategic workspace,
not a separate rules engine. Its forecasts, eligibility explanations, map
facts, Situation actions, and command refusals must come from the same
authoritative APIs that resolve play.

Spectator mode changes authority, not observability. A spectator can use the
maps, panels, searches, ledgers, and logs, but is not offered actions that
require a player organisation.

## Current interface architecture

### Front door and loading

**Implemented.** The client preloads the skybox, body textures, province-ID
maps, and starbase model behind a loading spinner before reaching the title
screen. Failed presentation assets count as settled rather than trapping the
player in an endless load; the result may be visually poorer, but the client
continues.

The title screen is the boundary between presentation-only startup and a live
campaign. It offers:

- **New Game**, which creates the authored scenario with a fresh seed;
- a **Spectator mode** toggle, which creates a campaign without a player house;
- **Continue** on native builds only when an autosave exists, parses, passes
  full state-hash verification, and matches the embedded content.

Nothing simulation-shaped is created until the player makes a title-screen
choice. Continue is deliberately absent from the web build because the current
autosave path is native-file based.

### Map-centred campaign shell

**Implemented and accepted design.** One egui root owns layout. The central 3D
viewport remains visible while edge docks claim space around it. The default
layout places the inspector on the left, Situations on the right, and the log
and active assignments along the bottom. Listing, idle household members,
ledger, and—in native development builds—the theme specimen can be summoned as
needed.

Panels are data-driven placements rather than hard-coded regions. A toolbar
control can show, hide, or move a panel between supported edges; its tooltip
states the panel's purpose, present location, and secondary-click behaviour.
Panel layout is presentation state and is not part of the authoritative
campaign snapshot.

The permanent top bar spends screen width only on high-frequency orientation
and navigation:

- player organisation and current head, both inspectable links;
- scenario name, campaign date, and player resources;
- pause/resume and three speed steps;
- current system or body context, return navigation, body projection, and map
  modes where applicable;
- campaign-over state;
- panel toggles and global search.

The player's identity remains visible under other selections so that inspecting
a rival never obscures the organisation through which the player acts.

### Inspection and navigation

**Implemented.** Selection is shared across the map, search, Situation links,
identity links, listings, and inspectors. Selecting a body, province,
character, organisation, army, or ship opens the same inspector surface rather
than creating parallel detail screens. Search finds named campaign subjects and
turns a result into the corresponding selection.

The system view is a stable strategic diagram rather than an animated orrery.
Opening a world or moon reveals its political surface; the player can switch
between a rotatable globe and an equirectangular flat map. Labels, selection,
and click targets use the same direction-based positions in both projections.
The Aurelian Spire uses an authored GLB model rather than a provincial globe.

Map modes are presented as a one-click icon bar. Each mode names the strategic
question it answers, indicates its active state without requiring label
reading, and can expose a colour ledger. The actual political values are
simulation readouts, not client reconstructions. Map presence, movement, ships,
armies, and projection rules are specified in [Map, Presence, Travel, Ships,
and Armies](04-map-presence-travel-ships-and-armies.md).

### Acting and comparing

**Implemented and accepted design.** Actions appear under the selected subject
that is being acted upon or ordered. Composition exposes required target slots,
the leader, immediate costs, order delay, duration, recall point, outcome
distribution, governing skill contest, and personal risk where those facts
apply.

Choosing a leader opens one non-modal comparison workspace. It remains open as
candidates are compared, shows every adult household member, separates those
currently available from those who cannot lead, and explains why and, when
known, until when. Suitability and forecasts are produced by authoritative
simulation code. Disabled actions remain visible with their refusal reason;
absence is reserved for actions that are not relevant to the selected subject.

An issued click becomes a validated `PlayerCommand`; a rejection is feedback,
not a silent no-op. Cancellation likewise reports whether it was accepted,
deferred to the next cancellable phase, or refused because the point of no
return has passed.

## Information hierarchy

The current hierarchy is intentional:

1. **Persistent orientation:** identity, date, resources, time, map context.
2. **Attention:** active and newly resolved Situations in the default right
   dock, plus compact map-attention links.
3. **Object context:** the inspector for the current selection.
4. **Decision support:** action composition, candidate comparison, and
   authoritative forecast.
5. **Ongoing work:** active assignments and idle household members.
6. **Reference and history:** listing, ledgers, global search, and log.

**Proposal.** Preserve this hierarchy as new systems arrive. A new persistent
HUD element should be justified by a fact needed in most decisions; otherwise
it belongs in an existing panel, inspector section, Situation, or ledger. A new
warning should link to the subject or Situation that explains it and should not
duplicate a permanent log entry as an unrelated notification.

## Onboarding surfaces

**Implemented.** Present onboarding is contextual rather than tutorial-led:
tooltips explain map modes, panel controls, costs, delays, duration, forecasts,
risks, and points of no return; empty states say why a surface has nothing to
show; unavailable leaders and actions name their blockers; Situation cards
state the problem, stage, warning, deadline, participants, related subjects,
history, and available responses. The title screen explains spectator mode.

There is no evidenced first-campaign tutorial, guided opening sequence,
codex/manual surface, control-remapping screen, or difficulty-selection flow.
The wider onboarding questions and proposed campaign opening belong in [Player
Experience, Campaign, and Onboarding](01-player-experience-campaign-and-onboarding.md).

**Proposal.** Treat the first-play onboarding design as a separate acceptance
decision. Until it exists, every implemented interaction must remain usable
through local explanation: a labelled control or tooltip, a meaningful empty
state, an inspectable cause, and a recoverable route back to the player's
identity and map context.

## Accessibility

### What exists now

**Implemented.** The interface provides several useful foundations:

- controls pair state with text and tooltips rather than relying exclusively
  on transient animation;
- unavailable choices remain present with reasons;
- semantic theme colours distinguish positive, negative, warning, urgent,
  muted, and selection states against a dark panel ground;
- map modes have distinct primitive-drawn icons, active treatment, labels,
  descriptions, and an optional colour ledger;
- primitive-drawn icons scale with their control and render identically on
  native and web;
- theme tokens centralise colour, typography sizes, spacing, hit sizes,
  shadows, scroll behaviour, panel dimensions, and tooltip width.

These are implementation facts, not proof of compliance with any accessibility
standard. The repository does not currently evidence a contrast audit,
colour-vision-safe palette validation, scalable UI setting, reduced-motion
mode, keyboard-complete navigation, remappable controls, controller support,
screen-reader semantics, caption settings, or an accessibility test plan.
The build ships one interface face; font family is deliberately not a theme
token. No minimum resolution or maximum text expansion has been accepted.

### Interface scale and density

**Implemented and accepted design.** Interface scale is a client preference
with four supported values: 100%, 125%, 150%, and 200%. Information density is
a separate preference with Compact and Comfortable values. Compact preserves
the original authored spacing and is the default; Comfortable increases gaps,
row heights, and control targets without removing information. Scale changes
the whole egui interface rather than changing only body text.

The same controls appear at the title screen and within the running campaign.
Preferences persist in a versioned, fail-soft document: a native file for the
desktop build under the operating system's per-user application configuration
directory, and the stable `last-aeon.ui.preferences` local-storage entry for
the web build. Missing, corrupt, inaccessible, or unsupported future documents
restore the 100%/Compact defaults. Shared adapter-contract tests cover browser
storage without claiming a browser-runtime test harness the repository does not
have; the Trunk build provides wasm compile coverage. These values are
presentation state only; an isolation test proves changing and persisting them
does not alter campaign snapshots, command logs, or state hashes.

### Required decision work

**Proposal.** Before an accessibility target is promised, decide and record:

- supported input methods and whether every campaign action must be reachable
  without a pointer;
- minimum contrast and non-colour redundancy requirements for UI and maps;
- minimum viewport, wrapping, responsive reflow, and dock overflow behaviour at
  the accepted interface scales;
- motion, flashing, camera, and selection-pulse limits;
- assistive-technology expectations for native and browser builds;
- caption/subtitle requirements if sound is introduced;
- the test matrix and which checks are automated, manual, or user-tested.

Until those decisions are accepted, new presentation should at least preserve
the existing pattern of text explanation alongside semantic colour and should
not encode a critical fact by colour or hover alone.

## Localisation and text

**Accepted design and implemented.** Every player-facing string belongs in the
single CSV table at `assets/text/strings.csv`; Rust and Rhai refer to stable
keys. Authored definitions derive their display keys from their IDs. Missing
keys, bad placeholders, and unused rows are test failures. Text is embedded
with content so native and web use the same table.

Square brackets are an editorial status marker: English prose not yet approved
by a human is bracketed in the data and remains visibly bracketed everywhere it
appears. Brackets are not fictional voice, emphasis, or a localisation marker.

The infrastructure is localisation-ready only in the narrow sense of
centralised, keyed text with named interpolation and plural-row support. There
is currently one language. Simulation log entries are resolved to plain text
when written, so a save records the language in which its history was created.
There is no evidenced language selector, runtime language switch, fallback
policy, translator notes workflow beyond the CSV context field, right-to-left
layout, font fallback, grammatical gender/case system, or multi-language QA.

**Proposal.** A second language should trigger an explicit localisation design
pass covering save-language behaviour, text expansion, fonts and shaping,
sorting/search, date and number formats, and whether old log history changes
language on load. Do not infer those rules from the current English-only table.

## Visual and art direction

### Evidenced current direction

**Implemented.** The present visual language is a dark, compact strategic
interface over a near-black space viewport. Near-opaque charcoal panels are
designed not to compete with the coloured political map. A restrained warm
accent, muted secondary text, semantic status colours, shallow rounding,
subtle borders, and controlled shadows establish hierarchy. The interface's
appearance is authored in `theme.ron`; native development builds can reload it
on save and expose a specimen panel that demonstrates tokens in context.

The map combines:

- a space cubemap;
- authored Ashkarr ecumenopolis and Vesk volcanic surface textures;
- equirectangular province-ID textures and manifests;
- dynamically baked political colours, neutral unheld territory, and dark
  province borders;
- globe and flat projections from the same texture and positional data;
- a pulsing selected-province shader;
- a GLB model for the Aurelian Spire;
- painter-drawn UI and map-mode icons rather than an image icon atlas.

Code comments explicitly describe parts of the planetary presentation as
programmer art. The available assets demonstrate a science-fiction political
map and functional environmental distinction; they do not establish a final
concept-art style, character portrait style, animation language, VFX bible,
cinematic standard, or production asset budget. The existing skybox filename
is legacy asset naming, not the game title.

### Art boundaries

**Accepted design.** UI icons remain resolution-independent shapes drawn from
primitives and tinted by the theme unless that decision is revisited in PASM.
The same presentation assets and political data serve native and web. Visuals
must not become authoritative sources of political truth: colour, labels,
selection, and force markers render simulation readouts.

**Proposal.** A future art brief should decide the intended level of
ornamentation, factional visual identity, portraiture, animation, environmental
specificity, asset performance budgets, and how the ancient/cosmic setting is
distinguished from the grounded Ashkarr political slice. This GDD does not
select those answers.

## Audio

**Current status.** No music, sound-effect, voice, audio middleware, mixer,
volume settings, or audio assets are evidenced in the client or asset tree.
The headless simulation deliberately excludes audio along with rendering and
windowing; any future audio remains presentation-only and must not affect
authoritative state, command ordering, or deterministic outcomes.

No musical style, voice strategy, diegetic sound language, dynamic score,
alert vocabulary, loudness target, or accessibility commitment is currently
accepted. “An alarm should sound” in design prose is figurative and is not an
audio requirement.

**Proposal.** If audio enters scope, first define its jobs—atmosphere, time and
command feedback, attention signalling, consequence punctuation, or narrative
voice—and guarantee that every gameplay-critical cue also has persistent
visual/text feedback. Platform parity, browser autoplay restrictions, mixer
categories, mute defaults, captions, and saved volume settings require explicit
acceptance criteria before implementation.

## Narrative presentation

Narrative is primarily systemic and documentary rather than cinematic.

### Situations

**Implemented and accepted design.** Situations are the main continuing-story
surface. Authored definitions provide title, summary, stages, optional warnings
and deadlines, metrics, participant groups, semantic links, actions, and
resolution text. The client displays these fixed projection blocks without
deriving the underlying rules. Resolutions persist until dismissed through a
logged player command. Cards can expose a short, occurrence-specific history
drawn from structurally tagged log entries.

Situation actions launch existing assignment flows with relevant subject and
occurrence context preselected. They do not bypass validation, forecasts, order
delay, leadership, or result machinery. Runtime Situation errors become an
unavailable card with suppressed actions and deterministic log-once diagnosis,
rather than silently removing the story problem.

### Log, results, and popups

**Implemented.** The campaign log is durable chronological context, not only a
toast feed. It records assignment outcomes and cancellation, AI reasons,
standing orders, plans and goals, political and economic changes, unrest,
warfare, Situation activation/diagnostics, and other notable results. The log
can be filtered and is capped in the client presentation while authoritative
history remains tied to campaign state. Result popups carry authored choice
text where an outcome requires the player to respond.

AI presentation is deliberately bounded: visible actions can include an
authored reason, while hidden plans are not exposed merely because the client
could inspect state. Information rules and spectator exceptions are specified
in [AI Agency and Information Rules](06-ai-agency-and-information-rules.md).

### Narrative voice

The string table presently contains concise strategic labels, explanatory
tooltips, authored Situation prose, and chronicle-like log lines. Much remains
bracketed as unapproved prose. This is evidence of an editorial pipeline, not a
settled voice guide. World canon comes from `the_last_aeons/`; the client and
content table should present only canon selected for play and should not use UI
copy to invent new setting truth.

**Proposal.** A later narrative style guide should define register, naming,
capitalisation, point of view, spoiler boundaries, how uncertainty is voiced,
and the division between Situation copy, inspector facts, tooltips, popups, and
chronicle entries. Approval should be represented by removing editorial
brackets from reviewed rows, never by changing code paths.

## Native and web constraints

**Implemented.** Both targets run the same Rust client, embedded content, egui
interface, and authoritative simulation. Runtime presentation assets are
served alongside the web build and preloaded. Primitive icons, shared map
textures, and the starbase model avoid target-specific visual semantics.

The current deliberate differences are:

- Continue and native filesystem autosave are native-only;
- native development builds support hot-reloading `theme.ron` and a specimen
  panel that is not part of the web panel list;
- the browser supplies an HTML loading overlay before the Bevy canvas exists,
  followed by the in-client asset-loading screen.

**Proposal.** Any new platform-specific feature must state whether the
difference is a development convenience, a delivery constraint, or a player
experience difference. A feature essential to understanding or commanding the
campaign cannot exist on only one shipping target without an accepted scope
decision.

## Rules, data, and edge cases

- The client never decides eligibility, outcome odds, map politics, Situation
  stages, or AI intent. It requests and renders authoritative answers.
- A missing player organisation produces an observation-only interface; it
  must not fabricate an acting identity or show command affordances.
- Missing, unreadable, unparseable, state-hash-invalid, or content-mismatched
  autosaves disable Continue before selection.
- Presentation asset failure must settle loading and degrade gracefully; it
  must not change simulation content or state.
- An empty panel explains its empty state. An unavailable action or leader
  remains inspectable with its reason where the subject is relevant.
- A projected action that later fails revalidation reports the simulation's
  refusal; a click must never appear to vanish.
- Selection links exist only for subject kinds the client can inspect. Other
  semantic links retain textual identity without pretending to navigate.
- Situation histories match exact occurrence tags, so repeated wars or
  recurring Situations do not borrow one another's log entries.
- A malformed authored text key, interpolation contract, or unused row is a
  validation failure, not a blank runtime label.
- Panel placement, projection, selection, open search, and other view state are
  non-authoritative and need not reproduce a campaign outcome.
- Campaign-over state remains visible and commands are refused by the
  simulation.

## Feedback contract

Every meaningful player interaction should produce at least one of these
forms of feedback:

- visible state change in selection, control state, panel placement, or map;
- an authoritative forecast before commitment;
- a disabled-state or refusal explanation;
- a queued, active, phased, completed, cancelled, or failed assignment state;
- a persistent log entry, Situation lifecycle change, or authored result
  popup for consequential outcomes.

Feedback may use motion and semantic colour for emphasis, but critical meaning
must remain available as text or persistent state. The log and Situation
history are the recovery path when a transient moment is missed.

## Dependencies

- `aeon_sim` supplies campaign state, selections' facts, leader availability,
  forecasts, command validation, logs, Situations, information visibility, and
  campaign-over state.
- `aeon_data` supplies validated authored definitions and display-key
  contracts.
- `aeon_core` supplies stable identities, dates, deterministic campaign
  foundations, and snapshot compatibility.
- `assets/content/` supplies authored assignments, events, goals, plans,
  Situations, and the Ashkarr scenario.
- `assets/text/strings.csv` supplies every player-facing string.
- `crates/aeon_client/assets/theme.ron` supplies interface design tokens.
- map textures, the skybox, shader, and starbase GLB supply current scene art.
- `the_last_aeons/` remains the setting authority.
- Content validity, determinism, platform builds, and release gates are covered
  in [Content, Balance, Verification, and Acceptance](08-content-balance-verification-and-acceptance.md).

## Acceptance criteria

The current presentation contract is accepted when:

1. Native and web start behind honest loading feedback and reach the title
   without creating a campaign prematurely.
2. New Game and spectator start create the intended authority state. Native
   Continue is enabled only for a fully verified compatible autosave and
   restores that campaign exactly.
3. Identity, date, time control, map context, resources where applicable, and
   campaign-over state remain legible during play.
4. Default docks expose inspection, Situations, log, and active assignments;
   panel movement never duplicates or loses a panel.
5. Map modes, globe/flat projection, labels, selection, and click resolution
   agree on what place and political fact are shown.
6. Action and Situation surfaces use authoritative availability, validation,
   and forecasts, including specific disabled or refusal reasons.
7. A spectator can inspect the campaign but cannot issue player-authority
   commands.
8. Consequential outcomes remain recoverable through log, Situation history,
   resolution cards, assignment state, or result popup as appropriate.
9. Every displayed string is table-backed; key, placeholder, orphan-row, and
   bracket-status validation continues to pass.
10. Theme tokens parse and apply, target states remain distinguishable, and
    native hot reload fails safely without destroying the running interface.
11. Presentation-only state cannot alter a headless result, replay, snapshot
    hash, or command ordering.
12. No release claim is made for accessibility, localisation, art, or audio
    capabilities that lack an accepted requirement and matching verification.

## Evidence and verification

- `pasm/spec/roadmap/milestone-3-interface-and-quality-of-life.yaml` records
  accepted interface, picker, map-mode, string-table, theme, and projection
  goals plus test and manual-review evidence.
- `pasm/spec/roadmap/milestone-7-the-front-door.yaml` records title, new-game,
  spectator, autosave, and native-only Continue decisions.
- `pasm/spec/architecture/implementation-decisions.yaml` records the owned egui
  token layer, painter icons, non-modal character picker, display-string table,
  editorial brackets, and Situation presentation decisions.
- `crates/aeon_client/src/ui/` implements the shell, docks, panels, inspector,
  action flows, forecasts, Situations, search, log, icons, widgets, and theme.
- `crates/aeon_client/src/title.rs`, `loading.rs`, `scene.rs`, `view.rs`,
  `selection.rs`, `map_modes.rs`, and `map_overlay.rs` implement the front door
  and strategic scene.
- `crates/aeon_client/src/ui/theme.rs`, `view.rs`, `dock.rs`, and
  `situations_panel.rs` contain unit tests for key presentation invariants.
- `crates/aeon_client/tests/strings.rs` checks UI/simulation string use,
  placeholders, and orphan rows against `assets/text/strings.csv`.
- `crates/aeon_client/index.html` and `Trunk.toml` evidence browser loading and
  delivery; `README.md` documents the shared native/web client.
- `assets/text/strings.csv`, `crates/aeon_client/assets/theme.ron`, map textures,
  shader, cubemap, model, and their attribution files evidence current content
  and visual presentation.

## Open questions

- Which accessibility standard and target platforms define release acceptance?
- Which pointer, keyboard, controller, and touch interactions must be complete?
- What resolutions, aspect ratios, UI scales, and text expansion must docks and
  overlays support?
- Should panel layout and presentation preferences persist between sessions,
  independently of campaign saves?
- Does the web release need a browser-backed Continue path, and if so what are
  its compatibility and deletion rules?
- What is the final visual brief beyond the current functional dark strategic
  map, and which present assets are placeholders?
- Are character portraits, faction heraldry, animation, or cinematics in the
  intended product scope?
- What jobs, if any, should music, sound effects, voice, or audio alerts perform?
- What is the approved narrative voice, and who approves bracketed prose?
- Which information should remain unknown to the player, and how should the UI
  distinguish uncertainty from missing content or error?
- When a second language is added, should existing saved log text retain its
  original language or be stored as resolvable semantic events?

## Related GDD sections

- [Player Experience, Campaign, and Onboarding](01-player-experience-campaign-and-onboarding.md)
- [Map, Presence, Travel, Ships, and Armies](04-map-presence-travel-ships-and-armies.md)
- [AI Agency and Information Rules](06-ai-agency-and-information-rules.md)
- [Content, Balance, Verification, and Acceptance](08-content-balance-verification-and-acceptance.md)
