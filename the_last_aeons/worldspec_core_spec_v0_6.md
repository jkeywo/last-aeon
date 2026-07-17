# WorldSpec Core Specification v0.6

**Specification ID:** `worldspec-core/0.6`
**Status:** Draft normative specification (mirror of WorldSpec.md onto the shared conventions)
**Extension mechanism:** the shared `spec-extension/0.6` contract
**Role:** the **leaf setting base** — consumed as a `dependency` by `gamespec-core` games and `storyspec-core` prose; it depends on no other base.

## 1. Purpose

WorldSpec is a structured representation of a **fictional world** — its entities, canon, relationships and chronology — that an LLM and a human can ingest, query for contradictions, extend, and compile into a world bible. In the v0.6 family it is the shared **setting** that games and stories read; it owns *fictional truth*, never mechanics or prose voice.

> **WorldSpec owns what is true in the world. Games own mechanics. Stories own craft. Style profiles own tone.**

### 1.1 Where "setting genre" lives (three-axis model)

The **setting genre** — science fiction, fantasy, historical — is WorldSpec content, expressed as (a) the entities themselves (a world with FTL and machine-saints *is* sci-fi) and (b) an optional **genre profile** (§7) that sets conventions, tech level, cosmology and vocabulary defaults. Genre-as-setting is not a StorySpec extension and not a style profile; it is world truth.

---

## 2. Canonical ownership

```yaml
ownership:
  fictional_truth: worldspec              # entities, canon, relationships, chronology
  setting_genre_conventions: worldspec_genre_profile
  bible_voice: style_profile              # tone of the generated bible (shared artifact)
  mechanics: consuming_gamespec_package       # never here
  narrative_craft: consuming_storyspec_package  # never here
```

A consuming game or story **references** WorldSpec entities and layers its own concern (a mechanical profile, or a story arc). WorldSpec must not own skill ratings, dice, resources, POV, or arcs.

---

## 3. Package model

```text
worldspec-package/
  contract.md                   # orientation / world contract (required)
  package.spec.yaml             # manifest (extensions, genre profile, style profiles)
  entities/
    <entity records>.md
  chronology/
    timelines.md
  design/
    contradictions.md
    open_questions.md
  bible/                        # derived world bible (optional, generated)
```

Required: `contract.md` and the manifest. WorldSpec normally has **no dependencies** (it is the leaf); it may load worldspec extensions and profiles.

---

## 4. Manifest

Reuses the shared conventions (`spec-extension/0.6`).

```yaml
package:
  id: long_night_home_setting
  display_name: The Long Night Home — Setting
  spec: worldspec-core/0.6

extensions: []                  # worldspec extensions are possible; none needed here

genre_profile: science_fiction  # the setting-genre conventions (§7)

style_profiles:                 # tone for the generated bible (optional; shared artifacts)
  - grimdark
  - gothic

validation:
  mode: development
```

---

## 5. Entity model

WorldSpec's entity types (carried over from the standalone WorldSpec DSL, archived in `OldDSL/`), each with a stable ID, canon status and certainty:

```yaml
entity_types:
  - world | era | event | location | character | organisation | polity
  - culture | species | belief_system | language | tech_or_magic_system
  - item | creature | material | law | theme | narrative_asset
```

Each entity carries: `id`, `name`, `summary`, `canon_status` (§6), `certainty`, `relationships`, `sources`, `open_questions`. A **character here is a person** (who they are), distinct from a StorySpec `character_role` (their story function) and a GameSpec `actor`/`unit` (their game role) — consumers reference this entity and add their own layer.

---

## 6. Canon, reliability, chronology

- **Canon status:** `canon | provisional | unreliable | legend | rumour | discarded` (a claim may be true, doubted, or in-world propaganda).
- **Source tracking:** important claims cite the note/chapter/session they came from.
- **Relationships:** a typed graph (kinship, allegiance, causation, location) between entities.
- **Chronology:** eras, events and a calendar record; contradictions are recorded, not silently resolved.

These reuse the same certainty vocabulary StorySpec uses, so a shared setting reads consistently to both a game and a story. (The full treatment of canon status, source tracking, relationships and chronology carried over from the standalone WorldSpec DSL is normative as summarised here; the predecessor is archived in `OldDSL/` for history.)

---

## 7. Genre profile (setting genre)

A **genre profile** captures setting-genre conventions so entities are authored consistently. It is WorldSpec's answer to "what kind of world is this?"

```yaml
genre_profile:
  id: science_fiction
  tech_level: post-imperial FTL; machine intelligences; gene-craft; void habitats
  cosmology: material, indifferent; "gods" are ancient AIs / alien minds open to interpretation
  vocabulary_defaults: [station, ark, bioforge, machine-saint, void, reactor, lineage]
  conventions:
    - technology may be treated as sacred (shrine, rite) without being supernatural
    - power is sovereignty over life-support, docking, defence and stored lineages
```

A fantasy or historical world would declare a different genre profile. The genre profile is consumed when generating the bible and when a consuming game/story asks "what's plausible here?".

---

## 8. Bible style mechanism (style profiles are cross-base)

WorldSpec supplies a **style mechanism** for its generated bible, consuming the *same* shared style profiles a story or game uses. So one `grimdark` profile tones the novel, the world-bible and any game rulebook alike — the demonstration that tone is a portable artifact, not owned by one base.

```yaml
bible_style_mechanism:
  applies_profiles: [grimdark, gothic]
  scope: [entity_summaries, era_overviews, faction_descriptions]
```

---

## 9. Validation model

Mirrors GameSpec/StorySpec (§22): ERRORS (dangling relationship, duplicate ID, an entity referencing a non-existent era), OPEN ITEMS (declared open questions), WARNINGS (an entity with no relationships; a legend never referenced), IMPACT ITEMS (a canon change affecting consumers or the bible). Extension-contributed checks and validation modes work as in `spec-extension/0.6` §7.3. **Incomplete is allowed when declared; broken is not.**

---

## 10. Out of scope

WorldSpec does not own mechanics (GameSpec), narrative craft (StorySpec), the extension mechanism (`spec-extension/0.6`), tone profiles (shared artifacts), or a prose generator. It is the shared fictional-truth base the other bases depend on.
