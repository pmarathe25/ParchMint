# History Git2

## Goal

Implement app-managed linear History with vendored libgit2.

## Depends on

- [07 Project filesystem](07-project-fs.md)
- [08 History API](08-history-api.md)

## Owning crate(s)

[`parchmint-history-git2`](../../docs/architecture/crates/parchmint-history-git2.md)

## Requirements and UI design

- [History and snapshots](../../docs/product/history-and-snapshots.md)
- [Privacy and security](../../docs/product/privacy-and-security.md)

## Work

- Implement `HistoryStore` using the pinned vendored `git2` selection specified by the crate page; keep network transports and installed Git out of the path.
- Add corruption isolation, stale-lock handling, and bounded maintenance.

## Stage-specific tests and validation

Verify checkpoint continuation, corrupt-object isolation, stale-lock recovery, line-ending portability, and no-network or Git-executable dependency.
