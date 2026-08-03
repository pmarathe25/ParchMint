# S50 — Design System and Application Shell

## Goal

Import the approved Light/Dark design deterministically and implement a navigable accessible shell against mock services.

## Tasks

1. Validate approved reconciliation/map.
2. Generate semantic Light/Dark CSS and TypeScript metadata from handoff tokens.
3. Add deterministic regeneration/dirty-diff CI.
4. Import/checksum assets.
5. Implement System/Light/Dark preference and generation-ordered live propagation across mock windows; initialize each window before first paint and expose durable-write failure without project dirty/save/history effects.
6. Implement shared components and shell destinations/states from the approved design.
7. Implement tree/Card virtualization, selection, drag/drop, tabs, splitters, menus/dialogs, keyboard/focus.
8. Implement Appearance and dictionary settings shells; no per-document language UI.
9. Establish Light/Dark visual/accessibility fixtures.

## Pass criteria

- Complete deterministic theme output.
- No hard-coded theme-dependent production values.
- Dark manuscript canvas is dark.
- Appearance updates all windows without project mutation.
- New-window first paint, stale system-event rejection, restart persistence, and preference-write failure tests pass.
- Reference/focus/accessibility shell tests pass.
- Native shell launches on all platforms.
