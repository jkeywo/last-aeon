# PASM for Project Murmur

PASM is a portable, executable architecture model. It validates authored YAML,
tracks state authority and dependencies, scans a repository for implementation
evidence, and can generate traceability and audit bundles.

This copy contains only reusable tooling. Project Murmur's authored model begins
in `pasm/spec/`.

## Common commands

```powershell
uv run pasm validate
uv run pasm scan --json
uv run pasm traceability
uv run pasm context --entity <id>
```

Add the model for a system before (or alongside) its implementation. Keep
player experience and gameplay rules in authored PASM declarations rather than
inside the tool itself.
