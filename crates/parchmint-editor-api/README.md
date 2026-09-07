# `parchmint-editor-api`

**Purpose:** Define the rich-text editor contract through ParchMint types.
Application code opens sessions, attaches views, runs commands, observes changes,
and requests project-file snapshots through this interface.

## Session and view ownership

One `SharedEditorSession` owns an open document's text, styles, comments, anchors,
revisions, and undo. Both panes share that session. Each view has its own cursor,
selection, local search, scroll, viewport, and focus; the concrete adapters divide
logical selection from mounted-widget geometry.

Commands include the revision they observed. A valid command applies once, maps
view positions and anchors, and advances the revision. Undo from either pane uses
the shared document history.

A `CanonicalProjection` is a deterministic project-file snapshot of one revision:
body, comments, anchors, word count, and semantic blocks. Editing can continue
while it is built. Requests outside the retained revision budget fail explicitly.

## Interface

`EditorAdapter` manages sessions, views, commands, selection, clipboard values,
and exact-revision projections. `DurableProjectionBatch` pairs a projection with
its persistence revisions. `CanonicalComment` converts losslessly to and from
annotation contracts, preserving unknown fields. `style_id_from_canonical`
resolves semantic style names and stable IDs for commands and layout.

`ViewHostCapability` is an opaque identity for a mounted view; callers cannot
inspect its GUI handle. `SelectionGeometry` positions comment and spelling menus.
Search and spelling decorations are disposable per-view state.

`close` is idempotent: it detaches views and emits `Closed`; later session
operations return `EditorError::Closed`. See [lib.rs](src/lib.rs) for command,
event, view, capability, persistence-token, and error types.

## Implementation boundary

[Editor core](../parchmint-editor-core/README.md) owns sessions, transactions,
logical views, and projection queues. [Editor Iced](../parchmint-editor-iced/README.md)
owns mounted widgets. Engine documents, transactions, selections, render trees,
and storage types stay behind those adapters. Application persistence owns the
journal, saves, and acknowledgements of durable revisions.
