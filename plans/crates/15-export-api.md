# Export API

## Goal

Define immutable whole-Manuscript export plans and checked output targets.

## Depends on

- [03 Domain](03-domain.md)
- [04 Project format](04-project-format.md)

## Owning crate(s)

[`parchmint-export-api`](../../docs/architecture/crates/parchmint-export-api.md)

## Requirements and UI design

- [Export](../../docs/product/export.md)
- [Canonical project data](../../docs/product/canonical-project-data.md)

## Work

- Define ordered semantic export items, inherited settings, source revisions, validation, cancellation, and temporary output completion.

## Stage-specific tests and validation

Verify the plan excludes Research/comments/metadata, detects mixed revisions and unsafe targets, and cancellation cannot report a completed output.
