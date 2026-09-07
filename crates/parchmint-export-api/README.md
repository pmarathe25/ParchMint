# `parchmint-export-api`

**Purpose:** Define an immutable manuscript export plan and the interfaces that
render it. The application chooses content; exporters render that content
without changing the project.

## Interface

`ExportPlan::build` validates a fixed project snapshot and `ExportRequest`. The
plan contains ordered manuscript blocks, formatting, styles, titles, page breaks,
options, checked output target, and exact source revisions. It copies only planned
manuscript bodies; Research, comments, and metadata stay out of the plan.

`Exporter::export` renders synchronously through `ExportSink`, so callers run it
on a worker. `ExportHandle` tracks progress and accepts cancellation from another
thread. See [lib.rs](src/lib.rs) for signatures and ParchMint value types.

## Validation and output

Plan construction rejects missing sources, duplicate documents, mixed revisions,
and unsafe targets. Later edits cannot enter an export already in progress.
Application appearance does not affect project styles or output.

The sink writes a temporary destination and reports success after the complete
output is safe. Cancellation takes effect before the next chunk, aborts the
temporary destination, and returns `ExportStatus::Cancelled` with
`ExportError::Cancelled`.

Export leaves project files, dirty state, undo, recovery, History, and search
unchanged. Editor-engine nodes, parser nodes, widgets, and raw OS paths do not
cross the API. Exporters write only through the checked sink and have no shell
or network access.
