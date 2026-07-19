# Stage 01 handoff: foundation and risk spikes

Status: implementation delivered; Linux evidence passes. The stage acceptance
gate remains open until the committed CI workflow passes on Windows and macOS
and the physical-input portions of the editor charter are recorded there. This
handoff does not claim those unavailable results.

## Working-tree state

Verification used the Stage 01 working tree on 2026-07-19. The repository began
with only `PLAN.md` and `plans/`; all implementation files were added by this
stage. Build outputs and machine-local toolchains are ignored. No user-authored
changes were present or overwritten.

## Toolchain and verification hardware

- Rust 1.97.1, Cargo 1.97.1
- Qt 6.8.3 `linux_gcc_64`, dynamically linked
- CXX-Qt and CXX-Qt CMake helper 0.9.1
- CMake 4.3.4
- GCC/G++ 15.2.0
- just 1.57.0 (project-local ignored installation for exact command verification)
- Ubuntu 26.04, Linux 7.0.0
- Intel Core i7-8550U, 4 cores / 8 threads
- 7.1 GiB RAM
- Toshiba 256 GB NVMe SSD

This is older and has less memory than the 2022-era/16 GB reference hardware,
so passing latency results are conservative but not a replacement for reference
hardware trends.

## Delivered behavior

- Reproducible Cargo/CMake/CXX-Qt build with pinned Rust, Qt, CXX-Qt, lockfile,
  dynamic Qt posture, and stable `just` commands.
- Native Qt Quick Controls application with Material style, shared design tokens,
  externalized QML strings, keyboard-focusable controls, dual editor documents,
  local JSON-lines diagnostics, top-level error popup, and offscreen smoke mode.
- CXX-Qt QObject proving Rust-to-QML scalar properties, invokables, completion
  and error signals, property mutation, and 10,000-node Rust-owned outline data.
- Lazy `QAbstractListModel` Qt adapter that requests title/depth/parent roles from
  the Rust snapshot only for rows Qt asks to display.
- `QQuickTextDocument` production host with C++ `QTextCursor` adapter for
  character and paragraph formatting, grouped edit blocks, selection state,
  semantic custom page-break objects, scene breaks, and independent documents.
- Qt Test coverage for independent undo, explicit stable style properties,
  headings, alignment, bold/italic/superscript, links, lists, asset images,
  opaque objects, page breaks, plain/rich paste paths, grapheme boundaries, and
  synthetic Japanese IME/dead-key pre-edit and commit.
- Source-aware Rust Markdown spike using parser offset events, preserved raw
  supported/opaque block slices, unknown YAML mappings, deterministic exact
  serialization, and the representative golden fixture.
- Same-directory temporary write, file flush, atomic replacement, and Unix
  directory flush primitive with phase-specific errors and failure tests.
- Bundled SQLite FTS5 cache proving create, update, delete, query, and complete
  rebuild from supplied source rows.
- Non-blocking Rust worker with project-generation/document-revision stamps and
  stale-result rejection; the UI thread never performs the submitted operation.
- Three-platform CI definition, Rust/Qt tests, QML linting, smoke launch,
  dependency policy, dependency inventory, bootstrap instructions, format draft,
  platform editor charter, eight accepted ADRs, and smoke packaging command.

Deliberately deferred behavior matches the stage plan: production project
schemas, full semantic Markdown codec, canonical editor conversion/autosave,
recovery, final binder/search/export UI, installers, signing, and distribution
license.

## Accepted ADRs

1. `0001-qt-version-linking-and-license.md`: Qt 6.8.3, dynamic linking, module set,
   Material foundation, no release publishing before licensing approval.
2. `0002-qml-text-editor-host.md`: Qt Quick `TextArea`/`QQuickTextDocument`, not a
   hosted Widgets editor.
3. `0003-cxx-qt-boundaries-and-threading.md`: Rust state ownership, narrow DTOs,
   generation/revision validation, explicit errors.
4. `0004-markdown-parser-and-semantic-ast.md`: `pulldown-cmark` source ranges,
   source-aware Rust AST, opaque nodes, no canonical Qt conversion.
5. `0005-parchmint-markdown-extensions.md`: stable style attributes, alignment
   divs, sup/sub HTML, page-break marker, asset scheme, opaque preservation.
6. `0006-atomic-write-and-recovery-direction.md`: same-directory flush/replace and
   later revisioned journals/backups.
7. `0007-sqlite-fts-cache.md`: bundled FTS5 as disposable derived state.
8. `0008-testing-and-ci.md`: Rust + Qt Test + QML lint/smoke across the three
   target operating systems.

No ADR was superseded.

## Persisted-format and public-interface decisions

The initial ParchMint Markdown syntax is documented in
`docs/format/parchmint-markdown-1.md`. It is an accepted syntax decision, not a
frozen complete version 1 schema. The exact representative fixture is the
minimum golden compatibility case.

Stable Rust interfaces available to Stage 02 are:

- `parchmint_domain::{ProjectGeneration, Revision, WorkStamp}`
- Qt-free `EditorSnapshot` / `EditorBlock` boundary types
- `parchmint_markdown::{Document, Block, BlockKind}` spike boundary
- `parchmint_storage::atomic_write`
- `parchmint_index::{SearchIndex, SourceDocument}` (cache only)
- `parchmint_app::{BackgroundWorker, BackgroundJob, BackgroundResult}`
- `parchmint_app::{LazyTreeSnapshot, TreeRow}`

Stage 02 may extend these APIs compatibly. Replacing deprecated `serde_yaml`
before format freeze is explicitly recommended and does not change the accepted
syntax.

## Commands and exact results

Linux, with Qt 6.8.3 on `PATH`/`CMAKE_PREFIX_PATH` and its library path active:

The verified Linux Qt SDK and `just` executable are retained under the ignored
`.toolchains/` directory. See `docs/development/bootstrap.md` for the activation
environment used by these commands.

- `just format-check`: passed.
- `just lint`: passed; Rust Clippy with warnings denied and QML lint generated no
  warning.
- `just test`: passed. Rust: 9 passed, 1 ignored manual measurement. Qt/CTest:
  3 test programs passed (native smoke, editor adapter, outline model).
- `just build`: passed and linked the native `parchmint` executable.
- `just package-smoke`: passed; staged the executable and produced a local
  non-release tar smoke artifact.
- `cargo check -p parchmint_bridge`: passed against Qt 6.8.3.
- `cmake --build build --target editor-benchmark`: passed.
- `cargo test -p parchmint-app --release records_tree_stress_measurement --
  --ignored --nocapture`: passed.

Windows and macOS were not locally available. `.github/workflows/ci.yml` contains
the required build/lint/test/smoke matrix, but no remote workflow run occurred for
this commit. Atomic replacement, physical IME/dead-key input,
and native launch on those systems therefore remain acceptance evidence gaps.

## Performance measurements

Measured on the hardware above:

- Qt `QTextDocument`, 250,000 words / 10,001 blocks: load 24 ms; middle insertion
  53 µs; formatting a 2,000-character selection 39 µs.
- Rust 10,000-node snapshot: construction 1.136 ms; simulated visible-range
  scrolling queries 1.624 µs total in the release test.
- Offscreen native smoke creation and the intentional 300 ms hold: 0.45–0.49 s
  per CTest run.
- Complete CTest suite: 0.69 s with the added synthetic IME case; the editor
  test alone is about 0.19 s.

These demonstrate a credible path to the master latency budgets. They do not yet
measure real GPU frame pacing, long interactive typing percentiles, or stress
memory; Stage 09 owns release-grade trends and the fixture/matrix stages expand
breadth.

## Known defects, risks, and platform gaps

- Cross-platform CI has been authored but not executed from this working tree;
  Stage 01 cannot be declared fully accepted until it passes.
- Physical IME/dead-key, Narrator/VoiceOver/Orca, high-DPI, and clipboard formats
  require the manual charter on target hardware. Linux synthetic IME, grapheme,
  bidi fixture, and rich/plain paste tests pass.
- CMake reports missing optional XKB development metadata and Vulkan headers on
  this host. The offscreen application and synthetic IME test pass; physical X11
  IME validation remains open.
- GNU `ld.bfd` triggers a CXX-Qt performance warning, though linking succeeds.
  CI images should use their default supported linker; this is build-time only.
- `serde_yaml` is deprecated. Its use is confined to the spike and must be
  replaced or explicitly accepted before Stage 02 freezes front matter.
- The Stage 01 Markdown serializer preserves exact source slices. Stage 03 still
  owns full supported-node editing and deterministic reconstruction after edits.
- Atomic replacement is locally tested on Linux only. Windows replacement and
  macOS directory durability need CI failure-injection evidence in Stage 02.
- No distribution license is selected. Do not publish artifacts or statically
  link Qt.

## Stage 02 prerequisites and recommended first task

Use the stable `just` commands and keep Qt-free crates independent of the bridge.
Read all eight ADRs and the Markdown syntax draft before format work. First,
define stable IDs and the project/node/style graph in `parchmint-domain`, then
replace/review the YAML parser and turn the Stage 01 atomic-write primitive into
schema-aware storage with target-platform failure injection. Do not expand the
QML model into a competing mutable project model.
