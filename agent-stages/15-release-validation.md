# S130 — Independent Release Validation

## Goal

Independently determine whether the release candidate satisfies every v1 requirement and release gate. Do not modify product requirements or production code during validation.

## Independence

Use a fresh validation agent/session that did not implement the candidate. It may file defects but must not redefine pass criteria.

## Tasks

Create the complete release-evidence package from `docs/implementation/06-acceptance-and-release-plan.md`, including:

- Requirement-by-requirement disposition.
- Windows/macOS/Linux package and runtime results.
- Ordinary and approximately 250,000-word one/two-view editor evidence.
- IME, clipboard, accessibility, high-DPI, visual, performance, memory, and background-load evidence.
- Save/recovery/fault, history/search scale, and cross-platform interchange evidence.
- Security, license, notices, provenance, and SBOM results.
- Known issues and proposed explicit waivers.

## Output and stop

Write `release-evidence/<candidate-version>/release-approval.yaml` with `status: pending`. Set the stage result to `needs_approval` and stop at G90.
