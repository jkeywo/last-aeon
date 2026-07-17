# World: The Last Aeons (v0.1, worldspec-core)

An imported far-future setting of human and alien polities, ancient infrastructure, Maelstrom travel, posthuman species, and cosmic entities. This package is a direct ingestion of the supplied World Anvil export; it owns setting truth only.

```yaml
world:
  id: the_last_aeons
  premise: |
    Humanity and its descendants contend across the Milky Way through a sparse network of Maelstrom Gates. The Pan Human Hegemony, its successor states, resistant cultures, ancient Precursor works, and entities of the Maelstrom all shape the current age.

genre_profile:
  id: science_fantasy_space_opera
  tech_level: interstellar civilisations linked by ancient FTL gates; genetic enhancement, cloning, artificial intelligences, and megastructures
  cosmology: real space is connected to the mysterious Maelstrom; its Elders, Dreamers, and Worm Gods may be entities, divinities, or both
  vocabulary_defaults: [Hegemony, Maelstrom, gate, Legion, sectorum, Precursor, Torch-Engine, AI, alien]
  conventions:
    - Maelstrom travel is a scarce strategic infrastructure rather than ordinary transit.
    - Ancient and alien technology is consequential, partly understood, and politically contested.
    - In-world claims about Maelstrom entities may be canon, legend, or rumour; preserve their stated status.
```

## Source and import policy

The source is `The Last Æons Homepage _ World Anvil.html`, supplied in the workspace. The 98 exported articles are recorded in `entities/` with their World Anvil UUID, template, category, full body text, and a source reference. The two exported timelines are recorded in `chronology/world_anvil_timelines.md`.

Imported records are `canon_status: canon` and `certainty: certain` by default: the export is the supplied authority. This is an ingestion state, not an editorial claim that every in-world assertion is objectively true. Reclassify specific records when their source material establishes legend, rumour, or unreliable narration.
