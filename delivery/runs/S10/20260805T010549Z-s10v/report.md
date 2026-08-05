# Stage Validation Report

**Stage:** S10
**Run:** 20260805T010549Z-s10v
**Run role:** validation
**Result:** failed
**Baseline commit:** `1594f396fd0fa4f60ec235e396104f949b42376b`
**Candidate commit:** `fa85a0b31ec9ca9cc9d1765520825cc67d958551`
**Output commit:** pending

## Scope and output

- Candidate and independent-test commits reviewed: frozen repair output
  `fa85a0b`, substantive handoff `238920e`; S10 is non-production and
  independently-test exempt. PR head `0dd515e` contains only this validation
  dispatch/run metadata.
- PR and CI reviewed: draft PR #2 is open and merge-clean at `0dd515e`; its
  check rollup is empty because CI is introduced by S20.
- Requirements, design IDs, contracts, and state owners checked: all 79
  component rows, 80 screen rows, 20 candidate references, canonical v1 state
  constraints, mapping coverage, architecture ports/state owners, and G20/G10
  boundaries.

## Validation

- Commands and evidence: checksum verification, the 17-check repair validator,
  diff/immutability checks, PR metadata, native-archive structure and source
  identity, plus direct inspection of all ten Light/Dark pairs. Structured
  evidence is `evidence/validation.json`.
- Result and evidence limits: failed. Static/package checks pass, but the Dark
  dual-editor reference is visibly unusable and traceability omits at least
  three demonstrably applicable mappings. Static evidence does not prove native
  interaction, accessibility, performance, IME, screen readers, spellcheck, or
  platform behavior.
- Deviations and exact next action: stop before G10. Obtain product-owner
  direction; if approved, repair/re-export the Dark dual-editor baseline,
  complete or explicitly disposition applicable mapping gaps, and rerun fresh
  validation.

## Findings

1. `editor-dual-dark.png` shows both manuscript panes/prose nearly black or
   dimmed while the Inspector remains normally visible. No blocking state
   explains the scrim-like presentation. The Light baseline correctly shows two
   readable Manuscript documents, and the source archive indicates light prose
   fills. This blocks `APPR-005/008`, `EDIT-001–005`, `WS-006`, and
   `TREE-017`.
2. Seventy-four traceability rows have no Penpot screen or component mapping.
   At least `WS-011`, `SAVE-014`, and `EXP-009` are directly represented
   in candidate specs/inventories but lack a mapping or blocking disposition.
3. The other nine Light/Dark pairs are readable and coherent. The repaired
   recovery pair shows the intended recovery summary, retained History
   checkpoint, and Discard/Recover choices. Canonical Search, Export, restore,
   spellcheck, comments-filter, and Entire Manuscript metadata checks pass.
