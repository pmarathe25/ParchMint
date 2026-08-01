# Build Kit Changelog

## 1.2 — 2026-07-31

- Reconciled the automated v1.1 agent workflow against `pmarathe25/ParchMint@4fe7976acd526faf0faf4e5fa3834bfb3bc2f742`.
- Preserved product specification 2.0, final architecture 1.2, Penpot design brief 2.3, acceptance plan, future-work changes, decisions/evidence, and the updated design-manifest template.
- Merged whole-project History restore and context-menu-only comment creation into the automated implementation and stage instructions.
- Removed stale stage instructions for a floating selection-end comment affordance and user-visible Global Search scopes.
- Added current v1 guardrails for whole-project search, entire-Manuscript export, read-only Cards metadata, and the always-visible shared toolbar.
- Repointed orchestration instructions to the repository's actual `docs/...` locations.
- Added a baseline-verification script and reconciliation record so this overlay can be applied without overwriting protected product/design files.

## 1.1 — 2026-07-30

- Replaced prompt-by-prompt implementation guidance with a repository-backed Orchestrator Agent workflow.
- Left the design-agent and product-owner design-review steps unchanged.
- Defined G10 design reconciliation, G20 material deviation, and G90 release approval as the only normal product-owner review gates.
- Added standalone stage instructions and machine-readable workflow/reconciliation templates.
