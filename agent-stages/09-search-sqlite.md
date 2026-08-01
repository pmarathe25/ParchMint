# S80 — SQLite FTS5 Search Adapter

## Goal

Implement `SearchIndex` with exact bundled `rusqlite =0.40.1`/SQLite FTS5 using the validated design.

## Tasks

- Dedicated worker and connection.
- Startup FTS5 capability assertion.
- Stable project/node/document/block/revision schema.
- Body/title/Synopsis/metadata indexing.
- Entire-project v1 queries across body, title, Synopsis, and metadata fields. Do not implement or expose user-selectable section/subtree scope in v1; retain only narrow internal hooks already required by the architecture.
- Escaped, allow-listed MATCH construction.
- Case-sensitive and Unicode whole-word post-filtering.
- Streaming result batches, generation cancellation, snippets, and revision revalidation.
- Integrity checks and deterministic disposable rebuild.
- Reproduce V04 20-million-word and cross-platform parity behavior.

## Boundary rules

No SQLite connection/types escape `SearchIndex`. No SQLite work runs on the UI thread. Do not add Tantivy.

## Pass criteria

Contract, safety, cancellation, rebuild, parity, and scale targets pass on Windows, macOS, and Linux.
