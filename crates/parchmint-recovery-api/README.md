# `parchmint-recovery-api`

**Purpose:** Define a durable journal for edits newer than the last completed
save. Records contain versioned ParchMint operations, with schemas in
[contracts](../parchmint-contracts/README.md); engine transactions and snapshot
fragments are not record formats.

## Interface

`RecoveryJournal` appends and flushes `RecoveryBatch` records, inspects and replays
them, and compacts or discards records through explicitly saved revisions.
`RecoveryReceipt` identifies the last record flushed to durable storage.
See [lib.rs](src/lib.rs).

## Validation and replay

`RecoveryAppendFrontier` checks project order, per-document revisions, and
resource hashes across interleaved edits. A document's first retained record may
continue a saved revision; replay checks it against the saved base.

Replay starts from the last complete save and applies consecutive records while
versions, revisions, and hashes match. An unknown version, missing or truncated
record, bad hash, or ambiguity isolates that record and all later records for
review. Accepted replay uses the normal canonical save path.

After saving, the journal can remove records through the exact saved revisions
and keeps newer records. Editing can continue after a journal failure, but the
application reports that crash recovery is unavailable.

A record may retain identical content hashes while advancing document revisions:
coalesced editing and undo can return to the same bytes. Such records preserve
the consecutive revision frontier. Resource-only records must still change a hash.
