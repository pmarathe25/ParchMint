# Headless backend integration

## Goal

Prove the real backend services work together through the headless CLI.

## Depends on

- [09 History Git2](../crates/09-history-git2.md)
- [12 Recovery filesystem](../crates/12-recovery-fs.md)
- [14 Search SQLite](../crates/14-search-sqlite.md)
- [16 Export HTML](../crates/16-export-html.md)
- [19 Test-support services](../crates/19-test-support-services.md)
- [20 Application](../crates/20-application.md)
- [21 Core CLI](../crates/21-core-cli.md)

## Owning paths

Cross-crate service composition and headless end-to-end tests.

## Requirements and UI design

- [Canonical user flows](../../docs/product/canonical-user-flows.md)
- [Release gates](../../docs/product/release-gates.md)

## Work

- Compose canonical formats, commands/undo, save/recovery, History, search, and export through the real CLI.

## Stage-specific tests and validation

On Windows, macOS, and Linux, exercise create, open, edit, save, terminate, recover, checkpoint, restore, index, query, rebuild, close, reopen, corrupt derived state, and project interchange without a GUI runtime.
