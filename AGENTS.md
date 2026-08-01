# ParchMint Agent Instructions

These instructions apply to every design, coding, testing, orchestration, and review agent working on ParchMint.

## Read before changing anything

Read, in order:

1. `README.md`
2. `docs/product/01-product-specification.md`
3. `docs/architecture/02-final-architecture.md`
4. `docs/09-agent-playbook.md` when participating in the automated pipeline
5. The dispatched file under `agent-stages/` and the run's `dispatch.yaml`
6. `docs/design/03-penpot-design-brief.md` and the approved `design/handoff/<version>/design-manifest.yaml` for UI work

Do not infer v1 scope from extension hooks. `docs/future/07-future-work.md` is not v1 scope.

## Authority

- The product specification controls observable behavior.
- The architecture controls boundaries, state ownership, canonical formats, and selected technology.
- The approved design handoff and design brief control visual composition and presentation where they do not conflict with the product specification.
- Generated Penpot HTML/CSS is explanatory input, not automatically production code.
- ProseMirror JSON, SQLite, Git objects, recovery logs, and frontend state must never become the only copy of completed authored content.

## Current v1 guardrails

- Global Search opens from the Explorer header, replaces Explorer in the left sidebar, and searches the entire project. Do not add a ribbon Search destination or a user-visible scope selector.
- Comment creation is available through the editor context menu and Comments panel. Do not add a floating selection-end control.
- Project History restores the complete checkpoint state. Document, group, and subtree checkpoint restore are future work.
- Export always covers the entire Manuscript in v1.
- Cards preserve the full hierarchy. Title and Synopsis may be edited where designed; metadata values are read-only in Cards and are edited in Inspector.
- Editor mode has exactly one always-visible formatting toolbar targeting the focused editor view.
- Appearance is configured only in Project Settings/Preferences through System, Light, and Dark choices; do not add a toolbar quick toggle.
- Dark appearance uses fully dark application and manuscript surfaces. Appearance must not alter authored styles or export output.
- Group, Research, and whole-project aggregate word counts are deferred. Selection, active-document, and Manuscript counts remain v1.
- Spellcheck uses the project-default language in v1. Per-document language overrides are deferred.

## Mandatory boundaries

- Do not import Tauri, React, DOM, ProseMirror, `git2`, `rusqlite`, or platform-webview types into public domain/application APIs.
- Access history only through `HistoryStore`.
- Access search only through `SearchIndex`.
- Access spellcheck only through `SpellcheckService`.
- Access project files only through `ProjectRepository`, `CanonicalCodec`, and `AtomicWriter` ports.
- Access the rich editor only through the ParchMint editor contract.
- Keep operating-system behavior behind platform-service adapters.
- Give every authored state a deterministic canonical representation.
- Keep caches, indexes, editor recovery logs, spellcheck decorations, and workspace state rebuildable or disposable as specified.

## Editor-risk rule

Do not implement a worker mirror merely because an earlier concept named one. S55 must compare the allowed projection strategies and prove the shared two-view state topology. S60 must implement the selected strategy. A shared document/history authority, independent per-view selection/scroll/search/focus, and nonblocking canonical projection are mandatory; the mirror mechanism is not predetermined.

## Project undo rule

All project-authoring mutations go through `ProjectCommandDispatcher`. Do not let React components or individual adapters mutate hierarchy, metadata, styles, Synopsis, or global-replacement state directly. Composite operations must produce one project-undo entry and, after durable save, one history checkpoint.

## UI and performance

- Never block the webview/UI thread on filesystem I/O, Git, SQLite, spellcheck dictionaries, canonical serialization, export, or project-wide analysis.
- Do not introduce a size-based feature mode or silently refuse a second view.
- Preserve independent selections and scroll positions when one document is open in two panes.
- Implement Windows, macOS, and Linux behavior from the beginning.
- Use semantic Light/Dark tokens. Hard-coded theme-dependent colors in production components fail the design-system gate.

## Design handoff

Before broad UI implementation:

1. Validate the frozen handoff against `docs/design/04-design-artifact-handoff-contract.md`.
2. Produce the versioned reconciliation package under `docs/design/reconciliation/<handoff-version>/`.
3. Obtain committed G10 approval.
4. Produce stable Penpot-to-code component and screen mappings.
5. Import Light and Dark tokens into generated CSS custom properties and typed metadata.
6. Preserve exported SVGs as source assets.
7. Record active conflicts and deviations in the reconciliation package; do not create a permanent design-decision log.

## Pipeline rules

When dispatched by the Orchestrator Agent:

- Read the stage instruction and `agent-workflow/runs/<stage-id>/<run-id>/dispatch.yaml` before changing files.
- Work only from the dispatched baseline and declared ownership scope.
- Create the required `status.yaml`, `handoff.yaml`, `report.md`, and evidence directory.
- Do not edit `agent-workflow/pipeline-state.yaml`, merge branches, or mark your own work accepted.
- Do not modify governing documents or the approved handoff without a G20 proposal and product-owner approval.
- Update `docs/traceability.csv` for every requirement addressed by the stage. S00 owns complete requirement-row initialization; S10 owns design mappings; later stages own implementation, test, tier, and disposition fields for their work.
- Later agents must be able to proceed from committed artifacts without conversation history.

## Governing changes

G20 is required before changing:

- A must-level product requirement or approved design behavior.
- A selected framework, backend, or canonical format.
- An authoritative state owner or public architectural boundary.
- A process/thread model with material correctness, packaging, licensing, security, or privacy impact.
- A mandatory performance, accessibility, data-safety, or cross-platform gate.

After approval, update the current governing documents directly. Do not add a historical ADR or changelog entry merely to preserve old reasoning.

## Testing

- Add tests with every functional change.
- Use golden fixtures for canonical HTML/TOML/JSON/CSS/text.
- Add shared contract tests for every replaceable port.
- Add visual-reference tests for Penpot-mapped components in Light and Dark.
- CI must regenerate Rust/TypeScript contract types and fail on a dirty diff.
- Use the test tiers in the acceptance plan. Do not claim native performance or accessibility from headless evidence.

## Reporting

Every substantial stage reports:

- Files changed.
- Requirements and design components addressed.
- Contracts and state owners affected.
- Commands and platforms tested.
- Known gaps and assumptions.
- Any required G20 proposal.
- Whether output is production, prototype, generated, or reference-only.
