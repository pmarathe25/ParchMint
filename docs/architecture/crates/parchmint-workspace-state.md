# `parchmint-workspace-state`

## What it does

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
  -> wait briefly for related changes
  -> save one versioned workspace file

open project
  -> load its workspace file
  -> remove references to missing project items
  -> restore the remaining layout and views
```

Each saved workspace belongs to one project identity. Several open projects can
save their workspace state independently.

## Public API

```rust
pub type WorkspaceFuture<'a, T> =
    Pin<Box<dyn Future<Output = T> + Send + 'a>>;

pub trait WorkspaceStateStore: Send + Sync {
    fn load(
        &self,
        project: ProjectIdentity,
    ) -> WorkspaceFuture<'_, Result<Option<WorkspaceSnapshot>, WorkspaceError>>;

    fn save(
        &self,
        project: ProjectIdentity,
        snapshot: &WorkspaceSnapshot,
    ) -> WorkspaceFuture<'_, Result<WorkspaceRevision, WorkspaceError>>;

    fn remove(
        &self,
        project: ProjectIdentity,
    ) -> WorkspaceFuture<'_, Result<(), WorkspaceError>>;
}

pub struct WorkspaceSnapshot {
    pub layout: PaneLayout,
    pub explorer: ExplorerWorkspaceState,
    pub tabs: Vec<OpenTabState>,
    pub active_view: Option<ViewId>,
    pub views: BTreeMap<ViewId, SavedViewState>,
    pub mode: WorkspaceMode,
}
```

The store uses project and node IDs. It does not store document text or other
authored content.

## Implementation

The crate writes one versioned file per project under the application's data
directory. It writes a temporary file, flushes it, and replaces the previous
file. Workspace changes can be grouped for a short time so dragging a splitter
does not write on every pointer event.

```rust
fn restore(saved: WorkspaceSnapshot, project: &Project) -> WorkspaceSnapshot {
    saved.remove_missing_nodes(project.node_ids())
}
```

If the workspace file is missing or invalid, ParchMint opens the project with
the default layout and reports the invalid file. A workspace save error does not
change the project or prevent project saving.
