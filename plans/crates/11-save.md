# Save

## Goal

Coordinate revisioned canonical saves and matching History checkpoints.

## Depends on

- [04 Project format](04-project-format.md)
- [06 Project repository](06-project-repository.md)
- [08 History API](08-history-api.md)
- [10 Recovery API](10-recovery-api.md)

## Owning crate(s)

[`parchmint-save`](../../docs/architecture/crates/parchmint-save.md)

## Requirements and UI design

- [Save, recovery, and closing](../../docs/product/save-recovery-and-closing.md)
- [History and snapshots](../../docs/product/history-and-snapshots.md)

## Work

- Implement one writer and save queue per project, immutable revision vectors, priority close saves, checkpoint intents, and open reconciliation.
- Acknowledge Saved only after project files and the matching checkpoint contain the requested revisions.

## Stage-specific tests and validation

Pause and reorder saves to prove coalescing preserves written revision vectors; inject History failure after file commit and verify retry never falsely reports Saved.
