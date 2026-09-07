# `parchmint-history-git2`

**Purpose:** Implement [HistoryStore](../parchmint-history-api/README.md) with
vendored libgit2. Git types and errors stay inside this crate; calls require
neither an installed Git executable nor network access.

## Interface

`Git2HistoryStore::new` accepts a validated `NativeProjectRoot`.
See [lib.rs](src/lib.rs) for methods and [Cargo.toml](Cargo.toml) for pinned
`git2`, vendored libgit2, and static-zlib dependencies. HTTPS, SSH, and remote
transports are disabled.

Repositories open per operation because `git2::Repository` is not `Sync`.
A process-wide gate serializes all stores targeting the same project root.
Callers receive ParchMint IDs and errors.

## Storage and failure handling

The project root is the Git repository root, with one app-managed `main` branch.
Checkpointing stages only saved project files, normalizes line endings, commits,
verifies objects, and records the save intent's checkpoint ID. Automatic line-ending
conversion, executable-mode tracking, and symlink tracking are disabled. Absolute,
escaping, and unexpected paths are rejected.

Named snapshots use empty commits when needed. Restore reads a checkpoint
without moving `main`; the normal save path writes the restoration. History
listing reads only the requested page and returns a continuation cursor.

Only the project lock owner can recover a stale Git lock. Invalid objects return
a History error without changing current files. Maintenance pauses for active
work, verifies new packs before removing equivalent loose objects, and keeps
every commit reachable from retained History.
