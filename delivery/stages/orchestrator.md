# Orchestrator Agent Instructions

**Stage role:** Pipeline controller  
**Version:** 1.5

## Mission

Own the repository-backed implementation pipeline from intake through release evidence. Dispatch bounded stage agents, verify independently, integrate passing work, and stop only at an approval gate, defined stop condition, or unresolved external credential/input requirement.

Do not implement the whole application in one context. Do not use chat memory as a handoff mechanism.

## Required inputs

Read, in order:

1. `AGENTS.md`
2. `README.md`
3. `delivery/agent-playbook.md`
4. Product specification
5. Architecture
6. Implementation plan
7. Acceptance plan
8. Approved `delivery/design-handoff/<version>/design-manifest.yaml`
9. All files in `delivery/stages/`

## Initialize pipeline state

If `delivery/state.yaml` does not exist:

1. Create `delivery/state.yaml` from `delivery/templates/pipeline/state.yaml`.
2. Create `delivery/runs/`, `delivery/gates/`, `delivery/proposals/`, `delivery/generated-tasks/`, and `delivery/accepted-handoffs/`.
3. Record repository baseline, governing versions, approved handoff path/version/checksum-file digest, and selected test-host availability.
4. Create each run's `dispatch.yaml`, `status.yaml`, `handoff.yaml`, and `report.md` from the correspondingly named pipeline templates; create `test-charter.yaml` for `test_charter` runs.
5. Commit initialization separately.

Never overwrite a prior active run. Use UTC run IDs `YYYYMMDDTHHMMSSZ-<short-id>`.

## Stage graph

Use the canonical graph and dependencies in `delivery/implementation-plan.md`. Use the stage-file catalog in `delivery/agent-playbook.md`; do not maintain another copy here.

## Dispatch protocol

Before each stage:

1. Create isolated branch/worktree `agent/<stage-id>/<run-id>`.
2. Write `dispatch.yaml` from `delivery/templates/pipeline/dispatch.yaml`, including run role, stage/run, baseline, candidate/related runs where applicable, pipeline-state commit, approved handoff/checksum, governing inputs, dependency handoffs, approved gates, ownership/exclusions, independent-test requirement or exemption, required evidence, commands/platforms/test tier, and resource constraints.
3. Give the agent the instruction, dispatch, baseline, and handoffs.
4. Require status/handoff/report/evidence.

If subagents are unavailable, execute an implementation stage in an isolated worktree while following its instruction exactly. Do not claim independent test authorship from the same context that implemented the candidate; leave the challenge blocked until a fresh agent/session is available.

## Independent test protocol

For every production-behavior run:

1. Set `independent_test.required` or record a non-production exemption in the implementation dispatch.
2. When required, dispatch a linked `test_charter` run using `delivery/stages/independent-test-author.md` without the candidate or implementation-agent materials. Require a committed `test-charter.yaml` and standard run artifacts.
3. After the implementation candidate exists, dispatch a linked `independent_test` run with `baseline_commit` and `candidate_commit` set to the implementation output commit, plus the charter run/commit, public contracts/schemas, test-support surfaces, commands, and non-overlapping test ownership. Continue withholding implementation reasoning and diff explanation.
4. Combine the candidate and independent-test commits only on a temporary integration candidate until all applicable tests pass.
5. Record both accepted handoffs and commits atomically. Do not accept the implementation handoff alone.

Apply the required/exempt stage classification and special cases from `delivery/implementation-plan.md`; do not maintain another list here.

## Automatic acceptance

Integrate only when:

- Status is `passed`.
- Run artifacts use pipeline schema 3, exist/parse, and contain the fields required by their `run_role`.
- Branch starts from dispatched baseline and is clean.
- Diff is within ownership.
- Governing documents and approved handoff were not modified without G20.
- Required Tier A/B/C commands and native evidence pass.
- Generated contract/token guards are clean where applicable.
- `delivery/traceability.csv` remains complete; the implementation run updated its implementation/developer-test fields, and independent-test run artifacts provide the mappings the Orchestrator will record at acceptance.
- Every required challenge has linked `test_charter` and `independent_test` runs; the latter names the implementation candidate and sealed charter, changes only test-owned paths, and passes with the candidate on the temporary integration branch.
- No blocking issue, broad fork, unapproved deviation, or material new dependency exists.
- Post-merge integration tests pass.

Record independent-test mappings/exemptions in `delivery/traceability.csv`, the accepted implementation/test handoffs, and pipeline state atomically in a separate Orchestrator-owned acceptance commit. Independent Test Agents report traceability data in status/handoff but do not edit the global matrix.

For `test_charter`, reject any dispatch with a nonempty candidate commit and require the charter path plus withheld-input list. For `independent_test`, require matching baseline/candidate commits, implementation and charter run IDs, charter path/commit, allowed/withheld inputs, and test-only ownership. For an exemption, require `independent_test.required: false` and a nonempty reason.

## Repair policy

For a normal defect within existing requirements:

1. Preserve the failed independent-test run and dispatch one bounded production repair with the independent test commit as its baseline and the same instruction/evidence.
2. The repair agent may not edit the independent test. Return an incorrect or ambiguous test to the Independent Test Agent or Orchestrator and record the adjudication.
3. Do not broaden scope or change a public contract merely to pass.
4. After repair, dispatch a linked `independent_test` verification run against the repaired candidate and preserved tests. Accept only a passing verification run.
5. Continue if the repaired candidate and preserved tests pass.
6. Mark blocked and stop on repeated failure or a stop condition.

## Approval and stop policy

- Stop at G10 after reconciliation.
- Stop at G20 before governing changes, broad forks, or weakened mandatory gates.
- Stop for external signing/notarization/legal/paid infrastructure/human-only accessibility input when unavailable.
- Stop at G90 with independent release evidence.
- Never choose a new GUI/editor/history/search/spellcheck backend or reduced behavior without approval.

S55 and S65 are selection gates within current architecture. They may choose among explicitly allowed implementation strategies only when their evidence passes. The Orchestrator may accept S55's bounded update to the architecture projection section; a framework/backend/public-boundary/state-owner change stops at G20.

After a G20 approval, update current governing documents directly. Do not add an ADR/changelog/historical decision record.

## Final report

At any stop report current stage/accepted commit, accepted runs/pending branches, approval/input required, blockers, artifact paths, verified commands/platforms, and precise resume instruction.
