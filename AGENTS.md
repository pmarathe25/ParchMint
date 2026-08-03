# ParchMint Agent Instructions

These instructions apply to every design, coding, testing, orchestration, and review agent working on ParchMint.

`docs/` contains maintained product, architecture, and design knowledge. `delivery/` contains temporary v1 implementation machinery and evidence. A lasting behavior, boundary, visual rule, test obligation, or operating procedure must be promoted to its maintained owner before the temporary delivery artifact is retired; it must not remain authoritative only in a stage report or handoff.

## Read before changing anything

Read, in order:

1. `README.md`
2. `docs/product/product-specification.md`
3. `docs/architecture/architecture.md`
4. `delivery/agent-playbook.md` when participating in the automated pipeline
5. The dispatched file under `delivery/stages/` and the run's `dispatch.yaml`
6. `docs/design/penpot-design-brief.md` and the approved `delivery/design-handoff/<version>/design-manifest.yaml` for UI work

Do not infer v1 scope from extension hooks or `docs/product/future-work.md`. The product specification's Included and Explicitly deferred sections are the complete v1 scope boundary.

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

Do not implement a worker mirror merely because an earlier concept named one. S55 must compare concrete shared-state mechanisms and the allowed projection strategies, then prove the selected pair. S60 must implement that pair. A shared document/history authority, independent per-view selection/scroll/search/focus/composition, and nonblocking canonical projection are mandatory; neither the sharing mechanism nor mirror mechanism is predetermined.

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

1. Validate the frozen handoff against `delivery/design-handoff-contract.md`.
2. Produce the versioned reconciliation package under `delivery/design-reconciliation/<handoff-version>/`.
3. Obtain committed G10 approval.
4. Produce stable Penpot-to-code component and screen mappings.
5. Import Light and Dark tokens into generated CSS custom properties and typed metadata.
6. Preserve exported SVGs as source assets.
7. Record active conflicts and deviations in the reconciliation package; do not create a permanent design-decision log.

## Pipeline rules

When dispatched by the Orchestrator Agent:

- Read the stage instruction and `delivery/runs/<stage-id>/<run-id>/dispatch.yaml` before changing files.
- Work only from the dispatched baseline and declared ownership scope.
- Create the required `status.yaml`, `handoff.yaml`, `report.md`, and evidence directory.
- Do not edit `delivery/state.yaml`, merge branches, or mark your own work accepted.
- Do not modify governing documents or the approved handoff without a G20 proposal and product-owner approval, except for the pre-authorized S55 concretization described below.
- Update `delivery/traceability.csv` for every requirement addressed by the stage. S00 owns complete requirement-row initialization; S10 owns design mappings; later stages own implementation, test, tier, and disposition fields for their work.
- Later agents must be able to proceed from committed artifacts without conversation history.

## Governing changes

G20 is required before changing:

- A must-level product requirement or approved design behavior.
- A selected framework, backend, or canonical format.
- An authoritative state owner or public architectural boundary.
- A process/thread model with material correctness, packaging, licensing, security, or privacy impact.
- A mandatory performance, accessibility, data-safety, or cross-platform gate.

After approval, update the current governing documents directly. Do not add a historical ADR or changelog entry merely to preserve old reasoning.

S55 is the only pre-authorized governing-document concretization: after its shared-state mechanism and projection strategy pass the declared native gates, the stage records the exact architecture patch in its handoff. The Orchestrator independently verifies the evidence and applies or accepts that bounded projection-section update. Any framework, public-boundary, state-owner, requirement, or gate change still requires G20.

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
