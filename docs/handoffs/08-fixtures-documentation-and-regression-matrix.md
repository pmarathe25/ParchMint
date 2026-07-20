# Stage 08 handoff: fixtures, documentation, and regression matrix

Status: implemented in the shared worktree. Linux verification used the pinned
Qt 6.8.3 installation under `.toolchains/qt/6.8.3/gcc_64`. No production
format, exporter, ownership, threading, recovery, or path-security behavior
was changed.

## Delivered coverage

- Added seven focused Markdown fixtures covering all documented front-matter
  fields, pairwise formatting/extensions, Unicode/newline stability, unknown
  styles, opaque blocks, malformed extensions, and unclosed front matter.
- Added six focused project-format fixtures covering styles, compile presets,
  attachment catalogs, duplicate assets, unsafe outline paths, and a newer
  project version. Added format-1 recovery and no-op migration fixtures,
  including a newer recovery version.
- Added `crates/parchmint-test-support`, a Qt-free deterministic corpus helper
  and `generate-corpus` binary. It generates documents on demand from a seed;
  no large generated corpus is committed.
- Added checked-in corpus manifests for 100 nodes × 50 words, 1,000 nodes ×
  100 words, and 10,000 nodes × 1,000 words (10 million configured words).
- Added five human-readable example projects: `tour`, `medium-novel`,
  `research-heavy`, `unicode-notes`, and `format-edge-case`. Storage tests open
  every project read-only and assert its expected document count.
- Added Markdown fixture tests, corpus manifest tests, and the example-project
  catalog test. Existing compile tests continue to cover all six structural
  validators and deterministic outputs; no exporter semantics were invented.

## Generator commands and seeds

The checked-in seed is `20260720`:

```sh
cargo run -p parchmint-test-support --bin generate-corpus -- \
  --seed 20260720 --nodes 10000 --words 1000 \
  --manifest tests/fixtures/corpus/10000-nodes.toml
```

The manifest files are the reproducibility record. `CorpusConfig::document`
emits one document at a time, so stress runs can choose how much to materialize.

## Documentation and traceability

- User guides cover binder/summaries, editor/styles, research/split panes,
  search/statistics, compile/export, and backups/recovery under
  [`docs/user-guide/`](../user-guide/).
- Developer documentation covers architecture navigation, build/test commands,
  keyboard shortcuts, translation source catalog, and the Windows/macOS/Linux
  platform charter under [`docs/development/`](../development/).
- The exporter contract is recorded in
  [`docs/export/support-matrix.md`](../export/support-matrix.md).
- The requirement-to-coverage table is
  [`docs/development/regression-matrix.md`](../development/regression-matrix.md).
- The English Qt source catalog is `translations/ParchMint_en.ts`; no guessed
  non-English translations were added.

## Verification

- `cargo test --workspace --exclude parchmint_bridge`: passed. 55 Rust tests
  were discovered, 52 passed, and 3 existing manual-measurement tests were
  ignored; the new test-support crate
  contributed 3 tests, Markdown contributed 3 fixture tests, and storage
  contributed 1 example-catalog test.
- `just test` with `QMAKE`, `PATH`, `LD_LIBRARY_PATH`, and `CMAKE_PREFIX_PATH`
  pointed at the pinned Qt: passed. Rust tests passed, the desktop executable
  built, and all 3 CTest cases passed.
- `cargo clippy --workspace --all-targets --exclude parchmint_bridge -- -D
  warnings`: passed.
- `cargo fmt --all --check`: passed.
- `cmake --build build --target qmllint`: passed with the existing QML
  unqualified-access advisories and no errors.
- `git diff --check`: passed.
- The generator command above was run and its 100-node output matched the
  checked-in manifest byte-for-byte.

The full `just lint` wrapper was retried on the pinned toolchain but stalled in
the fresh `cxx-qt-lib` compilation without producing a diagnostic; the
Qt-free Clippy pass and the separate QML lint target completed successfully.
Stage 07 had previously recorded the full wrapper passing on this same Linux
toolchain.

## Deliberately deferred / Stage 09 blockers

No failing product regression was observed, so none is being mislabeled as
expected behavior or handed to Stage 09 as a production defect. The following
are environment/manual coverage gaps already called out in the matrix:

- Windows and macOS consumer validation, installers, file associations, and
  native screen-reader runs (Narrator, VoiceOver, Orca).
- Physical IME/dead-key input, bidirectional text, high-DPI, reduced-motion,
  sleep/resume, abrupt termination, full-disk/permission injection, and
  external-editor exercises on each release platform.
- Word, LibreOffice, browser/reader, EPUBCheck, and native Qt PDF consumer
  opening, plus final font fallback/pagination measurements.
- 10,000-node/10-million-word release-mode performance measurements and final
  packaging/license audits.

These are manual or release-environment gates, not changes to the Stage 07
export contract. The next agent should begin with the platform charter and
consumer export matrix, then run the full cross-platform traceability table.
