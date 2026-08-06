# `parchmint-editor-iced`

## What it does

`parchmint-editor-iced` implements `EditorAdapter` as a custom virtualized
`iced` widget. It draws a document as independent semantic blocks and sends
editing commands to one shared ParchMint editor session.

The widget uses `iced` text primitives for layout, rendering, and hit testing.
It does not use `iced::widget::text_editor::Content` as the document model.

## How it works

```text
iced event
  -> find the affected block and ParchMint text position
  -> change the shared editor-core session
  -> invalidate changed blocks in mounted views
  -> lay out only visible changed blocks
  -> draw text, selection, caret, comments, and decorations from that layout
```

Each pane combines a logical view record in `parchmint-editor-core` with a
mounted widget record in this crate. The logical record holds selection and
local search. The mounted record holds scroll position, focus, viewport, and
layout cache. Both refer to the same editor-core session. An edit in either
pane changes the session once and appears in the other pane on its next frame.

The widget uses one layout result for drawing, pointer hit testing, caret
placement, selection rectangles, comments, search highlights, and spelling
underlines. This avoids each feature calculating slightly different text
geometry.

## Public API

The adapter is constructed without exposing `iced` or editor-engine types:

```rust
pub struct EditorIcedConfig {
    pub projection_budget: ProjectionBudget,
    pub resource_limits: EditorResourceLimits,
}

pub struct EditorIcedAdapter {
    inner: Box<dyn EditorIcedRuntime>,
}

impl EditorIcedAdapter {
    pub fn new(config: EditorIcedConfig) -> Result<Self, EditorStartupError>;

    pub fn create_view_host(
        &self,
        window: WindowCapability,
        view: ViewId,
    ) -> Result<ViewHostCapability, EditorError>;
}

impl EditorAdapter for EditorIcedAdapter {
    // Implements the engine-neutral public contract.
}
```

`EditorIcedRuntime` is private. The adapter returns ParchMint values only.

## Implementation

```rust
struct MountedView {
    view: ViewId,
    scroll: ScrollOffset,
    focus: FocusState,
    viewport: Viewport,
    visible_blocks: BlockLayoutCache,
}

fn apply_edit(
    session: &mut EditorSession,
    mounted: &mut HashMap<ViewId, MountedView>,
    command: EditorCommand,
) -> Result<()> {
    let change = session.execute(command)?;
    for view in mounted.values_mut() {
        view.visible_blocks.invalidate(change.changed_blocks());
    }
    Ok(())
}
```

The editor-core session owns and maps each view's logical selection and local
search positions. The mounted widget owns pixel scroll, focus, viewport, and
layout data. Its cache keeps geometry for visible blocks and a bounded overscan
area and removes layouts that leave that area. A change relayouts only the
blocks that the editor-core session reports as changed. Rendering uses `iced`
text primitives; the widget keeps no second editable document.

V1 accepts normal keyboard input for en-US writing. It keeps text as valid
UTF-8 and leaves the input and layout layers replaceable for later IME,
multilingual, bidirectional, and assistive-technology work. It does not include
placeholder implementations for those features.
