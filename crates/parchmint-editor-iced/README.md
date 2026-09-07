# `parchmint-editor-iced`

`parchmint-editor-iced` implements `EditorAdapter` as a custom virtualized
`iced` widget. It draws a document as independent semantic blocks and sends
editing commands to one shared ParchMint editor session.

The widget paints each scalar with `iced` canvas text primitives. Layout,
wrapping, hit testing, and caret geometry come from the crate's own
deterministic proportional-width model (`EditorLayoutMetrics` and
`BlockLayoutGeometry`), not from iced's text pipeline. It does not use
`iced::widget::text_editor::Content` as the document model.

## How it works

```text
iced event
  -> find the affected block and ParchMint text position
  -> change the shared editor-core session
  -> invalidate changed blocks in mounted views
  -> lay out only visible changed blocks
  -> draw text, selection, caret, comments, and decorations from that layout
```

Each pane combines a logical view record in `parchmint-editor-core` (the
view's selection) with a mounted record in this crate (pixel scroll, focus,
viewport, layout cache, search and spellcheck decorations, and the active
comment). Both refer to the same editor-core session. An edit in either pane
changes the session once; the next `next_frame` relayouts the changed blocks
in every mounted view, and the other pane observes them on its next frame.

The widget uses one layout result for drawing, pointer hit testing, caret
placement, selection rectangles, comments, search highlights, and spelling
underlines. This avoids each feature calculating slightly different text
geometry.

## Interface

`EditorIcedAdapter` implements `EditorAdapter`. `MountedEditorBinding` connects
a session and view to a `MountedEditorHost`; the host supplies the Iced widget.
`EditorIcedConfig` controls projection, resource, and layout limits.

See [the source](src/lib.rs) for method signatures.

The adapter keeps session and mounted-view records private. Beyond
the `EditorAdapter` methods, the adapter exposes the host-facing entry points
the mounted surface and binding use: `open_session`, `set_view_presentation`,
`view_snapshot`, `cache_visible_blocks`, `next_frame`, `geometry`,
`spellcheck_decorations`, `comment_decorations`, `set_active_comment_decoration`,
`input_en_us`, `paste_untrusted`, `paste_untrusted_at`,
`paste_untrusted_plain_at`, `revision`, `active_style`, and
`primary_visible_block`. All return ParchMint values only.

`apply_composite_project_edit` is part of the adapter contract but is rejected
by this implementation; composite project replacement is dispatched by the
application layer, not the editor.

## Implementation

The adapter retains immutable prepared projections within its revision budget.
It serializes canonical HTML and annotations when a projection is requested,
outside the adapter lock. Typing and rendering use the prepared semantic data.
The native shell releases input focus on other document hosts when the active
pane changes. Caret formatting survives the automatic selection advance after
each typed character; explicit selection changes reset that pending format.

The editor-core session owns and maps each view's logical selection (and the
session's comment anchors). The mounted record owns pixel scroll, focus,
viewport, and the per-view layout cache plus search and spellcheck decorations;
comment decorations are derived from the shared canonical comments on each
refresh. The host owns the visible-block set: `cache_visible_blocks` retains
layouts only for the blocks the host supplies (the binding initializes with
the session's primary block), and the count is bounded by
`max_visible_blocks_per_view`. Each geometry materializes only the scalars
inside the viewport's scroll window plus overscan. Pending changed blocks are
applied to every mounted view's cached geometry at the next `next_frame`, so a
change relayouts only the blocks that the editor-core session reports as
changed. Rendering paints the per-scalar geometry with `iced` canvas text
primitives; the widget keeps no second editable document.

V1 accepts normal keyboard input for en-US writing. It keeps text as valid
UTF-8 and leaves the input and layout layers replaceable for later IME,
multilingual, bidirectional, and assistive-technology work. It does not include
placeholder implementations for those features.

`EditorIcedAdapter::text_context` copies a bounded range around a view's caret
for background text services. It preserves scalar positions and omits partial
words without constructing a whole-document layout or text buffer.
