# parchmint-domain

> Read when changing IDs, project graph invariants, styles, presets, commands,
> events, undo/redo, generations, or revisions.

Owns Qt-free domain rules and validation. Does not perform I/O, parsing, or UI
work. Persisted shape changes require the relevant
[format specification](../../docs/format/README.md) and possibly an
[ADR](../../docs/architecture/adr/README.md).

Check: `cargo test -p parchmint-domain`.
