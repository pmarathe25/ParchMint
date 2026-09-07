# `parchmint-recovery-fs`

**Purpose:** Store recovery records and pending History checkpoints under
`.parchmint/recovery/`. Canonical file-replacement transaction records belong to
[project-fs](../parchmint-project-fs/README.md).

## Interface

`FsRecoveryJournal::open` opens storage beneath an existing project root and
implements [RecoveryJournal](../parchmint-recovery-api/README.md) and
`CheckpointIntentStore`. See [lib.rs](src/lib.rs).

## Durable records

One worker appends records per project. Records carry lengths and checksums;
append and inspection validate document revisions and hashes across interleaved
edits. A receipt covers only records flushed to durable storage. Inspection
identifies incomplete or corrupt journal tails.

Before project files are replaced, the save coordinator records the planned
History checkpoint. After History succeeds, this store records its checkpoint
ID; repeating completion gives the same result.

Compaction copies records newer than the saved revision list into a temporary
file, flushes it, and replaces the journal. The save coordinator supplies revisions
that completed both file replacement and History.

Paths are checked beneath the recovery directory, including symlink escapes.
Permission and disk-space failures stop the operation with a typed error. This
crate has no Git, SQLite, shell, or network access.
