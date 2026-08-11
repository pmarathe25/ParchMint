//! Disposable SQLite FTS5 storage for whole-project search.

use std::{
    collections::BTreeSet,
    fs,
    io::ErrorKind,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex, MutexGuard,
        atomic::{AtomicU64, Ordering},
        mpsc,
    },
    thread,
    time::Duration,
};

use parchmint_search_api::{
    BlockId, DocumentId, MetadataFieldId, ProjectId, ProjectionReceipt, RebuildReport, RevisionId,
    SearchBatch, SearchBatchSink, SearchDocumentProjection, SearchError, SearchField,
    SearchFrontierId, SearchHit, SearchIndex, SearchIndexProblem, SearchIndexState,
    SearchIntegrityReport, SearchProjectionSource, SearchProjectionVisitor, SearchQuery,
    SearchRebuildStatus, SearchSnippet, SearchTextProjection, TextRange,
};
use rusqlite::{Connection, ErrorCode, InterruptHandle, OptionalExtension, Transaction, params};

const APPLICATION_ID: i32 = 0x504d_5349;
const SCHEMA_VERSION: i32 = 2;
const RESULT_BATCH_SIZE: usize = 64;
const REBUILD_BUFFER_SIZE: usize = 8;
const FIRST_REBUILD_GENERATION: u64 = 1 << 63;

const SCHEMA: &str = r#"
    PRAGMA application_id = 1347244873;
    PRAGMA user_version = 2;

    CREATE TABLE index_metadata (
        project_id BLOB NOT NULL CHECK(length(project_id) = 16),
        frontier_identity BLOB CHECK(frontier_identity IS NULL OR length(frontier_identity) = 32),
        rebuild_complete INTEGER NOT NULL CHECK(rebuild_complete IN (0, 1))
    ) STRICT;

    CREATE TABLE documents (
        document_id BLOB PRIMARY KEY NOT NULL CHECK(length(document_id) = 16),
        revision BLOB NOT NULL CHECK(length(revision) = 8),
        deleted INTEGER NOT NULL CHECK(deleted IN (0, 1))
    ) STRICT;

    CREATE TABLE search_content (
        row_id INTEGER PRIMARY KEY,
        document_id BLOB NOT NULL REFERENCES documents(document_id) ON DELETE CASCADE,
        block_id BLOB NOT NULL CHECK(length(block_id) = 16),
        field_kind INTEGER NOT NULL CHECK(field_kind BETWEEN 0 AND 3),
        metadata_field_id BLOB,
        text TEXT NOT NULL,
        CHECK(
            (field_kind = 3 AND length(metadata_field_id) = 16)
            OR (field_kind != 3 AND metadata_field_id IS NULL)
        )
    ) STRICT;

    CREATE UNIQUE INDEX search_content_source_unit
        ON search_content(document_id, block_id, field_kind, ifnull(metadata_field_id, X''));

    CREATE VIRTUAL TABLE search_fts USING fts5(
        text,
        content = 'search_content',
        content_rowid = 'row_id',
        tokenize = 'unicode61 remove_diacritics 2'
    );

    CREATE TRIGGER search_content_insert AFTER INSERT ON search_content BEGIN
        INSERT INTO search_fts(rowid, text) VALUES (new.row_id, new.text);
    END;

    CREATE TRIGGER search_content_delete AFTER DELETE ON search_content BEGIN
        INSERT INTO search_fts(search_fts, rowid, text)
            VALUES ('delete', old.row_id, old.text);
    END;

    CREATE TRIGGER search_content_update AFTER UPDATE ON search_content BEGIN
        INSERT INTO search_fts(search_fts, rowid, text)
            VALUES ('delete', old.row_id, old.text);
        INSERT INTO search_fts(rowid, text) VALUES (new.row_id, new.text);
    END;
"#;

/// A project-local search index whose SQLite connection stays on one worker.
pub struct SqliteSearchIndex {
    sender: mpsc::Sender<Command>,
    worker: Mutex<Option<thread::JoinHandle<()>>>,
    operation: Mutex<()>,
    cancellation: Arc<Cancellation>,
    rebuild_status: Arc<Mutex<SearchRebuildStatus>>,
    background_rebuild: Mutex<Option<BackgroundRebuild>>,
    next_rebuild_generation: AtomicU64,
}

struct BackgroundRebuild {
    generation: u64,
    worker: thread::JoinHandle<()>,
}

impl SqliteSearchIndex {
    /// Creates a lazy index rooted at `<project>/.parchmint/cache/search.sqlite`.
    #[must_use]
    pub fn new(project_root: impl AsRef<Path>) -> Self {
        let database_path = project_root
            .as_ref()
            .join(".parchmint")
            .join("cache")
            .join("search.sqlite");
        let (sender, receiver) = mpsc::channel();
        let cancellation = Arc::new(Cancellation::default());
        let worker_cancellation = Arc::clone(&cancellation);
        let worker = thread::Builder::new()
            .name("parchmint-search-sqlite".into())
            .spawn(move || {
                Worker::new(database_path, worker_cancellation).run(receiver);
            })
            .expect("the search worker thread should start");
        let rebuild_status = Arc::new(Mutex::new(SearchRebuildStatus::Idle));

        Self {
            sender,
            worker: Mutex::new(Some(worker)),
            operation: Mutex::new(()),
            cancellation,
            rebuild_status,
            background_rebuild: Mutex::new(None),
            next_rebuild_generation: AtomicU64::new(FIRST_REBUILD_GENERATION),
        }
    }

    fn operation_guard(&self) -> Result<MutexGuard<'_, ()>, SearchError> {
        self.operation.lock().map_err(|_| SearchError::Storage {
            operation: "coordinate worker",
            reason: "operation lock was poisoned".into(),
        })
    }

    fn request<T>(
        &self,
        operation: &'static str,
        command: impl FnOnce(mpsc::SyncSender<Result<T, SearchError>>) -> Command,
    ) -> Result<T, SearchError> {
        let (reply, receiver) = mpsc::sync_channel(1);
        self.sender
            .send(command(reply))
            .map_err(|_| worker_stopped(operation))?;
        receiver.recv().map_err(|_| worker_stopped(operation))?
    }

    fn rebuild_from(
        &self,
        source: &dyn SearchProjectionSource,
    ) -> Result<RebuildReport, SearchError> {
        let (projection_sender, projection_receiver) = mpsc::sync_channel(REBUILD_BUFFER_SIZE);
        let (reply, reply_receiver) = mpsc::sync_channel(1);
        self.sender
            .send(Command::Rebuild {
                projections: projection_receiver,
                frontier: source.frontier_identity(),
                reply,
            })
            .map_err(|_| worker_stopped("start rebuild"))?;

        let mut visitor = RebuildVisitor {
            sender: &projection_sender,
        };
        let source_result = source.visit_projections(&mut visitor);
        let terminal = if source_result.is_ok() {
            RebuildInput::Finish
        } else {
            RebuildInput::Abort
        };
        let terminal_result = projection_sender.send(terminal);
        drop(projection_sender);
        let worker_result = reply_receiver
            .recv()
            .map_err(|_| worker_stopped("finish rebuild"))?;

        let worker_result = worker_result?;
        source_result?;
        terminal_result.map_err(|_| worker_stopped("finish rebuild"))?;
        worker_result.ok_or_else(|| SearchError::Storage {
            operation: "rebuild",
            reason: "worker aborted a completed projection stream".into(),
        })
    }

    fn background_status(&self) -> SearchRebuildStatus {
        self.rebuild_status.lock().map_or_else(
            |_| SearchRebuildStatus::Failed {
                generation: 0,
                reason: "search rebuild status is unavailable".into(),
            },
            |status| status.clone(),
        )
    }

    fn require_not_rebuilding(&self) -> Result<(), SearchError> {
        if let SearchRebuildStatus::Running { generation, .. } = self.background_status() {
            return Err(SearchError::Rebuilding { generation });
        }
        Ok(())
    }

    fn stop_background_rebuild(&self) {
        let task = self
            .background_rebuild
            .lock()
            .ok()
            .and_then(|mut task| task.take());
        if let Some(task) = task {
            self.cancellation.cancel(task.generation);
            let _ = task.worker.join();
        }
    }

    fn start_background_rebuild(
        &self,
        source: Arc<dyn SearchProjectionSource>,
        previous: SearchIndexProblem,
    ) -> Result<u64, SearchError> {
        let generation = self.next_rebuild_generation.fetch_add(1, Ordering::Relaxed);
        let frontier = source.frontier_identity();
        let (projection_sender, projection_receiver) = mpsc::sync_channel(REBUILD_BUFFER_SIZE);
        let (reply, reply_receiver) = mpsc::sync_channel(1);
        self.sender
            .send(Command::Rebuild {
                projections: projection_receiver,
                frontier,
                reply,
            })
            .map_err(|_| worker_stopped("start background rebuild"))?;
        *self
            .rebuild_status
            .lock()
            .map_err(|_| worker_stopped("record background rebuild"))? =
            SearchRebuildStatus::Running {
                generation,
                previous,
                processed_documents: 0,
            };
        let cancellation = Arc::clone(&self.cancellation);
        let status = Arc::clone(&self.rebuild_status);
        let worker = thread::Builder::new()
            .name("parchmint-search-rebuild-source".into())
            .spawn(move || {
                let mut visitor = BackgroundRebuildVisitor {
                    sender: &projection_sender,
                    cancellation: &cancellation,
                    generation,
                    status: &status,
                };
                let source_result = source.visit_projections(&mut visitor);
                let cancelled = cancellation.is_cancelled(generation);
                let terminal = if source_result.is_ok() && !cancelled {
                    RebuildInput::Finish
                } else {
                    RebuildInput::Abort
                };
                let terminal_result = projection_sender.send(terminal);
                drop(projection_sender);
                let worker_result = reply_receiver.recv();
                let next = if cancelled {
                    SearchRebuildStatus::Cancelled { generation }
                } else if let Err(error) = source_result {
                    SearchRebuildStatus::Failed {
                        generation,
                        reason: error.to_string(),
                    }
                } else if terminal_result.is_err() {
                    SearchRebuildStatus::Failed {
                        generation,
                        reason: "search rebuild worker stopped before publication".into(),
                    }
                } else {
                    match worker_result {
                        Ok(Ok(Some(report))) => SearchRebuildStatus::Complete {
                            generation,
                            indexed_documents: report.indexed_documents,
                        },
                        Ok(Ok(None)) => SearchRebuildStatus::Cancelled { generation },
                        Ok(Err(error)) => SearchRebuildStatus::Failed {
                            generation,
                            reason: error.to_string(),
                        },
                        Err(_) => SearchRebuildStatus::Failed {
                            generation,
                            reason: "search rebuild worker stopped before replying".into(),
                        },
                    }
                };
                if let Ok(mut current) = status.lock()
                    && matches!(
                        &*current,
                        SearchRebuildStatus::Running {
                            generation: active,
                            ..
                        } if *active == generation
                    )
                {
                    *current = next;
                }
            })
            .map_err(|error| SearchError::Storage {
                operation: "start background rebuild",
                reason: error.to_string(),
            })?;
        *self
            .background_rebuild
            .lock()
            .map_err(|_| worker_stopped("retain background rebuild"))? =
            Some(BackgroundRebuild { generation, worker });
        Ok(generation)
    }
}

struct BackgroundRebuildVisitor<'a> {
    sender: &'a mpsc::SyncSender<RebuildInput>,
    cancellation: &'a Cancellation,
    generation: u64,
    status: &'a Mutex<SearchRebuildStatus>,
}

impl SearchProjectionVisitor for BackgroundRebuildVisitor<'_> {
    fn visit(&mut self, projection: SearchDocumentProjection) -> Result<(), SearchError> {
        if self.cancellation.is_cancelled(self.generation) {
            return Err(SearchError::Rebuilding {
                generation: self.generation,
            });
        }
        projection.validate()?;
        self.sender
            .send(RebuildInput::Projection(projection))
            .map_err(|_| worker_stopped("stream background rebuild projection"))?;
        if let Ok(mut status) = self.status.lock()
            && let SearchRebuildStatus::Running {
                generation,
                processed_documents,
                ..
            } = &mut *status
            && *generation == self.generation
        {
            *processed_documents = processed_documents.saturating_add(1);
        }
        Ok(())
    }
}

impl SearchIndex for SqliteSearchIndex {
    fn open_or_rebuild(
        &self,
        project: ProjectId,
        source: &dyn SearchProjectionSource,
    ) -> Result<SearchIndexState, SearchError> {
        let _guard = self.operation_guard()?;
        self.stop_background_rebuild();
        let frontier = source.frontier_identity();
        let problem = self.request("open", |reply| Command::Open {
            project,
            frontier,
            reply,
        })?;
        if let Some(previous) = problem {
            self.rebuild_from(source)?;
            Ok(SearchIndexState::Rebuilt { previous })
        } else {
            Ok(SearchIndexState::Opened)
        }
    }

    fn open_or_rebuild_background(
        &self,
        project: ProjectId,
        source: Arc<dyn SearchProjectionSource>,
    ) -> Result<SearchIndexState, SearchError> {
        let _guard = self.operation_guard()?;
        self.stop_background_rebuild();
        let frontier = source.frontier_identity();
        let problem = self.request("open", |reply| Command::Open {
            project,
            frontier,
            reply,
        })?;
        let Some(previous) = problem else {
            if let Ok(mut status) = self.rebuild_status.lock() {
                *status = SearchRebuildStatus::Idle;
            }
            return Ok(SearchIndexState::Opened);
        };
        let generation = self.start_background_rebuild(source, previous)?;
        Ok(SearchIndexState::Rebuilding {
            previous,
            generation,
        })
    }

    fn rebuild_status(&self) -> SearchRebuildStatus {
        self.background_status()
    }

    fn replace_document(
        &self,
        projection: SearchDocumentProjection,
    ) -> Result<ProjectionReceipt, SearchError> {
        projection.validate()?;
        self.require_not_rebuilding()?;
        let _guard = self.operation_guard()?;
        self.request("replace document", |reply| Command::Replace {
            projection,
            reply,
        })
    }

    fn delete_document(
        &self,
        id: DocumentId,
        revision: RevisionId,
    ) -> Result<ProjectionReceipt, SearchError> {
        self.require_not_rebuilding()?;
        let _guard = self.operation_guard()?;
        self.request("delete document", |reply| Command::Delete {
            id,
            revision,
            reply,
        })
    }

    fn query(&self, query: SearchQuery, sink: Box<dyn SearchBatchSink>) -> Result<(), SearchError> {
        query.validate()?;
        self.require_not_rebuilding()?;
        let _guard = self.operation_guard()?;
        self.request("query", |reply| Command::Query { query, sink, reply })
    }

    fn cancel(&self, generation: u64) {
        self.cancellation.cancel(generation);
    }

    fn verify(&self) -> Result<SearchIntegrityReport, SearchError> {
        if let SearchRebuildStatus::Running { previous, .. } = self.background_status() {
            return Ok(SearchIntegrityReport {
                indexed_documents: 0,
                healthy: false,
                problem: Some(previous),
            });
        }
        let _guard = self.operation_guard()?;
        self.request("verify", |reply| Command::Verify { reply })
    }

    fn rebuild(&self, source: &dyn SearchProjectionSource) -> Result<RebuildReport, SearchError> {
        let _guard = self.operation_guard()?;
        self.stop_background_rebuild();
        self.rebuild_from(source)
    }
}

impl Drop for SqliteSearchIndex {
    fn drop(&mut self) {
        self.stop_background_rebuild();
        let _ = self.sender.send(Command::Shutdown);
        if let Ok(mut worker) = self.worker.lock()
            && let Some(worker) = worker.take()
        {
            let _ = worker.join();
        }
    }
}

enum Command {
    Open {
        project: ProjectId,
        frontier: Option<SearchFrontierId>,
        reply: Reply<Option<SearchIndexProblem>>,
    },
    Replace {
        projection: SearchDocumentProjection,
        reply: Reply<ProjectionReceipt>,
    },
    Delete {
        id: DocumentId,
        revision: RevisionId,
        reply: Reply<ProjectionReceipt>,
    },
    Query {
        query: SearchQuery,
        sink: Box<dyn SearchBatchSink>,
        reply: Reply<()>,
    },
    Verify {
        reply: Reply<SearchIntegrityReport>,
    },
    Rebuild {
        projections: mpsc::Receiver<RebuildInput>,
        frontier: Option<SearchFrontierId>,
        reply: Reply<Option<RebuildReport>>,
    },
    Shutdown,
}

type Reply<T> = mpsc::SyncSender<Result<T, SearchError>>;

enum RebuildInput {
    Projection(SearchDocumentProjection),
    Finish,
    Abort,
}

struct RebuildVisitor<'a> {
    sender: &'a mpsc::SyncSender<RebuildInput>,
}

impl SearchProjectionVisitor for RebuildVisitor<'_> {
    fn visit(&mut self, projection: SearchDocumentProjection) -> Result<(), SearchError> {
        projection.validate()?;
        self.sender
            .send(RebuildInput::Projection(projection))
            .map_err(|_| worker_stopped("stream rebuild projection"))
    }
}

#[derive(Default)]
struct Cancellation {
    state: Mutex<CancellationState>,
    delivery: Mutex<()>,
}

#[derive(Default)]
struct CancellationState {
    generations: BTreeSet<u64>,
    active: Option<(u64, InterruptHandle)>,
}

impl Cancellation {
    fn cancel(&self, generation: u64) {
        if let Ok(mut state) = self.state.lock() {
            state.generations.insert(generation);
            if let Some((active_generation, interrupt)) = state.active.as_ref()
                && *active_generation == generation
            {
                interrupt.interrupt();
            }
        }
        // Wait for any sink callback already in progress. Once this lock is
        // acquired, the generation is visible to every later delivery check.
        drop(self.delivery.lock());
    }

    fn start(&self, generation: u64, interrupt: InterruptHandle) -> bool {
        let Ok(mut state) = self.state.lock() else {
            return false;
        };
        if state.generations.contains(&generation) {
            return false;
        }
        state.active = Some((generation, interrupt));
        true
    }

    fn finish(&self, generation: u64) {
        if let Ok(mut state) = self.state.lock()
            && state
                .active
                .as_ref()
                .is_some_and(|(active, _)| *active == generation)
        {
            state.active = None;
        }
    }

    fn is_cancelled(&self, generation: u64) -> bool {
        self.state
            .lock()
            .map_or(true, |state| state.generations.contains(&generation))
    }

    fn deliver(&self, generation: u64, sink: &dyn SearchBatchSink, batch: SearchBatch) -> bool {
        let Ok(_delivery) = self.delivery.lock() else {
            return false;
        };
        let Ok(state) = self.state.lock() else {
            return false;
        };
        if state.generations.contains(&generation) {
            return false;
        }
        drop(state);
        sink.push(batch);
        true
    }
}

struct Worker {
    database_path: PathBuf,
    database: Option<ProjectDatabase>,
    cancellation: Arc<Cancellation>,
}

impl Worker {
    fn new(database_path: PathBuf, cancellation: Arc<Cancellation>) -> Self {
        Self {
            database_path,
            database: None,
            cancellation,
        }
    }

    fn run(mut self, receiver: mpsc::Receiver<Command>) {
        while let Ok(command) = receiver.recv() {
            match command {
                Command::Open {
                    project,
                    frontier,
                    reply,
                } => {
                    send_reply(reply, self.open(project, frontier));
                }
                Command::Replace { projection, reply } => {
                    send_reply(reply, self.replace_document(&projection));
                }
                Command::Delete {
                    id,
                    revision,
                    reply,
                } => send_reply(reply, self.delete_document(id, revision)),
                Command::Query { query, sink, reply } => {
                    send_reply(reply, self.query(&query, sink.as_ref()));
                }
                Command::Verify { reply } => send_reply(reply, self.verify()),
                Command::Rebuild {
                    projections,
                    frontier,
                    reply,
                } => {
                    send_reply(reply, self.rebuild(&projections, frontier));
                }
                Command::Shutdown => break,
            }
        }
    }

    fn open(
        &mut self,
        project: ProjectId,
        frontier: Option<SearchFrontierId>,
    ) -> Result<Option<SearchIndexProblem>, SearchError> {
        self.database = None;

        let existed = self.database_path.is_file();
        if !existed {
            let database = self.create_database(project, frontier)?;
            self.install_database(project, frontier, database);
            return Ok(Some(SearchIndexProblem::Missing));
        }

        let connection = match open_connection(&self.database_path) {
            Ok(connection) => connection,
            Err(error) => {
                let Some(problem) = sqlite_problem(&error) else {
                    return Err(sqlite_error("open", error));
                };
                let database = self.recreate_database(project, frontier)?;
                self.install_database(project, frontier, database);
                return Ok(Some(problem));
            }
        };
        let problem = match inspect_connection(&connection, project, frontier) {
            Ok(None) if frontier.is_none() => SearchIndexProblem::Incompatible,
            Ok(None) => {
                self.install_database(project, frontier, connection);
                return Ok(None);
            }
            Ok(Some(problem)) => problem,
            Err(error) => sqlite_problem(&error).ok_or_else(|| sqlite_error("inspect", error))?,
        };
        drop(connection);
        let database = self.recreate_database(project, frontier)?;
        self.install_database(project, frontier, database);
        Ok(Some(problem))
    }

    fn install_database(
        &mut self,
        project: ProjectId,
        frontier: Option<SearchFrontierId>,
        connection: Connection,
    ) {
        self.database = Some(ProjectDatabase {
            project,
            frontier,
            connection,
        });
    }

    fn create_database(
        &self,
        project: ProjectId,
        frontier: Option<SearchFrontierId>,
    ) -> Result<Connection, SearchError> {
        let parent = self
            .database_path
            .parent()
            .ok_or_else(|| SearchError::Storage {
                operation: "create cache directory",
                reason: "search database path has no parent".into(),
            })?;
        fs::create_dir_all(parent).map_err(|error| io_error("create cache directory", error))?;
        let mut connection =
            open_connection(&self.database_path).map_err(|error| sqlite_error("create", error))?;
        let transaction = connection
            .transaction()
            .map_err(|error| sqlite_error("create schema", error))?;
        transaction
            .execute_batch(SCHEMA)
            .map_err(|error| sqlite_error("create schema", error))?;
        transaction
            .execute(
                "INSERT INTO index_metadata(project_id, frontier_identity, rebuild_complete)
                 VALUES (?1, ?2, 0)",
                params![
                    project.as_bytes().as_slice(),
                    frontier.map(|identity| identity.as_bytes().to_vec())
                ],
            )
            .map_err(|error| sqlite_error("record project identity", error))?;
        transaction
            .commit()
            .map_err(|error| sqlite_error("commit schema", error))?;
        Ok(connection)
    }

    fn recreate_database(
        &self,
        project: ProjectId,
        frontier: Option<SearchFrontierId>,
    ) -> Result<Connection, SearchError> {
        remove_database_files(&self.database_path)?;
        self.create_database(project, frontier)
    }

    fn database_mut(
        &mut self,
        operation: &'static str,
    ) -> Result<&mut ProjectDatabase, SearchError> {
        self.database.as_mut().ok_or_else(|| SearchError::Storage {
            operation,
            reason: "search index is not open".into(),
        })
    }

    fn replace_document(
        &mut self,
        projection: &SearchDocumentProjection,
    ) -> Result<ProjectionReceipt, SearchError> {
        let database = self.database_mut("replace document")?;
        let transaction = database
            .connection
            .transaction()
            .map_err(|error| sqlite_error("begin document replacement", error))?;
        let receipt = replace_in_transaction(&transaction, projection)?;
        if receipt.replaced {
            invalidate_frontier(&transaction)?;
        }
        transaction
            .commit()
            .map_err(|error| sqlite_error("commit document replacement", error))?;
        if receipt.replaced {
            database.frontier = None;
        }
        Ok(receipt)
    }

    fn delete_document(
        &mut self,
        id: DocumentId,
        revision: RevisionId,
    ) -> Result<ProjectionReceipt, SearchError> {
        let database = self.database_mut("delete document")?;
        let transaction = database
            .connection
            .transaction()
            .map_err(|error| sqlite_error("begin document deletion", error))?;
        if let Some((indexed_revision, deleted)) = current_document(&transaction, id)?
            && (indexed_revision > revision || (deleted && indexed_revision == revision))
        {
            return Ok(ProjectionReceipt {
                document_id: id,
                indexed_revision,
                replaced: false,
            });
        }
        transaction
            .execute(
                "DELETE FROM search_content WHERE document_id = ?1",
                params![id.as_bytes().as_slice()],
            )
            .map_err(|error| sqlite_error("delete indexed text", error))?;
        transaction
            .execute(
                "INSERT INTO documents(document_id, revision, deleted) VALUES (?1, ?2, 1)
                 ON CONFLICT(document_id) DO UPDATE SET revision = excluded.revision, deleted = 1",
                params![
                    id.as_bytes().as_slice(),
                    revision_bytes(revision).as_slice()
                ],
            )
            .map_err(|error| sqlite_error("record document deletion", error))?;
        invalidate_frontier(&transaction)?;
        transaction
            .commit()
            .map_err(|error| sqlite_error("commit document deletion", error))?;
        database.frontier = None;
        Ok(ProjectionReceipt {
            document_id: id,
            indexed_revision: revision,
            replaced: true,
        })
    }

    fn query(
        &mut self,
        query: &SearchQuery,
        sink: &dyn SearchBatchSink,
    ) -> Result<(), SearchError> {
        let cancellation = Arc::clone(&self.cancellation);
        let database = self.database_mut("query")?;
        if !cancellation.start(query.generation, database.connection.get_interrupt_handle()) {
            return Ok(());
        }
        let result = execute_query(&database.connection, query, sink, &cancellation);
        cancellation.finish(query.generation);
        result
    }

    fn verify(&mut self) -> Result<SearchIntegrityReport, SearchError> {
        let database = self.database_mut("verify")?;
        match inspect_connection(&database.connection, database.project, database.frontier) {
            Ok(None) => Ok(SearchIntegrityReport {
                indexed_documents: indexed_document_count(&database.connection)?,
                healthy: true,
                problem: None,
            }),
            Ok(Some(problem)) => Ok(unhealthy_report(&database.connection, problem)),
            Err(error) => sqlite_problem(&error).map_or_else(
                || Err(sqlite_error("verify", error)),
                |problem| Ok(unhealthy_report(&database.connection, problem)),
            ),
        }
    }

    fn rebuild(
        &mut self,
        projections: &mpsc::Receiver<RebuildInput>,
        frontier: Option<SearchFrontierId>,
    ) -> Result<Option<RebuildReport>, SearchError> {
        let (project, inspection) = {
            let database = self.database_mut("rebuild")?;
            (
                database.project,
                inspect_connection(&database.connection, database.project, frontier),
            )
        };
        let needs_recreation = match inspection {
            Ok(problem) => problem.is_some(),
            Err(error) if sqlite_problem(&error).is_some() => true,
            Err(error) => return Err(sqlite_error("inspect before rebuild", error)),
        };
        if needs_recreation {
            self.database = None;
            let connection = self.recreate_database(project, frontier)?;
            self.install_database(project, frontier, connection);
        }

        let database = self.database_mut("rebuild")?;
        let transaction = database
            .connection
            .transaction()
            .map_err(|error| sqlite_error("begin rebuild", error))?;
        transaction
            .execute("DELETE FROM documents", [])
            .map_err(|error| sqlite_error("clear index for rebuild", error))?;
        loop {
            match projections.recv() {
                Ok(RebuildInput::Projection(projection)) => {
                    replace_in_transaction(&transaction, &projection)?;
                }
                Ok(RebuildInput::Finish) => {
                    let indexed_documents = indexed_document_count(&transaction)?;
                    transaction
                        .execute(
                            "UPDATE index_metadata
                             SET frontier_identity = ?1, rebuild_complete = 1",
                            params![frontier.map(|identity| identity.as_bytes().to_vec())],
                        )
                        .map_err(|error| sqlite_error("complete rebuild", error))?;
                    transaction
                        .commit()
                        .map_err(|error| sqlite_error("commit rebuild", error))?;
                    database.frontier = frontier;
                    return Ok(Some(RebuildReport { indexed_documents }));
                }
                Ok(RebuildInput::Abort) | Err(_) => return Ok(None),
            }
        }
    }
}

fn invalidate_frontier(transaction: &Transaction<'_>) -> Result<(), SearchError> {
    transaction
        .execute("UPDATE index_metadata SET frontier_identity = NULL", [])
        .map_err(|error| sqlite_error("invalidate search frontier", error))?;
    Ok(())
}

struct ProjectDatabase {
    project: ProjectId,
    frontier: Option<SearchFrontierId>,
    connection: Connection,
}

fn unhealthy_report(connection: &Connection, problem: SearchIndexProblem) -> SearchIntegrityReport {
    SearchIntegrityReport {
        indexed_documents: indexed_document_count(connection).unwrap_or(0),
        healthy: false,
        problem: Some(problem),
    }
}

fn open_connection(path: &Path) -> rusqlite::Result<Connection> {
    let connection = Connection::open(path)?;
    connection.busy_timeout(Duration::from_secs(5))?;
    connection.execute_batch(
        "PRAGMA foreign_keys = ON;
         PRAGMA journal_mode = DELETE;
         PRAGMA synchronous = NORMAL;",
    )?;
    Ok(connection)
}

fn inspect_connection(
    connection: &Connection,
    project: ProjectId,
    expected_frontier: Option<SearchFrontierId>,
) -> rusqlite::Result<Option<SearchIndexProblem>> {
    type StoredIndexMetadata = (i64, Option<Vec<u8>>, Option<Vec<u8>>, Option<i64>);
    let application_id: i32 =
        connection.query_row("PRAGMA application_id", [], |row| row.get(0))?;
    let schema_version: i32 = connection.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if application_id != APPLICATION_ID || schema_version != SCHEMA_VERSION {
        return Ok(Some(SearchIndexProblem::Incompatible));
    }

    let object_count: i64 = match connection.query_row(
        "SELECT count(*) FROM sqlite_schema
         WHERE name IN (
            'index_metadata', 'documents', 'search_content',
            'search_content_source_unit', 'search_fts',
            'search_content_insert', 'search_content_delete', 'search_content_update'
         )",
        [],
        |row| row.get(0),
    ) {
        Ok(count) => count,
        Err(error) => return inspection_schema_error(error),
    };
    if object_count != 8 {
        return Ok(Some(SearchIndexProblem::Incompatible));
    }

    let (metadata_count, stored_project, stored_frontier, rebuild_complete): StoredIndexMetadata =
        match connection.query_row(
            "SELECT count(*), min(project_id), min(frontier_identity), min(rebuild_complete)
             FROM index_metadata",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        ) {
            Ok(metadata) => metadata,
            Err(error) => return inspection_schema_error(error),
        };
    if metadata_count != 1
        || stored_project.as_deref() != Some(project.as_bytes().as_slice())
        || stored_frontier != expected_frontier.map(|identity| identity.as_bytes().to_vec())
        || rebuild_complete != Some(1)
    {
        return Ok(Some(SearchIndexProblem::Incompatible));
    }

    let quick_check: String =
        connection.query_row("PRAGMA quick_check(1)", [], |row| row.get(0))?;
    if quick_check != "ok" {
        return Ok(Some(SearchIndexProblem::Corrupt));
    }
    if let Err(error) = connection.execute(
        "INSERT INTO search_fts(search_fts, rank) VALUES ('integrity-check', 1)",
        [],
    ) {
        return inspection_schema_error(error);
    }
    Ok(None)
}

fn inspection_schema_error(error: rusqlite::Error) -> rusqlite::Result<Option<SearchIndexProblem>> {
    if sqlite_problem(&error).is_some() {
        Err(error)
    } else {
        Ok(Some(SearchIndexProblem::Incompatible))
    }
}

fn replace_in_transaction(
    transaction: &Transaction<'_>,
    projection: &SearchDocumentProjection,
) -> Result<ProjectionReceipt, SearchError> {
    if let Some((indexed_revision, deleted)) =
        current_document(transaction, projection.document_id)?
        && (indexed_revision > projection.revision
            || (deleted && indexed_revision == projection.revision)
            || (!deleted
                && indexed_revision == projection.revision
                && projection_matches(transaction, projection)?))
    {
        return Ok(ProjectionReceipt {
            document_id: projection.document_id,
            indexed_revision,
            replaced: false,
        });
    }

    transaction
        .execute(
            "DELETE FROM search_content WHERE document_id = ?1",
            params![projection.document_id.as_bytes().as_slice()],
        )
        .map_err(|error| sqlite_error("remove previous indexed text", error))?;
    transaction
        .execute(
            "INSERT INTO documents(document_id, revision, deleted) VALUES (?1, ?2, 0)
             ON CONFLICT(document_id) DO UPDATE SET revision = excluded.revision, deleted = 0",
            params![
                projection.document_id.as_bytes().as_slice(),
                revision_bytes(projection.revision).as_slice()
            ],
        )
        .map_err(|error| sqlite_error("record indexed revision", error))?;
    for text in &projection.texts {
        insert_text(transaction, projection.document_id, text)?;
    }
    Ok(ProjectionReceipt {
        document_id: projection.document_id,
        indexed_revision: projection.revision,
        replaced: true,
    })
}

fn projection_matches(
    transaction: &Transaction<'_>,
    projection: &SearchDocumentProjection,
) -> Result<bool, SearchError> {
    let count: i64 = transaction
        .query_row(
            "SELECT count(*) FROM search_content WHERE document_id = ?1",
            params![projection.document_id.as_bytes().as_slice()],
            |row| row.get(0),
        )
        .map_err(|error| sqlite_error("compare indexed projection", error))?;
    if usize::try_from(count).ok() != Some(projection.texts.len()) {
        return Ok(false);
    }
    for text in &projection.texts {
        let (field_kind, metadata): (i64, Option<&[u8]>) = match &text.field {
            SearchField::Body => (0, None),
            SearchField::DisplayTitle => (1, None),
            SearchField::Synopsis => (2, None),
            SearchField::Metadata(id) => (3, Some(id.as_bytes().as_slice())),
        };
        let found: i64 = transaction
            .query_row(
                "SELECT EXISTS(
                    SELECT 1 FROM search_content
                    WHERE document_id = ?1 AND block_id = ?2 AND field_kind = ?3
                      AND metadata_field_id IS ?4 AND text = ?5
                )",
                params![
                    projection.document_id.as_bytes().as_slice(),
                    text.block_id.as_bytes().as_slice(),
                    field_kind,
                    metadata,
                    text.text,
                ],
                |row| row.get(0),
            )
            .map_err(|error| sqlite_error("compare indexed projection", error))?;
        if found == 0 {
            return Ok(false);
        }
    }
    Ok(true)
}

fn insert_text(
    transaction: &Transaction<'_>,
    document: DocumentId,
    projection: &SearchTextProjection,
) -> Result<(), SearchError> {
    let (field_kind, metadata): (i64, Option<&[u8]>) = match &projection.field {
        SearchField::Body => (0, None),
        SearchField::DisplayTitle => (1, None),
        SearchField::Synopsis => (2, None),
        SearchField::Metadata(id) => (3, Some(id.as_bytes().as_slice())),
    };
    transaction
        .execute(
            "INSERT INTO search_content(
                document_id, block_id, field_kind, metadata_field_id, text
             ) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                document.as_bytes().as_slice(),
                projection.block_id.as_bytes().as_slice(),
                field_kind,
                metadata,
                projection.text
            ],
        )
        .map_err(|error| sqlite_error("insert indexed text", error))?;
    Ok(())
}

fn current_document(
    transaction: &Transaction<'_>,
    document: DocumentId,
) -> Result<Option<(RevisionId, bool)>, SearchError> {
    let stored: Option<(Vec<u8>, bool)> = transaction
        .query_row(
            "SELECT revision, deleted FROM documents WHERE document_id = ?1",
            params![document.as_bytes().as_slice()],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .map_err(|error| sqlite_error("read indexed revision", error))?;
    stored
        .map(|(bytes, deleted)| decode_revision(&bytes).map(|revision| (revision, deleted)))
        .transpose()
        .map_err(|reason| SearchError::Storage {
            operation: "read indexed revision",
            reason,
        })
}

fn execute_query(
    connection: &Connection,
    query: &SearchQuery,
    sink: &dyn SearchBatchSink,
    cancellation: &Cancellation,
) -> Result<(), SearchError> {
    // unicode61 can narrow whole-word candidates, but its token index cannot
    // safely represent arbitrary substrings inside a token.
    let fts_query = query
        .whole_word
        .then(|| fts_literal_query(&query.text))
        .flatten();
    let sql = if fts_query.is_some() {
        "SELECT c.document_id, c.block_id, c.field_kind, c.metadata_field_id,
                d.revision, c.text
         FROM search_fts AS f
         JOIN search_content AS c ON c.row_id = f.rowid
         JOIN documents AS d ON d.document_id = c.document_id
         WHERE search_fts MATCH ?1 AND d.deleted = 0
         ORDER BY c.document_id, c.row_id"
    } else {
        "SELECT c.document_id, c.block_id, c.field_kind, c.metadata_field_id,
                d.revision, c.text
         FROM search_content AS c
         JOIN documents AS d ON d.document_id = c.document_id
         WHERE ?1 IS NOT NULL AND d.deleted = 0
         ORDER BY c.document_id, c.row_id"
    };
    let parameter = fts_query.as_deref().unwrap_or(&query.text);
    let mut statement = connection
        .prepare(sql)
        .map_err(|error| sqlite_error("prepare query", error))?;
    let mut rows = statement
        .query(params![parameter])
        .map_err(|error| sqlite_error("start query", error))?;
    let mut hits = Vec::with_capacity(RESULT_BATCH_SIZE);

    loop {
        if cancellation.is_cancelled(query.generation) {
            return Ok(());
        }
        let row = match rows.next() {
            Ok(row) => row,
            Err(error) if is_interrupted(&error) && cancellation.is_cancelled(query.generation) => {
                return Ok(());
            }
            Err(error) => return Err(sqlite_error("read query results", error)),
        };
        let Some(row) = row else {
            break;
        };
        let field = decode_field(
            row.get(2)
                .map_err(|error| sqlite_error("read field", error))?,
            row.get(3)
                .map_err(|error| sqlite_error("read metadata field", error))?,
        )?;
        if !query.fields.contains(&field) {
            continue;
        }
        let document = decode_document(
            &row.get::<_, Vec<u8>>(0)
                .map_err(|error| sqlite_error("read document identity", error))?,
        )?;
        let block = decode_block(
            &row.get::<_, Vec<u8>>(1)
                .map_err(|error| sqlite_error("read block identity", error))?,
        )?;
        let revision = decode_revision(
            &row.get::<_, Vec<u8>>(4)
                .map_err(|error| sqlite_error("read revision", error))?,
        )
        .map_err(|reason| SearchError::Storage {
            operation: "read query revision",
            reason,
        })?;
        let text: String = row
            .get(5)
            .map_err(|error| sqlite_error("read indexed text", error))?;

        let delivered_all =
            visit_literal_ranges(&text, &query.text, query.case_sensitive, |range| {
                if query.whole_word && !is_whole_word(&text, range) {
                    return true;
                }
                hits.push(SearchHit {
                    document_id: document,
                    block_id: block,
                    indexed_revision: revision,
                    field,
                    candidate_range: range,
                    snippet: make_snippet(&text, range),
                });
                if hits.len() == RESULT_BATCH_SIZE
                    && !deliver_batch(query.generation, &mut hits, false, sink, cancellation)
                {
                    return false;
                }
                true
            });
        if !delivered_all {
            return Ok(());
        }
    }

    let _ = deliver_batch(query.generation, &mut hits, true, sink, cancellation);
    Ok(())
}

fn deliver_batch(
    generation: u64,
    hits: &mut Vec<SearchHit>,
    finished: bool,
    sink: &dyn SearchBatchSink,
    cancellation: &Cancellation,
) -> bool {
    cancellation.deliver(
        generation,
        sink,
        SearchBatch {
            generation,
            hits: std::mem::take(hits),
            finished,
        },
    )
}

fn visit_literal_ranges(
    text: &str,
    needle: &str,
    case_sensitive: bool,
    mut visitor: impl FnMut(TextRange) -> bool,
) -> bool {
    if case_sensitive {
        for (start, matched) in text.match_indices(needle) {
            let Some(range) = TextRange::new(start, start + matched.len()) else {
                continue;
            };
            if !visitor(range) {
                return false;
            }
        }
        return true;
    }

    let folded_needle: String = needle.chars().flat_map(char::to_lowercase).collect();
    let mut folded_text = String::new();
    let mut characters = Vec::new();
    for (original_start, character) in text.char_indices() {
        let folded_start = folded_text.len();
        folded_text.extend(character.to_lowercase());
        characters.push((
            folded_start,
            folded_text.len(),
            original_start,
            original_start + character.len_utf8(),
        ));
    }

    for (folded_start, matched) in folded_text.match_indices(&folded_needle) {
        let folded_end = folded_start + matched.len();
        let Ok(start_index) =
            characters.binary_search_by_key(&folded_start, |character| character.0)
        else {
            continue;
        };
        let Ok(end_index) = characters.binary_search_by_key(&folded_end, |character| character.1)
        else {
            continue;
        };
        let Some(range) = TextRange::new(characters[start_index].2, characters[end_index].3) else {
            continue;
        };
        if !visitor(range) {
            return false;
        }
    }
    true
}

fn is_whole_word(text: &str, range: TextRange) -> bool {
    let before = text[..range.start()].chars().next_back();
    let after = text[range.end()..].chars().next();
    before.is_none_or(|character| !is_word_character(character))
        && after.is_none_or(|character| !is_word_character(character))
}

fn is_word_character(character: char) -> bool {
    character.is_alphanumeric() || character == '_'
}

fn make_snippet(text: &str, range: TextRange) -> SearchSnippet {
    const CONTEXT_CHARACTERS: usize = 48;
    let start = text[..range.start()]
        .char_indices()
        .rev()
        .nth(CONTEXT_CHARACTERS)
        .map_or(0, |(offset, _)| offset);
    let end = text[range.end()..]
        .char_indices()
        .nth(CONTEXT_CHARACTERS)
        .map_or(text.len(), |(offset, _)| range.end() + offset);
    SearchSnippet {
        text: text[start..end].to_owned(),
        match_range: TextRange::new(range.start() - start, range.end() - start)
            .expect("a snippet range is ordered"),
    }
}

fn decode_field(kind: i64, metadata: Option<Vec<u8>>) -> Result<SearchField, SearchError> {
    match (kind, metadata) {
        (0, None) => Ok(SearchField::Body),
        (1, None) => Ok(SearchField::DisplayTitle),
        (2, None) => Ok(SearchField::Synopsis),
        (3, Some(bytes)) => decode_id(&bytes, "metadata field").map(SearchField::Metadata),
        _ => Err(SearchError::Storage {
            operation: "read field",
            reason: "index contains an invalid field encoding".into(),
        }),
    }
}

fn decode_document(bytes: &[u8]) -> Result<DocumentId, SearchError> {
    decode_id(bytes, "document")
}

fn decode_block(bytes: &[u8]) -> Result<BlockId, SearchError> {
    decode_id(bytes, "block")
}

fn decode_id<T>(bytes: &[u8], name: &'static str) -> Result<T, SearchError>
where
    T: FromBytes,
{
    let bytes: [u8; 16] = bytes.try_into().map_err(|_| SearchError::Storage {
        operation: "decode identity",
        reason: format!("index contains an invalid {name} identity"),
    })?;
    Ok(T::from_bytes(bytes))
}

trait FromBytes {
    fn from_bytes(bytes: [u8; 16]) -> Self;
}

impl FromBytes for DocumentId {
    fn from_bytes(bytes: [u8; 16]) -> Self {
        Self::from_bytes(bytes)
    }
}

impl FromBytes for BlockId {
    fn from_bytes(bytes: [u8; 16]) -> Self {
        Self::from_bytes(bytes)
    }
}

impl FromBytes for MetadataFieldId {
    fn from_bytes(bytes: [u8; 16]) -> Self {
        Self::from_bytes(bytes)
    }
}

fn revision_bytes(revision: RevisionId) -> [u8; 8] {
    revision.value().to_be_bytes()
}

fn decode_revision(bytes: &[u8]) -> Result<RevisionId, String> {
    let bytes: [u8; 8] = bytes
        .try_into()
        .map_err(|_| "index contains an invalid revision".to_owned())?;
    Ok(RevisionId::from(u64::from_be_bytes(bytes)))
}

fn indexed_document_count(connection: &Connection) -> Result<usize, SearchError> {
    let count: i64 = connection
        .query_row(
            "SELECT count(*) FROM documents WHERE deleted = 0",
            [],
            |row| row.get(0),
        )
        .map_err(|error| sqlite_error("count indexed documents", error))?;
    usize::try_from(count).map_err(|error| SearchError::Storage {
        operation: "count indexed documents",
        reason: error.to_string(),
    })
}

fn fts_literal_query(text: &str) -> Option<String> {
    text.chars()
        .any(is_word_character)
        .then(|| format!("\"{}\"", text.replace('"', "\"\"")))
}

fn remove_database_files(path: &Path) -> Result<(), SearchError> {
    for candidate in database_files(path) {
        match fs::remove_file(candidate) {
            Ok(()) => {}
            Err(error) if error.kind() == ErrorKind::NotFound => {}
            Err(error) => return Err(io_error("remove invalid cache", error)),
        }
    }
    Ok(())
}

fn database_files(path: &Path) -> [PathBuf; 4] {
    let with_suffix = |suffix: &str| {
        let mut value = path.as_os_str().to_os_string();
        value.push(suffix);
        PathBuf::from(value)
    };
    [
        path.to_path_buf(),
        with_suffix("-wal"),
        with_suffix("-shm"),
        with_suffix("-journal"),
    ]
}

fn sqlite_problem(error: &rusqlite::Error) -> Option<SearchIndexProblem> {
    match error {
        rusqlite::Error::SqliteFailure(inner, _)
            if matches!(
                inner.code,
                ErrorCode::DatabaseCorrupt | ErrorCode::NotADatabase
            ) =>
        {
            Some(SearchIndexProblem::Corrupt)
        }
        _ => None,
    }
}

fn is_interrupted(error: &rusqlite::Error) -> bool {
    matches!(
        error,
        rusqlite::Error::SqliteFailure(inner, _) if inner.code == ErrorCode::OperationInterrupted
    )
}

fn send_reply<T>(reply: Reply<T>, result: Result<T, SearchError>) {
    let _ = reply.send(result);
}

fn worker_stopped(operation: &'static str) -> SearchError {
    SearchError::Storage {
        operation,
        reason: "search worker stopped unexpectedly".into(),
    }
}

fn sqlite_error(operation: &'static str, error: rusqlite::Error) -> SearchError {
    SearchError::Storage {
        operation,
        reason: error.to_string(),
    }
}

fn io_error(operation: &'static str, error: std::io::Error) -> SearchError {
    SearchError::Storage {
        operation,
        reason: error.to_string(),
    }
}
