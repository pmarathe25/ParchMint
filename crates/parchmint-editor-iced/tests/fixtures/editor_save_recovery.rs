use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    fs,
    path::{Path, PathBuf},
    sync::{
        Arc, Condvar, Mutex,
        atomic::{AtomicU64, Ordering},
    },
    thread::{self, JoinHandle},
    time::Duration,
};

use iced_test::futures::futures::executor::block_on;
use parchmint_application::{
    DocumentCommand, DocumentSnapshot, DocumentVisibility, EditorPersistenceCoordinator,
    NativeDocumentStateOwner, NativeProjectCommandDispatcher, Project, ProjectId, ProjectRevision,
    RevisionedSaveRequest,
};
use parchmint_contracts::generated::RecoveryRecordV1;
use parchmint_editor_api::{
    CanonicalDocumentLoad, CanonicalProjection, DocumentId, DocumentPosition,
    DurableProjectionBatch, EditorAdapter, EditorCommand, EditorCommandKind, EditorCommandOrigin,
    EditorError, EditorRevision, EditorSelection, SharedEditorSession, ViewId,
};
use parchmint_editor_iced::{EditorIcedAdapter, EditorIcedConfig, ProjectionBudget};
use parchmint_history_api::{
    CheckpointId, CheckpointInput, CheckpointIntentHash, HistoryCursor, HistoryError,
    HistoryIntegrityReport, HistoryPage, HistoryPageQuery, HistoryState, HistoryStore,
    MaintenanceBudget, MaintenanceReport, RestorePlan, SnapshotPreview,
};
use parchmint_platform_api::WindowCapability;
use parchmint_project_repository::{
    Abandonment, AtomicWritePlan, AtomicWriter, CommitReceipt, InMemoryAtomicWriter,
    ProjectRootCapability, Reconciliation, SaveTransactionRecord, StagedResource, StagedWrite,
    ValidationReport, WriteError,
};
use parchmint_recovery_api::{
    ContentHash, DocumentRevision, DurableRevisionVector, RecoveryBaseSnapshot, RecoveryBatch,
    RecoveryError, RecoveryJournal, RecoveryReceipt, RecoveryReplay, RecoveryRevisionVector,
    ResourceId, VersionedRecoveryPayload,
};
use parchmint_recovery_fs::FsRecoveryJournal;
use parchmint_save::{
    CheckpointCategory, CheckpointIntent, CheckpointIntentStore, CheckpointReceipt,
    IntentStoreError, ProjectSaveCoordinator, SaveError, SaveGeneration, SavePriority, SaveRequest,
    SaveRevisionVector, SaveState, SavedAcknowledgement,
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

const WAIT: Duration = Duration::from_secs(3);
const DOCUMENT_PATH: &str = "manuscript/editor-integration.html";
const DOCUMENT: DocumentId = DocumentId::from_bytes([34; 16]);
const PROJECT: ProjectId = ProjectId::from_bytes([34; 16]);
const VIEW: ViewId = ViewId::from_bytes([34; 16]);

#[allow(dead_code)]
pub const fn document_id() -> DocumentId {
    DOCUMENT
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Boundary {
    BeforeProjection,
    AfterProjection,
    BeforeRecoveryAppend,
    /// The journal has durably accepted the batch; frontier acknowledgement is next.
    AfterRecoveryAppend,
    BeforeSave,
    AfterCanonicalCommit,
    BeforeSaveAcknowledgement,
    ForcedTermination,
}

#[derive(Debug, Default)]
struct BoundaryState {
    paused: BTreeSet<Boundary>,
    released: BTreeSet<Boundary>,
    reached: BTreeMap<Boundary, usize>,
}

#[derive(Debug, Default)]
pub struct BoundaryController {
    state: Mutex<BoundaryState>,
    changed: Condvar,
}

impl BoundaryController {
    pub fn pause_at(&self, boundary: Boundary) {
        self.state
            .lock()
            .expect("boundary lock")
            .paused
            .insert(boundary);
    }

    pub fn release(&self, boundary: Boundary) {
        let mut state = self.state.lock().expect("boundary lock");
        state.released.insert(boundary);
        self.changed.notify_all();
    }

    pub fn release_all(&self) {
        let mut state = self.state.lock().expect("boundary lock");
        let paused = state.paused.iter().copied().collect::<Vec<_>>();
        state.released.extend(paused);
        self.changed.notify_all();
    }

    pub fn wait_until(&self, boundary: Boundary) {
        self.wait_until_count(boundary, 1);
    }

    pub fn wait_until_count(&self, boundary: Boundary, expected: usize) {
        let state = self.state.lock().expect("boundary lock");
        let (state, timeout) = self
            .changed
            .wait_timeout_while(state, WAIT, |state| {
                state.reached.get(&boundary).copied().unwrap_or_default() < expected
            })
            .expect("boundary lock");
        assert!(
            state.reached.get(&boundary).copied().unwrap_or_default() >= expected,
            "worker did not reach {boundary:?}"
        );
        assert!(!timeout.timed_out(), "waiting for {boundary:?} timed out");
    }

    pub fn count(&self, boundary: Boundary) -> usize {
        self.state
            .lock()
            .expect("boundary lock")
            .reached
            .get(&boundary)
            .copied()
            .unwrap_or_default()
    }

    fn reach(&self, boundary: Boundary) {
        let mut state = self.state.lock().expect("boundary lock");
        *state.reached.entry(boundary).or_default() += 1;
        self.changed.notify_all();
        while state.paused.contains(&boundary) && !state.released.contains(&boundary) {
            state = self.changed.wait(state).expect("boundary lock");
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PersistenceFailure {
    Projection(EditorError),
    Recovery(RecoveryError),
    Save(SaveError),
    RevisionMismatch,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PersistenceStatus {
    pub state: SaveState,
    pub requested: Option<SaveRevisionVector>,
    pub active: Option<SaveRevisionVector>,
    pub saved_through: Option<SaveRevisionVector>,
    pub failure: Option<PersistenceFailure>,
}

impl Default for PersistenceStatus {
    fn default() -> Self {
        Self {
            state: SaveState::Clean,
            requested: None,
            active: None,
            saved_through: None,
            failure: None,
        }
    }
}

#[derive(Debug, Clone)]
struct PendingProjection {
    revision: EditorRevision,
    application: RevisionedSaveRequest,
}

#[derive(Debug)]
struct PipelineState {
    pending: VecDeque<PendingProjection>,
    active: Option<EditorRevision>,
    save_targets: BTreeSet<EditorRevision>,
    current_revision: EditorRevision,
    max_backlog: usize,
    max_save_backlog: usize,
    max_recovery_backlog: usize,
    projected: usize,
    recovery_batches: usize,
    recovery_project_revision: ProjectRevision,
    recovery_document_revision: DocumentRevision,
    recovery_hash: ContentHash,
    acknowledgements: Vec<SavedAcknowledgement>,
    in_flight_recovery: Option<RecoveryBatch>,
    in_flight_receipt: Option<RecoveryReceipt>,
    status: PersistenceStatus,
    crash_requested: bool,
    stopping: bool,
}

impl PipelineState {
    fn new(initial_body: &str) -> Self {
        Self {
            pending: VecDeque::new(),
            active: None,
            save_targets: BTreeSet::new(),
            current_revision: EditorRevision::default(),
            max_backlog: 0,
            max_save_backlog: 0,
            max_recovery_backlog: 0,
            projected: 0,
            recovery_batches: 0,
            recovery_project_revision: ProjectRevision::default(),
            recovery_document_revision: DocumentRevision::default(),
            recovery_hash: content_hash(initial_body.as_bytes()),
            acknowledgements: Vec::new(),
            in_flight_recovery: None,
            in_flight_receipt: None,
            status: PersistenceStatus::default(),
            crash_requested: false,
            stopping: false,
        }
    }

    fn backlog(&self) -> usize {
        usize::from(self.active.is_some()) + self.pending.len()
    }

    fn observe_backlog(&mut self) {
        self.max_backlog = self.max_backlog.max(self.backlog());
        self.max_save_backlog = self
            .max_save_backlog
            .max(self.save_targets.len() + usize::from(self.status.active.is_some()));
        self.max_recovery_backlog = self
            .max_recovery_backlog
            .max(self.pending.len() + usize::from(self.in_flight_recovery.is_some()));
    }

    fn enqueue(&mut self, application: RevisionedSaveRequest, save: bool) {
        let revision = application.open_documents[&DOCUMENT];
        self.current_revision = revision;
        let vector = save_vector(&application, None);
        self.status.requested = Some(vector.clone());
        if self.status.state != SaveState::Error {
            self.status.state = if self.status.active.is_some() {
                SaveState::Saving
            } else {
                SaveState::Dirty
            };
        }
        if save {
            self.save_targets.insert(revision);
            self.status.active = Some(vector);
            if self.status.state != SaveState::Error {
                self.status.state = SaveState::Saving;
            }
        }

        if self.active == Some(revision)
            || self
                .pending
                .iter()
                .any(|pending| pending.revision == revision)
        {
            self.observe_backlog();
            return;
        }

        let last_is_unpinned = self
            .pending
            .back()
            .is_some_and(|pending| !self.save_targets.contains(&pending.revision));
        if last_is_unpinned {
            self.pending.pop_back();
        }
        if self.pending.len() == 2 {
            let replace = self
                .pending
                .iter()
                .rposition(|pending| !self.save_targets.contains(&pending.revision))
                .unwrap_or(1);
            self.pending.remove(replace);
        }
        self.pending.push_back(PendingProjection {
            revision,
            application,
        });
        self.observe_backlog();
    }

    fn newest_unpinned(&mut self, pending: PendingProjection) -> PendingProjection {
        if self.save_targets.contains(&pending.revision) {
            return pending;
        }
        let Some(newest) = self.pending.back().cloned() else {
            return pending;
        };
        if self.save_targets.contains(&newest.revision) {
            return pending;
        }
        self.pending.clear();
        self.active = Some(newest.revision);
        newest
    }

    fn fail(&mut self, failure: PersistenceFailure) {
        self.status.state = SaveState::Error;
        self.status.active = None;
        self.status.failure = Some(failure);
    }
}

#[derive(Debug)]
struct SharedPipeline {
    state: Mutex<PipelineState>,
    changed: Condvar,
}

impl SharedPipeline {
    fn new(initial_body: &str) -> Self {
        Self {
            state: Mutex::new(PipelineState::new(initial_body)),
            changed: Condvar::new(),
        }
    }
}

#[derive(Debug)]
struct RecordingWriter {
    inner: InMemoryAtomicWriter,
    boundaries: Arc<BoundaryController>,
    committed: Arc<Mutex<Vec<AtomicWritePlan>>>,
}

impl AtomicWriter for RecordingWriter {
    fn stage(&self, plan: AtomicWritePlan) -> Result<StagedWrite, WriteError> {
        self.inner.stage(plan)
    }

    fn validate_staged(&self, staged: &StagedWrite) -> ValidationReport {
        self.inner.validate_staged(staged)
    }

    fn commit(&self, staged: StagedWrite) -> Result<CommitReceipt, WriteError> {
        let plan = staged.plan().clone();
        let receipt = self.inner.commit(staged)?;
        self.committed.lock().expect("commit log").push(plan);
        self.boundaries.reach(Boundary::AfterCanonicalCommit);
        Ok(receipt)
    }

    fn reconcile(&self, record: SaveTransactionRecord) -> Result<Reconciliation, WriteError> {
        self.inner.reconcile(record)
    }

    fn abandon(&self, staged: StagedWrite) -> Result<Abandonment, WriteError> {
        self.inner.abandon(staged)
    }
}

#[derive(Debug)]
struct AcknowledgementIntentStore {
    inner: Arc<FsRecoveryJournal>,
    boundaries: Arc<BoundaryController>,
}

impl CheckpointIntentStore for AcknowledgementIntentStore {
    fn persist(&self, intent: CheckpointIntent) -> Result<(), IntentStoreError> {
        self.inner.persist(intent)
    }

    fn pending(&self) -> Result<Vec<CheckpointIntent>, IntentStoreError> {
        self.inner.pending()
    }

    fn complete(&self, receipt: CheckpointReceipt) -> Result<(), IntentStoreError> {
        self.boundaries.reach(Boundary::BeforeSaveAcknowledgement);
        self.inner.complete(receipt)
    }
}

#[derive(Debug, Default)]
struct RecordingHistory {
    checkpoints: Mutex<BTreeMap<CheckpointIntentHash, CheckpointId>>,
}

impl HistoryStore for RecordingHistory {
    fn initialize(&self, project: ProjectRootCapability) -> Result<HistoryState, HistoryError> {
        Ok(HistoryState {
            project,
            checkpoint_count: self.checkpoints.lock().expect("history lock").len(),
        })
    }

    fn checkpoint(&self, input: CheckpointInput) -> Result<CheckpointId, HistoryError> {
        input.validate()?;
        let mut checkpoints = self.checkpoints.lock().expect("history lock");
        let next = u8::try_from(checkpoints.len() + 1).expect("small fixture history");
        Ok(*checkpoints
            .entry(input.intent_hash)
            .or_insert_with(|| CheckpointId::from_bytes([next; 16])))
    }

    fn list(&self, _query: HistoryPageQuery) -> Result<HistoryPage, HistoryError> {
        Ok(HistoryPage {
            checkpoints: Vec::new(),
            next_cursor: None::<HistoryCursor>,
        })
    }

    fn preview(&self, checkpoint: CheckpointId) -> Result<SnapshotPreview, HistoryError> {
        Err(HistoryError::UnknownCheckpoint { checkpoint })
    }

    fn restore(&self, checkpoint: CheckpointId) -> Result<RestorePlan, HistoryError> {
        Err(HistoryError::UnknownCheckpoint { checkpoint })
    }

    fn verify(&self) -> Result<HistoryIntegrityReport, HistoryError> {
        Ok(HistoryIntegrityReport {
            checked_checkpoints: self.checkpoints.lock().expect("history lock").len(),
        })
    }

    fn maintain(&self, _budget: MaintenanceBudget) -> Result<MaintenanceReport, HistoryError> {
        Ok(MaintenanceReport {
            checked_objects: 0,
            retained_checkpoints: self.checkpoints.lock().expect("history lock").len(),
        })
    }
}

static PROJECT_SEQUENCE: AtomicU64 = AtomicU64::new(1);

#[derive(Debug)]
struct TemporaryProject(PathBuf);

impl TemporaryProject {
    fn new() -> Self {
        let sequence = PROJECT_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "parchmint-editor-save-recovery-{}-{sequence}",
            std::process::id()
        ));
        fs::create_dir(&path).expect("temporary project directory");
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TemporaryProject {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

pub struct EditorSaveRecoveryHarness {
    adapter: Arc<EditorIcedAdapter>,
    session: SharedEditorSession,
    application: NativeProjectCommandDispatcher,
    application_body: Mutex<String>,
    shared: Arc<SharedPipeline>,
    boundaries: Arc<BoundaryController>,
    journal: Arc<FsRecoveryJournal>,
    save: Arc<dyn parchmint_save::SaveCoordinator>,
    persistence: Arc<EditorPersistenceCoordinator>,
    committed: Arc<Mutex<Vec<AtomicWritePlan>>>,
    worker: Option<JoinHandle<()>>,
    project: TemporaryProject,
    initial_body: String,
}

impl EditorSaveRecoveryHarness {
    pub fn new(initial_body: &str) -> Self {
        Self::with_projection_budget(initial_body, ProjectionBudget::default().retained_revisions)
    }

    pub fn with_projection_budget(initial_body: &str, retained_revisions: usize) -> Self {
        let project = TemporaryProject::new();
        let adapter = Arc::new(
            EditorIcedAdapter::new(EditorIcedConfig {
                projection_budget: ProjectionBudget { retained_revisions },
                ..EditorIcedConfig::default()
            })
            .expect("editor adapter"),
        );
        let session = adapter
            .open_session(CanonicalDocumentLoad::new(DOCUMENT, initial_body))
            .expect("editor session");
        let host = adapter
            .create_view_host(WindowCapability::new(34, 1), VIEW)
            .expect("editor host");
        adapter
            .attach_view(session.clone(), VIEW, host)
            .expect("mounted editor");
        let end = DocumentPosition::from(
            u64::try_from(initial_body.chars().count()).expect("fixture body length"),
        );
        adapter
            .execute(
                session.clone(),
                EditorCommandOrigin::new(VIEW),
                EditorCommand::new(
                    EditorRevision::default(),
                    EditorCommandKind::SetSelection {
                        selection: EditorSelection::new(end, end),
                    },
                ),
            )
            .expect("initial editor caret");
        let application_documents = Arc::new(NativeDocumentStateOwner::new([DocumentSnapshot {
            document_id: DOCUMENT,
            body: initial_body.to_owned(),
            revision: EditorRevision::default(),
            visibility: DocumentVisibility::Open,
            comments: Vec::new(),
        }]));
        let application =
            NativeProjectCommandDispatcher::new(Project::new(PROJECT), application_documents);

        let boundaries = Arc::new(BoundaryController::default());
        let journal =
            Arc::new(FsRecoveryJournal::open(project.path()).expect("filesystem recovery journal"));
        let committed = Arc::new(Mutex::new(Vec::new()));
        let writer = Arc::new(RecordingWriter {
            inner: InMemoryAtomicWriter::new(ProjectRootCapability::new(34)),
            boundaries: boundaries.clone(),
            committed: committed.clone(),
        });
        let history = Arc::new(RecordingHistory::default());
        let intents = Arc::new(AcknowledgementIntentStore {
            inner: journal.clone(),
            boundaries: boundaries.clone(),
        });
        let save: Arc<dyn parchmint_save::SaveCoordinator> = Arc::new(
            ProjectSaveCoordinator::new(PROJECT, writer, history, intents)
                .expect("save coordinator"),
        );
        let persistence = Arc::new(EditorPersistenceCoordinator::new(
            journal.clone(),
            save.clone(),
            recovery_base(initial_body),
        ));
        let shared = Arc::new(SharedPipeline::new(initial_body));
        let worker = spawn_worker(
            adapter.clone(),
            session.clone(),
            shared.clone(),
            boundaries.clone(),
            persistence.clone(),
        );

        Self {
            adapter,
            session,
            application,
            application_body: Mutex::new(initial_body.to_owned()),
            shared,
            boundaries,
            journal,
            save,
            persistence,
            committed,
            worker: Some(worker),
            project,
            initial_body: initial_body.to_owned(),
        }
    }

    pub fn boundaries(&self) -> &BoundaryController {
        self.boundaries.as_ref()
    }

    pub fn type_text(&self, text: &str, request_save: bool) -> EditorRevision {
        self.adapter
            .input_en_us(self.session.clone(), VIEW, text)
            .expect("mounted input remains available");
        let revision = self
            .adapter
            .revision(self.session.clone())
            .expect("editor revision");
        let mut body = self.application_body.lock().expect("application body lock");
        body.push_str(text);
        let result = self
            .application
            .execute_document(DocumentCommand {
                document_id: DOCUMENT,
                observed_revision: EditorRevision::from(revision.value() - 1),
                body: body.clone(),
            })
            .expect("application document revision");
        assert_eq!(result.revision, revision);
        let application = self
            .application
            .capture_save_request()
            .expect("application revision capture");
        assert_eq!(application.open_documents[&DOCUMENT], revision);
        let mut state = self.shared.state.lock().expect("pipeline lock");
        state.enqueue(application, request_save);
        self.shared.changed.notify_all();
        revision
    }

    pub fn wait_until_idle(&self) {
        let state = self.shared.state.lock().expect("pipeline lock");
        let (state, timeout) = self
            .shared
            .changed
            .wait_timeout_while(state, WAIT, |state| {
                state.active.is_some() || !state.pending.is_empty()
            })
            .expect("pipeline lock");
        assert!(
            state.active.is_none() && state.pending.is_empty(),
            "persistence worker did not become idle"
        );
        assert!(!timeout.timed_out(), "persistence worker timed out");
    }

    pub fn status(&self) -> PersistenceStatus {
        self.shared
            .state
            .lock()
            .expect("pipeline lock")
            .status
            .clone()
    }

    #[allow(dead_code)]
    pub fn production_status(&self) -> parchmint_application::EditorPersistenceStatus {
        self.persistence.status()
    }

    pub fn acknowledgements(&self) -> Vec<SavedAcknowledgement> {
        self.shared
            .state
            .lock()
            .expect("pipeline lock")
            .acknowledgements
            .clone()
    }

    #[allow(dead_code)]
    pub fn max_backlog(&self) -> usize {
        self.shared.state.lock().expect("pipeline lock").max_backlog
    }

    #[allow(dead_code)]
    pub fn queue_bounds(&self) -> (usize, usize, usize) {
        let state = self.shared.state.lock().expect("pipeline lock");
        (
            state.max_backlog,
            state.max_save_backlog,
            state.max_recovery_backlog,
        )
    }

    pub fn projected_count(&self) -> usize {
        self.shared.state.lock().expect("pipeline lock").projected
    }

    pub fn recovery_batch_count(&self) -> usize {
        self.shared
            .state
            .lock()
            .expect("pipeline lock")
            .recovery_batches
    }

    #[allow(dead_code)]
    /// Returns the durable batch that has not yet advanced the in-memory frontier.
    pub fn in_flight_recovery(&self) -> Option<RecoveryBatch> {
        self.shared
            .state
            .lock()
            .expect("pipeline lock")
            .in_flight_recovery
            .clone()
    }

    #[allow(dead_code)]
    pub fn in_flight_receipt(&self) -> Option<RecoveryReceipt> {
        self.shared
            .state
            .lock()
            .expect("pipeline lock")
            .in_flight_receipt
            .clone()
    }

    pub fn committed_bodies(&self) -> Vec<String> {
        self.committed
            .lock()
            .expect("commit log")
            .iter()
            .flat_map(|plan| plan.writes.iter())
            .map(|write| String::from_utf8(write.bytes.clone()).expect("fixture UTF-8"))
            .collect()
    }

    pub fn replay(&self) -> RecoveryReplay {
        self.journal
            .replay(recovery_base(&self.initial_body))
            .expect("recovery replay")
    }

    pub fn force_terminate(&mut self) {
        self.boundaries.reach(Boundary::ForcedTermination);
        {
            let mut state = self.shared.state.lock().expect("pipeline lock");
            state.crash_requested = true;
            state.stopping = true;
            state.pending.clear();
            self.shared.changed.notify_all();
        }
        self.boundaries.release_all();
        self.stop_worker();
    }

    pub fn replay_after_reopen(&self, initial_body: &str) -> RecoveryReplay {
        let journal = Arc::new(
            FsRecoveryJournal::open(self.project.path()).expect("reopen recovery journal"),
        );
        let persistence = EditorPersistenceCoordinator::new(
            journal,
            self.save.clone(),
            recovery_base(initial_body),
        );
        persistence
            .reconcile_recovery(recovery_base(initial_body))
            .expect("replay after forced termination")
    }

    #[allow(dead_code)]
    pub fn reconciled_frontier_after_reopen(&self, initial_body: &str) -> RecoveryRevisionVector {
        let journal = Arc::new(
            FsRecoveryJournal::open(self.project.path()).expect("reopen recovery journal"),
        );
        let persistence = EditorPersistenceCoordinator::new(
            journal,
            self.save.clone(),
            recovery_base(initial_body),
        );
        persistence
            .reconcile_recovery(recovery_base(initial_body))
            .expect("reconcile after forced termination");
        persistence.frontier().expect("reconciled frontier")
    }

    #[allow(dead_code)]
    pub fn resume_interrupted_recovery_after_reopen(
        &self,
        initial_body: &str,
    ) -> RecoveryRevisionVector {
        let batch = self
            .in_flight_recovery()
            .expect("interrupted durable batch");
        let receipt = self
            .in_flight_receipt()
            .expect("original durable recovery receipt");
        let journal = Arc::new(
            FsRecoveryJournal::open(self.project.path()).expect("reopen recovery journal"),
        );
        let persistence = EditorPersistenceCoordinator::new(
            journal,
            self.save.clone(),
            recovery_base(initial_body),
        );
        persistence
            .resume_recovery_acknowledgement(
                recovery_base(initial_body),
                DurableProjectionBatch::new(batch, receipt)
                    .expect("original receipt must authenticate the batch"),
            )
            .expect("resume interrupted recovery acknowledgement")
    }

    fn stop_worker(&mut self) {
        {
            let mut state = self.shared.state.lock().expect("pipeline lock");
            state.stopping = true;
            state.pending.clear();
            self.shared.changed.notify_all();
        }
        if let Some(worker) = self.worker.take() {
            worker.join().expect("persistence worker");
        }
    }
}

impl Drop for EditorSaveRecoveryHarness {
    fn drop(&mut self) {
        self.stop_worker();
    }
}

fn spawn_worker(
    adapter: Arc<EditorIcedAdapter>,
    session: SharedEditorSession,
    shared: Arc<SharedPipeline>,
    boundaries: Arc<BoundaryController>,
    persistence: Arc<EditorPersistenceCoordinator>,
) -> JoinHandle<()> {
    thread::Builder::new()
        .name("parchmint-editor-persistence-test".into())
        .spawn(move || {
            loop {
                let pending = {
                    let mut state = shared.state.lock().expect("pipeline lock");
                    while state.pending.is_empty() && !state.stopping {
                        state = shared.changed.wait(state).expect("pipeline lock");
                    }
                    if state.stopping {
                        return;
                    }
                    let pending = state.pending.pop_front().expect("pending projection");
                    state.active = Some(pending.revision);
                    state.observe_backlog();
                    pending
                };

                boundaries.reach(Boundary::BeforeProjection);
                let pending = {
                    let mut state = shared.state.lock().expect("pipeline lock");
                    state.newest_unpinned(pending)
                };
                let revision = pending.revision;
                let projection = block_on(adapter.project(session.clone(), revision));
                let projection = match projection {
                    Ok(projection) => projection,
                    Err(error) => {
                        persistence.mark_projection_failure(error.clone());
                        finish_with_failure(
                            &shared,
                            revision,
                            PersistenceFailure::Projection(error),
                        );
                        continue;
                    }
                };
                boundaries.reach(Boundary::AfterProjection);
                {
                    shared.state.lock().expect("pipeline lock").projected += 1;
                }

                let recovery = match append_recovery(
                    &shared,
                    boundaries.as_ref(),
                    persistence.as_ref(),
                    &pending.application,
                    &projection,
                ) {
                    Ok(recovery) => recovery,
                    Err(error) => {
                        finish_with_failure(&shared, revision, PersistenceFailure::Recovery(error));
                        continue;
                    }
                };
                if recovery.is_none() {
                    return;
                }

                let save_requested = shared
                    .state
                    .lock()
                    .expect("pipeline lock")
                    .save_targets
                    .remove(&revision);
                if save_requested
                    && let Err(failure) = save_projection(
                        &shared,
                        boundaries.as_ref(),
                        persistence.as_ref(),
                        &pending.application,
                        &projection,
                    )
                {
                    finish_with_failure(&shared, revision, failure);
                    continue;
                }
                finish_work(&shared);
            }
        })
        .expect("start persistence worker")
}

fn append_recovery(
    shared: &SharedPipeline,
    boundaries: &BoundaryController,
    persistence: &EditorPersistenceCoordinator,
    application: &RevisionedSaveRequest,
    projection: &CanonicalProjection,
) -> Result<Option<RecoveryBatch>, RecoveryError> {
    boundaries.reach(Boundary::BeforeRecoveryAppend);
    let durable = persistence
        .persist_projection(
            projection,
            &save_vector(
                application,
                Some(content_hash(projection.body().as_bytes())),
            ),
            VersionedRecoveryPayload::V1(RecoveryRecordV1 {
                schema: "parchmint.recovery-record/v1".into(),
                record_id: format!("editor-revision-{}", projection.revision().value()),
                operations: vec![json!({
                    "kind": "replace-document",
                    "path": DOCUMENT_PATH,
                    "body": projection.body(),
                })],
            }),
        )
        .map_err(|error| match error {
            parchmint_editor_api::EditorPersistenceError::Recovery(error) => error,
            _ => RecoveryError::Storage {
                operation: "coordinate editor projection",
                reason: error.to_string(),
            },
        })?;
    let batch = durable.batch().clone();
    let receipt = durable.receipt().clone();
    shared
        .state
        .lock()
        .expect("pipeline lock")
        .in_flight_recovery = Some(batch.clone());
    shared
        .state
        .lock()
        .expect("pipeline lock")
        .in_flight_receipt = Some(receipt);
    boundaries.reach(Boundary::AfterRecoveryAppend);
    if shared.state.lock().expect("pipeline lock").crash_requested {
        return Ok(None);
    }
    persistence
        .acknowledge_recovery(durable)
        .map_err(|error| RecoveryError::Storage {
            operation: "acknowledge editor recovery frontier",
            reason: error.to_string(),
        })?;
    let mut state = shared.state.lock().expect("pipeline lock");
    state.recovery_project_revision = batch.project_revision;
    state.recovery_document_revision = batch.documents[&DOCUMENT].last;
    state.recovery_hash = batch.result_hashes[&ResourceId::Document];
    state.recovery_batches += 1;
    state.in_flight_recovery = None;
    state.in_flight_receipt = None;
    Ok(Some(batch))
}

fn save_projection(
    shared: &SharedPipeline,
    boundaries: &BoundaryController,
    persistence: &EditorPersistenceCoordinator,
    application: &RevisionedSaveRequest,
    projection: &CanonicalProjection,
) -> Result<(), PersistenceFailure> {
    let hash = content_hash(projection.body().as_bytes());
    if application.open_documents.get(&projection.document_id()) != Some(&projection.revision()) {
        return Err(PersistenceFailure::RevisionMismatch);
    }
    let revisions = save_vector(application, Some(hash));
    let path = parchmint_history_api::CanonicalRelativePath::parse(DOCUMENT_PATH)
        .expect("canonical fixture path");
    let intent_hash = CheckpointIntentHash::from_bytes(
        Sha256::digest(
            [
                projection.body().as_bytes(),
                &projection.revision().value().to_le_bytes(),
            ]
            .concat(),
        )
        .into(),
    );
    let request = SaveRequest::new(
        revisions.clone(),
        AtomicWritePlan::new(vec![StagedResource {
            path: DOCUMENT_PATH.into(),
            bytes: projection.body().as_bytes().to_vec(),
        }]),
        CheckpointInput {
            intent_hash,
            resources: BTreeMap::from([(path, hash)]),
            category: CheckpointCategory::Autosave,
            affected_documents: vec![DOCUMENT],
            name: None,
            recorded_at_unix_millis: Some(1_725_000_000_000),
        },
        SavePriority::Autosave,
    );

    boundaries.reach(Boundary::BeforeSave);
    let ticket = persistence
        .submit_save(projection, request)
        .map_err(|error| match error {
            parchmint_editor_api::EditorPersistenceError::Save(error) => {
                PersistenceFailure::Save(error)
            }
            _ => PersistenceFailure::RevisionMismatch,
        })?;
    let acknowledgement = ticket.wait().map_err(PersistenceFailure::Save)?;
    persistence
        .acknowledge_save(&acknowledgement)
        .map_err(|_| PersistenceFailure::RevisionMismatch)?;
    if acknowledgement.requested_revisions != revisions
        || !acknowledgement
            .written_revisions
            .covers(&acknowledgement.requested_revisions)
    {
        return Err(PersistenceFailure::RevisionMismatch);
    }

    let mut state = shared.state.lock().expect("pipeline lock");
    state.status.active = None;
    state.status.saved_through = Some(acknowledgement.written_revisions.clone());
    state.status.failure = None;
    state.status.state = if state
        .status
        .requested
        .as_ref()
        .is_some_and(|requested| acknowledgement.written_revisions.covers(requested))
    {
        SaveState::Saved
    } else {
        SaveState::Dirty
    };
    state.acknowledgements.push(acknowledgement);
    Ok(())
}

fn finish_with_failure(
    shared: &SharedPipeline,
    revision: EditorRevision,
    failure: PersistenceFailure,
) {
    let mut state = shared.state.lock().expect("pipeline lock");
    state.save_targets.remove(&revision);
    state.fail(failure);
    state.active = None;
    shared.changed.notify_all();
}

fn finish_work(shared: &SharedPipeline) {
    let mut state = shared.state.lock().expect("pipeline lock");
    state.active = None;
    shared.changed.notify_all();
}

fn save_vector(
    request: &RevisionedSaveRequest,
    canonical_hash: Option<ContentHash>,
) -> SaveRevisionVector {
    SaveRevisionVector {
        project_revision: request.project_revision,
        open_documents: request
            .open_documents
            .iter()
            .map(|(document, revision)| (*document, DocumentRevision::from(revision.value())))
            .collect(),
        closed_resources: BTreeMap::new(),
        canonical_hashes: canonical_hash
            .map(|hash| BTreeMap::from([(ResourceId::Document, hash)]))
            .unwrap_or_default(),
        generation: SaveGeneration::from(request.generation),
    }
}

fn recovery_base(initial_body: &str) -> RecoveryBaseSnapshot {
    RecoveryBaseSnapshot {
        revisions: RecoveryRevisionVector::new(ProjectRevision::default(), BTreeMap::new()),
        hashes: BTreeMap::from([(ResourceId::Document, content_hash(initial_body.as_bytes()))]),
    }
}

fn content_hash(bytes: &[u8]) -> ContentHash {
    ContentHash::from_bytes(Sha256::digest(bytes).into())
}

#[allow(dead_code)]
pub fn durable_vector(replay: &RecoveryReplay) -> Option<DurableRevisionVector> {
    replay
        .accepted
        .last()
        .map(|batch| DurableRevisionVector::new(batch.revision_vector()))
}

pub fn recovered_body(replay: &RecoveryReplay) -> Option<&str> {
    replay.accepted.last().and_then(|batch| {
        let VersionedRecoveryPayload::V1(payload) = &batch.payload;
        payload.operations.last().and_then(|operation| {
            operation
                .as_object()
                .and_then(|fields| fields.get("body"))
                .and_then(Value::as_str)
        })
    })
}
