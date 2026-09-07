# `parchmint-project-repository`

**Purpose:** Define project creation, opening, lazy reads, and multi-file writes
through stable resource IDs. Callers use ParchMint values without depending on
filesystem layout or OS handles.

## Interface

`ProjectRepository` creates and opens projects and loads document bodies on
demand. `OpenProject` pairs an immutable manifest/document-index snapshot with an
opaque write lease. Dropping it releases the lease.

`AtomicWriter` stages, validates, commits, reconciles, or abandons a multi-file
write. Plans, receipts, and writer states belong to this contract. Repository
operations return `RepositoryError`; writer transitions return `WriteError`.
See [lib.rs](src/lib.rs).

## Open and lease rules

Opening validates the directory, acquires the lock, reads format control and the
manifest, validates referenced resources, and loads only what is needed. Missing
files, unsupported formats, unsafe paths, lock failures, and unreconciled writes
fail before editing starts.

`ProjectRootCapability` is the numeric identity of a validated root.
`ProjectLockLease` proves that the process holds its write lock. Closing the
project invalidates both. The opened snapshot remains immutable.

[project-fs](../parchmint-project-fs/README.md) implements disk access through
`CanonicalCodec` on a storage worker. `InMemoryProjectRepository` and
`InMemoryAtomicWriter` model the same invariants for tests. The in-memory index
uses resources supplied at creation; the filesystem implementation derives the
index from the project directory.
