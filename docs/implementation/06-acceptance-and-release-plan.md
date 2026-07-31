# ParchMint v1 Acceptance and Release Plan

**Status:** Final validation baseline  
**Version:** 1.0  
**Date:** 2026-07-28

## 1. Purpose

This document defines how ParchMint v1 is validated against the PRD, approved Penpot handoff, final architecture, and cross-platform release obligations.

Passing a unit test is not sufficient evidence for native input, accessibility, performance, packaging, or data durability. Evidence must match the claim.

## 2. Traceability

Maintain `docs/traceability.csv` from the provided template. Every must-level PRD requirement must map to:

- One or more implementation modules.
- One or more automated tests where possible.
- A Penpot screen/component where applicable.
- A native manual/instrumented test where automation is insufficient.
- A final disposition: pass, approved waiver, or blocked.

No requirement may be marked complete based only on code presence.

## 3. Test layers

### 3.1 Domain unit and property tests

Cover:

- Tree invariants and cycle rejection.
- Stable ordering.
- Multi-selection normalization.
- Move/copy/cut semantics.
- Title synchronization.
- Style inheritance and replacement.
- Metadata applicability/default/deletion rules.
- Comment thread state.
- Export-setting inheritance.
- Word-count rules.
- Undo command grouping.

Use generated/randomized trees and operation sequences to find invariant failures.

### 3.2 Contract tests

Every replaceable port has one shared suite:

- `ProjectRepositoryContract`
- `CanonicalCodecContract`
- `RecoveryJournalContract`
- `HistoryStoreContract`
- `SearchIndexContract`
- `ExporterContract`
- `EditorAdapterContract`
- `PlatformServiceContract` where feasible

An alternative backend or frontend adapter cannot be accepted until it passes the same suite.

### 3.3 Golden canonical-format tests

Maintain fixtures for:

- Every block/mark/style.
- Lists and nested lists.
- Literal tabs and whitespace.
- Scene/Page Break nodes.
- Links and sanitization.
- Titles and title divergence.
- Unicode, combining marks, emoji, CJK, Arabic/BiDi.
- Comments/anchors sidecars.
- Metadata/style manifests.
- Deleted/restored subtrees.

Tests must prove:

- Parse → serialize byte equality for canonical input.
- Semantically equivalent input canonicalizes deterministically.
- Unsupported/unsafe HTML is rejected or normalized safely.
- No platform changes line endings, paths, or Unicode identity.

### 3.4 Editor semantic tests

Run against the ProseMirror adapter without requiring the full app:

- Each command and key behavior.
- Shared document/two-view changes.
- Independent selection mapping.
- Shared undo/redo.
- Toolbar focus targeting.
- Paste and Paste Without Formatting.
- Comments and anchor mapping through insert/delete/split/join/undo.
- Local search decorations.
- Title synchronization events.
- Worker projection and recovery replay.
- Canonical round trips.

### 3.5 Persistence and fault tests

Inject failure/termination:

- Before temporary file write.
- During document write.
- During manifest/style/annotation write.
- Before/after flush.
- Before/after atomic replace.
- During history object/tree/ref update.
- During pack/verification.
- During recovery compaction.
- With permission denial, disk-full simulation, and locked files where feasible.

Verify:

- Last acknowledged durable state remains valid.
- UI never claims a failed revision is Saved.
- Recovery can restore pending edits.
- Current project files remain readable when history/search are broken.
- A subsequent save/checkpoint succeeds after bounded recovery.

### 3.6 History tests

Retain the validated V03 suite as a regression baseline:

- Checkpoint categories.
- Named empty snapshot.
- Bounded paging.
- Filtering and previews.
- Whole-checkpoint project restore; document and subtree checkpoint restore remain unavailable.
- Additive history.
- Missing/corrupt object isolation.
- Interrupted ref lock recovery.
- 250,000 and periodic 1,000,000 checkpoint longevity runs.
- Cross-platform same-repository continuation.
- Pack/verify/cleanup policy.

Full 1M runs may be scheduled nightly/release rather than on every pull request.

### 3.7 Search tests

Retain the validated V04 semantics:

- FTS5 startup assertion.
- Exact 20-million-word corpus.
- Plain, phrase, case-sensitive, whole-word.
- Field and section/subtree scopes.
- Safe MATCH escaping.
- Streaming and cancellation.
- Snippets/ranking.
- Small and 250k document replacement.
- Stale/deleted-result revalidation.
- Index deletion/rebuild.
- Integrity/quick check.
- Identical corpus behavior across platforms.

### 3.8 Visual and interaction tests

For every approved reference frame:

- Use the deterministic fixture/state in `design-manifest.yaml`.
- Capture at the exact dimensions and scale.
- Run automated image comparison.
- Review focus, hierarchy, spacing, wrapping, panel dimensions, component states, and truncation.
- Record approved platform-specific differences.

Interaction tests cover prototype flows from the Penpot design brief.

### 3.9 Accessibility tests

Automated:

- Roles/names/states.
- Focus order.
- Keyboard access.
- Dialog focus trap/restoration.
- Tree levels and expansion.
- Tab and close semantics.
- Live regions for save/search/error.
- Contrast and minimum target checks where tooling allows.

Native manual/instrumented:

- VoiceOver on macOS.
- Narrator and/or NVDA on Windows.
- Orca on Linux.
- Editing, selection, formatting, comments, tree, Cards, search, history, dialogs.

A browser accessibility tree existing is not sufficient; usable editing transcripts and task completion are required.

## 4. Performance fixtures

### Project fixtures

- `project-small`: 20 documents, 50k total words.
- `project-medium`: 300 Manuscript + 25 Research documents, 5M words.
- `project-max`: 500 Manuscript + 50 Research documents, 20M words.

### Document fixtures

- `doc-typical`: representative 5k–15k-word chapter.
- `doc-large-100k`: mixed semantic blocks and comments.
- `doc-max-250k`: approximately 250,000 words, about 1,900+ variable-height blocks, Unicode and comments.
- `doc-pathological`: one extremely long paragraph; robustness test, not strongest latency guarantee.

All generated fixtures have deterministic seeds and checksums.

## 5. Performance measurements

### 5.1 Input-to-frame

Instrument editor transaction start and next committed visual frame.

Measure:

- Beginning/middle/end.
- One and two views.
- Ordinary and 250k documents.
- Formatting and comment operations.
- Background autosave, word count, search updates, history checkpoint.

Gate:

- p95 ≤16 ms target.
- p99 ≤33 ms target.
- No repeated multi-frame stalls under ordinary typing.

### 5.2 First editable viewport

Measure from document-open request to the point at which the editor is visible, focused on request, and accepts a real edit.

- Ordinary documents: target ≤250 ms warm.
- 250k document: release gate ≤1 second on agreed reference hardware.
- Same functionality and plugins remain enabled.

Report one-view and two-view results separately; neither may be skipped.

### 5.3 Project open

Measure:

- Launcher selection to tree/metadata usable.
- Bodies loaded at startup.
- Cache current versus rebuild.
- History with large checkpoint counts.

Project open must not read every document body.

### 5.4 Search

- Time to first batch and completion.
- Cold index, warm index, rebuild.
- Cancellation latency.
- Search while editing.

Warm first result gate: ≤200 ms; validated baseline is substantially faster.

### 5.5 Memory

Measure full process tree:

- Empty app.
- Typical document one view.
- 250k document one view.
- 250k same document two views.
- Repeated open/edit/search/undo/close cycles.
- After closing companion.
- After closing document/project.

Gate:

- Memory stabilizes.
- Material editor resources are reclaimed after close.
- No sustained monotonic growth attributable to leaked views/plugins/decorations.
- No unusable swapping or native memory-pressure termination on reference machines.

### 5.6 Background operations

Measure typing while:

- Saving canonical state.
- Creating Git checkpoint.
- Updating/rebuilding SQLite index.
- Exporting.
- Packing/verifying history.

Maintenance must yield/cancel and not compete with active input.

## 6. Native input and clipboard matrix

Run on each supported platform/runtime:

- Basic Latin and extended Latin.
- Combining marks.
- Emoji and variation sequences.
- CJK IME composition and candidate selection.
- Arabic and mixed BiDi.
- Grapheme cursor/backspace.
- Literal Tab.
- Rich paste from browser and word processor.
- Plain paste/Paste Without Formatting.
- Copy/paste across the two views.
- Context menu.
- Undo/redo around composition and paste.
- Comment decorations and context-menu creation during composition.

Unknown is a failure for release.

## 7. High-DPI and window matrix

Validate:

- 100%, 125%/150% where available, and 200%.
- Multiple displays with different scale factors.
- Window resize while editing.
- Minimum 1280×720 study.
- Fullscreen/maximized/native title bar.
- Splitter hit targets.
- Caret, selection, comment bubble, context menu, and drag/drop geometry.

## 8. Cross-platform project interchange

Automated release test:

1. Create and edit project on Linux.
2. Save/checkpoint and package/copy directory.
3. Open and continue on Windows.
4. Save/checkpoint and continue on macOS.
5. Return to Linux.

At every step verify:

- Same hierarchy/order/content/styles/metadata/comments.
- Clean history repository and reachable checkpoints.
- Unicode and decomposed/composed filenames/metadata.
- No line-ending churn.
- Search rebuilds and returns equivalent semantics.
- Workspace state may vary appropriately.

## 9. Packaging gates

### Windows

- Clean Windows install.
- Installer upgrade and uninstall.
- WebView2 runtime behavior.
- File associations only if explicitly added.
- No installed Git/SQLite requirement.

### macOS

- Signed/notarized application.
- Gatekeeper clean launch.
- Upgrade and uninstall guidance.
- Native menus/shortcuts/dialogs.

### Linux

- `.deb` clean install on each supported distro/runtime target.
- Declared WebKitGTK dependencies.
- Wayland and X11 where supported.
- AppImage is not required for v1.

## 10. Design acceptance

A design implementation passes when:

- All reference screens/states exist.
- Token values are generated from the approved handoff.
- Component mapping is complete.
- No unexplained major visual deviations remain.
- Focus/keyboard/error/loading states match approved intent.
- Responsive/collapsed behavior works at specified dimensions.
- Native platform differences are documented and approved.

Design acceptance does not override accessibility or native-control correctness.

## 11. Security and dependency gates

- Exact Cargo/npm locks committed.
- Rust and npm advisory scans.
- License/source inventory.
- SBOM for each package.
- Tauri capability audit and CSP test.
- No remote privileged content.
- No Git network features.
- SQL injection/MATCH escaping tests.
- Paste sanitization fuzzing.
- Path traversal/symlink/case collision tests.

## 12. Release evidence package

Every release candidate produces:

```text
release-evidence/<version>/
├── requirement-traceability.csv
├── design-reconciliation.md
├── visual-diff-report/
├── performance/
├── accessibility/
├── platform/
├── fault-tests/
├── history/
├── search/
├── security/
├── licenses/
├── sbom/
├── package-hashes.txt
└── release-decision.md
```

The release decision names every waiver and residual risk.

## 13. Failure handling

If the implementation cannot meet a normative requirement:

1. Preserve the failing reproducible fixture and raw evidence.
2. Identify whether the issue is implementation, dependency, architecture, or product scope.
3. Propose bounded alternatives and consequences.
4. Stop before silently changing behavior.
5. Obtain product-owner approval for any waiver or specification change.

In particular, failure at the 250,000-word fixture must not result in an unapproved feature-reduced mode.
