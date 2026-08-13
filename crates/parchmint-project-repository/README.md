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

The manifest lists the canonical resources and their order. The snapshot
carries the manifest text and a document index: the in-memory model keeps the
set given at creation, while the filesystem implementation derives its index
from the project directory. The in-memory repository models the same
invariants: one lease per opened project, and missing, locked, unsafe, or
interrupted projects are rejected before a snapshot is returned.

## Interface

```rust
pub trait ProjectRepository: Send + Sync {
    fn create(&self, request: CreateProject) -> Result<OpenProject, RepositoryError>;
    fn open(&self, path: ProjectPath) -> Result<OpenProject, RepositoryError>;
    fn load_document(&self, document: DocumentId) -> Result<Vec<u8>, RepositoryError>;
}

pub struct ProjectSnapshot {
    pub path: ProjectPath,
    pub manifest: String,
    pub document_ids: Vec<DocumentId>,
}

pub struct OpenProject {
    pub snapshot: ProjectSnapshot,
    // opaque writable lease; dropping the opened project releases it
}

pub trait AtomicWriter: Send + Sync {
    fn stage(&self, plan: AtomicWritePlan) -> Result<StagedWrite, WriteError>;
    fn validate_staged(&self, staged: &StagedWrite) -> ValidationReport;
    fn commit(&self, staged: StagedWrite) -> Result<CommitReceipt, WriteError>;
    fn reconcile(&self, record: SaveTransactionRecord) -> Result<Reconciliation, WriteError>;
    fn abandon(&self, staged: StagedWrite) -> Result<Abandonment, WriteError>;
}

pub struct AtomicWritePlan {
    pub writes: Vec<StagedResource>,
    pub deletions: Vec<String>,
}

pub struct StagedResource {
    pub path: String,
    pub bytes: Vec<u8>,
}
```

These are ParchMint-owned value types; the filesystem implementation converts
operating-system handles, paths, codec values, and filesystem errors into them
before returning. Failures surface as `RepositoryError` for create, open, and
load, and as `WriteError` for writer state transitions.

These methods are called only on an application storage worker. The UI receives
an asynchronous application result; the UI calls the application layer.

## Implementation

This crate defines the repository and writer contracts and an in-memory model
of their invariants. The filesystem crate is the executable repository
implementation: it reads project files through `CanonicalCodec`, validates
every resource path relative to the project directory, and loads document
bodies only when a caller requests them.

An opened project snapshot cannot change. `OpenProject` pairs the immutable
snapshot with an opaque writable lease that is released when the opened project
is dropped. `ProjectRootCapability` is a stable numeric identity for one
validated project root; the filesystem crate creates it only after validating
the directory, and its `ProjectLockLease` proves that this process holds the
write lock. Closing the project invalidates both.

The repository returns an error before editing starts when it cannot acquire the
lock, cannot read the format, finds a path outside the project, or cannot find a
required file. After an interrupted save, it also returns an error if the
filesystem crate cannot restore one complete set of project files.

`AtomicWriter` and its plans, receipts, and other ParchMint-owned value types
are repository contracts. The filesystem crate supplies the implementation;
`InMemoryProjectRepository` and `InMemoryAtomicWriter` are contract models and
test doubles.
