# `parchmint-save`

**Purpose:** Write one fixed project snapshot to files and History, then report
which revisions are saved. A document body and its comments always come from the
same revision. Edits made during a save remain dirty for the next save.

## Save and open flows

```text
capture revisions -> encode changed files -> persist checkpoint intent
  -> replace project files -> create matching History checkpoint
  -> acknowledge saved revisions -> refresh search and word counts
```

On open, unfinished work is handled in this order:

```text
acquire lock -> reconcile file replacement -> validate project files
  -> finish pending History checkpoint -> replay accepted recovery
  -> reset undo after recovery -> rebuild calculated data
```

## Interface

`ProjectSaveCoordinator` implements `SaveCoordinator`: request saves, inspect
status, reconcile interrupted opens, and cancel queued work. `SaveTicket` reports
completion asynchronously. `CheckpointIntentStore` persists pending History work.
See [lib.rs](src/lib.rs).

## Queue and failure rules

Each project has one file writer and serial save queue. Pending requests can
combine, but an active save retains its captured revisions. Closing raises save
priority and waits for completion; a failed close save leaves the project open.

`Saved` requires both complete project files and their matching History checkpoint.
If files are safe but History fails, the pending intent remains for retry of the
same checkpoint. Search or word-count failures mark those results outdated
without failing the save.

Save errors leave editing available and unsaved changes available to recovery.
Accepted recovery and whole-project History restore reset interactive undo before
editing resumes; completed migration is also represented as an undo-reset reason.
