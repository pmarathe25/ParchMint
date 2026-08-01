# S00 — Repository Intake and Baseline

## Goal

Create a reproducible implementation baseline containing the governing build kit, approved Penpot handoff, and initialized agent-workflow state. Do not bootstrap Tauri or implement product code.

## Inputs

- Full build kit.
- Approved `design/handoff/<version>/` package.
- Repository access.
- `templates/agent-workflow/`.

## Tasks

1. Verify the approved handoff manifest, declared product-spec version, file paths, and checksums.
2. Verify all governing documents are present and internally referenced paths resolve.
3. Confirm the design handoff directory is immutable for later stages.
4. Initialize Git when needed and create a planning-baseline commit.
5. Initialize `agent-workflow/` from templates.
6. Record toolchain availability and Windows/macOS/Linux runner availability without installing the application stack yet.
7. Record whether a prior approved reconciliation exists; do not assume one from chat.

## Required outputs

- Baseline commit.
- Initialized `agent-workflow/pipeline-state.yaml`.
- `status.yaml`, `handoff.yaml`, and `report.md` for S00.
- Evidence containing checksum validation and repository inventory.

## Pass criteria

- Approved handoff is complete and checksum-valid.
- Governing documents and versions are identifiable.
- Repository is clean and baseline commit is recorded.
- No live Penpot file is treated as the implementation source instead of the frozen handoff.

## Stop conditions

Stop when the handoff is incomplete, unapproved, checksum-invalid, or references an incompatible product-spec version.
