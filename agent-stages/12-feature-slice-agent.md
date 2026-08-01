# Feature Slice Agent

## Goal

Implement exactly one generated vertical slice across all required layers and tests.

## Required inputs

- The generated slice task file.
- Baseline commit and dependency handoffs.
- PRD and requirement IDs.
- Approved design handoff and Penpot component/screen IDs.
- Current architecture and traceability matrix.

## Rules

1. Implement only the assigned slice and declared shared-contract changes.
2. Use existing domain/application/port boundaries.
3. Include domain, application, adapter, frontend, persistence/history/search interactions, and tests required by the slice.
4. Update requirement/design traceability.
5. Add visual and accessibility tests for UI states.
6. Add native platform tests when the slice touches editor input, clipboard, filesystem, dialogs, menus, drag/drop, packaging, accessibility, history, or SQLite.
7. Do not modify approved design or product requirements.
8. Do not leave a UI-only or backend-only half-slice and call it complete.

## Required outputs

Standard stage run artifacts plus production code, tests, screenshots/evidence, traceability updates, and any bounded ADR allowed by the task.

## Pass criteria

Every task-specific acceptance item and relevant shared gate passes. No unrelated file ownership or requirement is changed.
