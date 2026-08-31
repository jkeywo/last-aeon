# Domain documentation

Last Aeon is a single-context repository. Engineering skills should use the
project's existing design, architecture, and setting sources rather than
creating a parallel glossary or ADR hierarchy.

## Authoritative sources

- Read `docs/gdd/` for the current game-design contract and project domain
  vocabulary.
- Read `pasm/spec/` for structural architecture, implementation mappings, and
  accepted decisions. Model structural changes before or alongside code.
- Read `the_last_aeons/` for setting canon and authored world facts.
- Read the root `AGENTS.md` and `RTK.md` before repository work.

## Decision handling

- Surface conflicts between proposed work and existing GDD or PASM decisions;
  do not silently override them.
- Preserve the repository's AI-origin markers. New agent-originated PASM
  entities use `origin: ai`, and agent-originated rationale uses the literal
  `[ai] ` prefix.
- Do not change an unmarked accepted decision without explicit user approval.
- Use the domain terms already established by the GDD and PASM model in issue
  titles, PRDs, tests, and implementation proposals.

If `CONTEXT.md`, `CONTEXT-MAP.md`, or `docs/adr/` is introduced later, consume
the relevant files in addition to these sources.
