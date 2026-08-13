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

## Interface

```rust
pub trait Exporter: Send + Sync {
    fn plan(
        &self,
        request: ExportRequest,
        project: &ProjectSnapshot,
    ) -> Result<ExportPlan, ExportError>;
    fn validate(&self, plan: &ExportPlan) -> ExportValidationReport;
    fn export(
        &self,
        plan: ExportPlan,
        sink: Box<dyn ExportSink>,
        handle: ExportHandle,
        progress: Arc<dyn ExportProgressSink>,
    ) -> Result<ExportCompletion, ExportError>;
    fn cancel(&self, handle: &ExportHandle);
}

pub struct ExportPlan {
    scope: OrderedExportScope,
    styles: ExportStyleCatalog,
    items: Vec<SemanticExportItem>,
    run_options: ExportRunOptions,
    target: ExportTargetCapability,
    source_revisions: BTreeMap<DocumentId, SourceRevision>,
}

impl ExportPlan {
    pub fn build(
        request: ExportRequest,
        project: &ProjectSnapshot,
    ) -> Result<Self, ExportValidationReport>;
    // Fields are private; read them with scope(), styles(), items(),
    // run_options(), target(), and source_revisions().
}

pub enum SemanticExportItem {
    GroupHeading(ExportHeading),
    Document(ExportDocument),
    PageBreak,
}
```

## Implementation

A plan is immutable. It records the exact revision of every source and copies
document bodies only for the planned manuscript, never reading research,
comments, or metadata, so it does not keep the whole project in memory.

Validation reports missing sources, duplicate documents, mixed source
revisions, and unsafe output targets. `Exporter::export` renders synchronously,
so platform code can run it on its own worker thread. `ExportHandle::cancel`
from another thread takes effect before the next chunk; a cancelled export
aborts its temporary destination and settles on `ExportStatus::Cancelled` with
`ExportError::Cancelled`, never a completion.

An export reads project data and does not change project files, unsaved changes,
undo, recovery data, History, or search data. `ExportSink` writes to a temporary
destination and reports success after the complete output is safe. Application
appearance does not change the plan or the exported project styles. The
exporter can write only through its checked output target and has no shell or
network access.
