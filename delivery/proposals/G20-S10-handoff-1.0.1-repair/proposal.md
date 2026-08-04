# G20 — Correct the design handoff to canonical v1 requirements

**Baseline:** `de4e778f885118b536d524da89510b011a6262d9`
**Affected stage:** S10
**Affected approved handoff:** `delivery/design-handoff/1.0.0/`
**Candidate handoff:** `delivery/design-handoff/1.0.1/`

## Evidence

Separate S10 validation found that component metadata and two reference exports do not consistently represent the canonical v1 product requirements. The approved `1.0.0` package remains immutable.

- `delivery/runs/S10/20260804T205500Z-s10v/evidence/validation.json`
- `delivery/runs/S10/20260804T224650Z-s10x/evidence/penpot_probe.json`

The live Penpot probe confirmed the recovered-after-crash board is correct. It also confirmed the board named `editor-dual-two-manuscript` still contains visible Research/Harbor Notes content and must be corrected before export.

## Approved direction

The product specification remains canonical. Produce a complete patch handoff `1.0.1` that:

1. Corrects the dual-editor Penpot board and exports true two-Manuscript Light/Dark references.
2. Exports the existing recovered-after-crash board instead of the corrupt-canonical-file board.
3. Removes unsupported subtree Search, partial Export, partial History restore, and per-document spellcheck states from handoff component metadata.
4. Clarifies that any resolved-comments filter narrows one continuous unsectioned list; it must not create resolved/unresolved sections.
5. Corrects the `Entire Manual centroid` editorial typo to `Entire Manuscript`.
6. Preserves all product, architecture, state-owner, canonical-format, accessibility, performance, and platform requirements.

The exact checksum-valid `1.0.1` candidate and its reconciliation still require product-owner approval before replacing the active handoff or advancing past G10.
