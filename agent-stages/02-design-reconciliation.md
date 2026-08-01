# S10 — Design Reconciliation

## Goal

Translate the approved Penpot handoff into a durable, reviewable implementation interpretation. Do not create the Tauri application or production UI.

## Inputs

- Approved design handoff.
- Product specification.
- Final architecture.
- Design handoff contract.
- Implementation plan.
- S00 handoff.
- Templates under `templates/design-reconciliation/`.

## Tasks

1. Validate the design manifest, paths, checksums, component IDs, screens, states, tokens, assets, accessibility annotations, and platform variants.
2. Compare the approved design with every relevant product requirement.
3. Map Penpot components and screens to proposed implementation components and workspace states.
4. Define deterministic token and asset import plans.
5. Define visual-regression fixtures, capture sizes, and tolerances.
6. Identify conflicts, missing states, implementation ambiguities, and accessibility concerns.
7. Produce a work breakdown aligned with the implementation plan.
8. Do not silently resolve a product/design conflict.

## Required repository outputs

```text
docs/design/reconciliation/<handoff-version>/
├── design-reconciliation.md
├── implementation-map.yaml
├── visual-regression-plan.md
├── open-issues.yaml
├── work-breakdown.md
└── approval.yaml
```

Create `approval.yaml` with `status: pending`.

## Stage result

- Use `needs_approval` when the package is complete.
- List every blocking issue by ID.
- Stop at G10. Do not dispatch implementation stages.
