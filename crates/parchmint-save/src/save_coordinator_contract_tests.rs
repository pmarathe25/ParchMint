//! Contract scenarios for the production per-project save coordinator.

use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{Arc, Condvar, Mutex},
    time::Duration,
};

use parchmint_history_api::{
    HistoryIntegrityReport, HistoryPage, HistoryPageQuery, HistoryState, MaintenanceBudget,
    MaintenanceReport, RestorePlan, SnapshotPreview,
};
use parchmint_project_format::CanonicalRelativePath;
use parchmint_project_repository::{
    Abandonment, ProjectRootCapability, Reconciliation, SaveTransactionRecord, StagedResource,
    StagedWrite, ValidationReport,
};

use super::*;

const WAIT: Duration = Duration::from_secs(3);

#[derive(Debug, Default)]
struct PauseState {
    block_first_commit: bool,
    first_commit_entered: bool,
}

#[derive(Debug, Default)]
struct CommitPause {
    state: Mutex<PauseState>,
    changed: Condvar,
}

impl CommitPause {
    fn blocked() -> Self {
        Self {
            state: Mutex::new(PauseState {
                block_first_commit: true,
                first_commit_entered: false,
            }),
            changed: Condvar::new(),
        }
    }

    fn pause_first(&self) {
        let mut state = self.state.lock().expect("pause lock");
        state.first_commit_entered = true;
        self.changed.notify_all();
        while state.block_first_commit {
            state = self.changed.wait(state).expect("pause lock");
        }
    }

    fn wait_until_first_commit(&self) {
        let state = self.state.lock().expect("pause lock");
        let (state, timeout) = self
            .changed
            .wait_timeout_while(state, WAIT, |state| !state.first_commit_entered)
            .expect("pause lock");
        assert!(state.first_commit_entered, "writer did not reach its pause");
        assert!(!timeout.timed_out(), "writer pause timed out");
    }

    fn resume(&self) {
        let mut state = self.state.lock().expect("pause lock");
        state.block_first_commit = false;
        self.changed.notify_all();
    }
}

#[derive(Debug, Default)]
struct WriterState {
    next_generation: u64,
    active_writers: usize,
    max_active_writers: usize,
    committed_payloads: Vec<Vec<u8>>,
}

#[derive(Debug)]
struct RecordingWriter {
    root: ProjectRootCapability,
    state: Mutex<WriterState>,
    pause: Option<Arc<CommitPause>>,
}

impl RecordingWriter {
    fn new(pause: Option<Arc<CommitPause>>) -> Self {
        Self {
            root: ProjectRootCapability::new(1),
            state: Mutex::new(WriterState::default()),
            pause,
        }
    }

    fn committed_generations(&self) -> Vec<u64> {
        self.state
            .lock()
            .expect("writer lock")
            .committed_payloads
            .iter()
            .map(|payload| u64::from(payload[0]))
            .collect()
    }

    fn max_active_writers(&self) -> usize {
        self.state.lock().expect("writer lock").max_active_writers
    }
}

impl AtomicWriter for RecordingWriter {
    fn stage(&self, plan: AtomicWritePlan) -> Result<StagedWrite, WriteError> {
        let mut state = self.state.lock().expect("writer lock");
        state.next_generation += 1;
        Ok(StagedWrite::new(
            self.root.clone(),
            state.next_generation,
            plan,
        ))
    }

    fn validate_staged(&self, _staged: &StagedWrite) -> ValidationReport {
        ValidationReport::new(true)
    }

    fn commit(&self, staged: StagedWrite) -> Result<CommitReceipt, WriteError> {
        let should_pause = {
            let mut state = self.state.lock().expect("writer lock");
            state.active_writers += 1;
            state.max_active_writers = state.max_active_writers.max(state.active_writers);
            staged.generation() == 1 && self.pause.is_some()
        };
        if should_pause {
            self.pause.as_ref().expect("pause exists").pause_first();
        }
        let mut state = self.state.lock().expect("writer lock");
        state.committed_payloads.push(
            staged
                .plan()
                .writes
                .first()
                .expect("test write")
                .bytes
                .clone(),
        );
        state.active_writers -= 1;
        Ok(CommitReceipt::new(staged.generation()))
    }

    fn reconcile(&self, _record: SaveTransactionRecord) -> Result<Reconciliation, WriteError> {
        Ok(Reconciliation::new(true))
    }

    fn abandon(&self, _staged: StagedWrite) -> Result<Abandonment, WriteError> {
        Ok(Abandonment::new(true))
    }
}

#[derive(Debug)]
struct PanickingWriter {
    pause: Arc<CommitPause>,
}

impl PanickingWriter {
    fn new(pause: Arc<CommitPause>) -> Self {
        Self { pause }
    }
}

impl AtomicWriter for PanickingWriter {
    fn stage(&self, _plan: AtomicWritePlan) -> Result<StagedWrite, WriteError> {
        self.pause.pause_first();
        panic!("injected production dependency panic")
    }

    fn validate_staged(&self, _staged: &StagedWrite) -> ValidationReport {
        unreachable!("the injected panic stops the worker during staging")
    }

    fn commit(&self, _staged: StagedWrite) -> Result<CommitReceipt, WriteError> {
        unreachable!("the injected panic stops the worker during staging")
    }

    fn reconcile(&self, _record: SaveTransactionRecord) -> Result<Reconciliation, WriteError> {
        unreachable!("the injected panic stops the worker during staging")
    }

    fn abandon(&self, _staged: StagedWrite) -> Result<Abandonment, WriteError> {
        unreachable!("the injected panic stops the worker during staging")
    }
}

#[derive(Debug, Default)]
struct HistoryStateForTest {
    failures: BTreeSet<CheckpointIntentHash>,
    calls: Vec<CheckpointIntentHash>,
    checkpoints: BTreeMap<CheckpointIntentHash, CheckpointId>,
}

#[derive(Debug, Default)]
struct RecordingHistory {
    state: Mutex<HistoryStateForTest>,
}

impl RecordingHistory {
    fn fail_once(&self, intent: CheckpointIntentHash) {
        self.state
            .lock()
            .expect("history lock")
            .failures
            .insert(intent);
    }

    fn call_count(&self) -> usize {
        self.state.lock().expect("history lock").calls.len()
    }
}

impl HistoryStore for RecordingHistory {
    fn initialize(&self, project: ProjectRootCapability) -> Result<HistoryState, HistoryError> {
        Ok(HistoryState {
            project,
            checkpoint_count: self.state.lock().expect("history lock").checkpoints.len(),
        })
    }

    fn checkpoint(&self, input: CheckpointInput) -> Result<CheckpointId, HistoryError> {
        let mut state = self.state.lock().expect("history lock");
        state.calls.push(input.intent_hash);
        if state.failures.remove(&input.intent_hash) {
            return Err(HistoryError::Storage {
                operation: "checkpoint",
                reason: "injected failure after file commit".into(),
            });
        }
        let next = u8::try_from(state.checkpoints.len() + 1).expect("small test history");
        Ok(*state
            .checkpoints
            .entry(input.intent_hash)
            .or_insert_with(|| CheckpointId::from_bytes([next; 16])))
    }

    fn list(&self, _query: HistoryPageQuery) -> Result<HistoryPage, HistoryError> {
        Ok(HistoryPage {
            checkpoints: Vec::new(),
            next_cursor: None,
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
            checked_checkpoints: self.state.lock().expect("history lock").checkpoints.len(),
        })
    }

    fn maintain(&self, _budget: MaintenanceBudget) -> Result<MaintenanceReport, HistoryError> {
        Ok(MaintenanceReport {
            checked_objects: 0,
            retained_checkpoints: self.state.lock().expect("history lock").checkpoints.len(),
        })
    }
}

struct Harness {
    coordinator: ProjectSaveCoordinator,
    writer: Arc<RecordingWriter>,
    history: Arc<RecordingHistory>,
    intents: Arc<InMemoryCheckpointIntentStore>,
}

impl Harness {
    fn new(pause: Option<Arc<CommitPause>>) -> Self {
        let writer = Arc::new(RecordingWriter::new(pause));
        let history = Arc::new(RecordingHistory::default());
        let intents = Arc::new(InMemoryCheckpointIntentStore::new());
        let coordinator = ProjectSaveCoordinator::new(
            project_id(1),
            writer.clone(),
            history.clone(),
            intents.clone(),
        )
        .expect("start coordinator");
        Self {
            coordinator,
            writer,
            history,
            intents,
        }
    }
}

fn project_id(value: u8) -> ProjectId {
    ProjectId::from_bytes([value; 16])
}

fn document_id(value: u8) -> DocumentId {
    DocumentId::from_bytes([value; 16])
}

fn hash(value: u8) -> ContentHash {
    ContentHash::from_bytes([value; 32])
}

fn intent_hash(value: u8) -> CheckpointIntentHash {
    CheckpointIntentHash::from_bytes([value; 32])
}

fn revisions(generation: u64, documents: &[(u8, u64)]) -> SaveRevisionVector {
    SaveRevisionVector {
        project_revision: ProjectRevision::from(generation),
        open_documents: documents
            .iter()
            .map(|(document, revision)| (document_id(*document), EditorRevision::from(*revision)))
            .collect(),
        closed_resources: BTreeMap::new(),
        canonical_hashes: BTreeMap::from([(ResourceId::Document, hash(generation as u8))]),
        generation: SaveGeneration::from(generation),
    }
}

fn request(
    intent: u8,
    generation: u64,
    documents: &[(u8, u64)],
    priority: SavePriority,
) -> SaveRequest {
    let content_hash = hash(generation as u8);
    SaveRequest::new(
        revisions(generation, documents),
        AtomicWritePlan::new(vec![StagedResource {
            path: "documents/document.html".into(),
            bytes: vec![generation as u8],
        }]),
        CheckpointInput {
            intent_hash: intent_hash(intent),
            resources: BTreeMap::from([(
                CanonicalRelativePath::parse("documents/document.html").expect("canonical path"),
                content_hash,
            )]),
            category: match priority {
                SavePriority::Autosave | SavePriority::Close => CheckpointCategory::Autosave,
                SavePriority::Structural => CheckpointCategory::StructuralChange,
                SavePriority::Explicit => CheckpointCategory::ExplicitSave,
            },
            affected_documents: documents
                .iter()
                .map(|(document, _)| document_id(*document))
                .collect(),
            name: None,
        },
        priority,
    )
}

#[test]
fn each_project_coordinator_has_one_serial_writer_and_queue() {
    let pause = Arc::new(CommitPause::blocked());
    let harness = Harness::new(Some(pause.clone()));
    let first = harness
        .coordinator
        .request(request(1, 1, &[(1, 1)], SavePriority::Autosave))
        .unwrap();
    pause.wait_until_first_commit();
    let second = harness
        .coordinator
        .request(request(2, 2, &[(1, 2)], SavePriority::Autosave))
        .unwrap();
    let third = harness
        .coordinator
        .request(request(3, 3, &[(1, 3)], SavePriority::Autosave))
        .unwrap();

    pause.resume();

    assert_eq!(
        first
            .wait_timeout(WAIT)
            .unwrap()
            .written_revisions
            .generation,
        SaveGeneration::from(1)
    );
    assert_eq!(
        second
            .wait_timeout(WAIT)
            .unwrap()
            .written_revisions
            .generation,
        SaveGeneration::from(3)
    );
    assert_eq!(
        third
            .wait_timeout(WAIT)
            .unwrap()
            .written_revisions
            .generation,
        SaveGeneration::from(3)
    );
    assert_eq!(harness.writer.max_active_writers(), 1);
    assert_eq!(harness.writer.committed_generations(), vec![1, 3]);
}

#[test]
fn real_worker_coalesces_distinct_requests_while_execution_is_paused() {
    let pause = Arc::new(WorkerPause::blocked());
    let writer = Arc::new(RecordingWriter::new(None));
    let history = Arc::new(RecordingHistory::default());
    let intents = Arc::new(InMemoryCheckpointIntentStore::new());
    let coordinator = ProjectSaveCoordinator::new_with_worker_pause(
        project_id(9),
        writer,
        history,
        intents,
        pause.clone(),
    )
    .expect("start paused save worker");

    let mut tickets = Vec::new();
    for generation in 1..=8 {
        tickets.push(
            coordinator
                .request(request(
                    generation as u8,
                    generation,
                    &[(1, generation)],
                    SavePriority::Autosave,
                ))
                .expect("queue save request"),
        );
    }

    assert_eq!(pause.wait_until_entered(), 1);
    pause.wait_until_generation(SaveGeneration::from(8));
    let status = coordinator.status();
    assert_eq!(status.queued_requests, 1);
    assert_eq!(status.max_queued_requests, 1);
    pause.resume();

    for ticket in tickets {
        assert_eq!(
            ticket
                .wait_timeout(WAIT)
                .expect("save completion")
                .written_revisions,
            revisions(8, &[(1, 8)])
        );
    }
    let status = coordinator.status();
    assert_eq!(status.queued_requests, 0);
    assert_eq!(status.max_queued_requests, 1);
}

#[test]
fn active_save_keeps_its_revision_vector_when_later_edits_arrive() {
    let pause = Arc::new(CommitPause::blocked());
    let harness = Harness::new(Some(pause.clone()));
    let captured = revisions(7, &[(1, 3)]);
    let first = harness
        .coordinator
        .request(request(7, 7, &[(1, 3)], SavePriority::Autosave))
        .unwrap();
    pause.wait_until_first_commit();
    let planned = harness.intents.pending().unwrap();
    assert_eq!(planned.len(), 1);
    assert_eq!(planned[0].revisions, captured);
    assert_eq!(planned[0].state, CheckpointIntentState::Planned);
    let later = harness
        .coordinator
        .request(request(8, 8, &[(1, 4)], SavePriority::Autosave))
        .unwrap();
    pause.resume();

    assert_eq!(
        first.wait_timeout(WAIT).unwrap().written_revisions,
        captured
    );
    assert_eq!(
        later.wait_timeout(WAIT).unwrap().written_revisions,
        revisions(8, &[(1, 4)])
    );
    assert_eq!(harness.writer.committed_generations(), vec![7, 8]);
}

#[test]
fn close_save_overtakes_lower_priority_pending_work_without_blocking_request() {
    let pause = Arc::new(CommitPause::blocked());
    let harness = Harness::new(Some(pause.clone()));
    let active = harness
        .coordinator
        .request(request(1, 1, &[(1, 1)], SavePriority::Autosave))
        .unwrap();
    pause.wait_until_first_commit();
    let later_explicit = harness
        .coordinator
        .request(request(4, 4, &[(1, 4)], SavePriority::Explicit))
        .unwrap();
    let close = harness
        .coordinator
        .request(request(3, 3, &[(1, 3)], SavePriority::Close))
        .unwrap();
    assert!(close.try_result().is_none());
    pause.resume();

    active.wait_timeout(WAIT).unwrap();
    close.wait_timeout(WAIT).unwrap();
    later_explicit.wait_timeout(WAIT).unwrap();
    assert_eq!(harness.writer.committed_generations(), vec![1, 3, 4]);
}

#[test]
fn worker_unwind_completes_every_accepted_ticket_instead_of_stranding_waiters() {
    let pause = Arc::new(CommitPause::blocked());
    let coordinator = ProjectSaveCoordinator::new(
        project_id(11),
        Arc::new(PanickingWriter::new(pause.clone())),
        Arc::new(RecordingHistory::default()),
        Arc::new(InMemoryCheckpointIntentStore::new()),
    )
    .expect("start coordinator");
    let active = coordinator
        .request(request(11, 11, &[(1, 11)], SavePriority::Close))
        .expect("the worker accepts the close save before its dependency unwinds");
    pause.wait_until_first_commit();
    let queued = coordinator
        .request(request(12, 12, &[(1, 12)], SavePriority::Close))
        .expect("the worker accepts a second close save while the first is active");
    let (reported, received) = std::sync::mpsc::sync_channel(1);
    std::thread::spawn(move || {
        let _ = reported.send([active.wait(), queued.wait()]);
    });
    pause.resume();

    assert_eq!(
        received.recv_timeout(WAIT),
        Ok([Err(SaveError::WorkerStopped), Err(SaveError::WorkerStopped)]),
        "worker loss must be observable through the accepted ticket"
    );
    assert_eq!(coordinator.status().state, SaveState::Error);
    assert_eq!(coordinator.status().error, Some(SaveError::WorkerStopped));
    assert!(matches!(
        coordinator.request(request(13, 13, &[(1, 13)], SavePriority::Close)),
        Err(SaveError::WorkerStopped)
    ));
}

#[test]
fn paused_requests_coalesce_only_into_a_covering_immutable_snapshot() {
    let pause = Arc::new(CommitPause::blocked());
    let harness = Harness::new(Some(pause.clone()));
    let active = harness
        .coordinator
        .request(request(1, 1, &[(1, 1), (2, 1)], SavePriority::Autosave))
        .unwrap();
    pause.wait_until_first_commit();
    let superseded = harness
        .coordinator
        .request(request(2, 2, &[(1, 2), (2, 1)], SavePriority::Autosave))
        .unwrap();
    let covering = harness
        .coordinator
        .request(request(3, 3, &[(1, 2), (2, 3)], SavePriority::Close))
        .unwrap();
    pause.resume();

    assert_eq!(
        active.wait_timeout(WAIT).unwrap().written_revisions,
        revisions(1, &[(1, 1), (2, 1)])
    );
    let expected = revisions(3, &[(1, 2), (2, 3)]);
    assert_eq!(
        superseded.wait_timeout(WAIT).unwrap().written_revisions,
        expected
    );
    assert_eq!(
        covering.wait_timeout(WAIT).unwrap().written_revisions,
        revisions(3, &[(1, 2), (2, 3)])
    );
    assert_eq!(harness.writer.committed_generations(), vec![1, 3]);
}

#[test]
fn history_failure_after_file_commit_retries_without_a_false_saved_ack() {
    let harness = Harness::new(None);
    let save = request(5, 5, &[(1, 5)], SavePriority::Autosave);
    harness.history.fail_once(save.checkpoint.intent_hash);
    let first = harness.coordinator.request(save.clone()).unwrap();

    assert!(matches!(
        first.wait_timeout(WAIT),
        Err(SaveError::History(_))
    ));
    assert_eq!(harness.coordinator.status().state, SaveState::Error);
    let pending = harness.intents.pending().unwrap();
    assert_eq!(pending.len(), 1);
    assert!(matches!(
        pending[0].state,
        CheckpointIntentState::FilesCommitted { .. }
    ));

    let retry = harness.coordinator.request(save).unwrap();
    let acknowledgement = retry.wait_timeout(WAIT).unwrap();
    assert_eq!(acknowledgement.written_revisions, revisions(5, &[(1, 5)]));
    assert_eq!(harness.writer.committed_generations(), vec![5]);
    assert_eq!(harness.history.call_count(), 2);
    assert!(harness.intents.pending().unwrap().is_empty());
    assert_eq!(harness.coordinator.status().state, SaveState::Saved);
}

#[test]
fn open_reconciliation_keeps_history_failures_pending_then_finishes_them() {
    let harness = Harness::new(None);
    let completed = request(9, 9, &[(1, 9)], SavePriority::Explicit);
    let retryable = request(10, 10, &[(1, 10)], SavePriority::Autosave);
    let mut completed_intent = CheckpointIntent::planned(project_id(1), &completed);
    completed_intent.state = CheckpointIntentState::FilesCommitted {
        receipt: CommitReceipt::new(99),
    };
    let mut retryable_intent = CheckpointIntent::planned(project_id(1), &retryable);
    retryable_intent.state = CheckpointIntentState::FilesCommitted {
        receipt: CommitReceipt::new(100),
    };
    harness.intents.persist(completed_intent).unwrap();
    harness.intents.persist(retryable_intent).unwrap();
    harness.history.fail_once(retryable.checkpoint.intent_hash);

    assert!(matches!(
        harness.coordinator.reconcile_open(),
        Err(SaveError::History(_))
    ));
    assert_eq!(harness.coordinator.status().state, SaveState::Error);
    assert_eq!(
        harness.intents.pending().unwrap(),
        vec![CheckpointIntent {
            project: project_id(1),
            revisions: retryable.revisions.clone(),
            writes: retryable.writes.clone(),
            checkpoint: retryable.checkpoint.clone(),
            priority: retryable.priority,
            state: CheckpointIntentState::FilesCommitted {
                receipt: CommitReceipt::new(100),
            },
        }]
    );
    assert!(harness.writer.committed_generations().is_empty());
    assert_eq!(harness.history.call_count(), 2);

    let reconciliation = harness.coordinator.reconcile_open().unwrap();
    assert_eq!(reconciliation.completed_checkpoints, 1);
    assert_eq!(reconciliation.awaiting_file_reconciliation, 0);
    assert_eq!(reconciliation.saved_through, Some(retryable.revisions));
    assert!(harness.intents.pending().unwrap().is_empty());
    assert_eq!(harness.history.call_count(), 3);
    assert_eq!(harness.coordinator.status().state, SaveState::Saved);
}
