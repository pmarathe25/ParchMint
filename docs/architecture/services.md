# Application services

> Read when changing binder commands, projections, search, statistics,
> compile, export, or their crate ownership.

`parchmint-app` coordinates Qt-free use cases over domain, storage, Markdown,
index, and compile crates. The bridge converts these results for Qt.

## Structure and projections

The domain graph owns stable node/document IDs, parent-child invariants, styles,
presets, commands, events, undo, and redo. Workspace projections expose bounded
binder, outline, card, pane, and metadata rows keyed by stable IDs. Routine
changes publish typed deltas; full resets are for filter/sort projection changes.

## Search and statistics

Canonical metadata and bodies project into versioned SQLite FTS5 rows. The
index stores query fields and cached counts, but it is never authoritative.
Rebuilds and searches carry revisions and publish bounded result batches.

## Compile and export

Compile freezes binder order, selected scope, preset, metadata, styles, assets,
and body revisions into immutable input. The compile crate builds validated IR
and prepares Markdown, text, HTML, EPUB, DOCX, or PDF-render input. A prepared
artifact is committed only after a final freshness and collision check.

## Rules

- Put invariants in `parchmint-domain`, orchestration in `parchmint-app`, Qt conversion in the bridge.
- Keep research excluded unless selection or preset includes it.
- Preserve deterministic order and output.
- Cancellation or stale completion never changes the requested destination.
- Keep format-specific fidelity explicit in the [export table](../reference/export-fidelity.md).
