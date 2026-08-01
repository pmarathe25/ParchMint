# ParchMint Agent Instructions

These instructions apply to every design, coding, testing, orchestration, and review agent working on ParchMint.

## Read before changing anything

Read these files in order:

1. `README.md`
2. `docs/product/01-product-specification.md`
3. `docs/architecture/02-final-architecture.md`
4. `docs/09-agent-playbook.md` when participating in the automated pipeline
5. The task-specific file under `agent-stages/` and the run's `dispatch.yaml`
6. `docs/design/03-penpot-design-brief.md` and the latest approved `design/handoff/<version>/design-manifest.yaml` when working on UI

Do not infer v1 scope from architecture hooks. `docs/future/07-future-work.md` is not v1 scope.

## Source-of-truth rules

- The product specification controls observable behavior.
- The architecture controls module boundaries, state ownership, data formats, and selected technology.
- The approved Penpot handoff and design brief control visual layout, components, and presentation where they do not conflict with the product specification.
- Generated Penpot HTML/CSS is reference material, not automatically production code.
- No implementation may make ProseMirror JSON, SQLite, Git objects, or a frontend store the only copy of authored content.

## Current v1 interaction guardrails

- Global Search is opened from the Explorer header, replaces Explorer in the left sidebar, and searches the entire project. Do not add a top-ribbon Search destination or a user-visible scope selector in v1.
- Comment creation is available through the editor context menu and Comments panel. Do not add a floating selection-end comment affordance.
- Project History restores an entire checkpoint/project state. Document, group, subtree, and other partial checkpoint restore are future work.
- Export always covers the entire Manuscript in v1. Do not add partial-scope or per-node inclusion controls.
- Cards preserve the full hierarchy. Title and Synopsis may be edited where designed; metadata values are read-only in Cards and are edited in Inspector.
- Editor mode has exactly one always-visible formatting toolbar targeting the focused editor view.

## Mandatory architecture boundaries

- Do not import Tauri, React, DOM, ProseMirror, `git2`, or `rusqlite` types into domain-facing public APIs.
- Access history only through `HistoryStore`.
- Access search only through `SearchIndex`.
- Access project files only through `ProjectRepository`, `CanonicalCodec`, and `AtomicWriter` ports.
- Access the rich editor only through the ParchMint editor-adapter contract.
- Keep operating-system behavior behind platform-service adapters.
- Give every authored state a deterministic canonical representation.
- Keep caches, indexes, editor recovery logs, and workspace state rebuildable or disposable as specified.

## UI and performance rules

- Never block the webview/UI thread on filesystem I/O, Git operations, SQLite work, canonical serialization, export, or project-wide analysis.
- Do not introduce a large-document mode, disable features by size, or silently refuse a second view.
- Preserve independent selections and scroll positions when one document is open in two panes.
- Implement Windows, macOS, and Linux behavior from the beginning.

## Design handoff rules

Before broad UI implementation:

1. Validate the frozen handoff against `docs/design/04-design-artifact-handoff-contract.md`.
2. Produce the versioned reconciliation package under `docs/design/reconciliation/<handoff-version>/`.
3. Obtain committed G10 approval before importing final UI details.
4. Produce a stable Penpot-to-code component and screen map.
5. Import tokens into generated CSS custom properties and typed metadata.
6. Preserve exported SVGs as source assets rather than casually redrawing them.
7. Record every intentional visual deviation.

## Automated pipeline rules

When dispatched by the Orchestrator Agent:

- Read the stage instruction and `agent-workflow/runs/<stage-id>/<run-id>/dispatch.yaml` before changing files.
- Work only from the dispatched baseline and declared ownership scope.
- Create the required `status.yaml`, `handoff.yaml`, `report.md`, and `evidence/` directory.
- Do not edit `agent-workflow/pipeline-state.yaml`, merge branches, or mark your own work accepted.
- Do not modify the PRD, architecture, design brief, approved handoff, or acceptance criteria. Draft a G20 proposal for governing changes.
- Later agents must be able to proceed from committed artifacts without conversation history.

## Changes and ADRs

Create an ADR before:

- Changing a selected backend or framework.
- Moving responsibility across an architectural boundary.
- Changing canonical formats or schema versions.
- Introducing a new runtime process or persistent database.
- Adding a dependency that materially affects packaging, licensing, security, or cross-platform behavior.

A product or approved-design change also requires explicit product-owner approval and the corresponding governing-document update.

## Testing discipline

- Add tests with every functional change.
- Use golden fixtures for canonical HTML/TOML/JSON/CSS.
- Add adapter contract tests for replaceable implementations.
- Add visual-reference tests for Penpot-mapped components.
- Run cross-platform CI for merges affecting packaging, editor input, filesystem behavior, Git, SQLite, or platform services.
- Do not claim native performance or accessibility from synthetic/headless evidence alone.

## Reporting

Every substantial stage must report:

- Files changed.
- Requirements and design components addressed.
- Tests and platforms run.
- Known gaps and assumptions.
- Proposed ADR, G20, or PRD/design change.
- Whether output is production, prototype, generated, or reference-only.
