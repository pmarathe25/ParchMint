# Domain

## Goal

Implement valid project state, project commands, and project undo inputs.

## Depends on

- [01 Bootstrap CI and supply chain](../integration/01-bootstrap-ci-and-supply-chain.md)

## Owning crate(s)

[`parchmint-domain`](../../docs/architecture/crates/parchmint-domain.md)

## Requirements and UI design

- [Project model](../../docs/product/project-model.md)
- [Explorer and hierarchy](../../docs/product/explorer-and-hierarchy.md)
- [Rich text and semantic styles](../../docs/product/rich-text-and-semantic-styles.md)
- [Titles](../../docs/product/titles.md)
- [Synopsis and metadata](../../docs/product/synopsis-and-metadata.md)
- [Deletion and Recently Deleted](../../docs/product/deletion-and-recently-deleted.md)
- [Research](../../docs/product/research.md)
- [Word counts](../../docs/product/word-counts.md)
- [Undo and redo](../../docs/product/undo-and-redo.md)
- [Canonical project data](../../docs/product/canonical-project-data.md)

## Work

- Implement stable IDs, ordered-tree rules, styles, metadata, dictionaries, deletion tombstones, commands, inverse commands, title synchronization, and word counting.
- Reject invalid trees and stale revisions before publishing a changed project.

## Stage-specific tests and validation

Run randomized command/undo sequences, fixed-root and cycle rejection cases, title synchronization cases, and deterministic word-count fixtures.
