# UI Iced editor

## Goal

Integrate the proven editor experience into the Iced workspace.

## Depends on

- [33 Editor Iced](33-editor-iced.md)
- [34 Editor save and recovery integration](../integration/34-editor-save-recovery-integration.md)
- [35 Spellcheck engine evaluation and implementation](35-spellcheck-en-us.md)

## Owning crate(s)

[`parchmint-ui-iced`](../../docs/architecture/crates/parchmint-ui-iced.md)

## Requirements and UI design

- [Editor panes and tabs](../../docs/product/editor-panes-and-tabs.md)
- [Formatting toolbar](../../docs/product/formatting-toolbar.md)
- [Comments and annotations](../../docs/product/comments-and-annotations.md)
- [Search and replacement](../../docs/product/search-and-replacement.md)
- [Spellcheck](../../docs/product/spellcheck.md)
- [Word counts](../../docs/product/word-counts.md)
- [Editor and tabs](../../docs/ui-design/editor-and-tabs.md)
- [Explorer, Inspector, and comments](../../docs/ui-design/explorer-inspector-and-comments.md)
- [Search and replace UI](../../docs/ui-design/search-and-replace.md)
- [Spellcheck UI](../../docs/ui-design/spellcheck.md)
- [Screen catalog](../../docs/ui-design/screen-catalog.md)
- [Word counts](../../docs/ui-design/word-counts.md)

## Work

- Add pane/tab management, toolbar targeting, local Find/Replace, comments, Inspector context, editor decorations, and editor-scoped asynchronous messages.

## Stage-specific tests and validation

Run dual-pane and same-document visual fixtures, focused-pane command/undo routing, independent local search, comment-anchor navigation, spelling-menu geometry, tab overflow, visible focus checks in Light and Dark, and status-bar tests proving selection count, active-document count, and Manuscript total behavior.
