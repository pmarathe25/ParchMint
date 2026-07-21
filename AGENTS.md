# Agent entry point

Load the least context needed for the assigned task.

## Start

1. Inspect `git status`; preserve unrelated work.
2. Open [`docs/README.md`](docs/README.md).
3. Read only the task row and its first linked page.
4. Open the owning `crates/*/README.md` when changing Rust.
5. Load a format specification or ADR only when the task touches that boundary.

Do not read every document, ADR, or plan. Read a plan only when the task names
it or explicitly asks for plan work. Plan structure is defined in
[`plans/README.md`](plans/README.md).

## Hard boundaries

- Rust owns canonical state; QML never reads project files.
- Qt types stop at `parchmint-bridge`.
- Canonical content is Markdown/TOML/assets; SQLite and `.parchmint/` are non-authoritative.
- Qt Markdown/HTML conversion is never canonical serialization.
- Async work carries generation/revision stamps and stale results are dropped.
- Unknown fields and unsupported Markdown remain recoverable source.
- No runtime network access, telemetry, or hidden content export.

## Verify

Use the narrow command while iterating, then the full affected gate:

```sh
just format-check
just lint
just test-rust
just test
```

See [testing](docs/development/testing.md) for routing. Update the single
authoritative document when behavior or a boundary changes. Follow
[documentation conventions](docs/conventions.md).
