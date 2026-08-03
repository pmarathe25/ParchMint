# S10 — Design Reconciliation

## Goal

Translate the approved handoff into a durable implementation interpretation. Do not create the Tauri application or production UI.

## Tasks

1. Validate manifest, checksums, component IDs, screens, states, Light/Dark tokens, assets, accessibility annotations, and platform variants.
2. Compare design against every relevant requirement, including Appearance, word-count scope, and spellcheck settings.
3. Map components/screens to implementation components/workspace states.
4. Populate `penpot_screen_ids` and `penpot_component_ids` in `delivery/traceability.csv` for every applicable requirement. Record any applicable must-level requirement without an approved design mapping as a blocking issue.
5. Define deterministic token/asset import plans and generated-drift checks.
6. Define Light/Dark visual-regression fixtures, sizes, scales, tolerances.
7. Identify conflicts, missing states, hard-coded theme values, implementation ambiguities, and accessibility concerns.
8. Produce work breakdown aligned with S50 and later feature slices.
9. Do not silently resolve product/design conflicts.

## Outputs

```text
delivery/design-reconciliation/<handoff-version>/
├── design-reconciliation.md
├── implementation-map.yaml
├── visual-regression-plan.md
├── open-issues.yaml
├── work-breakdown.md
└── approval.yaml
```

Also update `delivery/traceability.csv` with the approved design mappings. S10 may return `needs_approval` only when every applicable requirement has a design mapping or a corresponding blocking entry in `open-issues.yaml`.

Create approval as `pending`; return `needs_approval`; stop at G10.
