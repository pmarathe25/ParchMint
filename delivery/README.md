# ParchMint v1 Delivery Kit

**Lifecycle:** Temporary implementation-phase material

This directory exists to turn the maintained product, architecture, and design documents into the first working release. It is authoritative while the v1 delivery pipeline is active, but it is not the long-term home for product behavior, architecture, visual rules, source code, tests, or operational knowledge.

## Maintained versus temporary

| Maintained outside this directory | Temporary inside this directory |
|---|---|
| Product requirements and deferred scope | Current implementation and acceptance plans |
| Architecture, state ownership, canonical formats, and public boundaries | Penpot implementation handoff and reconciliation packages |
| Current visual and interaction rules | Stage instructions, templates, dispatches, reports, and approvals |
| Production contracts, schemas, source code, tests, and CI once created | Pipeline state, generated tasks, traceability work matrix, and release-candidate evidence |

## Checked-in source files

- `design-handoff-contract.md` — defines the one approved Penpot package required to start implementation.
- `implementation-plan.md` — current v1 stage graph and implementation sequence.
- `acceptance-and-release-plan.md` — temporary delivery gates that must become executable tests, CI checks, or maintained release procedures.
- `agent-playbook.md` — orchestrator, independent-test, and approval workflow.
- `stage-agent-routing.md` — authoritative per-stage implementation, independent-test, validation, worktree, and PR routing.
- `stages/` — bounded instructions for implementation and independent test agents.
- `templates/` — blueprints for handoff, reconciliation, pipeline, and release artifacts.

## Generated during delivery

The pipeline creates `design-handoff/`, `design-reconciliation/`, `state.yaml`, `traceability.csv`, `runs/`, `gates/`, `proposals/`, `generated-tasks/`, `accepted-handoffs/`, and `release-evidence/` here. Runs include sealed independent-test charters where required. These artifacts support current execution and evidence; they do not become maintained product documentation merely because they are committed.

## Promotion before retirement

Before deleting or replacing this delivery kit:

1. Move lasting behavior and acceptance outcomes into `docs/product/product-specification.md`.
2. Move lasting boundaries, state ownership, formats, security rules, and selected technology into `docs/architecture/architecture.md`.
3. Move lasting visual and interaction rules into `docs/design/penpot-design-brief.md`.
4. Put enforceable contracts, schemas, migrations, tests, CI gates, and production tooling beside the implementation they govern.
5. Promote the approved Penpot source and reusable design assets into a maintained design-source location chosen during implementation.
6. Promote recurring developer/release operations into maintained documentation only after the real commands and ownership exist.
7. Retain release evidence outside the maintained docs set only when distribution, legal, security, or support policy requires it.

The product owner confirms this extraction after G90. Then remove superseded stage plans, templates, run artifacts, handoffs, reconciliation packages, temporary acceptance prose, and resolved proposals rather than keeping a permanent implementation diary.
