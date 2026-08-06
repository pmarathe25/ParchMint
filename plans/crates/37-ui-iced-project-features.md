# UI Iced project features

## Goal

Implement the remaining project-facing views and flows in the Iced UI.

## Depends on

- [09 History Git2](09-history-git2.md)
- [14 Search SQLite](14-search-sqlite.md)
- [16 Export HTML](16-export-html.md)
- [20 Application](20-application.md)
- [25 Workspace state](25-workspace-state.md)
- [36 UI Iced editor](36-ui-iced-editor.md)

## Owning crate(s)

[`parchmint-ui-iced`](../../docs/architecture/crates/parchmint-ui-iced.md)

## Requirements and UI design

- [Explorer and hierarchy](../../docs/product/explorer-and-hierarchy.md)
- [Synopsis and metadata](../../docs/product/synopsis-and-metadata.md)
- [Cards](../../docs/product/cards.md)
- [Search and replacement](../../docs/product/search-and-replacement.md)
- [History and snapshots](../../docs/product/history-and-snapshots.md)
- [Deletion and Recently Deleted](../../docs/product/deletion-and-recently-deleted.md)
- [Appearance](../../docs/product/appearance.md)
- [Export](../../docs/product/export.md)
- [Save, recovery, and closing](../../docs/product/save-recovery-and-closing.md)
- [Explorer, Inspector, and comments](../../docs/ui-design/explorer-inspector-and-comments.md)
- [Cards UI](../../docs/ui-design/cards.md)
- [Search and replace UI](../../docs/ui-design/search-and-replace.md)
- [History and Recently Deleted UI](../../docs/ui-design/history-and-recently-deleted.md)
- [Settings and appearance](../../docs/ui-design/settings-and-appearance.md)
- [Export and save states](../../docs/ui-design/export-and-save-states.md)
- [Empty, loading, error, and recovery states](../../docs/ui-design/empty-loading-error-recovery.md)
- [Screen catalog](../../docs/ui-design/screen-catalog.md)

## Work

- Add Explorer/Cards commands, global search and replacement preview, History and Recently Deleted views, project settings, appearance, export, and save/recovery states.

## Stage-specific tests and validation

Run requirement-linked Light/Dark fixtures for each view, hierarchy selection/drag rules, replacement preview indeterminate states, History restore confirmation, Recently Deleted restoration, appearance propagation, and export/save error states.
