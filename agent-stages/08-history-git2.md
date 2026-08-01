# S70 — Git History Adapter

## Goal

Implement `HistoryStore` using the selected exact `git2 =0.21.0` composition and validated policies.

## Tasks

- Initialize and maintain the app-managed linear `main` history.
- Create autosave, explicit-save, structural, named-snapshot, and restoration checkpoints.
- Support empty named snapshots.
- Implement bounded paging/filtering, whole-checkpoint preview, comparison inputs, and additive whole-project restore. Do not expose partial document/group/subtree restore in v1.
- Isolate missing/corrupt history from current canonical files.
- Implement exclusive-owner stale-lock recovery.
- Implement low-priority pack, verify, and redundant loose-object cleanup without pruning reachable checkpoints.
- Enforce vendored libgit2/static-zlib and no network features.
- Reproduce V03 functional, fault, longevity, and cross-platform interchange semantics.

## Boundary rules

No Git IDs/types escape `HistoryStore`. No history work runs on the UI thread.

## Pass criteria

Adapter contract tests, 250k/1M longevity regressions, fault tests, maintenance scheduling, and Linux→Windows→macOS repository-continuation pass within the architecture’s operating constraints.
