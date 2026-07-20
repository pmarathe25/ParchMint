# Stage 03 handoff: editor document lifecycle

Status: production Rust lifecycle and Linux native harness delivered. Linux
automated gates pass. The complete acceptance gate remains open for physical
IME/dead-key/native clipboard runs on all three platforms, Windows/macOS Qt CI,
and destructive real-device full-disk/process-kill persistence testing. The
deterministic fault boundaries are covered, but this handoff does not substitute
simulated `FullDisk`/`PermissionDenied` errors for target-filesystem evidence.

## Working-tree and architecture state

Stage 03 was implemented on the clean Stage 02 tree on 2026-07-19 and committed
under the subject `Implement editor document lifecycle`. No unrelated user
changes were present. ADR-0009 selects maintained pure-Rust YAML parsing;
ADR-0010 fixes the revision/journal/threading contract. ADRs 0001–0008 remain in
force and none was superseded.

Canonical Markdown/TOML remains authoritative. Qt Markdown/HTML conversion is
not used by the codec or save path. The additive `styles.toml` fields are
`kind` (`paragraph` default or `character`) and paragraph-only `next_style`.
Early format-1 projects missing the new stable Emphasis built-in receive it in
memory and persist it on their next ordinary save; the format version remains 1.

## Delivered behavior

- `parchmint-markdown` is now an editable, source-aware semantic AST with
  absolute spans and typed diagnostics. Unchanged blocks keep exact source;
  changed blocks use deterministic Markdown 1 spelling.
- Deprecated `serde_yaml` is removed. Markdown/storage use `noyalib` 0.0.15,
  preserve unknown mapping values, and retain the Stage 02 size/depth limits.
- `DocumentSession` owns semantic state, revision, saved/journaled revisions,
  dirty block ranges, source buffer/status, save state, canonical fingerprint,
  and undo epoch for one open document.
- Journals are mandatory before canonical save. Focus loss/clean shutdown force
  the debounce path. Autosaves use `OpenProject::set_body` and the new narrow
  `ProjectStorage::save_document` (so another document is never rewritten),
  create rotating canonical-source backups, acknowledge only after replacement,
  and compact fulfilled recovery.
- Recovery supports scan, fingerprint validation, preview, restore, discard,
  and atomic save-copy. Deterministic injection covers journal before/after
  replacement, canonical before backup/write and after write, stale work,
  simulated full disk, and simulated permission denial.
- External detection reads and fingerprints canonical bytes rather than
  timestamps. Clean sessions auto-reload; dirty sessions return an immutable
  conflict with local/external bodies for compare, reload, explicit overwrite,
  or save-copy.
- Raw mode owns a buffer and diagnostics. A hard parse failure retains the
  buffer and prevents WYSIWYG return. Mode transitions and external resolution
  increment an undo epoch.
- Styles have stable IDs, paragraph/character kind, inheritance, next-style,
  bounded properties, computed Qt-free appearance, safe display-name mutation,
  explicit same-kind delete/replacement, and lossless structural undo.
- The Qt adapter loads/emits semantic block maps, stores style/object identity
  in Qt user properties, renders page breaks/opaque blocks visibly, blocks
  formatting across protected objects, exposes mixed formatting/focus-aware
  undo state, groups commands, and emits revisioned dirty block ranges.
- QML includes a compact preview style picker, style manager/replacement flow,
  formatting toolbar, source editor with diagnostics, and native Markdown
  syntax highlighter. Keyboard bold/italic/undo/redo and semantic Backspace are
  wired in the isolated main-window harness.
- The CXX-Qt boundary accepts incremental dirty ranges and raw Markdown
  validation without requesting a full document serialization per keystroke.

## Supported Markdown matrix

| Construct | Semantic/editable | Deterministic reconstruction | Exact source when untouched |
|---|---:|---:|---:|
| YAML mapping/unknown keys | yes | storage-owned | yes |
| ATX headings + attributes | yes | yes | yes |
| Paragraphs | yes | yes | yes |
| bold/italic/strike/code | yes | yes | yes |
| super/subscript | yes | yes | yes |
| links/asset images | yes | yes | yes |
| paragraph/character style attributes | yes | yes | yes |
| ordered/unordered/task lists | yes | yes | yes |
| block quotes | source-aware supported | on later conversion | yes |
| fenced/indented code | yes | yes | yes |
| GFM tables | source-aware supported | on later conversion | yes |
| GFM footnotes | source-aware supported | on later conversion | yes |
| thematic/scene breaks | yes | yes | yes |
| alignment fenced divs | yes | yes | yes |
| page breaks | yes/protected | yes | yes |
| unknown fenced div/HTML/explicit opaque | protected opaque | no implicit conversion | yes |

The complete golden is `tests/fixtures/markdown/supported.md`. Current table,
blockquote, and footnote nodes are classified semantically but remain
source-backed rather than decomposed into independently editable child cells or
blocks. Editing one requires an explicit conversion in a future UI command;
ordinary WYSIWYG operations do not discard it.

## Opaque and malformed behavior

Unknown divs, HTML blocks, paragraphs with unsupported inline HTML, explicit
opaque fences, and malformed/unclosed fences/divs become protected source
objects. Duplicate anchors, duplicate top-level YAML keys, missing `style-id`,
and absent project style IDs are diagnostics without destructive normalization.
Unclosed/invalid front matter is a hard raw-source parse error. Opaque source is
removed only by the explicit semantic delete/conversion path.

## Editor host API

`EditorAdapter` is one instance per `QTextDocument` and exposes:

- document/cursor/selection/focus properties; mixed `boldState`/`italicState`;
  paragraph alignment/style; focus-aware `canUndo`/`canRedo`; and revision;
- `loadSemanticBlocks` / `semanticBlocks` at load/save boundaries;
- heading, alignment, list, link, asset image, named paragraph/character style,
  direct-format clear, bold/italic/super/subscript, scene/page break, opaque,
  grouped edit, paste, semantic delete, undo, and redo commands;
- `incrementalDirty(revision, position, removed, added, firstBlock,
  lastBlockExclusive)` and `focusLostFlushRequested(revision)`.

Stable style is `QTextFormat::UserProperty + 10`; opaque source is `+11`; image
alt text `+12`; protected marker `+13`; direct-format marker `+14`. Page break
and opaque object types are `UserObject + 1/+2`. A registered
`QTextObjectInterface` renders both rather than exposing an invisible object
replacement character.

## Threading and revision rules

The UI thread applies Qt cursor operations and emits small dirty ranges. One
serial Rust worker owns each open project's mutable `OpenProject` and document
sessions. Parsing/serialization, journal I/O, backup copy/rotation, canonical
save, and external reads run there. Work is immutable after submission and
carries `WorkStamp`. The worker checks a request immediately before mutation;
the UI/session checks completion again before changing saved/error state.

`Saved` means canonical replacement completed. `Journaling`, `Dirty`, `Saving`,
and `Error` never imply canonical durability. A current journal completion
advances only `journaled_revision`; only a current canonical completion advances
`saved_revision`. Project close increments generation. One serial project
worker preserves write order even when cooperative cancellation arrives late.

## Recovery and backups

The exact recovery format is documented in `docs/format/recovery-1.md`. Default
debounce is 750 ms and default rotation retains 10 backups per document under
`.parchmint/backups/<document-id>/<revision>.md`. The backup is the previous
complete canonical file including YAML metadata. Stage 02's idempotent
`pre-migration-v<version>` snapshots remain unchanged and happen before any
migration mutation.

## Verification and measurements

Linux environment: Ubuntu 26.04, Qt 6.8.3, Rust 1.97.1, the same i7-8550U/
7.1 GiB host recorded by Stage 01.

- `cargo fmt --all --check`: passed.
- `cargo clippy --workspace --all-targets --offline -- -D warnings` with pinned
  Qt environment: passed.
- `cargo test --workspace --exclude parchmint_bridge --offline`: passed (28
  tests passed, 2 manual measurements ignored at final verification).
- Full CMake build, QML lint, and `ctest --output-on-failure`: passed; native
  smoke plus editor/outline Qt tests all passed.
- `cargo test -p parchmint-app --release
  records_large_document_journal_and_save_latency --offline -- --ignored
  --nocapture`: 250,000 words; semantic load 33.935 ms; UI dirty scheduling
  0.675 µs; atomic journal 15.905 ms; backup plus canonical document save
  9.883 ms.
- Offscreen `parchmint-editor-benchmark`: 250,000-word load 30 ms; single
  insertion 64 µs; 2,000-character formatting 39 µs. Two panes loaded in 21 ms;
  500 tracked insertions p95 4.628 ms and p99 5.293 ms; save-boundary semantic
  snapshot 92 ms; exactly 500 incremental dirty signals.

The UI-thread dirty submission is below 8 ms and measured typing is below the
16/50 ms budgets on this older-than-reference host. Journal/serialization and
canonical work are worker-only. Real GPU paint latency and reference-corpus
memory remain Stage 09 trend work.

## Known defects, risks, and platform gaps

- Windows/macOS CI was not available. Native atomic replacement, physical IME,
  dead keys, platform-rich clipboard identities, bidirectional selection, and
  accessibility remain unrecorded there and on physical Linux input methods.
- Fault tests deterministically exercise every Stage 03 service boundary and
  stale ordering. They do not fill a real filesystem or kill a process between
  syscalls; run those destructive charters on disposable target volumes before
  calling the complete acceptance gate closed.
- The bridge/main window is an isolated editor harness over demo outline data;
  Stage 04 must bind real project/document selection to `DocumentSession`. The
  persistence service is complete below that pending binder integration.
- Qt rich HTML import drops active script in tests, but native office-suite
  clipboard flavors still need the platform matrix. Unsupported imported
  appearance must degrade to the supported semantic snapshot.
- `QTextDocument::contentsChange` requires a document layout in an isolated
  harness; the adapter eagerly creates one before connecting dirty tracking.
- Custom opaque/page-break rendering is not yet accessibility-reviewed with
  Narrator, VoiceOver, or Orca.

## Stage 04 prerequisites and first task

Use stable document/style IDs and consume Rust snapshots/events. Instantiate
one `DocumentSession` per open editor pane on the project worker, translate its
semantic blocks once on load and at explicit save boundaries, and feed
`incrementalDirty` ranges between them. Do not make binder QML own bodies or
call Qt Markdown serialization. The recommended first task is binder selection
opening a real `OpenProject` document into one adapter while preserving that
session's cursor, scroll, selection, undo stack, and generation/revision stamp.
