# `parchmint-search-sqlite`

## What it does

This crate implements `SearchIndex` with bundled SQLite FTS5. One background
worker uses one database connection for each open project. SQL, connections,
rows, statements, and SQLite errors stay inside this crate.

The crate uses `rusqlite` 0.40.1 with bundled SQLite. Startup creates an FTS5
table and fails to open the index if FTS5 is unavailable.

Each project's database is `.parchmint/cache/search.sqlite` inside the project
directory. It is auxiliary data, excluded from History, and safe to delete and
rebuild from the current project files.

```toml
rusqlite = { version = "=0.40.1", default-features = false, features = ["bundled"] }
```

## How it works

```text
document text and revision -> update content and FTS rows in one transaction
                           -> verify IDs and revision -> receipt

SearchQuery -> check field names and quote text -> possible FTS5 matches
            -> check exact rules -> small result batches
```

## Interface

```rust
pub struct SqliteSearchIndex {
    sender: mpsc::Sender<Command>,
    worker: Mutex<Option<thread::JoinHandle<()>>>,
    operation: Mutex<()>,
    cancellation: Arc<Cancellation>,
    rebuild_status: Arc<Mutex<SearchRebuildStatus>>,
    background_rebuild: Mutex<Option<BackgroundRebuild>>,
    next_rebuild_generation: AtomicU64,
}

impl SqliteSearchIndex {
    /// Creates a lazy index rooted at `<project>/.parchmint/cache/search.sqlite`.
    pub fn new(project_root: impl AsRef<Path>) -> Self;
}

impl SearchIndex for SqliteSearchIndex {
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
```

## Implementation

The database stores stable project, document, block, field, and revision IDs.
Its tokenizer is `unicode61 remove_diacritics 2`. After FTS5 finds possible
matches, the crate checks case-sensitive and Unicode whole-word rules against
the current text.

The API never accepts raw SQL or raw FTS5 `MATCH` expressions. It quotes or
escapes user text and accepts only known field names. One database transaction
updates the content row, FTS row, and revision together.

Each query has a generation number. Cancellation interrupts SQLite when that is
safe, and the application ignores results from an older generation. If the
database is corrupt, has the wrong schema, or fails its integrity check, the
crate deletes it and rebuilds it from the project text. SQLite work runs on its
background worker and has no network access.
