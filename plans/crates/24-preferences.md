# Preferences

## Goal

Persist application-only preferences and publish appearance changes to every window.

## Depends on

- [23 Design system](23-design-system.md)

## Owning crate(s)

[`parchmint-preferences`](../../docs/architecture/crates/parchmint-preferences.md)

## Requirements and UI design

- [Appearance](../../docs/product/appearance.md)
- [Spellcheck](../../docs/product/spellcheck.md)

## Work

- Implement versioned preference storage, revision-checked updates, recent projects, global dictionary, System/Light/Dark resolution, and numbered theme snapshots.

## Stage-specific tests and validation

Test stale preference writes, unreadable-file preservation, System appearance events, stable all-window theme ordering, and exclusion from project save, undo, History, and export.
