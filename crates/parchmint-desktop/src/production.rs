use std::{
    collections::{BTreeMap, VecDeque},
    fs,
    future::Future,
    path::{Path, PathBuf},
    pin::Pin,
    sync::{Arc, Mutex},
    task::{Context, Poll, Wake, Waker},
    thread,
    time::Duration,
};

use parchmint_application::{
    DocumentSnapshot, DocumentVisibility, EditorPersistenceCoordinator, NativeDocumentStateOwner,
    NativeProjectCommandDispatcher,
};
use parchmint_domain::{
    BlockId, DocumentId, NodeId, Project, ProjectCommand, ProjectId, apply_project_command,
};
use parchmint_editor_iced::{EditorIcedAdapter, EditorIcedConfig};
use parchmint_export_api::{
    ExportError, ExportHandle, ExportPlan, ExportRequest, ExportSink, ExportValidationReport,
    Exporter, ProjectSnapshot as ExportProjectSnapshot,
};
use parchmint_export_html::HtmlExporter;
use parchmint_history_api::{self as history, HistoryStore, ProjectRootCapability as HistoryRoot};
use parchmint_history_git2::Git2HistoryStore;
use parchmint_platform_api::WindowCapability;
use parchmint_platform_native::{NativePlatform, iced_adapter::IcedWindowRegistry};
use parchmint_preferences::{
    AppearanceController, FilePreferenceStore, PreferenceCoordinator, PreferenceService,
};
use parchmint_project_format::{CanonicalCodec, CanonicalRelativePath, ProjectFormatCodec};
use parchmint_project_fs::{
    FsAtomicWriter, FsProjectRepository, NativeAtomicFileOps, NativeProjectFileSystem,
    ProjectFileSystem,
};
use parchmint_project_repository::{OpenProject, ProjectPath, ProjectRepository, RepositoryError};
use parchmint_recovery_api::{self as recovery, RecoveryJournal};
use parchmint_recovery_fs::FsRecoveryJournal;
use parchmint_save::{
    CheckpointIntent, CheckpointIntentStore, CheckpointReceipt, IntentStoreError,
    ProjectSaveCoordinator, SaveCoordinator, SaveRequest, SaveTicket,
};
use parchmint_search_api::{
    self as search, RevisionId, SearchDocumentProjection, SearchField, SearchIndex,
    SearchProjectionSource, SearchProjectionVisitor, SearchTextProjection,
};
use parchmint_search_sqlite::SqliteSearchIndex;
use parchmint_spellcheck_api::SpellcheckService;
use parchmint_spellcheck_en_us::{EnUsSpellcheckService, SpellcheckError, SpellcheckOperation};
use parchmint_ui_iced::{Shell, ShellWindows};
use parchmint_workspace_state::FileWorkspaceStateStore;
use sha2::{Digest, Sha256};

use crate::{
    ApplicationServices, DesktopBootstrap, DesktopRuntime, DesktopStartup, DesktopUi,
    DesktopUiError, PlatformServices, ProjectFilesystemError, ProjectFilesystemService,
    ProjectSession, RequestedProjectPath, StartupError,
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
}

impl ControlledSearch {
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
    ) -> Result<ExportHandle, ExportError> {
        if let Some(kind) = self.controls.take_fault(ProductionFaultPoint::Export) {
            self.controls
                .service_operation(ProductionFaultPoint::Export, "write", false);
            return Err(export_fault("write", kind));
        }
        let result = self.inner.export(plan, sink);
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
    exporter: Arc<ControlledExporter>,
    workspace_state: Arc<FileWorkspaceStateStore>,
    controls: ProductionControls,
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
pub struct ProductionProjectSession {
    path: PathBuf,
    project_id: ProjectId,
    commands: Arc<NativeProjectCommandDispatcher>,
    history: Arc<ControlledHistory>,
    recovery: Arc<ControlledRecovery>,
    search: Arc<ControlledSearch>,
    save: Arc<ProjectSaveCoordinator>,
    persistence: Arc<EditorPersistenceCoordinator>,
    _repository: Arc<FsProjectRepository>,
    _open_project: OpenProject,
    controls: ProductionControls,
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
        let application = match self.commands.capture_save_request() {
            Ok(application) => application,
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
        if application.dirty_resources.iter().next().is_some() {
            self.controls.service_operation(
                ProductionFaultPoint::FinalSave,
                "reconcile final save",
                false,
            );
            return Err(ProjectFilesystemError::failed(
                "save",
                "dirty application state has no acknowledged encoded save frontier",
            ));
        }
        if let Err(error) = self.save.reconcile_open() {
            self.controls.service_operation(
                ProductionFaultPoint::FinalSave,
                "reconcile final save",
                false,
            );
            return Err(ProjectFilesystemError::failed(
                "reconcile save",
                error.to_string(),
            ));
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
        let project_id = project_id(&root, &resources);
        let (project, documents, search_source) = application_state(project_id, &resources)?;
        let recovery_base = recovery_base(&documents, &resources);
        let document_owner = Arc::new(NativeDocumentStateOwner::new(documents));
        let commands = Arc::new(NativeProjectCommandDispatcher::new(project, document_owner));

        let history = Arc::new(ControlledHistory {
            inner: Git2HistoryStore::new(root.clone()),
            controls: self.shared.controls.clone(),
        });
        history
            .initialize(HistoryRoot::new(history_root_id(project_id)))
            .map_err(|error| {
                ProjectFilesystemError::failed("initialize History", error.to_string())
            })?;
        let recovery = Arc::new(ControlledRecovery {
            inner: FsRecoveryJournal::open(&root_path).map_err(|error| {
                ProjectFilesystemError::failed("initialize recovery", error.to_string())
            })?,
            controls: self.shared.controls.clone(),
        });
        let search = Arc::new(ControlledSearch {
            inner: SqliteSearchIndex::new(&root_path),
            controls: self.shared.controls.clone(),
        });
        search
            .open_or_rebuild(project_id, &search_source)
            .map_err(|error| {
                ProjectFilesystemError::failed("initialize search", error.to_string())
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
            recovery_base,
        ));

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
            recovery,
            search,
            save,
            persistence,
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
}

struct ProductionDesktopUi {
    windows: Mutex<ShellWindows>,
    registry: IcedWindowRegistry,
    controls: ProductionControls,
}

impl DesktopUi for ProductionDesktopUi {
    fn start(&self, _startup: DesktopStartup) -> Result<(), DesktopUiError> {
        Ok(())
    }

    fn project_opened(
        &self,
        _project: &RequestedProjectPath,
        window: WindowCapability,
        session: parchmint_ui_api::ProjectSessionCapability,
    ) -> Result<(), DesktopUiError> {
        let window = self.registry.register_window(window);
        self.windows
            .lock()
            .map_err(|_| DesktopUiError::new("Iced window registry is unavailable"))?
            .insert(window, Shell::new(window));
        self.controls.observe(ProductionObservation::WindowOpened {
            window,
            session_id: session.session_id(),
            session_generation: session.generation(),
        });
        Ok(())
    }

    fn focus_window(&self, window: WindowCapability) -> Result<(), DesktopUiError> {
        let known = self
            .windows
            .lock()
            .map_err(|_| DesktopUiError::new("Iced window registry is unavailable"))?
            .values()
            .any(|shell| shell.window() == window);
        if !known {
            return Err(DesktopUiError::new("cannot focus a stale project window"));
        }
        self.controls
            .observe(ProductionObservation::WindowFocused(window));
        Ok(())
    }

    fn project_locked(&self, project: &RequestedProjectPath) -> Result<(), DesktopUiError> {
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
        self.registry.close_window(window);
        self.windows
            .lock()
            .map_err(|_| DesktopUiError::new("Iced window registry is unavailable"))?
            .remove(window);
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

    fn run(&self, _runtime: DesktopRuntime) -> Result<parchmint_ui_api::ExitCode, DesktopUiError> {
        // The model and native capabilities are live here. A platform desktop
        // interaction driver owns entering and exiting the Iced event loop;
        // headless tests must not manufacture that evidence.
        Ok(parchmint_ui_api::ExitCode::SUCCESS)
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
    let editor = Arc::new(
        EditorIcedAdapter::new(EditorIcedConfig::default())
            .map_err(|error| StartupError::production("editor", error))?,
    );
    let spellcheck = Arc::new(
        EnUsSpellcheckService::new(Default::default())
            .map_err(|error| StartupError::production("spellcheck", error))?,
    );
    let shared = Arc::new(SharedServices {
        editor,
        spellcheck,
        exporter: Arc::new(ControlledExporter {
            inner: HtmlExporter,
            controls: controls.clone(),
        }),
        workspace_state: Arc::new(FileWorkspaceStateStore::new(
            paths.data().join("workspaces"),
        )),
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
        windows: Mutex::new(Shell::windows()),
        registry: platform.iced_window_registry(),
        controls,
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

fn canonical_resources(
    root: &parchmint_project_fs::ProjectRootCapability,
) -> Result<BTreeMap<CanonicalRelativePath, Vec<u8>>, ProjectFilesystemError> {
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
    let files = NativeProjectFileSystem::new();
    paths
        .into_iter()
        .map(|path| {
            files
                .read(root, &path)
                .map(|bytes| (path, bytes))
                .map_err(|error| {
                    ProjectFilesystemError::failed("read canonical resource", error.to_string())
                })
        })
        .collect()
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

struct CanonicalSearchSource(Vec<SearchDocumentProjection>);

impl SearchProjectionSource for CanonicalSearchSource {
    fn visit_projections(
        &self,
        visitor: &mut dyn SearchProjectionVisitor,
    ) -> Result<(), parchmint_search_api::SearchError> {
        for projection in &self.0 {
            visitor.visit(projection.clone())?;
        }
        Ok(())
    }
}

fn application_state(
    project_id: ProjectId,
    resources: &BTreeMap<CanonicalRelativePath, Vec<u8>>,
) -> Result<(Project, Vec<DocumentSnapshot>, CanonicalSearchSource), ProjectFilesystemError> {
    let codec = ProjectFormatCodec::default();
    let mut project = Project::new(project_id);
    let mut documents = Vec::new();
    let mut projections = Vec::new();
    let mut section_counts = BTreeMap::from([
        (NodeId::manuscript_root(), 0usize),
        (NodeId::research_root(), 0usize),
    ]);
    for (path, bytes) in resources {
        if !is_document_resource(path) {
            continue;
        }
        let canonical = codec.decode_document(bytes).map_err(|error| {
            ProjectFilesystemError::failed("decode canonical document", error.to_string())
        })?;
        let id = stable_id(b"document", path.as_str().as_bytes());
        let document_id = DocumentId::from_bytes(id);
        let node_id = NodeId::from_bytes(stable_id(b"node", path.as_str().as_bytes()));
        let parent = if path.as_str().starts_with("research/") {
            NodeId::research_root()
        } else {
            NodeId::manuscript_root()
        };
        let index = section_counts
            .get_mut(&parent)
            .expect("both project sections have counters");
        let title = Path::new(path.as_str())
            .file_stem()
            .and_then(|value| value.to_str())
            .unwrap_or("Untitled Document")
            .to_owned();
        let applied = apply_project_command(
            &project,
            project.revision,
            ProjectCommand::create_document(node_id, document_id, parent, *index, title),
        )
        .map_err(|error| {
            ProjectFilesystemError::failed("assemble project model", error.to_string())
        })?;
        project = applied.project;
        *index += 1;
        let body = canonical.as_html().to_owned();
        documents.push(DocumentSnapshot {
            document_id,
            body: body.clone(),
            revision: Default::default(),
            visibility: DocumentVisibility::Closed,
        });
        projections.push(SearchDocumentProjection {
            document_id,
            revision: RevisionId::from(0),
            texts: vec![SearchTextProjection {
                block_id: BlockId::from_bytes(id),
                field: SearchField::Body,
                text: searchable_text(&body),
            }],
        });
    }
    project.revision = Default::default();
    Ok((project, documents, CanonicalSearchSource(projections)))
}

fn recovery_base(
    documents: &[DocumentSnapshot],
    resources: &BTreeMap<CanonicalRelativePath, Vec<u8>>,
) -> recovery::RecoveryBaseSnapshot {
    let revisions = documents
        .iter()
        .map(|document| (document.document_id, recovery::DocumentRevision::default()))
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
                recovery::ResourceId::Document
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
        revisions: recovery::RecoveryRevisionVector::new(Default::default(), revisions),
        hashes,
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

fn block_on<T>(mut future: Pin<Box<dyn Future<Output = T> + Send>>) -> T {
    let waker = Waker::from(Arc::new(ThreadWake(thread::current())));
    let mut context = Context::from_waker(&waker);
    loop {
        match future.as_mut().poll(&mut context) {
            Poll::Ready(value) => return value,
            Poll::Pending => thread::park(),
        }
    }
}
