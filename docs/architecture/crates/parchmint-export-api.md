# `parchmint-export-api`

## What it does

This crate defines a common `ExportPlan` for every export format. The plan
contains the ordered manuscript blocks, formatting marks, project styles,
titles, page breaks, export options, and a checked output target. It contains no
editor-engine nodes, parser nodes, UI widgets, or raw operating-system paths.

The project and application decide what belongs in the plan. An exporter renders
that list. It does not discover extra project content or edit authored data.

## How it works

```text
request + fixed project snapshot
  -> resolve ordered content and inherited settings
  -> build and validate immutable ExportPlan
  -> render small chunks through ExportSink
```

The plan names every source revision, so later edits cannot silently enter an
export already in progress.

## Public API

```rust
pub trait Exporter: Send + Sync {
    fn plan(&self, request: ExportRequest, project: &ProjectSnapshot)
        -> Result<ExportPlan, ExportError>;
    fn validate(&self, plan: &ExportPlan) -> ExportValidationReport;
    fn export(&self, plan: ExportPlan, sink: ExportSink)
        -> Result<ExportHandle, ExportError>;
    fn cancel(&self, handle: ExportHandle);
}

pub struct ExportPlan {
    pub scope: OrderedExportScope,
    pub styles: ExportStyleCatalog,
    pub items: Vec<SemanticExportItem>,
    pub run_options: ExportRunOptions,
    pub target: ExportTargetCapability,
    pub source_revisions: BTreeMap<ResourceId, RevisionId>,
}

pub enum SemanticExportItem {
    GroupHeading(ExportHeading),
    Document(ExportDocument),
    PageBreak,
}
```

## Implementation

A plan is immutable. It records the exact revision of every source and loads
document bodies only as they are needed, so it does not keep the whole project
in memory.

Validation reports missing resources, mixed revisions, invalid document
structure, unsupported links, and invalid output targets. A background worker
renders the plan and writes the destination. It checks for cancellation between
small chunks and reports a cancelled export as incomplete.

An export reads project data and does not change project files, unsaved changes,
undo, recovery data, History, or search data. `ExportSink` writes to a temporary
destination and reports success after the complete output is safe. Application
appearance does not change the plan or the exported project styles. The
exporter can write only through its checked output target and has no shell or
network access.
