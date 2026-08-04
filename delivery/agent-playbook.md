# ParchMint Agent Playbook

**Status:** Current automated workflow
**Version:** 1.5
**Date:** 2026-08-04

## 1. Overview

ParchMint uses a review-gated repository pipeline rather than loosely connected prompts.

Roles:

1. **Design Agent:** creates/remediates Penpot and exports the immutable handoff.
2. **Orchestrator Agent:** owns pipeline state, dispatch, independent verification, integration, and approval stops.
3. **Stage Agents:** execute one bounded stage or generated slice.
4. **Independent Test Agent:** seals a requirements-first charter, then authors test-only changes against a candidate without using implementation reasoning as its oracle.
5. **Validation Agent:** independently evaluates the integrated release candidate.

The product owner controls product behavior, approved design versions, material architecture changes, distribution/licensing/security exceptions, and final release approval.

## 2. Review gates

- **G10 — Design reconciliation approval:** implementation interpretation of the approved handoff.
- **G20 — Material deviation approval:** product/design/selected technology/canonical format/state ownership/licensing/security/mandatory requirement must change.
- **G90 — Release approval:** independent release evidence.

All other objective stage gates may advance automatically.

## 3. Durable communication

Agents communicate through committed repository artifacts, not remembered conversation context. Every stage receives a baseline commit and dependency handoffs and produces a versioned status, handoff, report, and evidence directory.

Current governing documents are updated directly after approved changes. Do not create changelogs, ADRs, historical exploration records, or alternate decision logs.

## 4. Current design status

The implementation pipeline is not ready until a final product-owner-approved handoff is committed under:

```text
delivery/design-handoff/<version>/
```

The approved handoff must include:

- Remediated Light and Dark token sets/components/references.
- System/Light/Dark Appearance setting and behavior.
- Fully dark manuscript canvas in Dark.
- All files required by the handoff contract.
- Valid checksums and matching governing versions.

S00 must block rather than infer approval from a live Penpot file or previous conversation.

## 5. Design-agent setup

Give the Design Agent:

- `AGENTS.md`
- `docs/product/product-specification.md`
- `docs/design/penpot-design-brief.md`
- `delivery/design-handoff-contract.md`
- `delivery/templates/design-handoff/design-manifest.yaml`
- `delivery/templates/pipeline/traceability.csv`

### Initial/remediation prompt

> Read all supplied ParchMint documents before changing Penpot. Use the design brief as the operational brief and the product specification as the behavior source of truth. Connect to the open Penpot file. Complete or remediate the semantic Light/Dark token system, System/Light/Dark Appearance setting, shared components, screens, states, accessibility annotations, and prototype flows. Dark must use fully dark application and manuscript surfaces. Do not add deferred features or resolve product conflicts silently. Use stable PM names and requirement IDs. Report current blockers and handoff gaps. Do not mark or export an approved final handoff until the product owner approves the design.

### Iteration prompt

> Apply only approved feedback to the existing Penpot design. Update component mains and instances rather than isolated boards. Preserve stable IDs/names where practical. Update affected screens, Light/Dark references, appearance matrix, and handoff inventory. Do not make unrelated product changes.

### Final handoff prompt

> Freeze the approved ParchMint design as `<VERSION>` and produce every artifact required by the handoff contract: `.penpot`, complete Light/Dark token JSON, used assets, deterministic Light/Dark reference PNGs, manifest, interaction specification, component matrix, screen inventory, keyboard/focus map, appearance matrix, cross-platform variants, asset/font provenance, deterministic reference fixtures, known deviations, and SHA-256 checksums. Validate the manifest schema, paths/checksums, theme coverage, and governing versions. Report incomplete items instead of omitting them.

## 6. Product-owner design review

Review:

- Core information architecture and shell.
- Editor/companion/Inspector focus.
- Cards as the same hierarchy projection.
- Deep tree/multi-selection.
- Comments and context-menu creation.
- Search/replace preview.
- History/Recently Deleted.
- Save/error/recovery.
- Appearance System/Light/Dark and fully dark canvas.
- Light/Dark focus/contrast/state treatment.
- Spellcheck/dictionary settings presentation.
- Keyboard/focus/accessibility.
- 1280×720/high-DPI/cross-platform variants.

Approval identifies an immutable handoff version.

## 7. Start the pipeline

After the approved handoff is committed, give one lead agent:

- Complete repository/build kit.
- Repository access.
- Native/CI access for Windows, macOS, Linux.
- `delivery/stages/orchestrator.md`.

Prompt:

> Read `AGENTS.md`, `delivery/agent-playbook.md`, and `delivery/stages/orchestrator.md`. Initialize the repository-backed pipeline and execute from S00. Automatically delegate each bounded stage and independent non-overlapping workstream to fresh subagents using isolated branches/worktrees where supported. Pair each production-behavior run with the requirements-first independent test challenge defined by the playbook; retain orchestration, verification, integration, and approval decisions in the primary agent. Do not rely on prior chat context. Stop only at G10, G20, G90, a defined stop condition, or an external credential/input requirement.

## 8. Approve/resume G10

S10 writes:

```text
delivery/design-reconciliation/<handoff-version>/
├── design-reconciliation.md
├── implementation-map.yaml
├── visual-regression-plan.md
├── open-issues.yaml
├── work-breakdown.md
└── approval.yaml
```

Review it, resolve blockers, set `approval.yaml` to `approved`, and commit.

Resume prompt:

> Resume from the approved G10 reconciliation. Revalidate its commit, handoff checksum, theme coverage, and continue automatically. Stop only at G20, G90, a defined stop condition, or unresolved external input.

## 9. Stage instruction catalog

The canonical ordering and dependencies are in `delivery/implementation-plan.md`.

| Stage | Instruction file |
|---|---|
| S00 | `delivery/stages/s00-repository-intake.md` |
| S10 | `delivery/stages/s10-design-reconciliation.md` |
| S20 | `delivery/stages/s20-repository-bootstrap.md` |
| S30 | `delivery/stages/s30-contracts-domain-format.md` |
| S40 | `delivery/stages/s40-persistence-recovery.md` |
| S50 | `delivery/stages/s50-design-system-shell.md` |
| S55 | `delivery/stages/s55-editor-feasibility.md` |
| S60 | `delivery/stages/s60-editor-foundation.md` |
| S65 | `delivery/stages/s65-spellcheck-foundation.md` |
| S70 | `delivery/stages/s70-history-git2.md` |
| S80 | `delivery/stages/s80-search-sqlite.md` |
| S90 | `delivery/stages/s90-foundation-integration.md` |
| S100 | `delivery/stages/s100-feature-wave-orchestrator.md` |
| Slice | `delivery/stages/feature-slice.md` |
| S110 | `delivery/stages/s110-system-integration-validation.md` |
| S120 | `delivery/stages/s120-release-hardening.md` |
| S130 | `delivery/stages/s130-release-validation.md` |
| Test | `delivery/stages/independent-test-author.md` |
| Change | `delivery/stages/design-change-reconciliation.md` |

## 10. Stage handoff contract

Every dispatched stage receives:

- Baseline commit SHA.
- Stage instruction.
- `delivery/state.yaml`.
- Approved gates.
- Dependency handoffs.
- Relevant governing/design documents.

Every stage creates:

```text
delivery/runs/<stage-id>/<run-id>/
├── dispatch.yaml
├── status.yaml
├── handoff.yaml
├── report.md
└── evidence/
```

A `test_charter` run also creates `test-charter.yaml` from the pipeline template and commits it before candidate access.

Allowed results: `passed`, `failed`, `blocked`, `needs_approval`.

`status.yaml` records baseline/output commits, files, commands, platforms, requirements, blockers, deviations, and recommended next stages.

`handoff.yaml` records stable outputs: contract/schema versions, ports, generated bindings, fixtures/tests, commands, known approved limitations, and exact artifact paths.

## 11. Independent test challenge

Every dispatch that produces production behavior declares `independent_test.required`. `delivery/implementation-plan.md` owns the default stage applicability and exception list. A non-production exemption always requires a recorded reason.

The Orchestrator runs the challenge as two linked repository runs:

1. Dispatch a fresh `test_charter` run from the governing inputs, dependency handoffs, acceptance criteria, stage instruction, and task. Withhold the candidate, implementation report, implementation conversation, and diff explanation.
2. The agent writes and commits `test-charter.yaml` with the standard run artifacts. The charter records observable behaviors, public surfaces, test levels, negative/fault cases, fixtures, native evidence, ambiguities, and test ownership.
3. The implementation agent produces a candidate plus developer tests on its isolated branch.
4. Dispatch a linked `independent_test` run to a fresh or resumed Independent Test Agent with both `baseline_commit` and `candidate_commit` set to the implementation output commit, plus the sealed charter, public contracts/generated schemas, test-support surfaces, and required commands. Do not give it the implementation agent's reasoning.
5. The Independent Test Agent commits only dispatched tests, fixtures, and run artifacts. Its tests may be public-API unit tests, shared contract tests, property/golden/fault tests, or cross-component/black-box integration tests. It does not edit production code or acceptance criteria.
6. Run the candidate and independent-test commits together on a temporary integration candidate. Mainline acceptance waits until the paired commits and applicable tiers pass.

Independence is procedural, not a claim of filesystem isolation. Production bodies must not be used as the expected-behavior oracle. If a test cannot observe required behavior through an existing contract or harness, the agent reports a testability gap rather than adding shipped test-only behavior or changing a public boundary.

When an independent test exposes a defect, preserve the failed run and test commit and dispatch a production repair with the test commit as its baseline. The implementation or repair agent may not edit that test. After repair, dispatch a linked `independent_test` verification run against the repaired candidate and preserved tests; only the passing verification run is accepted. If the test is wrong or the governing input is ambiguous, return it to the Independent Test Agent or Orchestrator; record the correction and evidence instead of silently weakening the assertion.

## 12. Automatic verification/integration

The Orchestrator independently confirms:

1. Correct baseline and clean branch.
2. Required run artifacts parse.
3. Diff stays inside ownership.
4. Governing docs/approved handoff were not modified without G20, except an independently verified S55 projection-section concretization already authorized by the architecture.
5. Mandatory commands and test tier pass.
6. Required Windows/macOS/Linux evidence exists.
7. Generated contract/token output is clean.
8. No unapproved dependency/provenance/product/architecture deviation.
9. Integration-branch post-merge tests pass.
10. Every required `test_charter` run completed without candidate access, the linked independent-test diff stays within test ownership, and the paired candidate/test commits pass together.
11. Independent-test traceability fields, pipeline state, and accepted implementation/test handoff pointers update atomically in the Orchestrator's acceptance commit.

One bounded repair attempt is allowed for an ordinary implementation/test defect. Repeated failure, broad fork, data-loss risk, or governing change stops at G20.

## 13. G20 material deviation

Create a temporary proposal under:

```text
delivery/proposals/<proposal-id>/
├── proposal.md
├── options.yaml
├── evidence/
└── approval.yaml
```

G20 is required for a must-level behavior/design change; selected framework/backend/engine change; canonical/state-owner/public-boundary change; material licensing/security/privacy exception; or weakened save/recovery/performance/accessibility/cross-platform requirement.

The proposal presents reproducible evidence and bounded options. The agent does not choose for the product owner.

After approval:

1. Update the current governing documents/contracts.
2. Commit the approval and current-document updates together or in an explicitly linked sequence.
3. Regenerate affected tasks.
4. Resume from repository state.

The proposal is pipeline working material, not a permanent alternate decision history.

## 14. Feature-wave automation

S100 reads approved reconciliation, implementation plan, traceability, accepted foundation handoffs, and current code. It creates one task per bounded vertical slice under `delivery/generated-tasks/<wave-id>/`.

Each task includes requirements/design IDs, dependencies/ownership, domain/application/adapter/frontend/persistence/history/search/spellcheck work, developer-test work, independent-test requirement or exemption, test tier, commands/platforms, and stop conditions.

Do not dispatch overlapping file/public-contract ownership in parallel.

## 15. Continuous validation

- Stage agents test their changes.
- Independent Test Agents add requirements-first tests before applicable stage acceptance.
- Orchestrator reruns stage gates.
- S110 runs complete integrated suites and bounded fixes.
- S130 independently validates and produces the unified release package.
- A validation agent never changes criteria to make a candidate pass.

## 16. Release approval G90

S130 creates exactly:

```text
delivery/release-evidence/<candidate-version>/
├── requirement-disposition.csv
├── platform-matrix.yaml
├── visual/
├── performance/
├── accessibility/
├── appearance/
├── editor-projection/
├── spellcheck/
├── recovery-project-undo/
├── history-search/
├── packaging/
├── security-licenses-sbom/
├── package-hashes.txt
├── known-issues.yaml
└── release-approval.yaml
```

`release-approval.yaml` starts `pending`. The product owner approves, records current waivers by updating the current specification/release file, or requests fixes.

## 17. Design revision after implementation starts

1. Export a candidate handoff beside the active immutable handoff.
2. Dispatch `delivery/stages/design-change-reconciliation.md`.
3. Compare manifests/tokens/themes/components/screens/references/requirements.
4. Produce design diff, impact map, generated tasks, pending approval.
5. Pause only affected workstreams.
6. After approval, replace the active handoff, remove the superseded package when no run depends on it, update current design/governing inputs, and dispatch tasks.

Do not continuously sync a mutable live Penpot file into production.

## 18. Quality controls

- Small reviewable tasks and isolated worktrees.
- Sealed test charters and separate production/test ownership for required challenges.
- Exact commands/raw evidence.
- Prototypes clearly separate from production.
- Public-contract changes independently verified.
- Exact application locks and scheduled provenance checks.
- No fabricated native/runtime/accessibility/performance evidence.
- No conversation-memory dependency.
- No historical evidence document competing with current governing files.
