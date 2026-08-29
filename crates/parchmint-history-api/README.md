# `parchmint-history-api`

## What it does

This crate defines the interface for saving and reading project History. A
History implementation stores complete project checkpoints. Callers use
ParchMint types and do not depend on Git types.

History stores earlier versions in addition to the current project files. If
History is lost, ParchMint can still open the current project, but the earlier
checkpoints are gone.

## How it works

```text
filesystem commit receipt + checkpoint intent
  -> History verifies the hashes of the written project files
  -> History returns the existing ID when this checkpoint already exists
  -> History adds the checkpoint and returns CheckpointId

CheckpointId -> validate complete snapshot -> RestorePlan -> normal save path
```

A restore plan describes files to write. The normal save path applies the plan
and creates a new restoration checkpoint; History stays in its existing order.

## Interface

```rust
pub trait HistoryStore: Send + Sync {
    fn initialize(&self, project: ProjectRootCapability)
        -> Result<HistoryState, HistoryError>;
    fn reinitialize_availability(&self)
        -> Result<HistoryReinitializeAvailability, HistoryError>;
    fn reinitialize(&self, project: ProjectRootCapability)
        -> Result<HistoryReinitializeReport, HistoryError>;
    fn checkpoint(&self, input: CheckpointInput)
        -> Result<CheckpointId, HistoryError>;
    fn list(&self, query: HistoryPageQuery)
        -> Result<HistoryPage, HistoryError>;
    fn preview(&self, checkpoint: CheckpointId)
        -> Result<SnapshotPreview, HistoryError>;
    fn read_resource(&self, checkpoint: CheckpointId, path: &CanonicalRelativePath)
        -> Result<CheckpointResource, HistoryError>;
    fn restore(&self, checkpoint: CheckpointId)
        -> Result<RestorePlan, HistoryError>;
    fn verify(&self) -> Result<HistoryIntegrityReport, HistoryError>;
    fn maintain(&self, budget: MaintenanceBudget)
        -> Result<MaintenanceReport, HistoryError>;
}

pub struct CheckpointInput {
    pub intent_hash: CheckpointIntentHash,
    pub resources: BTreeMap<CanonicalRelativePath, ContentHash>,
    pub category: CheckpointCategory,
    pub affected_documents: Vec<DocumentId>,
    pub name: Option<SnapshotName>,
    pub recorded_at_unix_millis: Option<u64>,
}
```

These methods run on an application worker. The UI receives an asynchronous
result and the worker performs Git operations.

## Implementation

History adds checkpoints in order and does not rewrite earlier checkpoints. A
retry with the same intent hash and file hashes returns the same checkpoint ID.
Checkpoint categories record an autosave, explicit save, structural change,
named snapshot, or restoration. A named snapshot can create a checkpoint even
when no project file changed.

Checkpoints contain the manifest, documents, styles, project dictionary,
annotations, deletion tombstones, and format control. They exclude recovery,
caches, workspace state, appearance, and the global dictionary.

The list method returns one page at a time and includes a cursor for the next
page. Callers can filter checkpoints by affected document. Preview and restore
always include every project file. Maintenance runs at low priority and keeps
all retained checkpoints. If History is missing or corrupt, the user can create
a new History store from the current project files.
