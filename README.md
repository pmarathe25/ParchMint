# ParchMint v1 Build Kit

**Status:** Implementation planning is current; final design handoff is pending.

This repository contains the governing product, architecture, design-handoff, implementation, and validation documents for the first cross-platform release of ParchMint.

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

All supported documents, including documents near 250,000 words, retain the same user-visible features. Internal optimizations may vary transparently, but implementation agents may not introduce a large-document mode or reduce editing, comments, formatting, search, spellcheck, or two-view behavior by document size.

## Source-of-truth order

When materials conflict, use this precedence:

1. `docs/product/01-product-specification.md`
2. `docs/architecture/02-final-architecture.md`
3. The latest product-owner-approved handoff at `design/handoff/<version>/design-manifest.yaml`
4. `docs/design/03-penpot-design-brief.md`
5. `docs/design/04-design-artifact-handoff-contract.md`
6. `docs/implementation/05-implementation-plan.md`
7. `docs/implementation/06-acceptance-and-release-plan.md`
8. `docs/future/07-future-work.md`
9. `docs/09-agent-playbook.md` and `agent-stages/` for execution mechanics

A design artifact may clarify layout and interaction but may not silently change product behavior. A coding agent must route a material conflict through G20 instead of choosing its own interpretation.

## Current entry condition

The implementation pipeline must not start until a product-owner-approved, checksum-valid handoff is committed under:

```text
design/handoff/<version>/
```

The final handoff must include the remediated Light and Dark designs and the Appearance setting. Until that directory exists and validates, S00 must report `blocked`.

## Automated workflow

After the approved handoff is committed, start one lead coding agent with:

- `AGENTS.md`
- `docs/09-agent-playbook.md`
- `agent-stages/00-orchestrator.md`

The Orchestrator Agent initializes repository-backed pipeline state, dispatches design reconciliation, and stops at G10. After G10 approval it advances routine stages automatically and stops only at:

- **G20:** a material product, design, architecture, licensing, security, or mandatory-requirement change is required.
- **G90:** the release candidate is ready for product-owner approval.
- A defined external-input requirement such as signing credentials or a human-only native accessibility session.

The highest-risk foundations are selected and proven before broad feature work:

- S55: shared two-view editor and projection feasibility.
- S60: production editor foundation using the proven strategy.
- S65: cross-platform spellcheck foundation.

Agents communicate through committed run artifacts under `agent-workflow/`; later agents must not depend on earlier chat transcripts.

## Governing files

| Path | Purpose |
|---|---|
| `AGENTS.md` | Repository-wide implementation rules |
| `docs/product/01-product-specification.md` | Normative v1 product behavior |
| `docs/architecture/02-final-architecture.md` | State ownership, module boundaries, and implementation architecture |
| `docs/design/03-penpot-design-brief.md` | Current visual and interaction brief |
| `docs/design/04-design-artifact-handoff-contract.md` | Required immutable Penpot handoff |
| `docs/implementation/05-implementation-plan.md` | Ordered implementation stages and risk gates |
| `docs/implementation/06-acceptance-and-release-plan.md` | Test tiers and release gates |
| `docs/future/07-future-work.md` | Explicitly deferred capabilities |
| `docs/09-agent-playbook.md` | Pipeline operation and approval instructions |
| `agent-stages/` | Bounded stage-agent contracts |

## Documentation policy

These documents exist to drive the current implementation. Do not add changelogs, architecture-decision logs, historical exploration reports, or superseded rationale to the governing set. When an approved change occurs, update the current source-of-truth documents directly. Temporary G20 proposals and stage evidence may exist while work is active, but they do not become an alternate product or architecture history.
