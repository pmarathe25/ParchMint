# S80 — SQLite FTS5 Search Adapter

## Goal

Implement `SearchIndex` with `rusqlite =0.40.1` and bundled SQLite FTS5 under the current architecture.

## Tasks

- Dedicated worker/connection and startup FTS5 assertion.
- Stable project/document/block/revision schema.
- Body/title/Synopsis/metadata indexing.
- Entire-project v1 queries; no user-selectable section/subtree scopes.
- Escaped allow-listed MATCH construction.
- Case-sensitive/Unicode whole-word post-filtering.
- Streaming batches, cancellation, snippets/ranking, revision revalidation.
- Integrity checks and deterministic disposable rebuild.
- Bundled SQLite/resolved-lock assertions.

## Boundary rules

No SQLite connection/types escape `SearchIndex`; no SQLite work runs on UI thread. Do not add another search backend without G20.

## Pass criteria

- Contract, safety, cancellation, rebuild, parity, and medium-scale tests pass on all platforms.
- Warm first-result target passes.
- The exact 20-million-word corpus is Tier C nightly/release-candidate evidence rather than an ordinary pull-request requirement.
