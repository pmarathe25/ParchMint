# ParchMint v1 Build Kit

**Status:** Reconciled design-and-implementation baseline  
**Build-kit workflow version:** 1.2  
**Reconciled:** 2026-07-31  
**Repository baseline:** `pmarathe25/ParchMint@4fe7976acd526faf0faf4e5fa3834bfb3bc2f742`

This repository contains the governing documents and automated agent workflow for the first cross-platform release of **ParchMint**, a local-first novel-writing desktop application.

This workflow revision was reconciled against the product-owner changes already present on `main`. It intentionally preserves the repository versions of the product specification, final architecture, Penpot design brief, acceptance plan, future-work record, decisions/evidence record, and `templates/design-manifest.yaml`.

## Current governing versions

- Product specification: `docs/product/01-product-specification.md` — **2.0**
- Final architecture: `docs/architecture/02-final-architecture.md` — **1.2**
- Penpot design brief: `docs/design/03-penpot-design-brief.md` — **2.3**
- Design handoff contract: `docs/design/04-design-artifact-handoff-contract.md` — **1.1**
- Implementation plan: `docs/implementation/05-implementation-plan.md` — **1.2**
- Acceptance plan: `docs/implementation/06-acceptance-and-release-plan.md` — **1.0**
- Future work: `docs/future/07-future-work.md` — **1.2**
- Decisions and evidence: `docs/evidence/08-decisions-and-evidence.md` — **1.0**
- Automated agent playbook: `docs/09-agent-playbook.md` — **1.2**

## Final technology decisions

- Desktop shell: **Tauri 2.11.5**
- UI: **TypeScript + React**, rendered in the platform webview
- Rich-text editor: **exact-locked ProseMirror packages**, isolated behind a ParchMint editor adapter
- Core application services: **Rust**
- Canonical project data: **restricted deterministic HTML5 + TOML + CSS + JSON**
- History: **`git2 =0.21.0` with vendored libgit2**, hidden behind `HistoryStore`
- Search: **`rusqlite =0.40.1` with bundled SQLite FTS5**, hidden behind `SearchIndex`
- Initial export: **one self-contained HTML5 export of the entire Manuscript**
- First-class platforms: **Windows, macOS, and Linux from v1**

All supported documents, including documents near 250,000 words, retain the same product features. Internal optimizations may vary transparently, but agents may not introduce a user-visible large-document mode or reduce editing, comments, search, formatting, or dual-view behavior by size.

## Source-of-truth order

When materials conflict, use this precedence:

1. `docs/product/01-product-specification.md`
2. `docs/architecture/02-final-architecture.md`
3. The latest product-owner-approved Penpot handoff identified by `design/handoff/<version>/design-manifest.yaml`
4. `docs/design/03-penpot-design-brief.md` for visual language and approved presentation rules
5. `docs/design/04-design-artifact-handoff-contract.md`
6. `docs/implementation/05-implementation-plan.md`
7. `docs/implementation/06-acceptance-and-release-plan.md`
8. `docs/future/07-future-work.md`
9. Historical evidence and rejected alternatives

A design artifact may clarify layout and interaction but may not silently change product behavior. A coding agent must record a conflict and route it through G20 instead of choosing its own interpretation.

## Automated workflow

The design-agent and product-owner design-review stages are already complete. Begin with the approved handoff committed under:

```text
design/handoff/<version>/
```

Then start one lead coding agent with:

- `AGENTS.md`
- `docs/09-agent-playbook.md`
- `agent-stages/00-orchestrator.md`

The Orchestrator Agent initializes repository-backed pipeline state, dispatches design reconciliation, and stops at G10. After G10 approval, it automatically dispatches, verifies, and integrates routine stages. Product-owner review is required only for:

- **G10:** approved design-to-implementation reconciliation
- **G20:** material product, design, architecture, licensing, security, or mandatory-requirement deviation
- **G90:** final release approval

All stage communication is committed under `agent-workflow/`; later agents must not depend on earlier chat transcripts.

## Workflow files

| Path | Purpose |
|---|---|
| `AGENTS.md` | Repository-wide rules for all agents |
| `docs/09-agent-playbook.md` | How to start, approve, resume, and govern the automated pipeline |
| `agent-stages/00-orchestrator.md` | Lead-agent execution contract |
| `agent-stages/01-16*.md` | Bounded stage instructions |
| `templates/agent-workflow/` | Pipeline, dispatch, status, handoff, and gate templates |
| `templates/design-reconciliation/` | G10 reconciliation package templates |
| `templates/agent-task.yaml` | Generated feature-slice task template |
| `docs/evidence/build-kit-v1.2-reconciliation.md` | Exact preservation and merge record |

## Important residual risk

The selected Tauri/ProseMirror stack still requires early release-mode native validation on Windows, macOS, and Linux for IME, clipboard, accessibility, high DPI, memory, and the same-feature 250,000-word requirement. Failure must stop at G20 with evidence; an agent may not introduce a special large-document mode or choose a different frontend independently.
