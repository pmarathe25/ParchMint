# S130 — Independent Release Validation

## Goal

Independently validate the release candidate and produce the G90 package. Do not change production behavior or acceptance criteria.

## Independence

Use an agent/session that did not implement the candidate. It may report defects but not alter governing documents or silently fix criteria.

## Tasks

- Re-run release-required Tier A/B/C gates from clean candidate artifacts.
- Verify traceability and platform matrix.
- Verify Light/Dark visual/accessibility/appearance behavior.
- Verify shared editor/projection, spellcheck, project undo/recovery, history/search, packaging/security/provenance.
- Verify package hashes and clean installs.
- Record reproducible blockers and current known issues.

## Required output

```text
delivery/release-evidence/<candidate-version>/
├── requirement-disposition.csv
├── platform-matrix.yaml
├── visual/
├── performance/
├── accessibility/
├── appearance/
├── editor-projection/
├── spellcheck/
├── recovery-project-undo/
├── history-search/
├── packaging/
├── security-licenses-sbom/
├── package-hashes.txt
├── known-issues.yaml
└── release-approval.yaml
```

Create `release-approval.yaml` with `status: pending` and return `needs_approval`. Stop at G90.

Create the machine-readable files from `delivery/templates/release/` and validate required paths, dispositions, platform rows, and package-hash coverage before returning.
