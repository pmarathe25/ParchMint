# `parchmint-project-fs`

## What it does

This crate implements `ProjectRepository` and `AtomicWriter` for projects stored
in a normal directory. It validates the directory, holds the lock that allows
one writer, reads project files through `CanonicalCodec`, and replaces files
safely. All creation, replacement, and deletion of project files passes through
this crate.

## How it works

```text
write plan
  -> verify root and expected target identities
  -> write and flush temporary files beside their targets
  -> record transaction progress durably
  -> atomically replace each target and flush directories
  -> return one complete commit receipt
```

Most filesystems cannot replace several files in one operation. This crate
records the progress of a multi-file save before replacing the files. If the
application stops partway through, the next open uses that record to finish the
save or restore the previous files. It returns a commit receipt only after the
whole set is complete.

## Interface

```rust
pub trait ProjectFileSystem: Send + Sync {
    fn create_root(&self, path: UntrustedProjectPath)
        -> Result<(ProjectRootCapability, ProjectLockLease), FsError>;
    fn acquire(&self, path: UntrustedProjectPath)
        -> Result<(ProjectRootCapability, ProjectLockLease), FsError>;
    fn read(&self, root: &ProjectRootCapability, path: &CanonicalRelativePath)
        -> Result<Vec<u8>, FsError>;
    fn transaction_records(&self, root: &ProjectRootCapability)
        -> Result<Vec<SaveTransactionRecord>, FsError>;
}

pub struct FsProjectRepository<F: ProjectFileSystem = NativeProjectFileSystem> {
    files: F,
    active: Mutex<Option<ActiveProject>>, // private session state
}

impl<F: ProjectFileSystem> ProjectRepository for FsProjectRepository<F> {
    fn create(&self, request: CreateProject) -> Result<OpenProject, RepositoryError>;
    fn open(&self, path: ProjectPath) -> Result<OpenProject, RepositoryError>;
    fn load_document(&self, document: DocumentId) -> Result<Vec<u8>, RepositoryError>;
}

pub trait AtomicFileOps: Send + Sync {
    fn write_temporary(&self, write: TemporaryWrite) -> Result<TemporaryFile, FsError>;
    fn flush_file(&self, file: &TemporaryFile) -> Result<(), FsError>;
    fn replace(&self, file: TemporaryFile, target: &CheckedTarget) -> Result<(), FsError>;
    fn remove(&self, target: &CheckedTarget) -> Result<(), FsError>;
    fn flush_parent(&self, target: &CheckedTarget) -> Result<(), FsError>;
    fn root(&self) -> Option<&ProjectRootCapability>;
}

pub struct FsAtomicWriter<F: AtomicFileOps> {
    files: F,
    state: Mutex<WriterState>, // private
}
```

## Implementation

- One project can have only one writable ParchMint session, including across
  separately started processes.
- Only the current lock owner can recover a stale lock. If the crate cannot tell
  whether another process still owns the lock, it returns an error.
- Project creation is rejected inside another Git working tree.
- Absolute paths, parent traversal, symlink or reparse escapes, case collisions,
  and Unicode-normalization collisions are rejected.
- Temporary files stay inside the project directory and beside their target
  files. The crate validates each target again immediately before replacement.
- If the disk is full, permission is denied, a write is incomplete, or
  replacement is interrupted, the last completed project files remain
  available. The crate returns an error that identifies the failed operation.
- This crate has no Git command or network access.

`FsAtomicWriter` implements the `AtomicWriter` contract defined by the
repository crate. `AtomicFileOps` contains the small set of disk operations used
by `FsAtomicWriter`; `remove` and `root` have default implementations. The
desktop application selects the native filesystem implementation of
`ProjectFileSystem` and `AtomicFileOps`, and `FsProjectRepository::active_root()`
hands the retained root capability to History, recovery, search, and save
services without acquiring a second project lock. Tests supply wrappers that
can pause or fail a specific disk operation. The application calls this crate on
a storage worker, so disk access does not block the UI thread.
