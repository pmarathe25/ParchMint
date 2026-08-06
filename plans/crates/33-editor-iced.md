# Editor Iced

## Goal

Implement the proven custom virtualized Iced editor adapter.

## Depends on

- [32 Editor feasibility](../integration/32-editor-feasibility.md)

## Owning crate(s)

[`parchmint-editor-iced`](../../docs/architecture/crates/parchmint-editor-iced.md)

## Requirements and UI design

- [Editor panes and tabs](../../docs/product/editor-panes-and-tabs.md)
- [Rich text and semantic styles](../../docs/product/rich-text-and-semantic-styles.md)
- [Editor and tabs](../../docs/ui-design/editor-and-tabs.md)

## Work

- Mount core sessions, own pixel scroll/focus/viewport/layout caches, use one layout geometry for drawing and interaction, and expose only ParchMint editor values.

## Stage-specific tests and validation

Verify visible-block cache bounds, changed-block relayout only, two-pane next-frame propagation, caret/selection/hit-test geometry agreement, and normal en-US keyboard/clipboard behavior on all platforms.
