# `parchmint-editor-iced`

**Purpose:** Implement `EditorAdapter` with a virtualized Iced widget backed by
one shared [editor-core](../parchmint-editor-core/README.md) session.

## Rendering and input

The widget draws semantic blocks with Iced canvas text primitives.
`EditorLayoutMetrics` and `BlockLayoutGeometry` supply a deterministic
proportional-width model for wrapping, hit testing, caret placement, selections,
comments, search highlights, and spelling underlines. Layout uses this model
rather than Iced's text layout pipeline.

```text
Iced input -> block and ParchMint position -> shared session command
  -> changed blocks -> next_frame updates mounted views -> draw visible geometry
```

The core maps each view's logical selection and shared comment anchors. Mounted
records own pixel scroll, viewport, focus, layout caches, search and spelling
decorations, and the active comment. Comment decorations refresh from shared
comments. The widget keeps no second editable document.

## Interface

`EditorIcedAdapter` implements [EditorAdapter](../parchmint-editor-api/README.md).
`MountedEditorBinding` connects a session and view to `MountedEditorHost`, which
supplies the widget. `EditorIcedConfig` sets projection, resource, and layout limits.
See [lib.rs](src/lib.rs) for signatures.

Host-facing methods cover session opening, presentation and snapshots, visible
blocks, frames and geometry, decorations, text input and paste, revisions, and
active styles. They return ParchMint values. `text_context` copies a bounded
caret-adjacent range, preserves scalar positions, and omits partial words without
building a whole-document text buffer or layout.

This adapter rejects `apply_composite_project_edit`; application-level global
replacement coordinates affected document sessions.

## Implementation limits

Prepared projections retain immutable revisions within the configured budget.
Canonical HTML and annotations serialize on request outside the adapter lock;
typing and rendering use semantic data.

The host supplies visible blocks to `cache_visible_blocks`, bounded by
`max_visible_blocks_per_view`; bindings start with the primary block. Geometry
materializes only scalars inside the viewport plus overscan. `next_frame` updates
changed cached blocks in every mounted view; each pane paints on its next frame.

The native shell releases other document hosts' focus when the active pane
changes. Caret formatting survives selection advances caused by typing; explicit
selection changes clear it.

Input currently targets normal en-US keyboards and preserves valid UTF-8. IME,
multilingual layout, bidirectional editing, and assistive-technology work remain
in [future work](../../docs/future-work.md).
