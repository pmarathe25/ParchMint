# `parchmint-recovery-api`

This crate defines the recovery journal that protects edits made after the last
completed save. The journal stores enough information to rebuild those edits
after a crash. The save crate writes accepted recovery through the normal save
path.

Recovery records contain versioned ParchMint edit operations. They contain no
editor-engine transactions. Snapshot-fragment records are not yet implemented.
Their schemas live in
[`parchmint-contracts`](../parchmint-contracts/README.md).

## How it works

```text
revisioned edit -> append and flush recovery batch -> exact durable receipt

last completed save + journal -> check versions, revisions, and hashes
                              -> replayable edits or isolated invalid records
```

The save coordinator decides whether to accept a replay and writes the result
through the normal canonical save path.

## Interface

`RecoveryJournal` appends and flushes `RecoveryBatch` records, inspects and
replays them, and compacts or discards records through explicit saved revisions.

See [the source](src/lib.rs) for method signatures.

## Implementation

`RecoveryAppendFrontier` validates project order, per-document revisions, and
resource hashes across interleaved edits. A document’s first retained record can
continue a saved revision; replay checks that revision against the saved base.
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
