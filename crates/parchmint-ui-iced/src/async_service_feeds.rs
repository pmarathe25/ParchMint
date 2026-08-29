//! Background service controllers for project UI effects.
//!
//! These helpers are deliberately independent of Iced widgets. Synchronous
//! service calls are packaged as [`BlockingServiceJob`] values; the native UI
//! must run those jobs on a blocking worker and deliver the owned results back
//! to its reducer. Every job reacquires [`ProjectUiPorts::access`] when it runs.

#![allow(
    dead_code,
    reason = "the controller is wired into the native Iced runtime in a separate integration pass"
)]

use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
    future::Future,
    pin::Pin,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
        mpsc::{self, Receiver, Sender},
    },
};

use parchmint_application::DocumentSnapshot;
use parchmint_domain::{
    DocumentId, MetadataFieldId, NodeId, NodeKind, Project, ProjectExportSetting, ProjectSection,
};
use parchmint_editor_api::CanonicalDocumentLoad;
use parchmint_editor_core::EditorCoreSession;
use parchmint_export_api::{
    CancelOutcome, ExportCompletion, ExportDefaults, ExportError, ExportHandle, ExportNode,
    ExportNumbering, ExportPlan, ExportProgress as OperationProgress, ExportProgressSink,
    ExportRequest, ExportRunOptions, ExportSettings, ExportSink, ExportSource, ExportStatus,
    ExportStyleCatalog, ExportValidationReport, InheritedSetting,
    ProjectSnapshot as ExportProjectSnapshot, SourceRevision,
};
use parchmint_history_api::{
    CheckpointCategory, CheckpointId, CheckpointResource, CheckpointSummary, HistoryCursor,
    HistoryPage, HistoryPageQuery, RestorePlan, SnapshotResourcePaths,
};
use parchmint_project_format::CanonicalRelativePath;
use parchmint_search_api::{SearchBatch, SearchBatchSink, SearchField, SearchHit, SearchQuery};
use parchmint_ui_api::{
    ProjectRecoveryAcceptance, ProjectRecoveryState, ProjectSaveKind, ProjectSnapshot,
    ProjectUiPorts,
};

use crate::{
    GlobalSearchResult, HistoryCheckpointCategory, HistoryCheckpointRow, HistoryDocumentPreview,
    HistoryPreviewData, ProjectTaskPayload,
};

/// A boxed operation that may call blocking service traits.
///
/// Constructing a job is update-loop safe. Calling [`Self::run`] is not: the
/// native integration must run it on a blocking worker.
#[must_use = "run the service job on a blocking worker"]
pub struct BlockingServiceJob<T> {
    operation: &'static str,
    run: Option<Box<dyn FnOnce() -> Result<T, ServiceFeedError> + Send + 'static>>,
}

impl<T> fmt::Debug for BlockingServiceJob<T> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("BlockingServiceJob")
            .field("operation", &self.operation)
            .field("pending", &self.run.is_some())
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
            run: Some(Box::new(run)),
        }
    }

    pub const fn operation(&self) -> &'static str {
        self.operation
    }

    pub fn run(mut self) -> Result<T, ServiceFeedError> {
        let run = self.run.take().ok_or(ServiceFeedError::InvalidState {
            operation: self.operation,
            reason: "service job was already consumed",
        })?;
        run()
    }
}

pub type ServiceFuture<T> =
    Pin<Box<dyn Future<Output = Result<T, ServiceFeedError>> + Send + 'static>>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServiceKind {
    Search,
    History,
    Recovery,
    Export,
    ProjectQuery,
    PlatformOutput,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnsupportedBoundary {
    /// `ProjectEffect::RestoreDeletedSubtree` is already executed by the
    /// authoritative project command executor and must not be run twice.
    RecentlyDeletedOwnedByCommandExecutor,
    /// `Exporter::export` creates its handle internally and returns it only
    /// after the synchronous call, so the UI cannot cancel an in-flight render.
    ExportHandleUnavailableWhileRendering,
    /// The exporter API has no progress callback. Only queued and terminal
    /// progress can be reported without inventing intermediate work.
    ExportProgressFeedUnavailable,
    /// The platform API supports validated HTTPS intents, not file open/reveal.
    ExportArtifactPathResolverUnavailable,
    /// History returns a write/delete plan; applying it requires the canonical
    /// save/restore coordinator rather than a HistoryStore call.
    HistoryRestorePlanExecutorUnavailable,
    /// The recovery API accepted data but exposes no editor focus action.
    RecoveryFocusOwnedByWidgetRuntime,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConversionGap {
    /// Export requires deterministic project CSS, which is not present in the
    /// UI snapshot and must come from the canonical project-format projection.
    ExportStyleCssUnavailable,
    /// A UI document body has no block identities/ranges, so it cannot be
    /// fabricated into a `SearchDocumentProjection` for result revalidation.
    SearchBlockProjectionUnavailable,
    /// History exposes manifests and hashes, not reconstructed rich previews.
    HistoryRichPreviewUnavailable,
    /// Deleted tombstones expose structural metadata but no rich content feed.
    RecentlyDeletedRichPreviewUnavailable,
    MalformedProjectHierarchy {
        node_id: String,
    },
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
    CanceledSearchGeneration {
        generation: u64,
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
    Unsupported(UnsupportedBoundary),
    Conversion(ConversionGap),
    NoRecoveryToAccept,
    OutputUnavailable,
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
            Self::CanceledSearchGeneration { generation } => {
                write!(formatter, "search generation {generation} was canceled")
            }
            Self::InvalidIdentifier { kind, value } => {
                write!(formatter, "invalid {kind} identifier {value:?}")
            }
            Self::InvalidServiceData { service, reason } => {
                write!(formatter, "invalid {service:?} service data: {reason}")
            }
            Self::Service { service, message } => {
                write!(formatter, "{service:?} service failed: {message}")
            }
            Self::Unsupported(boundary) => write!(formatter, "unsupported boundary: {boundary:?}"),
            Self::Conversion(gap) => write!(formatter, "data conversion gap: {gap:?}"),
            Self::NoRecoveryToAccept => {
                formatter.write_str("there is no reconciled recovery to accept")
            }
            Self::OutputUnavailable => formatter.write_str("export output is unavailable"),
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
    active_export: Arc<Mutex<Option<ExportHandle>>>,
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
            active_export: Arc::new(Mutex::new(None)),
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
        document_id: Option<String>,
    ) -> BlockingServiceJob<HistoryPreviewResult> {
        let ports = self.ports.clone();
        let checkpoint_id = checkpoint_id.into();
        BlockingServiceJob::new("preview History", move || {
            let checkpoint = parse_stable_id(&checkpoint_id, "History checkpoint")?;
            let checkpoint = CheckpointId::from_bytes(checkpoint);
            let preview = ports.history_preview(checkpoint)?;
            let document = document_id
                .map(|document| {
                    let document =
                        DocumentId::from_bytes(parse_stable_id(&document, "History document")?);
                    load_checkpoint_document(ports.as_ref(), checkpoint, &preview, document)
                })
                .transpose()?
                .flatten();
            Ok(HistoryPreviewResult::from_preview(preview, document))
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

    /// Returns the whole-project restore plan. It does not apply the plan.
    pub fn history_restore_plan(
        &self,
        checkpoint_id: impl Into<String>,
    ) -> BlockingServiceJob<HistoryRestorePlanResult> {
        let ports = self.ports.clone();
        let checkpoint_id = checkpoint_id.into();
        BlockingServiceJob::new("plan History restore", move || {
            let checkpoint = parse_stable_id(&checkpoint_id, "History checkpoint")?;
            ports
                .history_restore(CheckpointId::from_bytes(checkpoint))
                .map(|plan| HistoryRestorePlanResult { plan })
        })
    }

    pub const fn recently_deleted_restore_disposition(&self) -> RecentlyDeletedRestoreDisposition {
        RecentlyDeletedRestoreDisposition::CommandExecutorOwned
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

    pub fn plan_export(
        &self,
        request: ExportRequest,
        project: ExportProjectSnapshot,
    ) -> BlockingServiceJob<ExportPlan> {
        let ports = self.ports.clone();
        BlockingServiceJob::new("plan export", move || ports.export_plan(request, &project))
    }

    pub fn validate_export(&self, plan: ExportPlan) -> BlockingServiceJob<ExportValidationReport> {
        let ports = self.ports.clone();
        BlockingServiceJob::new("validate export", move || ports.export_validate(&plan))
    }

    /// Starts a synchronous exporter on a blocking worker.
    ///
    /// Progress is forwarded from the exporter, while its returned
    /// [`ExportCompletion`] remains the authoritative success result.
    pub fn start_export(
        &self,
        plan: ExportPlan,
        sink: Box<dyn ExportSink>,
        source_revision: u64,
    ) -> ExportStart {
        let ports = self.ports.clone();
        let active_export = self.active_export.clone();
        let output_name = plan.target().name().as_str().to_owned();
        let (progress_sender, progress) = mpsc::channel();
        let handle = ExportHandle::new();
        if let Ok(mut active) = active_export.lock()
            && let Some(replaced) = active.replace(handle.clone())
        {
            let _ = replaced.cancel();
        }
        let progress_sink = Arc::new(ChannelExportProgress {
            sender: progress_sender,
        });
        let job = BlockingServiceJob::new("start export", move || {
            let result = ports.export_start(plan, sink, handle.clone(), progress_sink);
            if let Ok(mut active) = active_export.lock()
                && active
                    .as_ref()
                    .is_some_and(|current| current.same_operation(&handle))
            {
                *active = None;
            }
            result?;
            match handle.status() {
                // `ExportCompletion` is returned only after the temporary
                // output has been safely finished. Treat it as authoritative
                // when an adapter leaves its status handle unsettled.
                ExportStatus::Completed | ExportStatus::Pending | ExportStatus::Running => {
                    Ok(SuccessfulExportOutput {
                        output_name,
                        source_revision,
                    })
                }
                ExportStatus::Cancelled => Err(ServiceFeedError::Service {
                    service: ServiceKind::Export,
                    message: ExportError::Cancelled.to_string(),
                }),
                ExportStatus::Failed => Err(ServiceFeedError::Service {
                    service: ServiceKind::Export,
                    message: "export handle reported failure".to_owned(),
                }),
            }
        });
        ExportStart { progress, job }
    }

    /// Reports the current cancellation boundary honestly. `Exporter::export`
    /// returns its handle only after synchronous rendering, so no controller
    /// can cancel the in-flight operation through the existing trait.
    pub fn cancel_export(&self) -> Result<BlockingServiceJob<CancelOutcome>, ServiceFeedError> {
        let handle = self
            .active_export
            .lock()
            .map_err(|_| ServiceFeedError::InvalidState {
                operation: "cancel export",
                reason: "export operation state is unavailable",
            })?
            .clone()
            .ok_or(ServiceFeedError::OutputUnavailable)?;
        Ok(BlockingServiceJob::new("cancel export", move || {
            Ok(handle.cancel())
        }))
    }

    /// Invokes an output adapter only for a success-gated intent and after
    /// reauthorizing the exact project-session generation.
    pub fn invoke_output_intent(
        &self,
        platform: Arc<dyn ExportOutputPlatform>,
        intent: ExportOutputIntent,
    ) -> ServiceFuture<()> {
        let ports = self.ports.clone();
        Box::pin(async move {
            ports.authorize()?;
            platform.invoke(intent).await
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecentlyDeletedRestoreDisposition {
    CommandExecutorOwned,
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
}

impl HistoryPreviewResult {
    fn from_preview(
        preview: SnapshotResourcePaths,
        document: Option<HistoryDocumentPreview>,
    ) -> Self {
        Self {
            checkpoint: HistoryCheckpointRow::from_summary(preview.checkpoint),
            resource_paths: preview
                .resource_paths
                .into_iter()
                .map(|path| path.as_str().to_owned())
                .collect(),
            document,
        }
    }

    pub fn reducer_payload(&self) -> ProjectTaskPayload {
        ProjectTaskPayload::HistoryPreviewReady {
            preview: HistoryPreviewData {
                checkpoint: self.checkpoint.clone(),
                resource_paths: self.resource_paths.iter().cloned().collect(),
                document: self.document.clone(),
            },
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
    let suffix = format!("/{document_id}.html");
    let mut matches = preview
        .resource_paths
        .iter()
        .filter(|path| path.as_str().ends_with(&suffix));
    let Some(path) = matches.next().cloned() else {
        return Ok(None);
    };
    if matches.next().is_some() {
        return Err(ServiceFeedError::InvalidServiceData {
            service: ServiceKind::History,
            reason: format!("checkpoint has duplicate paths for document {document_id}"),
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryRestorePlanResult {
    pub plan: RestorePlan,
}

impl HistoryRestorePlanResult {
    pub const fn apply_in_ui_layer(&self) -> Result<(), ServiceFeedError> {
        Err(ServiceFeedError::Unsupported(
            UnsupportedBoundary::HistoryRestorePlanExecutorUnavailable,
        ))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RecoveryAcceptanceTicket {
    sequence: u64,
}

impl RecoveryAcceptanceTicket {
    pub const fn sequence(self) -> u64 {
        self.sequence
    }
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

impl RecoveryAcceptedResult {
    pub const fn reducer_payload(&self) -> ProjectTaskPayload {
        ProjectTaskPayload::RecoveryAccepted {
            revision: self.project_revision,
        }
    }
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
    pub generation: u64,
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
    canceled: BTreeSet<u64>,
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
            state.canceled.remove(&request.generation);
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
        SearchStart {
            generation,
            batches,
            job,
        }
    }

    /// Invalidates delivery immediately and returns the blocking service call.
    pub fn cancel(&self, generation: u64) -> BlockingServiceJob<()> {
        if let Ok(mut state) = self.state.lock() {
            state.canceled.insert(generation);
            if state.current == Some(generation) {
                state.current = None;
            }
        }
        let ports = self.ports.clone();
        BlockingServiceJob::new("cancel global search", move || {
            ports.search_cancel(generation)
        })
    }

    pub fn accept_batch(&self, batch: &SearchBatchResult) -> Result<(), ServiceFeedError> {
        let state = self
            .state
            .lock()
            .map_err(|_| ServiceFeedError::InvalidState {
                operation: "accept search batch",
                reason: "search generation state is unavailable",
            })?;
        if state.canceled.contains(&batch.generation) {
            return Err(ServiceFeedError::CanceledSearchGeneration {
                generation: batch.generation,
            });
        }
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
                && !state.canceled.contains(&self.expected_generation)
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportProgress {
    Planning,
    Rendering { completed: u64, total: u64 },
    Committing,
}

struct ChannelExportProgress {
    sender: Sender<ExportProgress>,
}

impl ExportProgressSink for ChannelExportProgress {
    fn report(&self, progress: OperationProgress) {
        let progress = match progress {
            OperationProgress::Planning => ExportProgress::Planning,
            OperationProgress::Rendering { completed, total } => {
                ExportProgress::Rendering { completed, total }
            }
            OperationProgress::Committing => ExportProgress::Committing,
        };
        let _ = self.sender.send(progress);
    }
}

#[derive(Debug)]
pub struct ExportStart {
    pub progress: Receiver<ExportProgress>,
    pub job: BlockingServiceJob<SuccessfulExportOutput>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SuccessfulExportOutput {
    output_name: String,
    source_revision: u64,
}

impl SuccessfulExportOutput {
    pub fn output_name(&self) -> &str {
        &self.output_name
    }

    pub const fn source_revision(&self) -> u64 {
        self.source_revision
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportOutputAction {
    Open,
    Reveal,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportOutputIntent {
    output: SuccessfulExportOutput,
    action: ExportOutputAction,
}

impl ExportOutputIntent {
    pub fn output(&self) -> &SuccessfulExportOutput {
        &self.output
    }

    pub const fn action(&self) -> ExportOutputAction {
        self.action
    }
}

/// Creates an open/reveal intent only from a successful terminal result.
pub fn successful_output_intent(
    result: &Result<SuccessfulExportOutput, ServiceFeedError>,
    action: ExportOutputAction,
) -> Result<ExportOutputIntent, ServiceFeedError> {
    result
        .as_ref()
        .map(|output| ExportOutputIntent {
            output: output.clone(),
            action,
        })
        .map_err(|_| ServiceFeedError::OutputUnavailable)
}

pub trait ExportOutputPlatform: Send + Sync {
    fn invoke(&self, intent: ExportOutputIntent) -> ServiceFuture<()>;
}

/// Converts an authoritative UI snapshot into an exporter snapshot only when
/// canonical serialized CSS is supplied by the project-format boundary.
pub fn export_snapshot_from_ui(
    snapshot: &ProjectSnapshot,
    canonical_styles_css: Option<String>,
) -> Result<ExportProjectSnapshot, ServiceFeedError> {
    let css = canonical_styles_css.ok_or(ServiceFeedError::Conversion(
        ConversionGap::ExportStyleCssUnavailable,
    ))?;
    export_snapshot_with_css(&snapshot.project, &snapshot.documents, css)
}

fn export_snapshot_with_css(
    project: &Project,
    documents: &[DocumentSnapshot],
    css: String,
) -> Result<ExportProjectSnapshot, ServiceFeedError> {
    let mut visited = BTreeSet::new();
    let manuscript = export_children(project, ProjectSection::Manuscript.root_id(), &mut visited)?;
    let included_documents = collect_export_document_ids(&manuscript);
    let sources = documents
        .iter()
        .filter(|document| included_documents.contains(&document.document_id))
        .map(|document| {
            (
                document.document_id,
                ExportSource {
                    revision: SourceRevision::from(document.revision.value()),
                    body: document.body.clone(),
                },
            )
        })
        .collect();
    Ok(ExportProjectSnapshot::new(
        ExportStyleCatalog::new(css),
        ExportDefaults {
            emit_titles: project.export_settings.emit_titles != ProjectExportSetting::Disabled,
            start_new_page: project.export_settings.starts_new_page,
        },
        manuscript,
        sources,
    ))
}

fn export_children(
    project: &Project,
    parent: NodeId,
    visited: &mut BTreeSet<NodeId>,
) -> Result<Vec<ExportNode>, ServiceFeedError> {
    let mut nodes = Vec::new();
    for id in project.nodes.children(parent) {
        if !visited.insert(*id) {
            return Err(ServiceFeedError::Conversion(
                ConversionGap::MalformedProjectHierarchy {
                    node_id: encode_hex(id.as_bytes()),
                },
            ));
        }
        let node = project.nodes.get(*id).ok_or_else(|| {
            ServiceFeedError::Conversion(ConversionGap::MalformedProjectHierarchy {
                node_id: encode_hex(id.as_bytes()),
            })
        })?;
        let settings = ExportSettings {
            emit_titles: match node.export_settings.emit_titles {
                ProjectExportSetting::Inherit => InheritedSetting::Inherit,
                ProjectExportSetting::Enabled => InheritedSetting::Enabled,
                ProjectExportSetting::Disabled => InheritedSetting::Disabled,
            },
            start_new_page: if node.export_settings.starts_new_page {
                InheritedSetting::Enabled
            } else {
                InheritedSetting::Inherit
            },
        };
        match node.kind {
            NodeKind::Root(_) => {
                return Err(ServiceFeedError::Conversion(
                    ConversionGap::MalformedProjectHierarchy {
                        node_id: encode_hex(id.as_bytes()),
                    },
                ));
            }
            NodeKind::Group => nodes.push(ExportNode::group(
                node.title.clone(),
                settings,
                export_children(project, *id, visited)?,
            )),
            NodeKind::Document(document) => {
                nodes.push(ExportNode::document(document, node.title.clone(), settings));
            }
        }
    }
    Ok(nodes)
}

fn collect_export_document_ids(nodes: &[ExportNode]) -> BTreeSet<DocumentId> {
    let mut documents = BTreeSet::new();
    for node in nodes {
        match node {
            ExportNode::Group { children, .. } => {
                documents.extend(collect_export_document_ids(children));
            }
            ExportNode::Document { id, .. } => {
                documents.insert(*id);
            }
        }
    }
    documents
}

pub fn export_request(output_name: impl Into<String>, number_documents: bool) -> ExportRequest {
    ExportRequest::new(
        output_name,
        ExportRunOptions {
            numbering: if number_documents {
                ExportNumbering::Documents
            } else {
                ExportNumbering::None
            },
        },
    )
}

trait ServiceFeedPorts: Send + Sync {
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
    fn history_restore(&self, checkpoint: CheckpointId) -> Result<RestorePlan, ServiceFeedError>;
    fn reconcile_recovery(
        &self,
        sequence: u64,
    ) -> Result<RecoveryReconcileResult, ServiceFeedError>;
    fn accept_recovery(&self, sequence: u64) -> Result<RecoveryAcceptedResult, ServiceFeedError>;
    fn discard_recovery(&self, sequence: u64) -> Result<RecoveryDiscardedResult, ServiceFeedError>;
    fn export_plan(
        &self,
        request: ExportRequest,
        project: &ExportProjectSnapshot,
    ) -> Result<ExportPlan, ServiceFeedError>;
    fn export_validate(
        &self,
        plan: &ExportPlan,
    ) -> Result<ExportValidationReport, ServiceFeedError>;
    fn export_start(
        &self,
        plan: ExportPlan,
        sink: Box<dyn ExportSink>,
        handle: ExportHandle,
        progress: Arc<dyn ExportProgressSink>,
    ) -> Result<ExportCompletion, ServiceFeedError>;
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

    fn history_restore(&self, checkpoint: CheckpointId) -> Result<RestorePlan, ServiceFeedError> {
        self.access()?
            .history(|history| history.restore(checkpoint))
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

    fn export_plan(
        &self,
        request: ExportRequest,
        project: &ExportProjectSnapshot,
    ) -> Result<ExportPlan, ServiceFeedError> {
        self.access()?
            .exporter(|exporter| exporter.plan(request, project))
            .map_err(stale_session)?
            .map_err(|error| service_error(ServiceKind::Export, error))
    }

    fn export_validate(
        &self,
        plan: &ExportPlan,
    ) -> Result<ExportValidationReport, ServiceFeedError> {
        self.access()?
            .exporter(|exporter| exporter.validate(plan))
            .map_err(stale_session)
    }

    fn export_start(
        &self,
        plan: ExportPlan,
        sink: Box<dyn ExportSink>,
        handle: ExportHandle,
        progress: Arc<dyn ExportProgressSink>,
    ) -> Result<ExportCompletion, ServiceFeedError> {
        self.access()?
            .exporter(|exporter| exporter.export(plan, sink, handle, progress))
            .map_err(stale_session)?
            .map_err(|error| service_error(ServiceKind::Export, error))
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

fn encode_hex(bytes: &[u8]) -> String {
    use fmt::Write as _;
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        write!(&mut encoded, "{byte:02x}").expect("writing to a String cannot fail");
    }
    encoded
}

fn parse_stable_id(value: &str, kind: &'static str) -> Result<[u8; 16], ServiceFeedError> {
    if value.len() != 32 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(ServiceFeedError::InvalidIdentifier {
            kind,
            value: value.to_owned(),
        });
    }
    let mut bytes = [0_u8; 16];
    for (index, byte) in bytes.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&value[index * 2..index * 2 + 2], 16).map_err(|_| {
            ServiceFeedError::InvalidIdentifier {
                kind,
                value: value.to_owned(),
            }
        })?;
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicBool, AtomicUsize};

    use parchmint_application::{DocumentVisibility, EditorRevision};
    use parchmint_domain::{ProjectId, ProjectSection};
    use parchmint_export_api::{ExportCompletion, ExportTargetCapability};
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
        recovery_revision: AtomicU64,
        export_mode: Mutex<FakeExportMode>,
    }

    #[derive(Debug, Clone, Copy, Default)]
    enum FakeExportMode {
        #[default]
        Success,
        Failure,
        PendingAfterCompletion,
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
            _: &CanonicalRelativePath,
        ) -> Result<CheckpointResource, ServiceFeedError> {
            self.check()?;
            self.history_resource
                .lock()
                .expect("history resource")
                .clone()
                .ok_or_else(|| ServiceFeedError::Service {
                    service: ServiceKind::History,
                    message: "missing fake resource".to_owned(),
                })
        }

        fn history_restore(&self, _: CheckpointId) -> Result<RestorePlan, ServiceFeedError> {
            Err(ServiceFeedError::Unsupported(
                UnsupportedBoundary::HistoryRestorePlanExecutorUnavailable,
            ))
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

        fn export_plan(
            &self,
            request: ExportRequest,
            project: &ExportProjectSnapshot,
        ) -> Result<ExportPlan, ServiceFeedError> {
            self.check()?;
            ExportPlan::build(request, project)
                .map_err(|error| service_error(ServiceKind::Export, ExportError::Validation(error)))
        }

        fn export_validate(
            &self,
            _: &ExportPlan,
        ) -> Result<ExportValidationReport, ServiceFeedError> {
            self.check()?;
            Ok(ExportValidationReport::default())
        }

        fn export_start(
            &self,
            plan: ExportPlan,
            mut sink: Box<dyn ExportSink>,
            handle: ExportHandle,
            progress: Arc<dyn ExportProgressSink>,
        ) -> Result<ExportCompletion, ServiceFeedError> {
            self.check()?;
            match *self.export_mode.lock().expect("export mode") {
                FakeExportMode::Failure => {
                    return Err(ServiceFeedError::Service {
                        service: ServiceKind::Export,
                        message: "fake write failed".to_owned(),
                    });
                }
                FakeExportMode::PendingAfterCompletion => {
                    return Ok(ExportCompletion {
                        target: plan.target().clone(),
                    });
                }
                FakeExportMode::Success => {}
            }
            progress.report(OperationProgress::Rendering {
                completed: 0,
                total: 0,
            });
            let output = handle
                .begin_temporary(sink.as_mut(), plan.target())
                .map_err(|error| service_error(ServiceKind::Export, error))?;
            let completion = output
                .finish()
                .map_err(|error| service_error(ServiceKind::Export, error))?;
            progress.report(OperationProgress::Committing);
            Ok(completion)
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

    #[derive(Default)]
    struct FakeSink;

    impl ExportSink for FakeSink {
        fn start(&mut self, _: &ExportTargetCapability) -> Result<(), ExportError> {
            Ok(())
        }

        fn write_chunk(&mut self, _: &[u8]) -> Result<(), ExportError> {
            Ok(())
        }

        fn finish(&mut self) -> Result<(), ExportError> {
            Ok(())
        }

        fn abort(&mut self) {}
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
            recorded_at_unix_millis: Some(u64::from(sequence)),
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

    fn export_project() -> ExportProjectSnapshot {
        let document = DocumentId::from_bytes([4; 16]);
        ExportProjectSnapshot::new(
            ExportStyleCatalog::new("p {}"),
            ExportDefaults::default(),
            vec![ExportNode::document(
                document,
                "Chapter",
                ExportSettings::default(),
            )],
            BTreeMap::from([(
                document,
                ExportSource {
                    revision: SourceRevision::from(7),
                    body: "<p>Body</p>".to_owned(),
                },
            )]),
        )
    }

    #[test]
    fn export_snapshot_preserves_excluded_nodes_for_planning() {
        let document = DocumentId::from_bytes([4; 16]);
        let node = NodeId::from_bytes([5; 16]);
        let mut project = Project::new(ProjectId::from_bytes([7; 16]));
        project
            .nodes
            .try_insert_document(
                node,
                document,
                ProjectSection::Manuscript.root_id(),
                0,
                "Excluded chapter",
            )
            .expect("insert document");
        project
            .nodes
            .get_mut(node)
            .expect("document node")
            .export_settings
            .excluded = true;
        let documents = [DocumentSnapshot {
            document_id: document,
            revision: EditorRevision::from(3),
            body: "preserved body".to_owned(),
            comments: Vec::new(),
            visibility: DocumentVisibility::Closed,
        }];

        let snapshot = export_snapshot_with_css(&project, &documents, String::new())
            .expect("capture full project snapshot");

        assert!(matches!(
            snapshot.manuscript.as_slice(),
            [ExportNode::Document { id, title, .. }]
                if *id == document && title == "Excluded chapter"
        ));
        assert_eq!(
            snapshot
                .sources
                .get(&document)
                .map(|source| source.body.as_str()),
            Some("preserved body")
        );
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
    fn canceled_and_old_search_generations_are_not_accepted() {
        let fake = Arc::new(FakePorts::default());
        fake.search_batches
            .lock()
            .expect("search batches")
            .extend([hit(1), hit(2)]);
        let controller = feeds(fake).search().clone();
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
        controller.cancel(2).run().expect("cancel current");
        assert!(matches!(
            controller.accept_batch(&batch),
            Err(ServiceFeedError::CanceledSearchGeneration { generation: 2 })
        ));
        drop(old);
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

        let preview = feeds(fake)
            .history_preview(
                encode_hex(checkpoint.as_bytes()),
                Some(encode_hex(document.as_bytes())),
            )
            .run()
            .expect("checkpoint document preview");
        let document = preview.document.expect("document content");
        assert_eq!(document.canonical_path, path.as_str());
        assert_eq!(document.semantic.blocks().len(), 1);
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
        assert_eq!(
            accepted.reducer_payload(),
            ProjectTaskPayload::RecoveryAccepted { revision: 18 }
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

    #[derive(Default)]
    struct FakeOutputPlatform {
        calls: Arc<AtomicUsize>,
    }

    impl ExportOutputPlatform for FakeOutputPlatform {
        fn invoke(&self, _: ExportOutputIntent) -> ServiceFuture<()> {
            let calls = self.calls.clone();
            Box::pin(async move {
                calls.fetch_add(1, Ordering::Relaxed);
                Ok(())
            })
        }
    }

    #[test]
    fn export_reports_terminal_progress_and_gates_output_intents() {
        let fake = Arc::new(FakePorts::default());
        let feeds = feeds(fake.clone());
        let plan = feeds
            .plan_export(export_request("manuscript.html", false), export_project())
            .run()
            .expect("plan");
        let validation = feeds.validate_export(plan.clone()).run().expect("validate");
        assert!(validation.is_valid());
        let start = feeds.start_export(plan, Box::new(FakeSink), 7);
        let success = start.job.run();
        assert_eq!(
            start.progress.into_iter().collect::<Vec<_>>(),
            vec![
                ExportProgress::Rendering {
                    completed: 0,
                    total: 0,
                },
                ExportProgress::Committing,
            ]
        );
        assert_eq!(
            success.as_ref().expect("success").output_name(),
            "manuscript.html"
        );
        let open = successful_output_intent(&success, ExportOutputAction::Open)
            .expect("successful open intent");

        *fake.export_mode.lock().expect("export mode") = FakeExportMode::Failure;
        let failed_plan = feeds
            .plan_export(export_request("failed.html", false), export_project())
            .run()
            .expect("failed plan");
        let failed = feeds
            .start_export(failed_plan, Box::new(FakeSink), 7)
            .job
            .run();
        assert!(matches!(failed, Err(ServiceFeedError::Service { .. })));
        assert_eq!(
            successful_output_intent(&failed, ExportOutputAction::Reveal),
            Err(ServiceFeedError::OutputUnavailable)
        );
        let platform = Arc::new(FakeOutputPlatform::default());
        iced::futures::executor::block_on(feeds.invoke_output_intent(platform.clone(), open))
            .expect("invoke successful open");
        assert_eq!(platform.calls.load(Ordering::Relaxed), 1);
        assert!(matches!(
            feeds.cancel_export(),
            Err(ServiceFeedError::OutputUnavailable)
        ));
    }

    #[test]
    fn export_completion_is_successful_when_an_adapter_leaves_its_handle_pending() {
        let fake = Arc::new(FakePorts::default());
        *fake.export_mode.lock().expect("export mode") = FakeExportMode::PendingAfterCompletion;
        let feeds = feeds(fake);
        let plan = feeds
            .plan_export(export_request("unsettled.html", false), export_project())
            .run()
            .expect("plan");

        let completed = feeds
            .start_export(plan, Box::new(FakeSink), 7)
            .job
            .run()
            .expect("completion is authoritative");

        assert_eq!(completed.output_name(), "unsettled.html");
    }
}
