# Search API

## Goal

Define rebuildable whole-project search and safe replacement candidates.

## Depends on

- [03 Domain](03-domain.md)

## Owning crate(s)

[`parchmint-search-api`](../../docs/architecture/crates/parchmint-search-api.md)

## Requirements and UI design

- [Search and replacement](../../docs/product/search-and-replacement.md)
- [Scale and performance](../../docs/product/scale-and-performance.md)

## Work

- Define revisioned document projections, streamed search batches, cancellation, index verification/rebuild, and revalidated global-replacement candidates.

## Stage-specific tests and validation

Verify stale query batches are ignored, candidate revisions/text are rechecked, and corrupt or missing indexes rebuild without changing authored data.
