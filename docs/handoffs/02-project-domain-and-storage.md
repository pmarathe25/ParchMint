# Stage 02 handoff: project domain and storage

Status: implementation delivered with Linux Rust verification. The complete
acceptance gate remains open: this handoff does not claim platform-specific Qt
verification, true crash/termination fault injection across a multi-file save,
or a maintained replacement for the Stage 01 deprecated YAML dependency.

## Working-tree state

Stage 02 extends the clean Stage 01 implementation on 2026-07-19. It adds
UUID/TOML dependencies and canonical project files, with no edits to the Qt
editor implementation or its Stage 01 bridge boundary.

## Delivered behavior

- `parchmint-domain` now provides UUID-backed `ProjectId`, `NodeId`,
  `DocumentId`, `StyleId`, `AssetId`, and `CompilePresetId`; a Qt-free project
  graph; document metadata; built-in manuscript/research roots and styles;
  compile-preset and workspace placeholders; and validated structural commands.
- `ProjectCommand` supports create, rename, reorder, reparent, duplicate,
  trash, restore, metadata edits, and style mutations. Each execution returns
  `ProjectEvent` values and a Qt-independent `StructuralUndo` inverse.
- Graph validation checks required roots, parent/child agreement, one active
  membership, deterministic children, path safety, tombstone references, and
  cyclic style inheritance. Generated 100-step structural test sequences check
  invariants after each mutation.
- `parchmint-storage::ProjectStorage` creates, opens (read/write or read-only),
  validates, saves, closes, and reopens real project directories. `OpenProject`
  applies structural commands while preserving/initializing Markdown body and
  unknown-front-matter state.
- Writes are same-directory temporary writes with file flush, replacement, and
  Unix directory flush. A write advisory lock under `.parchmint/open.lock`
  allows concurrent read-only opens. Path resolution rejects lexical traversal
  and symlinks in all project-relative path components.
- Trashed Markdown documents are canonical `trash/<node-id>.md`; tombstones
  are `trash/<trashed-root-id>.toml`. `.parchmint` can be removed without
  losing openable data.
- V1 detection is a no-op migration. Newer formats fail before mutation. Any
  older format creates idempotent `pre-migration-v<version>` canonical backup
  before reporting that no old-format migration exists yet.
- The deterministic v1 schema, limits, compatibility rules, JSON-Schema-like
  artifact, and a normal-text-editor example project live in `docs/format/`
  and `examples/harbor-lights/`.

## Format and public APIs

Format version: `1`.

- Schema documentation: `docs/format/project-format-1.md`
- Machine-readable artifact: `docs/format/project-format-1.json`
- Files: `parchmint.toml`, `outline.toml`, `styles.toml`,
  `manuscript/<node-id>.md`, `research/<node-id>.md`,
  `trash/<node-id>.md`, and `trash/<trashed-root-id>.toml`.
- Existing Markdown syntax remains `docs/format/parchmint-markdown-1.md`.

Stage 03 should use `OpenProject::body` / `set_body` and
`ProjectStorage::save`, rather than changing document paths or serializing
front matter itself. It must preserve unknown YAML mapping entries, retain the
required `document_id`, and leave `outline.toml` free of titles and summaries.
It must not serialize editor HTML/Qt Markdown as canonical source.

## Filesystem and migration guarantees

The storage layer only accepts normal, relative path components and rejects
existing symlinks. It bounds manifests at 4 MiB, front matter at 256 KiB and 64
levels, and documents at 64 MiB. Canonical data is acknowledged after each
individual file's atomic replacement; a complete cross-file transaction/journal
is intentionally deferred to Stage 03 recovery work. Migration backup occurs
before a future canonical migration mutation and retry observes the same backup.

## Verification

On Linux, with Cargo offline cache available:

- `cargo fmt --all`: passed.
- `cargo clippy --all-targets --offline -- -D warnings`: passed for the default
  Rust workspace members.
- `cargo test --workspace --exclude parchmint_bridge --offline`: passed: 15
  tests, 1 Stage 01 manual benchmark ignored.
- Targeted storage tests passed for create/save/close/reopen, structural
  create/duplicate/trash/restore, read-only access with writer lock, symlink
  rejection, newer-version refusal, atomic replacement, and the hand-authored
  project.

`cargo clippy --workspace --all-targets --offline -- -D warnings` could not be
run in this shell because CXX-Qt cannot locate Qt (`QMAKE`/Qt SDK absent). This
is an environment evidence gap, not a Rust compile failure.

## Known defects, risks, and recommended next task

- `serde_yaml` remains in `parchmint-markdown` and storage because no maintained
  YAML replacement was available in the pre-populated offline registry. Before
  the format is permanently frozen, Stage 03 should replace it with an audited,
  maintained parser and retain the storage bounds/unknown-key behavior.
- Multi-file save has atomic files, not an all-or-nothing project transaction;
  Stage 03 owns revisioned journal/recovery semantics. Add injected failures at
  each multi-file phase there.
- `Duplicate` currently preserves metadata/body but Stage 03 must copy semantic
  Markdown AST state through its canonical codec before editor-driven duplicate
  is exposed.
- Qt bridge UI commands remain the Stage 01 demonstration only. Stage 04 should
  add binder-facing commands over the established domain snapshot/event APIs,
  never a mutable QML project model.

Recommended first task for Stage 03: adopt the `OpenProject` lifecycle around a
single editor document, replace the YAML parser, then add revisioned autosave
journals before scheduling `ProjectStorage::save`.
