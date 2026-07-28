# ParchMint v1 Final Architecture

**Status:** Final v1 architecture baseline  
**Version:** 1.0  
**Date:** 2026-07-28  
**Primary audience:** Implementation agents, architecture reviewers, maintainers

## 1. Decision summary

ParchMint v1 will use:

- **Tauri 2.11.5** as the cross-platform desktop shell.
- **TypeScript + React** for the application UI.
- **ProseMirror** for semantic rich-text editing, exact-locked initially to the validated V02 dependency graph.
- **Rust** for project/domain services, persistence coordination, recovery, history, search, export, and platform-independent application logic.
- **Restricted deterministic HTML5, TOML, CSS, and JSON** as canonical authored data.
- **`git2 =0.21.0` with vendored libgit2** as the initial `HistoryStore` adapter.
- **`rusqlite =0.40.1` with bundled SQLite FTS5** as the initial `SearchIndex` adapter.
- **A ports-and-adapters modular architecture** that makes each selected technology replaceable without changing the canonical project model.

The GPUI + `text-document` + `text-typeset` route is rejected for v1. No third frontend comparison is planned.

The V02-R report did not pass its strict native-runtime gates. Selecting Tauri/ProseMirror is therefore a deliberate product decision, not a claim that every performance/accessibility risk has already been proven. The implementation must validate those risks early and must not hide failure by adding a user-visible large-document mode. All supported documents receive the same functionality.

## 2. Architectural drivers

1. Cross-platform parity from the first release.
2. 10–20 million words per project without eager loading.
3. Documents up to approximately 250,000 words with one consistent feature set.
4. Low-latency typing and nonblocking autosave/history/search.
5. Two views of one document with independent view state and shared content/undo.
6. Open, Git-friendly, deterministic canonical data.
7. Complete checkpoint history and recoverability.
8. Penpot-first design handoff with traceable implementation.
9. Replaceability of GUI/editor, history, search, save/load, recovery, and exporters.
10. Testability without initializing the GUI.

## 3. Architectural principles

### 3.1 Canonical data and derived data are separate

Canonical authored state consists only of versioned HTML/TOML/CSS/JSON files. Git objects, SQLite indexes, editor JSON, recovery logs, and workspace state are implementation or derived data.

### 3.2 Depend on ParchMint-owned contracts

Domain/application modules depend on interfaces defined by ParchMint, never directly on Tauri, React, ProseMirror, `git2`, `rusqlite`, or operating-system APIs.

### 3.3 One authority per state category

At any moment, each state category has one authority:

- Project hierarchy, metadata definitions, styles, export settings, and persisted authored metadata: Rust application/domain core.
- Active rich-text editor state: the document’s `EditorSession` in the ProseMirror adapter.
- Per-view selection, scroll, focus, and local-search state: individual `EditorViewSession`s.
- Durable current files: canonical project store after save acknowledgement.
- Historical checkpoints: `HistoryStore`.
- Search results: disposable `SearchIndex`, revalidated against current revisions.
- Pure layout/hover/animation state: frontend UI store.

The Rust mirror of an actively edited document is a revisioned persistence/search mirror, not a competing interactive editor authority.

### 3.4 No synchronous persistence from input

A keypress must not wait for Rust IPC, disk, Git, SQLite, export, or canonical serialization before painting. Editor transactions are applied locally and then propagated asynchronously through revisioned channels.

### 3.5 Coarse, durable replaceability beats generic widgets

ParchMint will not build a generic cross-framework widget toolkit. Replaceability occurs at stable service and editor-adapter boundaries. Replacing the GUI still requires rewriting widgets and editor rendering, but must retain project format, domain logic, persistence, history, search, export, and headless tests.

### 3.6 Cross-platform behavior is proven natively

Package creation and headless tests are not substitutes for native runtime validation of IME, clipboard, accessibility, scaling, filesystem durability, or packaging.

## 4. System context

```text
┌──────────────────────────────── Desktop process ───────────────────────────────┐
│                                                                                │
│  Tauri webview frontend                                                        │
│  ┌──────────────────────────────────────────────────────────────────────────┐  │
│  │ React shell                                                              │  │
│  │ ├── Launcher / workspace / Explorer / Cards / Inspector / History        │  │
│  │ ├── Design-token CSS and exported SVG assets                             │  │
│  │ ├── ProseMirror editor adapter                                           │  │
│  │ ├── EditorSession + two EditorViewSessions                               │  │
│  │ └── Editor worker mirror / canonical projection                          │  │
│  └────────────────────────────────┬─────────────────────────────────────────┘  │
│                                   │ versioned typed commands/events             │
│  Rust application core            ▼                                             │
│  ┌──────────────────────────────────────────────────────────────────────────┐  │
│  │ Domain + application services                                            │  │
│  │ ├── ProjectService / HierarchyService / MetadataService                  │  │
│  │ ├── SaveCoordinator / RecoveryCoordinator                                │  │
│  │ ├── HistoryService → HistoryStore                                        │  │
│  │ ├── SearchService → SearchIndex                                           │  │
│  │ ├── ExportService → Exporter                                              │  │
│  │ └── PlatformService / ProjectLock                                         │  │
│  └─────────────────────┬──────────────────┬──────────────────┬───────────────┘  │
│                        │                  │                  │                  │
│                 ProjectRepository   GitHistoryStore   SqliteFtsIndex           │
└────────────────────────┼──────────────────┼──────────────────┼──────────────────┘
                         ▼                  ▼                  ▼
                Canonical project files   .git/       .parchmint/cache/search.db
```

## 5. Repository and module layout

Recommended monorepo structure:

```text
ParchMint/
├── apps/
│   ├── desktop-ui/                      # React/TypeScript frontend
│   │   ├── src/app/
│   │   ├── src/components/
│   │   ├── src/features/
│   │   ├── src/editor/
│   │   ├── src/design-tokens/
│   │   └── src/generated-contracts/
│   └── desktop-tauri/                   # Tauri configuration and Rust adapter
│       └── src-tauri/
├── packages/
│   ├── ui-contract/                     # JSON Schemas and generated TS/Rust types
│   ├── editor-contract/                 # Editor adapter interfaces and fixtures
│   ├── editor-prosemirror/              # All ProseMirror-specific code
│   ├── design-system/                   # Tokens, CSS variables, shared web components
│   └── test-fixtures/                   # Canonical documents and operation traces
├── crates/
│   ├── parchmint-domain/                # Pure entities, invariants, IDs
│   ├── parchmint-application/           # Use cases and orchestration
│   ├── parchmint-project-format/        # TOML/HTML/CSS/JSON schemas and migrations
│   ├── parchmint-project-repository/    # ProjectRepository port
│   ├── parchmint-project-fs/            # Filesystem implementation
│   ├── parchmint-save/                  # SaveCoordinator and atomic transaction plan
│   ├── parchmint-recovery-api/          # RecoveryJournal port
│   ├── parchmint-recovery-fs/           # Initial recovery implementation
│   ├── parchmint-history-api/            # HistoryStore port
│   ├── parchmint-history-git2/           # Selected git2 implementation
│   ├── parchmint-search-api/             # SearchIndex port and query/result types
│   ├── parchmint-search-sqlite/          # Selected FTS5 implementation
│   ├── parchmint-export-api/             # Exporter contract and neutral export model
│   ├── parchmint-export-html/            # v1 self-contained HTML exporter
│   ├── parchmint-platform-api/           # Dialog/menu/clipboard/path/lock abstractions
│   ├── parchmint-tauri-adapter/          # Commands/events/platform implementations
│   ├── parchmint-core-cli/               # Headless validate/search/history/export tool
│   └── parchmint-test-support/           # Fault injection, clocks, IDs, fixtures
├── contracts/                            # Versioned JSON Schemas
├── design/
│   ├── source/                            # Approved .penpot and token exports
│   ├── handoff/<version>/                 # Frozen implementation handoff
│   └── generated/                         # Generated token CSS and asset manifests
├── docs/
├── scripts/
└── tests/
```

Only adapters import external framework/backend types. Public signatures in domain/application ports use ParchMint-owned types.

## 6. Contract and type strategy

### 6.1 Versioned schemas

The source of truth for Tauri IPC payloads and design manifests is versioned JSON Schema under `contracts/`. Rust and TypeScript types must be generated from or validated against the same schemas; they must not be independently maintained by hand.

Every command/event envelope includes:

```text
schema_version
request_id or event_id
project_id
expected_project_revision where applicable
payload
```

### 6.2 Compatibility

- Additive fields are preferred.
- Removed/renamed fields require a schema-version bump and migration.
- Unknown noncritical fields are ignored safely.
- Contract tests serialize in Rust, deserialize in TypeScript, and vice versa.

### 6.3 Command categories

- Project/hierarchy commands.
- Metadata/style commands.
- Save/recovery commands.
- History/search/export requests.
- Workspace/platform commands.
- Editor snapshot/change notifications.

High-frequency keystrokes do not cross Tauri IPC individually.

## 7. Domain model

Core entities use stable opaque IDs:

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
```

Core authored aggregate:

```text
Project
├── project metadata
├── fixed Manuscript root
├── fixed Research root
├── ordered Node graph
├── StyleCatalog
├── MetadataSchema
├── node metadata values
├── export settings
└── annotation registry/index
```

Group and Document invariants are enforced in `parchmint-domain`; the frontend may not create invalid trees and rely on later repair.

## 8. Canonical project format

Illustrative layout:

```text
my-novel/
├── .git/
├── project.toml
├── styles.css
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
    │   └── search.sqlite
    └── workspace.json
```

### 8.1 `project.toml`

Contains:

- Format/schema version.
- Project ID, display title, optional author, language.
- Ordered node records with stable IDs and relative paths.
- Display titles, section, type, parent/order.
- Synopsis and metadata values or references to partitioned metadata files if later required.
- Metadata-field definitions.
- Style semantic metadata not representable in CSS alone.
- Export defaults/overrides.
- Deletion tombstones.

The manifest is authoritative for order and identity. Filesystem directory enumeration is never order.

### 8.2 Restricted HTML profile

Canonical documents use standard HTML plus `data-pm-*` attributes where necessary.

Allowed block concepts include:

- Paragraph.
- Document Title and heading levels.
- Block quote.
- Ordered/unordered list and list item.
- Verse/line-preserving block.
- Atomic scene break.
- Atomic page break.

Allowed inline concepts include:

- Text.
- Strong/emphasis/underline/strikethrough.
- Small caps.
- Superscript/subscript.
- Link.

Every addressable block has a stable `data-pm-id`. Semantic style references use stable IDs, not visible style names. Scripts, event handlers, arbitrary inline styles, unsupported elements, and remote embeds are forbidden.

### 8.3 Deterministic serialization

- UTF-8 and LF.
- Stable attribute order.
- Stable class and ID generation.
- One deterministic representation for equivalent documents.
- No formatter-driven paragraph rewrapping.
- No rewriting unchanged documents.
- Sanitize before canonicalization.
- Golden byte-equality tests.

### 8.4 Migrations

`parchmint-project-format` owns explicit `N → N+1` migrations. A migration:

1. Takes a pre-migration checkpoint when history is available.
2. Writes through the atomic save pipeline.
3. Preserves stable IDs where possible.
4. Is tested on real fixture projects from every prior supported version.
5. Never relies on SQLite or frontend state.

## 9. Frontend and editor architecture

### 9.1 React shell

React implements the application shell and consumes approved Penpot tokens/assets. It is divided by feature, not by visual page alone.

Framework-specific frontend state remains in `apps/desktop-ui`. Domain decisions are requested through typed application commands.

CSS strategy:

- Penpot token JSON is transformed into versioned CSS custom properties.
- Component styles use CSS Modules or equally scoped static CSS.
- Avoid runtime CSS-in-JS as the baseline.
- Native platform variations use explicit token/theme or component variants, not scattered user-agent checks.

### 9.2 ProseMirror isolation

All ProseMirror imports live in `packages/editor-prosemirror`. Other frontend code sees only the ParchMint editor contract.

ProseMirror JSON, plugin keys, selections, steps, and DOM nodes must not cross into Rust domain APIs or canonical files.

### 9.3 `EditorAdapter` contract

Conceptual TypeScript interface:

```ts
interface EditorAdapter {
  openDocument(input: CanonicalDocumentLoad): Promise<EditorSessionHandle>;
  attachView(session: EditorSessionHandle, view: ViewId, host: HTMLElement): void;
  detachView(session: EditorSessionHandle, view: ViewId): ViewState;
  executeCommand(session: EditorSessionHandle, view: ViewId, command: EditorCommand): void;
  getSelection(session: EditorSessionHandle, view: ViewId): EditorSelection;
  getSelectionGeometry(session: EditorSessionHandle, view: ViewId): SelectionGeometry | null;
  setStyleCatalog(session: EditorSessionHandle, styles: StyleCatalogProjection): void;
  setSearchDecorations(session: EditorSessionHandle, view: ViewId, matches: SearchDecoration[]): void;
  requestCanonicalProjection(session: EditorSessionHandle, throughRevision: number): Promise<CanonicalProjection>;
  subscribe(listener: EditorEventListener): Unsubscribe;
  closeDocument(session: EditorSessionHandle): Promise<void>;
  capabilities(): EditorCapabilities;
}
```

The implementation may use more granular APIs internally. Tests target externally observable behavior.

### 9.4 One document, two views

ProseMirror normally associates selection/history with an `EditorState`; ParchMint therefore needs a session controller rather than two unrelated editors.

Recommended design:

```text
SharedEditorSession
├── canonical ProseMirror doc state
├── shared transaction log/history controller
├── style/plugin configuration
├── comments/anchor mapper
├── editor revision
├── worker mirror
└── ViewSession A / ViewSession B
    ├── independent selection
    ├── scroll anchor
    ├── local search
    ├── focus
    └── mounted EditorView
```

Transaction dispatch:

1. The focused view produces a ProseMirror transaction.
2. `SharedEditorSession` applies document-changing steps once to the shared document/history authority.
3. Each view’s selection is mapped independently through the step mapping.
4. Both views receive the resulting document state; the originating view receives its requested selection, the other retains its mapped selection.
5. The toolbar and Inspector target the focused `ViewSession`.
6. Undo/redo operates on the shared history and then maps both selections.
7. A revisioned change notification is sent to the worker mirror and Rust services.

Do not use two independent ProseMirror history plugins and attempt to reconcile them.

### 9.5 Editor worker mirror

A Web Worker inside `editor-prosemirror` maintains a transient mirror sufficient to:

- Project editor state into deterministic canonical HTML.
- Extract changed block text, title, and word-count deltas.
- Batch recovery records.
- Avoid expensive serialization on the webview main thread.

The worker may use exact-locked ProseMirror packages as implementation data. Its output to Rust is ParchMint-owned:

```text
CanonicalProjection
├── document_id
├── editor_revision
├── html_bytes
├── block_text_projection[]
├── observed_content_title
├── word_count
└── annotation_projection/version
```

If worker APIs or webview limitations make a mirror impractical, an ADR may replace it with a Rust-side neutral-delta mirror, but the UI-thread and canonical-boundary requirements remain.

### 9.6 No size-based behavior

The adapter must not expose a `large_document_mode` capability in v1. The same schema, plugins, comments, two-view behavior, search, and formatting apply throughout the supported range.

Transparent optimizations are allowed:

- Incremental decorations.
- Viewport-aware spellchecking.
- CSS `content-visibility` when correctness is preserved.
- Lazy offscreen measurements.
- Worker-based projection.
- Plugin throttling/debouncing.
- Future internal bounded rendering that preserves full behavior.

Any optimization that changes selection, accessibility, search, comments, or clipboard semantics requires dedicated compatibility tests.

## 10. Application services and replaceable ports

### 10.1 Project repository

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

`parchmint-project-fs` is the v1 implementation. A future packaged/cloud store can implement the port without changing domain use cases.

### 10.2 Canonical codec

```rust
trait CanonicalCodec {
    fn parse_project(...);
    fn serialize_project(...);
    fn parse_document(...);
    fn serialize_document(...);
    fn migrate(...);
    fn validate(...);
}
```

The codec is separate from filesystem access so format tests are pure and alternate stores reuse it.

### 10.3 History store

```rust
trait HistoryStore {
    fn initialize(&self, project: &ProjectPath) -> Result<()>;
    fn checkpoint(&self, input: CheckpointInput) -> Result<CheckpointId>;
    fn list(&self, query: HistoryPageQuery) -> Result<HistoryPage>;
    fn preview(&self, checkpoint: CheckpointId, scope: RestoreScope) -> Result<SnapshotPreview>;
    fn restore(&self, checkpoint: CheckpointId, scope: RestoreScope) -> Result<RestorePlan>;
    fn verify(&self) -> Result<HistoryIntegrityReport>;
    fn maintain(&self, budget: MaintenanceBudget) -> Result<MaintenanceReport>;
}
```

Only `parchmint-history-git2` imports `git2`.

### 10.4 Search index

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

Only `parchmint-search-sqlite` imports `rusqlite`.

### 10.5 Recovery journal

```rust
trait RecoveryJournal {
    fn begin_session(&self, document: DocumentId, base: DurableRevision) -> Result<RecoverySession>;
    fn append(&self, session: RecoverySession, record: RecoveryRecord) -> Result<()>;
    fn load_pending(&self, project: ProjectId) -> Result<Vec<PendingRecovery>>;
    fn mark_durable(&self, session: RecoverySession, through: EditorRevision) -> Result<()>;
    fn discard_completed(&self, session: RecoverySession) -> Result<()>;
}
```

Recovery records may be ProseMirror-adapter-specific, but they are versioned and isolated under this port. A GUI/editor replacement can add a new recovery implementation without migrating canonical files.

### 10.6 Exporter

```rust
trait Exporter {
    fn id(&self) -> ExporterId;
    fn capabilities(&self) -> ExportCapabilities;
    fn plan(&self, project: &ProjectSnapshot, options: &ExportOptions) -> Result<ExportPlan>;
    fn render(&self, plan: ExportPlan, output: &mut dyn Write) -> Result<ExportReport>;
}
```

The neutral `ExportPlan` contains ordered semantic blocks, styles, and title decisions rather than HTML editor nodes.

### 10.7 Platform services

Ports cover:

- Native dialogs.
- Menus and command labels.
- Clipboard extensions outside editor-native paths.
- Reveal/open external file.
- Project locking.
- Paths and directories.
- Notifications.
- Window state.

The web frontend requests semantic actions; platform adapters implement them.

## 11. Save and recovery architecture

### 11.1 Revisions

- `editor_revision`: monotonically increasing per open document session.
- `project_revision`: monotonically increasing for authored project changes.
- `durable_revision`: latest canonical state successfully written.
- `checkpoint_id`: history identity for a completed durable state.

UI Saved status always names or internally tracks the acknowledged durable revision; it is never a timer-based guess.

### 11.2 Save flow

```text
Editor transaction
  → immediate ProseMirror render
  → worker/recovery batch
  → debounced canonical projection
  → SaveCoordinator queues project transaction
  → serialize dirty manifest/style/annotation/document resources
  → write temporary files
  → flush and atomically replace
  → HistoryStore checkpoint
  → SearchIndex replacement/update
  → acknowledge durable revision to UI
```

Search update failure does not invalidate a canonical save; the index is marked stale and rebuilt/retried. History checkpoint failure leaves current canonical files valid but must surface an error and retain recovery/context sufficient to retry.

### 11.3 Multi-file transaction descriptor

A save transaction records:

- Target project revision.
- Expected previous durable revision.
- Files to create/replace/delete.
- Content hashes.
- Temporary paths.
- Commit stage.

On restart, the recovery coordinator finishes or rolls back incomplete filesystem stages before opening the project for editing.

### 11.4 Durability

Implement and test platform-specific flush/rename semantics. The architecture must not assume identical `fsync`/rename behavior across Windows, macOS, and Linux.

### 11.5 Backpressure

- At most one canonical writer per project.
- New changes coalesce behind the current save.
- Explicit Save raises priority but does not create concurrent writers.
- Search and history receive bounded queues.
- Maintenance yields when editor activity increases.

## 12. History architecture: selected `git2`

Composition:

```toml
git2 = { version = "=0.21.0", default-features = false, features = ["vendored-libgit2"] }
```

Lock and release controls:

- Lock `libgit2-sys 0.18.7+1.9.6` unless an approved upgrade ADR changes it.
- Set `LIBZ_SYS_STATIC=1` for reproducible release builds.
- Prevent `LIBGIT2_NO_VENDOR` from silently selecting a system library.
- Enable no HTTPS or SSH features in v1.
- Include libgit2 linking exception and native notices in release artifacts.

Repository policy:

- Project root is repository root.
- One linear app-managed `main`.
- Normalize LF, disable autocrlf, executable-mode tracking, and symlink tracking.
- Reject absolute/traversal paths.
- Use bounded unsorted revwalk paging with opaque cursors.
- Named snapshots may use empty commits.
- Restore is additive and creates a new commit.
- Missing/corrupt history never makes current canonical files unreadable.

Fault recovery:

- Interrupted ref transactions may leave `.git/refs/heads/main.lock`.
- Remove it only after restart, verified exclusive project ownership, and repository validation.
- Verify the previous completed `main` before proceeding.

Maintenance:

- ParchMint schedules pack creation and reachable verification.
- Maintenance runs in a dedicated low-priority worker.
- Do not prune reachable checkpoints.
- Verify newly created packs before deleting redundant loose objects.
- Long maintenance is cancellable/yielding and never blocks close/save acknowledgements.

Backend replacement:

A future backend must implement `HistoryStore`. Migration occurs through exported logical checkpoint streams and canonical snapshots, not by exposing Git object IDs to application code.

## 13. Search architecture: selected SQLite FTS5

Composition:

```toml
rusqlite = { version = "=0.40.1", default-features = false, features = ["bundled"] }
```

- Assert FTS5 at worker startup by creating/validating the actual table.
- Use one dedicated SQLite connection on a named worker.
- Never expose the connection across threads or to UI code.
- Index stable project/document/block/revision IDs.
- Index body, title, Synopsis, and metadata with field-aware weighting.
- Use external-content FTS5 tables and triggers or equally verified consistency logic.
- Tokenizer baseline: `unicode61 remove_diacritics 2`.
- User text is escaped/quoted; field names come only from allow lists.
- Case-sensitive and Unicode whole-word semantics are post-filtered against stored field text.
- Stream result batches and support generation-based cancellation.
- Revalidate result revision/text before navigation or replacement.
- Rebuild deterministically after deletion/corruption/schema change.

Backend replacement:

SQLite files are derived state. A different `SearchIndex` implementation requires no project migration; it rebuilds from canonical projections.

## 14. Comments and anchors

Canonical comments are stored in `annotations/<document-id>.json`.

Anchor model:

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

The ProseMirror adapter maps anchors through transaction mappings. The worker/canonical projection emits updated anchors as needed. On load after canonical/external transformations, reattachment is conservative; ambiguity produces an orphan.

Comment UI geometry comes from the focused EditorView. The selection-end Add Comment affordance must be view-local and must not be persisted.

## 15. Title synchronization

The application stores:

```text
display_title
last_observed_content_title
```

When the first reserved title block changes:

```text
if display_title == previous_content_title:
    display_title = new_content_title
last_observed_content_title = new_content_title
```

Tree/Card renames update only `display_title`. This rule belongs in a shared application/editor command service and is covered by adapter-independent tests.

## 16. UI and Penpot integration

### 16.1 Design artifact intake

The approved handoff contains:

- `.penpot` source export.
- Token JSON.
- SVG assets.
- Reference PNGs.
- `design-manifest.yaml`.
- Component and interaction maps.

### 16.2 Token pipeline

```text
Penpot token JSON
  → validate/import script
  → normalized design-token JSON
  → generated CSS custom properties
  → component CSS modules
```

Generated token output is committed; the source token export is also committed for traceability. Manual edits to generated CSS are prohibited.

### 16.3 Component mapping

Penpot component names and IDs map to implementation component IDs in `design-component-map.yaml`. Refactoring code names is allowed only if the mapping is updated.

### 16.4 Generated code policy

Penpot MCP-generated HTML/CSS may be used to understand layout or bootstrap a disposable prototype. Production code must satisfy semantic HTML, accessibility, maintainability, component reuse, and application state requirements. Generated output is never accepted merely because it visually resembles the design.

## 17. Cross-platform architecture

### Windows

- WebView2 runtime.
- Native menus, dialogs, clipboard, file locking, and high-DPI validation.
- MSI or NSIS package selected before beta.
- No dependency on installed Git or SQLite.

### macOS

- WKWebView.
- Native menu placement/shortcuts, dialogs, clipboard, VoiceOver, signing and notarization.
- `.app` plus `.dmg` distribution baseline.

### Linux

- WebKitGTK 4.1 runtime closure.
- Native X11 and Wayland input/clipboard validation on supported distributions.
- `.deb` is the initial required package. AppImage is deferred until its GLib/GVFS/EGL runtime path is proven.
- Orca/AT-SPI validation.

Platform code stays in adapter crates/modules. Product commands use semantic names such as `Save`, `Find`, `AddComment`, and `CloseTab`, with platform-specific accelerators assigned centrally.

## 18. Security architecture

- Local bundled frontend only; no remote origin privileged access.
- Strict CSP; no `eval`; least-privilege Tauri capability files.
- Validate all project paths and normalize before file operations.
- Sanitize pasted/imported HTML.
- No Git network features in v1.
- No arbitrary SQL construction from user input.
- Dependency locks, SBOM, advisory scan, license inventory, and native notices required for release.
- Secrets are not expected in project files; future remote backup requires a separate threat model.

## 19. Observability

Local, privacy-preserving diagnostics include:

- Save queue and duration.
- Canonical projection duration.
- Git checkpoint/maintenance duration.
- Search query/update/rebuild duration.
- Document open-to-editable and input-to-frame metrics in test/instrumented builds.
- Memory by process where platforms permit.
- Structured errors with operation and revision IDs.

Telemetry is disabled by default and is not required for diagnostics. User-exportable diagnostic bundles must redact project prose unless explicitly included.

## 20. Testing architecture

### Headless core

`parchmint-core-cli` must be able to:

- Create/open/validate/migrate projects.
- Parse and serialize documents.
- Apply hierarchy/metadata/style commands.
- Save and recover fixtures.
- Create/list/preview/restore history.
- Build/query/rebuild search.
- Export HTML.

This is the principal proof that core behavior has not leaked into the GUI.

### Adapter contracts

Each replaceable port has a shared contract suite. Alternative implementations must pass it without changing callers.

### Editor fixtures

Shared fixtures cover:

- Every block/mark type.
- Titles.
- Comments/anchors.
- Literal tabs.
- Unicode/IME-representative text.
- Paste normalization.
- Shared two-view transactions and selections.
- 250,000-word document.

### Visual validation

Approved Penpot reference frames are compared to deterministic application screenshots at specified window sizes and states. Pixel differences are reviewed in context; accessibility or native-control correctness may justify documented deviations.

## 21. Replacement scenarios

### Replace the GUI/editor

Retained:

- Canonical format.
- Rust domain/application services.
- History/search/save/recovery/export ports and implementations.
- IPC/application command schemas, possibly adapted to a new transport.
- Penpot design tokens, interaction specs, and acceptance fixtures.

Rewritten:

- Widgets, windows, focus, drag/drop, menus.
- Editor rendering/input/IME/accessibility integration.
- Frontend-specific state and tests.

A replacement editor must load/save the same canonical HTML and pass `editor-contract` fixtures.

### Replace history

Implement `HistoryStore`, pass the contract and longevity/fault suite, and provide logical checkpoint import/export. No caller sees Git hashes.

### Replace search

Implement `SearchIndex`, pass query semantics/scale/rebuild tests, and rebuild from canonical project state. No authored migration is needed.

### Replace save/load

Implement `ProjectRepository`, `CanonicalCodec`, and `AtomicWriter` contracts. Canonical format changes require explicit migrations and ADRs.

### Replace recovery

Implement `RecoveryJournal`; recovery data is versioned and adapter-specific, while completed canonical content is unaffected.

### Add exporters

Implement `Exporter` over the neutral export plan. Editor internals are not available to exporters.

## 22. Implementation sequence

1. Lock contracts, IDs, canonical schemas, and fixtures.
2. Build headless domain/project-format/CLI.
3. Build atomic project repository and recovery skeleton.
4. Integrate selected `git2` and SQLite adapters behind contracts.
5. Import approved Penpot tokens/assets and implement shell components.
6. Implement ProseMirror schema/adapter and canonical round trips.
7. Implement two-view session controller and shared undo.
8. Add metadata/comments/search/history/Cards feature slices.
9. Add export.
10. Complete cross-platform packaging, native input, accessibility, and performance gates.

Detailed work packages are in `05-implementation-plan.md`.

## 23. Architecture governance

An ADR is mandatory for:

- Frontend/editor or major runtime change.
- Backend replacement.
- Canonical schema change.
- New persistent store.
- Change in state ownership.
- New process/thread architecture.
- New dependency with material license/security/package consequences.
- Performance optimization that changes editor semantics.

The selected architecture is final for v1 unless implementation evidence shows a normative requirement cannot be met. In that case, the agent returns evidence and alternatives to the product owner; it does not silently change product behavior or reopen a framework search.
