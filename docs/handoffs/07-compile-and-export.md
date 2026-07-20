# Stage 07 handoff: compile and export

Status: implemented in the shared worktree. The complete Rust and Qt desktop
test suites pass on the available Linux host with the pinned Qt 6.8.3 toolchain
under `.toolchains/qt/6.8.3/gcc_64`. Windows and macOS consumer validation
remain required before the cross-platform acceptance gate can be claimed.

## Delivered behavior

- `parchmint-compile` is a Qt-free compiler and exporter crate. It accepts an
  immutable `CompileInput` snapshot, never an editor document, and produces a
  deterministic format-neutral IR.
- A snapshot carries `WorkStamp` (project generation and revision). Progress
  callbacks carry the same stamp, cancellation is cooperative, and the app
  offers a serial `CompileExportWorker` that keeps compile/export work off the
  UI thread. The bridge must cancel work when its revision advances and reject
  non-current completions.
- Compile traversal is manuscript-then-research binder preorder. Empty selected
  roots mean manuscript. Research is excluded by default and becomes eligible
  only through an explicitly selected research root/node or the persisted
  `research = "all"` rule. Preview rows include selected content and explicit
  exclusions (research, selection, document flag, or emptiness).
- Rust resolves style inheritance through the domain model before IR emission.
  Generated project/document titles, scene separators, and page separators are
  semantic IR nodes; source Markdown is never changed.
- Presets are now typed, stable-ID, human-readable `styles.toml` records with
  selection, inclusion, title, separator, metadata, style mapping, page, and
  namespaced exporter settings. Preset create/update/delete runs through the
  domain command/undo path and survives reopen.
- All output is built and structurally validated before a same-directory atomic
  replacement. The default collision policy refuses existing destinations.
  Replacement retains the old file until the final atomic replace. Directory
  Markdown output intentionally refuses replacement because portable atomic
  directory replacement is not available.

## Compile IR/API

`CompileInput::from_open_project(opened, stamp)` copies project metadata,
bodies, safe asset references, and a revision stamp. `compile` returns
`(CompileIr, Vec<CompileWarning>)`; `compile_with_progress` adds monotonic
stamped progress; `preview` returns source order, exclusions, warnings, and
counts. `CompileIr` has a schema version, project/preset IDs, metadata/page
settings, copied safe asset paths, resolved style map, normalized blocks and
inlines, source provenance, and Unicode word/character/block counts.

Each block provenance is either generated (with semantic role and source node
where applicable) or exact Markdown node/document byte span. IR normalizes
paragraphs, headings, quotes, code, lists/tasks, tables, footnotes, alignment,
thematic/scene/page breaks, opaque blocks, links, images, styles, and inline
emphasis/strong/strike/code/super/subscript/breaks.

`ProjectWorkspace` exposes `compile_input`, `compile_preview`,
`compile_project`, `export_compiled`, and preset CRUD. `CompileExportWorker`
accepts only an immutable input/preset/options job and returns a typed stamped
completion.

The Qt bridge now has a Project-menu export destination/format dialog,
cancellation action, and timer-polled worker status. It uses the first saved
preset or a default manuscript preset. The richer editable preset picker and
ordered preview panel are application APIs awaiting final QML model
presentation.

## Export support matrix

| Semantic feature | Markdown | text | HTML | PDF fallback | EPUB | DOCX |
|---|---|---|---|---|---|---|
| Headings/titles, lists, links, Unicode | native | normalized text | native | basic; non-ASCII warning | XHTML | native OOXML |
| Bold/italic/strike, super/subscript | native | text only | native | text fallback | XHTML | native OOXML |
| Named styles/alignment | ParchMint attributes | text only | CSS | basic text fallback | CSS | styles/justification |
| Images | `asset:` reference | alt text | embedded or safe relative | alt text warning | packaged images | PNG/JPEG package images |
| Scene/page breaks | thematic/marker | `***`/form feed | semantic `hr` | pages/basic marker | XHTML/CSS | native break |
| Tables, footnotes, opaque source | preserved source | visible source | preformatted warning | text warning | preformatted warning | visible text warning |

The portable Rust PDF fallback validates a real PDF but has an explicit
non-Latin shaping degradation warning. `crates/parchmint-bridge/src/pdf_renderer.cpp`
is the minimal Qt adapter that takes normalized text/page settings through a
fresh `QTextDocument` and `QPdfWriter`; platform smoke work should route the
PDF UI through it for shaped fonts, headers/footers, images, and richer styles.
PDF headers/footers and full image/style projection are deliberately not yet
wired through that small adapter.

DOCX decision and constraints are recorded in
[ADR-0011](../architecture/0011-docx-writer-boundary.md). Pandoc is never a
silent fallback.

## Validators and deterministic tests

- `validate_html` checks the generated UTF-8 HTML5 shell and rejects active
  script output.
- `validate_pdf` checks PDF header, cross-reference and EOF structure.
- `validate_epub` parses the generated ZIP central directory and validates
  `mimetype`, container, OPF/spine, nav, and XHTML package parts.
- `validate_docx` validates required OOXML parts, main document shape, and
  content-type declaration.
- EPUB/DOCX are deterministic uncompressed ZIPs: lexical entry order, fixed
  DOS timestamps, fixed OOXML core timestamps, stable relationship order, and
  no random export IDs. Golden tests compare bytes directly; real metadata is
  not altered at user export time except those intentionally fixed package
  fields.
- Tests cover nested/multi-root selection order, research exclusion, opaque
  warnings, cancellation, every format validator, byte determinism, and an
  existing-destination collision that proves old contents remain intact.

## Verification

- `just test`, with `QMAKE`, `PATH`, `LD_LIBRARY_PATH`, and
  `CMAKE_PREFIX_PATH` pointed at `.toolchains/qt/6.8.3/gcc_64`: passed. It ran
  45 Rust tests (3 intentional manual-measurement ignores), built the CXX-Qt
  desktop executable, and passed all three offscreen CTest cases.
- `just lint` with the same Qt environment: passed (`cargo clippy --workspace
  --all-targets -- -D warnings` and generated-module `qmllint`). Qt reports
  existing unqualified-access QML advisories; no errors were emitted.
- `cargo fmt --all --check` and `git diff --check`: passed after all Stage 07
  edits.
- `cargo check -p parchmint_bridge --offline` with the pinned Qt environment:
  passed. The toolchain warns that a faster linker is unavailable and falls
  back to GNU `ld.bfd`; this did not prevent the build or CTest suite.
- `g++ -std=c++17 -fsyntax-only` against the pinned Qt Core/Gui headers for
  `pdf_renderer.cpp`: passed. `qmllint Main.qml` parsed the added dialog; it
  reports the existing expected unresolved generated `org.parchmint.*` imports
  when run outside the CXX-Qt build directory.

## Persisted/public changes

- `CompilePreset` changed from the Stage 02 placeholder to the typed schema
  described in `docs/format/project-format-1.md`. The legacy `settings` map is
  still deserialized and preserved.
- New domain commands/events: `UpsertCompilePreset`, `RemoveCompilePreset`,
  `CompilePresetSaved`, and `CompilePresetRemoved`.
- New compile/export public crate and app worker interfaces are Qt-independent.

## Known risks and recommended next task

- Perform the stated Windows/macOS/Linux validation with Word, LibreOffice,
  EPUBCheck, browser/reader engines, and native Qt 6.8.3. The in-tree ZIP/XML
  validators are structural, not replacements for those consumer validators.
- Complete bridge/QML preset picker plus ordered preview rows/warnings, and
  wire Qt PDF rendering for full headers, footers, images, and named style
  fidelity. Keep app worker stamps/cancellation in the UI integration.
- Add broad external golden fixtures (missing asset, full disk/failure
  injection, all image types, complex tables/footnotes, and consumer-opened
  DOCX/EPUB) in Stage 08 without weakening the existing tests.
- Font fallback and PDF pagination differ by operating system. The Qt adapter
  must use target-installed fonts and report substitutions; pixel identity is
  not a product requirement.
