# ParchMint Agent Instructions

These instructions apply to every design, coding, testing, and review agent working on ParchMint.

## Read before changing anything

Read these files in order:

1. `README.md`
2. `01-product-specification.md`
3. `02-final-architecture.md`
4. The task-specific document named by the user
5. The latest approved `design-manifest.yaml`, when working on UI

Do not infer deferred features from architecture hooks. `07-future-work.md` is not v1 scope.

## Source-of-truth rules

- The PRD controls product behavior.
- The architecture controls module boundaries, state ownership, and technology decisions.
- Approved Penpot artifacts control visual layout and interaction details only where they do not conflict with the PRD.
- Generated Penpot HTML/CSS is reference material, not automatically production code.
- No implementation detail may make ProseMirror JSON, SQLite, Git objects, or a frontend store the only copy of authored project content.

## Mandatory architecture boundaries

- Do not import Tauri, React, DOM, ProseMirror, `git2`, or `rusqlite` types into domain-facing public APIs.
- Access history only through `HistoryStore`.
- Access search only through `SearchIndex`.
- Access project files only through `ProjectRepository`, `CanonicalCodec`, and `AtomicWriter` ports.
- Access the rich editor only through the ParchMint editor adapter contract.
- Keep all operating-system behavior behind platform-service adapters.
- All authored state must have a deterministic canonical representation.
- Caches, indexes, editor recovery logs, and workspace state must remain rebuildable or disposable as specified.

## UI and performance rules

- Never block the webview/UI thread on filesystem I/O, Git operations, SQLite work, canonical serialization, export, or project-wide analysis.
- All supported documents receive the same user-visible behavior. Do not introduce a large-document mode, disable features by size, or silently refuse a second view.
- Preserve independent selections and scroll positions when one document is open in two panes.
- The single shared formatting toolbar always targets the focused editor view.
- Implement cross-platform behavior from the beginning; do not defer Windows or macOS integration until after Linux completion.

## Design handoff rules

Before broad UI implementation:

1. Validate the handoff against `04-design-artifact-handoff-contract.md`.
2. Produce `docs/design/design-reconciliation.md` using the supplied template.
3. Produce a stable component map from Penpot names/IDs to implementation components.
4. Import design tokens into generated CSS custom properties.
5. Preserve exported SVGs as source assets rather than redrawing them casually.
6. Record every intentional visual deviation.

## Changes and ADRs

Create an ADR before:

- Changing a selected backend or framework.
- Moving responsibility across an architectural boundary.
- Changing canonical file formats or schema versions.
- Introducing a new runtime process or persistent database.
- Adding a dependency that materially affects packaging, licensing, security, or cross-platform behavior.

A product requirement change also requires explicit product-owner approval and a PRD update.

## Testing discipline

- Add tests with every functional change.
- Use golden fixtures for canonical HTML/TOML/JSON/CSS.
- Add adapter contract tests for replaceable implementations.
- Add visual reference tests for Penpot-mapped components.
- Run cross-platform CI for every merge that affects platform, packaging, editor input, filesystem, Git, or SQLite behavior.
- Do not claim a performance or accessibility pass from synthetic or headless-only evidence when native interactive behavior is required.

## Reporting

Every substantial agent task must report:

- Files changed.
- Requirements and design components addressed.
- Tests and platforms run.
- Known gaps or assumptions.
- Any proposed ADR or PRD change.
- Whether the output is production code, prototype code, or reference-only code.
