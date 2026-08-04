# S55 — Shared Editor and Projection Feasibility

## Goal

Select and prove the shared two-view state topology and canonical-projection strategy before production editor implementation.

## Shared-state mechanisms to evaluate

Compare concrete mechanisms for one document/history authority across two mounted views. Each candidate must define transaction ordering, undo grouping, independent selection/composition/plugin state, simultaneous and stale edits, detach/reattach, and lifecycle recovery. Two ordinary independent ProseMirror histories are not a valid candidate.

## Projection strategies to evaluate

1. ProseMirror model/document mirror in a Web Worker.
2. Neutral block/delta mirror in a Web Worker or Rust worker.
3. Bounded incremental/idle projection without a persistent mirror.

Retire a strategy early only with concrete incompatibility evidence.

## Tasks

- Build bounded prototypes with representative schema/comments/Unicode.
- Implement each viable shared document/history mechanism plus two independent view sessions.
- Classify shared, view-local, and derived plugin state.
- Exercise alternating edits/undo, selection mapping, scroll/focus/local search, stale transactions, and IME composition.
- Run ordinary and approximately 250k fixtures in one/two views.
- Produce canonical/change/title/word-count/annotation/recovery projections.
- Implement bounded queue/coalescing, projection-target failure, snapshot resync.
- Measure first-editable, input-to-frame, propagation, projection latency/backlog, memory/reclamation, canonical equality, recovery replay.
- Run packaged release builds on Windows WebView2, macOS WKWebView, and Linux WebKitGTK, including native IME/clipboard/accessibility.

## Pass criteria

At least one shared-state/projection pair preserves required semantics/features, meets performance gates, never blocks input, uses bounded/coalesced queues, recovers deterministically after lifecycle/projection-target failure, and passes packaged native behavior on all platforms.

## Output

- Selected shared-state mechanism, projection strategy, and exact production contract changes.
- Raw comparative evidence.
- An independently sealed oracle/benchmark charter and mechanism-neutral regression tests for the selected public behavior where feasible.
- Exact bounded patch for the current architecture projection section; the Orchestrator applies or accepts it only after independent verification.

If none passes, stop at G20. Do not default to the original worker-mirror concept.
