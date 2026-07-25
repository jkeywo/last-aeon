@RTK.md

# The Last Aeons — Agent Guide

A character-led grand strategy game in a science-fantasy setting: personal
rule, succession, political relations, territory, and war across a world, a
moon, and an orbital starbase. Three design pillars carry everything:
**deterministic** (seed + data + ordered commands reproduce a campaign; saves
are a versioned snapshot plus an append-only command log, replays verified by
state hash), **headless-authoritative**, and **data-driven** (Rhai scripts
read validated context and emit typed effects; scripts never mutate sim
state). The setting canon lives under `the_last_aeons/` (worldspec).

## Tech stack

| Layer | Technology |
|---|---|
| Foundations | `crates/aeon_core` — stable IDs, deterministic RNG, calendar, fixed-point, state hashing; no Bevy |
| Content | `crates/aeon_data` — Rhai script host, content definitions, loaders, validation; no Bevy |
| Simulation | `crates/aeon_sim` — headless authoritative sim; Bevy ECS with no renderer/window/asset plugins |
| Client | `crates/aeon_client` (bin `last_aeons`) — native + web presentation; all UI is egui |
| Dev CLI | `crates/aeon_tools` (bin `aeon`) — validate-content, headless runs, replay verify/accept |
| Game data | Rhai under `assets/content/`, embedded at compile time by `aeon_client/build.rs`; display text in `assets/text/strings.csv` |
| Architecture model | PASM — YAML spec under `pasm/spec/`, tool pinned from vellum |
| CI | fleet-ci caller (`.github/workflows/ci.yml`) → pasm gates, clippy `-D warnings`, tests, content validation, replay acceptance, Trunk build, Pages deploy |

## Project rules

- Every meaningful player decision is a validated, logged `PlayerCommand`
  envelope; `clock::advance_one_day` is the single time entry point.
- RNG streams are *derived*, not shared: each use site derives from the
  campaign seed, a frozen purpose label, and the stable identities involved.
  Purpose labels are identities — renaming one re-rolls every outcome it
  ever produced, so they stay frozen even when the concept is renamed.
- Scripts read validated context and emit typed effects; they never mutate
  simulation state directly. Content is embedded so native, wasm, and CI
  ship byte-identical data.
- Read and update `pasm/spec/` before or alongside every structural change;
  record accepted choices in `pasm/spec/architecture/implementation-decisions.yaml`.

## PASM — keep it up to date

1. Model first, then build — spec entities before Rust for a new system.
2. Record decisions as you make them.
3. `uv run pasm validate pasm/spec` after any model change; fix before commit.
4. `uv run pasm scan pasm/spec --json` gates CI — keep implementation
   mappings current.
5. Never leave dead spec — removing a system updates its declarations.

## Common commands

```bash
# CI gates — run all of these before calling work done
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo run -p aeon_tools -- validate-content
cargo run -p aeon_tools -- accept
uv run pasm validate pasm/spec

# Run the game (run.bat / run.bat release wrap the same)
cargo run -p aeon_client

# Web build (Trunk config lives in crates/aeon_client/)
trunk serve --config crates/aeon_client/Trunk.toml     # http://localhost:8642
```

## Vellum — the shared foundation

This repo pins vellum by rev in `pyproject.toml` (pasm) and the `uses:` line
of `.github/workflows/ci.yml` (and `Cargo.toml` once the crate adoptions
land). A vellum bump PR aligns every pin and touches nothing else. Local
override etiquette: vellum `docs/handbook/local-dev.md` — never committed
active. Note: this repo's committed `.cargo/config.toml` carries real wasm
rustflags; a vellum `[patch]` must never be added to it.
