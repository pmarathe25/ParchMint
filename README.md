# ParchMint v1 Build Kit

**Status:** Final planning set for design and implementation  
**Version:** 1.0  
**Date:** 2026-07-28

This directory is the governing document set for designing and implementing the first cross-platform release of **ParchMint**, a local-first novel-writing desktop application.

The prior architecture explorations are retired. Their accepted conclusions are summarized in `08-decisions-and-evidence.md`; the raw reports are retained under `evidence/` for traceability. The product owner has chosen to proceed with Tauri and ProseMirror despite incomplete native-runtime validation, and has explicitly rejected any user-visible special mode or reduced feature set for large documents in v1.

## Final technology decisions

- Desktop shell: **Tauri 2.11.5**
- UI: **TypeScript + React**, rendered in the platform webview
- Rich-text editor: **exact-locked ProseMirror packages**, isolated behind a ParchMint editor adapter
- Core application services: **Rust**
- Canonical project data: **restricted deterministic HTML5 + TOML + CSS + JSON**
- History: **`git2 =0.21.0` with vendored libgit2**, hidden behind `HistoryStore`
- Search: **`rusqlite =0.40.1` with bundled SQLite FTS5**, hidden behind `SearchIndex`
- Initial export: **single self-contained HTML5 manuscript**
- First-class platforms from the first release: **Windows, macOS, and Linux**

All documents, including documents up to approximately 250,000 words, must expose the same editing features. Internal optimizations may vary transparently, but ParchMint must not silently disable two-view editing, comments, formatting, search, or other v1 functionality based on document size.

## Source-of-truth order

When documents or artifacts conflict, use this precedence:

1. `01-product-specification.md`
2. `02-final-architecture.md`
3. The latest product-owner-approved Penpot handoff identified by `design-manifest.yaml`
4. `04-design-artifact-handoff-contract.md`
5. `05-implementation-plan.md`
6. `06-acceptance-and-release-plan.md`
7. `07-future-work.md`
8. Historical evidence and rejected alternatives

A design artifact may clarify layout and interaction, but it may not silently add, remove, or change a product requirement. A coding agent must record any conflict and request or propose a documented resolution.

## Documents

| File | Purpose |
|---|---|
| `AGENTS.md` | Repository-wide rules for coding and design agents |
| `01-product-specification.md` | Normative product requirements and acceptance behavior |
| `02-final-architecture.md` | Final v1 architecture, module boundaries, data ownership, and replaceable ports |
| `03-penpot-design-brief.md` | Copyable design brief for the Penpot/Codex design agent |
| `04-design-artifact-handoff-contract.md` | Required Penpot exports, naming, manifests, and design-to-code reconciliation |
| `05-implementation-plan.md` | Ordered implementation phases and safe parallel workstreams |
| `06-acceptance-and-release-plan.md` | Automated, visual, performance, accessibility, and cross-platform release gates |
| `07-future-work.md` | Deferred capabilities and extension paths |
| `08-decisions-and-evidence.md` | Final ADR summary and exploration evidence disposition |
| `09-agent-playbook.md` | Exact workflow and prompts for design, implementation, and validation agents |

Templates under `templates/` are intended to be copied into the implementation repository and completed as work proceeds.

## Recommended repository placement

Place the contents of this kit under:

```text
ParchMint/
├── AGENTS.md
├── docs/
│   ├── product/
│   │   └── 01-product-specification.md
│   ├── architecture/
│   │   ├── 02-final-architecture.md
│   │   └── adr/
│   ├── design/
│   │   ├── 03-penpot-design-brief.md
│   │   ├── 04-design-artifact-handoff-contract.md
│   │   └── handoff/
│   ├── implementation/
│   │   ├── 05-implementation-plan.md
│   │   └── 06-acceptance-and-release-plan.md
│   ├── future/
│   │   └── 07-future-work.md
│   └── evidence/
│       └── 08-decisions-and-evidence.md
└── templates/
```

Keeping the files flat initially is also acceptable. Do not change requirement IDs when moving documents.

## Two-stage build workflow

### Stage A: Design in Penpot

1. Give the design agent `01-product-specification.md`, `03-penpot-design-brief.md`, `04-design-artifact-handoff-contract.md`, and `AGENTS.md`.
2. Connect Codex to the Penpot MCP server.
3. Have the agent create the token system, components, screens, states, and prototype flows described in the design brief.
4. Iterate in Penpot with the product owner.
5. Freeze an approved design version and export the complete handoff pack required by the artifact contract.
6. Commit the handoff pack under `design/handoff/<version>/`.

### Stage B: Implement and validate

1. Give the implementation agent the full build kit and approved design handoff.
2. The first implementation task must produce a design reconciliation report and component mapping; it must not begin broad UI coding immediately.
3. Review and approve that mapping.
4. Implement the application in the phases defined by `05-implementation-plan.md`.
5. Run requirement, visual, performance, accessibility, recovery, and cross-platform gates from `06-acceptance-and-release-plan.md` continuously.
6. Record architecture changes as ADRs. Product behavior changes require PRD updates and product-owner approval.

## Important residual risk

The supplied V02-R exploration did not prove native interactive performance, IME, accessibility, or memory behavior on all three operating systems. It also measured a 637–640 ms first editable viewport for the 248,079-word two-view development fixture under Linux WebKitGTK, while the packaged fixture loader was defective. This build kit treats those findings as implementation and release risks rather than reopening frontend exploration.

The implementation must therefore validate Tauri/ProseMirror early using release-mode builds on all three platforms. If the selected stack cannot meet the normative requirements without changing product behavior, the agent must stop and return evidence; it must not silently add a special large-document mode or choose a new framework.
