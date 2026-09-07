# `parchmint-recovery-fs`

This crate writes the recovery journal and pending History checkpoint records
under `.parchmint/recovery/` in the project directory. It implements
`RecoveryJournal` and `CheckpointIntentStore`. The save crate tells it which
revisions have completed both the project-file write and the History checkpoint.

Filesystem transaction records for canonical replacement remain with
[`parchmint-project-fs`](../parchmint-project-fs/README.md).

## How it works

```text
recovery batch -> frame record -> append -> flush -> durable receipt

saved revision list -> copy newer records to a temporary file -> flush -> replace
```

Each record includes its length and checksum. On the next open, the crate can
detect an incomplete or corrupt record at the end of the journal.

## Interface

`FsRecoveryJournal::open` opens storage beneath an existing project directory.
It implements both `RecoveryJournal` and `CheckpointIntentStore`.

See [the source](src/lib.rs) for method signatures.

## Implementation

Append and inspection validate the accumulated document revisions and hashes,
including records separated by edits to other documents.

One worker appends recovery records for each project. It returns a flush receipt
after every record through the requested revision reaches durable storage. The
save crate records a planned History checkpoint before replacing project files.
After History succeeds, this crate records the checkpoint ID. Repeating that
completion writes the same result.

Compaction removes records only through the saved revision list and keeps newer
records. Every path is checked against the recovery directory before use.
Symlinks cannot redirect a write outside that directory. Permission or disk
space errors stop the operation and return an error. This crate has no Git,
SQLite, shell, or network access.
