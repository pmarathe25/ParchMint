# `parchmint-recovery-api`

## What it does

This crate defines the recovery journal that protects edits made after the last
completed save. The journal stores enough information to rebuild those edits
after a crash. The save crate writes accepted recovery through the normal save
path.

Recovery records contain versioned ParchMint edit operations or fragments of a
saved-file snapshot. They contain no editor-engine transactions. Their schemas
live in [`parchmint-contracts`](parchmint-contracts.md).

## How it works

```text
revisioned edit -> append and flush recovery batch -> exact durable receipt

last completed save + journal -> check versions, revisions, and hashes
                              -> replayable edits or isolated invalid records
```

The save coordinator decides whether to accept a replay and writes the result
through the normal canonical save path.

## Public API

```rust
pub trait RecoveryJournal: Send + Sync {
    fn append(&self, batch: RecoveryBatch)
        -> Result<RecoveryReceipt, RecoveryError>;
    fn flush_through(&self, target: RecoveryRevisionVector)
        -> Result<RecoveryReceipt, RecoveryError>;
    fn inspect(&self) -> Result<RecoveryInventory, RecoveryError>;
    fn replay(&self, base: RecoveryBaseSnapshot)
        -> Result<RecoveryReplay, RecoveryError>;
    fn compact(&self, durable: DurableRevisionVector)
        -> Result<CompactionReport, RecoveryError>;
    fn discard_through(&self, durable: DurableRevisionVector)
        -> Result<DiscardReport, RecoveryError>;
}

pub struct RecoveryBatch {
    pub project_revision: ProjectRevision,
    pub documents: BTreeMap<DocumentId, EditorRevisionRange>,
    pub base_hashes: BTreeMap<ResourceId, ContentHash>,
    pub payload: VersionedRecoveryPayload,
}
```

## Implementation

The journal adds records in order. A `RecoveryReceipt` identifies the last
record that has reached durable storage.

Replay starts from the last completed project save. It applies consecutive
journal records while their versions, revisions, and hashes match. When it finds
an unknown version, missing record, bad hash, truncated record, or ambiguous
record, it isolates that record and everything after it for review.

After a save, the save crate gives the journal the exact saved revisions. The
journal can remove records through those revisions and keeps all newer records.
Editing can continue in memory after a journal error, and the application shows
that crash recovery is currently unavailable.
