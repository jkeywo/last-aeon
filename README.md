# The Last Aeons

[![CI](https://github.com/jkeywo/last-aeon/actions/workflows/ci.yml/badge.svg)](https://github.com/jkeywo/last-aeon/actions/workflows/ci.yml)

A character-led grand strategy game set in **The Last Aeons** — personal rule,
succession, political relationships, territorial power, and conflict across a
world, its moon, and an orbital starbase. Built in Rust on Bevy with a
deterministic, headless, data-driven simulation and Rhai-authored content.

**▶ Play in the browser:** <https://jkeywo.github.io/last-aeon/>
(published from `main` by CI; lead House Harrow through the Ashkarr Succession).

Each body's globe is a political map — provinces painted by their holding
house with black borders, standing armies and ships shown where they are —
and it can be re-read as order, wealth, military strength, relations, or
the race for the Paramountcy. Every action shows its odds, costs and risks
before you commit to it. Provinces you neglect slip out of order and will
eventually throw you off; houses owe each other favours and nurse
grievances; and rival houses act on the pressures they are actually under,
and say why.

## Repository layout

| Path | Contents |
| --- | --- |
| `crates/aeon_core` | Engine-agnostic foundations: stable IDs, deterministic RNG, calendar, fixed-point arithmetic, state hashing |
| `crates/aeon_data` | Authored-content pipeline: Rhai script host, content definitions, loaders, validation |
| `crates/aeon_sim` | Headless authoritative simulation: ECS components, systems, commands, persistence |
| `crates/aeon_client` | Native + web presentation: 3D system/planet maps, 2D information panels |
| `crates/aeon_tools` | Developer CLI (`aeon`): content validation, headless runs, replay verification |
| `pasm/` | PASM — the executable architecture model and its authored spec (`pasm/spec/`) |
| `the_last_aeons/` | Setting canon source material |

## Working on the game

```powershell
cargo test --workspace                 # headless simulation test suite
cargo run -p aeon_client               # run the game natively
cargo run -p aeon_tools -- validate-content  # validate authored content
cargo run -p aeon_tools -- accept      # end-to-end deterministic-replay acceptance
uv run pasm validate                   # validate the architecture model
```

The architecture model in `pasm/spec/` is authoritative for design intent.
Add or update PASM entities alongside (or before) the code that implements
them; CI validates the model on every push.

### Web build and delivery

The client shares the Rust/Bevy simulation between native and web. CI builds
the release WebAssembly bundle with [trunk](https://trunkrs.dev) and deploys
it to GitHub Pages from `main`. To build it locally:

```powershell
trunk serve --config crates/aeon_client/Trunk.toml   # dev server at :8642
```

> **One-time setup:** GitHub Pages must be enabled for the repository with
> **Settings → Pages → Source: GitHub Actions** for the deploy job to publish.

## Design pillars

- **Deterministic**: one campaign seed + authored data + ordered player
  commands fully determine the campaign. Saves are versioned snapshots plus an
  append-only command log, and replays are verified by state hash.
- **Headless-authoritative**: the simulation runs without a renderer; native
  and web clients attach presentation to the same simulation.
- **Data-driven**: scenarios, jobs, results, and events are authored as Rhai
  scripts that read validated context and emit typed effects — scripts never
  mutate simulation state directly.
