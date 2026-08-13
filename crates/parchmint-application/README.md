# `parchmint-application`

## What it does

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

Global replacement changes several documents as one project command. Before it
changes any document, it prepares the data needed to reverse every change. The
operation uses one ID, creates one project undo entry, and creates one History
checkpoint after the save completes. Open editor sessions receive a
project-command boundary and do not add separate document-undo entries. If any
part fails, the prepared inverse restores every affected open and closed
document as one operation; recovery never accepts a partial replacement.

## Interface

```rust
pub type AppFuture<'a, T> =
    Pin<Box<dyn Future<Output = T> + Send + 'a>>;

pub trait ProjectCommandDispatcher: Send + Sync {
    fn execute(
        &self,
        command: ProjectCommand,
    ) -> AppFuture<'_, Result<ProjectCommandResult, ApplicationError>>;

    fn undo(&self) -> AppFuture<'_, Result<ProjectCommandResult, ApplicationError>>;
    fn redo(&self) -> AppFuture<'_, Result<ProjectCommandResult, ApplicationError>>;
    fn undo_state(&self) -> ProjectUndoState;
    fn reset_undo(&self, reason: UndoResetReason);
}

pub struct ProjectCommandResult {
    pub operation_id: ProjectOperationId,
    pub revision: ProjectRevision,
    pub dirty_resources: ResourceSet,
    pub events: Vec<ProjectEvent>,
    pub checkpoint_group: CheckpointGroupId,
}

pub struct ProjectUndoEntry {
    pub operation_id: ProjectOperationId,
    pub label: String,
    pub forward: ProjectPatch,
    pub inverse: ProjectPatch,
    pub revisions: RevisionRange,
    pub affected: ResourceSet,
    pub byte_cost: usize,
    pub checkpoint_group: CheckpointGroupId,
}

pub trait GlobalReplacement: Send + Sync {
    fn preview(&self, selection: ReplacementSelection)
        -> AppFuture<'_, Result<ReplacementPreview, ApplicationError>>;
    fn apply(&self, selection: ReplacementSelection)
        -> AppFuture<'_, Result<ProjectCommandResult, ApplicationError>>;
}
```

Long-running application methods return a future, stream, task handle, or event
receiver. Application state stays synchronous behind mutexes, and the actual
file, History, and index work happens inside the service crates (for example
the serial save worker in `parchmint-save`). Public methods use ParchMint types
instead of types from the storage libraries.

`EditorPersistenceCoordinator` owns projection-to-recovery routing, the
receipt/frontier acknowledgement boundary, bounded repeated-save coalescing,
and the public Saved/Dirty/Error frontier. It does not assemble the desktop
service graph; `parchmint-desktop` does.

## Implementation

Project undo retains up to 100 complete operations and 64 MiB of inverse data
in memory. Eviction removes whole operations, and a new project command clears
redo. A larger inverse is not retained after eviction; moving it to a temporary
session file is not implemented.

```rust
fn execute_now(&self, command: ProjectCommand) -> Result<ProjectCommandResult, ApplicationError> {
    let mut state = lock(&self.state)?;
    let before = state.project.revision;
    let forward = command.clone();
    let applied = apply_project_command(&state.project, before, command)?;
    if let ProjectCommand::CreateDocument { document_id, .. } = &forward {
        self.documents
            .insert_document(default_open_snapshot(*document_id))?;
    }
    let operation_id = state.operation_id();
    let mutation = state.mark_dirty(&applied.changed_resources);
    let checkpoint_group = state.stage_checkpoint(mutation);
    state.push_undo(ProjectUndoEntry {
        operation_id,
        label: command_label(&forward).to_owned(),
        forward: ProjectPatch::Domain(forward),
        inverse: ProjectPatch::Domain(applied.inverse),
        revisions: RevisionRange { before, after: applied.project.revision },
        affected: applied.changed_resources.clone(),
        byte_cost: patch_byte_cost(&command) + patch_byte_cost(&applied.inverse),
        checkpoint_group,
    });
    state.project = applied.project;
    state.redo.clear();
    Ok(ProjectCommandResult {
        operation_id,
        revision: state.project.revision,
        dirty_resources: applied.changed_resources,
        events: vec![ProjectEvent::Executed],
        checkpoint_group,
    })
}
```

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
