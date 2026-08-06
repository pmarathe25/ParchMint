# `parchmint-save`

## What it does

This crate saves changes from memory to the project files and History. It keeps
the save queue, records the exact revisions included in each save, tracks the
matching History checkpoint, and tells the application when it can show
`Saved`.

The input to a save is one fixed snapshot. Each document body and comment
sidecar in that snapshot comes from the same revision.

## How it works

```text
unsaved revisions -> capture one fixed revision list -> encode changed files
  -> record the planned History checkpoint -> replace project files safely
  -> create the matching History checkpoint -> mark those revisions as saved
  -> refresh search and word counts
```

New edits can arrive while a save runs. The current save keeps its original
revision list. The new edits remain unsaved and enter the next save.

## Public API

```rust
pub struct SaveRevisionVector {
    pub project_revision: ProjectRevision,
    pub open_documents: BTreeMap<DocumentId, EditorRevision>,
    pub closed_resources: BTreeMap<ResourceId, ResourceRevision>,
    pub canonical_hashes: BTreeMap<ResourceId, ContentHash>,
    pub generation: SaveGeneration,
}

pub trait SaveCoordinator: Send + Sync {
    fn request(&self, request: SaveRequest) -> Result<SaveTicket, SaveError>;
    fn status(&self) -> SaveStatusSnapshot;
    fn reconcile_open(&self) -> Result<OpenReconciliation, SaveError>;
    fn cancel_pending(&self, ticket: SaveTicket) -> CancelOutcome;
}

pub trait CheckpointIntentStore: Send + Sync {
    fn persist(&self, intent: CheckpointIntent) -> Result<(), IntentStoreError>;
    fn pending(&self) -> Result<Vec<CheckpointIntent>, IntentStoreError>;
    fn complete(&self, receipt: CheckpointReceipt)
        -> Result<(), IntentStoreError>;
}
```

`SaveTicket` reports the result asynchronously. A background worker converts
values into project files and writes them to disk.

## Implementation

Each project has one file writer and one save queue. A close request can raise a
save's priority. The queue can combine pending saves while preserving the
revision list already being written.

The application shows `Saved` after the project files and the matching History
checkpoint both contain the requested revisions. If the project files are safe
but History fails, the crate keeps the pending checkpoint record and retries
that same checkpoint. It does not show `Saved` until the retry succeeds.

A search or word-count error marks those calculated results as out of date. It
does not fail the save. After a save error, editing remains available and the
recovery journal continues to protect unsaved edits. A close request waits for
its high-priority save and leaves the project open if that save fails.

Opening a project checks unfinished work in this order:

```text
acquire lock -> finish or roll back an interrupted file replacement
  -> validate the complete project files -> finish a pending History checkpoint
  -> replay accepted recovery records -> clear undo after recovery
  -> rebuild search and other calculated data
```

Accepted recovery, completed migration, and whole-project History restore reset
interactive undo before more editing.
