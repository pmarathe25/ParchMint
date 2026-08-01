# ParchMint Agent Playbook

**Status:** Final automated workflow  
**Version:** 1.2  
**Date:** 2026-07-31

## 1. Overview

ParchMint uses a review-gated agent pipeline rather than a sequence of loosely connected chat prompts.

Use four agent roles:

1. **Design Agent:** creates and iterates the Penpot design.
2. **Orchestrator Agent:** owns pipeline state, dispatches bounded stage agents, verifies gates, integrates passing work, and stops only at defined approval boundaries.
3. **Stage Agents:** implement one bounded stage or feature slice from a dedicated instruction file.
4. **Validation Agent:** independently evaluates integrated work and produces release evidence.

The product owner remains the authority for product behavior, approved design versions, material architecture changes, distribution/licensing exceptions, and final release approval.

### Review gates

Only these events require product-owner review or approval:

- **G10 — Design reconciliation approval.** The implementation interpretation of the approved Penpot handoff must be reviewed before broad UI work.
- **G20 — Material deviation approval.** Required only when an agent identifies a conflict with the PRD or approved design, proposes a selected-framework/backend change, changes a canonical format or architectural boundary, introduces a material licensing/security exception, or cannot satisfy a mandatory product requirement.
- **G90 — Release approval.** The independent release-evidence package is reviewed before release.

All other phase gates may be evaluated and advanced automatically by the Orchestrator Agent when their objective criteria pass.

### Durable communication rule

Agents communicate through committed repository artifacts, not through remembered conversation context. Every stage receives a baseline commit and prior handoff files, and produces a versioned status, handoff, report, and evidence directory. A subsequent agent must be able to proceed in a fresh session using only the repository.

## 2. Design-agent setup

In this repository, the governing inputs are located under `docs/product/`, `docs/design/`, and `templates/`; the filenames below remain the design-agent handoff labels.

Give the Design Agent:

- `AGENTS.md`
- `01-product-specification.md`
- `03-penpot-design-brief.md`
- `04-design-artifact-handoff-contract.md`
- `templates/design-manifest.yaml`
- `templates/traceability-matrix.csv`

Connect Codex to Penpot MCP using current Penpot and Codex MCP instructions. Prefer a project-scoped MCP configuration when the environment supports it.

### Design-agent initial prompt

> Read all supplied ParchMint documents before changing Penpot. Use `03-penpot-design-brief.md` as the operational brief and `01-product-specification.md` as the product source of truth. Connect to the open Penpot file through MCP. Create the file structure, tokens, components, screens, states, accessibility annotations, and prototype flows in the brief. Do not add deferred features or resolve product ambiguities silently. Use stable `PM/` names and cite requirement IDs. At the end of each pass, report completed pages, unresolved product questions, design decisions, and handoff gaps. Do not export the final handoff until I explicitly approve the design.

### Iteration prompt

> Apply the approved feedback to the existing Penpot design without breaking stable component IDs/names unnecessarily. Update components and instances rather than patching individual boards. Update the design decision log and identify any screens/reference images that must be re-exported. Do not make unrelated visual or product changes.

### Final handoff prompt

> Freeze the approved ParchMint design as version `<VERSION>`. Produce every artifact required by `04-design-artifact-handoff-contract.md`: `.penpot`, token JSON, used SVG assets, deterministic reference PNGs, `design-manifest.yaml`, interaction specification, component matrix, screen inventory, keyboard/focus map, cross-platform variants, design decisions, known deviations, and SHA-256 checksums. Validate that all manifest paths exist and that the product-spec version matches. Report any incomplete item rather than omitting it.

## 3. Product-owner design review

Before approving the handoff, review:

- Core layout and information architecture.
- Editor/companion/Inspector focus behavior.
- Cards as the same data projection.
- Deep tree and multi-selection states.
- Comments, editor-context-menu creation, and anchor indication.
- Search and replacement preview.
- History/Recently Deleted clarity.
- Save/error/recovery states.
- Keyboard/focus/accessibility boards.
- 1280×720 and high-DPI studies.
- Cross-platform variants.

Approval should identify a design version; do not approve an unversioned live file.

## 4. Start the automated implementation pipeline

After the approved Penpot handoff is committed under `design/handoff/<version>/`, give one lead agent:

- The complete build kit.
- Repository access.
- The approved design handoff.
- Native or CI access for Windows, macOS, and Linux.
- `agent-stages/00-orchestrator.md` as its operational instruction.

Use this initial prompt:

> Read `AGENTS.md`, `docs/09-agent-playbook.md`, and `agent-stages/00-orchestrator.md`. Initialize the repository-backed agent pipeline and execute it from Stage S00. Use fresh stage agents or isolated worktrees where supported. Do not rely on prior chat context. Stop only at an approval gate, a defined stop condition, or an external credential/input requirement that cannot be resolved from the repository.

The Orchestrator Agent will:

1. Validate repository intake and initialize `agent-workflow/` state.
2. Dispatch the design-reconciliation stage.
3. Stop at **G10** with a versioned reconciliation package.
4. After G10 approval, automatically dispatch, verify, integrate, and advance all stages that do not require product-owner review.
5. Stop at a material deviation, unrecoverable gate failure, or final release approval.

If the environment cannot create subagents, the Orchestrator Agent may execute stage files sequentially itself, but it must still use isolated branches/worktrees, stage artifacts, and the same gate rules.

## 5. Approve and resume after design reconciliation

The reconciliation stage writes:

```text
docs/design/reconciliation/<handoff-version>/
├── design-reconciliation.md
├── implementation-map.yaml
├── visual-regression-plan.md
├── open-issues.yaml
├── work-breakdown.md
└── approval.yaml
```

It also writes its stage evidence under:

```text
agent-workflow/runs/S10-design-reconciliation/<run-id>/
```

Review the reconciliation package. Resolve blocking issues, then change `approval.yaml` from `pending` to `approved` and commit the approval, or instruct the Orchestrator Agent to prepare that exact approval commit for you.

Resume with:

> Resume the pipeline from the approved G10 reconciliation gate. Revalidate the approval commit and continue automatically according to `agent-stages/00-orchestrator.md`. Stop only at G20, G90, a defined stop condition, or an unresolved external credential/input requirement.

A new Orchestrator Agent session may be used. Pipeline state must come from the repository, so no conversation handoff is required.

## 6. Stage instruction index

The Orchestrator Agent dispatches these instruction files. Do not paste informal summaries in place of them.

| Stage | Instruction file | Default behavior | Dependencies |
|---|---|---|---|
| S00 | `agent-stages/01-repository-intake.md` | Automatic | Approved design handoff |
| S10 | `agent-stages/02-design-reconciliation.md` | Stops for G10 approval | S00 |
| S20 | `agent-stages/03-repository-bootstrap.md` | Automatic | G10 approved |
| S30 | `agent-stages/04-contracts-domain-format.md` | Automatic | S20 |
| S40 | `agent-stages/05-persistence-recovery.md` | Automatic | S30 |
| S50 | `agent-stages/06-design-system-shell.md` | Automatic, parallel-capable | S30, G10 |
| S60 | `agent-stages/07-editor-foundation.md` | Automatic unless runtime gate fails | S30, G10 |
| S70 | `agent-stages/08-history-git2.md` | Automatic, parallel-capable | S40 |
| S80 | `agent-stages/09-search-sqlite.md` | Automatic, parallel-capable | S40 |
| S90 | `agent-stages/10-foundation-integration.md` | Automatic | S40, S50, S60, S70, S80 |
| S100 | `agent-stages/11-feature-wave-orchestrator.md` | Automatically plans and dispatches slices | S90 |
| Slice | `agent-stages/12-feature-slice-agent.md` | Automatic per bounded slice | Generated task |
| S110 | `agent-stages/13-system-integration-validation.md` | Automatic repair loop within scope | All v1 slices |
| S120 | `agent-stages/14-release-hardening.md` | Automatic; may request signing credentials | S110 |
| S130 | `agent-stages/15-release-validation.md` | Produces G90 package, then stops | S120 |
| Change | `agent-stages/16-design-change-reconciliation.md` | Stops only when a new design needs approval | New Penpot handoff |

The stage numbers describe dependencies, not necessarily wall-clock order. After S30, independent stages may use parallel worktrees where their ownership maps do not overlap.

## 7. Repository-backed handoff contract

Every dispatched stage receives:

- A baseline commit SHA.
- Its stage instruction file.
- `agent-workflow/pipeline-state.yaml`.
- Approved gate files.
- Handoff files from all declared dependencies.
- The relevant PRD, architecture, design, and implementation documents.

Every stage must create:

```text
agent-workflow/runs/<stage-id>/<run-id>/
├── dispatch.yaml
├── status.yaml
├── handoff.yaml
├── report.md
└── evidence/
```

The stage branch must also contain all production code, tests, ADRs, traceability updates, and design-reference changes produced by the stage.

### `status.yaml`

Records the result, baseline and output commits, files changed, commands run, platforms tested, requirements addressed, blocking issues, deviations, dependencies, and recommended next stages.

Allowed results:

- `passed`
- `failed`
- `blocked`
- `needs_approval`

### `handoff.yaml`

Records stable outputs for later agents:

- Contract/schema versions.
- New ports and implementations.
- Generated bindings.
- Fixture and test locations.
- Runtime or build commands.
- Known limitations that remain within approved scope.
- Exact artifact and evidence paths.

### `report.md`

Provides the human-readable summary and rationale. Later agents may read it, but machine progression must rely on `status.yaml`, `handoff.yaml`, approved gates, tests, and repository state.

## 8. Automatic verification and integration

A stage is not automatically accepted merely because its agent reports success. The Orchestrator Agent must:

1. Confirm the stage used the dispatched baseline.
2. Confirm the branch is clean and contains the required run artifacts.
3. Check that the diff stays within the stage ownership scope.
4. Check that no governing document or approved design handoff was modified.
5. Re-run the stage’s mandatory gate commands or dispatch an independent verifier.
6. Confirm required Windows/macOS/Linux evidence where the stage demands it.
7. Confirm no unapproved product, architecture, canonical-format, licensing, or security deviation is present.
8. Merge through an integration branch and run the post-merge gate.
9. Update `pipeline-state.yaml` and accepted handoff pointers.
10. Dispatch newly unblocked stages.

The Orchestrator Agent may automatically dispatch one bounded repair attempt for an implementation or test defect that does not change requirements or architecture. A repeated failure, broad fork, data-loss risk, or required product change must stop at G20.

## 9. Material-deviation gate G20

The Orchestrator Agent must stop and create a proposal under:

```text
agent-workflow/proposals/<proposal-id>/
├── proposal.md
├── options.yaml
├── evidence/
└── approval.yaml
```

G20 is required when work would:

- Change a PRD requirement or user-visible behavior.
- Conflict materially with the approved Penpot handoff.
- Change Tauri, ProseMirror, `git2`, SQLite FTS5, or another selected architectural component.
- Change a public architectural boundary or authoritative state owner.
- Change canonical formats or migration guarantees beyond the approved architecture.
- Add a material distribution, licensing, security, or privacy exception.
- Weaken save/recovery, cross-platform, accessibility, performance, or 250,000-word requirements.
- Require a broad maintained fork.

The proposal must present evidence and bounded options. The agent must not select an option for the product owner.

## 10. Feature-slice automation

After S90, the Feature-Wave Orchestrator reads the approved reconciliation work breakdown, implementation plan, traceability matrix, and current code. It generates one task file per slice under:

```text
agent-workflow/generated-tasks/<wave-id>/
```

Each task identifies:

- Requirements and Penpot component/screen IDs.
- Dependencies and file ownership.
- Domain, application, adapter, frontend, persistence, history, search, and test work required.
- Commands and native platforms required for completion.
- Stop conditions.

Independent slices may run in parallel. The orchestrator must not dispatch two agents that own the same files or public contract. Each slice uses `agent-stages/12-feature-slice-agent.md` plus its generated task file.

## 11. Continuous validation

Validation is not deferred until the end.

- Stage agents add tests with their work.
- The Orchestrator Agent runs stage gates before integration.
- S110 runs the complete requirement, visual, accessibility, performance, recovery, history, search, and cross-platform suites.
- S110 may automatically dispatch bounded fix tasks for defects when the fix does not require G20.
- S130 is performed by an independent validation agent and produces the release-evidence package.

A validation agent must not alter the PRD, architecture, approved design, or acceptance criteria to make a build pass.

## 12. Release approval G90

The release-validation stage creates:

```text
release-evidence/<candidate-version>/
├── requirement-disposition.csv
├── platform-matrix.yaml
├── performance/
├── accessibility/
├── visual/
├── recovery/
├── history-search/
├── packaging/
├── security-licenses-sbom/
├── known-issues.yaml
└── release-approval.yaml
```

`release-approval.yaml` is created with `status: pending`. The Orchestrator Agent stops. The product owner reviews the evidence and either approves the release, records explicit waivers, or requests fixes.

## 13. Design revision after implementation starts

When Penpot changes:

1. Export a new immutable handoff version.
2. Point an agent to `agent-stages/16-design-change-reconciliation.md`.
3. The agent compares old/new manifests, tokens, components, screens, snapshots, and requirements.
4. It produces a design-diff report, impact map, generated implementation tasks, and a pending approval file.
5. The Orchestrator Agent pauses only affected workstreams.
6. After approval, it dispatches the generated tasks through the normal pipeline.

Do not continuously sync a mutable live Penpot file into production.

## 14. Agent quality controls

- Use small, reviewable tasks and isolated branches/worktrees.
- Require tests, raw evidence, and exact commands.
- Separate prototypes from production code.
- Prevent agents from changing both a public contract and all implementations without explicit gate verification.
- Keep PRD/design/architecture conflicts visible.
- Preserve exact dependency locks.
- Require clean working trees and reproducible commands at handoff.
- Stop rather than fabricate native runtime, accessibility, packaging, or performance evidence.
- Do not rely on agent conversation memory for a later stage.

## 15. Current Codex MCP references

Codex supports user-level or project-scoped MCP configuration and shares MCP configuration across supported local Codex clients. Use current official instructions:

- <https://developers.openai.com/codex/mcp>
- <https://developers.openai.com/codex/config-basic>

A project-scoped `.codex/config.toml` is preferred when the Penpot server configuration should travel with a trusted project, provided no secrets are committed. Verify active tools with the Codex MCP list before beginning design work.
