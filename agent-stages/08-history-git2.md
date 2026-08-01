# S70 — Git History Adapter

## Goal

Implement `HistoryStore` using `git2 =0.21.0` with the current architecture policies.

## Tasks

- Initialize/maintain app-managed linear `main`.
- Create autosave, explicit-save, structural, named-snapshot, and restoration checkpoints.
- Support empty named snapshots.
- Implement bounded paging/filtering, whole-checkpoint preview/comparison inputs, and additive whole-project restore. No partial document/group/subtree restore.
- Include canonical project dictionary; exclude appearance/global dictionary/workspace/derived state.
- Isolate missing/corrupt history from current files.
- Implement exclusive-owner stale-lock recovery.
- Implement low-priority pack/verify/redundant-loose-object cleanup without pruning reachable checkpoints.
- Enforce vendored libgit2/static-zlib/no network and resolved-lock assertions.

## Boundary rules

No Git IDs/types escape `HistoryStore`; no history work runs on UI thread.

## Pass criteria

- Shared adapter contract and fault tests.
- Tier A/B native functional/continuation tests on all platforms.
- Stage-scale checkpoint tests sufficient to catch regressions.
- Full 1,000,000-checkpoint longevity/pack/memory run is scheduled as Tier C nightly/release evidence rather than required on every stage repair or pull request.
