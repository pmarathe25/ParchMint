# `parchmint-workspace-state`

This crate saves the way each project workspace was arranged. It restores pane
widths, the split layout, collapsed sections, open tabs, the active view, scroll
positions, and the current workspace mode when the project opens again.

Workspace state is application data. ParchMint stores it outside the project
folder, so changing a pane or tab does not change project files, project undo,
or History.

## How it works

```text
workspace change
  -> update the in-memory workspace
  -> persist the settled workspace once the gesture ends
  -> save one versioned workspace file

open project
  -> load its workspace file
  -> remove references to missing project items
  -> restore the remaining layout and views
```

Each saved workspace belongs to one project identity. Several open projects can
save their workspace state independently.

## Interface

`WorkspaceStateStore` loads, saves, and removes `WorkspaceSnapshot` values by
project identity. `FileWorkspaceStateStore` implements the interface on disk.

See [the source](src/lib.rs) for method signatures.

The store uses project and node IDs. It does not store document text or other
authored content.

## Implementation

The crate writes one versioned file per project under the application's data
directory. It writes a temporary file, flushes it, and replaces the previous
file. The UI holds splitter movements in memory and persists the settled layout
once the pointer is released, so dragging a splitter does not write on every
pointer event.

If the workspace file is missing or invalid, ParchMint opens the project with
the default layout and reports the invalid file.
`FileWorkspaceStateStore::load_or_default` returns the default snapshot plus a
`WorkspaceWarning` for the invalid file, which is preserved for diagnosis. A
workspace save error does not change the project or prevent project saving.
