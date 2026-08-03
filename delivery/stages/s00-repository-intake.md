# S00 — Repository Intake and Baseline

## Goal

Create a reproducible baseline containing current governing documents, an approved theme-complete Penpot handoff, and initialized workflow state. Do not bootstrap Tauri or implement product code.

## Tasks

1. Validate the approved handoff manifest against `delivery/templates/design-handoff/design-manifest.schema.json`, then verify governing versions, file paths, and checksums.
2. Verify complete Light/Dark tokens, Appearance states, appearance matrix, and required reference pairs/fixtures.
3. Verify asset/font inventories, licensing/redistribution status, and deterministic fixture definitions.
4. Verify all governing paths resolve and no deleted historical source is referenced.
5. Confirm the handoff directory is immutable for later stages.
6. Generate or reconcile `delivery/traceability.csv` from the current product specification using `delivery/templates/pipeline/traceability.csv` as the schema. Include every requirement ID exactly once, preserve fields for unchanged IDs, add missing IDs with `status: not_started`, and flag duplicate or unknown IDs.
7. Initialize workflow state and baseline commit.
8. Record Windows/macOS/Linux runner/native-host availability.
9. Record whether a matching approved reconciliation exists; do not infer one from chat.

## Pass criteria

- Approved handoff schema-valid, checksum-valid, theme-complete, licensed/provenanced, and fixture-complete.
- Governing documents identifiable and internally consistent.
- `delivery/traceability.csv` contains every current product requirement exactly once and contains no duplicate or unknown requirement IDs.
- Repository clean and baseline recorded.
- No live Penpot file is treated as the implementation source instead of the frozen handoff.

## Stop conditions

Stop when handoff is missing, unapproved, checksum-invalid, incomplete in Light/Dark/System behavior, or references incompatible governing versions.
