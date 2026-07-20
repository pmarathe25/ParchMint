# Stage 06 handoff: search, index, and statistics

Status: implemented in the shared worktree. Rust index/app tests pass on the
available Linux environment. Qt is unavailable (`QMAKE` is not installed), so
the CXX-Qt/QML additions need native verification on the pinned Qt 6.8.3 host.

## Delivered behavior

- `.parchmint/index.sqlite` is a disposable SQLite FTS5 cache. It stores a
  schema version separate from the project format, and projects remain fully
  readable/editable if the cache cannot be opened.
- Schema version 2 stores node/document identity, manuscript/research scope,
  title, synopsis, normalized body text, canonical relative path, source
  fingerprint, status/labels/tags, hierarchy ancestry, and stored word and
  character totals. FTS uses bundled SQLite `unicode61` with diacritic removal.
- An absent, incompatible, or malformed cache is recreated. Recovery deletes
  only the exact `index.sqlite` cache and its WAL/SHM sidecars; it never writes
  Markdown, TOML, assets, or trash.
- `ProjectWorkspace` opens the small cache without scanning all bodies. The
  first explicit search/count rebuild derives rows from its canonical open
  state; subsequent document saves, metadata edits, creates, duplicates,
  moves/reparents/reorders, trash, restore, undo, and redo update/delete only
  affected node/subtree rows. A cache failure is retained as a non-fatal index
  diagnostic after the canonical operation succeeds.
- Rust exposes ranked stable search rows and stored aggregate totals. The QML
  header has a search palette with active/other-pane open actions; document
  panes have case-sensitive optional local find/replace (whole-word and regex
  are intentionally deferred until they have native QML tests) and live
  selection-or-document counts.

## Index schema/version and recovery

`parchmint-index::INDEX_SCHEMA_VERSION` is `2`. `index_meta` contains
`schema_version`; `document_meta` holds non-FTS fields/counts; `document_fts`
holds `node_id` (unindexed), title, synopsis, normalized body, labels, tags,
and status. Opening version 1 or a partial schema drops and recreates these
derived tables. A `file is not a database`, malformed schema, or malformed
database image retries after removing only this derived SQLite database.

## Event-to-index update map

| Canonical event | Derived action |
|---|---|
| Document body save | Upsert its node after atomic canonical save |
| `NodeCreated`, `NodeRenamed`, `NodeReordered` | Upsert the node/subtree |
| `NodeReparented`, `NodeDuplicated`, `NodeRestored` | Upsert restored/moved subtree to refresh hierarchy/scope |
| `NodeTrashed` | Delete every subtree row |
| `MetadataEdited` | Upsert owning node |
| Style-only event | No text/index change |

The cache starts `RebuildNeeded`; mutations do not create a partial cache
before its initial rebuild. Once ready, incremental work is transactional. The
app APIs preserve cache status/diagnostics so a worker adapter can schedule the
explicit rebuild without blocking QML. The current bridge invokes the initial
query synchronously; moving that call to the existing `BackgroundWorker` is the
next integration step for the full 10-million-word gate.

## Search syntax

Whitespace-separated unquoted words are Unicode token-prefix matches:
`orch` finds `orchard`. Quote an exact phrase: `"winter harbor"`. Terms are
combined with AND. The Rust `SearchQuery` also filters by `scope` (`manuscript`
or `research`), title/synopsis/body field selection, ancestor `subtree`, exact
status, label, and tag. Results sort by FTS rank then stable node ID, which
makes incrementally appended batches deterministic. Snippets use private FTS
markers and are rendered as plain text by QML.

## Unicode count rules

Character count is Unicode scalar values, including whitespace and punctuation
(not bytes or UTF-16 units). A word is a run containing Unicode letters or
numbers; apostrophe/curly apostrophe and hyphen/non-breaking hyphen may join a
run. This is pure Rust character classification, not a locale/platform API, so
the behavior is stable across supported platforms. Live editor counts use this
same function; aggregate counts are the stored cache sums.

## Verification and measurements

- `cargo fmt --all --check`: passed.
- `cargo test -p parchmint-index -p parchmint-app --offline`: 19 passed, 3
  ignored manual measurements.
- `cargo test -p parchmint-index --offline
  records_stress_corpus_rebuild_and_first_result_timing -- --ignored --nocapture`:
  passed. Debug build, 10,000 documents × 50 repeated four-word phrases:
  rebuild **834.36 ms**; first 50 prefix-search results **114.95 ms**.
- `cargo check -p parchmint_bridge --offline`: blocked before compiling bridge
  changes because the environment has no Qt installation (`QMAKE` missing), as
  in Stages 01–05.

## Remaining risks and recommended next task

The existing Stage 02 storage open still reads all Markdown bodies, so the
complete 10,000-node open-time gate cannot be demonstrated until storage gains
front-matter-only lazy body loading. Also connect `ProjectSearch::rebuild` and
search execution to the existing revision/generation-stamped background worker
before the stress-project UX gate; the service API was deliberately kept Qt-free
for that adapter. On a Qt 6.8.3 host, compile and test the new bridge/QML search
palette, local find/replace, keyboard focus, and screen-reader labels.
