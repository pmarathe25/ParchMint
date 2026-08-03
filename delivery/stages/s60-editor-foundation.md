# S60 — Production ProseMirror Editor Foundation

## Goal

Implement the production editor adapter using the S55-selected shared-state mechanism and projection strategy, then run the early packaged native runtime gate.

## Tasks

- Complete v1 schema/marks/lists/quotes/stable IDs/Scene/Page Breaks/literal tabs.
- Deterministic canonical parse/serialize and paste sanitization.
- Shared session with two independent view sessions, shared history, selection mapping, composition handling.
- One shared toolbar, attach/detach/restore, local Find.
- Selected projection/recovery implementation, bounded queues, resync, revision acknowledgements.
- Foundational comment anchors/decorations/context-menu/Comments-panel commands.
- Project-document-operation boundary for composite project commands.
- Packaged release-mode native ordinary/250k one/two-view checks on all platforms.

## Restrictions

No size-based mode, feature disabling, broad fork, or canonical coupling to ProseMirror internals.

## Pass criteria

Canonical fidelity, two-view correctness, shared undo, input/open/projection/memory budgets, IME/clipboard/BiDi/tabs/paste, screen-reader editing, and projection failure recovery pass.

Failure stops at G20.
