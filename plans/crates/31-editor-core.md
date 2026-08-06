# Editor core

## Goal

Implement the ParchMint-owned shared document session behind `DocumentEngine`.

## Depends on

- [10 Recovery API](10-recovery-api.md)
- [17 Editor API](17-editor-api.md)

## Owning crate(s)

[`parchmint-editor-core`](../../docs/architecture/crates/parchmint-editor-core.md)

## Requirements and UI design

- [Editor panes and tabs](../../docs/product/editor-panes-and-tabs.md)
- [Comments and annotations](../../docs/product/comments-and-annotations.md)
- [Scale and performance](../../docs/product/scale-and-performance.md)

## Work

- Own stable IDs, transactions, comments, anchors, shared undo, revision mapping, logical view state, and bounded canonical projection through a private engine seam.

## Stage-specific tests and validation

Test shared two-view content/undo with independent logical selections, anchor mapping, deterministic projections, bounded projection coalescing, and full-snapshot fallback after incremental backlog.
