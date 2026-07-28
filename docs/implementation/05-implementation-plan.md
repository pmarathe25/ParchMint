# ParchMint v1 Implementation Plan

**Status:** Final execution plan  
**Version:** 1.0  
**Date:** 2026-07-28

## 1. Purpose

This plan turns the approved product specification, final architecture, and Penpot handoff into an implementation sequence suitable for coding agents. It emphasizes small vertical slices, early architecture-risk validation, contract tests, and safe parallel work.

Do not build the entire product in one agent run. Each phase has explicit entry conditions, deliverables, and gates.

## 2. Required inputs

Before implementation begins:

- The complete build kit is committed.
- The approved Penpot handoff passes `04-design-artifact-handoff-contract.md`.
- The design version is frozen and recorded.
- Tauri/ProseMirror reference lock hashes are available under `evidence/reference-locks/`.
- Windows, macOS, and Linux CI runners or native test hosts are available.

## 3. Engineering workflow

### 3.1 Mainline and worktrees

Use a protected main branch and short-lived feature branches/worktrees. Parallel agents may work on independent contracts/adapters after Phase 1 freezes their interfaces.

Suggested workstreams:

- `core-domain-format`
- `persistence-recovery`
- `history-git2`
- `search-sqlite`
- `design-system-shell`
- `editor-prosemirror`
- `feature-slices`
- `cross-platform-release`

Do not run multiple agents against the same files without explicit ownership.

### 3.2 Pull-request expectations

Every pull request must identify:

- Requirement IDs addressed.
- Design component/screen IDs addressed where applicable.
- Architecture ports touched.
- Tests and platforms run.
- Performance/accessibility implications.
- New dependencies or ADRs.
- Known gaps.

### 3.3 Feature flags

Temporary development flags are allowed for incomplete vertical slices. No v1 requirement may remain hidden behind an undocumented release flag.

## 4. Phase 0 — Repository bootstrap and governance

### Goal

Create a reproducible monorepo with cross-platform CI, contracts, linting, dependency policy, and document placement.

### Tasks

1. Create the repository layout from `02-final-architecture.md`.
2. Add Rust and Node toolchain pin files.
3. Bootstrap Tauri 2.11.5 and React/TypeScript.
4. Import the exact ProseMirror package versions from the validated reference lock, then commit a new application lockfile.
5. Add `cargo fmt`, `clippy`, unit test, TypeScript typecheck, lint, frontend test, and build commands.
6. Add CI jobs for Windows, macOS, and Linux from the first commit.
7. Add license/advisory/SBOM tooling and policy.
8. Add ADR directory and templates.
9. Add JSON Schema contract generation/validation tooling.
10. Add deterministic fixture generation and checksum commands.

### Deliverables

- Clean builds on all three platforms.
- Repository `README` and developer setup.
- Lockfiles and dependency inventory.
- CI matrix.
- Empty headless CLI and desktop shell.
- ADR-0001 recording the accepted architecture.

### Gate

No feature work begins until all three platform builds pass and the contract-generation strategy is proven by one round-trip type.

## 5. Phase 1 — Contracts, domain, and canonical format

### Goal

Establish the durable architecture before GUI or backend details spread.

### Tasks

1. Define stable ID types and project/node/style/metadata/comment entities.
2. Implement group/document invariants and ordered-tree commands.
3. Define versioned JSON Schemas for IPC/application commands.
4. Define restricted HTML schema and canonical serializer/parser.
5. Define `project.toml`, annotation JSON, and CSS formats.
6. Implement format validation and `N → N+1` migration framework.
7. Build golden fixtures for every block/mark, titles, comments, metadata, literal tabs, and structural breaks.
8. Build the headless CLI commands:
   - `create`
   - `validate`
   - `inspect`
   - `roundtrip`
9. Implement title synchronization as adapter-independent command logic.
10. Implement pure word-count rules.

### Parallelization

- Domain/tree commands.
- Canonical document codec.
- Manifest/annotation/style codecs.
- Contract schemas and generated bindings.
- Fixture and property-test work.

### Deliverables

- `parchmint-domain`.
- `parchmint-project-format`.
- Golden fixture set.
- Headless round-trip CLI.
- Contract and property tests.

### Gate

- Byte-identical canonical round trips.
- Invalid structures rejected.
- Windows/macOS/Linux path and Unicode fixture tests pass.
- No frontend/backend dependency leaks into domain or format crates.

## 6. Phase 2 — Project repository, save, and recovery skeleton

### Goal

Make current project data safe before adding history/search/UI complexity.

### Tasks

1. Implement `ProjectRepository`, `AtomicWriter`, and platform durability adapters.
2. Create/open projects and load documents lazily.
3. Implement one-writer project lock.
4. Implement save transaction descriptors and atomic multi-file state machine.
5. Implement recovery-journal port and a simple filesystem-backed versioned journal.
6. Implement save queue, dirty-resource tracking, revisions, acknowledgement, and errors.
7. Add forced-termination and partial-write fault harnesses.
8. Add canonical-file readability tests with `.git`, SQLite, and recovery directories removed.

### Deliverables

- Headless create/open/save/recover workflows.
- Fault-injection suite.
- Structured save diagnostics.

### Gate

- No partial canonical state after injected failures.
- Recovery restores edits after process termination.
- Current files remain usable without derived state.
- Save tests run on all three operating systems.

## 7. Phase 3 — Selected backend adapters

### 7.1 History workstream

Implement `parchmint-history-git2` using the validated composition and policies.

Tasks:

- Initialize app-managed `main`.
- Create checkpoint categories and metadata.
- Named snapshots including empty commits.
- Bounded paging and filtering.
- Preview and additive restore by scope.
- Missing/corrupt history isolation.
- Exclusive-owner stale lock recovery.
- Low-priority pack/verify/cleanup maintenance.
- Static-zlib and vendoring guards.

Do not add network features.

### 7.2 Search workstream

Implement `parchmint-search-sqlite` using the validated FTS5 design.

Tasks:

- Dedicated worker/connection.
- FTS5 assertion.
- Stable block/revision schema.
- Body/title/Synopsis/metadata indexing.
- Scope filtering.
- Escaped MATCH query generation.
- Case-sensitive/whole-word post-filter.
- Streaming batches/cancellation.
- Revision revalidation.
- Integrity check and deterministic rebuild.

### Deliverables

- Adapter contract suites.
- CLI history/search commands.
- Cross-platform smoke and scale baselines.

### Gate

- Contract tests pass without callers importing `git2`/`rusqlite` types.
- V03/V04 semantics are reproduced in the application workspace.
- Background tasks do not run on the UI thread.

## 8. Phase 4 — Design reconciliation and design-system import

### Goal

Translate the approved Penpot source into an implementation plan before broad UI coding.

### Tasks

1. Validate the handoff manifest/checksums.
2. Use Penpot MCP and/or `.penpot` inspection to inventory components/screens/tokens.
3. Complete `templates/design-reconciliation-report.md`.
4. Complete implementation targets in `component-matrix.csv`.
5. Import tokens and generate CSS custom properties.
6. Import/optimize SVG assets with stable names.
7. Create deterministic UI fixtures matching reference images.
8. Establish visual-regression capture sizes and tolerances.
9. Record approved deviations or unresolved design questions.

### Deliverables

- `docs/design/design-reconciliation.md`.
- `design/generated/tokens.css`.
- `design/generated/token-metadata.ts`.
- Imported assets and asset manifest.
- Code component map.
- Initial screenshot harness.

### Gate

Product owner or designated reviewer approves the reconciliation report. Broad UI implementation does not precede this gate.

## 9. Phase 5 — Application shell

### Goal

Implement the full navigable shell with mocked/headless data, without the production editor yet.

### Tasks

1. Launcher and project-creation flows.
2. Mode switch.
3. Resizable/collapsible Explorer, editor area, Inspector.
4. Tabs and companion-pane shell.
5. Tree rendering, deep virtualization, multi-selection, drag/drop, rename, cut/copy states.
6. Cards projection and virtualized hierarchy.
7. Inspector sections and settings shell.
8. Search sidebar and replacement-preview shell.
9. History and Recently Deleted shell.
10. Native menus/dialog adapters and semantic command registry.
11. Design reference screenshots and accessibility tree tests.

Use fake project/application services behind the same contracts as production.

### Gate

- Approved reference screens are acceptably reconciled.
- Keyboard navigation and focus behavior pass.
- Shell launches and works on all three platforms.
- No domain logic is duplicated in React components.

## 10. Phase 6 — ProseMirror editor adapter

### Goal

Implement the highest-risk application component before integrating every feature.

### 10.1 Schema and canonical adapter

Implement:

- Every v1 block and mark.
- Stable block IDs.
- Stable style IDs.
- Atomic Scene/Page Break nodes.
- Literal Tab preservation.
- Paste sanitization and plain paste.
- Canonical HTML parse/serialize tests.

### 10.2 Shared session and dual views

Implement:

- `SharedEditorSession`.
- Independent per-view selection/scroll/search/focus.
- Shared document transaction/history controller.
- Selection mapping through transactions.
- One shared toolbar targeting focused view.
- View attach/detach/restore.

### 10.3 Worker and save projection

Implement:

- Worker mirror.
- Canonical projection.
- Changed block text/word count/title projection.
- Recovery batching.
- Revision acknowledgements.

### 10.4 Comments

Implement:

- Anchor mapping.
- Decorations.
- Selection-end affordance geometry.
- Context-menu creation.
- Replies/resolve/orphan states through application services.

### 10.5 Early cross-platform runtime gate

Before building later feature slices, run release-mode native tests on all three platforms for:

- One and two views of ordinary and 250,000-word fixtures.
- First editable viewport.
- Input-to-frame.
- Shared undo/independent selections.
- CJK IME, combining marks, emoji, Arabic/BiDi, literal Tab.
- Rich/plain paste.
- VoiceOver/Narrator-or-NVDA/Orca editing.
- 100%, intermediate, and 200% scaling.
- Memory stabilization and view close reclamation.

All features must remain available at 250,000 words. If this gate fails materially, stop and report evidence rather than introducing a special mode.

### Deliverables

- Production editor package.
- Editor contract test suite.
- Cross-platform instrumentation report.
- ADRs for any accepted optimization.

## 11. Phase 7 — Vertical feature slices

Implement complete end-to-end slices, each including domain/application/frontend/persistence/history/search/tests/design reconciliation.

Recommended order:

1. Create/open/rename/write/save/reopen.
2. Group/document hierarchy and moves.
3. Styles and formatting toolbar.
4. Synopsis/metadata and Inspector focus.
5. Comments and replies.
6. Cards editing/reordering.
7. Local search/replace.
8. Global search/navigation.
9. Global replacement preview/apply/undo.
10. History and named snapshots.
11. Delete/Recently Deleted/restore.
12. Word counts and conditional spellcheck.
13. Export.

Each slice must be usable on all three platforms before moving from “implemented” to “complete.”

## 12. Phase 8 — Cross-platform packaging and release hardening

### Windows

- Installer selection and upgrade behavior.
- WebView2 availability/update behavior.
- File locks, clipboard, drag/drop, high DPI, native shortcuts, screen reader.
- Long/unicode/case-conflict paths.

### macOS

- `.app`/`.dmg`, signing, notarization.
- WKWebView behavior.
- Native menu placement/shortcuts/dialogs.
- VoiceOver and scaling.
- Case-insensitive and normalization behavior.

### Linux

- `.deb` and declared WebKitGTK dependency matrix.
- X11/Wayland clipboard/IME/drag/drop.
- Orca/AT-SPI.
- WebKitGTK versions across supported distributions.
- AppImage remains deferred until separately proven.

### Shared

- Upgrade/migration tests.
- SBOM/notices/advisory scan.
- Clean-machine install/launch/uninstall.
- Project interchange sequence between all three platforms.

## 13. Parallel-agent allocation

After contracts freeze, safe parallel tasks include:

| Agent | Work | Must not modify |
|---|---|---|
| Core agent | Domain, format, CLI | Frontend/editor adapters |
| Persistence agent | Repository/save/recovery | History/search internals |
| History agent | `HistoryStore` adapter | Domain public model |
| Search agent | `SearchIndex` adapter | Canonical format |
| Design-system agent | Token import, shared UI components | Product behavior |
| Editor agent | ProseMirror adapter/session | Canonical project store directly |
| QA agent | Fixtures, contract tests, visual/performance harness | Production behavior without review |
| Release agent | CI, packaging, signing | Domain/editor code except platform adapters |

Integration agents should merge through contracts and fixture tests rather than editing another agent’s internal module.

## 14. Implementation reports

At each phase, produce:

- `status.yaml` based on the supplied template.
- Requirement/design traceability updates.
- Tests and benchmark results.
- Cross-platform result matrix.
- Known risks.
- ADR list.
- Screenshots for changed reference states.

## 15. Stop conditions

An agent must stop and report rather than improvise when:

- PRD and approved Penpot design conflict materially.
- A required port cannot express the needed behavior.
- A selected dependency requires a broad fork.
- Tauri/ProseMirror cannot meet the same-feature 250,000-word requirement after bounded implementation optimization.
- Cross-platform IME/accessibility is fundamentally blocked.
- Canonical data would depend on derived/proprietary state.
- A save/recovery path can lose acknowledged edits.
- A license/security issue changes distribution feasibility.

Stopping with evidence is preferable to silently changing the product.
