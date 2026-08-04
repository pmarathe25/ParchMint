# ParchMint v1 Implementation Plan

**Status:** Current execution plan
**Version:** 1.5
**Date:** 2026-08-04

## 1. Purpose

This plan turns the product specification, architecture, and approved Penpot handoff into an implementation sequence for coding agents. It uses bounded stages, early risk selection, generated contracts, native evidence, and safe parallel work.

Do not build the entire product in one agent run. Each stage has explicit entry conditions, outputs, gates, and ownership.

## 2. Entry conditions

Before S00 can pass:

- The complete current build kit is committed.
- A product-owner-approved Penpot handoff exists at `delivery/design-handoff/<version>/`.
- The handoff validates against `delivery/design-handoff-contract.md`, including Light/Dark tokens, Appearance states, references, and checksums.
- Windows, macOS, and Linux CI runners or native test hosts are identified.

Historical prototype lockfiles are not prerequisites. S20 creates the actual application lockfiles from the direct dependency baseline in the architecture.

## 3. Workflow rules

### 3.1 Mainline and worktrees

Use a protected `main` branch and short-lived isolated branches/worktrees. Parallel agents may work only where contracts and file ownership do not overlap.

Suggested workstreams:

- `core-domain-format`
- `persistence-recovery`
- `history-git2`
- `search-sqlite`
- `design-system-shell`
- `editor-feasibility`
- `editor-prosemirror`
- `spellcheck-foundation`
- `feature-slices`
- `independent-test-challenges`
- `cross-platform-release`

### 3.2 Change reports

Every pull request or stage reports:

- Requirement IDs.
- Penpot component/screen IDs where applicable.
- Architectural ports/state owners touched.
- Tests/platforms run.
- Performance/accessibility implications.
- New dependencies and supply-chain effects.
- Known gaps or required G20 proposal.

### 3.3 Governing-document changes

Agents do not create ADRs or changelog entries. A material change stops at G20. After product-owner approval, update the current product specification, architecture, implementation plan, acceptance plan, and design inputs directly before resuming.

### 3.4 Feature flags

Temporary development flags are allowed for incomplete slices. No v1 requirement may remain behind an undocumented release flag.

### 3.5 Automated orchestration

`delivery/agent-playbook.md` and `delivery/stages/` are authoritative for dispatch, independent verification, integration, accepted handoffs, and pipeline state.

Only G10 design reconciliation, G20 material deviation, and G90 release approval normally require product-owner review.

### 3.6 Requirements-first independent test challenge

Implementation agents keep responsibility for developer tests. Each production-behavior run also receives a separate independent test challenge unless its dispatch records a non-production exemption.

The Independent Test Agent seals a charter from requirements, public contracts, acceptance criteria, and the stage task before receiving the candidate or implementation-agent material. It then adds test-only changes against public observation surfaces. Stage acceptance requires the implementation and independent-test commits to pass together. A detected production defect is repaired against the preserved test; a disputed test returns to the Independent Test Agent or Orchestrator rather than being weakened by the implementation agent.

The default required stages are S30, S40, S50, S60, S65, S70, S80, S90, generated feature slices, functional S110 repairs, and S120 work that changes installers, security configuration, bundled resources, upgrade behavior, or other shipped package behavior. S20 is challenged after its provisional harness exists. S55 uses an independent oracle/benchmark charter and post-selection regression tests. Intake, reconciliation, planning, validation-only, and evidence-only runs—including an S120 evidence rerun with no shipped change—may be exempt with a recorded reason.

## 4. Stage graph

```text
S00 repository intake
  └─ S10 design reconciliation
       └─ G10 approval
            └─ S20 repository bootstrap
                 └─ S30 contracts/domain/format
                      ├─ S40 persistence/recovery
                      ├─ S50 design-system/shell
                      └─ S55 editor feasibility
                           └─ S60 editor foundation
                                └─ S65 spellcheck foundation

S40 ─┬─ S70 history
     └─ S80 search

S40 + S50 + S60 + S65 + S70 + S80
  └─ S90 foundation integration
       └─ S100 feature-wave planning/dispatch
            └─ generated feature slices
                 └─ S110 system integration/validation
                      └─ S120 release hardening
                           └─ S130 independent release validation
                                └─ G90 release approval
```

S40, S50, and S55 may run in parallel after S30. S70/S80 may run in parallel after S40. S60 follows a passing S55 selection. S65 follows the editor contract/runtime foundation. Broad feature waves do not begin before S65.

A production-behavior stage shown in the graph is not accepted until its required independent test challenge passes with its candidate. The challenge is a paired run, not another numbered product stage or approval gate.

## 5. S20 — Repository bootstrap and governance

### Goal

Create a reproducible monorepo, exact application locks, cross-platform CI, generated-contract drift guards, and empty application/headless shells.

### Tasks

1. Create the repository layout from the architecture.
2. Pin Rust, Node, package manager, Tauri, React, TypeScript, and build tools.
3. Apply the direct git2/rusqlite/ProseMirror dependency baseline and create real application lockfiles.
4. Assert vendored libgit2/static-zlib and bundled SQLite composition from the resolved lock/build metadata.
5. Bootstrap an empty Tauri/React desktop shell and headless CLI.
6. Prove one Rust↔TypeScript JSON Schema round trip.
7. Add generated-contract commands and CI dirty-diff guard.
8. Add format, lint, typecheck, unit-test, build, and package commands.
9. Add Windows/macOS/Linux CI from the first implementation commit.
10. Add dependency, advisory, license, provenance, SBOM, and native-notice tooling.
11. Add deterministic fixture/checksum tooling.
12. Add weekly supply-chain/provenance workflow.
13. Commit the deny-by-default Tauri threat model/capability matrix and cross-window/project rejection tests.
14. Produce and launch a minimal packaged release artifact on Windows, macOS, and Linux; prove bundled asset load and one privileged IPC round trip without a development server.

### Gate

- Clean builds on all three platforms.
- Actual locks committed.
- Dependency assertions pass.
- Contract generation produces a clean diff and one cross-language round trip.
- Packaged release artifact launches, loads bundled assets, and completes the IPC smoke on all three platforms; record evidence as packaged rather than development-webview.
- Capability/CSP/navigation/session-isolation checks pass.
- No product feature or final UI interpretation is introduced.

## 6. S30 — Contracts, domain, and canonical format

### Goal

Freeze durable state, project commands, and canonical formats before adapters/UI spread.

### Tasks

- Stable IDs and project/node/style/metadata/comment entities.
- Group/document invariants and ordered-tree commands.
- `ProjectCommandDispatcher` and `ProjectUndoManager` contracts, including bounds/reset rules.
- Composite-operation contract for global replacement.
- Versioned IPC/application schemas and generated bindings.
- Restricted deterministic HTML schema/codec.
- `project.toml`, annotation JSON, style CSS, and `dictionary.txt` codecs.
- Migrations.
- Golden fixtures for blocks, marks, comments, metadata, literal tabs, breaks, dictionaries, and deletion tombstones.
- Headless `create`, `validate`, `inspect`, `roundtrip`, and project-command/undo commands.
- Adapter-independent title synchronization and word counting.

### Gate

- Byte-identical canonical round trips.
- Invalid structures rejected.
- Randomized command/undo/redo sequences preserve invariants.
- Unicode/path fixtures pass on Windows/macOS/Linux.
- Generated contracts are clean.
- No frontend/backend type leakage.

## 7. S40 — Project repository, save, and recovery

### Goal

Make current authored data safe before history, search, spellcheck, or broad UI integration.

### Tasks

- `ProjectRepository`, `CanonicalCodec`, `AtomicWriter`, durability adapters.
- Create/open/lazy-load.
- One-writer project lock and application-process project-session routing.
- Schema-versioned atomic `ApplicationPreferenceStore` for appearance and global dictionary, with compare-and-store generations and failure recovery.
- Revisioned save queue and dirty-resource tracking.
- Immutable save revision vectors spanning project revision and every captured open-document editor revision.
- Atomic multi-file save transaction.
- Idempotent checkpoint intents and restart reconciliation between canonical replacement and History.
- Versioned recovery journal/replay.
- Save acknowledgements/errors.
- Project-undo persistence interactions.
- Forced-termination, partial-write, disk-full/permission, stale-lock fault harnesses.
- Readability when history/index/cache/recovery directories are removed.

### Gate

- No partial canonical state after injected failures.
- Acknowledged edits are not lost.
- Recovery restores uncheckpointed edits.
- Recovery/migration/whole-restore reset interactive undo as specified.
- Current project opens without derived state.
- Application preferences survive restart atomically; failed/stale writes do not overwrite newer values or affect project state.
- No filesystem/serialization work runs on UI thread.

## 8. S50 — Approved design system and application shell

### Goal

Import the approved Light/Dark design deterministically and implement the navigable shell against mock application services.

### Tasks

1. Revalidate G10 reconciliation and manifest checksum.
2. Generate semantic Light/Dark CSS and TypeScript metadata from approved tokens.
3. Import/checksum approved assets.
4. Add token-generation dirty-diff CI.
5. Implement System/Light/Dark preference plumbing and live propagation across mock windows.
6. Implement launcher, top navigation, Explorer/Search sidebar, editor/companion shell, Inspector, Cards, History, Recently Deleted, Settings, Export, status bar, dialogs, menus, empty/loading/error/recovery states.
7. Implement tree/Card virtualization, selection, drag/drop, tabs, splitters, keyboard focus, and accessible semantics with fake services.
8. Establish Light/Dark screenshot and accessibility fixtures.

### Gate

- Generated output deterministic and complete for both themes.
- No hard-coded theme-dependent production colors.
- Approved references are acceptably reconciled.
- Appearance switches all mock windows without project mutations.
- Keyboard/focus/accessibility shell tests pass.
- Native shell launches on all three platforms.

## 9. S55 — Shared editor and projection feasibility

### Goal

Select and prove the highest-risk editor state/projection architecture before production commitment.

### Required shared-state mechanisms

Compare concrete ways to host one shared document/history authority across two mounted editor views. Every viable mechanism must define transaction ordering, undo grouping, independent selection/composition/plugin state, simultaneous/stale edits, detach/reattach, and failure recovery. Do not assume that two ordinary independent ProseMirror views with separate history satisfy this requirement.

### Required projection alternatives

At minimum compare:

1. ProseMirror model/document mirror in a Web Worker.
2. Neutral block/delta mirror in a Web Worker or Rust worker.
3. Bounded incremental/idle projection without a persistent mirror.

An alternative may be retired early only with concrete incompatibility evidence.

### Probe scope

- ProseMirror schema subset sufficient for representative prose, styles, comments, lists, breaks, and Unicode.
- Candidate shared document/history mechanisms and two view sessions.
- Independent selection/scroll/focus/local-search state.
- Alternating edits/undo from both views.
- Composition-sensitive behavior and stale transaction handling.
- Ordinary and approximately 250,000-word fixtures.
- Canonical projection, changed-block text, title, word count, annotation/recovery batch.
- Bounded queues/coalescing, projection-target crash, snapshot resync.
- Main/webview/worker memory and resource reclamation.

### Measurements

- First editable viewport, one/two views.
- Input-to-frame beginning/middle/end.
- View-to-view propagation.
- Initial projection synchronization.
- Per-edit and full canonical projection latency.
- Queue depth/backlog during sustained typing.
- Memory stabilization and close reclamation.
- Canonical byte equality and recovery replay.
- IME/clipboard/accessibility behavior in packaged release builds on all three native webviews.

### Gate

Select a shared-state mechanism and projection strategy only if the pair preserves the same features, meets the editor performance gates, does not block input, uses bounded/coalescing queues, recovers deterministically after lifecycle/projection-target failure, and passes the packaged native path on all three platforms.

On pass, record the exact bounded architecture patch in the S55 handoff. The Orchestrator independently verifies the evidence and accepts the pre-authorized projection-section concretization before S60. On failure, stop at G20 with fixtures/evidence; do not implement the original worker concept by default.

Before candidate access, an Independent Test Agent seals the observable correctness/performance/native oracle and fixture plan. After selection, it codifies mechanism-neutral regression tests where the selected public contract permits; it does not independently choose the architecture.

## 10. S60 — Production editor foundation

### Goal

Implement the production ProseMirror adapter using the S55-selected shared-state mechanism and projection strategy.

### Tasks

- Complete v1 schema, stable block/style IDs, marks, lists, quotes, Scene/Page Breaks, literal tabs.
- Canonical parse/serialize and paste sanitization.
- `SharedEditorSession`, independent view sessions, shared history, selection mapping, composition handling.
- One shared toolbar and attach/detach/restore.
- Selected projection/recovery implementation and revision acknowledgements.
- Foundational comments/anchors/decorations/context-menu/Comments-panel commands.
- Local Find state/decorations.
- Project-operation application boundary for global replacement/other composite operations.
- Release-mode native editor gate on Windows, macOS, and Linux.

### Gate

- Canonical fidelity.
- Two-view correctness and shared undo.
- Ordinary/250k same features.
- Input/open/projection/memory budgets.
- CJK IME, combining marks, emoji, Arabic/BiDi, tabs, rich/plain paste.
- Native screen-reader editing and geometry.
- Worker/projection failure recovery where applicable.

Failure stops at G20; do not choose another frontend or reduce behavior independently.

## 11. S65 — Spellcheck foundation

### Goal

Select and prove an offline `SpellcheckService` before broad feature waves.

### Tasks

1. Evaluate native-webview and custom/offline-engine options against the actual contract; do not accept red underlines alone as proof.
2. Freeze the identical v1 language inventory for all platforms.
3. Prove project/global dictionaries, add/remove, normalization, persistence, and invalidation.
4. Prove token-level ranked suggestions and application-owned anchored context menu.
5. Prove viewport/recent-change bounded checking, cancellation, stale-revision rejection, and typing noninterference.
6. Audit dictionary/engine licenses, source provenance, package size, update method, and offline behavior.
7. Decide whether native webview spellcheck must be disabled.
8. Implement the selected adapter and shared contract suite.

### Gate

- Same language inventory and semantics across Windows/macOS/Linux.
- Project/global dictionaries work identically.
- Suggestions/menu/decorations are application controlled and accessible.
- No network use.
- 250k viewport-bounded checking does not violate typing/memory gates.
- Failure remains visible/recoverable and never blocks save.

If no strategy satisfies v1 requirements within bounded implementation, stop at G20 with options such as reducing the supported language inventory or changing the requirement; do not silently rely on inconsistent native spellcheck.

## 12. S70 — History adapter

Implement `parchmint-history-git2` behind `HistoryStore`: linear app-managed main, checkpoint categories, named empty snapshots, bounded paging/filtering, whole-project preview/restore, corruption isolation, stale-lock recovery, background maintenance, and vendoring guards.

Gate: contract/fault/scale/native continuation tests pass without leaking Git types.

## 13. S80 — Search adapter

Implement bundled SQLite FTS5 behind `SearchIndex`: dedicated worker, FTS5 assertion, stable IDs, body/title/Synopsis/metadata indexing, entire-project query semantics, safe escaping, post-filtering, streaming/cancellation, revalidation, integrity/rebuild.

Do not expose section/subtree scope controls in v1.

Gate: contract/semantic/native parity tests pass; background work stays off UI thread.

## 14. S90 — Foundation integration

Integrate S40/S50/S60/S65/S70/S80 into one working foundation:

- Open/create/save/recover through real services.
- Mount editor inside approved shell.
- Light/Dark/System across real windows.
- Search/history/spellcheck/project undo wiring.
- One process/multiple project windows and project-lock routing.
- End-to-end revision/error/status flow.

No broad feature waves until integration gates pass on all three platforms.

## 15. S100 — Vertical feature waves

Generate bounded end-to-end slices. Recommended order:

1. Create/open/rename/write/save/reopen.
2. Project command/undo and hierarchy moves/deletion.
3. Styles and formatting toolbar.
4. Synopsis/metadata and Inspector focus.
5. Comments and replies.
6. Cards editing/reordering.
7. Local search/replace.
8. Global search/navigation.
9. Global replacement preview/apply/undo/checkpoint.
10. History and named snapshots.
11. Recently Deleted/restore.
12. Word counts.
13. Full spellcheck UI/dictionary settings on S65 foundation.
14. Appearance settings final polish.
15. Export.

Each task declares developer-test work, independent-test requirement or exemption, test tier, and separate production/test ownership. “Complete” requires the paired independent challenge and applicable native evidence, not every expensive release workload on every pull request.

## 16. S110 — System integration and validation

Run complete requirement, visual, accessibility, performance, recovery, project-undo, history, search, spellcheck, appearance, and cross-platform suites. Dispatch bounded repair tasks where no G20 change is required; functional repairs retain or receive an independent test challenge.

## 17. S120 — Packaging and release hardening

### Windows

Installer/upgrade/uninstall, WebView2 behavior, project locks/single-instance routing, clipboard/drag/drop/high-DPI/shortcuts/screen reader, long/Unicode/case paths.

### macOS

`.app`/`.dmg`, signing/notarization, WKWebView, menus/dialogs, VoiceOver, scaling, case/normalization behavior.

### Linux

`.deb`, supported WebKitGTK dependency matrix, X11/Wayland clipboard/IME/drag/drop, Orca/AT-SPI. AppImage remains deferred.

### Shared

Upgrade/migration, clean-machine install, SBOM/notices/advisories/provenance, project interchange Linux → Windows → macOS → Linux.

## 18. S130 — Independent release validation

A validation agent that did not implement the candidate produces the unified package defined in the acceptance plan and stops at G90.

## 19. Test workload tiers

- **Tier A — every affected pull request:** lint/type/unit/property/golden/contract tests; builds on all three OSes; generated-contract/token drift guards.
- **Tier B — native affected-feature gate:** release-mode launch and focused native interaction on all three platforms for editor/input/clipboard/windowing/spellcheck/accessibility/packaging changes; selected 250k smoke for editor changes.
- **Tier C — nightly/release-candidate:** full 250k matrix, exact 20M-word corpus, 1M-checkpoint longevity, extended IME/screen-reader/high-DPI/memory/fault/interchange/clean-install suites.

One-million checkpoints and the 20-million-word corpus are not ordinary pull-request requirements. They remain mandatory nightly/release-candidate evidence.

Test tier and test authorship are separate claims. Developer tests and independently authored tests may use the same tier, but traceability records their locations and producing runs separately. Independent authorship does not upgrade headless evidence into a native claim.

## 20. Stop conditions

Stop rather than improvise when:

- Product specification and approved design conflict materially.
- A required port cannot express behavior.
- A selected dependency requires a broad maintained fork.
- No S55 strategy can meet the shared two-view/same-feature 250k requirement.
- No S65 strategy can meet cross-platform spellcheck requirements.
- Native IME/accessibility is fundamentally blocked.
- Canonical data would depend on derived/proprietary state.
- Save/recovery/project undo can lose or partially apply acknowledged operations.
- A license/security/provenance issue changes distribution feasibility.
