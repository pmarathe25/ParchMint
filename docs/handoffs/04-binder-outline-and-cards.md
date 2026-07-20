# Stage 04 handoff: binder, outline, and cards

Status: implemented and Linux automated checks pass. The Stage 04 workspace is
now backed by one Rust `ProjectWorkspace`; QML projects immutable rows and
submits commands only through the bridge. No canonical project data is owned by
Qt/QML.

## Delivered behavior

- Create, open, close, and error-reporting project shell flows are exposed by
  `ParchMintBackend`. `RecentProjects` is a bounded, caller-configured,
  noncanonical TOML store ready for the platform settings host.
- `ProjectWorkspace` persists every acknowledged structural/metadata command
  through `ProjectStorage::save`, maintains Rust structural undo/redo, and
  preserves trash tombstones/files until explicit future empty-trash work.
- The immutable `BinderSnapshot` carries stable IDs, parent, depth, group/root,
  title, synopsis, status, first label, current/placeholder count, and compile
  flag. A QML `OutlineModel` only requests rows on demand and resets when the
  Rust revision changes.
- Create, rename, duplicate, before/after/inside move, reparent, move up/down,
  indent/outdent, trash, restore, and metadata edits route to validated domain
  commands. Failed cycle/validation operations do not refresh the projection,
  so no optimistic residue is retained.
- Filtering retains matched rows and every ancestor. Title/status sorting is a
  projection-only operation; canonical sibling order remains untouched.
- `BinderPane`, `OutlineView`, `CardsView`, and `InspectorPane` are virtualized
  `ListView`/`GridView` components. The outline edits title/synopsis/status/
  label in place; cards are ordered compact summaries; inspector edits the same
  selected document metadata.
- Structural actions have menu and shortcut routes: `Ctrl+Shift+Up/Down` move,
  `Ctrl+[`/`Ctrl+]` outdent/indent, Delete trash, standard Undo/Redo, and the
  binder context menu. Action names are exposed as accessible strings.

## Tree and selection semantics

`BinderSnapshot` is fully replaced only after an acknowledged command or a
view-projection change. Its row identity is the domain `NodeId`, never a Qt
index. Selection is a deduplicated list of active node IDs; stale/trashed IDs
are discarded on open. A filter leaves selection intact even when its row is
not visible. A single selection supplies inspector metadata; zero or multiple
selection deliberately has no ambiguous inspector value.

`DropPlacement::Before`, `After`, and `Inside` resolve parent/index in Rust.
Same-parent after-target movement compensates for removal before the domain
command. The domain rejects roots, trashed nodes, bad indexes, and cycles.

## Workspace and format

`<project>/.parchmint/workspace.toml` stores optional selected/expanded IDs,
view name, and pane visibility. It is best-effort only and is never read as
canonical validity. No format version or canonical Markdown/TOML schema changed.
No ADR was added or superseded.

## Verification and measurements

On the existing Ubuntu 26.04 / Qt 6.8.3 development environment:

- `cargo fmt --all --check`: passed.
- `cargo test -p parchmint-app --offline`: 11 passed, 2 ignored manual
  measurements.
- `cargo check -p parchmint_bridge --offline` with the pinned Qt environment:
  passed.
- CMake native build and `QT_QPA_PLATFORM=offscreen ctest --output-on-failure`:
  3/3 passed (smoke, editor adapter, outline model).
- QML lint completed; remaining diagnostics are only QML's informational
  unqualified-access notices in delegates.
- The Rust stress test constructs and projects 10,000 user nodes in under one
  second and verifies a bounded 40-row visible slice. This is a conservative
  unit budget, not a GPU 60-FPS measurement.

## Remaining integration / risks

- Stage 03's `DocumentSession` is not yet instantiated by the Stage 04 bridge
  when a binder row is opened. The shell identifies selected documents but the
  central editor intentionally does not serialize QML text as a substitute.
  Stage 05 must wire selection to one real session/adapter while preserving the
  Stage 03 revision and cursor contract.
- Ordered drag target affordances are represented by the bridge placement API;
  the current compact QML uses context/menu and keyboard movement rather than a
  polished visual drop indicator. Accessibility and physical platform drag
  validation remain Stage 09 work.
- Recent-project storage needs the platform application-settings path supplied
  by the eventual native settings host. Window geometry is likewise presentation
  state and not canonical data.

## Stable components for Stage 05

Reuse `ProjectWorkspace`, `BinderSnapshot`, `OutlineModel`, `BinderPane.qml`,
`OutlineView.qml`, `CardsView.qml`, and `InspectorPane.qml`. Keep all new
research/split-pane selection work on the same snapshot/command boundary; do
not add a competing QML project model.
