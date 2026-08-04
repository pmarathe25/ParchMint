# ParchMint v1 Build Kit

**Status:** Implementation planning is current; an approved design handoff is committed.

This repository separates maintained product/architecture/design knowledge under `docs/` from the temporary v1 implementation and validation kit under `delivery/`.

ParchMint is a local-first desktop application for planning and writing novels. The first release targets Windows, macOS, and Linux and uses Tauri, React, ProseMirror, Rust application services, open canonical project files, app-managed Git history, and a disposable SQLite FTS5 search index.

## Current implementation decisions

- Desktop shell: Tauri 2.11.5.
- Application UI: TypeScript and React in the platform webview.
- Rich-text editor: exact-locked ProseMirror packages behind a ParchMint-owned editor contract.
- Application services: Rust.
- Canonical authored data: restricted deterministic HTML5, TOML, CSS, and JSON/text sidecars.
- History: `git2 =0.21.0` with vendored libgit2 behind `HistoryStore`.
- Search: `rusqlite =0.40.1` with bundled SQLite FTS5 behind `SearchIndex`.
- Export: one self-contained HTML5 export of the entire Manuscript.
- Appearance: System, Light, and Dark. Dark uses fully dark application and manuscript surfaces with a charcoal-and-mint visual system.
- First-class platforms: Windows, macOS, and Linux from v1.

This is the selected implementation baseline, not native acceptance evidence. S20 must prove packaged launch and privileged IPC on all three platforms; S55 and S60 must prove the editor and projection architecture in packaged release builds. A failure stops at G20 rather than silently changing the framework or product requirements.

All supported documents, including documents near 250,000 words, retain the same user-visible features. Internal optimizations may vary transparently, but implementation agents may not introduce a large-document mode or reduce editing, comments, formatting, search, spellcheck, or two-view behavior by document size.

## Source-of-truth order

When materials conflict, use this precedence:

1. `docs/product/product-specification.md`
2. `docs/architecture/architecture.md`
3. The product-owner-approved handoff at `delivery/design-handoff/<version>/design-manifest.yaml`
4. `docs/design/penpot-design-brief.md`
5. `delivery/design-handoff-contract.md`
6. `delivery/implementation-plan.md`
7. `delivery/acceptance-and-release-plan.md`
8. `delivery/agent-playbook.md` and `delivery/stages/` for execution mechanics

A design artifact may clarify layout and interaction but may not silently change product behavior. A coding agent must route a material conflict through G20 instead of choosing its own interpretation.

## Current entry condition

The implementation pipeline must not start until a product-owner-approved, checksum-valid handoff is committed under:

```text
delivery/design-handoff/<version>/
```

The final handoff must include the remediated Light and Dark designs and the Appearance setting. Until that directory exists and validates, S00 must report `blocked`.

## Automated workflow

After the approved handoff is committed, start one lead coding agent with:

- `AGENTS.md`
- `delivery/agent-playbook.md`
- `delivery/stages/orchestrator.md`

The Orchestrator Agent initializes repository-backed pipeline state, dispatches fresh bounded agents where work can safely be separated, pairs production-behavior stages with requirements-first independent test challenges, verifies their committed artifacts, and stops at G10. After G10 approval it advances routine stages automatically and stops only at:

- **G20:** a material product, design, architecture, licensing, security, or mandatory-requirement change is required.
- **G90:** the release candidate is ready for product-owner approval.
- A defined external-input requirement such as signing credentials or a human-only native accessibility session.

The single canonical stage graph is in `delivery/implementation-plan.md`. Agents communicate through committed run artifacts under `delivery/`; later agents must not depend on earlier chat transcripts.

## Maintained documentation

| Path | Purpose |
|---|---|
| `AGENTS.md` | Repository-wide implementation rules |
| `docs/product/product-specification.md` | Normative v1 product behavior |
| `docs/product/future-work.md` | Non-v1 roadmap and extension constraints; never current scope |
| `docs/architecture/architecture.md` | State ownership, module boundaries, and implementation architecture |
| `docs/design/penpot-design-brief.md` | Current visual and interaction brief |

## Temporary v1 delivery kit

Everything under `delivery/` exists to produce and validate the first implementation. See `delivery/README.md` for its retirement and promotion rules.

| Path | Temporary purpose |
|---|---|
| `delivery/design-handoff-contract.md` | Assemble the approved Penpot implementation input |
| `delivery/implementation-plan.md` | Current implementation sequence and risk gates |
| `delivery/acceptance-and-release-plan.md` | Acceptance obligations to encode in tests, CI, and maintained release procedures |
| `delivery/agent-playbook.md` | Orchestration and approval mechanics |
| `delivery/stages/` | Bounded current-delivery agent instructions |
| `delivery/templates/README.md` | Template owners, generated destinations, and lifecycle |

## Documentation policy

Maintained documents describe the current product and system, not the delivery history. The product specification—not the roadmap—defines deferred v1 scope. When an approved change occurs, update the maintained source of truth directly. Temporary handoffs, reconciliation, acceptance prose, proposals, and stage evidence stay under `delivery/` and must not become an alternate product or architecture history.
