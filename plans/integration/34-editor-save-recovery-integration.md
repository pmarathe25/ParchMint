# Editor save and recovery integration

## Goal

Prove editor revisions, canonical projections, saves, and recovery records stay consistent.

## Depends on

- [11 Save](../crates/11-save.md)
- [12 Recovery filesystem](../crates/12-recovery-fs.md)
- [19 Test-support services](../crates/19-test-support-services.md)
- [20 Application](../crates/20-application.md)
- [30 Desktop](../crates/30-desktop.md)
- [31 Editor core](../crates/31-editor-core.md)
- [33 Editor Iced](../crates/33-editor-iced.md)

## Owning paths

Cross-crate editor, save, and recovery integration tests and fixtures.

## Requirements and UI design

- [Save, recovery, and closing](../../docs/product/save-recovery-and-closing.md)
- [Undo and redo](../../docs/product/undo-and-redo.md)
- [Scale and performance](../../docs/product/scale-and-performance.md)

## Work

- Join editor projections to revisioned save vectors and recovery batches without blocking mounted editor input.

## Stage-specific tests and validation

Pause projection, save, and recovery at named boundaries; verify acknowledged revisions survive/replay exactly, newer edits remain dirty, projection failure prevents Saved, recovery resumes after forced termination, and continuous typing does not create an unbounded projection or recovery backlog.
