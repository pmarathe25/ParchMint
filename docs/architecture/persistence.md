# Persistence and recovery

> Read when changing project paths, manifests, document saves, locking,
> transactions, backups, recovery, or external-change behavior.

Canonical projects are ordinary directories. Storage safety is enforced below
the UI.

## Canonical and local state

| Canonical | Local/derived under `.parchmint/` |
|---|---|
| `parchmint.toml`, `outline.toml`, `styles.toml` | Workspace preferences |
| Preset manifests | Recovery journals and rotating backups |
| Markdown documents | Pending transaction state |
| Project-relative assets | SQLite search/count cache |

Deleting `.parchmint/` must never delete authored content.

## Write path

1. Validate bounded input and project-relative paths.
2. Serialize deterministically.
3. Write and sync a sibling temporary file.
4. Atomically replace the destination and sync its parent.

Document autosave writes a revisioned recovery record before canonical content.
Multi-file structural changes stage `.parchmint/pending-save-v1`; project open
rolls back an interrupted transaction before exposing state. Only dirty
resources are written.

## Conflict rules

- OS advisory locks prevent two live writers; read-only fallback performs no writes.
- Clean external changes reload through the normal parser.
- Dirty external changes require explicit reload, overwrite, or save-copy choice.
- A newer recovery record is offered before editing; corrupt records are isolated.
- Unsupported newer schemas are retained and rejected for writing.
- Index failure triggers rebuild, never canonical repair.

Normative contracts: [project format](../format/project-format-1.md) and
[recovery format](../format/recovery-1.md). Rationale:
[ADR-0006](adr/0006-atomic-write-and-recovery-direction.md),
[ADR-0007](adr/0007-sqlite-fts-cache.md), and
[ADR-0014](adr/0014-incremental-transactions-and-revisions.md).
