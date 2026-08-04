# ParchMint v1 Acceptance and Release Plan

**Status:** Current validation baseline
**Version:** 1.3
**Date:** 2026-08-04

## 1. Purpose

This document defines how ParchMint v1 is validated against the product specification, approved Penpot handoff, architecture, and cross-platform obligations.

Evidence must match the claim. Unit tests do not prove native input, accessibility, spellcheck menu behavior, performance, packaging, or data durability.

Every runtime result is labelled `headless`, `development webview`, `packaged executable`, `installed package`, or `native interactive`. A stronger label requires evidence from that exact environment; development content cannot stand in for the packaged custom-scheme/runtime path.

## 2. Traceability

Maintain `delivery/traceability.csv`. Every must-level requirement maps to:

- Implementation modules.
- Developer-authored automated tests where possible.
- Independently authored automated tests for production-behavior stages, or a recorded non-production exemption.
- Penpot screen/component where applicable.
- Native manual/instrumented test where automation is insufficient.
- Current disposition: not started, in progress, pass, blocked, or product-owner-approved current-spec change.

No requirement is complete based only on code presence.

Traceability names the implementation run, independent-test run, charter commit, candidate commit, and test locations needed to establish provenance. Test authorship remains distinct from evidence strength.

## 3. Test tiers

### Tier A — every affected pull request

- Format/lint/typecheck.
- Unit/property/golden tests.
- Shared adapter contract tests.
- Windows/macOS/Linux build and applicable headless tests.
- Generated Rust/TypeScript contract clean-diff check.
- Generated design-token clean-diff check.
- Small deterministic performance regression fixtures.

### Tier B — native affected-feature gate

Required for changes touching editor input/view state, clipboard, menus, windows/project locking, spellcheck, accessibility, scaling, filesystem durability, or packaging:

- Release-mode native checks on all three platforms.
- Focused real interaction and accessibility evidence.
- Selected 250k editor smoke for editor/projection/plugin changes.

### Tier C — nightly and release candidate

- Full one/two-view 250k matrix.
- Exact 20-million-word search/project corpus.
- 1,000,000-checkpoint history longevity.
- Extended IME, screen-reader, high-DPI, memory, fault, project interchange, and clean-install suites.

Tier C workloads remain mandatory before release but do not run on every pull request.

### Test authorship and independence

Implementation agents add developer tests for fast feedback, internal invariants, and regression protection. A separate Independent Test Agent challenges observable behavior, public contracts, negative/fault cases, and cross-component integration before applicable stage acceptance.

The independent charter is committed before the agent receives the candidate, implementation report, implementation conversation, or diff explanation. After sealing, the agent may use public interfaces, generated schemas, test-support surfaces, and build/test failures to implement its cases, but production bodies are not the expected-behavior oracle. The independent agent changes only dispatched tests, fixtures, and run artifacts.

A required challenge may be exempt only for a run that produces no production behavior, with the reason recorded in dispatch and traceability. Stage applicability and special cases are owned by the implementation plan. S130 candidate validation remains independently executed and is not replaced by stage-level independent authorship.

When an independent test fails, preserve it while production is repaired. The implementation agent may not weaken it. An incorrect or ambiguous test is corrected by the Independent Test Agent or through recorded Orchestrator adjudication against governing inputs.

## 4. Domain and project-command tests

Property/randomized tests cover:

- Tree invariants/cycle rejection/order.
- Multi-selection normalization.
- Move/copy/cut/delete/restore.
- Title synchronization.
- Style inheritance/replacement.
- Metadata applicability/default/deletion.
- Comment thread state.
- Export-setting inheritance.
- Word-count rules.
- Project-command grouping, inverse patches, bounded eviction, redo invalidation, and reset semantics.
- Global replacement as one project undo and one checkpoint.

Run random command/undo/redo sequences and prove invariants and canonical equivalence.

## 5. Contract tests

Shared suites:

- `ProjectRepositoryContract`
- `CanonicalCodecContract`
- `RecoveryJournalContract`
- `HistoryStoreContract`
- `SearchIndexContract`
- `SpellcheckServiceContract`
- `ExporterContract`
- `EditorAdapterContract`
- `PlatformServiceContract` where feasible

Alternative implementations pass the same suite without caller changes.

## 6. Generated contract drift

CI runs generators and then:

```text
git diff --exit-code -- <generated Rust paths> <generated TypeScript paths>
```

Golden fixtures serialize in Rust/deserialise in TypeScript and vice versa. Generated headers contain schema version/checksum.

## 7. Canonical format tests

Fixtures cover every block/mark/style, lists, literal tabs/whitespace, Scene/Page Breaks, links/sanitization, titles/divergence, Unicode/combining/emoji/CJK/Arabic/BiDi, comments/anchors, metadata/styles, dictionary normalization, deletion/restoration.

Prove:

- Canonical parse → serialize byte equality.
- Equivalent input canonicalizes deterministically.
- Unsafe/unsupported HTML is rejected/normalized.
- No platform changes line endings, paths, Unicode identity, or dictionary sorting.

## 8. Editor and projection tests

Adapter tests cover:

- Commands/key behavior.
- Shared document/two-view changes.
- Independent selection/scroll/local-search mapping.
- Shared undo/redo from alternating views.
- Stale transaction and IME composition handling.
- Toolbar focus targeting.
- Paste/Paste Without Formatting.
- Comments/anchor mapping through insert/delete/split/join/undo.
- Project-operation application boundary.
- Projection/recovery revisions, bounded queues/coalescing, target crash/resync.
- Canonical round trips.

S55 evidence compares projection alternatives; S60 evidence validates the selected production strategy.

## 9. Spellcheck tests

Contract and native tests cover:

- Identical supported-language inventory on all platforms.
- Project-default language changes.
- Project/global dictionary add/remove/persistence/normalization.
- Token-level ranked suggestions.
- In-place decorations and correctly anchored application-owned menu.
- Viewport/recent-change bounded checking.
- Cancellation and stale-revision rejection.
- Unicode word boundaries, contractions, hyphenation, apostrophes, combining marks, and mixed scripts.
- Native webview spellcheck disabled or reconciled as selected.
- Offline behavior and no prose network traffic.
- Failure visibility/recovery without blocking typing/save.
- License/provenance/package inventory.

## 10. Appearance tests

Automated and native tests cover:

- System is default.
- System follows operating-system changes while running.
- Explicit Light/Dark overrides persist.
- Every open window updates without restart.
- A newly created window receives the current resolved theme before first paint.
- Stale operating-system appearance events cannot override a newer explicit choice.
- Preference write failure remains visible and does not claim the choice persisted.
- Appearance creates no project command, dirty state, save, recovery, or history entry.
- Canonical project files/export are byte-identical before/after appearance changes.
- All approved Light/Dark reference states.
- No hard-coded theme-dependent production values.
- Dark manuscript canvas is fully dark.
- Contrast/focus/selection/error/comment/search/spellcheck states in both themes.

## 11. Persistence and fault tests

Inject failure/termination before/during/after projection-vector capture, temporary write, flush, each atomic replace, canonical transaction commit, checkpoint-intent persistence, history checkpoint commit, acknowledgement, pack, recovery compaction, and composite global replacement.

Verify:

- Last acknowledged durable state remains valid.
- UI never claims a failed revision is Saved.
- Recovery restores pending edits.
- Project undo does not leave partially applied state.
- Current files remain readable with history/search/cache broken.
- Reopen reconciliation completes each canonical revision vector's required checkpoint exactly once and never combines mismatched project/editor revisions.
- Subsequent save/checkpoint succeeds after bounded recovery.

## 12. History tests

- Checkpoint categories and named empty snapshot.
- Bounded paging/filtering/previews.
- Whole-project restore only.
- Additive history.
- Missing/corrupt object isolation.
- Interrupted ref lock recovery.
- Project dictionary included; appearance/global dictionary/workspace excluded.
- 250k and periodic 1M checkpoint longevity.
- Cross-platform same-repository continuation.
- Pack/verify/cleanup.

Full 1M runs are Tier C nightly/release.

## 13. Search tests

- FTS5 startup assertion.
- Plain/phrase/case-sensitive/whole-word.
- Body/title/Synopsis/metadata fields.
- Entire-project v1 query behavior with no user scope selector.
- Safe MATCH escaping.
- Streaming/cancellation/snippets/ranking.
- Small/250k replacement projection updates.
- Stale/deleted-result revalidation.
- Index deletion/rebuild/integrity.
- Cross-platform corpus parity.
- Exact 20M-word corpus as Tier C.

Do not require section/subtree search scopes in v1 acceptance.

## 14. Visual and interaction tests

For every approved reference:

- Load deterministic fixture/state.
- Use exact dimensions, scale, platform, and theme.
- Capture and compare.
- Review focus, hierarchy, spacing, wrapping, panel dimensions, truncation, and component states.
- Record current approved platform-specific deviations.

Prototype flows from the design brief become interaction tests.

## 15. Accessibility tests

Automated:

- Roles/names/states.
- Focus order and restoration.
- Dialog focus trap.
- Tree levels/expansion.
- Tabs/close semantics.
- Live regions for save/search/spellcheck/error.
- Contrast/targets in Light/Dark.

Native:

- VoiceOver on macOS.
- Narrator and/or NVDA on Windows.
- Orca on Linux.
- Editing/selection/formatting/comments/tree/Cards/search/history/spellcheck/appearance/dialogs.

A browser accessibility tree alone is insufficient; record usable editing/task transcripts.

## 16. Performance fixtures

Projects:

- `project-small`: 20 documents, 50k words.
- `project-medium`: 300 Manuscript + 25 Research, 5M words.
- `project-max`: 500 Manuscript + 50 Research, exactly 20M words.

Documents:

- `doc-typical`: 5k–15k words.
- `doc-large-100k`.
- `doc-max-250k`: approximately 250k words, 1,900+ variable-height blocks, Unicode/comments.
- `doc-pathological`: one extremely long paragraph, robustness rather than strongest latency guarantee.

All generated fixtures have deterministic seeds/checksums.

## 17. Performance gates

### 17.1 Input-to-frame

Beginning/middle/end; one/two views; ordinary/250k; formatting/comments; save/search/history/spellcheck/projection background work.

- p95 ≤16 ms target.
- p99 ≤33 ms target.
- No repeated multi-frame stalls under ordinary typing.

### 17.2 First editable viewport

- Ordinary warm: target ≤250 ms.
- 250k: release gate ≤1 second on agreed hardware.
- One/two views reported separately.
- Same plugins/features enabled.

### 17.3 Projection

- UI-thread work remains within the 2 ms event-turn requirement.
- Queues stay bounded/coalesced under sustained typing.
- No unbounded revision lag.
- Projection target crash/resync is deterministic.
- Canonical projection becomes available in time for the autosave pipeline without blocking input.

S55 records strategy-specific budgets that are at least as strict as these observable constraints.

### 17.4 Project open

Tree/metadata usable without reading every body. Measure cache current/rebuild and large history.

### 17.5 Search

Warm first result ≤200 ms. Measure cold/warm/rebuild/cancellation/search-while-editing.

### 17.6 Spellcheck

Measure changed visible block to decoration/suggestion availability, cancellation, scroll/viewport churn, and typing under dictionary load. No full-document synchronous check on open.

### 17.7 Memory

Measure full process tree for empty app, typical one view, 250k one/two views, projection worker/neutral mirror, spellcheck engine/dictionaries, repeated cycles, companion/document/project close.

Memory must stabilize and material resources must be reclaimed.

## 18. Native input and clipboard matrix

On every supported runtime:

- Latin/extended Latin/combining marks/emoji.
- CJK IME composition/candidates.
- Arabic/mixed BiDi.
- Grapheme cursor/backspace.
- Literal Tab.
- Rich/plain paste from browser/word processor.
- Copy/paste across two views.
- Context menus.
- Undo/redo around composition/paste.
- Comment/spellcheck decorations and menus during composition.

Unknown is release failure.

## 19. High-DPI and windows

Validate 100%, 125/150% where available, 200%, mixed-DPI displays, resize, minimum 1280×720, fullscreen/maximized/title bar, splitters, caret/selection/comment/spellcheck/menu/drag geometry, and appearance propagation across multiple project windows.

## 20. Cross-platform interchange

Automated release sequence: Linux → Windows → macOS → Linux.

Verify hierarchy/order/content/styles/metadata/comments/dictionary, clean reachable history, Unicode/filename normalization, no line-ending churn, equivalent search rebuild, and appropriate workspace differences.

## 21. Packaging gates

### Windows

S20 packaged launch/asset/privileged-IPC smoke; clean install/upgrade/uninstall, WebView2, no installed Git/SQLite, project lock/single-instance behavior.

### macOS

S20 packaged launch/asset/privileged-IPC smoke; signed/notarized clean Gatekeeper launch, upgrade/uninstall guidance, native menus/dialogs.

### Linux

S20 packaged launch/asset/privileged-IPC smoke; `.deb` clean install on supported distro/runtime matrix, Wayland/X11. AppImage not required.

## 22. Security and dependency gates

- Exact application Cargo/npm locks.
- Rust/npm advisory scans.
- License/source/provenance inventory.
- SBOM for each package.
- Tauri capability/CSP tests.
- No remote privileged content.
- Remote-navigation and iframe rejection.
- Server-side window/project-session binding for every privileged command and rejection of cross-window/project requests.
- Opaque path-handle scope and absence of generic filesystem/shell/HTTP webview capabilities.
- No Git network features.
- SQL/MATCH escaping.
- Paste sanitization fuzzing.
- Path traversal/symlink/case collision.
- Offline spellcheck and dictionary license inventory.
- Weekly and release-candidate provenance checks.
- Machine-readable advisory/license/provenance policy, unexpired approved exceptions, bundled-resource hashes, and SBOM diffs for lock changes.

## 23. Unified release evidence package

Every release candidate produces exactly:

```text
delivery/release-evidence/<candidate-version>/
├── requirement-disposition.csv
├── platform-matrix.yaml
├── visual/
├── performance/
├── accessibility/
├── appearance/
├── editor-projection/
├── spellcheck/
├── recovery-project-undo/
├── history-search/
├── packaging/
├── security-licenses-sbom/
├── package-hashes.txt
├── known-issues.yaml
└── release-approval.yaml
```

`release-approval.yaml` starts with `status: pending`. This layout is authoritative for the playbook and S130.

Create the machine-readable files from `delivery/templates/release/`. Before G90, S130 must validate that every required path exists, all requirement dispositions and platform rows are non-pending or explicitly blocked, package hashes cover every candidate package, and the approval file remains `pending` for the product owner.

## 24. Failure handling

If a mandatory requirement cannot be met:

1. Preserve a reproducible fixture and raw evidence.
2. Classify implementation, dependency, architecture, or product-scope cause.
3. Propose bounded alternatives/consequences at G20.
4. Stop before silently changing behavior.
5. After approval, update current governing documents directly.

Failure at 250k, shared two-view, spellcheck, appearance accessibility, or data safety must not produce an unapproved reduced mode.
