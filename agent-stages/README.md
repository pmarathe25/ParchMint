# ParchMint Agent Stage Instructions

Use `00-orchestrator.md` for the lead agent. The lead agent dispatches the remaining files according to `docs/09-agent-playbook.md` and repository-backed pipeline state.

Do not ask a stage agent to infer its task from a prior agent’s chat. Give it:

- its stage instruction file;
- the generated `dispatch.yaml`;
- the baseline commit;
- dependency `handoff.yaml` files;
- approved gate files;
- repository access.

| File | Purpose |
|---|---|
| `00-orchestrator.md` | Pipeline controller and automatic dispatch/integration rules |
| `01-repository-intake.md` | Baseline and handoff validation |
| `02-design-reconciliation.md` | Versioned implementation interpretation; stops at G10 |
| `03-repository-bootstrap.md` | Monorepo, locks, shell, CI, governance |
| `04-contracts-domain-format.md` | Durable contracts, domain, canonical formats |
| `05-persistence-recovery.md` | Atomic save, project repository, recovery |
| `06-design-system-shell.md` | Approved tokens/assets and application shell |
| `07-editor-foundation.md` | ProseMirror adapter and early native gate |
| `08-history-git2.md` | Git-backed HistoryStore |
| `09-search-sqlite.md` | SQLite FTS5 SearchIndex |
| `10-foundation-integration.md` | Integrate foundations and freeze slice extension points |
| `11-feature-wave-orchestrator.md` | Generate and dispatch feature-slice tasks |
| `12-feature-slice-agent.md` | Generic instruction for one vertical slice |
| `13-system-integration-validation.md` | Full integrated validation and bounded repair loop |
| `14-release-hardening.md` | Packaging, signing workflow, platform hardening |
| `15-release-validation.md` | Independent release evidence; stops at G90 |
| `16-design-change-reconciliation.md` | Versioned Penpot changes after implementation begins |
