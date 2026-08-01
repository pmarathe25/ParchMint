# S100 — Feature-Wave Planning and Dispatch

## Goal

Generate and dispatch bounded vertical slices after every foundation, including spellcheck, is accepted.

## Inputs

- Accepted S90 handoff and dependency handoffs.
- Product/architecture/implementation/acceptance documents.
- Approved reconciliation/work breakdown.
- Traceability matrix.
- Current code and test inventory.

## Tasks

1. Compute remaining must-level requirements.
2. Group work into end-to-end slices with explicit ownership and dependencies.
3. Ensure project-command/undo, save/history/search/spellcheck/appearance implications are included where applicable.
4. Assign test tier and native platforms to each task.
5. Generate tasks under `agent-workflow/generated-tasks/<wave-id>/` using `templates/agent-task.yaml`.
6. Do not dispatch overlapping file/public-contract ownership in parallel.
7. Dispatch independent tasks using `agent-stages/12-feature-slice-agent.md`.
8. Replan after accepted slices without changing governing scope.

## Required slice coverage

Include global replacement as one project undo/checkpoint; word counts limited to selection/active document/Manuscript; full spellcheck UI on S65; Appearance final integration; and entire-Manuscript export.

## Stop conditions

Stop at G20 when remaining work requires a governing/public-boundary change, broad fork, or weakened mandatory gate.
