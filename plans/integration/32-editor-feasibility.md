# Editor feasibility

## Goal

Measure whether the editor approach satisfies mandatory current-scope gates before broad editor delivery.

## Depends on

- [19 Test-support services](../crates/19-test-support-services.md)
- [29 UI Iced shell](../crates/29-ui-iced-shell.md)
- [30 Desktop](../crates/30-desktop.md)
- [31 Editor core](../crates/31-editor-core.md)

## Owning paths

Cross-crate editor feasibility harnesses, measurements, and decision records.

## Requirements and UI design

- [Editor panes and tabs](../../docs/product/editor-panes-and-tabs.md)
- [Rich text and semantic styles](../../docs/product/rich-text-and-semantic-styles.md)
- [Scale and performance](../../docs/product/scale-and-performance.md)
- [Desktop interaction quality](../../docs/product/desktop-interaction-quality.md)
- [Editor and tabs](../../docs/ui-design/editor-and-tabs.md)
- [Screen catalog](../../docs/ui-design/screen-catalog.md)

## Work

- Build a bounded custom-widget prototype over the ParchMint-owned editor core; lay out independent semantic blocks with initial `iced` text primitives.
- Evaluate `text-document = 1.8.0` only behind the private `DocumentEngine`
  seam. ParchMint retains IDs, transactions, comments, anchors, undo,
  revisions, and canonical projection.
- Reconsider `text-typeset = 1.7.0` only for measured `iced` text-layout,
  rendering, hit-testing, or incremental-layout deficiencies. If
  `text-document` requires invasive patches or a maintained fork, use a
  ParchMint-owned engine. Compare GPUI only after `iced` fails a mandatory
  current-scope gate.

## Stage-specific tests and validation

Measure two simultaneous views, shared undo, independent selection/scroll/search/focus, normal en-US keyboard input, paste sanitization, layout-consistent drawing/hit-testing/caret/selection, affected-block invalidation, canonical fidelity, recovery, latency, memory, lifecycle, and failures on Windows, macOS, and Linux. IME, multilingual, bidirectional, and screen-reader gates are excluded. Stop before broad editor work if a mandatory gate fails.
