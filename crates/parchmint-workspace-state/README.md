# `parchmint-workspace-state`

**Purpose:** Restore each project's panes, widths, collapsed sections, tabs,
active view, scroll positions, and workspace mode. Files are keyed by project
identity under the application-data directory and contain no authored text.

## Interface

`WorkspaceStateStore` loads, saves, and removes `WorkspaceSnapshot` values.
`FileWorkspaceStateStore` implements it on disk. `load_or_default` returns a
snapshot and any `WorkspaceWarning`. See [lib.rs](src/lib.rs).

## Persistence and recovery

The UI retains splitter movement in memory and saves the settled layout on
pointer release. The store writes a versioned temporary file, flushes it, and
replaces the previous file. Separate projects save independent layouts.

Opening drops references to missing project items and restores the remaining
views. A missing file uses the default layout. An invalid file is preserved for
diagnosis and returns a warning with the default layout.

Workspace changes and write failures affect neither project files nor project
undo, History, or project saving.
