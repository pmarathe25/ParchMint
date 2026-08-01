# S10 — Design Reconciliation

## Goal

Translate the approved handoff into a durable implementation interpretation. Do not create the Tauri application or production UI.

## Tasks

1. Validate manifest, checksums, component IDs, screens, states, Light/Dark tokens, assets, accessibility annotations, and platform variants.
2. Compare design against every relevant requirement, including Appearance, word-count scope, and spellcheck settings.
3. Map components/screens to implementation components/workspace states.
4. Define deterministic token/asset import plans and generated-drift checks.
5. Define Light/Dark visual-regression fixtures, sizes, scales, tolerances.
6. Identify conflicts, missing states, hard-coded theme values, implementation ambiguities, and accessibility concerns.
7. Produce work breakdown aligned with S50 and later feature slices.
8. Do not silently resolve product/design conflicts.

## Outputs

```text
docs/design/reconciliation/<handoff-version>/
├── design-reconciliation.md
├── implementation-map.yaml
├── visual-regression-plan.md
├── open-issues.yaml
├── work-breakdown.md
└── approval.yaml
```

Create approval as `pending`; return `needs_approval`; stop at G10.
