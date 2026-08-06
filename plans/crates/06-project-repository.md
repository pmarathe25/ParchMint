# Project repository

## Goal

Define safe project creation, opening, on-demand loading, integrity reads, and
atomic-write contracts.

## Depends on

- [03 Domain](03-domain.md)
- [04 Project format](04-project-format.md)

## Owning crate(s)

[`parchmint-project-repository`](../../docs/architecture/crates/parchmint-project-repository.md)

## Requirements and UI design

- [Launcher and project creation](../../docs/product/launcher-and-project-creation.md)
- [Canonical project data](../../docs/product/canonical-project-data.md)
- [Save, recovery, and closing](../../docs/product/save-recovery-and-closing.md)

## Work

- Implement repository contracts around validated roots, immutable opened
  snapshots, lock leases, lazy document loading, and project integrity reports.
- Define `AtomicWriter` plans, staged writes, validation, commit receipts,
  reconciliation, and abandonment with ParchMint-owned values.

## Stage-specific tests and validation

Run shared repository contract tests for creation, locked projects, missing
resources, unsafe paths, lazy body loading, and interrupted-save rejection. Add
shared `AtomicWriter` contract tests for invalid transitions, stale or foreign
staged writes, receipt identity, reconciliation, and abandonment.
