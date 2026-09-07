# `parchmint-project-fs`

**Purpose:** Implement [ProjectRepository and AtomicWriter](../parchmint-project-repository/README.md)
for a directory. This crate validates roots and paths, holds the single-writer
lock, reads through `CanonicalCodec`, and creates, replaces, or deletes project
files on a storage worker.

## Interface

`FsProjectRepository` uses `ProjectFileSystem`; `FsAtomicWriter` uses
`AtomicFileOps`. Tests wrap these file-operation interfaces to pause or fail
reads, flushes, replacements, and reconciliation. `AtomicFileOps::remove` and
`root` have defaults. See [lib.rs](src/lib.rs).

`FsProjectRepository::active_root()` supplies the retained validated root to
History, recovery, search, and save services without taking a second lock.
The desktop selects the native implementations.

## Write transaction

```text
validate root and target identities -> write and flush adjacent temporary files
  -> record progress durably -> replace targets and flush directories
  -> return the complete commit receipt
```

Multi-file replacement uses a durable progress record because filesystems cannot
replace the whole set in one operation. After interruption, opening finishes the
save or restores the previous files. A receipt is returned only for a complete
set. Targets are revalidated immediately before replacement.

## Filesystem guarantees

- One process holds the writable project lease. Only the lock owner can recover
  a stale lock; uncertain ownership returns an error.
- Creation inside another Git working tree is rejected.
- Paths reject absolute names, parent traversal, symlink or reparse escapes,
  case collisions, and Unicode-normalization collisions.
- Temporary files stay beside their targets inside the project directory.
- Disk, permission, partial-write, or replacement failures identify the failed
  operation and preserve the last complete project for reconciliation.

The crate performs no Git commands or network operations.
