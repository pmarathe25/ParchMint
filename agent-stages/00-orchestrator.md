# Orchestrator Agent Instructions

**Stage role:** Pipeline controller  
**Version:** 1.3

## Mission

Own the repository-backed implementation pipeline from intake through release evidence. Dispatch bounded stage agents, verify independently, integrate passing work, and stop only at an approval gate, defined stop condition, or unresolved external credential/input requirement.

Do not implement the whole application in one context. Do not use chat memory as a handoff mechanism.

## Required inputs

Read, in order:

1. `AGENTS.md`
2. `README.md`
3. `docs/09-agent-playbook.md`
4. Product specification
5. Architecture
6. Implementation plan
7. Acceptance plan
8. Approved `design/handoff/<version>/design-manifest.yaml`
9. All files in `agent-stages/`

## Initialize pipeline state

If `agent-workflow/pipeline-state.yaml` does not exist:

1. Copy templates under `templates/agent-workflow/`.
2. Record repository baseline, governing versions, approved handoff version/checksum, and selected test-host availability.
3. Create run/gate/proposal/generated-task/accepted-handoff directories.
4. Commit initialization separately.

Never overwrite a prior active run. Use UTC run IDs `YYYYMMDDTHHMMSSZ-<short-id>`.

## Stage graph

```text
S00 → S10 → G10 → S20 → S30
                       ├─ S40 → S70
                       │      └→ S80
                       ├─ S50
                       └─ S55 → S60 → S65

S40 + S50 + S60 + S65 + S70 + S80
  → S90 → S100 → slices → S110 → S120 → S130 → G90
```

S40/S50/S55 may run in parallel after S30. S70/S80 follow S40. S60 follows S55. S65 follows S60. S90 waits for every listed foundation.

## Dispatch protocol

Before each stage:

1. Create isolated branch/worktree `agent/<stage-id>/<run-id>`.
2. Write `dispatch.yaml` with stage/run, baseline, instruction, dependency handoffs, approved gates, ownership/exclusions, required commands/platforms/test tier, and resource constraints.
3. Give the agent the instruction, dispatch, baseline, and handoffs.
4. Require status/handoff/report/evidence.

If subagents are unavailable, execute the stage in an isolated worktree while following its instruction exactly.

## Automatic acceptance

Integrate only when:

- Status is `passed`.
- Run artifacts exist/parse.
- Branch starts from dispatched baseline and is clean.
- Diff is within ownership.
- Governing documents and approved handoff were not modified without G20.
- Required Tier A/B/C commands and native evidence pass.
- Generated contract/token guards are clean where applicable.
- No blocking issue, broad fork, unapproved deviation, or material new dependency exists.
- Post-merge integration tests pass.

Record accepted handoff and update pipeline state in a separate commit.

## Repair policy

For a normal defect within existing requirements:

1. Dispatch one bounded repair against the same instruction and evidence.
2. Do not broaden scope or change a public contract merely to pass.
3. Continue if it passes.
4. Mark blocked and stop on repeated failure or a stop condition.

## Approval and stop policy

- Stop at G10 after reconciliation.
- Stop at G20 before governing changes, broad forks, or weakened mandatory gates.
- Stop for external signing/notarization/legal/paid infrastructure/human-only accessibility input when unavailable.
- Stop at G90 with independent release evidence.
- Never choose a new GUI/editor/history/search/spellcheck backend or reduced behavior without approval.

S55 and S65 are selection gates within current architecture. They may choose among explicitly allowed implementation strategies only when their evidence passes; a choice requiring a framework/backend/public-boundary change stops at G20.

After a G20 approval, update current governing documents directly. Do not add an ADR/changelog/historical decision record.

## Final report

At any stop report current stage/accepted commit, accepted runs/pending branches, approval/input required, blockers, artifact paths, verified commands/platforms, and precise resume instruction.
