# ParchMint Stage Agent Routing

**Status:** Current v1 subagent-selection policy
**Version:** 1.0
**Date:** 2026-08-04

## 1. Authority

This file is the source of truth for selecting implementation, independent-test,
and validation subagents during the v1 delivery pipeline. It also defines the
worktree, pull-request, and validation sequence used around those agents.

`delivery/implementation-plan.md` owns the stage graph and independent-test
applicability. Stage instructions own scope and gates. `delivery/agent-playbook.md`
owns pipeline mechanics. This file owns only agent selection and the additional
per-stage validation pass. `AGENTS.md` and governing product/architecture/design
documents remain higher authority.

Always start with the cheapest reliable tier. A worker that discovers broader
scope or risk stops, preserves evidence, and recommends escalation. The
Orchestrator owns scope, consequential decisions, Git integration, approvals,
and final synthesis.

## 2. Role separation

Each production stage or feature slice uses three distinct responsibilities:

1. **Implementation:** writes production code and developer tests within the
   dispatched ownership.
2. **Independent test:** seals a requirements-first charter before candidate
   access, then writes only independently owned tests, fixtures, and run
   artifacts.
3. **Validation:** reviews the combined candidate and evidence after applicable
   CI. It does not implement fixes, edit tests, or change criteria.

One agent identity must not hold more than one of these responsibilities for the
same candidate. S130 is itself the final independent validation stage and uses a
fresh agent that did not implement the release candidate.

Read-only validation analysts return a structured result to the Orchestrator.
The Orchestrator commits that result, including agent task, model tier, inputs,
candidate commit, PR, and evidence reviewed, into the validation run evidence.

## 3. Fixed routing table

| Work | Implementation | Independent test | Validation |
|---|---|---|---|
| S00 repository intake | `patch_worker` | Exempt: non-production intake | `fast_worker`, validation-only |
| S10 design reconciliation | `feature_worker` | Exempt: non-production reconciliation | `analyst` with Terra; use the `ui-review` procedure |
| S20 repository bootstrap | `complex_worker` | `complex_worker` | `analyst` with Sol |
| S30 contracts/domain/format | `complex_worker` | `complex_worker` | `analyst` with Sol |
| S40 persistence/recovery | `complex_worker` | `complex_worker` | `analyst` with Sol |
| S50 design system/shell | `feature_worker` | `feature_worker` | `analyst` with Terra; use the `ui-review` procedure |
| S55 editor feasibility | `complex_worker` | `complex_worker` | `analyst` with Sol |
| S60 editor foundation | `complex_worker` | `complex_worker` | `analyst` with Sol |
| S65 spellcheck foundation | `complex_worker` | `complex_worker` | `analyst` with Sol |
| S70 history/git2 | `complex_worker` | `complex_worker` | `analyst` with Sol |
| S80 search/SQLite | `feature_worker` | `feature_worker` | `analyst` with Terra |
| S90 foundation integration | `complex_worker` | `complex_worker` | `analyst` with Sol |
| S100 feature-wave planning | `feature_worker` | Exempt: planning/dispatch only | `analyst` with Terra |
| Generated feature slice | Classify using Section 4 | Classify independently using Section 4 | Classify using Section 5 |
| S110 system integration | `complex_worker`; use `fast_worker` for noisy suites | Exempt for validation-only work; functional repairs require a separately classified challenge | `analyst` with Sol |
| S120 release hardening | `complex_worker` | `complex_worker` when shipped behavior changes; exempt for evidence-only reruns | `analyst` with Sol |
| S130 release validation | Fresh `complex_worker` acting only as Validation Agent | Exempt: this is the independent validation stage | The S130 worker; Orchestrator verifies package completeness |
| Design-change reconciliation | `feature_worker` | Exempt: reconciliation only | `analyst` with Terra; use the `ui-review` procedure |

The fixed choice may be escalated but not downgraded without an evidence-backed
playbook update. The implementation, independent-test, and validation agents are
always fresh relative to the responsibility they are checking.

## 4. Generated slices and repairs

Classify implementation and independent-test work separately:

- `fast_worker`: validation commands, source builds, large test suites, or an
  obvious mechanical repair with no product/public-boundary judgment.
- `patch_worker`: bounded low-to-moderate-risk change in one subsystem.
- `feature_worker`: ordinary multi-file feature, UI slice, adapter work, or
  refactor with established contracts.
- `complex_worker`: architecture, concurrency, security, persistent/canonical
  data, migration, editor state, recovery, cross-cutting integration, or another
  high-risk change.

An independent-test agent is selected from the behavior and evidence risk, not
automatically copied from the implementation tier. It must be write-capable and
must not receive the candidate or implementation reasoning before charter
sealing.

## 5. Validation selection

Use the cheapest validator that can make the required judgment:

- `fast_worker`, constrained to validation-only: deterministic schema,
  checksum, formatter, compiler, build, or test-matrix confirmation.
- `analyst` with Terra: ordinary correctness/integration review, broad mapping,
  derived-state adapters, and multi-screen or nuanced UI/design validation.
- `analyst` with Sol: public-boundary, security, persistence, recovery,
  migration, editor/shared-state, native-gate, cross-cutting, or release-critical
  judgment.
- Fresh `complex_worker`: only when validation must write a substantial owned
  artifact, such as the S130 release-evidence package.

UI validation uses visible rendered evidence for every affected approved state,
Light and Dark where applicable. It cites screens/components/locations,
distinguishes observation from inference, and checks accessibility and behavior
as well as visual similarity. Headless structure does not prove visual or native
behavior.

Every validator reports:

- Candidate and independent-test commits reviewed.
- PR and CI runs reviewed.
- Requirements, design IDs, contracts, and state owners checked.
- Commands/platforms/evidence independently verified.
- Pass, fail, blocked, or needs-approval result.
- Deviations, evidence limits, and exact next action.

## 6. Worktrees and Git ownership

The Orchestrator alone may create/remove worktrees, create integration branches,
merge/rebase/cherry-pick, push/fetch, open/update/merge PRs, or manipulate
`main`.

A dispatched write agent may run `git status`, `git diff`, `git add` for owned
paths, and `git commit` only inside its assigned isolated worktree and branch. It
must not switch branches, reset, merge, rebase, push, create/remove worktrees, or
touch another agent's branch. Validation analysts perform no Git writes.

Parallel stages require all of the following proof in their dispatches:

1. The implementation graph permits concurrency.
2. Included file paths are disjoint.
3. Public contracts/schemas and generated outputs do not overlap.
4. Shared manifests, locks, CI, and root configuration have exactly one owner.
5. Independent-test ownership is separate from production ownership.
6. A deterministic merge order and post-combination test are declared.

If any condition is unproven, run the stages sequentially. The first candidates
for proven parallelism are S40/S50/S55 after S30 and S70/S80 after S40.

## 7. Pull-request and CI sequence

1. The Orchestrator combines the implementation and applicable independent-test
   commits on a temporary integration branch.
2. The Orchestrator pushes that branch and opens a draft PR to protected `main`.
3. GitHub-hosted Windows, macOS, and Linux jobs run the declared Tier A/B/C
   checks.
4. A fresh validation agent reviews the integrated diff, run artifacts, CI
   results, native evidence, and design evidence where applicable.
5. The Orchestrator records the validation result in committed run evidence.
6. The PR may leave draft state and merge only after required checks and
   validation pass. No stage is accepted from the implementation commit alone.
7. Stop at G10, G20, G90, or another declared external-input/stop condition.

Evidence labels remain exact: `headless`, `development webview`, `packaged
executable`, `installed package`, or `native interactive`. GitHub-hosted CI may
run Tier B automation, but it must not be presented as native-interactive IME,
screen-reader, clipboard, accessibility, or comparable evidence unless that
interaction actually occurred. If a mandatory claim cannot be produced in CI,
stop and request the required native input.

## 8. Failure and deviation handling

- Preserve failing independent tests and validation evidence.
- Dispatch one bounded repair at the cheapest reliable tier when governing
  behavior and public boundaries remain valid.
- Do not let a repair agent edit an independently authored test.
- Re-run the independent challenge and validation against the repaired candidate.
- Stop at G20 before any material product, design, architecture, security,
  licensing, data-safety, accessibility, performance, or platform deviation.
- Never advance merely because CI is green when required evidence or validation
  remains incomplete.
