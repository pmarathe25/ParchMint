# Stage 05 handoff: research and split workspace

Status: implemented in the shared worktree. Rust app/storage tests pass on the
available Linux environment. Qt is not installed in this environment, so the
bridge/QML native build remains a required follow-up verification.

## Delivered behavior

- Research uses the existing research root and ordinary document/group node
  types. `create_research_node` requires a research parent; research nodes
  default to `include-in-compile = false`. An explicit metadata flag is still
  available for a future compile preset/UI to opt in.
- Research notes use the same Markdown parsing and atomic document writer as
  manuscript notes. The symmetric `PaneHost.qml` is instantiated twice; each
  host owns its own editor object, so changing/closing one pane does not reset
  the other host's Qt undo state.
- `ProjectWorkspace` supports opening/navigating panes, pinning, focus-next,
  swapping, closing, split orientation/ratio, and per-pane cursor/scroll
  hints. Binder navigation changes only the focused unpinned pane. Structural
  selection is still Rust-owned and remains independent of pane restoration.
- Attachment import copies a regular, non-symlink source into `assets/` under
  an `AssetId` UUID filename, then writes canonical `assets.toml`. The original
  display name is metadata only. There is intentionally no hash deduplication:
  matching bytes can legitimately be separate writer references.
- Attachment reference nodes live below research and have front matter
  `attachment: <asset UUID>`. They are safe textual previews only in the
  current QML implementation; an external open is a separate button and only
  receives a contained catalog path.

## Pane focus and command rules

`focused_pane` is 0 or 1. `selectNode` changes binder selection and attempts to
navigate that focused pane. A pinned focused pane accepts selection but rejects
navigation, preserving the reference. `openInOtherPane` enables the split and
places the selected node in the opposite host. Swap exchanges complete pane
state; close only resets the requested pane (and hides the split when pane 1
is closed). Formatting/edit commands must target the active `PaneHost`; Rust
structural commands remain selected-node operations.

## Workspace schema

`.parchmint/workspace.toml` is local state with independent `version = 1`:

```toml
version = 1
selected_nodes = ["<node UUID>"]
expanded_nodes = []
binder_visible = true
inspector_visible = true
focused_pane = 0
split_enabled = true
split_orientation = "horizontal" # or "vertical"
split_ratio_milli = 500

[[panes]]
node = "<node UUID>"
view = "editor" # editor, attachment, outline, cards
pinned = false
cursor = 0
scroll = 0
```

It also records optional window position/size/maximized state. On open, stale,
trashed, or externally removed IDs are removed, a ratio is clamped to 100–900,
and malformed/newer TOML produces a diagnostic plus a safe default. Workspace
failure never blocks canonical project open.

## Attachment threat model and previews

The importer rejects source symlinks, non-regular files, oversized files
(100 MiB), unsafe names/extensions, catalog traversal, symlinked `assets/`
components, and a pre-existing destination. It copies with a temporary file,
syncs, and uses no-clobber persist. Catalog load verifies each stored path,
size, and UUID-derived name. It never executes, parses as markup, or embeds
attachment content.

The catalog classifies PNG/JPEG/GIF/WebP/BMP as images, PDF as PDF, and common
plain text extensions as text. This Linux QML build currently shows passive
metadata and allows an explicit platform system-open action; native inline PDF
rendering is deferred until the platform Qt PDF capability is present. No
unknown/active type is embedded.

## Persisted-format changes

`assets.toml` is a new optional, independently versioned canonical catalog
(`version = 1`). `DocumentMetadata` now has optional `attachment` front matter.
`docs/format/project-format-1.md` documents both. No ADR was added; this is an
additive implementation of the already-approved assets directory contract.

## Verification and measurements

- `cargo fmt --all --check`: passed.
- `cargo test -p parchmint-app -p parchmint-storage --offline`: 21 passed, 2
  ignored.
- `cargo test -p parchmint-app --offline records_large_document_journal_and_save_latency -- --ignored --nocapture`: passed. One 250,000-word document measured load 369.5 ms, UI dirty notification 7.8 µs, journal 65.3 ms, canonical save 34.9 ms.
- `cargo test --workspace --offline`: blocked at `cxx-qt` build setup because
  this environment has no Qt installation (`QMAKE` not found). No Qt memory
  measurement with two live `QTextDocument`s is available here; run the native
  offscreen CTest/QML suite and RSS measurement on the pinned Qt 6.8.3 host.

## Recommended next task

On a Qt 6.8.3 host, build the bridge/QML module and exercise image/plain-text
native preview, the explicit system-open confirmation/context, and two live
large editor documents. Stage 06 can consume research nodes unchanged; it must
index attachment metadata only unless a safe text-extraction policy is added.
