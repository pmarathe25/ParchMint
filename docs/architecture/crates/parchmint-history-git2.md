# `parchmint-history-git2`

## What it does

This crate implements `HistoryStore` with embedded libgit2. Git repositories,
commits, references, object IDs, locks, and errors stay inside this crate.

The crate uses `git2` 0.21.0 with vendored libgit2 and controlled static zlib.
HTTPS, SSH, remote transports, and installed Git are absent.

```toml
git2 = { version = "=0.21.0", default-features = false, features = ["vendored-libgit2"] }
```

## How it works

```text
CheckpointInput
  -> stage only files that belong to the saved project
  -> normalize line endings and ignore platform file-mode differences
  -> commit on app-managed main -> verify objects
  -> record which CheckpointId belongs to the save intent
```

## Public API

```rust
pub struct Git2HistoryStore {
    root: ProjectRootCapability,
    repository: PrivateGitRepository,
}

impl HistoryStore for Git2HistoryStore {
    fn initialize(&self, project: ProjectRootCapability)
        -> Result<HistoryState, HistoryError>;
    fn checkpoint(&self, input: CheckpointInput)
        -> Result<CheckpointId, HistoryError>;
    fn list(&self, query: HistoryPageQuery)
        -> Result<HistoryPage, HistoryError>;
    fn preview(&self, checkpoint: CheckpointId)
        -> Result<SnapshotPreview, HistoryError>;
    fn restore(&self, checkpoint: CheckpointId)
        -> Result<RestorePlan, HistoryError>;
    fn verify(&self) -> Result<HistoryIntegrityReport, HistoryError>;
    fn maintain(&self, budget: MaintenanceBudget)
        -> Result<MaintenanceReport, HistoryError>;
}
```

`PrivateGitRepository` represents the libgit2 repository inside this crate.
Callers see ParchMint checkpoint IDs and errors.

## Implementation

The project root is the Git repository root and has one app-managed `main`.
Automatic line-ending conversion, executable-mode tracking, and symlink
tracking are disabled. Absolute, escaping, and unexpected paths are rejected.

Named snapshots use empty commits when no project file changed. Restore reads a
commit and leaves the `main` branch in place. To list History, the crate reads
only enough commits for the requested page and returns a cursor that continues
from the next commit.

Only the process that holds the project lock can recover a stale Git lock.
Invalid Git objects return a History error and leave the current project files
unchanged. Maintenance pauses for active work, verifies each new pack, and
removes loose objects only after the pack contains the same data. It keeps every
commit reachable from retained History. The crate has no network access and
does not run a Git executable.
