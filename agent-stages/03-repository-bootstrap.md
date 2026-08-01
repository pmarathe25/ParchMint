# S20 — Repository Bootstrap and Governance

## Goal

Implement Phase 0 of the implementation plan only.

## Entry conditions

- G10 reconciliation approval is committed and checksum-consistent with the design handoff.
- S00 and S10 handoffs are accepted.

## Tasks

1. Create the monorepo layout from the final architecture.
2. Pin Rust, Node, package-manager, Tauri, React, and exact ProseMirror dependencies.
3. Bootstrap an empty Tauri/React desktop shell and empty headless CLI.
4. Prove one generated/validated Rust↔TypeScript JSON Schema contract round trip.
5. Add format, lint, typecheck, unit-test, build, and package commands.
6. Add Windows, macOS, and Linux CI from the first implementation commit.
7. Add dependency, license, advisory, provenance, and SBOM tooling.
8. Add ADR directory/templates and repository contribution/setup instructions.
9. Add deterministic fixture/checksum tooling.
10. Do not implement product features or final UI components.

## Required outputs

- Clean native builds on all three platforms or documented native runner evidence required by the plan.
- Exact application lockfiles and dependency inventory.
- CI matrix.
- ADR-0001 capturing the accepted architecture.
- Stage report and handoff with canonical commands.

## Pass criteria

All Phase 0 gate commands pass; one contract round trip works; no feature behavior or design interpretation was introduced.
