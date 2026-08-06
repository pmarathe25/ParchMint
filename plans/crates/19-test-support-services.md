# Test-support services

## Goal

Extend deterministic test support to service contracts and fault schedules.

## Depends on

- [06 Project repository](06-project-repository.md)
- [07 Project filesystem](07-project-fs.md)
- [08 History API](08-history-api.md)
- [09 History Git2](09-history-git2.md)
- [10 Recovery API](10-recovery-api.md)
- [11 Save](11-save.md)
- [12 Recovery filesystem](12-recovery-fs.md)
- [13 Search API](13-search-api.md)
- [14 Search SQLite](14-search-sqlite.md)
- [15 Export API](15-export-api.md)
- [16 Export HTML](16-export-html.md)
- [17 Editor API](17-editor-api.md)
- [18 Spellcheck API contract](18-spellcheck-api-contract.md)

## Owning crate(s)

[`parchmint-test-support`](../../docs/architecture/crates/parchmint-test-support.md)

## Requirements and UI design

- [Save, recovery, and closing](../../docs/product/save-recovery-and-closing.md)
- [Scale and performance](../../docs/product/scale-and-performance.md)

## Work

- Add controlled executors and faulting wrappers for filesystem, History, search, recovery, and editor adapters.

## Stage-specific tests and validation

Verify named fault points can pause, fail, cancel, and reorder service work without timing sleeps or writes outside each temporary project.
