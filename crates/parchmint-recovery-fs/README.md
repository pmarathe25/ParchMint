# `parchmint-recovery-fs`

## What it does

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

```rust
pub struct FsRecoveryJournal {
    root: PathBuf,
    root_identity: FileIdentity,
    operations: Mutex<()>,
}

impl FsRecoveryJournal {
    /// Opens or creates recovery storage beneath an existing project directory.
    pub fn open(project_root: impl AsRef<Path>) -> Result<Self, RecoveryError>;
}

impl RecoveryJournal for FsRecoveryJournal {
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

impl CheckpointIntentStore for FsRecoveryJournal {
    fn persist(&self, intent: CheckpointIntent) -> Result<(), IntentStoreError>;
    fn pending(&self) -> Result<Vec<CheckpointIntent>, IntentStoreError>;
    fn complete(&self, receipt: CheckpointReceipt)
        -> Result<(), IntentStoreError>;
}
```

## Implementation

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
