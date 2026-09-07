# `parchmint-application`

**Purpose:** Apply writing actions, route undo, and coordinate saves through
ParchMint service interfaces. The [desktop](../parchmint-desktop/README.md)
supplies concrete services and retains one persistence coordinator and serial
save worker per project lease.

## Command routing

| Action | Owner and undo scope |
| --- | --- |
| Hierarchy, display titles, Synopsis, metadata, styles, project dictionary, export settings, global replacement | Project commands and project undo |
| Prose, formatting, content title, comments, anchors | `EditorAdapter::execute` and document undo |
| Text-field input | The focused control's undo until it commits a project or document command |

A command for an unopened document opens a shared editor session without a
visible view. Rejected commands leave project state and undo unchanged. Undo
and redo create new revisions and use the normal save path.

## Interface

- `NativeProjectCommandDispatcher` applies project commands and workflows.
- `NativeDocumentStateOwner` manages loaded and lazy document state.
- `ProjectPersistenceCoordinator` and `EditorPersistenceCoordinator` connect
  revisioned snapshots to injected save and recovery services.

See [lib.rs](src/lib.rs) for signatures. Long-running work returns futures,
streams, task handles, or event receivers. Application state remains synchronous
behind mutexes; storage services perform file, History, and index work on workers.

`EditorPersistenceCoordinator` sends document snapshots to recovery and save,
tracks which revisions have reached durable storage, and publishes Saved,
Dirty, or Error status. It owns journal and save handles and combines repeated
pending save requests.

## Undo and failure handling

Project undo retains at most 100 operations and 64 MiB of inverse data. Eviction
removes whole operations, including an inverse that alone exceeds the budget.
A new command clears redo. Closing, accepted recovery, and whole-project restore
clear interactive undo; `MigrationCompleted` is also an undo-reset reason.

Global replacement rechecks all selected matches and prepares forward and
inverse patches before applying them atomically through
`DocumentStateOwner::apply_composite`. It records the project operation on each
affected session without adding document-undo entries, publishes state only after
all in-memory changes succeed, and saves affected files in one transaction.

A save failure leaves accepted edits dirty and available to recovery. Failures
in rebuildable services, such as search, mark their results outdated without
changing authored data.
