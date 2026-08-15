use std::{
    collections::{BTreeMap, VecDeque},
    fs,
    future::Future,
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
    task::{Context, Poll, Wake, Waker},
    thread,
    time::Duration,
};

use parchmint_application::{
    ApplicationError, DocumentSnapshot, DocumentSnapshotLoader, DocumentVisibility,
    EditorPersistenceCoordinator, LazyDocumentSummary, NativeDocumentStateOwner,
    NativeProjectCommandDispatcher, PersistenceSaveKind, ProjectPersistenceCoordinator,
};
use parchmint_contracts::{AnnotationAnchor, AnnotationThread};
use parchmint_domain::{
    BlockId, DocumentId, NodeId, NodeKind, Project, ProjectCommand, ProjectExportSetting,
    ProjectId, apply_project_command,
};
use parchmint_editor_api::{
    BlockId as EditorBlockId, CanonicalComment, CanonicalCommentAnchor, CanonicalCommentMessage,
    CommentId, DocumentPosition, EditorRevision, EditorSelection,
};
use parchmint_editor_iced::{EditorIcedAdapter, EditorIcedConfig};
use parchmint_export_api::{
    CancelOutcome, ExportDefaults, ExportError, ExportHandle, ExportNode, ExportPlan,
    ExportProgress, ExportProgressSink, ExportRequest, ExportRunOptions, ExportSettings,
    ExportSink, ExportSource, ExportStyleCatalog, ExportValidationReport, Exporter,
    InheritedSetting, ProjectSnapshot as ExportProjectSnapshot, SourceRevision,
};
use parchmint_export_html::HtmlExporter;
use parchmint_history_api::{self as history, HistoryStore, ProjectRootCapability as HistoryRoot};
use parchmint_history_git2::Git2HistoryStore;
use parchmint_platform_api::{PathDialog, PathDialogKind, WindowCapability};
use parchmint_platform_native::{NativePlatform, iced_adapter::IcedWindowRegistry};
use parchmint_preferences::{
    AppearanceController, AppearanceMode, AppearanceService, FilePreferenceStore,
    PreferenceCoordinator, PreferenceService, ResolvedAppearance,
};
use parchmint_project_format::{
    CanonicalCodec, CanonicalDocumentSummary, CanonicalProjectPathMap, CanonicalRelativePath,
    ContentHash, ProjectFormatCodec,
};
use parchmint_project_fs::{
    FsAtomicWriter, FsProjectRepository, NativeAtomicFileOps, NativeProjectFileSystem,
    ProjectFileSystem,
};
use parchmint_project_repository::{
    CreateProject as RepositoryCreateProject, DocumentId as RepositoryDocumentId, OpenProject,
    ProjectPath, ProjectRepository, RepositoryError,
};
use parchmint_recovery_api::{self as recovery, RecoveryJournal};
use parchmint_recovery_fs::FsRecoveryJournal;
use parchmint_save::{
    CheckpointIntent, CheckpointIntentStore, CheckpointReceipt, IntentStoreError,
    ProjectSaveCoordinator, SaveCoordinator, SaveRequest, SaveStatusSnapshot, SaveTicket,
};
use parchmint_search_api::{
    self as search, RevisionId, SearchDocumentProjection, SearchField, SearchIndex,
    SearchProjectionSource, SearchProjectionVisitor, SearchTextProjection,
};
use parchmint_search_sqlite::SqliteSearchIndex;
use parchmint_spellcheck_api::{
    DictionaryRevision, ProjectId as SpellcheckProjectId, SpellcheckService,
};
use parchmint_spellcheck_en_us::{
    DictionaryLoadError, EnUsSpellcheckConfig, EnUsSpellcheckService, SavedDictionarySource,
    SpellcheckError, SpellcheckOperation,
};
use parchmint_ui_api::{
    ApplicationServices as UiApplicationServices, CreateDocumentWorkflow, DocumentSummary,
    DocumentWordCount, DuplicateSubtreesWorkflow, ExportArtifact, ExportArtifactAction,
    ExportArtifactToken, ExportOperationToken, ExportOutcome, HistoryMaintenanceStatus,
    MoveNodesWorkflow, PlatformServices as UiPlatformServices, ProjectDuplicateWorkflowSnapshot,
    ProjectExportPort, ProjectHistoryMaintenancePort, ProjectPersistencePort, ProjectQueryError,
    ProjectSaveStatus, ProjectSnapshot as UiProjectSnapshot, ProjectSnapshotQuery,
    ProjectUiProject, ProjectUiServices, ProjectWorkflowPort, ProjectWorkflowSnapshot,
};
use parchmint_ui_iced::{
    NativeCaptureRequest, NativeDesktopCallbacks, NativeDesktopError, NativeDesktopStartup,
    NativeNewProjectRequest, NativeProjectOpenResult, NativeProjectWindow, run_native_desktop,
};
use parchmint_workspace_state::FileWorkspaceStateStore;
use sha2::{Digest, Sha256};

use crate::{
    ApplicationServices, DesktopBootstrap, DesktopRuntime, DesktopStartup, DesktopUi,
    DesktopUiError, NewProjectRequest, PlatformServices, ProjectFilesystemError,
    ProjectFilesystemService, ProjectSession, RequestedProjectPath, StartupError,
    resolved_appearance,
};

/// Named production boundaries that an integration driver may fail once.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ProductionFaultPoint {
    ProjectOpen,
    FinalSave,
    Recovery,
    History,
    Search,
    Spellcheck,
    Export,
}

/// A deterministic failure selected by a complete-application test driver.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProductionFaultKind {
    Io,
    Corruption,
    Cancelled,
    WorkerStopped,
}

/// An operation observed at the production composition boundary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProductionObservation {
    ComponentReady(&'static str),
    ProjectOpened {
        path: PathBuf,
        project: ProjectId,
    },
    ProjectLocked {
        path: PathBuf,
    },
    FinalSaveReconciled {
        path: PathBuf,
    },
    FaultInjected {
        point: ProductionFaultPoint,
        kind: ProductionFaultKind,
    },
    ServiceOperation {
        point: ProductionFaultPoint,
        operation: &'static str,
        succeeded: bool,
    },
    WindowOpened {
        window: WindowCapability,
        session_id: u64,
        session_generation: u64,
        typed_ports: bool,
        native_editor: bool,
    },
    WindowFocused(WindowCapability),
    WindowRetained(WindowCapability),
    WindowClosed(WindowCapability),
    FinalSaveFailed {
        window: WindowCapability,
        reason: String,
    },
}

/// One raw measurement collected by a real runner.
///
/// Stage 38 intentionally assigns no pass/fail threshold to these values.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProductionMeasurement {
    pub operation: String,
    pub elapsed: Duration,
    pub resident_bytes: Option<u64>,
    pub platform: String,
    pub hardware_profile: Option<String>,
}

#[derive(Debug, Default)]
struct ControlState {
    faults: BTreeMap<ProductionFaultPoint, VecDeque<ProductionFaultKind>>,
    observations: Vec<ProductionObservation>,
    measurements: Vec<ProductionMeasurement>,
}

/// Shared observation, one-shot fault, and raw-measurement controls.
///
/// Production uses an empty instance. Tests must explicitly enqueue faults;
/// consuming a fault records the exact boundary and kind.
#[derive(Debug, Clone, Default)]
pub struct ProductionControls {
    state: Arc<Mutex<ControlState>>,
}

impl ProductionControls {
    pub fn fail_next(&self, point: ProductionFaultPoint, kind: ProductionFaultKind) {
        self.state
            .lock()
            .expect("production controls mutex poisoned")
            .faults
            .entry(point)
            .or_default()
            .push_back(kind);
    }

    pub fn observations(&self) -> Vec<ProductionObservation> {
        self.state
            .lock()
            .expect("production controls mutex poisoned")
            .observations
            .clone()
    }

    pub fn record_measurement(&self, measurement: ProductionMeasurement) {
        self.state
            .lock()
            .expect("production controls mutex poisoned")
            .measurements
            .push(measurement);
    }

    pub fn measurements(&self) -> Vec<ProductionMeasurement> {
        self.state
            .lock()
            .expect("production controls mutex poisoned")
            .measurements
            .clone()
    }

    fn observe(&self, observation: ProductionObservation) {
        self.state
            .lock()
            .expect("production controls mutex poisoned")
            .observations
            .push(observation);
    }

    fn take_fault(&self, point: ProductionFaultPoint) -> Option<ProductionFaultKind> {
        let mut state = self
            .state
            .lock()
            .expect("production controls mutex poisoned");
        let kind = state.faults.get_mut(&point)?.pop_front()?;
        state
            .observations
            .push(ProductionObservation::FaultInjected { point, kind });
        Some(kind)
    }

    fn service_operation(
        &self,
        point: ProductionFaultPoint,
        operation: &'static str,
        succeeded: bool,
    ) {
        self.observe(ProductionObservation::ServiceOperation {
            point,
            operation,
            succeeded,
        });
    }
}

struct ControlledHistory {
    inner: Git2HistoryStore,
    controls: ProductionControls,
}

struct ProductionHistoryMaintenance {
    history: Arc<ControlledHistory>,
    root: HistoryRoot,
    problem: Option<String>,
}

impl ProjectHistoryMaintenancePort for ProductionHistoryMaintenance {
    fn status(&self) -> Result<HistoryMaintenanceStatus, ProjectQueryError> {
        let availability = self
            .history
            .reinitialize_availability()
            .map_err(|error| ProjectQueryError::new(error.to_string()))?;
        Ok(match (self.problem.as_ref(), availability) {
            (None, _) | (_, history::HistoryReinitializeAvailability::NotNeeded) => {
                HistoryMaintenanceStatus::Available
            }
            (Some(problem), history::HistoryReinitializeAvailability::Ready { .. }) => {
                HistoryMaintenanceStatus::Reinitializable {
                    problem: problem.clone(),
                }
            }
            (Some(problem), history::HistoryReinitializeAvailability::Blocked { reason }) => {
                HistoryMaintenanceStatus::Unavailable {
                    problem: problem.clone(),
                    reason,
                }
            }
        })
    }

    fn reinitialize(&self) -> Result<String, ProjectQueryError> {
        let report = self
            .history
            .reinitialize(self.root.clone())
            .map_err(|error| ProjectQueryError::new(error.to_string()))?;
        Ok(report.preserved_history.map_or_else(
            || "History was reinitialized.".to_owned(),
            |path| format!("History was reinitialized; the damaged store was preserved at {path}."),
        ))
    }
}

impl ControlledHistory {
    fn before(&self, operation: &'static str) -> Result<(), history::HistoryError> {
        if let Some(kind) = self.controls.take_fault(ProductionFaultPoint::History) {
            self.controls
                .service_operation(ProductionFaultPoint::History, operation, false);
            return Err(history::HistoryError::Storage {
                operation,
                reason: format!("injected {kind:?} fault"),
            });
        }
        Ok(())
    }

    fn observed<T>(
        &self,
        operation: &'static str,
        result: Result<T, history::HistoryError>,
    ) -> Result<T, history::HistoryError> {
        self.controls
            .service_operation(ProductionFaultPoint::History, operation, result.is_ok());
        result
    }
}

impl HistoryStore for ControlledHistory {
    fn initialize(
        &self,
        project: HistoryRoot,
    ) -> Result<history::HistoryState, history::HistoryError> {
        self.before("initialize")?;
        self.observed("initialize", self.inner.initialize(project))
    }

    fn reinitialize_availability(
        &self,
    ) -> Result<history::HistoryReinitializeAvailability, history::HistoryError> {
        self.before("inspect reinitialization")?;
        self.observed(
            "inspect reinitialization",
            self.inner.reinitialize_availability(),
        )
    }

    fn reinitialize(
        &self,
        project: HistoryRoot,
    ) -> Result<history::HistoryReinitializeReport, history::HistoryError> {
        self.before("reinitialize")?;
        self.observed("reinitialize", self.inner.reinitialize(project))
    }

    fn checkpoint(
        &self,
        input: history::CheckpointInput,
    ) -> Result<history::CheckpointId, history::HistoryError> {
        self.before("checkpoint")?;
        self.observed("checkpoint", self.inner.checkpoint(input))
    }

    fn list(
        &self,
        query: history::HistoryPageQuery,
    ) -> Result<history::HistoryPage, history::HistoryError> {
        self.before("list")?;
        self.observed("list", self.inner.list(query))
    }

    fn preview(
        &self,
        checkpoint: history::CheckpointId,
    ) -> Result<history::SnapshotPreview, history::HistoryError> {
        self.before("preview")?;
        self.observed("preview", self.inner.preview(checkpoint))
    }

    fn read_resource(
        &self,
        checkpoint: history::CheckpointId,
        path: &history::CanonicalRelativePath,
    ) -> Result<history::CheckpointResource, history::HistoryError> {
        self.before("read resource")?;
        self.observed("read resource", self.inner.read_resource(checkpoint, path))
    }

    fn restore(
        &self,
        checkpoint: history::CheckpointId,
    ) -> Result<history::RestorePlan, history::HistoryError> {
        self.before("restore")?;
        self.observed("restore", self.inner.restore(checkpoint))
    }

    fn verify(&self) -> Result<history::HistoryIntegrityReport, history::HistoryError> {
        self.before("verify")?;
        self.observed("verify", self.inner.verify())
    }

    fn maintain(
        &self,
        budget: history::MaintenanceBudget,
    ) -> Result<history::MaintenanceReport, history::HistoryError> {
        self.before("maintain")?;
        self.observed("maintain", self.inner.maintain(budget))
    }
}

struct ControlledRecovery {
    inner: FsRecoveryJournal,
    controls: ProductionControls,
}

impl ControlledRecovery {
    fn before(&self, operation: &'static str) -> Result<(), recovery::RecoveryError> {
        if let Some(kind) = self.controls.take_fault(ProductionFaultPoint::Recovery) {
            self.controls
                .service_operation(ProductionFaultPoint::Recovery, operation, false);
            return Err(recovery::RecoveryError::Storage {
                operation,
                reason: format!("injected {kind:?} fault"),
            });
        }
        Ok(())
    }

    fn observed<T>(
        &self,
        operation: &'static str,
        result: Result<T, recovery::RecoveryError>,
    ) -> Result<T, recovery::RecoveryError> {
        self.controls
            .service_operation(ProductionFaultPoint::Recovery, operation, result.is_ok());
        result
    }

    fn before_intent(&self, operation: &'static str) -> Result<(), IntentStoreError> {
        if let Some(kind) = self.controls.take_fault(ProductionFaultPoint::Recovery) {
            self.controls
                .service_operation(ProductionFaultPoint::Recovery, operation, false);
            return Err(IntentStoreError::Storage {
                operation,
                reason: format!("injected {kind:?} fault"),
            });
        }
        Ok(())
    }
}

impl RecoveryJournal for ControlledRecovery {
    fn append(
        &self,
        batch: recovery::RecoveryBatch,
    ) -> Result<recovery::RecoveryReceipt, recovery::RecoveryError> {
        self.before("append")?;
        self.observed("append", self.inner.append(batch))
    }

    fn flush_through(
        &self,
        target: recovery::RecoveryRevisionVector,
    ) -> Result<recovery::RecoveryReceipt, recovery::RecoveryError> {
        self.before("flush")?;
        self.observed("flush", self.inner.flush_through(target))
    }

    fn inspect(&self) -> Result<recovery::RecoveryInventory, recovery::RecoveryError> {
        self.before("inspect")?;
        self.observed("inspect", self.inner.inspect())
    }

    fn replay(
        &self,
        base: recovery::RecoveryBaseSnapshot,
    ) -> Result<recovery::RecoveryReplay, recovery::RecoveryError> {
        self.before("replay")?;
        self.observed("replay", self.inner.replay(base))
    }

    fn compact(
        &self,
        durable: recovery::DurableRevisionVector,
    ) -> Result<recovery::CompactionReport, recovery::RecoveryError> {
        self.before("compact")?;
        self.observed("compact", self.inner.compact(durable))
    }

    fn discard_through(
        &self,
        durable: recovery::DurableRevisionVector,
    ) -> Result<recovery::DiscardReport, recovery::RecoveryError> {
        self.before("discard")?;
        self.observed("discard", self.inner.discard_through(durable))
    }
}

impl CheckpointIntentStore for ControlledRecovery {
    fn persist(&self, intent: CheckpointIntent) -> Result<(), IntentStoreError> {
        self.before_intent("persist intent")?;
        let result = self.inner.persist(intent);
        self.controls.service_operation(
            ProductionFaultPoint::Recovery,
            "persist intent",
            result.is_ok(),
        );
        result
    }

    fn pending(&self) -> Result<Vec<CheckpointIntent>, IntentStoreError> {
        self.before_intent("read pending intents")?;
        let result = self.inner.pending();
        self.controls.service_operation(
            ProductionFaultPoint::Recovery,
            "read pending intents",
            result.is_ok(),
        );
        result
    }

    fn complete(&self, receipt: CheckpointReceipt) -> Result<(), IntentStoreError> {
        self.before_intent("complete intent")?;
        let result = self.inner.complete(receipt);
        self.controls.service_operation(
            ProductionFaultPoint::Recovery,
            "complete intent",
            result.is_ok(),
        );
        result
    }
}

struct ControlledSearch {
    inner: SqliteSearchIndex,
    controls: ProductionControls,
    live: Mutex<Option<LiveSearchProjectionState>>,
}

struct LiveSearchProjectionSource {
    commands: Arc<NativeProjectCommandDispatcher>,
    documents: Arc<NativeDocumentStateOwner>,
}

struct LiveSearchProjectionState {
    source: Arc<LiveSearchProjectionSource>,
    indexed: BTreeMap<DocumentId, SearchDocumentProjection>,
}

impl LiveSearchProjectionSource {
    fn projections(&self) -> Result<Vec<SearchDocumentProjection>, search::SearchError> {
        let project = self
            .commands
            .project()
            .map_err(|error| search::SearchError::Source {
                reason: error.to_string(),
            })?;
        let documents =
            self.documents
                .snapshots()
                .map_err(|error| search::SearchError::Source {
                    reason: error.to_string(),
                })?;
        let documents = documents
            .into_iter()
            .map(|document| (document.document_id, document))
            .collect::<BTreeMap<_, _>>();
        project
            .nodes
            .iter()
            .filter_map(|(_, node)| match node.kind {
                NodeKind::Document(document) => Some((document, documents.get(&document))),
                NodeKind::Root(_) | NodeKind::Group => None,
            })
            .map(|(document, source)| {
                let source = source.ok_or_else(|| search::SearchError::Source {
                    reason: format!("live document {document:?} has no canonical body"),
                })?;
                Ok(SearchDocumentProjection {
                    document_id: document,
                    // Search hits are revalidated against the canonical editor
                    // revision. Project-only fields may change without advancing
                    // it; SQLite deliberately permits equal-revision replacement.
                    revision: RevisionId::from(source.revision.value()),
                    texts: search_texts(&project, document, &source.body),
                })
            })
            .collect()
    }
}

impl SearchProjectionSource for LiveSearchProjectionSource {
    fn visit_projections(
        &self,
        visitor: &mut dyn SearchProjectionVisitor,
    ) -> Result<(), search::SearchError> {
        for projection in self.projections()? {
            visitor.visit(projection)?;
        }
        Ok(())
    }
}

impl ControlledSearch {
    fn attach_live_source(
        &self,
        source: Arc<LiveSearchProjectionSource>,
        initial: Vec<SearchDocumentProjection>,
    ) -> Result<(), search::SearchError> {
        let indexed = initial
            .into_iter()
            .map(|projection| (projection.document_id, projection))
            .collect();
        *self.live.lock().map_err(|_| search::SearchError::Storage {
            operation: "attach live projection source",
            reason: "live projection state lock was poisoned".into(),
        })? = Some(LiveSearchProjectionState { source, indexed });
        Ok(())
    }

    fn refresh_live(&self) -> Result<(), search::SearchError> {
        if let search::SearchRebuildStatus::Running { generation, .. } = self.inner.rebuild_status()
        {
            self.inner.cancel(generation);
            return Err(search::SearchError::Rebuilding { generation });
        }
        let mut guard = self.live.lock().map_err(|_| search::SearchError::Storage {
            operation: "refresh live projections",
            reason: "live projection state lock was poisoned".into(),
        })?;
        let Some(live) = guard.as_mut() else {
            return Ok(());
        };
        let current = live
            .source
            .projections()?
            .into_iter()
            .map(|projection| (projection.document_id, projection))
            .collect::<BTreeMap<_, _>>();
        let removed = live
            .indexed
            .keys()
            .filter(|document| !current.contains_key(document))
            .copied()
            .collect::<Vec<_>>();
        for document in removed {
            let deletion_revision = live
                .indexed
                .get(&document)
                .map(|projection| projection.revision.value().saturating_add(1))
                .unwrap_or_default();
            let receipt = self
                .inner
                .delete_document(document, RevisionId::from(deletion_revision))?;
            if receipt.replaced {
                live.indexed.remove(&document);
            }
        }
        for (document, projection) in current {
            if live.indexed.get(&document) == Some(&projection) {
                continue;
            }
            let receipt = self.inner.replace_document(projection.clone())?;
            if !receipt.replaced {
                // A restored document can legitimately return with a canonical
                // revision older than its disposable tombstone. The live source
                // is authoritative, so rebuild instead of publishing stale hits.
                self.inner.rebuild(live.source.as_ref())?;
                live.indexed = live
                    .source
                    .projections()?
                    .into_iter()
                    .map(|projection| (projection.document_id, projection))
                    .collect();
                return Ok(());
            }
            live.indexed.insert(document, projection);
        }
        Ok(())
    }

    fn before(&self, operation: &'static str) -> Result<(), search::SearchError> {
        if let Some(kind) = self.controls.take_fault(ProductionFaultPoint::Search) {
            self.controls
                .service_operation(ProductionFaultPoint::Search, operation, false);
            return Err(search::SearchError::Storage {
                operation,
                reason: format!("injected {kind:?} fault"),
            });
        }
        Ok(())
    }

    fn observed<T>(
        &self,
        operation: &'static str,
        result: Result<T, search::SearchError>,
    ) -> Result<T, search::SearchError> {
        self.controls
            .service_operation(ProductionFaultPoint::Search, operation, result.is_ok());
        result
    }
}

impl SearchIndex for ControlledSearch {
    fn open_or_rebuild(
        &self,
        project: ProjectId,
        source: &dyn SearchProjectionSource,
    ) -> Result<search::SearchIndexState, search::SearchError> {
        self.before("open or rebuild")?;
        self.observed(
            "open or rebuild",
            self.inner.open_or_rebuild(project, source),
        )
    }

    fn open_or_rebuild_background(
        &self,
        project: ProjectId,
        source: Arc<dyn SearchProjectionSource>,
    ) -> Result<search::SearchIndexState, search::SearchError> {
        self.before("open or rebuild in background")?;
        self.observed(
            "open or rebuild in background",
            self.inner.open_or_rebuild_background(project, source),
        )
    }

    fn rebuild_status(&self) -> search::SearchRebuildStatus {
        self.inner.rebuild_status()
    }

    fn replace_document(
        &self,
        projection: SearchDocumentProjection,
    ) -> Result<search::ProjectionReceipt, search::SearchError> {
        self.before("replace document")?;
        self.observed("replace document", self.inner.replace_document(projection))
    }

    fn delete_document(
        &self,
        id: DocumentId,
        revision: RevisionId,
    ) -> Result<search::ProjectionReceipt, search::SearchError> {
        self.before("delete document")?;
        self.observed("delete document", self.inner.delete_document(id, revision))
    }

    fn query(
        &self,
        query: search::SearchQuery,
        sink: Box<dyn search::SearchBatchSink>,
    ) -> Result<(), search::SearchError> {
        self.before("query")?;
        if let search::SearchRebuildStatus::Running { generation, .. } = self.inner.rebuild_status()
        {
            return Err(search::SearchError::Rebuilding { generation });
        }
        if let Err(error) = self.refresh_live() {
            self.controls
                .service_operation(ProductionFaultPoint::Search, "query", false);
            return Err(error);
        }
        self.observed("query", self.inner.query(query, sink))
    }

    fn cancel(&self, generation: u64) {
        self.inner.cancel(generation);
        self.controls
            .service_operation(ProductionFaultPoint::Search, "cancel", true);
    }

    fn verify(&self) -> Result<search::SearchIntegrityReport, search::SearchError> {
        self.before("verify")?;
        self.observed("verify", self.inner.verify())
    }

    fn rebuild(
        &self,
        source: &dyn SearchProjectionSource,
    ) -> Result<search::RebuildReport, search::SearchError> {
        self.before("rebuild")?;
        self.observed("rebuild", self.inner.rebuild(source))
    }
}

struct ControlledExporter {
    inner: HtmlExporter,
    controls: ProductionControls,
}

impl Exporter for ControlledExporter {
    fn plan(
        &self,
        request: ExportRequest,
        project: &ExportProjectSnapshot,
    ) -> Result<ExportPlan, ExportError> {
        if let Some(kind) = self.controls.take_fault(ProductionFaultPoint::Export) {
            self.controls
                .service_operation(ProductionFaultPoint::Export, "plan", false);
            return Err(export_fault("plan", kind));
        }
        let result = self.inner.plan(request, project);
        self.controls
            .service_operation(ProductionFaultPoint::Export, "plan", result.is_ok());
        result
    }

    fn validate(&self, plan: &ExportPlan) -> ExportValidationReport {
        self.inner.validate(plan)
    }

    fn export(
        &self,
        plan: ExportPlan,
        sink: Box<dyn ExportSink>,
        handle: ExportHandle,
        progress: Arc<dyn ExportProgressSink>,
    ) -> Result<parchmint_export_api::ExportCompletion, ExportError> {
        if let Some(kind) = self.controls.take_fault(ProductionFaultPoint::Export) {
            self.controls
                .service_operation(ProductionFaultPoint::Export, "write", false);
            return Err(export_fault("write", kind));
        }
        let result = self.inner.export(plan, sink, handle, progress);
        self.controls
            .service_operation(ProductionFaultPoint::Export, "write", result.is_ok());
        result
    }

    fn cancel(&self, handle: &ExportHandle) {
        self.inner.cancel(handle);
        self.controls
            .service_operation(ProductionFaultPoint::Export, "cancel", true);
    }
}

struct SharedServices {
    editor: Arc<EditorIcedAdapter>,
    spellcheck: Arc<EnUsSpellcheckService>,
    dictionary_source: Arc<ProductionDictionarySource>,
    exporter: Arc<ControlledExporter>,
    workspace_state: Arc<FileWorkspaceStateStore>,
    preferences: Arc<dyn PreferenceService>,
    appearance: Arc<dyn AppearanceService>,
    platform: UiPlatformServices,
    controls: ProductionControls,
}

/// Reads the authoritative persisted dictionaries at reload time. Project
/// entries are resolved through their live project query; global entries come
/// from the preference store. The spellcheck worker therefore never owns a
/// second, divergent dictionary store.
struct ProductionDictionarySource {
    projects: Mutex<BTreeMap<ProjectId, Arc<dyn ProjectSnapshotQuery>>>,
    preferences: Arc<dyn PreferenceService>,
}

impl ProductionDictionarySource {
    fn new(preferences: Arc<dyn PreferenceService>) -> Self {
        Self {
            projects: Mutex::new(BTreeMap::new()),
            preferences,
        }
    }

    fn register_project(&self, project: ProjectId, query: Arc<dyn ProjectSnapshotQuery>) {
        self.projects
            .lock()
            .expect("production dictionary project registry lock")
            .insert(project, query);
    }
}

impl SavedDictionarySource for ProductionDictionarySource {
    fn project_words(
        &self,
        project: SpellcheckProjectId,
        _revision: DictionaryRevision,
    ) -> Result<Vec<String>, DictionaryLoadError> {
        let query = self
            .projects
            .lock()
            .map_err(|_| DictionaryLoadError::new("project dictionary registry lock is poisoned"))?
            .get(&project)
            .cloned()
            .ok_or_else(|| DictionaryLoadError::new("project dictionary source is unavailable"))?;
        let snapshot = query
            .snapshot()
            .map_err(|error| DictionaryLoadError::new(error.to_string()))?;
        Ok(snapshot
            .project
            .dictionary
            .iter()
            .map(str::to_owned)
            .collect())
    }

    fn global_words(
        &self,
        _revision: DictionaryRevision,
    ) -> Result<Vec<String>, DictionaryLoadError> {
        block_on(self.preferences.load())
            .map(|snapshot| snapshot.values.global_dictionary)
            .map_err(|error| DictionaryLoadError::new(error.to_string()))
    }
}

/// Concrete application-wide services retained by the production bootstrap.
pub struct ProductionApplicationGraph {
    shared: Arc<SharedServices>,
}

impl ProductionApplicationGraph {
    pub fn controls(&self) -> &ProductionControls {
        &self.shared.controls
    }

    pub fn editor(&self) -> Arc<EditorIcedAdapter> {
        Arc::clone(&self.shared.editor)
    }

    pub fn spellcheck(&self) -> Arc<dyn SpellcheckService> {
        self.shared.spellcheck.clone()
    }

    pub fn exporter(&self) -> Arc<dyn Exporter> {
        self.shared.exporter.clone()
    }

    pub fn check_spelling(
        &self,
        request: parchmint_spellcheck_api::SpellcheckRequest,
    ) -> SpellcheckOperation<parchmint_spellcheck_api::SpellcheckResultStream> {
        if let Some(kind) = self
            .shared
            .controls
            .take_fault(ProductionFaultPoint::Spellcheck)
        {
            self.shared.controls.service_operation(
                ProductionFaultPoint::Spellcheck,
                "check",
                false,
            );
            return Box::pin(async move { Err(spellcheck_fault(kind)) });
        }
        self.shared.controls.service_operation(
            ProductionFaultPoint::Spellcheck,
            "check scheduled",
            true,
        );
        self.shared.spellcheck.check(request)
    }

    pub fn suggest_spelling(
        &self,
        request: parchmint_spellcheck_api::SuggestionRequest,
    ) -> SpellcheckOperation<Vec<parchmint_spellcheck_api::SpellingSuggestion>> {
        if let Some(kind) = self
            .shared
            .controls
            .take_fault(ProductionFaultPoint::Spellcheck)
        {
            self.shared.controls.service_operation(
                ProductionFaultPoint::Spellcheck,
                "suggest",
                false,
            );
            return Box::pin(async move { Err(spellcheck_fault(kind)) });
        }
        self.shared.controls.service_operation(
            ProductionFaultPoint::Spellcheck,
            "suggest scheduled",
            true,
        );
        self.shared.spellcheck.suggest(request)
    }

    pub fn reload_project_dictionary(
        &self,
        project: ProjectId,
        revision: parchmint_spellcheck_api::DictionaryRevision,
    ) -> SpellcheckOperation<()> {
        if let Some(kind) = self
            .shared
            .controls
            .take_fault(ProductionFaultPoint::Spellcheck)
        {
            self.shared.controls.service_operation(
                ProductionFaultPoint::Spellcheck,
                "reload project dictionary",
                false,
            );
            return Box::pin(async move { Err(spellcheck_fault(kind)) });
        }
        self.shared.controls.service_operation(
            ProductionFaultPoint::Spellcheck,
            "reload project dictionary scheduled",
            true,
        );
        self.shared
            .spellcheck
            .reload_project_dictionary(project, revision)
    }

    pub fn reload_global_dictionary(
        &self,
        revision: parchmint_spellcheck_api::DictionaryRevision,
    ) -> SpellcheckOperation<()> {
        if let Some(kind) = self
            .shared
            .controls
            .take_fault(ProductionFaultPoint::Spellcheck)
        {
            self.shared.controls.service_operation(
                ProductionFaultPoint::Spellcheck,
                "reload global dictionary",
                false,
            );
            return Box::pin(async move { Err(spellcheck_fault(kind)) });
        }
        self.shared.controls.service_operation(
            ProductionFaultPoint::Spellcheck,
            "reload global dictionary scheduled",
            true,
        );
        self.shared.spellcheck.reload_global_dictionary(revision)
    }

    pub fn workspace_state(&self) -> Arc<FileWorkspaceStateStore> {
        Arc::clone(&self.shared.workspace_state)
    }
}

/// Concrete services scoped to one exact writable project lease.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProductionHistoryStatus {
    Available {
        checkpoint_count: usize,
    },
    Unavailable {
        problem: String,
        reinitialize: history::HistoryReinitializeAvailability,
    },
}

pub struct ProductionProjectSession {
    path: PathBuf,
    project_id: ProjectId,
    commands: Arc<NativeProjectCommandDispatcher>,
    history: Arc<ControlledHistory>,
    history_status: ProductionHistoryStatus,
    recovery: Arc<ControlledRecovery>,
    search: Arc<ControlledSearch>,
    save: Arc<ProjectSaveCoordinator>,
    persistence: Arc<EditorPersistenceCoordinator>,
    project_persistence: Arc<ProjectPersistenceCoordinator>,
    query: Arc<ProductionProjectQuery>,
    ui_services: ProjectUiServices,
    _repository: Arc<FsProjectRepository>,
    _open_project: OpenProject,
    controls: ProductionControls,
}

struct ProductionProjectQuery {
    commands: Arc<NativeProjectCommandDispatcher>,
    documents: Arc<NativeDocumentStateOwner>,
    persistence: Arc<ProjectPersistenceCoordinator>,
    persisted_summaries: BTreeMap<DocumentId, CanonicalDocumentSummary>,
    document_loader: Arc<CanonicalDocumentLoader>,
    search: Arc<ControlledSearch>,
}

impl ProjectSnapshotQuery for ProductionProjectQuery {
    fn snapshot(&self) -> Result<UiProjectSnapshot, ProjectQueryError> {
        let project = self.commands.project().map_err(map_project_query_error)?;
        let documents = self
            .documents
            .loaded_snapshots()
            .map_err(map_project_query_error)?;
        let loaded = documents
            .iter()
            .map(|document| (document.document_id, document))
            .collect::<BTreeMap<_, _>>();
        let document_summaries = self
            .documents
            .summaries()
            .map_err(map_project_query_error)?
            .into_iter()
            .map(|summary| {
                let persisted = self
                    .persisted_summaries
                    .get(&summary.document_id)
                    .cloned()
                    .or_else(|| self.document_loader.hydrated_summary(summary.document_id));
                let loaded = loaded.get(&summary.document_id);
                DocumentSummary {
                    document_id: summary.document_id,
                    revision: summary.revision,
                    visibility: summary.visibility,
                    content_hash: loaded
                        .is_none()
                        .then(|| {
                            persisted
                                .as_ref()
                                .map(|summary| *summary.content_hash.as_bytes())
                        })
                        .flatten(),
                    word_count: loaded.map_or_else(
                        || {
                            persisted
                                .as_ref()
                                .map_or(DocumentWordCount::Pending, |summary| {
                                    DocumentWordCount::Known(summary.word_count)
                                })
                        },
                        |document| {
                            DocumentWordCount::Known(document.body.split_whitespace().count())
                        },
                    ),
                }
            })
            .collect();
        Ok(UiProjectSnapshot {
            project,
            document_summaries,
            documents,
            styles_css: self
                .persistence
                .canonical_text("styles.css")
                .map_err(map_project_query_error)?
                .unwrap_or_default(),
        })
    }

    fn load_document(&self, document: DocumentId) -> Result<DocumentSnapshot, ProjectQueryError> {
        let snapshot = self.documents.snapshot(document).map_err(|error| {
            let revision = self
                .documents
                .summaries()
                .ok()
                .and_then(|summaries| {
                    summaries
                        .into_iter()
                        .find(|summary| summary.document_id == document)
                })
                .map(|summary| RevisionId::from(summary.revision.value()))
                .unwrap_or_default();
            let _ = self.search.delete_document(document, revision);
            map_project_query_error(error)
        })?;
        if let Some(hash) = self.document_loader.recovery_hash(document) {
            self.persistence
                .register_loaded_document_base(document, snapshot.revision, hash)
                .map_err(map_project_query_error)?;
        }
        Ok(snapshot)
    }

    fn snapshot_for_export(&self) -> Result<UiProjectSnapshot, ProjectQueryError> {
        let mut snapshot = self.snapshot()?;
        snapshot.documents = self
            .documents
            .snapshots()
            .map_err(map_project_query_error)?;
        Ok(snapshot)
    }
}

fn map_project_query_error(error: impl std::fmt::Display) -> ProjectQueryError {
    ProjectQueryError::new(error.to_string())
}

struct ProductionSaveStatus {
    save: Arc<ProjectSaveCoordinator>,
}

impl ProjectSaveStatus for ProductionSaveStatus {
    fn status(&self) -> SaveStatusSnapshot {
        self.save.status()
    }
}

struct SearchRefreshingCommands {
    inner: Arc<NativeProjectCommandDispatcher>,
    search: Arc<ControlledSearch>,
}

impl parchmint_application::ProjectCommandDispatcher for SearchRefreshingCommands {
    fn execute(
        &self,
        command: ProjectCommand,
    ) -> parchmint_application::AppFuture<
        '_,
        Result<
            parchmint_application::ProjectCommandResult,
            parchmint_application::ApplicationError,
        >,
    > {
        Box::pin(async move {
            let result = parchmint_application::ProjectCommandDispatcher::execute(
                self.inner.as_ref(),
                command,
            )
            .await;
            if result.is_ok() {
                let _ = self.search.refresh_live();
            }
            result
        })
    }

    fn undo(
        &self,
    ) -> parchmint_application::AppFuture<
        '_,
        Result<
            parchmint_application::ProjectCommandResult,
            parchmint_application::ApplicationError,
        >,
    > {
        Box::pin(async move {
            let result =
                parchmint_application::ProjectCommandDispatcher::undo(self.inner.as_ref()).await;
            if result.is_ok() {
                let _ = self.search.refresh_live();
            }
            result
        })
    }

    fn redo(
        &self,
    ) -> parchmint_application::AppFuture<
        '_,
        Result<
            parchmint_application::ProjectCommandResult,
            parchmint_application::ApplicationError,
        >,
    > {
        Box::pin(async move {
            let result =
                parchmint_application::ProjectCommandDispatcher::redo(self.inner.as_ref()).await;
            if result.is_ok() {
                let _ = self.search.refresh_live();
            }
            result
        })
    }

    fn undo_state(&self) -> parchmint_application::ProjectUndoState {
        parchmint_application::ProjectCommandDispatcher::undo_state(self.inner.as_ref())
    }

    fn reset_undo(&self, reason: parchmint_application::UndoResetReason) {
        parchmint_application::ProjectCommandDispatcher::reset_undo(self.inner.as_ref(), reason);
    }
}

impl parchmint_application::GlobalReplacement for SearchRefreshingCommands {
    fn preview(
        &self,
        selection: parchmint_application::ReplacementSelection,
    ) -> parchmint_application::AppFuture<
        '_,
        Result<parchmint_application::ReplacementPreview, parchmint_application::ApplicationError>,
    > {
        parchmint_application::GlobalReplacement::preview(self.inner.as_ref(), selection)
    }

    fn apply(
        &self,
        selection: parchmint_application::ReplacementSelection,
    ) -> parchmint_application::AppFuture<
        '_,
        Result<
            parchmint_application::ProjectCommandResult,
            parchmint_application::ApplicationError,
        >,
    > {
        Box::pin(async move {
            let result =
                parchmint_application::GlobalReplacement::apply(self.inner.as_ref(), selection)
                    .await;
            if result.is_ok() {
                let _ = self.search.refresh_live();
            }
            result
        })
    }
}

struct SearchRefreshingPersistence {
    inner: Arc<ProjectPersistenceCoordinator>,
    search: Arc<ControlledSearch>,
}

impl ProjectPersistencePort for SearchRefreshingPersistence {
    fn persist_editor_projection(
        &self,
        projection: parchmint_ui_api::CanonicalProjection,
    ) -> Result<parchmint_ui_api::DurableProjectionAck, parchmint_ui_api::ProjectPersistenceError>
    {
        let result = self.inner.persist_editor_projection(projection);
        if result.is_ok() {
            let _ = self.search.refresh_live();
        }
        result
    }

    fn request_save(
        &self,
        kind: parchmint_ui_api::ProjectSaveKind,
    ) -> Result<
        (
            parchmint_ui_api::ProjectSaveHandle,
            parchmint_ui_api::ProjectPersistenceRevision,
        ),
        parchmint_ui_api::ProjectPersistenceError,
    > {
        self.inner.request_save(kind)
    }

    fn await_save(
        &self,
        handle: parchmint_ui_api::ProjectSaveHandle,
    ) -> Result<parchmint_ui_api::SavedProjectRevision, parchmint_ui_api::ProjectPersistenceError>
    {
        let result = self.inner.await_save(handle);
        if result.is_ok() {
            let _ = self.search.refresh_live();
        }
        result
    }

    fn status(&self) -> parchmint_ui_api::ProjectPersistenceStatus {
        self.inner.status()
    }

    fn reconcile_recovery(
        &self,
    ) -> Result<parchmint_ui_api::ProjectRecoveryState, parchmint_ui_api::ProjectPersistenceError>
    {
        self.inner.reconcile_recovery()
    }

    fn accept_recovery(
        &self,
        acceptance: parchmint_ui_api::ProjectRecoveryAcceptance,
    ) -> Result<parchmint_ui_api::ProjectRecoveryState, parchmint_ui_api::ProjectPersistenceError>
    {
        let result = self.inner.accept_recovery(acceptance);
        if result.is_ok() {
            let _ = self.search.refresh_live();
        }
        result
    }

    fn discard_recovery(
        &self,
        acceptance: parchmint_ui_api::ProjectRecoveryAcceptance,
    ) -> Result<parchmint_ui_api::ProjectRecoveryState, parchmint_ui_api::ProjectPersistenceError>
    {
        self.inner.discard_recovery(acceptance)
    }
}

struct ProductionProjectWorkflows {
    history: Arc<ControlledHistory>,
    persistence: Arc<ProjectPersistenceCoordinator>,
    query: Arc<ProductionProjectQuery>,
    exporter: Arc<ControlledExporter>,
    search: Arc<ControlledSearch>,
    artifacts: Mutex<BTreeMap<ExportArtifactToken, PathBuf>>,
    next_artifact: AtomicU64,
    active_export: Mutex<Option<ActiveExportOperation>>,
    next_export_operation: AtomicU64,
}

#[derive(Clone)]
struct ActiveExportOperation {
    token: ExportOperationToken,
    handle: ExportHandle,
    progress: Arc<dyn ExportProgressSink>,
}

impl ProjectWorkflowPort for ProductionProjectWorkflows {
    fn create_document(
        &self,
        request: CreateDocumentWorkflow,
    ) -> Result<ProjectWorkflowSnapshot, ProjectQueryError> {
        let saved = self
            .persistence
            .create_document(request)
            .map_err(map_project_workflow_error)?;
        self.refresh_search();
        Ok(ProjectWorkflowSnapshot {
            snapshot: self.query.snapshot()?,
            checkpoint: saved.revision.checkpoint,
        })
    }

    fn restore_checkpoint(
        &self,
        checkpoint: parchmint_domain::CheckpointId,
    ) -> Result<ProjectWorkflowSnapshot, ProjectQueryError> {
        let plan = self
            .history
            .restore(checkpoint)
            .map_err(|error| ProjectQueryError::new(error.to_string()))?;
        let restored = self
            .persistence
            .restore_history(plan)
            .map_err(map_project_workflow_error)?;
        self.refresh_search();
        Ok(ProjectWorkflowSnapshot {
            snapshot: self.query.snapshot()?,
            checkpoint: restored.revision.checkpoint,
        })
    }

    fn create_named_snapshot(
        &self,
        name: String,
    ) -> Result<ProjectWorkflowSnapshot, ProjectQueryError> {
        let saved = self
            .persistence
            .create_named_snapshot(name)
            .map_err(map_project_workflow_error)?;
        Ok(ProjectWorkflowSnapshot {
            snapshot: self.query.snapshot()?,
            checkpoint: saved.checkpoint,
        })
    }

    fn delete_subtrees(
        &self,
        request: parchmint_ui_api::DeleteSubtreesWorkflow,
    ) -> Result<ProjectWorkflowSnapshot, ProjectQueryError> {
        let deleted = self
            .persistence
            .delete_subtrees(request)
            .map_err(map_project_workflow_error)?;
        self.refresh_search();
        Ok(ProjectWorkflowSnapshot {
            snapshot: self.query.snapshot()?,
            checkpoint: deleted.revision.checkpoint,
        })
    }

    fn move_nodes(
        &self,
        request: MoveNodesWorkflow,
    ) -> Result<ProjectWorkflowSnapshot, ProjectQueryError> {
        let saved = self
            .persistence
            .move_nodes(request)
            .map_err(map_project_workflow_error)?;
        self.refresh_search();
        Ok(ProjectWorkflowSnapshot {
            snapshot: self.query.snapshot()?,
            checkpoint: saved.checkpoint,
        })
    }

    fn duplicate_subtrees(
        &self,
        request: DuplicateSubtreesWorkflow,
    ) -> Result<ProjectDuplicateWorkflowSnapshot, ProjectQueryError> {
        let duplicated = self
            .persistence
            .duplicate_subtrees(request)
            .map_err(map_project_workflow_error)?;
        self.refresh_search();
        Ok(ProjectDuplicateWorkflowSnapshot {
            workflow: ProjectWorkflowSnapshot {
                snapshot: self.query.snapshot()?,
                checkpoint: duplicated.revision.checkpoint,
            },
            created_roots: duplicated.created_roots,
            node_ids: duplicated.node_ids,
            document_ids: duplicated.document_ids,
        })
    }
}

impl ProjectExportPort for ProductionProjectWorkflows {
    fn begin_export(
        &self,
        progress: Arc<dyn ExportProgressSink>,
    ) -> Result<ExportOperationToken, ProjectQueryError> {
        let token = ExportOperationToken::from_raw(
            self.next_export_operation.fetch_add(1, Ordering::Relaxed),
        );
        let operation = ActiveExportOperation {
            token,
            handle: ExportHandle::new(),
            progress,
        };
        let replaced = self
            .active_export
            .lock()
            .map_err(|_| ProjectQueryError::new("export operation registry is unavailable"))?
            .replace(operation);
        if let Some(replaced) = replaced {
            let _ = replaced.handle.cancel();
        }
        Ok(token)
    }

    fn export_to_path(
        &self,
        operation: ExportOperationToken,
        selection: parchmint_platform_api::UntrustedPathSelection,
        options: ExportRunOptions,
    ) -> Result<ExportOutcome, ProjectQueryError> {
        let active = self.active_export(operation)?;
        active.progress.report(ExportProgress::Planning);
        let result = (|| {
            if active.handle.status() == parchmint_export_api::ExportStatus::Cancelled {
                return Ok(ExportOutcome::Cancelled);
            }
            let snapshot = self.query.snapshot_for_export()?;
            let project = export_snapshot(&snapshot)?;
            let (sink, output_name, completed_path) =
                NativeExportSink::acquire(selection.as_path()).map_err(map_export_error)?;
            let request = ExportRequest::new(output_name.clone(), options);
            let plan = self.exporter.plan(request, &project).map_err(|error| {
                active.handle.fail();
                map_export_error(error)
            })?;
            let report = self.exporter.validate(&plan);
            if !report.is_valid() {
                active.handle.fail();
                return Err(map_export_error(ExportError::Validation(report)));
            }
            match self.exporter.export(
                plan,
                Box::new(sink),
                active.handle.clone(),
                Arc::clone(&active.progress),
            ) {
                Ok(_) => {}
                Err(ExportError::Cancelled) => return Ok(ExportOutcome::Cancelled),
                Err(error) => return Err(map_export_error(error)),
            }
            if !self.export_is_active(operation)? {
                return Err(ProjectQueryError::new("export operation is stale"));
            }
            let artifact =
                ExportArtifactToken::from_raw(self.next_artifact.fetch_add(1, Ordering::Relaxed));
            self.artifacts
                .lock()
                .map_err(|_| ProjectQueryError::new("export artifact registry is unavailable"))?
                .insert(artifact, completed_path);
            Ok(ExportOutcome::Completed(ExportArtifact {
                token: artifact,
                display_name: output_name,
            }))
        })();
        self.clear_export_if_active(operation)?;
        result
    }

    fn cancel_export(
        &self,
        operation: ExportOperationToken,
    ) -> Result<CancelOutcome, ProjectQueryError> {
        let active = self.active_export(operation)?;
        Ok(active.handle.cancel())
    }

    fn act_on_artifact(
        &self,
        artifact: ExportArtifactToken,
        action: ExportArtifactAction,
    ) -> Result<(), ProjectQueryError> {
        let path = self
            .artifacts
            .lock()
            .map_err(|_| ProjectQueryError::new("export artifact registry is unavailable"))?
            .get(&artifact)
            .cloned()
            .ok_or_else(|| ProjectQueryError::new("export artifact token is unknown"))?;
        open_export_artifact(&path, action)
    }
}

impl ProductionProjectWorkflows {
    fn refresh_search(&self) {
        let _ = self.search.refresh_live();
    }

    fn active_export(
        &self,
        token: ExportOperationToken,
    ) -> Result<ActiveExportOperation, ProjectQueryError> {
        self.active_export
            .lock()
            .map_err(|_| ProjectQueryError::new("export operation registry is unavailable"))?
            .as_ref()
            .filter(|active| active.token == token)
            .cloned()
            .ok_or_else(|| ProjectQueryError::new("export operation is stale"))
    }

    fn export_is_active(&self, token: ExportOperationToken) -> Result<bool, ProjectQueryError> {
        Ok(self
            .active_export
            .lock()
            .map_err(|_| ProjectQueryError::new("export operation registry is unavailable"))?
            .as_ref()
            .is_some_and(|active| active.token == token))
    }

    fn clear_export_if_active(&self, token: ExportOperationToken) -> Result<(), ProjectQueryError> {
        let mut active = self
            .active_export
            .lock()
            .map_err(|_| ProjectQueryError::new("export operation registry is unavailable"))?;
        if active.as_ref().is_some_and(|active| active.token == token) {
            *active = None;
        }
        Ok(())
    }
}

fn map_project_workflow_error(error: impl std::fmt::Display) -> ProjectQueryError {
    ProjectQueryError::new(error.to_string())
}

fn map_export_error(error: ExportError) -> ProjectQueryError {
    ProjectQueryError::new(error.to_string())
}

struct NativeExportSink {
    target: PathBuf,
    temporary: PathBuf,
    file: Option<fs::File>,
    expected_name: String,
    started: bool,
}

impl NativeExportSink {
    fn acquire(path: &Path) -> Result<(Self, String, PathBuf), ExportError> {
        if !path.is_absolute() {
            return Err(export_sink_error(
                "authorize",
                "target must be an absolute path",
            ));
        }
        let parent = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .ok_or_else(|| export_sink_error("authorize", "target has no parent directory"))?;
        let parent_metadata = fs::symlink_metadata(parent)
            .map_err(|error| export_sink_error("authorize", error.to_string()))?;
        if !parent_metadata.is_dir() || parent_metadata.file_type().is_symlink() {
            return Err(export_sink_error(
                "authorize",
                "target parent is not a direct directory",
            ));
        }
        if let Ok(metadata) = fs::symlink_metadata(path)
            && (!metadata.is_file() || metadata.file_type().is_symlink())
        {
            return Err(export_sink_error(
                "authorize",
                "existing target is not a direct regular file",
            ));
        }
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .filter(|name| !name.is_empty())
            .ok_or_else(|| export_sink_error("authorize", "target filename is invalid"))?
            .to_owned();
        // The export planner accepts a portable name, never the OS path.
        parchmint_export_api::ExportTargetCapability::checked(&name)
            .map_err(|issue| ExportError::Validation(ExportValidationReport::from_issue(issue)))?;
        static NEXT_TEMPORARY: AtomicU64 = AtomicU64::new(1);
        let sequence = NEXT_TEMPORARY.fetch_add(1, Ordering::Relaxed);
        let temporary = parent.join(format!(
            ".{name}.parchmint-export-{}-{sequence}.tmp",
            std::process::id()
        ));
        let file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .map_err(|error| export_sink_error("authorize", error.to_string()))?;
        Ok((
            Self {
                target: path.to_path_buf(),
                temporary,
                file: Some(file),
                expected_name: name.clone(),
                started: false,
            },
            name,
            path.to_path_buf(),
        ))
    }
}

impl ExportSink for NativeExportSink {
    fn start(
        &mut self,
        target: &parchmint_export_api::ExportTargetCapability,
    ) -> Result<(), ExportError> {
        if self.started || target.name().as_str() != self.expected_name {
            return Err(ExportError::InvalidState);
        }
        self.started = true;
        Ok(())
    }

    fn write_chunk(&mut self, bytes: &[u8]) -> Result<(), ExportError> {
        if !self.started {
            return Err(ExportError::InvalidState);
        }
        self.file
            .as_mut()
            .ok_or(ExportError::InvalidState)?
            .write_all(bytes)
            .map_err(|error| export_sink_error("write", error.to_string()))
    }

    fn finish(&mut self) -> Result<(), ExportError> {
        let mut file = self.file.take().ok_or(ExportError::InvalidState)?;
        file.flush()
            .and_then(|_| file.sync_all())
            .map_err(|error| export_sink_error("finish", error.to_string()))?;
        drop(file);
        fs::rename(&self.temporary, &self.target)
            .map_err(|error| export_sink_error("finish", error.to_string()))
    }

    fn abort(&mut self) {
        self.file.take();
        let _ = fs::remove_file(&self.temporary);
    }
}

impl Drop for NativeExportSink {
    fn drop(&mut self) {
        if self.file.is_some() {
            self.abort();
        }
    }
}

fn export_sink_error(operation: &'static str, reason: impl Into<String>) -> ExportError {
    ExportError::Sink {
        operation,
        reason: reason.into(),
    }
}

fn export_snapshot(
    snapshot: &UiProjectSnapshot,
) -> Result<ExportProjectSnapshot, ProjectQueryError> {
    let sources = snapshot
        .documents
        .iter()
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
    let manuscript = export_nodes(&snapshot.project, NodeId::manuscript_root())?;
    let research = export_nodes(&snapshot.project, NodeId::research_root())?;
    let mut project = ExportProjectSnapshot::new(
        ExportStyleCatalog::new(snapshot.styles_css.clone()),
        ExportDefaults {
            emit_titles: snapshot.project.export_settings.emit_titles
                != ProjectExportSetting::Disabled,
            start_new_page: snapshot.project.export_settings.starts_new_page,
        },
        manuscript,
        sources,
    );
    project.research = research;
    Ok(project)
}

fn export_nodes(project: &Project, parent: NodeId) -> Result<Vec<ExportNode>, ProjectQueryError> {
    project
        .nodes
        .children(parent)
        .iter()
        .filter_map(|id| {
            let node = project.nodes.get(*id)?;
            let settings = ExportSettings {
                emit_titles: export_setting(node.export_settings.emit_titles),
                start_new_page: if node.export_settings.starts_new_page {
                    InheritedSetting::Enabled
                } else {
                    InheritedSetting::Inherit
                },
            };
            Some(match node.kind {
                NodeKind::Document(document) => {
                    Ok(ExportNode::document(document, node.title.clone(), settings))
                }
                NodeKind::Group => export_nodes(project, *id)
                    .map(|children| ExportNode::group(node.title.clone(), settings, children)),
                NodeKind::Root(_) => Err(ProjectQueryError::new(
                    "project section contains a nested root",
                )),
            })
        })
        .collect()
}

fn export_setting(setting: ProjectExportSetting) -> InheritedSetting {
    match setting {
        ProjectExportSetting::Inherit => InheritedSetting::Inherit,
        ProjectExportSetting::Enabled => InheritedSetting::Enabled,
        ProjectExportSetting::Disabled => InheritedSetting::Disabled,
    }
}

fn open_export_artifact(
    path: &Path,
    action: ExportArtifactAction,
) -> Result<(), ProjectQueryError> {
    let mut command = if cfg!(target_os = "macos") {
        let mut command = Command::new("open");
        if action == ExportArtifactAction::Reveal {
            command.arg("-R");
        }
        command.arg(path);
        command
    } else if cfg!(target_os = "windows") {
        let mut command = Command::new("explorer");
        if action == ExportArtifactAction::Reveal {
            command.arg(format!("/select,{}", path.display()));
        } else {
            command.arg(path);
        }
        command
    } else {
        let mut command = Command::new("xdg-open");
        command.arg(if action == ExportArtifactAction::Reveal {
            path.parent().unwrap_or(path)
        } else {
            path
        });
        command
    };
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map(|_| ())
        .map_err(|error| ProjectQueryError::new(format!("open export artifact failed: {error}")))
}

impl ProductionProjectSession {
    pub const fn project_id(&self) -> ProjectId {
        self.project_id
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn commands(&self) -> Arc<NativeProjectCommandDispatcher> {
        Arc::clone(&self.commands)
    }

    pub fn history(&self) -> Arc<dyn HistoryStore> {
        self.history.clone()
    }

    pub fn history_status(&self) -> &ProductionHistoryStatus {
        &self.history_status
    }

    pub fn recovery(&self) -> Arc<dyn RecoveryJournal> {
        self.recovery.clone()
    }

    pub fn search(&self) -> Arc<dyn SearchIndex> {
        self.search.clone()
    }

    pub fn save(&self) -> &dyn SaveCoordinator {
        self.save.as_ref()
    }

    pub fn persistence(&self) -> Arc<EditorPersistenceCoordinator> {
        Arc::clone(&self.persistence)
    }

    pub fn project_persistence(&self) -> Arc<ProjectPersistenceCoordinator> {
        Arc::clone(&self.project_persistence)
    }

    pub fn ui_snapshot(&self) -> Result<UiProjectSnapshot, ProjectQueryError> {
        self.query.snapshot()
    }

    pub fn request_save(&self, request: SaveRequest) -> Result<SaveTicket, ProjectFilesystemError> {
        if let Some(kind) = self.controls.take_fault(ProductionFaultPoint::FinalSave) {
            self.controls
                .service_operation(ProductionFaultPoint::FinalSave, "request save", false);
            return Err(injected_failure("save", kind));
        }
        let result = self
            .save
            .request(request)
            .map_err(|error| ProjectFilesystemError::failed("save", error.to_string()));
        self.controls.service_operation(
            ProductionFaultPoint::FinalSave,
            "request save",
            result.is_ok(),
        );
        result
    }

    fn reconcile_final_save(&self) -> Result<(), ProjectFilesystemError> {
        if let Some(kind) = self.controls.take_fault(ProductionFaultPoint::FinalSave) {
            self.controls.service_operation(
                ProductionFaultPoint::FinalSave,
                "reconcile final save",
                false,
            );
            return Err(injected_failure("save", kind));
        }
        if !self
            .project_persistence
            .has_unsaved_changes()
            .map_err(|error| {
                ProjectFilesystemError::failed("inspect final save", error.to_string())
            })?
        {
            self.controls
                .observe(ProductionObservation::FinalSaveReconciled {
                    path: self.path.clone(),
                });
            self.controls.service_operation(
                ProductionFaultPoint::FinalSave,
                "reconcile final save",
                true,
            );
            return Ok(());
        }
        let (handle, _) = match self
            .project_persistence
            .request_save(PersistenceSaveKind::Final)
        {
            Ok(request) => request,
            Err(error) => {
                self.controls.service_operation(
                    ProductionFaultPoint::FinalSave,
                    "reconcile final save",
                    false,
                );
                return Err(ProjectFilesystemError::failed(
                    "capture final save",
                    error.to_string(),
                ));
            }
        };
        if let Err(error) = self.project_persistence.await_save(handle) {
            self.controls.service_operation(
                ProductionFaultPoint::FinalSave,
                "reconcile final save",
                false,
            );
            return Err(ProjectFilesystemError::failed("save", error.to_string()));
        }
        self.controls
            .observe(ProductionObservation::FinalSaveReconciled {
                path: self.path.clone(),
            });
        self.controls.service_operation(
            ProductionFaultPoint::FinalSave,
            "reconcile final save",
            true,
        );
        Ok(())
    }
}

struct ProductionProjectFilesystem {
    shared: Arc<SharedServices>,
}

impl ProjectFilesystemService for ProductionProjectFilesystem {
    fn create(
        &self,
        request: &NewProjectRequest,
    ) -> Result<Box<dyn ProjectSession>, ProjectFilesystemError> {
        let repository = FsProjectRepository::native();
        let manifest = new_project_manifest(&request.title, request.author.as_deref());
        let created = repository
            .create(RepositoryCreateProject {
                path: ProjectPath::new(&request.destination),
                manifest,
                documents: BTreeMap::from([(
                    RepositoryDocumentId::new("untitled-document"),
                    b"<p></p>".to_vec(),
                )]),
            })
            .map_err(map_repository_error)?;
        drop(created);
        drop(repository);
        self.open(&RequestedProjectPath::new(&request.destination))
    }

    fn open(
        &self,
        requested: &RequestedProjectPath,
    ) -> Result<Box<dyn ProjectSession>, ProjectFilesystemError> {
        if let Some(kind) = self
            .shared
            .controls
            .take_fault(ProductionFaultPoint::ProjectOpen)
        {
            self.shared.controls.service_operation(
                ProductionFaultPoint::ProjectOpen,
                "open",
                false,
            );
            return Err(injected_failure("open", kind));
        }

        let path = requested.as_path().to_path_buf();
        let repository = Arc::new(FsProjectRepository::native());
        let open_project = repository
            .open(ProjectPath::new(&path))
            .map_err(map_repository_error)?;
        let root = repository.active_root().ok_or_else(|| {
            ProjectFilesystemError::failed("open", "validated root capability was not retained")
        })?;
        let root_path = root
            .checked_path()
            .map_err(|error| ProjectFilesystemError::failed("authorize root", error.to_string()))?
            .to_path_buf();
        let resources = canonical_resources(&root)?;
        let project_id = project_id(&root, &resources.metadata);
        let (
            project,
            document_summaries,
            document_loader,
            search_source,
            canonical_paths,
            persistence_frontier,
        ) = application_state(project_id, root.clone(), &resources)?;
        let document_owner = Arc::new(
            NativeDocumentStateOwner::new_lazy(document_summaries.clone(), document_loader.clone())
                .map_err(|error| {
                    ProjectFilesystemError::failed(
                        "initialize document sessions",
                        error.to_string(),
                    )
                })?,
        );
        let initial_document = document_summaries
            .iter()
            .find(|summary| summary.visibility == DocumentVisibility::Open)
            .map(|summary| summary.document_id);
        if let Some(initial) = initial_document {
            document_owner.snapshot(initial).map_err(|error| {
                ProjectFilesystemError::failed("load initial document", error.to_string())
            })?;
        }
        let mut recovery_base = recovery_base(
            &document_summaries,
            &resources.metadata,
            &canonical_paths,
            &persistence_frontier,
        );
        if let Some(initial) = initial_document
            && let Some(hash) = document_loader.recovery_hash(initial)
        {
            recovery_base.hashes.insert(
                recovery::ResourceId::DocumentById {
                    document_id: stable_id_text(initial.as_bytes()),
                },
                hash,
            );
        }
        let commands = Arc::new(NativeProjectCommandDispatcher::new(
            project,
            document_owner.clone(),
        ));

        let history = Arc::new(ControlledHistory {
            inner: Git2HistoryStore::new(root.clone()),
            controls: self.shared.controls.clone(),
        });
        let history_root = HistoryRoot::new(history_root_id(project_id));
        let history_status = match history.initialize(history_root.clone()) {
            Ok(state) => ProductionHistoryStatus::Available {
                checkpoint_count: state.checkpoint_count,
            },
            Err(
                error @ (history::HistoryError::CorruptHistory { .. }
                | history::HistoryError::MissingHistory),
            ) => ProductionHistoryStatus::Unavailable {
                problem: error.to_string(),
                reinitialize: history.reinitialize_availability().map_err(
                    |availability_error| {
                        ProjectFilesystemError::failed(
                            "inspect History reinitialization",
                            availability_error.to_string(),
                        )
                    },
                )?,
            },
            Err(error) => {
                return Err(ProjectFilesystemError::failed(
                    "initialize History",
                    error.to_string(),
                ));
            }
        };
        let recovery = Arc::new(ControlledRecovery {
            inner: FsRecoveryJournal::open(&root_path).map_err(|error| {
                ProjectFilesystemError::failed("initialize recovery", error.to_string())
            })?,
            controls: self.shared.controls.clone(),
        });
        let search = Arc::new(ControlledSearch {
            inner: SqliteSearchIndex::new(&root_path),
            controls: self.shared.controls.clone(),
            live: Mutex::new(None),
        });
        search
            .open_or_rebuild_background(project_id, Arc::new(search_source))
            .map_err(|error| {
                ProjectFilesystemError::failed("initialize search", error.to_string())
            })?;
        search
            .attach_live_source(
                Arc::new(LiveSearchProjectionSource {
                    commands: commands.clone(),
                    documents: document_owner.clone(),
                }),
                Vec::new(),
            )
            .map_err(|error| {
                ProjectFilesystemError::failed("hydrate live search", error.to_string())
            })?;
        let writer = Arc::new(FsAtomicWriter::new(NativeAtomicFileOps::new(root)));
        let save = Arc::new(
            ProjectSaveCoordinator::new(project_id, writer, history.clone(), recovery.clone())
                .map_err(|error| {
                    ProjectFilesystemError::failed("initialize save", error.to_string())
                })?,
        );
        save.reconcile_open().map_err(|error| {
            ProjectFilesystemError::failed("reconcile pending save", error.to_string())
        })?;
        let persistence = Arc::new(EditorPersistenceCoordinator::new(
            recovery.clone(),
            save.clone(),
            recovery_base.clone(),
        ));
        let project_persistence = Arc::new(ProjectPersistenceCoordinator::new(
            commands.clone(),
            document_owner.clone(),
            persistence.clone(),
            recovery_base,
            resources.metadata.clone(),
            canonical_paths,
        ));
        let query = Arc::new(ProductionProjectQuery {
            commands: commands.clone(),
            documents: document_owner,
            persistence: project_persistence.clone(),
            persisted_summaries: persistence_frontier.document_summaries.clone(),
            document_loader,
            search: search.clone(),
        });
        self.shared
            .dictionary_source
            .register_project(project_id, query.clone());
        let dictionary_revision = query
            .snapshot()
            .map_err(|error| {
                ProjectFilesystemError::failed("read project dictionary", error.to_string())
            })?
            .project
            .revision
            .value();
        block_on(
            self.shared.spellcheck.reload_project_dictionary(
                project_id,
                DictionaryRevision::from(dictionary_revision),
            ),
        )
        .map_err(|error| {
            ProjectFilesystemError::failed("hydrate project dictionary", error.to_string())
        })?;
        block_on(
            self.shared
                .spellcheck
                .reload_global_dictionary(DictionaryRevision::default()),
        )
        .map_err(|error| {
            ProjectFilesystemError::failed("hydrate global dictionary", error.to_string())
        })?;
        let workflows = Arc::new(ProductionProjectWorkflows {
            history: history.clone(),
            persistence: project_persistence.clone(),
            query: query.clone(),
            exporter: self.shared.exporter.clone(),
            search: search.clone(),
            artifacts: Mutex::new(BTreeMap::new()),
            next_artifact: AtomicU64::new(1),
            active_export: Mutex::new(None),
            next_export_operation: AtomicU64::new(1),
        });
        let refreshing_commands = Arc::new(SearchRefreshingCommands {
            inner: commands.clone(),
            search: search.clone(),
        });
        let refreshing_persistence = Arc::new(SearchRefreshingPersistence {
            inner: project_persistence.clone(),
            search: search.clone(),
        });
        let history_maintenance = Arc::new(ProductionHistoryMaintenance {
            history: history.clone(),
            root: history_root,
            problem: match &history_status {
                ProductionHistoryStatus::Available { .. } => None,
                ProductionHistoryStatus::Unavailable { problem, .. } => Some(problem.clone()),
            },
        });
        let ui_services = ProjectUiServices::new(
            UiApplicationServices::new(refreshing_commands.clone(), refreshing_commands),
            query.clone(),
            history.clone(),
            history_maintenance,
            recovery.clone(),
            search.clone(),
            Arc::new(ProductionSaveStatus { save: save.clone() }),
            refreshing_persistence,
            workflows.clone(),
            workflows,
            self.shared.exporter.clone(),
            self.shared.editor.clone(),
            self.shared.spellcheck.clone(),
            self.shared.workspace_state.clone(),
            self.shared.preferences.clone(),
            self.shared.appearance.clone(),
            self.shared.platform.clone(),
        );

        self.shared
            .controls
            .observe(ProductionObservation::ProjectOpened {
                path: path.clone(),
                project: project_id,
            });
        self.shared
            .controls
            .service_operation(ProductionFaultPoint::ProjectOpen, "open", true);
        Ok(Box::new(ProductionProjectSession {
            path,
            project_id,
            commands,
            history,
            history_status,
            recovery,
            search,
            save,
            persistence,
            project_persistence,
            query,
            ui_services,
            _repository: repository,
            _open_project: open_project,
            controls: self.shared.controls.clone(),
        }))
    }

    fn begin_final_save(&self, session: &dyn ProjectSession) -> Result<(), ProjectFilesystemError> {
        let session = session
            .as_any()
            .downcast_ref::<ProductionProjectSession>()
            .ok_or_else(|| {
                ProjectFilesystemError::failed(
                    "save",
                    "project session was not created by the production graph",
                )
            })?;
        session.reconcile_final_save()
    }

    fn ui_services(
        &self,
        session: &dyn ProjectSession,
    ) -> Result<Option<ProjectUiServices>, ProjectFilesystemError> {
        let session = session
            .as_any()
            .downcast_ref::<ProductionProjectSession>()
            .ok_or_else(|| {
                ProjectFilesystemError::failed(
                    "project UI services",
                    "project session was not created by the production graph",
                )
            })?;
        Ok(Some(session.ui_services.clone()))
    }
}

#[derive(Default)]
struct ProductionUiState {
    appearance: Option<ResolvedAppearance>,
    projects: BTreeMap<WindowCapability, NativeProjectWindow>,
    locked_project: Option<PathBuf>,
}

trait NativeDesktopDriver: Send + Sync {
    fn run(&self, startup: NativeDesktopStartup) -> Result<(), NativeDesktopError>;
}

struct IcedDesktopDriver;

impl NativeDesktopDriver for IcedDesktopDriver {
    fn run(&self, startup: NativeDesktopStartup) -> Result<(), NativeDesktopError> {
        run_native_desktop(startup)
    }
}

struct ProductionDesktopUi {
    state: Mutex<ProductionUiState>,
    registry: IcedWindowRegistry,
    editor: Arc<EditorIcedAdapter>,
    preferences: Arc<dyn PreferenceService>,
    appearance: Arc<dyn AppearanceService>,
    platform: UiPlatformServices,
    controls: ProductionControls,
    driver: Arc<dyn NativeDesktopDriver>,
}

impl DesktopUi for ProductionDesktopUi {
    fn start(&self, startup: DesktopStartup) -> Result<(), DesktopUiError> {
        self.state
            .lock()
            .map_err(|_| DesktopUiError::new("Iced desktop state is unavailable"))?
            .appearance = Some(startup.appearance.appearance);
        Ok(())
    }

    fn project_opened(
        &self,
        project: &RequestedProjectPath,
        window: WindowCapability,
        session: parchmint_ui_api::ProjectSessionCapability,
    ) -> Result<(), DesktopUiError> {
        self.state
            .lock()
            .map_err(|_| DesktopUiError::new("Iced desktop state is unavailable"))?
            .projects
            .insert(
                window,
                NativeProjectWindow {
                    project: project.as_path().to_path_buf(),
                    window,
                    session,
                    project_ui: None,
                    editor: Some(self.editor.clone()),
                },
            );
        self.controls.observe(ProductionObservation::WindowOpened {
            window,
            session_id: session.session_id(),
            session_generation: session.generation(),
            typed_ports: false,
            native_editor: true,
        });
        Ok(())
    }

    fn project_opened_with_ui(
        &self,
        project: &RequestedProjectPath,
        window: WindowCapability,
        project_ui: ProjectUiProject,
    ) -> Result<(), DesktopUiError> {
        let session = project_ui.session();
        self.state
            .lock()
            .map_err(|_| DesktopUiError::new("Iced desktop state is unavailable"))?
            .projects
            .insert(
                window,
                NativeProjectWindow::typed(
                    project.as_path().to_path_buf(),
                    window,
                    project_ui,
                    self.editor.clone(),
                ),
            );
        self.controls.observe(ProductionObservation::WindowOpened {
            window,
            session_id: session.session_id(),
            session_generation: session.generation(),
            typed_ports: true,
            native_editor: true,
        });
        Ok(())
    }

    fn focus_window(&self, window: WindowCapability) -> Result<(), DesktopUiError> {
        let known = self
            .state
            .lock()
            .map_err(|_| DesktopUiError::new("Iced desktop state is unavailable"))?
            .projects
            .contains_key(&window);
        if !known {
            return Err(DesktopUiError::new("cannot focus a stale project window"));
        }
        self.controls
            .observe(ProductionObservation::WindowFocused(window));
        Ok(())
    }

    fn project_locked(&self, project: &RequestedProjectPath) -> Result<(), DesktopUiError> {
        self.state
            .lock()
            .map_err(|_| DesktopUiError::new("Iced desktop state is unavailable"))?
            .locked_project = Some(project.as_path().to_path_buf());
        self.controls.observe(ProductionObservation::ProjectLocked {
            path: project.as_path().to_path_buf(),
        });
        Ok(())
    }

    fn retain_window_for_final_save(&self, window: WindowCapability) -> Result<(), DesktopUiError> {
        self.controls
            .observe(ProductionObservation::WindowRetained(window));
        Ok(())
    }

    fn project_closed(&self, window: WindowCapability) -> Result<(), DesktopUiError> {
        self.state
            .lock()
            .map_err(|_| DesktopUiError::new("Iced desktop state is unavailable"))?
            .projects
            .remove(&window);
        self.controls
            .observe(ProductionObservation::WindowClosed(window));
        Ok(())
    }

    fn final_save_failed(
        &self,
        window: WindowCapability,
        error: &ProjectFilesystemError,
    ) -> Result<(), DesktopUiError> {
        self.controls
            .observe(ProductionObservation::FinalSaveFailed {
                window,
                reason: error.to_string(),
            });
        Ok(())
    }

    fn run(&self, runtime: DesktopRuntime) -> Result<parchmint_ui_api::ExitCode, DesktopUiError> {
        self.run_native(runtime, None)
    }

    fn run_with_native_capture(
        &self,
        runtime: DesktopRuntime,
        capture: NativeCaptureRequest,
    ) -> Result<parchmint_ui_api::ExitCode, DesktopUiError> {
        self.run_native(runtime, Some(capture))
    }
}

impl ProductionDesktopUi {
    fn run_native(
        &self,
        runtime: DesktopRuntime,
        capture: Option<NativeCaptureRequest>,
    ) -> Result<parchmint_ui_api::ExitCode, DesktopUiError> {
        let (appearance, projects, locked_project) = {
            let state = self
                .state
                .lock()
                .map_err(|_| DesktopUiError::new("Iced desktop state is unavailable"))?;
            let appearance = state.appearance.ok_or_else(|| {
                DesktopUiError::new("Iced desktop was run before startup completed")
            })?;
            (
                appearance,
                state.projects.values().cloned().collect(),
                state.locked_project.clone(),
            )
        };
        let preferences = block_on(self.preferences.load())
            .map_err(|error| DesktopUiError::new(error.to_string()))?;
        let recent_projects = preferences.values.recent_projects;
        let appearance_mode = capture
            .as_ref()
            .map_or(preferences.values.appearance, |capture| {
                match capture.appearance {
                    ResolvedAppearance::Light => AppearanceMode::Light,
                    ResolvedAppearance::Dark => AppearanceMode::Dark,
                }
            });
        self.driver
            .run(NativeDesktopStartup {
                appearance: capture
                    .as_ref()
                    .map_or(appearance, |capture| capture.appearance),
                appearance_mode,
                recent_projects,
                projects,
                locked_project,
                capture,
                callbacks: Arc::new(ProductionUiCallbacks {
                    runtime,
                    registry: self.registry.clone(),
                    editor: self.editor.clone(),
                    preferences: self.preferences.clone(),
                    appearance: self.appearance.clone(),
                    platform: self.platform.clone(),
                }),
            })
            .map_err(|error| DesktopUiError::new(error.to_string()))?;
        Ok(parchmint_ui_api::ExitCode::SUCCESS)
    }
}

struct ProductionUiCallbacks {
    runtime: DesktopRuntime,
    registry: IcedWindowRegistry,
    editor: Arc<EditorIcedAdapter>,
    preferences: Arc<dyn PreferenceService>,
    appearance: Arc<dyn AppearanceService>,
    platform: UiPlatformServices,
}

impl NativeDesktopCallbacks for ProductionUiCallbacks {
    fn open_project(&self, project: PathBuf) -> Result<NativeProjectOpenResult, String> {
        let result = self
            .runtime
            .open_project(project.clone())
            .map_err(|error| error.to_string())?;
        Ok(match result {
            crate::OpenProjectResult::Opened { window, session } => {
                let project_ui = self
                    .runtime
                    .project_ui(session)
                    .map_err(|error| error.to_string())?
                    .ok_or_else(|| "production project UI ports are unavailable".to_owned())?;
                NativeProjectOpenResult::Opened(NativeProjectWindow::typed(
                    project,
                    window,
                    project_ui,
                    self.editor.clone(),
                ))
            }
            crate::OpenProjectResult::Focused(window) => NativeProjectOpenResult::Focused(window),
            crate::OpenProjectResult::Locked => NativeProjectOpenResult::Locked,
        })
    }

    fn close_project(&self, project: PathBuf) -> Result<(), String> {
        let request = self
            .runtime
            .begin_final_save(&project)
            .map_err(|error| error.to_string())?;
        match self
            .runtime
            .resolve_final_save(request, Ok(()))
            .map_err(|error| error.to_string())?
        {
            crate::FinalSaveResolution::Closed(_) => Ok(()),
            crate::FinalSaveResolution::SaveFailed => {
                Err(format!("final save failed for {}", project.display()))
            }
            crate::FinalSaveResolution::IgnoredStale => {
                Err(format!("project window is stale: {}", project.display()))
            }
        }
    }

    fn close_clean_project(&self, project: PathBuf) -> Result<(), String> {
        self.runtime
            .close_clean_project(&project)
            .map_err(|error| error.to_string())
    }

    fn create_project(
        &self,
        request: NativeNewProjectRequest,
    ) -> Result<NativeProjectOpenResult, String> {
        let project = request.destination.clone();
        let result = self
            .runtime
            .create_project(NewProjectRequest::new(
                request.title,
                request.destination,
                request.author,
            ))
            .map_err(|error| error.to_string())?;
        Ok(match result {
            crate::OpenProjectResult::Opened { window, session } => {
                let project_ui = self
                    .runtime
                    .project_ui(session)
                    .map_err(|error| error.to_string())?
                    .ok_or_else(|| "production project UI ports are unavailable".to_owned())?;
                NativeProjectOpenResult::Opened(NativeProjectWindow::typed(
                    project,
                    window,
                    project_ui,
                    self.editor.clone(),
                ))
            }
            crate::OpenProjectResult::Focused(window) => NativeProjectOpenResult::Focused(window),
            crate::OpenProjectResult::Locked => NativeProjectOpenResult::Locked,
        })
    }

    fn choose_project_directory(
        &self,
        window: WindowCapability,
        title: &'static str,
    ) -> Result<Option<PathBuf>, String> {
        block_on(self.platform.dialogs.choose_path(
            window,
            PathDialog {
                kind: PathDialogKind::OpenDirectory,
                title: Some(title.to_owned()),
            },
        ))
        .map_err(|error| error.to_string())
        .map(|result| {
            result
                .into_value()
                .map(|selection| selection.as_path().to_path_buf())
        })
    }

    fn set_appearance(&self, mode: AppearanceMode) -> Result<ResolvedAppearance, String> {
        if mode == AppearanceMode::System {
            let system = block_on(self.platform.system_appearance.current_appearance())
                .map(resolved_appearance)
                .map_err(|error| error.to_string())?;
            self.appearance
                .system_appearance_changed(system)
                .map_err(|error| error.to_string())?;
        }
        let current = block_on(self.preferences.load()).map_err(|error| error.to_string())?;
        block_on(self.appearance.set_mode(current.revision, mode))
            .map(|snapshot| snapshot.appearance)
            .map_err(|error| error.to_string())
    }

    fn system_appearance_changed(
        &self,
        appearance: ResolvedAppearance,
    ) -> Result<Option<ResolvedAppearance>, String> {
        self.appearance
            .system_appearance_changed(appearance)
            .map(|snapshot| snapshot.map(|snapshot| snapshot.appearance))
            .map_err(|error| error.to_string())
    }

    fn project_window_created(&self, window: WindowCapability) {
        self.registry.register_window(window);
    }

    fn project_window_destroyed(&self, window: WindowCapability) {
        self.registry.close_window(window);
    }
}

pub(crate) fn assemble() -> Result<DesktopBootstrap, StartupError> {
    let controls = ProductionControls::default();
    assemble_with_controls(controls)
}

pub(crate) fn assemble_with_controls(
    controls: ProductionControls,
) -> Result<DesktopBootstrap, StartupError> {
    let platform = NativePlatform::initialize()
        .map_err(|error| StartupError::production("native platform", error))?;
    let paths = block_on(platform.application_paths.application_paths())
        .map_err(|error| StartupError::production("application paths", error))?;
    let preference_store = Arc::new(FilePreferenceStore::new(
        paths.configuration().join("preferences.json"),
    ));
    let preferences: Arc<dyn PreferenceService> =
        Arc::new(PreferenceCoordinator::new(preference_store));
    let appearance = Arc::new(AppearanceController::new(Arc::clone(&preferences)));
    let dictionary_source = Arc::new(ProductionDictionarySource::new(preferences.clone()));
    let editor = Arc::new(
        EditorIcedAdapter::new(EditorIcedConfig::default())
            .map_err(|error| StartupError::production("editor", error))?,
    );
    let spellcheck = Arc::new(
        EnUsSpellcheckService::new(EnUsSpellcheckConfig {
            saved_dictionaries: dictionary_source.clone(),
            ..EnUsSpellcheckConfig::default()
        })
        .map_err(|error| StartupError::production("spellcheck", error))?,
    );
    let ui_platform = UiPlatformServices::new(
        platform.menus.clone(),
        platform.dialogs.clone(),
        platform.clipboard.clone(),
        platform.external_open.clone(),
        platform.application_paths.clone(),
        platform.appearance.clone(),
    )
    .with_menu_activations(platform.menu_activations.clone());
    let shared = Arc::new(SharedServices {
        editor,
        spellcheck,
        dictionary_source,
        exporter: Arc::new(ControlledExporter {
            inner: HtmlExporter,
            controls: controls.clone(),
        }),
        workspace_state: Arc::new(FileWorkspaceStateStore::new(
            paths.data().join("workspaces"),
        )),
        preferences: preferences.clone(),
        appearance: appearance.clone(),
        platform: ui_platform,
        controls: controls.clone(),
    });
    for component in [
        "platform",
        "preferences",
        "appearance",
        "editor",
        "spellcheck",
        "export",
        "workspace-state",
        "project-service-factory",
        "iced-ui",
    ] {
        controls.observe(ProductionObservation::ComponentReady(component));
    }

    let graph = Arc::new(ProductionApplicationGraph {
        shared: Arc::clone(&shared),
    });
    let application: ApplicationServices = graph;
    let project_filesystem = Arc::new(ProductionProjectFilesystem {
        shared: Arc::clone(&shared),
    });
    let ui = Arc::new(ProductionDesktopUi {
        state: Mutex::new(ProductionUiState::default()),
        registry: platform.iced_window_registry(),
        editor: shared.editor.clone(),
        preferences: preferences.clone(),
        appearance: appearance.clone(),
        platform: shared.platform.clone(),
        controls,
        driver: Arc::new(IcedDesktopDriver),
    });
    let platform_services: PlatformServices = platform.appearance.clone();

    Ok(DesktopBootstrap::new(
        application,
        project_filesystem,
        preferences,
        appearance,
        platform_services,
        ui,
    ))
}

#[derive(Clone)]
struct CanonicalOpenResources {
    metadata: BTreeMap<CanonicalRelativePath, Vec<u8>>,
    document_paths: Vec<CanonicalRelativePath>,
}

fn canonical_resources(
    root: &parchmint_project_fs::ProjectRootCapability,
) -> Result<CanonicalOpenResources, ProjectFilesystemError> {
    let root_path = root
        .checked_path()
        .map_err(|error| ProjectFilesystemError::failed("authorize root", error.to_string()))?;
    let mut paths = Vec::new();
    for relative in [
        ".parchmint/format-version",
        "project.toml",
        "styles.css",
        "dictionary.txt",
        "deletions.json",
    ] {
        let path = root_path.join(relative);
        match fs::symlink_metadata(&path) {
            Ok(metadata) if metadata.is_file() && !metadata.file_type().is_symlink() => {
                paths.push(CanonicalRelativePath::parse(relative).expect("fixed path is canonical"))
            }
            Ok(_) => {
                return Err(ProjectFilesystemError::failed(
                    "enumerate canonical resources",
                    format!("unsafe canonical path {}", path.display()),
                ));
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(ProjectFilesystemError::failed(
                    "enumerate canonical resources",
                    error.to_string(),
                ));
            }
        }
    }
    for directory in ["manuscript", "research", "annotations"] {
        collect_canonical_paths(root_path, &root_path.join(directory), &mut paths)?;
    }
    paths.sort();
    let document_paths = paths
        .iter()
        .filter(|path| is_document_resource(path))
        .cloned()
        .collect();
    let files = NativeProjectFileSystem::new();
    let metadata = paths
        .into_iter()
        .filter(|path| !is_document_resource(path))
        .map(|path| {
            files
                .read(root, &path)
                .map(|bytes| (path, bytes))
                .map_err(|error| {
                    ProjectFilesystemError::failed("read canonical resource", error.to_string())
                })
        })
        .collect::<Result<_, _>>()?;
    Ok(CanonicalOpenResources {
        metadata,
        document_paths,
    })
}

fn collect_canonical_paths(
    root: &Path,
    directory: &Path,
    paths: &mut Vec<CanonicalRelativePath>,
) -> Result<(), ProjectFilesystemError> {
    let entries = match fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => {
            return Err(ProjectFilesystemError::failed(
                "read canonical directory",
                error.to_string(),
            ));
        }
    };
    for entry in entries {
        let entry = entry.map_err(|error| {
            ProjectFilesystemError::failed("read canonical entry", error.to_string())
        })?;
        let kind = entry.file_type().map_err(|error| {
            ProjectFilesystemError::failed("inspect canonical entry", error.to_string())
        })?;
        if kind.is_symlink() || !kind.is_dir() && !kind.is_file() {
            return Err(ProjectFilesystemError::failed(
                "inspect canonical entry",
                format!("unsafe canonical path {}", entry.path().display()),
            ));
        }
        if kind.is_dir() {
            collect_canonical_paths(root, &entry.path(), paths)?;
            continue;
        }
        let relative = entry
            .path()
            .strip_prefix(root)
            .ok()
            .and_then(Path::to_str)
            .map(|path| path.replace('\\', "/"))
            .and_then(|path| CanonicalRelativePath::parse(path).ok())
            .filter(is_canonical_resource)
            .ok_or_else(|| {
                ProjectFilesystemError::failed(
                    "inspect canonical entry",
                    format!("unsupported canonical path {}", entry.path().display()),
                )
            })?;
        paths.push(relative);
    }
    Ok(())
}

fn is_canonical_resource(path: &CanonicalRelativePath) -> bool {
    let path = path.as_str();
    ((path.starts_with("manuscript/") || path.starts_with("research/")) && path.ends_with(".html"))
        || (path.starts_with("annotations/") && path.ends_with(".json"))
}

struct CanonicalDocumentLoader {
    root: parchmint_project_fs::ProjectRootCapability,
    paths: BTreeMap<DocumentId, CanonicalRelativePath>,
    summaries: BTreeMap<DocumentId, CanonicalDocumentSummary>,
    annotations: BTreeMap<DocumentId, Vec<CanonicalComment>>,
    annotation_bytes: BTreeMap<DocumentId, Vec<u8>>,
    recovery_hashes: Mutex<BTreeMap<DocumentId, recovery::ContentHash>>,
    hydrated_summaries: Mutex<BTreeMap<DocumentId, CanonicalDocumentSummary>>,
}

impl CanonicalDocumentLoader {
    fn recovery_hash(&self, document: DocumentId) -> Option<recovery::ContentHash> {
        self.recovery_hashes
            .lock()
            .ok()
            .and_then(|hashes| hashes.get(&document).copied())
    }

    fn hydrated_summary(&self, document: DocumentId) -> Option<CanonicalDocumentSummary> {
        self.hydrated_summaries
            .lock()
            .ok()
            .and_then(|summaries| summaries.get(&document).cloned())
    }
}

impl DocumentSnapshotLoader for CanonicalDocumentLoader {
    fn load(&self, document: DocumentId) -> Result<DocumentSnapshot, ApplicationError> {
        let path = self
            .paths
            .get(&document)
            .ok_or(ApplicationError::MissingDocument { document })?;
        let bytes = NativeProjectFileSystem::new()
            .read(&self.root, path)
            .map_err(|error| ApplicationError::DocumentLoad {
                document,
                reason: error.to_string(),
            })?;
        if let Some(summary) = self.summaries.get(&document) {
            let actual = ContentHash::from_bytes(Sha256::digest(&bytes).into());
            if actual != summary.content_hash {
                return Err(ApplicationError::DocumentLoad {
                    document,
                    reason: "canonical body changed outside the persisted save frontier".into(),
                });
            }
        }
        let body = ProjectFormatCodec::default()
            .decode_document(&bytes)
            .map_err(|error| ApplicationError::DocumentLoad {
                document,
                reason: error.to_string(),
            })?
            .as_html()
            .to_owned();
        let recovery_hash = recovery_document_content_hash(
            &bytes,
            self.annotation_bytes.get(&document).map(Vec::as_slice),
        );
        self.recovery_hashes
            .lock()
            .map_err(|_| ApplicationError::StateUnavailable)?
            .insert(document, recovery_hash);
        let revision = self
            .summaries
            .get(&document)
            .map_or(0, |summary| summary.revision);
        self.hydrated_summaries
            .lock()
            .map_err(|_| ApplicationError::StateUnavailable)?
            .insert(
                document,
                CanonicalDocumentSummary {
                    revision,
                    content_hash: ContentHash::from_bytes(Sha256::digest(&bytes).into()),
                    word_count: body.split_whitespace().count(),
                },
            );
        Ok(DocumentSnapshot {
            document_id: document,
            body,
            comments: self.annotations.get(&document).cloned().unwrap_or_default(),
            revision: self
                .summaries
                .get(&document)
                .map(|summary| EditorRevision::from(summary.revision))
                .unwrap_or_default(),
            visibility: DocumentVisibility::Closed,
        })
    }
}

struct CanonicalSearchSource {
    project: Project,
    documents: Vec<DocumentId>,
    loader: Arc<CanonicalDocumentLoader>,
    frontier: Option<parchmint_search_api::SearchFrontierId>,
}

type CanonicalApplicationState = (
    Project,
    Vec<LazyDocumentSummary>,
    Arc<CanonicalDocumentLoader>,
    CanonicalSearchSource,
    CanonicalProjectPathMap,
    parchmint_project_format::CanonicalPersistenceFrontier,
);

impl SearchProjectionSource for CanonicalSearchSource {
    fn visit_projections(
        &self,
        visitor: &mut dyn SearchProjectionVisitor,
    ) -> Result<(), parchmint_search_api::SearchError> {
        for document in &self.documents {
            let snapshot = self.loader.load(*document).map_err(|error| {
                parchmint_search_api::SearchError::Source {
                    reason: error.to_string(),
                }
            })?;
            visitor.visit(SearchDocumentProjection {
                document_id: *document,
                revision: RevisionId::from(snapshot.revision.value()),
                texts: search_texts(&self.project, *document, &snapshot.body),
            })?;
        }
        Ok(())
    }

    fn frontier_identity(&self) -> Option<parchmint_search_api::SearchFrontierId> {
        self.frontier
    }
}

fn application_state(
    project_id: ProjectId,
    root: parchmint_project_fs::ProjectRootCapability,
    resources: &CanonicalOpenResources,
) -> Result<CanonicalApplicationState, ProjectFilesystemError> {
    let codec = ProjectFormatCodec::default();
    let mut project = Project::new(project_id);
    let mut canonical_paths = CanonicalProjectPathMap::default();
    let mut persistence_frontier =
        parchmint_project_format::CanonicalPersistenceFrontier::default();
    if let Some(manifest) = resources
        .metadata
        .get(&CanonicalRelativePath::parse("project.toml").expect("static path"))
    {
        let manifest = codec.decode_manifest(manifest).map_err(|error| {
            ProjectFilesystemError::failed("decode project manifest", error.to_string())
        })?;
        persistence_frontier = codec
            .decode_persistence_frontier(&manifest)
            .map_err(|error| {
                ProjectFilesystemError::failed("decode persistence frontier", error.to_string())
            })?;
        let styles = resources
            .metadata
            .get(&CanonicalRelativePath::parse("styles.css").expect("static path"))
            .map(|bytes| codec.decode_styles(bytes))
            .transpose()
            .map_err(|error| {
                ProjectFilesystemError::failed("decode project styles", error.to_string())
            })?;
        if let Some((decoded, paths)) = codec
            .decode_domain_project_with_styles(&manifest, styles.as_ref(), project_id)
            .map_err(|error| {
                ProjectFilesystemError::failed("decode project structure", error.to_string())
            })?
        {
            project = decoded;
            canonical_paths = paths;
        } else {
            let project_table = manifest.value().get("project");
            project.display_title = project_table
                .and_then(|project| project.get("title"))
                .and_then(|value| value.as_str())
                .unwrap_or_default()
                .to_owned();
            project.author = project_table
                .and_then(|project| project.get("author"))
                .and_then(|value| value.as_str())
                .map(str::to_owned);
        }
    }
    let discovered = resources
        .document_paths
        .iter()
        .cloned()
        .collect::<std::collections::BTreeSet<_>>();
    if canonical_paths.documents.is_empty() {
        let mut section_counts = BTreeMap::from([
            (NodeId::manuscript_root(), 0usize),
            (NodeId::research_root(), 0usize),
        ]);
        for path in &resources.document_paths {
            let document_id =
                DocumentId::from_bytes(stable_id(b"document", path.as_str().as_bytes()));
            canonical_paths.documents.insert(document_id, path.clone());
            let node_id = NodeId::from_bytes(stable_id(b"node", path.as_str().as_bytes()));
            let parent = if path.as_str().starts_with("research/") {
                NodeId::research_root()
            } else {
                NodeId::manuscript_root()
            };
            let index = section_counts.get_mut(&parent).expect("section counter");
            let stem = Path::new(path.as_str())
                .file_stem()
                .and_then(|value| value.to_str())
                .unwrap_or("untitled-document");
            let title = if stem == "untitled-document" {
                "Untitled Document".to_owned()
            } else {
                stem.to_owned()
            };
            project = apply_project_command(
                &project,
                project.revision,
                ProjectCommand::create_document(node_id, document_id, parent, *index, title),
            )
            .map_err(|error| {
                ProjectFilesystemError::failed("assemble project model", error.to_string())
            })?
            .project;
            *index += 1;
        }
        project.revision = Default::default();
    } else {
        for path in canonical_paths.documents.values() {
            if !discovered.contains(path) {
                return Err(ProjectFilesystemError::failed(
                    "decode project structure",
                    format!("manifest document is missing: {}", path.as_str()),
                ));
            }
        }
    }

    let mut annotations = BTreeMap::new();
    let mut annotation_bytes = BTreeMap::new();
    for document_id in canonical_paths.documents.keys() {
        let annotation_path = CanonicalRelativePath::parse(format!(
            "annotations/{}.json",
            stable_id_text(document_id.as_bytes())
        ))
        .expect("stable annotation path");
        let comments = match resources.metadata.get(&annotation_path) {
            Some(bytes) => {
                annotation_bytes.insert(*document_id, bytes.clone());
                codec
                    .decode_annotations(bytes)
                    .and_then(|sidecar| {
                        if sidecar.document_id() != stable_id_text(document_id.as_bytes()) {
                            return Err(parchmint_project_format::FormatError::InvalidAnnotations(
                                "sidecar document ID does not match its path".into(),
                            ));
                        }
                        sidecar
                            .typed_threads()
                            .map(|threads| threads.into_iter().map(editor_thread).collect())
                    })
                    .map_err(|error| {
                        ProjectFilesystemError::failed(
                            "decode annotation sidecar",
                            error.to_string(),
                        )
                    })?
            }
            None => Vec::new(),
        };
        annotations.insert(*document_id, comments);
    }
    let mut document_order = Vec::new();
    append_document_ids(&project, NodeId::manuscript_root(), &mut document_order);
    append_document_ids(&project, NodeId::research_root(), &mut document_order);
    let summaries = document_order
        .iter()
        .enumerate()
        .map(|(index, document)| LazyDocumentSummary {
            document_id: *document,
            revision: persistence_frontier
                .document_revisions
                .get(document)
                .copied()
                .unwrap_or_default()
                .into(),
            visibility: if index == 0 {
                DocumentVisibility::Open
            } else {
                DocumentVisibility::Closed
            },
        })
        .collect::<Vec<_>>();
    let loader = Arc::new(CanonicalDocumentLoader {
        root,
        paths: canonical_paths.documents.clone(),
        summaries: persistence_frontier.document_summaries.clone(),
        annotations,
        annotation_bytes,
        recovery_hashes: Mutex::new(BTreeMap::new()),
        hydrated_summaries: Mutex::new(BTreeMap::new()),
    });
    let search_source = CanonicalSearchSource {
        project: project.clone(),
        documents: document_order,
        loader: loader.clone(),
        frontier: persistence_frontier.save_identity.map(|identity| {
            parchmint_search_api::SearchFrontierId::from_bytes(*identity.as_bytes())
        }),
    };
    Ok((
        project,
        summaries,
        loader,
        search_source,
        canonical_paths,
        persistence_frontier,
    ))
}

fn append_document_ids(project: &Project, node: NodeId, documents: &mut Vec<DocumentId>) {
    if let Some(node) = project.nodes.get(node)
        && let NodeKind::Document(document) = node.kind
    {
        documents.push(document);
    }
    for child in project.nodes.children(node) {
        append_document_ids(project, *child, documents);
    }
}

fn new_project_manifest(title: &str, author: Option<&str>) -> String {
    let mut manifest = format!(
        "[project]\ntitle = {}\nspellcheck-language = \"en-US\"\n",
        toml_string(title)
    );
    if let Some(author) = author {
        manifest.push_str(&format!("author = {}\n", toml_string(author)));
    }
    manifest
}

fn toml_string(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len() + 2);
    escaped.push('"');
    for character in value.chars() {
        match character {
            '\\' => escaped.push_str("\\\\"),
            '"' => escaped.push_str("\\\""),
            character => escaped.push(character),
        }
    }
    escaped.push('"');
    escaped
}

fn recovery_base(
    documents: &[LazyDocumentSummary],
    resources: &BTreeMap<CanonicalRelativePath, Vec<u8>>,
    paths: &CanonicalProjectPathMap,
    frontier: &parchmint_project_format::CanonicalPersistenceFrontier,
) -> recovery::RecoveryBaseSnapshot {
    let revisions = documents
        .iter()
        .map(|document| {
            (
                document.document_id,
                recovery::DocumentRevision::from(document.revision.value()),
            )
        })
        .collect();
    let mut hashes = BTreeMap::new();
    for (path, bytes) in resources {
        let resource = match path.as_str() {
            ".parchmint/format-version" => recovery::ResourceId::FormatControl,
            "project.toml" => recovery::ResourceId::Manifest,
            "styles.css" => recovery::ResourceId::Styles,
            "dictionary.txt" => recovery::ResourceId::Dictionary,
            path if (path.starts_with("manuscript/") || path.starts_with("research/"))
                && path.ends_with(".html") =>
            {
                let document_id = paths
                    .documents
                    .iter()
                    .find_map(|(document, canonical)| {
                        (canonical.as_str() == path).then_some(*document)
                    })
                    .map(|document| stable_id_text(document.as_bytes()))
                    .unwrap_or_else(|| stable_id_text(&stable_id(b"document", path.as_bytes())));
                recovery::ResourceId::DocumentById { document_id }
            }
            path if path.starts_with("annotations/") && path.ends_with(".json") => {
                recovery::ResourceId::Annotations {
                    document_id: path
                        .trim_start_matches("annotations/")
                        .trim_end_matches(".json")
                        .to_owned(),
                }
            }
            _ => continue,
        };
        hashes.insert(
            resource,
            recovery::ContentHash::from_bytes(Sha256::digest(bytes).into()),
        );
    }
    recovery::RecoveryBaseSnapshot {
        revisions: recovery::RecoveryRevisionVector::new(
            parchmint_domain::ProjectRevision::from(frontier.recovery_project_revision),
            revisions,
        ),
        hashes,
    }
}

fn recovery_document_content_hash(
    body: &[u8],
    annotations: Option<&[u8]>,
) -> recovery::ContentHash {
    let mut digest = Sha256::new();
    digest.update(b"parchmint recovery document v1\0");
    digest.update((body.len() as u64).to_be_bytes());
    digest.update(body);
    match annotations {
        Some(annotations) => {
            digest.update([1]);
            digest.update((annotations.len() as u64).to_be_bytes());
            digest.update(annotations);
        }
        None => digest.update([0]),
    }
    recovery::ContentHash::from_bytes(digest.finalize().into())
}

fn editor_thread(thread: AnnotationThread) -> CanonicalComment {
    CanonicalComment {
        id: CommentId::from_bytes(thread.id),
        messages: thread
            .messages
            .into_iter()
            .map(|message| CanonicalCommentMessage {
                id: CommentId::from_bytes(message.id),
                body: message.body,
                unknown_fields: message.unknown_fields,
            })
            .collect(),
        resolved: thread.resolved,
        anchor: match thread.anchor {
            AnnotationAnchor::Document { unknown_fields } => {
                CanonicalCommentAnchor::Document { unknown_fields }
            }
            AnnotationAnchor::Text {
                block,
                start,
                end,
                quote,
                context_before,
                context_after,
                orphaned,
                unknown_fields,
            } => CanonicalCommentAnchor::Text {
                block: EditorBlockId::from_bytes(block),
                range: EditorSelection::new(
                    DocumentPosition::from(start),
                    DocumentPosition::from(end),
                ),
                quote,
                context_before,
                context_after,
                orphaned,
                unknown_fields,
            },
        },
        unknown_fields: thread.unknown_fields,
    }
}

fn is_document_resource(path: &CanonicalRelativePath) -> bool {
    let path = path.as_str();
    (path.starts_with("manuscript/") || path.starts_with("research/")) && path.ends_with(".html")
}

fn searchable_text(html: &str) -> String {
    let mut text = String::with_capacity(html.len());
    let mut in_tag = false;
    for character in html.chars() {
        match character {
            '<' => in_tag = true,
            '>' => {
                in_tag = false;
                text.push(' ');
            }
            _ if !in_tag => text.push(character),
            _ => {}
        }
    }
    text
}

fn project_id(
    root: &parchmint_project_fs::ProjectRootCapability,
    resources: &BTreeMap<CanonicalRelativePath, Vec<u8>>,
) -> ProjectId {
    let root_id = CanonicalRelativePath::parse(".parchmint/root-id").expect("fixed path");
    let files = NativeProjectFileSystem::new();
    let identity = files.read(root, &root_id).unwrap_or_else(|_| {
        resources
            .get(&CanonicalRelativePath::parse("project.toml").expect("fixed path"))
            .cloned()
            .unwrap_or_default()
    });
    ProjectId::from_bytes(stable_id(b"project", &identity))
}

fn search_texts(
    project: &Project,
    document_id: DocumentId,
    body: &str,
) -> Vec<SearchTextProjection> {
    let mut texts = vec![SearchTextProjection {
        block_id: BlockId::from_bytes(*document_id.as_bytes()),
        field: SearchField::Body,
        text: searchable_text(body),
    }];
    let Some((_, node)) = project.nodes.iter().find(
        |(_, node)| matches!(node.kind, NodeKind::Document(candidate) if candidate == document_id),
    ) else {
        return texts;
    };
    texts.push(SearchTextProjection {
        block_id: BlockId::from_bytes(stable_id(b"search-title", document_id.as_bytes())),
        field: SearchField::DisplayTitle,
        text: node.title.clone(),
    });
    texts.push(SearchTextProjection {
        block_id: BlockId::from_bytes(stable_id(b"search-synopsis", document_id.as_bytes())),
        field: SearchField::Synopsis,
        text: node.synopsis.clone(),
    });
    texts.extend(node.metadata.iter().map(|(field, value)| {
        let mut identity = Vec::with_capacity(32);
        identity.extend_from_slice(document_id.as_bytes());
        identity.extend_from_slice(field.as_bytes());
        SearchTextProjection {
            block_id: BlockId::from_bytes(stable_id(b"search-metadata", &identity)),
            field: SearchField::Metadata(*field),
            text: value.clone(),
        }
    }));
    texts
}

fn stable_id(namespace: &[u8], value: &[u8]) -> [u8; 16] {
    let mut digest = Sha256::new();
    digest.update(namespace);
    digest.update([0]);
    digest.update(value);
    let digest = digest.finalize();
    let mut id = [0; 16];
    id.copy_from_slice(&digest[..16]);
    id
}

fn stable_id_text(bytes: &[u8; 16]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn history_root_id(project: ProjectId) -> u64 {
    let mut bytes = [0; 8];
    bytes.copy_from_slice(&project.as_bytes()[..8]);
    u64::from_be_bytes(bytes)
}

fn map_repository_error(error: RepositoryError) -> ProjectFilesystemError {
    match error {
        RepositoryError::Locked { path } => ProjectFilesystemError::Locked {
            path: path.as_path().to_path_buf(),
        },
        other => ProjectFilesystemError::failed("open", other.to_string()),
    }
}

fn injected_failure(operation: &'static str, kind: ProductionFaultKind) -> ProjectFilesystemError {
    ProjectFilesystemError::failed(operation, format!("injected {kind:?} fault"))
}

fn export_fault(operation: &'static str, kind: ProductionFaultKind) -> ExportError {
    match kind {
        ProductionFaultKind::Cancelled => ExportError::Cancelled,
        ProductionFaultKind::Io
        | ProductionFaultKind::Corruption
        | ProductionFaultKind::WorkerStopped => ExportError::Sink {
            operation,
            reason: format!("injected {kind:?} fault"),
        },
    }
}

fn spellcheck_fault(kind: ProductionFaultKind) -> SpellcheckError {
    match kind {
        ProductionFaultKind::Cancelled => SpellcheckError::QueueFull,
        ProductionFaultKind::Io
        | ProductionFaultKind::Corruption
        | ProductionFaultKind::WorkerStopped => SpellcheckError::WorkerStopped,
    }
}

struct ThreadWake(thread::Thread);

impl Wake for ThreadWake {
    fn wake(self: Arc<Self>) {
        self.0.unpark();
    }

    fn wake_by_ref(self: &Arc<Self>) {
        self.0.unpark();
    }
}

pub(crate) fn block_on<T>(future: impl Future<Output = T>) -> T {
    let mut future = Box::pin(future);
    let waker = Waker::from(Arc::new(ThreadWake(thread::current())));
    let mut context = Context::from_waker(&waker);
    loop {
        match future.as_mut().poll(&mut context) {
            Poll::Ready(value) => return value,
            Poll::Pending => thread::park(),
        }
    }
}

#[cfg(test)]
mod dictionary_source_tests {
    use super::*;
    use std::{
        fs,
        time::{SystemTime, UNIX_EPOCH},
    };

    use parchmint_preferences::PreferenceCommand;
    use parchmint_spellcheck_api::{
        EditorRevision, LanguageId, RevisionedTextRange, SpellcheckGeneration, SpellcheckPriority,
        SpellcheckRequest,
    };

    #[test]
    fn cancelled_native_export_preserves_target_and_removes_temporary_file() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let directory = std::env::temp_dir().join(format!("parchmint-export-cancel-{nonce}"));
        fs::create_dir(&directory).expect("temporary export directory");
        let target = directory.join("manuscript.html");
        fs::write(&target, b"previous output").expect("existing target");
        let (mut sink, name, _) = NativeExportSink::acquire(&target).expect("acquire output");
        let temporary = sink.temporary.clone();
        let target_capability =
            parchmint_export_api::ExportTargetCapability::checked(name).expect("target");
        let handle = ExportHandle::new();
        assert_eq!(handle.cancel(), CancelOutcome::Cancelled);
        assert!(matches!(
            handle.begin_temporary(&mut sink, &target_capability),
            Err(ExportError::Cancelled)
        ));
        drop(sink);

        assert_eq!(
            fs::read(&target).expect("preserved target"),
            b"previous output"
        );
        assert!(!temporary.exists(), "cancel must remove temporary output");
        fs::remove_file(&target).ok();
        fs::remove_dir(directory).ok();
    }

    struct FixedProjectQuery {
        snapshot: UiProjectSnapshot,
    }

    impl ProjectSnapshotQuery for FixedProjectQuery {
        fn snapshot(&self) -> Result<UiProjectSnapshot, ProjectQueryError> {
            Ok(self.snapshot.clone())
        }
    }

    fn project_snapshot(id: ProjectId, word: &str) -> UiProjectSnapshot {
        let mut project = Project::new(id);
        project
            .dictionary
            .insert(word)
            .expect("test dictionary word");
        UiProjectSnapshot {
            project,
            document_summaries: Vec::new(),
            documents: Vec::new(),
            styles_css: String::new(),
        }
    }

    #[test]
    fn persisted_dictionary_source_scopes_projects_and_reloads_global_preferences() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("test clock")
            .as_nanos();
        let path = std::env::temp_dir().join(format!("parchmint-dictionary-source-{nonce}.json"));
        let preferences: Arc<dyn PreferenceService> = Arc::new(PreferenceCoordinator::new(
            Arc::new(FilePreferenceStore::new(&path)),
        ));
        let source = Arc::new(ProductionDictionarySource::new(preferences.clone()));
        let first = ProjectId::from_bytes([11; 16]);
        let second = ProjectId::from_bytes([12; 16]);
        source.register_project(
            first,
            Arc::new(FixedProjectQuery {
                snapshot: project_snapshot(first, "Quillflux"),
            }),
        );
        source.register_project(
            second,
            Arc::new(FixedProjectQuery {
                snapshot: project_snapshot(second, "Fablewright"),
            }),
        );

        assert_eq!(
            source
                .project_words(first, DictionaryRevision::from(1))
                .expect("first project dictionary"),
            ["Quillflux"]
        );
        assert_eq!(
            source
                .project_words(second, DictionaryRevision::from(1))
                .expect("second project dictionary"),
            ["Fablewright"]
        );

        let current = block_on(preferences.load()).expect("load preferences");
        let updated = block_on(preferences.update(
            current.revision,
            PreferenceCommand::AddGlobalDictionaryWord("Globalthread".to_owned()),
        ))
        .expect("persist global dictionary word");
        assert_eq!(
            source
                .global_words(DictionaryRevision::from(updated.revision.value()))
                .expect("global dictionary"),
            ["Globalthread"]
        );

        let service = EnUsSpellcheckService::new(EnUsSpellcheckConfig {
            saved_dictionaries: source,
            ..EnUsSpellcheckConfig::default()
        })
        .expect("spellcheck service");
        block_on(service.reload_project_dictionary(first, DictionaryRevision::from(1)))
            .expect("hydrate project dictionary");
        block_on(
            service.reload_global_dictionary(DictionaryRevision::from(updated.revision.value())),
        )
        .expect("hydrate global dictionary");
        let request = SpellcheckRequest {
            language: LanguageId::EnUs,
            document_id: DocumentId::from_bytes([13; 16]),
            document_revision: EditorRevision::default(),
            blocks: vec![RevisionedTextRange {
                block_id: BlockId::from_bytes([13; 16]),
                range: parchmint_editor_api::EditorSelection::new(0_u64.into(), 22_u64.into()),
                text: "Quillflux Globalthread".to_owned(),
            }],
            project_dictionary: DictionaryRevision::from(1),
            global_dictionary: DictionaryRevision::from(updated.revision.value()),
            generation: SpellcheckGeneration::from(1),
            priority: SpellcheckPriority::Visible,
        };
        let mut results = block_on(service.check(request)).expect("spellcheck request");
        assert!(
            results.next().expect("spellcheck result").issues.is_empty(),
            "both persisted dictionary scopes must be active before recheck"
        );
        let _ = fs::remove_file(path);
    }
}
