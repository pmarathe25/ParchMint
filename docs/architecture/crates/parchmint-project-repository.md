# `parchmint-project-repository`

## What it does

This crate is the application's entry point for creating, opening, and reading
projects. It presents resources by stable ID and hides filesystem layout. Large
projects load document bodies on demand.

## How it works

```text
create/open request
  -> validate the project directory and acquire its write lock
  -> read format control and manifest
  -> validate manifest and referenced resources
  -> load only the resources needed now
  -> return an opened project snapshot
```

The manifest lists the resources and their order. Directory listing does not
define project contents.

## Public API

```rust
pub trait ProjectRepository: Send + Sync {
    fn create(&self, request: CreateProject)
        -> Result<OpenedProject, RepositoryError>;
    fn open(&self, path: ProjectPath)
        -> Result<OpenedProject, RepositoryError>;
    fn load_document(&self, id: DocumentId)
        -> Result<CanonicalDocument, RepositoryError>;
    fn load_annotations(&self, id: DocumentId)
        -> Result<CanonicalAnnotations, RepositoryError>;
    fn capture_closed_resources(&self, set: ResourceSet)
        -> Result<CanonicalResourceSnapshot, RepositoryError>;
    fn validate(&self) -> Result<ProjectIntegrityReport, RepositoryError>;
}

pub struct OpenedProject {
    pub root: ProjectRootCapability,
    pub lock: ProjectLockLease,
    pub snapshot: ProjectSnapshot,
}

pub trait AtomicWriter: Send + Sync {
    fn stage(&self, plan: AtomicWritePlan) -> Result<StagedWrite, WriteError>;
    fn validate_staged(&self, staged: &StagedWrite) -> StagedWriteReport;
    fn commit(&self, staged: StagedWrite) -> Result<AtomicCommitReceipt, WriteError>;
    fn reconcile(&self, record: SaveTransactionRecord) -> Result<ReconciliationResult, WriteError>;
    fn abandon(&self, staged: StagedWrite) -> Result<AbandonResult, WriteError>;
}

pub struct AtomicWritePlan {
    pub root: ProjectRootCapability,
    pub transaction: SaveTransactionId,
    pub expected_hashes: BTreeMap<ResourceId, ContentHash>,
    pub operations: Vec<TypedResourceWrite>,
}
```

These are ParchMint types. The repository converts file handles,
operating-system paths, parser values, and filesystem errors before returning.

These methods are called only on an application storage worker. The UI receives
an asynchronous application result; the UI calls the application layer.

## Implementation

The implementation uses `CanonicalCodec` to decode the manifest and project
files. It validates every resource path relative to the project directory and
loads document bodies only when a caller requests them.

An opened project snapshot cannot change. It records the revision of each
loaded resource. `ProjectRootCapability` is a token created after the repository
validates the project directory, and `ProjectLockLease` proves that this process
holds the write lock. Closing the project invalidates both tokens.

The repository returns an error before editing starts when it cannot acquire the
lock, cannot read the format, finds a path outside the project, or cannot find a
required file. After an interrupted save, it also returns an error if the
filesystem crate cannot restore one complete set of project files.

`AtomicWriter` and its plans, receipts, and other ParchMint-owned value types
are repository contracts. The filesystem crate supplies the implementation.
