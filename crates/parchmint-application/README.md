# `parchmint-application`

This crate runs ParchMint actions and sends each edit to the correct undo list.

Project commands change hierarchy, display titles, Synopsis, metadata, styles,
the project dictionary, export settings, or several documents through global
replacement. They use project undo.

`EditorAdapter::execute` handles prose, formatting, title, comment, and anchor
edits. If a command targets an unopened document, the application opens a shared
editor session without attaching a visible view, then sends the command to that
session.

The crate calls the service interfaces for saving, recovery, History, and
editor work. The desktop executable supplies the concrete service
implementations. The application-owned `EditorPersistenceCoordinator` can be
constructed with injected save and recovery services. The production desktop
graph retains one coordinator and one serial save worker beneath each exact
project lease.

## How it works

```text
authoring intent
      |
      +--> project command --> project state + project undo
      |
      +--> document command -> shared editor session + document undo
      |
      +--> text-field input --> focused control's native undo
      |                        -> project or document command on commit
      |
      +--> changed resources -> revisioned save request
```

The UI uses the focused control to choose the command. A text field keeps native
undo while the user is typing, then commits its value through the appropriate
project or document command. This prevents a text field from bypassing project
or document undo.

## Interface

`NativeProjectCommandDispatcher` applies project commands and workflows.
`NativeDocumentStateOwner` manages loaded and lazy document state.
`ProjectPersistenceCoordinator` and `EditorPersistenceCoordinator` connect
revisioned snapshots to save and recovery services.

See [the source](src/lib.rs) for method signatures.

Long-running application methods return a future, stream, task handle, or event
receiver. Application state stays synchronous behind mutexes, and the actual
file, History, and index work happens inside the service crates (for example
the serial save worker in `parchmint-save`). Public methods use ParchMint types
instead of types from the storage libraries.

`EditorPersistenceCoordinator` owns projection-to-recovery routing, the
receipt/frontier acknowledgement boundary, bounded repeated-save coalescing,
the recovery journal and save coordinator handles, and the public
Saved/Dirty/Error frontier. It does not assemble the desktop service graph;
`parchmint-desktop` does.

## Implementation

Project undo retains up to 100 complete operations and 64 MiB of inverse data
in memory. Eviction removes whole operations, and a new project command clears
redo. A larger inverse is not retained after eviction; moving it to a temporary
session file is not implemented.

Undo and redo create new project or document revisions and save like any other
edit. Closing and reopening a project clears its interactive undo lists. A
whole-project restore, format migration, or accepted recovery also clears the
project and document undo lists before editing continues.

If a command fails validation, the application leaves the project and undo list
unchanged. After a save failure, the accepted edit remains unsaved and recovery
continues to protect it. A search, word-count, or other rebuildable service can
report outdated data; that error does not change the project.

Global replacement revalidates every selected match, prepares the complete
forward and inverse patches, applies them atomically through
`DocumentStateOwner::apply_composite`, records the project-command boundary on
each affected session without touching document undo, and stages all affected
files in one save transaction. It publishes the new project state only when all
in-memory changes succeed. A later save failure leaves the complete replacement
dirty and protected by recovery.
