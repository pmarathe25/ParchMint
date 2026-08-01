# S100 — Feature-Wave Planning and Dispatch

## Goal

Convert the approved work breakdown and remaining v1 traceability gaps into bounded, dependency-aware feature-slice tasks, then dispatch them automatically.

## Inputs

- Accepted S90 foundation.
- Approved reconciliation `work-breakdown.md` and `implementation-map.yaml`.
- PRD, Penpot manifest, implementation plan, acceptance plan, and current traceability matrix.

## Tasks

1. Identify every unimplemented v1 requirement and required design state.
2. Group work into vertical slices that can be completed and tested end to end.
3. Define dependencies and exclusive file/contract ownership.
4. Write one generated task per slice under `agent-workflow/generated-tasks/<wave-id>/` using the supplied task template.
5. Dispatch independent slices using `agent-stages/12-feature-slice-agent.md`.
6. Verify and integrate slices using the Orchestrator Agent’s normal acceptance rules.
7. Recompute traceability and create additional waves until all v1 requirements are implemented or blocked.

## Suggested slice families

Use these only when they fit the approved design/work breakdown:

- Project launcher/create/open and workspace restoration state.
- Explorer hierarchy operations, multi-selection, drag/drop, cut/copy/paste.
- Cards full-hierarchy projection, title/Synopsis editing, and read-only metadata presentation; metadata values are edited through Inspector.
- Inspector Synopsis/metadata and field-definition settings.
- Rich formatting/style management.
- Comments and replies.
- Local search/replace.
- Entire-project Global Search from the Explorer header, with no v1 scope selector, plus central replacement preview.
- History with whole-project checkpoint restore, plus Recently Deleted subtree restoration.
- Entire-Manuscript HTML export with no partial-scope or per-node inclusion controls.
- Word counts and conditional spellcheck.

Do not place two agents on the same public contract or source files concurrently.
