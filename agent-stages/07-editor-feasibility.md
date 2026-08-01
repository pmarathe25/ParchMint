# S55 — Shared Editor and Projection Feasibility

## Goal

Select and prove the shared two-view state topology and canonical-projection strategy before production editor implementation.

## Required strategies to evaluate

1. ProseMirror model/document mirror in a Web Worker.
2. Neutral block/delta mirror in a Web Worker or Rust worker.
3. Bounded incremental/idle projection without a persistent mirror.

Retire a strategy early only with concrete incompatibility evidence.

## Tasks

- Build bounded prototypes with representative schema/comments/Unicode.
- Implement one shared document/history authority plus two independent view sessions.
- Classify shared, view-local, and derived plugin state.
- Exercise alternating edits/undo, selection mapping, scroll/focus/local search, stale transactions, and IME composition.
- Run ordinary and approximately 250k fixtures in one/two views.
- Produce canonical/change/title/word-count/annotation/recovery projections.
- Implement bounded queue/coalescing, projection-target failure, snapshot resync.
- Measure first-editable, input-to-frame, propagation, projection latency/backlog, memory/reclamation, canonical equality, recovery replay.
- Run release-mode Windows WebView2, macOS WKWebView, Linux WebKitGTK checks including native IME/clipboard/accessibility.

## Pass criteria

At least one strategy preserves required semantics/features, meets performance gates, never blocks input, uses bounded/coalesced queues, recovers deterministically after projection-target failure, and has credible native behavior on all platforms.

## Output

- Selected strategy and exact production contract changes.
- Raw comparative evidence.
- Direct update to the current architecture projection section before S60.

If none passes, stop at G20. Do not default to the original worker-mirror concept.
