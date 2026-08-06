# Recovery filesystem

## Goal

Persist recovery batches and checkpoint intents safely inside the project.

## Depends on

- [07 Project filesystem](07-project-fs.md)
- [10 Recovery API](10-recovery-api.md)
- [11 Save](11-save.md)

## Owning crate(s)

[`parchmint-recovery-fs`](../../docs/architecture/crates/parchmint-recovery-fs.md)

## Requirements and UI design

- [Save, recovery, and closing](../../docs/product/save-recovery-and-closing.md)
- [Privacy and security](../../docs/product/privacy-and-security.md)

## Work

- Implement framed recovery records, durable receipts, checkpoint-intent storage, compaction, and checked recovery paths.

## Stage-specific tests and validation

Test truncated-tail detection, record checksum failure, idempotent checkpoint completion, compaction retaining newer edits, and recovery-directory escape rejection.
