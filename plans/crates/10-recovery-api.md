# Recovery API

## Goal

Define durable, revisioned recovery records independent of editor engines.

## Depends on

- [02 Contracts](02-contracts.md)
- [03 Domain](03-domain.md)

## Owning crate(s)

[`parchmint-recovery-api`](../../docs/architecture/crates/parchmint-recovery-api.md)

## Requirements and UI design

- [Save, recovery, and closing](../../docs/product/save-recovery-and-closing.md)
- [Canonical project data](../../docs/product/canonical-project-data.md)

## Work

- Define ordered append, flush, inspect, replay, compact, and discard contracts with version, hash, and revision checks.

## Stage-specific tests and validation

Verify replay accepts only consecutive matching records and isolates an unknown, truncated, mismatched, or ambiguous record and every later record.
