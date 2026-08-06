# History API

## Goal

Define complete project checkpoints without exposing Git.

## Depends on

- [03 Domain](03-domain.md)
- [06 Project repository](06-project-repository.md)

## Owning crate(s)

[`parchmint-history-api`](../../docs/architecture/crates/parchmint-history-api.md)

## Requirements and UI design

- [History and snapshots](../../docs/product/history-and-snapshots.md)
- [Deletion and Recently Deleted](../../docs/product/deletion-and-recently-deleted.md)

## Work

- Define checkpoint, list, preview, restore, verify, and maintenance contracts using ParchMint identifiers and complete restore plans.

## Stage-specific tests and validation

Run contract tests for idempotent checkpoint intent, paging/filter cursors, named empty snapshots, and whole-project restore input.
