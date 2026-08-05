# S10 Draft 1.0.1 Integration Repair

**Result:** Draft candidate repaired; ready for a fresh independent Terra validation. G10 remains pending.
**Candidate/output commit:** `450e705238f381f3ab56cec389134adc8fae43d7`

## What changed

- Replaced the draft source provenance with the refreshed live Penpot archive and regenerated the 1.0.1 checksum inventory.
- Recorded and directly reviewed repaired Light/Dark dual-editor references: both show Manuscript and Research roots plus Harbor Notes while retaining Chapter One and Chapter Two in separate panes. Dark is fully dark and readable.
- Made `TREE-001` explicit: whenever Explorer is shown, both roots remain visible even with no Research document open. Only Global Search replacement and deliberately collapsed/hidden Explorer states are exceptions.
- Preserved the distinct same-document-two-views board as the evidence for independent scroll/selection state; it was not substituted for the two-document board.
- Added inventory-first mappings for `PRJ-010`, `WS-011`, `APPR-006`, `FMT-019`, `META-011`, `CARD-010`, `WORD-002`, `A11Y-005`, `A11Y-007`, and `A11Y-009`, plus seven Page 13 reference rows for stable platform/layout mapping. `SPELL-004` maps only to the Spelling Context Menu's visible recoverable `dictionaryerror` state; `SPELL-005` maps to both its word-anchored menu and the in-place underline.
- Preserved the failed validation. New adjudication records that `SAVE-014` and `EXP-009` were already mapped; `WS-011` was genuinely blank and is now mapped.
- Classified every still-unmapped requirement as deferred, nonvisual/state, native/release-only, performance, or security rather than inventing visual proof.

## Verification

- `checksums.sha256` verifies all 1,016 handoff files.
- Refreshed source ZIP passes `unzip -t`; source inspection confirms the two Explorer shapes visible and the two obsolete pane tabs hidden.
- `file` and the repair validator confirm both editor-dual PNGs are 1440×900; the validator also checks inventory annotations, traceability, and dispositions.
- Scoped `git diff --check` passes.

`identify` was unavailable, so exact dimensions were read from each PNG IHDR by the validator instead. This is a tooling limitation, not a claim about native visual behavior.

## Remaining gates

The candidate is still a draft. A fresh Terra validation must review the exact candidate and evidence before the product owner is asked to approve G10.
