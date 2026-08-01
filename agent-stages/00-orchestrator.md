# Orchestrator Agent Instructions

**Stage role:** Pipeline controller  
**Version:** 1.2

## Mission

Own the repository-backed implementation pipeline from intake through release evidence. Dispatch bounded stage agents, verify their work independently, integrate passing branches, and stop only at an approval gate, a defined stop condition, or an unresolved external credential/input requirement.

Do not implement the whole application in one context. Do not use chat memory as a handoff mechanism.

## Required inputs

Read, in order:

1. `AGENTS.md`
2. `README.md`
3. `docs/09-agent-playbook.md`
4. `docs/product/01-product-specification.md`
5. `docs/architecture/02-final-architecture.md`
6. `docs/implementation/05-implementation-plan.md`
7. `docs/implementation/06-acceptance-and-release-plan.md`
8. The approved `design/handoff/<version>/design-manifest.yaml`
9. All files in `agent-stages/`

## Initialize pipeline state

If `agent-workflow/pipeline-state.yaml` does not exist:

1. Copy the templates under `templates/agent-workflow/` into `agent-workflow/`.
2. Record the repository baseline commit, build-kit version, product-spec version, architecture version, approved design-handoff version, and design-manifest checksum.
3. Create `agent-workflow/runs/`, `agent-workflow/gates/`, `agent-workflow/proposals/`, `agent-workflow/generated-tasks/`, and `agent-workflow/accepted-handoffs/`.
4. Commit pipeline initialization separately from implementation work.

Never overwrite a prior run. Use UTC run IDs of the form `YYYYMMDDTHHMMSSZ-<short-id>`.

## Stage graph

Execute this dependency graph:

```text
S00 repository intake
  └─ S10 design reconciliation
       └─ G10 product-owner approval
            └─ S20 repository bootstrap
                 └─ S30 contracts/domain/format
                      ├─ S40 persistence/recovery
                      ├─ S50 design-system/shell
                      └─ S60 editor foundation
                           
S40 ─┬─ S70 history
     └─ S80 search

S40 + S50 + S60 + S70 + S80
  └─ S90 foundation integration
       └─ S100 feature-wave planning/dispatch
            └─ generated feature slices
                 └─ S110 system integration/validation
                      └─ S120 release hardening
                           └─ S130 independent release validation
                                └─ G90 product-owner release approval
```

S50 and S60 may run in parallel with S40 after S30. S70 and S80 may run in parallel after S40. Generated feature slices may run in parallel only when file ownership and contract dependencies do not overlap.

## Dispatch protocol

Before each stage:

1. Create an isolated branch/worktree named `agent/<stage-id>/<run-id>`.
2. Write `agent-workflow/runs/<stage-id>/<run-id>/dispatch.yaml` containing:
   - stage and run IDs;
   - baseline commit;
   - instruction-file path;
   - dependency handoffs;
   - approved gates;
   - file ownership/exclusions;
   - required commands/platforms;
   - deadline or resource constraints, if any.
3. Give the stage agent the stage instruction file, dispatch record, baseline commit, and dependency handoffs.
4. Require the agent to create `status.yaml`, `handoff.yaml`, `report.md`, and evidence before completion.

If subagents are unavailable, execute the stage yourself in the isolated branch while following its instruction file exactly.

## Automatic acceptance criteria

A stage may be integrated automatically only when:

- `status.yaml` says `passed`.
- Required run artifacts exist and parse.
- The branch starts from the dispatched baseline.
- The working tree is clean.
- The diff is inside declared ownership.
- Governing documents and approved handoff files were not modified.
- Mandatory tests and platform checks pass when independently rerun.
- No blocking issue, unapproved deviation, broad fork, or material new dependency exists.
- Required traceability, ADR, design, and evidence updates are present.
- Post-merge tests pass on an integration branch.

Record the accepted run in `agent-workflow/accepted-handoffs/<stage-id>.yaml`, update `pipeline-state.yaml`, and commit the state change.

## Repair policy

For a normal implementation/test defect within existing requirements:

1. Dispatch one bounded repair task against the same stage instruction and failure evidence.
2. Do not broaden scope or change a public contract merely to make a test pass.
3. If the repair passes, continue automatically.
4. If it fails again, or reveals a stop condition, mark the stage `blocked` and stop.

## Approval and stop policy

Stop at G10 after design reconciliation.

Stop at G20 and create a proposal when work would change product behavior, approved design, selected technology, canonical formats, authoritative state ownership, distribution/licensing/security policy, or a mandatory release requirement.

Stop for external input when signing/notarization credentials, paid infrastructure, legal approval, or a human-only native accessibility action cannot be provided by the environment. State exactly what input is needed and preserve completed automated evidence.

Stop at G90 with the independent release-evidence package.

Never choose a new GUI, editor, history backend, search backend, or reduced product behavior without approval.

## Integration policy

- Only the Orchestrator Agent merges stage branches.
- Prefer squash or regular merge according to repository policy, but preserve the stage run ID and source commit in the merge message.
- Resolve mechanical conflicts only when both stage contracts remain unchanged.
- If a conflict requires semantic interpretation, redispatch a bounded integration task or stop at G20.
- Re-run contract, canonical-format, frontend, and cross-platform smoke tests after each integration wave.

## Final completion report

At any stop or final completion, report:

- Current pipeline stage and accepted commit.
- Accepted stage runs and pending branches.
- Approval/input required.
- Blocking issue IDs.
- Exact artifact paths.
- Commands/platforms already verified.
- The precise resume instruction.
