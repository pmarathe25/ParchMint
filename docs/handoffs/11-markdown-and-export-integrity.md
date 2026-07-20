# Stage 11 handoff: Markdown and export integrity

Status: the codec/export integrity implementation is in the working tree and
passes the available automated suite. The platform consumer matrix remains
manual verification work; no release or consumer-validation claim is made by
this handoff.

## Working-tree and interface state

- Base commit: `2f97fb93ef1644bf5ae067f5ec3891ca8d964189`.
- Verification commit: the final `git log -1` commit containing this handoff.
- Canonical project and Markdown format versions remain 1. `ParseOptions` now
  exposes explicit resource limits. `CompileExportWorker` now returns either a
  validated `PreparedExport` or, for PDF only, a compiled IR for Qt-owner
  rendering. These are Rust service-boundary additions, not persisted schema
  changes.
- No ADR was added or superseded. The contained custom ZIP writer now rejects
  values requiring ZIP64 instead of saturating fields; a future ZIP64-capable
  maintained writer remains a possible replacement without a format change.

## Grammar and limit decisions

- Default limits: 16 MiB document bytes, 100,000 blocks, inline/fenced-div
  depth 64, 1,000,000 delimiter inspections, and 1,024 diagnostics. The codec
  returns `MarkdownError::ResourceLimit` with the exceeded category and bound.
- The inline scanner is escape-aware and bounded. It supports nested emphasis,
  strong, strike, styled spans, links, super/subscript, variable-length code
  spans, hard/soft breaks, and escaped brackets/parentheses and quoted titles.
  Recursion is bounded by the configured depth.
- Serialization uses a backtick run one longer than the content (at least
  three for blocks), preserves destination percent escapes, and uses only
  serializer-recognized backslash escapes. Attribute parsing is all-or-nothing:
  malformed quotes or duplicate/malformed tokens stay ordinary text.
- List items now retain nested lists and continuation blocks, including ordered
  starts. Reference links intentionally remain opaque source because the
  canonical AST does not yet represent their definition graph losslessly.
- The previous throwaway pulldown-cmark validation pass was removed. The
  source-aware codec is the parser of record.

## Fixed-point and hostile-input evidence

- The Markdown suite contains 20-cycle changed-node checks spanning paragraphs,
  headings, quotes, variable-fence code, ordered/nested lists, thematic and
  page breaks, alignment, all supported inline forms, escaped link/title data,
  and styled attributes.
- Permanent tests cover malformed attributes, opaque reference links, typed
  byte/depth/delimiter limits, code fences containing backtick runs, CRLF/empty
  front matter/EOF handling, comparison text, and the audit code-fence and
  alignment regressions.
- `cargo run -p parchmint-test-support --bin fuzz-smoke --offline -- 1000`
  completed. The deterministic xorshift seed is `0x9e37_79b9_7f4a_7c15` (the
  fixed seed in `fuzz-smoke.rs`); it exercises malformed Markdown and every
  package validator without panic.

## Exporter and transaction deltas

- File exports now render and validate to `PreparedExport`, a same-directory
  temporary artifact. Only `commit_prepared_export` mutates the requested path.
  `Fail` installs by atomic hard-link/no-replace; an appearance race returns
  `DestinationExists` and leaves the new file alone. `ReplaceFile` is exposed
  only through a QML confirmation dialog and commits only after validation.
- The bridge compares the worker `WorkStamp` with current generation/revision
  before commit. Stale/cancelled prepared artifacts are dropped. Projection-only
  `bump()` calls no longer cancel an export; editor deltas and explicit export
  replacement still cancel content-affecting work.
- The default UI PDF path compiles Rust IR in the worker, renders self-contained
  semantic HTML with the compiled Qt `QTextDocument`/`QPdfWriter` adapter into
  a destination-adjacent temporary file, validates it, then uses the common
  commit transaction. The portable Rust PDF remains only the explicit
  Qt-independent fallback used by direct compile-crate callers.
- Combined Markdown uses collision-resistant code fences, valid quoted titles
  and attributes, correct label/destination escaping, and nested list output.
  EPUB validation now resolves every local `src`/`href`, percent-decodes paths,
  checks fragments, and rejects archive escapes. DOCX allocates a separate
  numbering instance per list, retains nested run formatting, emits one
  paragraph style, and uses accurate supported image media types while visibly
  degrading unsupported images.
- ZIP fields are checked rather than saturated; an archive requiring ZIP64 is
  rejected before output, so a corrupt archive above 4 GiB is never emitted.

## Verification performed on the available Linux host

- `cargo fmt --all`, `git diff --check`, and
  `cargo test --workspace --exclude parchmint_bridge --offline`: passed.
  Rust results: 69 passed, 3 ignored, no failures (including 19 Markdown and 9
  compile tests).
- `cargo clippy -p parchmint-markdown -p parchmint-compile -p parchmint-app
  --all-targets --offline -- -D warnings`: passed.
- `cargo check -p parchmint_bridge --offline` with Qt 6.8.3 environment:
  passed. The only build note is the existing absence of mold/lld/gold.
- `just test` with `.toolchains/qt/6.8.3/gcc_64` and `.toolchains/tools/bin`:
  Rust tests and the native build passed. A follow-up
  `ctest --test-dir build --output-on-failure` passed all 3 tests (offscreen
  smoke, editor adapter, outline model).
- `cmake --build build --target qmllint`: passed with the repository's existing
  unqualified-delegate-access advisories and no QML syntax/type error.
- No Word/LibreOffice, EPUBCheck, desktop reader/browser, or real native PDF
  consumer was available in this environment. Those results are not inferred
  from structural validators.

## Remaining risks and deliberately unsupported constructs

- Reference-style Markdown links and definition graphs remain source-backed
  opaque blocks. Generic HTML, source-backed tables/footnotes, and unknown
  extensions likewise remain visible source rather than being guessed at.
- The UI-owner Qt PDF render has cancellation checks before rendering,
  validation, and commit, but a very large single `QTextDocument` render is not
  yet internally interruptible. Renderer traversal progress is similarly not
  surfaced at per-block granularity.
- DOCX preserves nested list child blocks but does not yet emit a full OOXML
  multi-level numbering hierarchy; this should be completed before claiming a
  consumer-tested nested-list fidelity matrix.
- The ZIP writer rejects ZIP64-sized archives rather than producing ZIP64.
  This meets the no-corrupt-output invariant but is a capacity limitation that
  should be replaced if >4 GiB exports become a product requirement.

Recommended next task: run the PDF/EPUB/DOCX consumer matrix on Windows,
macOS, and Linux; then add cooperative renderer traversal/progress and OOXML
multi-level numbering before relying on very large exports.
