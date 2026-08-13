# `parchmint-search-api`

## What it does

This crate defines whole-project search. The search index contains text copied
from the project files and open editor sessions. ParchMint can rebuild the index
from those sources. Each editor view handles its own local Find command.

Global replacement uses search hits only as candidates. The application
rechecks each hit and performs the actual document changes.

## How it works

```text
document text and revision -> replace indexed document -> receipt

search query -> small batches of possible matches -> revision and text recheck
             -> navigate or build replacement preview
```

The application recovers from a deleted or corrupt index by rebuilding it from
canonical project files.

## Interface

```rust
pub trait SearchIndex: Send + Sync {
    fn open_or_rebuild(&self, project: ProjectId, source: &dyn SearchProjectionSource)
        -> Result<SearchIndexState, SearchError>;
    fn open_or_rebuild_background(&self, project: ProjectId, source: Arc<dyn SearchProjectionSource>)
        -> Result<SearchIndexState, SearchError>;
    fn rebuild_status(&self) -> SearchRebuildStatus;
    fn replace_document(&self, projection: SearchDocumentProjection)
        -> Result<ProjectionReceipt, SearchError>;
    fn delete_document(&self, id: DocumentId, revision: RevisionId)
        -> Result<ProjectionReceipt, SearchError>;
    fn query(&self, query: SearchQuery, sink: Box<dyn SearchBatchSink>)
        -> Result<(), SearchError>;
    fn cancel(&self, generation: u64);
    fn verify(&self) -> Result<SearchIntegrityReport, SearchError>;
    fn rebuild(&self, source: &dyn SearchProjectionSource)
        -> Result<RebuildReport, SearchError>;
}

pub struct SearchHit {
    pub document_id: DocumentId,
    pub block_id: BlockId,
    pub indexed_revision: RevisionId,
    pub field: SearchField,
    pub candidate_range: TextRange,
    pub snippet: SearchSnippet,
}
```

## Implementation

The index stores body text, display title, Synopsis, and project-defined
metadata. Ranking matches differently per field is not yet implemented.
Callers provide known field names and ordinary search text. The implementation
builds the database query itself.

After the index finds possible matches, ParchMint checks case-sensitive and
Unicode whole-word rules against the current text. It also checks that the
document revision and text range still match the project file or open editor
session.

Results arrive in small batches. Each query has a generation number. When a new
query starts, the application cancels the old query and ignores any old batch
that arrives later. Search errors do not change project files or save state. A
missing, corrupt, or incompatible index is deleted and rebuilt. Search runs on
a background worker and has no network access.
