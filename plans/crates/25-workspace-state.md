# Workspace state

## Goal

Persist per-project workspace arrangement outside authored project data.

## Depends on

- [03 Domain](03-domain.md)

## Owning crate(s)

[`parchmint-workspace-state`](../../docs/architecture/crates/parchmint-workspace-state.md)

## Requirements and UI design

- [Workspace shell](../../docs/product/workspace-shell.md)

## Work

- Store pane layout, sections, tabs, active view, view state, and mode by project identity in versioned application-data files.

## Stage-specific tests and validation

Test restoring valid references, dropping deleted-node references, invalid-file fallback, and workspace-save failures that do not affect project saving or History.
