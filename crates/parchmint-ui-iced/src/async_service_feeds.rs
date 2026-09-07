//! Background service controllers for project UI effects.
//!
//! These helpers are deliberately independent of Iced widgets. Synchronous
//! service calls are packaged as [`BlockingServiceJob`] values; the native UI
//! must run those jobs on a blocking worker and deliver the owned results back
//! to its reducer. Every job reacquires [`ProjectUiPorts::access`] when it runs.

#[path = "history_project.rs"]
mod history_project;

use parchmint_domain::encode_stable_id as encode_hex;
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
        mpsc::{self, Receiver, Sender},
    },
};

use parchmint_domain::{DocumentId, MetadataFieldId};
use parchmint_editor_api::CanonicalDocumentLoad;
use parchmint_editor_core::EditorCoreSession;
use parchmint_history_api::{
    CheckpointCategory, CheckpointId, CheckpointResource, CheckpointSummary, HistoryCursor,
    HistoryPage, HistoryPageQuery, SnapshotResourcePaths,
};
use parchmint_project_format::CanonicalRelativePath;
use parchmint_search_api::{SearchBatch, SearchBatchSink, SearchField, SearchHit, SearchQuery};
use parchmint_ui_api::{
    ProjectRecoveryAcceptance, ProjectRecoveryState, ProjectSaveKind, ProjectSnapshot,
    ProjectUiPorts,
};

use crate::{
    GlobalSearchResult, HistoryCheckpointCategory, HistoryCheckpointRow, HistoryComparison,
    HistoryCurrentDocument, HistoryDocumentPreview, HistoryPreviewData, ProjectTaskPayload,
    compare_history_documents,
};

/// A boxed operation that may call blocking service traits.
///
/// Constructing a job is update-loop safe. Calling [`Self::run`] is not: the
/// native integration must run it on a blocking worker.
#[must_use = "run the service job on a blocking worker"]
pub struct BlockingServiceJob<T> {
    operation: &'static str,
    run: Box<dyn FnOnce() -> Result<T, ServiceFeedError> + Send + 'static>,
}

impl<T> fmt::Debug for BlockingServiceJob<T> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("BlockingServiceJob")
            .field("operation", &self.operation)
            .finish()
    }
}

impl<T> BlockingServiceJob<T> {
    fn new(
        operation: &'static str,
        run: impl FnOnce() -> Result<T, ServiceFeedError> + Send + 'static,
    ) -> Self {
        Self {
            operation,
            run: Box::new(run),
        }
    }

    pub const fn operation(&self) -> &'static str {
        self.operation
    }

    pub fn run(self) -> Result<T, ServiceFeedError> {
        (self.run)()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServiceKind {
    Search,
    History,
    Recovery,
    ProjectQuery,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServiceFeedError {
    StaleSession {
        session_id: u64,
        generation: u64,
    },
    StaleSearchGeneration {
        expected: Option<u64>,
        received: u64,
    },
    InvalidIdentifier {
        kind: &'static str,
        value: String,
    },
    InvalidServiceData {
        service: ServiceKind,
        reason: String,
    },
    Service {
        service: ServiceKind,
        message: String,
    },
    NoRecoveryToAccept,
    InvalidState {
        operation: &'static str,
        reason: &'static str,
    },
}

impl fmt::Display for ServiceFeedError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::StaleSession {
                session_id,
                generation,
            } => write!(
                formatter,
                "project session {session_id} generation {generation} is stale"
            ),
            Self::StaleSearchGeneration { expected, received } => write!(
                formatter,
                "search generation {received} is stale; current generation is {expected:?}"
            ),
            Self::InvalidIdentifier { kind, value } => {
                write!(formatter, "invalid {kind} identifier {value:?}")
            }
            Self::InvalidServiceData { service, reason } => {
                write!(formatter, "invalid {service:?} service data: {reason}")
            }
            Self::Service { service, message } => {
                write!(formatter, "{service:?} service failed: {message}")
            }
            Self::NoRecoveryToAccept => {
                formatter.write_str("there is no reconciled recovery to accept")
            }
            Self::InvalidState { operation, reason } => {
                write!(formatter, "invalid {operation} state: {reason}")
            }
        }
    }
}

impl std::error::Error for ServiceFeedError {}

/// The single project-service entry point retained by a native UI session.
#[derive(Clone)]
pub struct AsyncServiceFeeds {
    ports: Arc<dyn ServiceFeedPorts>,
    search: SearchFeedController,
    next_recovery: Arc<AtomicU64>,
}

impl fmt::Debug for AsyncServiceFeeds {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AsyncServiceFeeds")
            .finish_non_exhaustive()
    }
}

impl AsyncServiceFeeds {
    pub fn new(ports: ProjectUiPorts) -> Self {
        let ports: Arc<dyn ServiceFeedPorts> = Arc::new(ProjectUiPortAdapter::new(ports));
        Self::from_ports(ports)
    }

    fn from_ports(ports: Arc<dyn ServiceFeedPorts>) -> Self {
        Self {
            search: SearchFeedController::new(ports.clone()),
            ports,
            next_recovery: Arc::new(AtomicU64::new(0)),
        }
    }

    pub fn search(&self) -> &SearchFeedController {
        &self.search
    }

    pub fn history_list(
        &self,
        cursor: Option<HistoryCursor>,
        limit: usize,
        affected_document: Option<DocumentId>,
    ) -> BlockingServiceJob<HistoryListResult> {
        let ports = self.ports.clone();
        BlockingServiceJob::new("load History", move || {
            let page = ports.history_list(HistoryPageQuery {
                cursor,
                limit,
                affected_document,
            })?;
            Ok(HistoryListResult::from_page(page))
        })
    }

    pub fn history_preview(
        &self,
        checkpoint_id: impl Into<String>,
        current_document: Option<HistoryCurrentDocument>,
    ) -> BlockingServiceJob<HistoryPreviewResult> {
        let ports = self.ports.clone();
        let checkpoint_id = checkpoint_id.into();
        BlockingServiceJob::new("preview History", move || {
            let checkpoint = parse_stable_id(&checkpoint_id, "History checkpoint")?;
            let checkpoint = CheckpointId::from_bytes(checkpoint);
            let preview = ports.history_preview(checkpoint)?;
            let document = current_document
                .as_ref()
                .map(|current| {
                    let document = &current.document_id;
                    let document =
                        DocumentId::from_bytes(parse_stable_id(document, "History document")?);
                    load_checkpoint_document(ports.as_ref(), checkpoint, &preview, document)
                })
                .transpose()?
                .flatten();
            let comparison = document
                .as_ref()
                .zip(current_document.as_ref())
                .filter(|(before, after)| before.document_id == after.document_id)
                .map(|(before, after)| compare_history_documents(&checkpoint_id, before, after));
            Ok(HistoryPreviewResult::from_preview(
                preview,
                document,
                current_document,
                comparison,
            ))
        })
    }

    pub fn history_project_preview(
        &self,
        checkpoint_id: String,
        current_document: Option<HistoryCurrentDocument>,
        drafts: Vec<parchmint_editor_api::CanonicalProjection>,
    ) -> BlockingServiceJob<HistoryPreviewResult> {
        let ports = self.ports.clone();
        let focused = self.history_preview(checkpoint_id.clone(), current_document);
        BlockingServiceJob::new("compare project History", move || {
            let mut result = focused.run()?;
            let checkpoint =
                CheckpointId::from_bytes(parse_stable_id(&checkpoint_id, "History checkpoint")?);
            let preview = ports.history_preview(checkpoint)?;
            let current = ports.snapshot_with_documents()?;
            result.project_changes = Some(history_project::compare(
                ports.as_ref(),
                checkpoint,
                &preview,
                current,
                drafts,
            )?);
            Ok(result)
        })
    }

    pub fn deleted_preview(
        &self,
        node_id: impl Into<String>,
        checkpoint_id: impl Into<String>,
        document_id: impl Into<String>,
    ) -> BlockingServiceJob<DeletedPreviewResult> {
        let ports = self.ports.clone();
        let node_id = node_id.into();
        let checkpoint_id = checkpoint_id.into();
        let document_id = document_id.into();
        BlockingServiceJob::new("preview deleted document", move || {
            let checkpoint =
                CheckpointId::from_bytes(parse_stable_id(&checkpoint_id, "restoring checkpoint")?);
            let document =
                DocumentId::from_bytes(parse_stable_id(&document_id, "deleted document")?);
            let preview = ports.history_preview(checkpoint)?;
            let document =
                load_checkpoint_document(ports.as_ref(), checkpoint, &preview, document)?
                    .ok_or_else(|| ServiceFeedError::InvalidServiceData {
                        service: ServiceKind::History,
                        reason: format!(
                            "restoring checkpoint has no canonical document {document_id}"
                        ),
                    })?;
            Ok(DeletedPreviewResult {
                node_id,
                checkpoint_id,
                document,
            })
        })
    }

    pub fn reconcile_recovery(&self) -> BlockingServiceJob<RecoveryReconcileResult> {
        let ports = self.ports.clone();
        let sequence = self.next_recovery.fetch_add(1, Ordering::Relaxed) + 1;
        BlockingServiceJob::new("reconcile recovery", move || {
            ports.reconcile_recovery(sequence)
        })
    }

    pub fn accept_recovery(
        &self,
        acceptance: RecoveryAcceptanceTicket,
    ) -> BlockingServiceJob<RecoveryAcceptedResult> {
        let ports = self.ports.clone();
        BlockingServiceJob::new("accept recovery", move || {
            ports.accept_recovery(acceptance.sequence)
        })
    }

    pub fn discard_recovery(
        &self,
        acceptance: RecoveryAcceptanceTicket,
    ) -> BlockingServiceJob<RecoveryDiscardedResult> {
        let ports = self.ports.clone();
        BlockingServiceJob::new("discard recovery", move || {
            ports.discard_recovery(acceptance.sequence)
        })
    }
}

impl HistoryCheckpointRow {
    fn from_summary(summary: CheckpointSummary) -> Self {
        Self {
            checkpoint_id: encode_hex(summary.id.as_bytes()),
            sequence: summary.sequence,
            category: match summary.category {
                CheckpointCategory::Autosave => HistoryCheckpointCategory::Autosave,
                CheckpointCategory::ExplicitSave => HistoryCheckpointCategory::ExplicitSave,
                CheckpointCategory::StructuralChange => HistoryCheckpointCategory::StructuralChange,
                CheckpointCategory::NamedSnapshot => HistoryCheckpointCategory::NamedSnapshot,
                CheckpointCategory::Restoration => HistoryCheckpointCategory::Restoration,
            },
            affected_document_ids: summary
                .affected_documents
                .iter()
                .map(|document| encode_hex(document.as_bytes()))
                .collect(),
            name: summary.name.map(|name| name.as_str().to_owned()),
            recorded_at_unix_millis: summary.recorded_at_unix_millis,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryListResult {
    pub checkpoints: Vec<HistoryCheckpointRow>,
    pub next_cursor: Option<HistoryCursor>,
}

impl HistoryListResult {
    fn from_page(page: HistoryPage) -> Self {
        Self {
            checkpoints: page
                .checkpoints
                .into_iter()
                .map(HistoryCheckpointRow::from_summary)
                .collect(),
            next_cursor: page.next_cursor,
        }
    }

    pub fn reducer_payload(&self) -> ProjectTaskPayload {
        ProjectTaskPayload::HistoryLoaded {
            checkpoints: self.checkpoints.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryPreviewResult {
    pub checkpoint: HistoryCheckpointRow,
    pub resource_paths: Vec<String>,
    pub document: Option<HistoryDocumentPreview>,
    pub current_document: Option<HistoryCurrentDocument>,
    pub project_changes: Option<Vec<HistoryComparison>>,
    pub comparison: Option<HistoryComparison>,
}

impl HistoryPreviewResult {
    fn from_preview(
        preview: SnapshotResourcePaths,
        document: Option<HistoryDocumentPreview>,
        current_document: Option<HistoryCurrentDocument>,
        comparison: Option<HistoryComparison>,
    ) -> Self {
        Self {
            checkpoint: HistoryCheckpointRow::from_summary(preview.checkpoint),
            resource_paths: preview
                .resource_paths
                .into_iter()
                .map(|path| path.as_str().to_owned())
                .collect(),
            document,
            current_document,
            project_changes: None,
            comparison,
        }
    }

    pub fn into_reducer_payload(self) -> ProjectTaskPayload {
        ProjectTaskPayload::HistoryPreviewReady {
            preview: Box::new(HistoryPreviewData {
                checkpoint: self.checkpoint,
                resource_paths: self.resource_paths,
                project_changes: self.project_changes,
                document: self.document,
            }),
            current_document: self.current_document,
            comparison: self.comparison,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeletedPreviewResult {
    pub node_id: String,
    pub checkpoint_id: String,
    pub document: HistoryDocumentPreview,
}

impl DeletedPreviewResult {
    pub fn reducer_payload(&self) -> ProjectTaskPayload {
        ProjectTaskPayload::DeletedPreviewReady {
            node_id: self.node_id.clone(),
            checkpoint_id: self.checkpoint_id.clone(),
            document_id: self.document.document_id.clone(),
            semantic: self.document.semantic.clone(),
        }
    }
}

fn load_checkpoint_document(
    ports: &dyn ServiceFeedPorts,
    checkpoint: CheckpointId,
    preview: &SnapshotResourcePaths,
    document: DocumentId,
) -> Result<Option<HistoryDocumentPreview>, ServiceFeedError> {
    let document_id = encode_hex(document.as_bytes());
    let manifest_path = CanonicalRelativePath::parse("project.toml").expect("static path");
    let paths = if preview.resource_paths.contains(&manifest_path) {
        let resource = ports.history_resource(checkpoint, &manifest_path)?;
        let codec = parchmint_project_format::ProjectFormatCodec::default();
        let manifest = codec
            .decode_manifest(&resource.bytes)
            .map_err(|error| service_error(ServiceKind::History, error))?;
        // Project identity is immaterial to this read-only path lookup.
        codec
            .decode_domain_project(&manifest, parchmint_domain::ProjectId::from_bytes([0; 16]))
            .map_err(|error| service_error(ServiceKind::History, error))?
            .map(|(_, paths)| paths.documents)
    } else {
        None
    };
    let path = if let Some(paths) = paths {
        paths.get(&document).cloned()
    } else {
        let suffix = format!("/{document_id}.html");
        let mut matches = preview.resource_paths.iter().filter(|path| {
            let name = path.as_str();
            (name.starts_with("manuscript/") || name.starts_with("research/"))
                && name.ends_with(".html")
                && (name.ends_with(&suffix)
                    || parchmint_project_format::legacy_document_id(path) == document)
        });
        let path = matches.next().cloned();
        if matches.next().is_some() {
            return Err(ServiceFeedError::InvalidServiceData {
                service: ServiceKind::History,
                reason: format!("checkpoint has duplicate paths for document {document_id}"),
            });
        }
        path
    };
    let Some(path) = path else {
        return Ok(None);
    };
    if !preview.resource_paths.contains(&path) {
        return Err(ServiceFeedError::InvalidServiceData {
            service: ServiceKind::History,
            reason: format!(
                "checkpoint is missing the manifest's document file: {}",
                path.as_str()
            ),
        });
    }
    let resource = ports.history_resource(checkpoint, &path)?;
    let body = String::from_utf8(resource.bytes).map_err(|error| {
        ServiceFeedError::InvalidServiceData {
            service: ServiceKind::History,
            reason: format!("checkpoint document {document_id} is not UTF-8: {error}"),
        }
    })?;
    let semantic = EditorCoreSession::open(CanonicalDocumentLoad::new(document, body))
        .map_err(|error| ServiceFeedError::InvalidServiceData {
            service: ServiceKind::History,
            reason: format!("checkpoint document {document_id} is not canonical: {error}"),
        })?
        .canonical_projection()
        .semantic()
        .clone();
    Ok(Some(HistoryDocumentPreview {
        document_id,
        canonical_path: path.as_str().to_owned(),
        semantic,
    }))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RecoveryAcceptanceTicket {
    sequence: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecoveryReconcileResult {
    pub accepted_records: usize,
    pub affected_documents: Vec<RecoveryDocumentSummary>,
    pub isolation: Option<String>,
    pub acceptance: Option<RecoveryAcceptanceTicket>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecoveryDocumentSummary {
    pub document_id: String,
    pub revision: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RecoveryAcceptedResult {
    pub accepted_records: usize,
    pub isolation: Option<String>,
    pub project_revision: u64,
    pub recovered_document: Option<DocumentId>,
    pub snapshot: ProjectSnapshot,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RecoveryDiscardedResult {
    pub isolation: Option<String>,
    pub project_revision: u64,
    pub snapshot: ProjectSnapshot,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchRequest {
    pub text: String,
    pub case_sensitive: bool,
    pub whole_word: bool,
    pub generation: u64,
    pub metadata_fields: Vec<MetadataFieldId>,
}

#[derive(Debug)]
pub struct SearchStart {
    pub batches: Receiver<Result<SearchBatchResult, ServiceFeedError>>,
    pub job: BlockingServiceJob<SearchRunResult>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SearchRunResult {
    pub generation: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchBatchResult {
    pub generation: u64,
    pub results: Vec<GlobalSearchResult>,
    pub finished: bool,
}

impl SearchBatchResult {
    pub fn reducer_payload(&self) -> ProjectTaskPayload {
        ProjectTaskPayload::SearchBatch {
            results: self.results.clone(),
            finished: self.finished,
        }
    }
}

#[derive(Debug, Default)]
struct SearchGenerationState {
    current: Option<u64>,
}

#[derive(Clone)]
pub struct SearchFeedController {
    ports: Arc<dyn ServiceFeedPorts>,
    state: Arc<Mutex<SearchGenerationState>>,
}

impl fmt::Debug for SearchFeedController {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SearchFeedController")
            .field("state", &self.state)
            .finish_non_exhaustive()
    }
}

impl SearchFeedController {
    fn new(ports: Arc<dyn ServiceFeedPorts>) -> Self {
        Self {
            ports,
            state: Arc::new(Mutex::new(SearchGenerationState::default())),
        }
    }

    pub fn start(&self, request: SearchRequest) -> SearchStart {
        let superseded = self.state.lock().ok().and_then(|mut state| {
            let superseded = state.current.replace(request.generation);
            superseded.filter(|generation| *generation != request.generation)
        });
        let generation = request.generation;
        let fields = BTreeSet::from([
            SearchField::Body,
            SearchField::DisplayTitle,
            SearchField::Synopsis,
        ])
        .into_iter()
        .chain(
            request
                .metadata_fields
                .into_iter()
                .map(SearchField::Metadata),
        )
        .collect();
        let query = SearchQuery {
            text: request.text,
            fields,
            case_sensitive: request.case_sensitive,
            whole_word: request.whole_word,
            generation,
        };
        let (sender, batches) = mpsc::channel();
        let sink = GatedSearchSink {
            expected_generation: generation,
            state: self.state.clone(),
            ports: self.ports.clone(),
            sender: sender.clone(),
        };
        let ports = self.ports.clone();
        let job = BlockingServiceJob::new("run global search", move || {
            if let Some(old_generation) = superseded {
                ports.search_cancel(old_generation)?;
            }
            if let Err(error) = ports.search_query(query, Box::new(sink)) {
                let _ = sender.send(Err(error.clone()));
                return Err(error);
            }
            Ok(SearchRunResult { generation })
        });
        SearchStart { batches, job }
    }

    pub fn accept_batch(&self, batch: &SearchBatchResult) -> Result<(), ServiceFeedError> {
        let state = self
            .state
            .lock()
            .map_err(|_| ServiceFeedError::InvalidState {
                operation: "accept search batch",
                reason: "search generation state is unavailable",
            })?;
        if state.current != Some(batch.generation) {
            return Err(ServiceFeedError::StaleSearchGeneration {
                expected: state.current,
                received: batch.generation,
            });
        }
        Ok(())
    }
}

struct GatedSearchSink {
    expected_generation: u64,
    state: Arc<Mutex<SearchGenerationState>>,
    ports: Arc<dyn ServiceFeedPorts>,
    sender: Sender<Result<SearchBatchResult, ServiceFeedError>>,
}

impl SearchBatchSink for GatedSearchSink {
    fn push(&self, batch: SearchBatch) {
        if let Err(error) = self.ports.authorize() {
            let _ = self.sender.send(Err(error));
            return;
        }
        let accepted = self.state.lock().is_ok_and(|state| {
            state.current == Some(self.expected_generation)
                && batch.generation == self.expected_generation
        });
        if !accepted {
            return;
        }
        let converted = batch
            .hits
            .into_iter()
            .map(search_result_from_hit)
            .collect::<Result<Vec<_>, _>>()
            .map(|results| SearchBatchResult {
                generation: batch.generation,
                results,
                finished: batch.finished,
            });
        let _ = self.sender.send(converted);
    }
}

fn search_result_from_hit(hit: SearchHit) -> Result<GlobalSearchResult, ServiceFeedError> {
    let matched = hit
        .snippet
        .match_range
        .text(&hit.snippet.text)
        .ok_or_else(|| ServiceFeedError::InvalidServiceData {
            service: ServiceKind::Search,
            reason: "hit snippet range is not valid UTF-8 text".to_owned(),
        })?;
    let start = hit.snippet.match_range.start();
    let end = hit.snippet.match_range.end();
    Ok(GlobalSearchResult {
        document_id: encode_hex(hit.document_id.as_bytes()),
        match_id: format!(
            "{}:{}:{:?}:{}:{}:{}",
            encode_hex(hit.document_id.as_bytes()),
            encode_hex(hit.block_id.as_bytes()),
            hit.field,
            hit.candidate_range.start(),
            hit.candidate_range.end(),
            hit.indexed_revision.value()
        ),
        prefix: hit.snippet.text[..start].to_owned(),
        matching_text: matched.to_owned(),
        suffix: hit.snippet.text[end..].to_owned(),
        indexed_revision: hit.indexed_revision.value(),
    })
}

trait ServiceFeedPorts: Send + Sync {
    fn snapshot_with_documents(&self) -> Result<ProjectSnapshot, ServiceFeedError>;
    fn authorize(&self) -> Result<(), ServiceFeedError>;
    fn search_query(
        &self,
        query: SearchQuery,
        sink: Box<dyn SearchBatchSink>,
    ) -> Result<(), ServiceFeedError>;
    fn search_cancel(&self, generation: u64) -> Result<(), ServiceFeedError>;
    fn history_list(&self, query: HistoryPageQuery) -> Result<HistoryPage, ServiceFeedError>;
    fn history_preview(
        &self,
        checkpoint: CheckpointId,
    ) -> Result<SnapshotResourcePaths, ServiceFeedError>;
    fn history_resource(
        &self,
        checkpoint: CheckpointId,
        path: &CanonicalRelativePath,
    ) -> Result<CheckpointResource, ServiceFeedError>;

    fn reconcile_recovery(
        &self,
        sequence: u64,
    ) -> Result<RecoveryReconcileResult, ServiceFeedError>;
    fn accept_recovery(&self, sequence: u64) -> Result<RecoveryAcceptedResult, ServiceFeedError>;
    fn discard_recovery(&self, sequence: u64) -> Result<RecoveryDiscardedResult, ServiceFeedError>;
}

struct ProjectUiPortAdapter {
    ports: ProjectUiPorts,
    recovery_acceptances: Mutex<BTreeMap<u64, PendingRecoveryChoice>>,
}

#[derive(Debug, Clone)]
struct PendingRecoveryChoice {
    acceptance: ProjectRecoveryAcceptance,
    affected_documents: Vec<RecoveryDocumentSummary>,
}

impl ProjectUiPortAdapter {
    fn new(ports: ProjectUiPorts) -> Self {
        Self {
            ports,
            recovery_acceptances: Mutex::new(BTreeMap::new()),
        }
    }

    fn access(&self) -> Result<parchmint_ui_api::ProjectUiAccess<'_>, ServiceFeedError> {
        self.ports.access().map_err(stale_session)
    }
}

impl ServiceFeedPorts for ProjectUiPortAdapter {
    fn snapshot_with_documents(&self) -> Result<ProjectSnapshot, ServiceFeedError> {
        self.access()?
            .snapshot(|query| query.snapshot_with_documents())
            .map_err(stale_session)?
            .map_err(|error| service_error(ServiceKind::ProjectQuery, error))
    }
    fn authorize(&self) -> Result<(), ServiceFeedError> {
        self.access().map(|_| ())
    }

    fn search_query(
        &self,
        query: SearchQuery,
        sink: Box<dyn SearchBatchSink>,
    ) -> Result<(), ServiceFeedError> {
        self.access()?
            .search(|search| search.query(query, sink))
            .map_err(stale_session)?
            .map_err(|error| service_error(ServiceKind::Search, error))
    }

    fn search_cancel(&self, generation: u64) -> Result<(), ServiceFeedError> {
        self.access()?
            .search(|search| search.cancel(generation))
            .map_err(stale_session)
    }

    fn history_list(&self, query: HistoryPageQuery) -> Result<HistoryPage, ServiceFeedError> {
        self.access()?
            .history(|history| history.list(query))
            .map_err(stale_session)?
            .map_err(|error| service_error(ServiceKind::History, error))
    }

    fn history_preview(
        &self,
        checkpoint: CheckpointId,
    ) -> Result<SnapshotResourcePaths, ServiceFeedError> {
        self.access()?
            .history(|history| history.preview_resource_paths(checkpoint))
            .map_err(stale_session)?
            .map_err(|error| service_error(ServiceKind::History, error))
    }

    fn history_resource(
        &self,
        checkpoint: CheckpointId,
        path: &CanonicalRelativePath,
    ) -> Result<CheckpointResource, ServiceFeedError> {
        self.access()?
            .history(|history| history.read_resource(checkpoint, path))
            .map_err(stale_session)?
            .map_err(|error| service_error(ServiceKind::History, error))
    }

    fn reconcile_recovery(
        &self,
        sequence: u64,
    ) -> Result<RecoveryReconcileResult, ServiceFeedError> {
        let state = self
            .access()?
            .persistence(|persistence| persistence.reconcile_recovery())
            .map_err(stale_session)?
            .map_err(|error| service_error(ServiceKind::Recovery, error))?;
        recovery_reconcile_result(sequence, state, &self.recovery_acceptances)
    }

    fn accept_recovery(&self, sequence: u64) -> Result<RecoveryAcceptedResult, ServiceFeedError> {
        let pending = self
            .recovery_acceptances
            .lock()
            .map_err(|_| ServiceFeedError::InvalidState {
                operation: "accept recovery",
                reason: "recovery acceptance state is unavailable",
            })?
            .remove(&sequence)
            .ok_or(ServiceFeedError::NoRecoveryToAccept)?;
        let state = self
            .access()?
            .persistence(|persistence| persistence.accept_recovery(pending.acceptance))
            .map_err(stale_session)?
            .map_err(|error| service_error(ServiceKind::Recovery, error))?;
        let (handle, _) = self
            .access()?
            .persistence(|persistence| persistence.request_save(ProjectSaveKind::Restoration))
            .map_err(stale_session)?
            .map_err(|error| service_error(ServiceKind::Recovery, error))?;
        let saved = self
            .access()?
            .persistence(|persistence| persistence.await_save(handle))
            .map_err(stale_session)?
            .map_err(|error| service_error(ServiceKind::Recovery, error))?;
        let snapshot = self
            .access()?
            .snapshot(|query| query.snapshot())
            .map_err(stale_session)?
            .map_err(|error| service_error(ServiceKind::ProjectQuery, error))?;
        Ok(RecoveryAcceptedResult {
            accepted_records: state.accepted_records,
            isolation: state.isolation.map(|isolation| format!("{isolation:?}")),
            project_revision: saved.written.project_revision.value(),
            recovered_document: pending
                .affected_documents
                .first()
                .and_then(|summary| {
                    parse_stable_id(&summary.document_id, "recovered document").ok()
                })
                .map(DocumentId::from_bytes),
            snapshot,
        })
    }

    fn discard_recovery(&self, sequence: u64) -> Result<RecoveryDiscardedResult, ServiceFeedError> {
        let pending = self
            .recovery_acceptances
            .lock()
            .map_err(|_| ServiceFeedError::InvalidState {
                operation: "discard recovery",
                reason: "recovery acceptance state is unavailable",
            })?
            .remove(&sequence)
            .ok_or(ServiceFeedError::NoRecoveryToAccept)?;
        let state = self
            .access()?
            .persistence(|persistence| persistence.discard_recovery(pending.acceptance))
            .map_err(stale_session)?
            .map_err(|error| service_error(ServiceKind::Recovery, error))?;
        let snapshot = self
            .access()?
            .snapshot(|query| query.snapshot())
            .map_err(stale_session)?
            .map_err(|error| service_error(ServiceKind::ProjectQuery, error))?;
        Ok(RecoveryDiscardedResult {
            isolation: state.isolation.map(|isolation| format!("{isolation:?}")),
            project_revision: snapshot.project.revision.value(),
            snapshot,
        })
    }
}

fn recovery_reconcile_result(
    sequence: u64,
    state: ProjectRecoveryState,
    acceptances: &Mutex<BTreeMap<u64, PendingRecoveryChoice>>,
) -> Result<RecoveryReconcileResult, ServiceFeedError> {
    let affected_documents = state
        .affected_documents
        .iter()
        .map(|(document, revision)| RecoveryDocumentSummary {
            document_id: encode_hex(document.as_bytes()),
            revision: revision.value(),
        })
        .collect::<Vec<_>>();
    let isolation = state.isolation.map(|isolation| format!("{isolation:?}"));
    let acceptance = if let Some(acceptance) = state.acceptance {
        acceptances
            .lock()
            .map_err(|_| ServiceFeedError::InvalidState {
                operation: "reconcile recovery",
                reason: "recovery acceptance state is unavailable",
            })?
            .insert(
                sequence,
                PendingRecoveryChoice {
                    acceptance,
                    affected_documents: affected_documents.clone(),
                },
            );
        Some(RecoveryAcceptanceTicket { sequence })
    } else {
        None
    };
    Ok(RecoveryReconcileResult {
        accepted_records: state.accepted_records,
        affected_documents,
        isolation,
        acceptance,
    })
}

fn stale_session(error: parchmint_ui_api::StaleProjectSession) -> ServiceFeedError {
    ServiceFeedError::StaleSession {
        session_id: error.session().session_id(),
        generation: error.session().generation(),
    }
}

fn service_error(service: ServiceKind, error: impl fmt::Display) -> ServiceFeedError {
    ServiceFeedError::Service {
        service,
        message: error.to_string(),
    }
}

fn parse_stable_id(value: &str, kind: &'static str) -> Result<[u8; 16], ServiceFeedError> {
    parchmint_domain::decode_stable_id(value).ok_or_else(|| ServiceFeedError::InvalidIdentifier {
        kind,
        value: value.to_owned(),
    })
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::AtomicBool;

    use parchmint_domain::{NodeId, Project, ProjectId};
    use parchmint_history_api::{CheckpointSummary, HistoryPage, SnapshotName};
    use parchmint_search_api::{BlockId, RevisionId, SearchSnippet, TextRange};

    use super::*;

    #[derive(Default)]
    struct FakePorts {
        stale: AtomicBool,
        canceled: Mutex<Vec<u64>>,
        search_batches: Mutex<Vec<SearchBatch>>,
        history_page: Mutex<Option<HistoryPage>>,
        history_preview: Mutex<Option<SnapshotResourcePaths>>,
        history_resource: Mutex<Option<CheckpointResource>>,
        history_resources: Mutex<BTreeMap<CanonicalRelativePath, CheckpointResource>>,
        recovery_revision: AtomicU64,
    }

    impl FakePorts {
        fn check(&self) -> Result<(), ServiceFeedError> {
            if self.stale.load(Ordering::Relaxed) {
                Err(ServiceFeedError::StaleSession {
                    session_id: 7,
                    generation: 3,
                })
            } else {
                Ok(())
            }
        }
    }

    impl ServiceFeedPorts for FakePorts {
        fn snapshot_with_documents(&self) -> Result<ProjectSnapshot, ServiceFeedError> {
            Err(ServiceFeedError::InvalidState {
                operation: "test snapshot",
                reason: "not configured",
            })
        }
        fn authorize(&self) -> Result<(), ServiceFeedError> {
            self.check()
        }

        fn search_query(
            &self,
            _: SearchQuery,
            sink: Box<dyn SearchBatchSink>,
        ) -> Result<(), ServiceFeedError> {
            self.check()?;
            for batch in self.search_batches.lock().expect("search batches").clone() {
                sink.push(batch);
            }
            Ok(())
        }

        fn search_cancel(&self, generation: u64) -> Result<(), ServiceFeedError> {
            self.check()?;
            self.canceled.lock().expect("cancel log").push(generation);
            Ok(())
        }

        fn history_list(&self, _: HistoryPageQuery) -> Result<HistoryPage, ServiceFeedError> {
            self.check()?;
            self.history_page
                .lock()
                .expect("history page")
                .clone()
                .ok_or_else(|| ServiceFeedError::Service {
                    service: ServiceKind::History,
                    message: "missing fake page".to_owned(),
                })
        }

        fn history_preview(
            &self,
            _: CheckpointId,
        ) -> Result<SnapshotResourcePaths, ServiceFeedError> {
            self.check()?;
            self.history_preview
                .lock()
                .expect("history preview")
                .clone()
                .ok_or_else(|| ServiceFeedError::Service {
                    service: ServiceKind::History,
                    message: "missing fake preview".to_owned(),
                })
        }

        fn history_resource(
            &self,
            _: CheckpointId,
            path: &CanonicalRelativePath,
        ) -> Result<CheckpointResource, ServiceFeedError> {
            self.check()?;
            if let Some(resource) = self.history_resources.lock().unwrap().get(path) {
                return Ok(resource.clone());
            }
            self.history_resource
                .lock()
                .expect("history resource")
                .clone()
                .ok_or_else(|| ServiceFeedError::Service {
                    service: ServiceKind::History,
                    message: "missing fake resource".to_owned(),
                })
        }

        fn reconcile_recovery(
            &self,
            sequence: u64,
        ) -> Result<RecoveryReconcileResult, ServiceFeedError> {
            self.check()?;
            Ok(RecoveryReconcileResult {
                accepted_records: 2,
                affected_documents: vec![RecoveryDocumentSummary {
                    document_id: encode_hex(&[6; 16]),
                    revision: 11,
                }],
                isolation: None,
                acceptance: Some(RecoveryAcceptanceTicket { sequence }),
            })
        }

        fn accept_recovery(&self, _: u64) -> Result<RecoveryAcceptedResult, ServiceFeedError> {
            self.check()?;
            Ok(RecoveryAcceptedResult {
                accepted_records: 2,
                isolation: None,
                project_revision: self.recovery_revision.load(Ordering::Relaxed),
                recovered_document: Some(DocumentId::from_bytes([6; 16])),
                snapshot: recovery_snapshot(),
            })
        }

        fn discard_recovery(&self, _: u64) -> Result<RecoveryDiscardedResult, ServiceFeedError> {
            self.check()?;
            Ok(RecoveryDiscardedResult {
                isolation: None,
                project_revision: self.recovery_revision.load(Ordering::Relaxed),
                snapshot: recovery_snapshot(),
            })
        }
    }

    fn recovery_snapshot() -> ProjectSnapshot {
        ProjectSnapshot {
            project: Project::new(ProjectId::from_bytes([7; 16])),
            document_summaries: Vec::new(),
            documents: Vec::new(),
            styles_css: String::new(),
        }
    }

    fn feeds(fake: Arc<FakePorts>) -> AsyncServiceFeeds {
        AsyncServiceFeeds::from_ports(fake)
    }

    fn summary(id: u8, sequence: u64) -> CheckpointSummary {
        CheckpointSummary {
            id: CheckpointId::from_bytes([id; 16]),
            sequence,
            category: CheckpointCategory::NamedSnapshot,
            affected_documents: vec![DocumentId::from_bytes([id + 1; 16])],
            name: Some(SnapshotName::new("Draft").expect("snapshot name")),
            recorded_at_unix_millis: Some(sequence),
        }
    }

    fn hit(generation: u64) -> SearchBatch {
        SearchBatch {
            generation,
            hits: vec![SearchHit {
                document_id: DocumentId::from_bytes([2; 16]),
                block_id: BlockId::from_bytes([3; 16]),
                indexed_revision: RevisionId::from(9),
                field: SearchField::Body,
                candidate_range: TextRange::new(5, 11).expect("range"),
                snippet: SearchSnippet {
                    text: "the river bends".to_owned(),
                    match_range: TextRange::new(4, 9).expect("snippet range"),
                },
            }],
            finished: true,
        }
    }

    #[test]
    fn jobs_reject_a_stale_session_at_execution_time() {
        let fake = Arc::new(FakePorts::default());
        fake.history_page
            .lock()
            .expect("history page")
            .replace(HistoryPage {
                checkpoints: Vec::new(),
                next_cursor: None,
            });
        let job = feeds(fake.clone()).history_list(None, 20, None);
        fake.stale.store(true, Ordering::Relaxed);

        assert_eq!(
            job.run(),
            Err(ServiceFeedError::StaleSession {
                session_id: 7,
                generation: 3,
            })
        );
    }

    #[test]
    fn superseded_search_jobs_cannot_deliver_results() {
        let fake = Arc::new(FakePorts::default());
        fake.search_batches
            .lock()
            .expect("search batches")
            .extend([hit(1), hit(2)]);
        let controller = feeds(fake.clone()).search().clone();
        let old = controller.start(SearchRequest {
            text: "river".to_owned(),
            case_sensitive: false,
            whole_word: false,
            generation: 1,
            metadata_fields: Vec::new(),
        });
        let current = controller.start(SearchRequest {
            text: "river".to_owned(),
            case_sensitive: false,
            whole_word: false,
            generation: 2,
            metadata_fields: Vec::new(),
        });

        assert_eq!(
            controller.accept_batch(&SearchBatchResult {
                generation: 1,
                results: Vec::new(),
                finished: true,
            }),
            Err(ServiceFeedError::StaleSearchGeneration {
                expected: Some(2),
                received: 1,
            })
        );
        current.job.run().expect("current search");
        let batch = current
            .batches
            .recv()
            .expect("current batch")
            .expect("valid batch");
        assert_eq!(batch.generation, 2);
        assert_eq!(batch.results[0].matching_text, "river");
        assert!(current.batches.try_recv().is_err());
        assert_eq!(*fake.canceled.lock().unwrap(), vec![1]);
        old.job.run().expect("late old search");
        assert!(old.batches.try_recv().is_err());
        assert!(controller.accept_batch(&batch).is_ok());
    }

    #[test]
    fn history_list_and_preview_preserve_service_facts() {
        let fake = Arc::new(FakePorts::default());
        fake.history_page
            .lock()
            .expect("history page")
            .replace(HistoryPage {
                checkpoints: vec![summary(1, 42)],
                next_cursor: Some(HistoryCursor::new("next")),
            });
        let path = parchmint_history_api::CanonicalRelativePath::parse("project.toml")
            .expect("canonical path");
        fake.history_preview
            .lock()
            .expect("history preview")
            .replace(SnapshotResourcePaths {
                checkpoint: summary(1, 42),
                resource_paths: vec![path],
            });
        let feeds = feeds(fake);

        let page = feeds.history_list(None, 20, None).run().expect("list");
        assert_eq!(page.checkpoints[0].sequence, 42);
        assert_eq!(
            page.checkpoints[0].category,
            HistoryCheckpointCategory::NamedSnapshot
        );
        assert_eq!(page.checkpoints[0].name.as_deref(), Some("Draft"));
        assert_eq!(
            page.next_cursor.as_ref().map(HistoryCursor::as_str),
            Some("next")
        );
        let preview = feeds
            .history_preview(encode_hex(&[1; 16]), None)
            .run()
            .expect("preview");
        assert_eq!(preview.checkpoint.checkpoint_id, encode_hex(&[1; 16]));
        assert_eq!(preview.resource_paths, ["project.toml"]);
    }

    #[test]
    fn history_document_preview_reads_exact_checkpoint_bytes() {
        let fake = Arc::new(FakePorts::default());
        let checkpoint = CheckpointId::from_bytes([3; 16]);
        let document = DocumentId::from_bytes([4; 16]);
        let path = CanonicalRelativePath::parse(format!(
            "manuscript/{}.html",
            encode_hex(document.as_bytes())
        ))
        .unwrap();
        let body =
            br#"<p data-block-id="04040404040404040404040404040404">checkpoint words</p>"#.to_vec();
        let hash = parchmint_history_api::ContentHash::from_bytes([8; 32]);
        fake.history_preview
            .lock()
            .unwrap()
            .replace(SnapshotResourcePaths {
                checkpoint: summary(3, 9),
                resource_paths: vec![path.clone()],
            });
        fake.history_resource
            .lock()
            .unwrap()
            .replace(CheckpointResource {
                checkpoint,
                path: path.clone(),
                content_hash: hash,
                bytes: body,
            });

        let document_id = encode_hex(document.as_bytes());
        let preview = feeds(fake)
            .history_preview(
                encode_hex(checkpoint.as_bytes()),
                Some(HistoryCurrentDocument {
                    document_id: document_id.clone(),
                    title: "Current chapter".to_owned(),
                    body: String::new(),
                    semantic: Default::default(),
                }),
            )
            .run()
            .expect("checkpoint document preview");
        assert_eq!(
            preview
                .comparison
                .as_ref()
                .map(|comparison| comparison.document_title.as_str()),
            Some("Current chapter")
        );
        assert_eq!(
            preview
                .current_document
                .as_ref()
                .map(|document| document.document_id.as_str()),
            Some(document_id.as_str())
        );
        let document = preview.document.expect("document content");
        assert_eq!(document.canonical_path, path.as_str());
        assert_eq!(document.semantic.blocks().len(), 1);
    }

    #[test]
    fn legacy_history_preview_resolves_the_same_document_identity_as_project_open() {
        let fake = FakePorts::default();
        let checkpoint = CheckpointId::from_bytes([3; 16]);
        let path = CanonicalRelativePath::parse("manuscript/untitled-document.html").unwrap();
        let document = parchmint_project_format::legacy_document_id(&path);
        let preview = SnapshotResourcePaths {
            checkpoint: summary(3, 9),
            resource_paths: vec![path.clone()],
        };
        fake.history_resource
            .lock()
            .unwrap()
            .replace(CheckpointResource {
                checkpoint,
                path: path.clone(),
                content_hash: parchmint_history_api::ContentHash::from_bytes([8; 32]),
                bytes: b"<p>Earlier chapter, not an empty checkpoint.</p>".to_vec(),
            });
        let loaded = load_checkpoint_document(&fake, checkpoint, &preview, document)
            .unwrap()
            .unwrap();
        assert_eq!(loaded.canonical_path, path.as_str());
        assert_eq!(
            loaded.semantic.blocks()[0].text(),
            "Earlier chapter, not an empty checkpoint."
        );
        assert!(
            load_checkpoint_document(
                &fake,
                checkpoint,
                &preview,
                DocumentId::from_bytes([99; 16])
            )
            .unwrap()
            .is_none()
        );
    }

    #[test]
    fn history_uses_the_checkpoint_manifest_path_not_a_guessed_current_filename() {
        let fake = FakePorts::default();
        let checkpoint = CheckpointId::from_bytes([3; 16]);
        let document = DocumentId::from_bytes([4; 16]);
        let path = CanonicalRelativePath::parse("research/old-lore-name.html").unwrap();
        let manifest_path = CanonicalRelativePath::parse("project.toml").unwrap();
        let manifest = format!(
            "[project]\n[parchmint-structure]\nversion = 1\n[[parchmint-structure.nodes]]\nid = '{}'\nparent = '{}'\norder = 0\ntitle = 'Lore'\nkind = 'document'\ndocument-id = '{}'\npath = '{}'\n",
            encode_hex(&[7; 16]),
            encode_hex(NodeId::research_root().as_bytes()),
            encode_hex(document.as_bytes()),
            path.as_str()
        );
        for (path, bytes) in [
            (manifest_path.clone(), manifest.into_bytes()),
            (path.clone(), b"<p>Historical lore</p>".to_vec()),
        ] {
            fake.history_resources.lock().unwrap().insert(
                path.clone(),
                CheckpointResource {
                    checkpoint,
                    path,
                    content_hash: parchmint_history_api::ContentHash::from_bytes([8; 32]),
                    bytes,
                },
            );
        }
        let mut preview = SnapshotResourcePaths {
            checkpoint: summary(3, 9),
            resource_paths: vec![manifest_path, path.clone()],
        };
        let loaded = load_checkpoint_document(&fake, checkpoint, &preview, document)
            .unwrap()
            .unwrap();
        assert_eq!(loaded.canonical_path, path.as_str());
        assert_eq!(loaded.semantic.blocks()[0].text(), "Historical lore");
        preview.resource_paths.pop();
        assert!(
            load_checkpoint_document(&fake, checkpoint, &preview, document).is_err(),
            "missing data must be an error, not a blank preview"
        );
    }

    #[test]
    fn recovery_acceptance_maps_the_authoritative_revision() {
        let fake = Arc::new(FakePorts::default());
        fake.recovery_revision.store(18, Ordering::Relaxed);
        let feeds = feeds(fake);
        let reconciled = feeds.reconcile_recovery().run().expect("reconcile");
        assert_eq!(reconciled.accepted_records, 2);
        assert_eq!(reconciled.affected_documents[0].revision, 11);
        let accepted = feeds
            .accept_recovery(reconciled.acceptance.expect("acceptance"))
            .run()
            .expect("accept");
        assert_eq!(accepted.project_revision, 18);
        assert_eq!(
            accepted.recovered_document,
            Some(DocumentId::from_bytes([6; 16]))
        );
    }

    #[test]
    fn recovery_discard_returns_the_authoritative_current_snapshot() {
        let fake = Arc::new(FakePorts::default());
        fake.recovery_revision.store(12, Ordering::Relaxed);
        let feeds = feeds(fake);
        let reconciled = feeds.reconcile_recovery().run().expect("reconcile");

        let discarded = feeds
            .discard_recovery(reconciled.acceptance.expect("acceptance"))
            .run()
            .expect("discard");

        assert_eq!(discarded.project_revision, 12);
        assert_eq!(
            discarded.snapshot.project.id,
            ProjectId::from_bytes([7; 16])
        );
    }
}
