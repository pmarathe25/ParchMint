# `parchmint-project-fs`

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

`FsProjectRepository` implements `ProjectRepository` through `ProjectFileSystem`.
`FsAtomicWriter` implements `AtomicWriter` through `AtomicFileOps`. These smaller
interfaces let tests inject read, flush, replacement, and reconciliation failures.

See [the source](src/lib.rs) for method signatures.

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
