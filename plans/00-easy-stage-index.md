# ParchMint staged implementation index

This directory splits the master [implementation plan](../PLAN.md) into bounded stages that can be handed to different coding agents. The stages are ordered, not fully parallel: each agent should be able to begin from the repository and the prior stage's written handoff without needing conversational context from the previous agent.

## Filename convention

```text
NN[-luna]-DIFFICULTY-short-name.md
```

- `NN` is the required execution order.
- `very-hard`, `hard`, `medium`, or `easy` is the expected reasoning difficulty.
- `luna` marks a high-volume, low-judgment stage suitable for GPT-5.6 Luna.
- Files without `luna` should default to GPT-5.6 Terra or Sol as listed below.

## Model guidance

| Difficulty | Recommended model | Suggested reasoning | Use |
|---|---|---:|---|
| `luna-easy` | GPT-5.6 Luna | medium/high | Repetitive fixtures, docs, matrix expansion, mechanical test coverage |
| `medium` | GPT-5.6 Terra | medium/high | Bounded product features with established architecture |
| `hard` | GPT-5.6 Terra | high/xhigh | Cross-module work, format logic, export logic, concurrency |
| `very-hard` | GPT-5.6 Sol | high/max | Architecture, editor correctness, recovery, platform integration |

Terra at maximum reasoning is a cost-conscious fallback for a very-hard stage, but use Sol if the agent repeatedly revisits architecture, loses format fidelity, or cannot close cross-platform failures. Luna must not make persisted-format, threading, data-recovery, security, or architecture decisions.

## Ordered stages

| Stage | Plan | Difficulty | Primary outcome | Depends on |
|---:|---|---|---|---|
| 1 | [Foundation and risk spikes](01-very-hard-foundation-and-risk-spikes.md) | Very hard | Proven Qt/Rust/editor architecture and production scaffold | None |
| 2 | [Project domain and storage](02-hard-project-domain-and-storage.md) | Hard | Open project format, domain invariants, atomic persistence | 1 |
| 3 | [Editor document lifecycle](03-very-hard-editor-document-lifecycle.md) | Very hard | Lossless WYSIWYG editing, styles, autosave, recovery | 1–2 |
| 4 | [Binder, outline, and cards](04-medium-binder-outline-and-cards.md) | Medium | Complete manuscript organization and summary-first workflow | 2–3 |
| 5 | [Research and split workspace](05-medium-research-and-split-workspace.md) | Medium | Research material, attachments, and two-pane reference workflow | 4 |
| 6 | [Search, index, and statistics](06-medium-search-index-and-statistics.md) | Medium | Incremental full-text search and counts | 2, 4–5 |
| 7 | [Compile and export](07-hard-compile-and-export.md) | Hard | Compile presets and all version 1 output formats | 3–6 |
| 8 | [Fixtures, documentation, and regression matrix](08-luna-easy-fixtures-documentation-and-regression-matrix.md) | Luna/easy | High-volume coverage and user/developer documentation | 1–7 |
| 9 | [Cross-platform hardening and release](09-very-hard-cross-platform-hardening-and-release.md) | Very hard | Accessible, performant, packaged version 1 release | 1–8 |

Stages 1–3 deliberately cluster the hardest foundational changes at the beginning. Stage 9 deliberately collects the hardest integration, operating-system, performance, and release work at the end. This prevents medium-difficulty product agents from building against an unproven editor or changing platform architecture opportunistically.

## Agent operating contract

Every stage agent must:

1. Read `PLAN.md`, this index, its stage plan, all accepted ADRs, and the previous handoff.
2. Treat earlier stage acceptance gates and public interfaces as stable. Propose an ADR before changing them.
3. Inspect the worktree before editing and preserve unrelated user changes.
4. Work through vertical slices with tests; do not leave all testing until the end.
5. Keep canonical data in Markdown/TOML and never make SQLite or Qt serialization authoritative.
6. Run the stage's required commands and record exact outcomes.
7. Create `docs/handoffs/NN-<stage-name>.md` before declaring the stage complete.

Each handoff must contain:

- Commit or working-tree state used for verification
- Delivered behavior and deliberately deferred behavior
- ADRs added or superseded
- Persisted-format or public-interface changes
- Commands run and their results on each available platform
- Performance measurements relevant to the stage
- Known defects, risks, and platform gaps
- Exact prerequisites and recommended first task for the next agent

An agent may not declare a stage complete if its acceptance gate is unmet. If the repository cannot satisfy a gate because a required OS, signer, certificate, or external service is unavailable, the handoff must distinguish automated evidence from the remaining manual verification; only stage 9 may carry such release-environment items.

## Cross-stage ownership

The stage plans identify primary paths. They are ownership boundaries, not absolute prohibitions: integration sometimes requires a narrow edit elsewhere. An agent making a cross-boundary edit must preserve existing APIs when possible, add regression coverage, and document the reason in its handoff.

Tests required to prove a feature belong with the feature stage. Stage 8 expands breadth and documentation; it is not a substitute for tests omitted by stages 1–7.

