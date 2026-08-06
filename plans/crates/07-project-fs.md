# Project filesystem

## Goal

Implement the directory repository and crash-safe canonical file replacement.

## Depends on

- [05 Test-support fixtures](05-test-support-fixtures.md)
- [06 Project repository](06-project-repository.md)

## Owning crate(s)

[`parchmint-project-fs`](../../docs/architecture/crates/parchmint-project-fs.md)

## Requirements and UI design

- [Save, recovery, and closing](../../docs/product/save-recovery-and-closing.md)
- [Canonical project data](../../docs/product/canonical-project-data.md)

## Work

- Implement project locks, checked path access, temporary writes, flushes, multi-file transaction records, atomic replacement, and reconciliation.

## Stage-specific tests and validation

Inject failures before and during replacement, reopen afterward, and verify a complete old or new canonical state on Windows, macOS, and Linux.
