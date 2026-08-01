# S00 — Repository Intake and Baseline

## Goal

Create a reproducible baseline containing current governing documents, an approved theme-complete Penpot handoff, and initialized workflow state. Do not bootstrap Tauri or implement product code.

## Tasks

1. Verify the approved handoff manifest, governing versions, file paths, and checksums.
2. Verify complete Light/Dark tokens, Appearance states, appearance matrix, and required reference pairs.
3. Verify all governing paths resolve and no deleted historical source is referenced.
4. Confirm the handoff directory is immutable for later stages.
5. Generate or reconcile `docs/traceability.csv` from the current product specification using `templates/traceability-matrix.csv` as the schema. Include every requirement ID exactly once, preserve fields for unchanged IDs, add missing IDs with `status: not_started`, and flag duplicate or unknown IDs.
6. Initialize workflow state and baseline commit.
7. Record Windows/macOS/Linux runner/native-host availability.
8. Record whether a matching approved reconciliation exists; do not infer one from chat.

## Pass criteria

- Approved handoff complete/checksum-valid/theme-complete.
- Governing documents identifiable and internally consistent.
- `docs/traceability.csv` contains every current product requirement exactly once and contains no duplicate or unknown requirement IDs.
- Repository clean and baseline recorded.
- No live Penpot file is treated as the implementation source instead of the frozen handoff.

## Stop conditions

Stop when handoff is missing, unapproved, checksum-invalid, incomplete in Light/Dark/System behavior, or references incompatible governing versions.
