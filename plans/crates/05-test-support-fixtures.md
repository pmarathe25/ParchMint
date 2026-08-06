# Test-support fixtures

## Goal

Provide deterministic builders and canonical fixture support for early core work.

## Depends on

- [02 Contracts](02-contracts.md)
- [03 Domain](03-domain.md)
- [04 Project format](04-project-format.md)

## Owning crate(s)

[`parchmint-test-support`](../../docs/architecture/crates/parchmint-test-support.md)

## Requirements and UI design

- [Canonical project data](../../docs/product/canonical-project-data.md)
- [Scale and performance](../../docs/product/scale-and-performance.md)

## Work

- Add seeded project builders, fixed clocks and IDs, scoped temporary projects, canonical-byte fixtures, and shrinking randomized-case support.

## Stage-specific tests and validation

Confirm builders use public production APIs, fixture bytes are deterministic, and every randomized failure reports a seed and smallest failing sequence.
