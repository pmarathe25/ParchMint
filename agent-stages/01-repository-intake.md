# S00 — Repository Intake and Baseline

## Goal

Create a reproducible baseline containing current governing documents, an approved theme-complete Penpot handoff, and initialized workflow state. Do not bootstrap Tauri or implement product code.

## Tasks

1. Verify the approved handoff manifest, governing versions, file paths, and checksums.
2. Verify complete Light/Dark tokens, Appearance states, appearance matrix, and required reference pairs.
3. Verify all governing paths resolve and no deleted historical source is referenced.
4. Confirm the handoff directory is immutable for later stages.
5. Initialize workflow state and baseline commit.
6. Record Windows/macOS/Linux runner/native-host availability.
7. Record whether a matching approved reconciliation exists; do not infer one from chat.

## Pass criteria

- Approved handoff complete/checksum-valid/theme-complete.
- Governing documents identifiable and internally consistent.
- Repository clean and baseline recorded.
- No live Penpot file is treated as the implementation source instead of the frozen handoff.

## Stop conditions

Stop when handoff is missing, unapproved, checksum-invalid, incomplete in Light/Dark/System behavior, or references incompatible governing versions.
