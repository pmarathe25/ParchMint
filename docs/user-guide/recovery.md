# Saves and recovery

> Read when diagnosing save state, recovery records, backups, or external edits.

Typing updates one live Rust document session immediately. After the autosave
debounce, ParchMint writes a revisioned recovery journal before replacing the
canonical Markdown file. `Ctrl/Cmd+S` flushes open documents.

## Recovery behavior

- Newer recovery data is reviewed before normal editing.
- Each record can be restored, discarded, or saved as a separate Markdown copy.
- A corrupt record is isolated; it does not hide valid records.
- Read-only project access does not consume or update recovery data.

## Backups and outside edits

Canonical replacement keeps bounded prior versions under
`.parchmint/backups/<document-id>/`. These backups are local safety data, not
portable project content.

Clean files changed outside ParchMint reload through the normal parser. If both
disk and the live session changed, choose reload, overwrite, or save-copy;
ParchMint does not silently select a destructive action.

See [Persistence](../architecture/persistence.md) for implementation rules and
[Recovery format 1](../format/recovery-1.md) for the record schema.
