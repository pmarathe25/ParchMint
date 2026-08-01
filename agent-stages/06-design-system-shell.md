# S50 — Design System and Application Shell

## Goal

Import the approved design deterministically and implement the navigable shell against mock application services.

## Tasks

1. Validate the approved reconciliation and implementation map.
2. Generate CSS/custom-property and TypeScript token artifacts from the handoff.
3. Import and checksum approved assets.
4. Implement shared accessible components and layout primitives.
5. Implement launcher; the top destinations Editor, Cards, History, Recently Deleted, Export, and Settings; resizable/collapsible Explorer/editor/Inspector shell; one always-visible shared formatting-toolbar region; tabs and companion-pane shell; tree and full-hierarchy Cards fixtures; Inspector/settings shells; Global Search opened from the Explorer header with no v1 scope selector; replacement-preview shell; History/Recently Deleted shell; menus; and dialogs using mocks.
6. Add keyboard/focus behavior and semantic command routing.
7. Add deterministic screenshot and accessibility-tree fixtures.
8. Record every intentional deviation; trigger G20 rather than silently changing design behavior.

## Required outputs

- Generated design tokens/assets and manifests.
- Component map updated with implementation paths.
- Shell implementation and visual/accessibility tests.
- Stage handoff identifying mock service contracts and UI extension points.

## Pass criteria

- Reference screens reconcile within approved tolerances.
- Keyboard navigation/focus tests pass.
- Shell launches on Windows, macOS, and Linux.
- React components contain no duplicated domain logic.
