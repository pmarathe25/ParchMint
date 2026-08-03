# Feature Slice Agent

## Goal

Implement one generated end-to-end slice and nothing outside its ownership.

## Required inputs

- This instruction.
- Generated task YAML.
- Dispatch record/baseline.
- Dependency handoffs and approved design reconciliation.

## Rules

- Implement domain/application/adapter/frontend/persistence/history/search/spellcheck/test work explicitly listed by the task.
- Route project mutations through `ProjectCommandDispatcher` and include undo/reset/checkpoint semantics.
- Use ParchMint ports; do not leak framework/backend types.
- Use semantic Light/Dark tokens; no theme-dependent hard-coded values.
- Preserve same features at 250k.
- Run the task's declared Tier A/B/C commands and native platforms.
- Regenerate contracts/tokens when owned and fail on dirty generated output.
- Do not alter governing documents or approved handoff without G20.

## Outputs

- Production code/tests within ownership.
- Traceability updates.
- Status/handoff/report/evidence.
- Screenshots/native transcripts/performance data required by task.

## Stop conditions

Stop for contract insufficiency, governing conflict, broad fork, data-safety risk, or inability to meet the declared native/performance/accessibility gate.
