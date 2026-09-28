# `parchmint-history-api`

**Purpose:** Define complete project checkpoints through ParchMint types.
Current project files remain usable if History is lost; older checkpoints do not.

## Interface

`HistoryStore` initializes storage, creates and lists checkpoints, reads resources,
plans restores, verifies integrity, and maintains storage. `CheckpointInput`
identifies a save intent and exact file hashes. Calls run on an application worker;
the UI receives their asynchronous results. See [lib.rs](src/lib.rs).

## Checkpoint and restore rules

A checkpoint verifies the written files against the commit receipt. Retrying the
same intent and file hashes returns the same `CheckpointId`. Categories distinguish
autosave, explicit save, structural change, named snapshot, and restoration.
Named snapshots can create a checkpoint even when files have not changed.
Other saves reuse the current checkpoint when only manifest revision counters or
recovery bookkeeping differ. Current files retain the latest bookkeeping.

Checkpoints contain the manifest, documents, styles, project dictionary,
annotations, deletion tombstones, and format control. Recovery, caches, workspace
layout, appearance, and the global dictionary stay outside History.

Lists are paginated, with a continuation cursor and optional document filter.
Legacy consecutive revisions without authored changes are omitted from lists;
their stored IDs remain readable and restorable.
Document timelines include that document's changes and all named snapshots.
Preview and restore read complete project snapshots. A `RestorePlan`
describes writes through the normal save path, creating a new restoration
checkpoint without rewriting the timeline.

Maintenance runs at low priority and preserves retained checkpoints. Missing or
corrupt History can be reinitialized from current project files.
[history-git2](../parchmint-history-git2/README.md) implements this contract.
