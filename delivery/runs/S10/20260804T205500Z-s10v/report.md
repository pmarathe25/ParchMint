# Stage Validation Report

**Stage:** S10
**Run:** 20260804T205500Z-s10v
**Run role:** validation
**Result:** failed
**Baseline commit:** `1594f396fd0fa4f60ec235e396104f949b42376b`
**Candidate commit:** `378150aadf5a4cca04c9c7a4a5634dfc6c238b20`
**Output commit:** pending Orchestrator evidence commit

## Scope and output

- Candidate and independent-test commits reviewed: substantive content candidate `83fc7ac`; implementation run-artifact output `378150a`; S10 is non-production and independent-test exempt. PR-head commit `c3378fa` contains only this validation dispatch and was not attributed to implementation ownership.
- PR and CI reviewed: draft PR #2 is open and merge-clean at `c3378fa`; its check rollup is empty because CI is introduced by S20.
- Requirements, design IDs, contracts, and state owners checked: all 185 applicable requirement mappings, 79 components, 80 screen rows/70 boards, and 20 approved PNGs. State-owner and port interpretations remain aligned with the architecture.

## Validation

- Commands and evidence: checksum verification, S10 validator, traceability derivation, diff check, PR metadata, complete source reconciliation, and direct inspection of all ten Light/Dark image pairs. Structured evidence is `evidence/validation.json`.
- Result and evidence limits: failed. Structural commands pass, but the candidate omits two material frozen-handoff/reference mismatches and leaves implementation output provenance incomplete. Static screenshots do not prove native or interactive behavior.
- Deviations and exact next action: stop before G10. Obtain product-owner direction for the recovery and dual-editor baseline identities; then record ISSUE-008/009, align or revise/re-export the handoff, correct implementation output provenance, and rerun fresh S10 validation.

## Findings

1. `error-recovery-light.png` and `error-recovery-dark.png` visibly present an unreadable/corrupt canonical document, while the manifest, inventory, and fixture declare recovered-after-crash behavior bound to SAVE-011/012/013. The reconciliation and visual plan do not record this mismatch.
2. `editor-dual-light.png` and `editor-dual-dark.png` visibly present Manuscript plus Research (`Harbor Notes`), while the manifest/inventory identify two Manuscript documents. The fixture calls the visible state an equivalent rather than matching the baseline identity. The reconciliation does not record this mismatch.
3. The implementation run correctly distinguishes substantive candidate `83fc7ac` from artifact-only `378150a`, but `output_commit` remains blank and `approval.yaml.reconciliation_commit` is blank. The minimum repair is to preserve candidate `83fc7ac` and record output/reconciliation commit `378150a`.
4. ISSUE-001 through ISSUE-004 are confirmed conflicts. ISSUE-005 is an unresolved ambiguity: filtering one continuous list could satisfy CMT-003, while separate resolved/unresolved sections would not. ISSUE-006/007 correctly state evidence limits.
5. `interaction-spec.md` contains the editorial phrase `Entire Manual centroid`; visible export references and governing requirements consistently say `Entire Manuscript`, so this does not alter scope.
