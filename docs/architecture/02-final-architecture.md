# ParchMint v1 Final Architecture

**Status:** Current implementation architecture  
**Version:** 1.3  
**Date:** 2026-07-31  
**Primary audience:** Implementation agents, architecture reviewers, maintainers

## 1. Decision summary

ParchMint v1 uses:

- Tauri 2.11.5 as the cross-platform desktop shell.
- TypeScript and React for the application UI.
- Exact-locked ProseMirror packages behind a ParchMint-owned editor contract.
- Rust for domain/application services, persistence, recovery, history, search, spellcheck orchestration, export, and platform-independent logic.
- Restricted deterministic HTML5, TOML, CSS, JSON, and UTF-8 text sidecars as canonical authored data.
- `git2 =0.21.0` with vendored libgit2 behind `HistoryStore`.
- `rusqlite =0.40.1` with bundled SQLite FTS5 behind `SearchIndex`.
- A ParchMint-owned `SpellcheckService`; S65 selects and proves the initial offline engine/adapter.
- Ports-and-adapters boundaries that allow GUI/editor, history, search, spellcheck, save/load, recovery, and exporters to evolve independently.

The shared two-view editor topology is fixed at the contract level, but the canonical-projection implementation is intentionally selected by S55 rather than prescribed prematurely. A ProseMirror worker mirror, a neutral delta/block mirror, or bounded main-thread/idle projection may be selected only after measured native evidence.

All supported documents receive the same user-visible functionality. No implementation may hide failure by adding a user-visible large-document mode.

## 2. Architectural drivers

1. Cross-platform parity from the first release.
2. 10–20 million words per project without eager document-body loading.
3. Documents near 250,000 words with one consistent feature set.
4. Low-latency typing and nonblocking save/history/search/spellcheck/export.
5. Two views of one document with independent view state and shared content/undo.
6. Open, deterministic, Git-friendly current data.
7. Complete checkpoint history and recoverability.
8. Project-level interactive undo distinct from durable History.
9. Light/Dark/System appearance with no authored-data effects.
10. Penpot-first design handoff with traceable implementation.
11. Testability of core behavior without initializing the GUI.

## 3. Architectural principles

### 3.1 Canonical and derived state are separate

Canonical authored state consists only of versioned project files. Git objects, SQLite indexes, editor state, spellcheck decorations, recovery logs, application preferences, and workspace state are implementation or derived data unless explicitly named otherwise.

### 3.2 Depend on ParchMint-owned contracts

Domain/application modules depend on interfaces and data types defined by ParchMint, never directly on Tauri, React, ProseMirror, `git2`, `rusqlite`, a spellcheck library, or an operating-system API.

### 3.3 One authority per state category

At any moment:

- Project hierarchy, metadata definitions/values, style definitions, export settings, project dictionary, deletion tombstones, and project undo: Rust application/domain core.
- Active rich-text document content and document undo: one `SharedEditorSession` per open document in the editor adapter.
- Per-view selection, scroll, focus, composition, and local-search state: each `EditorViewSession`.
- Durable current files: canonical project store after save acknowledgement.
- Historical checkpoints: `HistoryStore`.
- Search results: disposable `SearchIndex`, revalidated against current revisions.
- Spellcheck results: disposable `SpellcheckService` results, revalidated against block/document revisions.
- Appearance and global dictionary: application-preference store outside project history.
- Pure layout/hover/animation state: frontend UI store.

A persistence/search/recovery projection is never a competing interactive editor authority.

### 3.4 No synchronous persistence or analysis from input

A keypress must not wait for Rust IPC, disk, Git, SQLite, spellcheck, export, canonical serialization, or full-document analysis before painting. High-frequency changes are applied locally and propagated through bounded, revisioned, asynchronous channels.

### 3.5 Replace coarse capabilities, not generic widgets

ParchMint does not build a cross-framework widget toolkit. Replacement occurs at service and editor boundaries. A GUI/editor replacement rewrites widgets and input integration but retains canonical format, application logic, history/search/save/recovery/export contracts, and headless tests.

### 3.6 Native claims require native evidence

Packaging and headless tests do not prove IME, clipboard, accessibility, spellcheck menu behavior, scaling, filesystem durability, or interactive performance. Those claims require native release-mode evidence on Windows, macOS, and Linux.

### 3.7 Current documents are authoritative

When an approved architecture change occurs, update this document and dependent contracts directly. Do not retain a parallel architecture-decision log or treat historical prototypes as governing inputs.

## 4. Process, windows, and project sessions

ParchMint uses one normal application process with zero or more project windows.

```text
ApplicationProcess
├── ApplicationPreferences
│   ├── appearance: system | light | dark
│   └── global_dictionary
├── PlatformServiceRegistry
├── WindowRegistry
└── ProjectSessionRegistry
    └── ProjectSession per open project
        ├── one writable project lock
        ├── project/domain state
        ├── save/recovery/history/search services
        └── one or more windows only if a future feature explicitly allows it
```

For v1, one project maps to one project window. A process may own multiple different project windows.

A single-instance platform adapter receives later process-launch requests where the platform/runtime supports it and routes an open-project request to the existing process. The project lock remains authoritative for independently started binaries, crashes, and unsupported focus paths. If an existing window cannot be focused safely, the second attempt shows a locked-project message.

Window focus, geometry, appearance, and recent-project lists are application/workspace state, not authored project history.

## 5. Repository and module layout

Recommended structure:

```text
ParchMint/
├── apps/
│   ├── desktop-ui/
│   │   ├── src/app/
│   │   ├── src/components/
│   │   ├── src/features/
│   │   ├── src/editor/
│   │   ├── src/design-tokens/
│   │   └── src/generated-contracts/
│   └── desktop-tauri/
│       └── src-tauri/
├── packages/
│   ├── ui-contract/
│   ├── editor-contract/
│   ├── editor-prosemirror/
│   ├── design-system/
│   └── test-fixtures/
├── crates/
│   ├── parchmint-domain/
│   ├── parchmint-application/
│   ├── parchmint-project-format/
│   ├── parchmint-project-repository/
│   ├── parchmint-project-fs/
│   ├── parchmint-save/
│   ├── parchmint-recovery-api/
│   ├── parchmint-recovery-fs/
│   ├── parchmint-history-api/
│   ├── parchmint-history-git2/
│   ├── parchmint-search-api/
│   ├── parchmint-search-sqlite/
│   ├── parchmint-spellcheck-api/
│   ├── parchmint-spellcheck-<selected>/
│   ├── parchmint-export-api/
│   ├── parchmint-export-html/
│   ├── parchmint-platform-api/
│   ├── parchmint-tauri-adapter/
│   ├── parchmint-core-cli/
│   └── parchmint-test-support/
├── contracts/
├── design/
│   ├── source/
│   ├── handoff/<version>/
│   └── generated/
├── docs/
├── scripts/
└── tests/
```

Only adapters import external framework/backend types. Public domain/application signatures use ParchMint-owned types.

## 6. Dependency baseline and supply-chain controls

### 6.1 Selected direct dependencies

Bootstrap begins with these accepted selections:

```toml
git2 = { version = "=0.21.0", default-features = false, features = ["vendored-libgit2"] }
rusqlite = { version = "=0.40.1", default-features = false, features = ["bundled"] }
```

Initial ProseMirror direct versions:

```text
prosemirror-commands      1.7.1
prosemirror-history       1.5.0
prosemirror-inputrules    1.5.1
prosemirror-keymap        1.2.3
prosemirror-model         1.25.11
prosemirror-schema-basic  1.2.4
prosemirror-schema-list   1.5.1
prosemirror-state         1.4.4
prosemirror-transform     1.12.0
prosemirror-view          1.42.2
```

S20 creates the actual application lockfiles. Those application lockfiles, not historical prototype locks, become authoritative.

### 6.2 Lock assertions

CI must assert:

- Direct versions match the approved baseline until a G20-approved update changes it.
- `git2 =0.21.0` resolves to the expected vendored `libgit2-sys`/libgit2 family or the stage stops for review.
- No system libgit2, installed Git executable, or system SQLite is silently selected.
- ProseMirror packages resolve from npm with expected integrity data.
- The canonical ProseMirror source host is the active upstream under `code.haverbeke.berlin/prosemirror/`; archived GitHub repositories are not treated as current provenance.

### 6.3 Scheduled provenance checks

Run on lockfile changes, weekly, and for every release candidate:

- Rust/npm advisory scans.
- License inventory.
- Package integrity/provenance where available.
- Canonical source availability and source/license changes.
- Unexpected registry maintainer/ownership changes.
- SBOM generation.

A new upstream release alone does not require G20. Missing or irreconcilable source, a material license/security issue, a required maintained fork, or a boundary-changing replacement does.

## 7. Contracts and type generation

### 7.1 Versioned schemas

Tauri command/event payloads, editor-neutral payloads where shared across languages, and design manifests use versioned JSON Schema under `contracts/`.

Rust and TypeScript types are generated from or validated against the same schemas. They are not maintained independently by hand.

Every command/event envelope includes:

```text
schema_version
request_id or event_id
project_id where applicable
expected_project_revision where applicable
payload
```

### 7.2 Drift guard

CI runs contract generation and fails if generated Rust/TypeScript files change:

```text
generate-contracts
git diff --exit-code -- <generated-rust> <generated-typescript>
```

Generated headers contain schema version and source checksum. Cross-language golden fixtures must serialize/deserialize in both directions.

### 7.3 Compatibility

- Prefer additive fields.
- Removed/renamed fields require a schema-version bump and migration.
- Unknown noncritical fields are ignored safely.
- High-frequency keystrokes do not cross Tauri IPC individually.

## 8. Domain model and canonical format

Core IDs are opaque and stable:

```text
ProjectId
NodeId
DocumentId
GroupId
StyleId
MetadataFieldId
CommentThreadId
CommentMessageId
BlockId
RevisionId
CheckpointId
ViewId
ProjectOperationId
UndoEntryId
```

Canonical layout:

```text
my-novel/
├── .git/
├── project.toml
├── styles.css
├── dictionary.txt
├── manuscript/
│   └── part-one--<id>/
│       └── chapter-one--<id>.html
├── research/
│   └── characters--<id>/
│       └── protagonist--<id>.html
├── annotations/
│   └── <document-id>.json
└── .parchmint/
    ├── format-version
    ├── recovery/
    ├── cache/
    │   ├── search.sqlite
    │   └── word-counts.json
    └── workspace.json
```

`project.toml` is authoritative for project identity, ordered nodes, relative paths, display titles, Synopsis/metadata, field definitions, style semantic metadata, export settings, language, and deletion tombstones.

`dictionary.txt` is deterministic UTF-8/LF, one normalized project word per line in stable sort order. Its normalization rules are versioned. The global dictionary uses the same logical format in the application-preference directory but does not enter project save/history.

### 8.1 Restricted HTML

Allowed blocks include paragraph, Document Title/headings, block quote, ordered/unordered lists, Verse, Scene Break, and Page Break. Allowed marks include strong, emphasis, underline, strikethrough, small caps, superscript, subscript, and link.

Every addressable block has a stable `data-pm-id`. Semantic style references use stable IDs. Scripts, event handlers, arbitrary inline styles, unsupported elements, and remote embeds are forbidden.

### 8.2 Deterministic serialization

- UTF-8 and LF.
- Stable attribute and record order.
- Stable class/ID generation.
- One representation for equivalent documents.
- No formatter-driven paragraph rewrapping.
- No rewriting unchanged documents.
- Sanitize before canonicalization.
- Golden byte-equality tests.

### 8.3 Migrations

Format migrations:

1. Validate the source project.
2. Take a pre-migration checkpoint when history is available.
3. Write through the atomic save pipeline.
4. Preserve stable IDs where possible.
5. Test every supported prior format fixture.
6. Reset interactive undo/redo after successful migration.
7. Never depend on SQLite or frontend state.

## 9. Application command and project-undo architecture

All project-authoring mutations execute through `ProjectCommandDispatcher`.

```rust
trait ProjectCommandDispatcher {
    fn execute(&self, command: ProjectCommand) -> Result<ProjectCommandResult>;
    fn undo(&self) -> Result<ProjectCommandResult>;
    fn redo(&self) -> Result<ProjectCommandResult>;
    fn state(&self) -> ProjectUndoState;
    fn reset(&self, reason: UndoResetReason);
}
```

A project command is validated against expected project/resource revisions and produces an atomic application result plus an inverse representation.

### 9.1 Undo entry

```text
ProjectUndoEntry
├── operation_id
├── label
├── forward_command or forward_patch
├── inverse_command or inverse_patch
├── expected_start/end revisions
├── affected resources/documents
├── approximate byte cost
└── checkpoint_group_id
```

Initial bounds are explicit constants: 100 complete logical operations and 64 MiB of in-memory inverse payload. The implementation may spill large inverse payloads to a transient session file. Eviction removes only complete oldest operations.

- A new project command clears redo.
- Undo/redo applies as a new authored state and saves/checkpoints normally.
- Project close/reopen resets project undo/redo.
- Whole-project History restore, accepted recovery replay, and migration reset project and open-document undo/redo.
- Text-input native undo remains local to focused text inputs and does not bypass project commands on commit.

### 9.2 Global replacement

Global replacement is a composite project command:

1. Revalidate selected matches against current revisions.
2. Build the complete inverse patch before applying changes.
3. Apply to open `SharedEditorSession`s and closed canonical documents under one operation ID.
4. Suppress independent document-history entries for the same composite operation; editor sessions receive a project-command boundary.
5. Stage every affected canonical resource in one save transaction.
6. Create one history checkpoint after the complete durable write succeeds.
7. On failure, roll back/recover as one operation rather than leaving a partial replacement.

## 10. Editor contract

Only `packages/editor-prosemirror` imports ProseMirror. Other code sees ParchMint types.

```ts
interface EditorAdapter {
  openDocument(input: CanonicalDocumentLoad): Promise<EditorSessionHandle>;
  attachView(session: EditorSessionHandle, view: ViewId, host: HTMLElement): void;
  detachView(session: EditorSessionHandle, view: ViewId): ViewState;
  executeCommand(session: EditorSessionHandle, view: ViewId, command: EditorCommand): void;
  applyProjectOperation(session: EditorSessionHandle, operation: ProjectDocumentOperation): Promise<void>;
  getSelection(session: EditorSessionHandle, view: ViewId): EditorSelection;
  getSelectionGeometry(session: EditorSessionHandle, view: ViewId): SelectionGeometry | null;
  setStyleCatalog(session: EditorSessionHandle, styles: StyleCatalogProjection): void;
  setSearchDecorations(session: EditorSessionHandle, view: ViewId, matches: SearchDecoration[]): void;
  setSpellcheckDecorations(session: EditorSessionHandle, view: ViewId, results: SpellcheckDecoration[]): void;
  requestCanonicalProjection(session: EditorSessionHandle, throughRevision: number): Promise<CanonicalProjection>;
  subscribe(listener: EditorEventListener): Unsubscribe;
  closeDocument(session: EditorSessionHandle): Promise<void>;
  capabilities(): EditorCapabilities;
}
```

ProseMirror nodes, steps, selections, plugin keys, and DOM nodes do not cross into Rust domain APIs or canonical files.

## 11. Shared document and two-view state topology

Each open document has one `SharedEditorSession`:

```text
SharedEditorSession
├── shared document value
├── shared document-history controller
├── shared semantic plugin state
├── shared comments/anchor mapping
├── editor revision
├── projection channel
└── ViewSession A / ViewSession B
    ├── independent selection
    ├── stored marks when view-local
    ├── scroll/viewport anchor
    ├── local search
    ├── focus
    ├── IME/composition state
    └── mounted EditorView
```

Plugin state is classified before implementation:

- **Shared semantic state:** history, document-level comments/anchors, document revision, schema/style configuration.
- **View-local state:** selection, local search, focus, scroll/viewport, transient menu geometry, composition.
- **Derived replicated state:** decorations that can be recreated from shared or view-local inputs.

Two ordinary independent ProseMirror history plugins are prohibited.

Transaction flow:

1. The focused view creates a transaction against its current session revision.
2. The session controller validates/rebases or rejects stale transactions; composition-sensitive transactions are never silently replayed across incompatible state.
3. Document-changing steps apply once to the shared document/history authority.
4. Each view's selection maps independently through the step mapping.
5. The originating view receives its requested valid selection; the other retains its mapped selection.
6. Shared semantic plugins update once; view-local plugins update per view.
7. Both views receive the resulting document by the next rendered frame under normal load.
8. A revisioned, coalescible change batch is sent to projection/recovery/search/spellcheck services.
9. Undo/redo uses shared document history and maps both selections.

S55 must prove this topology with real IME and accessibility-sensitive behavior before S60 production implementation.

## 12. Canonical projection and recovery channel

The editor publishes ParchMint-owned revisioned change batches:

```text
EditorChangeBatch
├── document_id
├── base_revision
├── through_revision
├── changed_block_ids
├── neutral text/structure deltas or adapter payload
├── title observation
└── annotation changes
```

The projection implementation selected by S55 must provide:

```text
CanonicalProjection
├── document_id
├── editor_revision
├── html_bytes or deterministic resource stream
├── changed_block_text[]
├── observed_content_title
├── word_count
└── annotation_projection/version
```

Allowed implementation strategies:

1. A Web Worker with a ProseMirror document/model mirror.
2. A Web Worker or Rust worker with a neutral block/delta mirror.
3. Bounded incremental/idle projection without a persistent mirror.

Selection criteria include one/two-view correctness, input-to-frame, projection latency, queue depth, initial synchronization, memory, worker failure recovery, canonical fidelity, and platform support.

Regardless of strategy:

- The UI thread never performs an unbounded full serialization during input.
- Queues are bounded and coalesce superseded revisions.
- A lagging/crashed projection target resynchronizes from a revisioned snapshot.
- Save acknowledgements name the projected editor revision.
- Recovery records can replay to the same canonical result.
- Projection failure does not corrupt or become interactive editor authority.

## 13. Spellcheck architecture

`SpellcheckService` is a ParchMint-owned asynchronous port:

```rust
trait SpellcheckService {
    fn available_languages(&self) -> Result<Vec<SpellcheckLanguage>>;
    fn check(&self, request: SpellcheckRequest, sink: SpellcheckBatchSink) -> Result<SpellcheckHandle>;
    fn suggest(&self, request: SuggestionRequest) -> Result<Vec<SpellingSuggestion>>;
    fn cancel(&self, handle: SpellcheckHandle);
    fn reload_project_dictionary(&self, project: ProjectId, revision: RevisionId) -> Result<()>;
    fn reload_global_dictionary(&self, revision: RevisionId) -> Result<()>;
}
```

Requests contain language, document/block revisions, text ranges, and generation ID. Results contain token range, normalized token, rule/category, and confidence/ranking metadata owned by ParchMint.

### 13.1 Ownership

- Project-default language: `project.toml`; project undo/save/history.
- Project dictionary: `dictionary.txt`; project undo/save/history.
- Global dictionary: application preferences; application-level undo is not required, but changes are immediately persistent and reversible through settings actions.
- Misspelling/suggestion results: disposable view/editor state.
- Language packages: bundled/offline implementation resources, not project data.

### 13.2 Runtime behavior

- Check only visible/recently changed blocks plus bounded lookaround.
- Cancel stale generations and reject results for mismatched revisions.
- Do not block typing, save, or close.
- Use one application-owned spelling context-menu model across platforms.
- Disable native webview spellcheck if it duplicates or conflicts with selected decorations/menus.
- Suggestions are token-level spelling suggestions, not grammar or semantic writing assistance.

S65 freezes the v1 language inventory, selects/proves the engine, audits licenses/package size, and validates all three webviews before feature waves.

## 14. Save and recovery

Revisions:

- `editor_revision`: monotonically increasing per open document session.
- `project_revision`: monotonically increasing for authored project changes.
- `durable_revision`: latest canonical state successfully written.
- `checkpoint_id`: identity of a completed durable checkpoint.

Save flow:

```text
Editor/project command
  → immediate interactive state update
  → revisioned projection/recovery batch
  → SaveCoordinator queues one project transaction
  → serialize dirty manifest/style/dictionary/annotation/document resources
  → write temporary files
  → flush and atomically replace
  → HistoryStore checkpoint
  → SearchIndex and derived word-count update
  → acknowledge durable revisions to UI
```

Search/word-count update failure does not invalidate canonical save; derived state is marked stale. History checkpoint failure leaves current canonical files valid but surfaces an error and retains enough recovery/context to retry.

A save transaction records target/expected revisions, files to create/replace/delete, hashes, temporary paths, and commit stage. Restart finishes or rolls back incomplete filesystem stages before editing.

Backpressure:

- One canonical writer per project.
- New changes coalesce behind current save.
- Explicit Save raises priority without creating another writer.
- Projection, recovery, history, search, and spellcheck use bounded queues.
- Maintenance yields to active input.

## 15. Replaceable application ports

### 15.1 Project repository

```rust
trait ProjectRepository {
    fn create(&self, request: CreateProject) -> Result<ProjectSnapshot>;
    fn open(&self, path: &Path) -> Result<ProjectSnapshot>;
    fn load_document(&self, id: DocumentId) -> Result<CanonicalDocument>;
    fn stage_write(&self, transaction: SaveTransaction) -> Result<StagedWrite>;
    fn commit_write(&self, staged: StagedWrite) -> Result<DurableRevision>;
    fn validate(&self) -> Result<ProjectIntegrityReport>;
}
```

### 15.2 Canonical codec

Parsing, serialization, migration, and validation are separate from filesystem access.

### 15.3 History store

```rust
trait HistoryStore {
    fn initialize(&self, project: &ProjectPath) -> Result<()>;
    fn checkpoint(&self, input: CheckpointInput) -> Result<CheckpointId>;
    fn list(&self, query: HistoryPageQuery) -> Result<HistoryPage>;
    fn preview(&self, checkpoint: CheckpointId) -> Result<SnapshotPreview>;
    fn restore(&self, checkpoint: CheckpointId) -> Result<RestorePlan>;
    fn verify(&self) -> Result<HistoryIntegrityReport>;
    fn maintain(&self, budget: MaintenanceBudget) -> Result<MaintenanceReport>;
}
```

Preview and restore operate on the entire canonical project state. Only the git2 adapter imports `git2`.

### 15.4 Search index

```rust
trait SearchIndex {
    fn open_or_create(&self, project: ProjectId) -> Result<()>;
    fn replace_document(&self, projection: SearchDocumentProjection) -> Result<()>;
    fn delete_document(&self, id: DocumentId, revision: RevisionId) -> Result<()>;
    fn query(&self, query: SearchQuery, sink: SearchBatchSink) -> Result<SearchHandle>;
    fn cancel(&self, handle: SearchHandle);
    fn verify(&self) -> Result<SearchIntegrityReport>;
    fn rebuild(&self, source: &dyn SearchProjectionSource) -> Result<RebuildReport>;
}
```

v1 queries the entire project; section/subtree scope controls are not part of the v1 contract surface. Only the SQLite adapter imports `rusqlite`.

### 15.5 Recovery journal

Recovery is versioned and adapter-specific; completed canonical content is unaffected by editor replacement.

### 15.6 Exporter

The neutral export plan contains ordered semantic blocks, styles, and title decisions rather than editor nodes.

### 15.7 Platform services

Ports cover dialogs, menus, clipboard extensions, reveal/open external file, project locking, paths/directories, notifications, window state, single-instance routing, system appearance, and application preferences.

## 16. History architecture

Composition:

```toml
git2 = { version = "=0.21.0", default-features = false, features = ["vendored-libgit2"] }
```

Controls:

- Resolve and assert the accepted vendored libgit2 composition in the real application lockfile.
- Set static zlib/reproducibility guards.
- Prevent environment variables from silently selecting a system library.
- Enable no HTTPS/SSH features in v1.
- Include required native notices.

Policy:

- Project root is repository root.
- One linear app-managed `main`.
- Normalize LF and disable autocrlf, executable-mode tracking, and symlink tracking.
- Reject traversal/absolute paths.
- Use bounded unsorted revwalk paging with opaque cursors.
- Named snapshots may use empty commits.
- Restore is additive and creates a new commit.
- Missing/corrupt history never makes current canonical files unreadable.
- Project dictionary is included; application appearance/global dictionary/workspace state are excluded.

Maintenance runs in a low-priority worker, is cancellable/yielding, verifies new packs, and never prunes reachable checkpoints.

## 17. Search architecture

Composition:

```toml
rusqlite = { version = "=0.40.1", default-features = false, features = ["bundled"] }
```

- Assert FTS5 at worker startup.
- Use one dedicated connection on a named worker.
- Index stable project/document/block/revision IDs.
- Index body, title, Synopsis, and metadata with field-aware weighting.
- Use verified external-content consistency logic.
- Tokenizer baseline: `unicode61 remove_diacritics 2`.
- Escape/quote user text; allow-list fields.
- Post-filter case-sensitive and Unicode whole-word semantics.
- Stream batches and support generation cancellation.
- Revalidate revision/text before navigation or replacement.
- Rebuild deterministically after deletion/corruption/schema change.

The index is disposable and does not migrate when the backend changes.

## 18. Comments and anchors

Canonical comments are stored in `annotations/<document-id>.json`.

```text
TextAnchor
├── block_id
├── start_offset
├── end_offset
├── quotation
├── context_before
├── context_after
└── anchor_revision
```

The editor maps anchors through transaction mappings. Canonical projection emits updated anchors. Reattachment after transformation is conservative; ambiguity creates an orphan.

Anchor geometry comes from the focused view and is view-local. Comment creation uses editor context-menu or Comments-panel commands; transient geometry is never persisted.

## 19. Title synchronization

The application stores `display_title` and `last_observed_content_title`.

When the first reserved title block changes:

```text
if display_title == previous_content_title:
    display_title = new_content_title
last_observed_content_title = new_content_title
```

Tree/Card rename changes only `display_title`. This rule is adapter-independent and tested outside the GUI.

## 20. Appearance and design-token architecture

Application preferences store:

```text
appearance_mode = system | light | dark
```

The platform adapter emits system-appearance changes. A resolved appearance (`light` or `dark`) is distributed to every window. Appearance updates never cross project command/save/history boundaries.

Token pipeline:

```text
Approved Penpot token JSON
  → deterministic validator/normalizer
  → semantic token model
  → generated Light/Dark CSS custom properties
  → generated TypeScript token metadata
  → components
```

Components consume semantic tokens such as application/sidebar/inspector/document/elevated surfaces, text roles, borders, focus, selection, comments, search, warnings, errors, and save states. They do not branch on arbitrary hard-coded colors.

Project prose styles remain a separate CSS/style projection. The app may adapt editor-only background/foreground presentation while preserving authored semantic style values and export output.

Generated token files are committed and never edited manually. CI regenerates and fails on drift.

## 21. Penpot integration

The approved handoff contains `.penpot`, Light/Dark token JSON, assets, deterministic references, manifest, component/interaction maps, focus/keyboard map, appearance matrix, platform variants, known deviations, and checksums.

The first UI deliverable is `docs/design/reconciliation/<version>/`. Production code must satisfy semantic HTML, accessibility, maintainability, component reuse, application state, and both themes; generated design code is never accepted solely for visual resemblance.

## 22. Cross-platform architecture

### Windows

- WebView2.
- Native menus/dialogs/clipboard/file locking/high-DPI validation.
- Narrator/NVDA.
- MSI or NSIS selected before beta.
- No installed Git/SQLite dependency.

### macOS

- WKWebView.
- Native menu placement, dialogs, clipboard, VoiceOver, signing/notarization.
- `.app` plus `.dmg` baseline.

### Linux

- WebKitGTK 4.1 runtime closure.
- X11 and Wayland input/clipboard validation on supported distributions.
- `.deb` required initially.
- AppImage deferred until its runtime path is proven.
- Orca/AT-SPI.

Product commands use semantic names and platform accelerators are assigned centrally.

## 23. Security and diagnostics

Security:

- Bundled local frontend only.
- Strict CSP; no `eval`; least-privilege Tauri capabilities.
- Validate and normalize project paths.
- Sanitize pasted HTML.
- No Git network features.
- No arbitrary SQL from user input.
- Offline spellcheck only; prose is not sent to a service.
- Dependency locks, SBOM, advisory/provenance scans, license inventory, and native notices.

Privacy-preserving diagnostics include save/projection/history/search/spellcheck durations, queue depths, open-to-editable and input-to-frame metrics, memory where available, and structured errors with revision IDs. Diagnostic bundles redact project prose unless explicitly included.

## 24. Testing architecture

### Headless core

`parchmint-core-cli` creates/opens/validates/migrates projects; applies project commands/undo; saves/recovers; operates history/search; validates dictionaries; and exports HTML.

### Shared contract suites

Contracts cover ProjectRepository, CanonicalCodec, RecoveryJournal, HistoryStore, SearchIndex, SpellcheckService, Exporter, EditorAdapter, and PlatformService where feasible.

### Editor fixtures

Cover blocks/marks, titles, comments, tabs, Unicode/IME text, paste, shared two-view operations, selection mapping, shared undo, projection/recovery, spellcheck decorations, and 250,000 words.

### Visual validation

Compare deterministic application screenshots to approved Light and Dark Penpot references. Accessibility/native-control correctness may justify documented differences.

## 25. Replacement scenarios

### GUI/editor

Retain canonical format, Rust application services, port implementations, contracts, design tokens, and fixtures. Rewrite widgets, windows, input/IME/accessibility integration, rendering, and frontend state.

### History/search/spellcheck

Implement the corresponding port and shared contract suite. History migration uses logical checkpoints; search rebuilds from canonical state; spellcheck dictionaries remain in ParchMint-owned formats.

### Save/load/recovery

Implement repository/codec/writer or recovery contracts. Canonical format changes require explicit migrations and G20 approval.

### Export

Implement `Exporter` over the neutral export plan.

## 26. Architecture-change policy

The selected architecture remains current unless implementation evidence shows a mandatory requirement cannot be met. In that case, stop at G20 with reproducible evidence and bounded alternatives.

After approval, update this architecture, the product specification, implementation plan, acceptance plan, and relevant contracts directly. Do not add a permanent ADR or preserve superseded architecture as a competing source of truth.
