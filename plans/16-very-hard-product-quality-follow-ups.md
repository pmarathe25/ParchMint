# Stage 16: product and maintainability follow-ups

Difficulty: very hard

Recommended model: strongest available implementation model, high or max reasoning

Escalate when: the semantic editor requires a new persisted representation or tags require a version-1 product-scope decision

Depends on: stage 15 only for physical release evidence; engineering work may proceed independently

Master references: [`docs/product/README.md`](../docs/product/README.md), [`docs/architecture/editor.md`](../docs/architecture/editor.md), and [`docs/format/parchmint-markdown-1.md`](../docs/format/parchmint-markdown-1.md)

## Outcome

Close the remaining gap between the version-1 contract and the shipping UI,
then reduce the cost and risk of future changes. The first release is blocked
until rich formatting and named styles persist through the Rust Markdown codec;
the current pane still loads canonical Markdown into a plain-text `TextArea`,
while the semantic `EditorAdapter` has no production serialization path.

The same pass must make trash recovery, promised metadata/statistics, save/quit
transitions, and QML interaction coverage complete. Refactoring follows
characterization tests and must not change canonical output or performance.

## Primary ownership

- Rust Markdown and application document-session boundaries
- CXX-Qt editor, command, and model publication boundaries
- QML editor, trash, metadata, statistics, and style workflows
- Qt Quick interaction and accessibility tests
- Large Rust module decomposition and public API documentation

## Required work

### 1. Complete the semantic editor boundary

- Define a typed Rust/Qt semantic-block projection for every editable ParchMint Markdown construct, including opaque source.
- Load that projection through `EditorAdapter.loadSemanticBlocks()` instead of assigning canonical Markdown to a plain-text editor.
- Convert semantic edits back through the source-aware Rust codec with revisions, journals, and external-change checks; never serialize with Qt Markdown/HTML conversion.
- Cover formatting, named styles, lists, links, images, protected objects, text undo, source transitions, save/reopen, recovery, and every exporter with one end-to-end fixture matrix.
- Expose style creation/editing only after stable IDs, inheritance, replacement, and in-use deletion survive restart tests.

### 2. Complete recovery, metadata, and compile UX

- Add a trash projection with title, original parent, deletion state, subtree size, restore, and explicitly confirmed empty-trash commands.
- Decide whether format-1 tags are a first-class 1.0 feature. If yes, add bounded editing and search filters; otherwise document them as compatibility-only metadata.
- Expose cached selection, section, subtree, manuscript, research, and project statistics without whole-document FFI rescans.
- Provide production entry points for the style manager and full compile-preset editing only after their persistence tests pass.

### 3. Make save and quit asynchronous

- Replace bounded UI-thread polling with a close state machine driven by worker completion.
- Disable conflicting actions, show progress, retain the journal-backed fallback, and quit only after success or an explicit failure choice.
- Test slow disk, worker delay, failed canonical save, emergency journal failure, platform quit, and repeated close requests.

### 4. Add UI interaction coverage

- Add Qt Quick tests for Unicode find/replace, dialogs, command-palette routing, drag/drop placements, view switching, editable binding safety, trash, and keyboard/accessibility behavior.
- Keep plain/rich clipboard fixtures exercised by the Qt adapter suite.
- Add automated checks for internal Markdown links and QML design-token/icon policy.

### 5. Reduce change risk

- Split compile, workspace, bridge, storage, Markdown, and domain files by stable responsibility after adding characterization tests.
- Move bridge business logic into Qt-free application modules where the boundary permits.
- Replace crate-wide `missing_docs` suppression with documented public modules/types or narrow justified allowances.
- Extract lifecycle-smoke orchestration from `main.cpp` into a dedicated harness.
- Run a focused redundancy pass after module boundaries make duplicate helpers and overlapping tests measurable.

## Verification

- Run end-to-end edit/save/close/reopen/recovery/export tests for every supported semantic construct.
- Run Qt Quick tests on offscreen CI and physical platform charters for focus, IME, accessibility, drag/drop, and dialogs.
- Inject worker delay and storage failures into save/quit transitions.
- Compare canonical fixtures, public behavior, full test results, and performance budgets before and after each module extraction.
- Run `cargo doc --workspace --no-deps` without blanket missing-documentation suppression.

## Acceptance gate

- Rich formatting and named styles survive save, close, reopen, recovery, source transitions, and export without source loss.
- Users can inspect, restore, and deliberately empty trash without editing canonical files manually.
- Advertised metadata and statistics match the UI, search, and compile surfaces.
- Save and quit never poll a slow worker on the UI thread.
- QML tests fail on incorrect routing, stale metadata, broken drop placement, or missing keyboard behavior.
- Extracted modules have one clear responsibility, unchanged canonical output, passing tests, and no material performance regression.

## Out of scope

- A public plugin API, cloud sync, or a new canonical format version
- Rewriting the Rust/Qt boundary for aesthetic purity
- Large refactors without characterization tests and benchmark comparison

## Handoff

Record the final editor projection/serialization protocol, trash and metadata
command map, asynchronous close state machine, Qt Quick coverage, module map,
before/after performance evidence, and deliberately deferred product choices in
the plan closure. Do not create a separate handoff document.
