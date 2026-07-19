# Stage 06: search, index, and statistics

Difficulty: medium  
Recommended model: GPT-5.6 Terra, medium or high reasoning  
Depends on: stages 2, 4, and 5  
Master references: `PLAN.md` “Locked architecture,” “Data-integrity, security, and privacy contract,” and “Cross-stage quality gates”

## Outcome

Deliver a rebuildable incremental full-text index, responsive project search, find/replace foundations, and Unicode-aware statistics without slowing project opening or typing.

## Primary ownership

- `crates/parchmint-index`
- Search/statistics services in `crates/parchmint-app`
- Search and count bridge models
- Search palette/panel and statistics QML
- Generated search corpora and index benchmarks

## Required work

### 1. Derived index schema

- Implement the stage 1 SQLite FTS5 design for node ID, scope, title, synopsis, normalized body text, path, fingerprints, and searchable metadata.
- Store index schema/version separately from the project format.
- Make delete/rebuild safe while the project is open.
- Detect missing, stale, corrupt, or incompatible indexes and recover without blocking project access.
- Keep SQLite connections and long transactions off the UI thread.

### 2. Incremental indexing

- Initial open reads only manifests/front matter needed for immediate UI.
- Scan and index bodies in bounded cancellable batches.
- Update only affected rows after canonical document saves or metadata changes.
- Remove or relocate entries after trash/restore/delete/move events.
- Use project generations and document revisions so stale indexing work cannot reintroduce old text.
- Surface indexing progress and degraded/unavailable status without disruptive dialogs.

### 3. Project search

- Stream ranked results in stable batches with title, hierarchy context, scope, and highlighted snippets.
- Filter by manuscript/research, title/synopsis/body, subtree, status, label, and tags.
- Support exact phrase and sensible token-prefix behavior; document query syntax.
- Open a result in the active or other pane and reveal it in the binder.
- Preserve user focus and current query while results update.

### 4. Statistics and find foundations

- Define and document Unicode-aware word and character counting.
- Maintain section counts after edits without rescanning the project synchronously.
- Aggregate counts for multi-selection, subtree, manuscript, research, and project.
- Show live selection/document counts in the editor and stored aggregate counts elsewhere.
- Provide document-local find and replace with case/whole-word/regex choices only if each option is safely testable; project-wide replacement may remain a stage 9 feature.

## Acceptance gate

- Deleting `.parchmint/index.sqlite` and rebuilding produces search/count results equivalent to canonical files.
- Saved edits, metadata changes, trash/restore, moves, recovery, and external changes update results correctly.
- Project opening does not wait for full indexing.
- First indexed results meet the 300 ms budget on the stress corpus.
- Reindexing can be canceled/restarted and does not visibly stall typing or tree scrolling.
- Word-count behavior is documented and consistent across platforms/locales.
- Corrupt indexes self-recover without risking canonical content.

## Out of scope

- Semantic/vector search
- Cloud search
- Search inside unsupported binary attachments
- Unreviewed bulk project replacement

## Handoff

Create `docs/handoffs/06-search-index-and-statistics.md`. Record index schema/version, event-to-index update map, query syntax, word-count rules, corruption recovery, and stress-corpus timings.
