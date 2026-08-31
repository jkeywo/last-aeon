# Last Aeon — Game Design Document

This directory is the federated Game Design Document for *Last Aeon*. Start
with the [Game Design Overview](overview.md), then follow the numbered system
documents for detailed player-facing rules, dependencies, edge cases,
acceptance criteria, and implementation evidence.

## Document map

| Section | Responsibility | Principal connections |
| --- | --- | --- |
| [Overview](overview.md) | Proposition, fantasy, pillars, core loop, scope, and current slice | All sections |
| [01 — Player Experience, Campaign, and Onboarding](01-player-experience-campaign-and-onboarding.md) | Entry, campaign rhythm, learning, failure, and recovery | Politics; presentation |
| [02 — Characters, Organisations, Politics, and Succession](02-characters-organisations-politics-and-succession.md) | People, authority, relationships, titles, offices, claims, and continuity | Assignments; obligations and war |
| [03 — Assignments, Forecasts, Plans, and Goals](03-assignments-forecasts-plans-and-goals.md) | The action vocabulary from individual orders to organisational ambitions | Characters; geography; AI |
| [04 — Map, Presence, Travel, Ships, and Armies](04-map-presence-travel-ships-and-armies.md) | Strategic geography, command latency, forces, and operational movement | Assignments; economy and war; presentation |
| [05 — Economy, Order, Obligations, Situations, Intrigue, and Warfare](05-economy-order-obligations-situations-intrigue-and-warfare.md) | The interlocking consequence and conflict systems | Politics; geography; AI; verification |
| [06 — AI Agency and Information Rules](06-ai-agency-and-information-rules.md) | Autonomous action, shared validation, visibility, explanation, and spectator rules | Assignments; consequence systems; presentation |
| [07 — Interface, Accessibility, Art, Audio, and Narrative Presentation](07-interface-accessibility-art-audio-and-narrative.md) | How authoritative state and action become legible to players | Experience; map; AI information; verification |
| [08 — Content, Balance, Verification, and Acceptance](08-content-balance-verification-and-acceptance.md) | Content contracts, tuning, determinism, persistence, QA, and release evidence | Every system document |

## Authority and status

The documents use three status classes even where their exact label wording
varies slightly:

- **Implemented/current** is evidenced by the repository now. Code and tests
  remain authoritative for exact current behaviour.
- **Accepted design** restates an accepted PASM decision. PASM remains the
  authority if a summary here differs from the model.
- **Proposal/open question** is not a commitment. It requires an explicit
  design decision before implementation.

Setting truth belongs to `the_last_aeons/`; authored playable instances belong
to `assets/content/`; player-facing prose belongs to the string table. A GDD
section explains how these sources become an experience without duplicating
their full data.

## Reading paths

- For the intended experience: **Overview → 01 → 07**.
- For political play: **02 → 03 → 05 → 06**.
- For movement and conflict: **04 → 03 → 05**.
- For implementation and release evidence: **08**, then the Evidence section
  of the relevant system document.

## Reconciliation ledger

The design conflicts found during drafting have been resolved with the project
owner and incorporated into PASM. Three contained gaps were also corrected in
the runtime: settled-day campaign failure, Situation-order en-route feedback,
and pre-selection autosave verification.

The authored-route, concrete-presence, personal-transport, parallel force-command,
leaderless-retreat, and whole-army transport gaps are now implemented. See
[04 — Physical presence](04-map-presence-travel-ships-and-armies.md#physical-presence)
and [04 — Ships and captains](04-map-presence-travel-ships-and-armies.md#ships-and-captains).
Browser persistence remains deliberately outside the current PASM scope rather
than being a PASM/code inconsistency. See [01 — Edge cases and recovery](01-player-experience-campaign-and-onboarding.md#edge-cases-and-recovery).
