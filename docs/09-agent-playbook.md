# ParchMint Agent Playbook

**Status:** Final workflow  
**Version:** 1.0  
**Date:** 2026-07-28

## 1. Overview

Use three agent roles:

1. **Design Agent:** creates and iterates the Penpot design.
2. **Implementation Agents:** build the application in bounded phases/workstreams.
3. **Validation Agent:** reconciles implementation against the PRD, architecture, approved Penpot handoff, and release gates.

The product owner remains the authority for requirement or design-approval changes.

## 2. Design-agent setup

Give the Design Agent:

- `AGENTS.md`
- `01-product-specification.md`
- `03-penpot-design-brief.md`
- `04-design-artifact-handoff-contract.md`
- `templates/design-manifest.yaml`
- `templates/traceability-matrix.csv`

Connect Codex to Penpot MCP using current Penpot and Codex MCP instructions. Prefer a project-scoped MCP configuration when the environment supports it.

### Design-agent initial prompt

> Read all supplied ParchMint documents before changing Penpot. Use `03-penpot-design-brief.md` as the operational brief and `01-product-specification.md` as the product source of truth. Connect to the open Penpot file through MCP. Create the file structure, tokens, components, screens, states, accessibility annotations, and prototype flows in the brief. Do not add deferred features or resolve product ambiguities silently. Use stable `PM/` names and cite requirement IDs. At the end of each pass, report completed pages, unresolved product questions, design decisions, and handoff gaps. Do not export the final handoff until I explicitly approve the design.

### Iteration prompt

> Apply the approved feedback to the existing Penpot design without breaking stable component IDs/names unnecessarily. Update components and instances rather than patching individual boards. Update the design decision log and identify any screens/reference images that must be re-exported. Do not make unrelated visual or product changes.

### Final handoff prompt

> Freeze the approved ParchMint design as version `<VERSION>`. Produce every artifact required by `04-design-artifact-handoff-contract.md`: `.penpot`, token JSON, used SVG assets, deterministic reference PNGs, `design-manifest.yaml`, interaction specification, component matrix, screen inventory, keyboard/focus map, cross-platform variants, design decisions, known deviations, and SHA-256 checksums. Validate that all manifest paths exist and that the product-spec version matches. Report any incomplete item rather than omitting it.

## 3. Product-owner design review

Before approving the handoff, review:

- Core layout and information architecture.
- Editor/companion/Inspector focus behavior.
- Cards as the same data projection.
- Deep tree and multi-selection states.
- Comments and selection affordance.
- Search and replacement preview.
- History/Recently Deleted clarity.
- Save/error/recovery states.
- Keyboard/focus/accessibility boards.
- 1280×720 and high-DPI studies.
- Cross-platform variants.

Approval should identify a design version; do not approve an unversioned live file.

## 4. Implementation-agent intake

Place the approved pack under:

```text
design/handoff/<version>/
```

Give the implementation lead:

- Full build kit.
- Approved design handoff.
- Historical evidence only as reference.
- Repository access.
- Native Windows/macOS/Linux runners or hosts.

### First implementation prompt — reconciliation only

> Read `AGENTS.md`, the PRD, final architecture, design handoff contract, implementation plan, and approved `design-manifest.yaml`. Do not begin broad feature implementation. Validate the handoff and produce:
>
> 1. `docs/design/design-reconciliation.md` using the template.
> 2. A Penpot-to-code component map.
> 3. A screen/state-to-route/store map.
> 4. A deterministic token and asset import plan.
> 5. A visual-regression plan.
> 6. A list of conflicts, missing states, or accessibility concerns.
> 7. A proposed repository work breakdown aligned to `05-implementation-plan.md`.
>
> Preserve requirement IDs and identify any decision that needs product-owner approval. Stop after these deliverables.

Review this output before authorizing implementation.

## 5. Repository-bootstrap agent

### Prompt

> Implement Phase 0 of `05-implementation-plan.md` only. Create the monorepo structure, exact toolchain/dependency locks, Tauri/React shell, JSON Schema contract round trip, cross-platform CI, lint/test/build commands, dependency/license/SBOM tooling, and ADR skeleton. Use the V02 reference locks as provenance, preserve the validated ProseMirror package versions initially, and commit a new application lockfile. Do not implement product features. Report builds on Windows, macOS, and Linux and any dependency changes.

## 6. Core and adapter agents

After Phase 0/contract approval, dispatch bounded agents.

### Core/domain agent

> Implement Phase 1 domain, canonical format, migrations, fixtures, title synchronization, word counting, and headless CLI. Do not import frontend, Git, or SQLite types. Pass golden and cross-platform path/Unicode tests.

### Persistence/recovery agent

> Implement Phase 2 project repository, atomic save coordinator, revision acknowledgements, project lock, and recovery journal behind the specified ports. Add fault injection. Do not add history/search behavior except mocks.

### History agent

> Implement `HistoryStore` with exact `git2 =0.21.0` and the policies in `02-final-architecture.md`. Reproduce V03 functional/fault/interchange behavior. Do not expose Git IDs or types to callers and do not enable network features.

### Search agent

> Implement `SearchIndex` with exact bundled `rusqlite =0.40.1`/FTS5 and the validated V04 schema/query behavior. Use a dedicated worker, safe MATCH construction, streaming/cancellation/revalidation, and deterministic rebuild. Do not add Tantivy.

### Design-system/shell agent

> Import approved tokens/assets deterministically and implement Phase 5 shell components against mocked application services. Match the approved design, keyboard/focus behavior, and cross-platform variants. Do not duplicate domain logic in React.

### Editor agent

> Implement Phase 6 only: ProseMirror schema, canonical adapter, SharedEditorSession with two independent ViewSessions and shared history, worker projection/recovery, paste behavior, styles, atomic breaks, comments/anchors, and toolbar targeting. Run the early native cross-platform runtime gate before proceeding. Do not add a large-document mode or reduce features by size.

## 7. Feature-slice agents

Once core, shell, and editor foundations pass, assign one vertical slice per agent or worktree. Each agent must implement the full stack and tests for its slice, not only UI.

A slice prompt should include:

> Implement the `<SLICE>` vertical slice from Phase 7. Cite requirement IDs and Penpot component/screen IDs. Use existing domain/application/port boundaries. Add domain, adapter, frontend, canonical/history/search, visual, accessibility, and cross-platform tests as applicable. Do not alter unrelated behavior. Update traceability and report intentional design deviations.

## 8. Validation agent

### Continuous validation prompt

> Review the current ParchMint implementation against the PRD, final architecture, approved design manifest, traceability matrix, and acceptance plan. Run automated tests, inspect visual-reference diffs, and review architecture-boundary violations. Report issues grouped by product correctness, design fidelity, accessibility, performance, data integrity, cross-platform behavior, and modularity. Do not modify product requirements to make the implementation pass.

### Release-candidate prompt

> Build the release evidence package described in `06-acceptance-and-release-plan.md`. Run native Windows, macOS, and Linux package/runtime tests; ordinary and 250,000-word one/two-view editor tests; IME/clipboard/accessibility/high-DPI tests; save/recovery/fault tests; history/search scale tests; cross-platform project interchange; visual reconciliation; security/license/SBOM checks. Return a requirement-by-requirement disposition and list every waiver. Unknown native evidence is not a pass.

## 9. Design revision after implementation starts

When Penpot changes:

1. Export a new handoff version.
2. Ask a reconciliation agent to compare old/new manifests, tokens, components, screens, and snapshots.
3. Produce a design-diff report and implementation impact list.
4. Review product/architecture conflicts.
5. Update generated tokens/assets and visual baselines.
6. Implement changes through normal pull requests.

Do not let an agent continuously sync a live mutable Penpot file into production without a versioned approval boundary.

## 10. Agent quality controls

- Use small, reviewable tasks.
- Require tests and raw evidence.
- Separate prototypes from production code.
- Prevent agents from changing both a contract and all its implementations without review.
- Keep PRD/design/architecture conflicts visible.
- Use worktrees for parallel agents.
- Require a clean working tree and reproducible commands at handoff.
- Preserve exact dependency locks.
- Stop rather than fabricate native runtime evidence.

## 11. Current Codex MCP references

Codex supports user-level or project-scoped MCP configuration and shares MCP configuration across supported local Codex clients. Use current official instructions:

- <https://developers.openai.com/codex/mcp>
- <https://developers.openai.com/codex/config-basic>

A project-scoped `.codex/config.toml` is preferred when the Penpot server configuration should travel with a trusted project, provided no secrets are committed. Verify active tools with the Codex MCP list before beginning design work.
