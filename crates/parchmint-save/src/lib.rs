//! Revisioned, asynchronous project saves with matching History checkpoints.

use std::{
    collections::{BTreeMap, VecDeque},
    error::Error,
    fmt,
    sync::{
        Arc, Condvar, Mutex,
        atomic::{AtomicU64, Ordering},
        mpsc,
    },
    thread::{self, JoinHandle},
    time::Duration,
};

use parchmint_domain::CheckpointId;
pub use parchmint_domain::{DocumentId, ProjectId, ProjectRevision};
pub use parchmint_history_api::{
    CheckpointCategory, CheckpointInput, CheckpointIntentHash, HistoryError, HistoryStore,
};
pub use parchmint_project_format::{ContentHash, ResourceId};
pub use parchmint_project_repository::{AtomicWritePlan, AtomicWriter, CommitReceipt, WriteError};
pub use parchmint_recovery_api::DocumentRevision as EditorRevision;

/// A monotonic revision for a closed canonical resource.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ResourceRevision(u64);

impl ResourceRevision {
    pub const fn value(self) -> u64 {
        self.0
    }
}

impl From<u64> for ResourceRevision {
    fn from(value: u64) -> Self {
        Self(value)
    }
}

/// A monotonic identity for one captured project snapshot.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SaveGeneration(u64);

impl SaveGeneration {
    pub const fn value(self) -> u64 {
        self.0
    }
}

impl From<u64> for SaveGeneration {
    fn from(value: u64) -> Self {
        Self(value)
    }
}

/// The immutable revision frontier captured for one save request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SaveRevisionVector {
    pub project_revision: ProjectRevision,
    pub open_documents: BTreeMap<DocumentId, EditorRevision>,
    pub closed_resources: BTreeMap<ResourceId, ResourceRevision>,
    pub canonical_hashes: BTreeMap<ResourceId, ContentHash>,
    pub generation: SaveGeneration,
}

impl SaveRevisionVector {
    /// Returns true when this complete snapshot includes every requested revision.
    pub fn covers(&self, requested: &Self) -> bool {
        self.project_revision >= requested.project_revision
            && self.generation >= requested.generation
            && requested
                .open_documents
                .iter()
                .all(|(document, revision)| self.open_documents.get(document) >= Some(revision))
            && requested
                .closed_resources
                .iter()
                .all(|(resource, revision)| self.closed_resources.get(resource) >= Some(revision))
    }
}

/// Queue priority. An active canonical write is never preempted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum SavePriority {
    Autosave,
    Structural,
    Explicit,
    Close,
}

/// One fixed snapshot and its already-encoded canonical writes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SaveRequest {
    pub revisions: SaveRevisionVector,
    pub writes: AtomicWritePlan,
    pub checkpoint: CheckpointInput,
    pub priority: SavePriority,
}

impl SaveRequest {
    pub fn new(
        revisions: SaveRevisionVector,
        writes: AtomicWritePlan,
        checkpoint: CheckpointInput,
        priority: SavePriority,
    ) -> Self {
        Self {
            revisions,
            writes,
            checkpoint,
            priority,
        }
    }
}

/// How far a durable checkpoint intent has progressed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CheckpointIntentState {
    Planned,
    FilesCommitted { receipt: CommitReceipt },
}

/// Durable record for completing a save after a crash or History failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckpointIntent {
    pub project: ProjectId,
    pub revisions: SaveRevisionVector,
    pub writes: AtomicWritePlan,
    pub checkpoint: CheckpointInput,
    pub priority: SavePriority,
    pub state: CheckpointIntentState,
}

impl CheckpointIntent {
    fn planned(project: ProjectId, request: &SaveRequest) -> Self {
        Self {
            project,
            revisions: request.revisions.clone(),
            writes: request.writes.clone(),
            checkpoint: request.checkpoint.clone(),
            priority: request.priority,
            state: CheckpointIntentState::Planned,
        }
    }

    pub const fn intent_hash(&self) -> CheckpointIntentHash {
        self.checkpoint.intent_hash
    }
}

/// Proof that one persisted intent has a matching History checkpoint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckpointReceipt {
    pub project: ProjectId,
    pub intent_hash: CheckpointIntentHash,
    pub checkpoint: CheckpointId,
    pub revisions: SaveRevisionVector,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IntentStoreError {
    Conflict {
        intent_hash: CheckpointIntentHash,
    },
    Storage {
        operation: &'static str,
        reason: String,
    },
}

impl fmt::Display for IntentStoreError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Conflict { .. } => {
                formatter.write_str("checkpoint intent conflicts with its durable record")
            }
            Self::Storage { operation, reason } => {
                write!(formatter, "checkpoint intent {operation} failed: {reason}")
            }
        }
    }
}

impl Error for IntentStoreError {}

/// Durable pending-checkpoint storage, implemented by the recovery layer.
pub trait CheckpointIntentStore: Send + Sync {
    fn persist(&self, intent: CheckpointIntent) -> Result<(), IntentStoreError>;
    fn pending(&self) -> Result<Vec<CheckpointIntent>, IntentStoreError>;
    fn complete(&self, receipt: CheckpointReceipt) -> Result<(), IntentStoreError>;
}

/// A deterministic in-memory implementation useful to embedders and tests.
#[derive(Debug, Default)]
pub struct InMemoryCheckpointIntentStore {
    state: Mutex<IntentStoreState>,
}

#[derive(Debug, Default)]
struct IntentStoreState {
    pending: BTreeMap<CheckpointIntentHash, CheckpointIntent>,
    completed: BTreeMap<CheckpointIntentHash, CheckpointReceipt>,
}

impl InMemoryCheckpointIntentStore {
    pub fn new() -> Self {
        Self::default()
    }
}

impl CheckpointIntentStore for InMemoryCheckpointIntentStore {
    fn persist(&self, intent: CheckpointIntent) -> Result<(), IntentStoreError> {
        let hash = intent.intent_hash();
        let mut state = self.state.lock().expect("intent store lock");
        if let Some(completed) = state.completed.get(&hash) {
            if completed.project == intent.project && completed.revisions == intent.revisions {
                return Ok(());
            }
            return Err(IntentStoreError::Conflict { intent_hash: hash });
        }
        if let Some(existing) = state.pending.get(&hash) {
            if existing.project != intent.project
                || existing.revisions != intent.revisions
                || existing.writes != intent.writes
                || existing.checkpoint != intent.checkpoint
            {
                return Err(IntentStoreError::Conflict { intent_hash: hash });
            }
            match (&existing.state, &intent.state) {
                (
                    CheckpointIntentState::FilesCommitted { receipt: existing },
                    CheckpointIntentState::FilesCommitted { receipt: incoming },
                ) if existing != incoming => {
                    return Err(IntentStoreError::Conflict { intent_hash: hash });
                }
                (CheckpointIntentState::FilesCommitted { .. }, CheckpointIntentState::Planned) => {
                    return Ok(());
                }
                _ => {}
            }
        }
        state.pending.insert(hash, intent);
        Ok(())
    }

    fn pending(&self) -> Result<Vec<CheckpointIntent>, IntentStoreError> {
        Ok(self
            .state
            .lock()
            .expect("intent store lock")
            .pending
            .values()
            .cloned()
            .collect())
    }

    fn complete(&self, receipt: CheckpointReceipt) -> Result<(), IntentStoreError> {
        let mut state = self.state.lock().expect("intent store lock");
        if let Some(completed) = state.completed.get(&receipt.intent_hash) {
            return if completed == &receipt {
                Ok(())
            } else {
                Err(IntentStoreError::Conflict {
                    intent_hash: receipt.intent_hash,
                })
            };
        }
        let matches = state
            .pending
            .get(&receipt.intent_hash)
            .is_some_and(|intent| {
                intent.project == receipt.project
                    && intent.revisions == receipt.revisions
                    && matches!(intent.state, CheckpointIntentState::FilesCommitted { .. })
            });
        if !matches {
            return Err(IntentStoreError::Conflict {
                intent_hash: receipt.intent_hash,
            });
        }
        state.pending.remove(&receipt.intent_hash);
        state.completed.insert(receipt.intent_hash, receipt);
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SaveError {
    IntentStore(IntentStoreError),
    Write(WriteError),
    History(HistoryError),
    InvalidStagedWrite,
    Cancelled,
    WorkerStopped,
    TicketTimedOut,
}

impl fmt::Display for SaveError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::IntentStore(error) => write!(formatter, "{error}"),
            Self::Write(error) => write!(formatter, "project write failed: {error}"),
            Self::History(error) => write!(formatter, "{error}"),
            Self::InvalidStagedWrite => {
                formatter.write_str("staged project write did not validate")
            }
            Self::Cancelled => formatter.write_str("save request was cancelled"),
            Self::WorkerStopped => formatter.write_str("project save worker stopped"),
            Self::TicketTimedOut => formatter.write_str("timed out waiting for save completion"),
        }
    }
}

impl Error for SaveError {}

impl From<IntentStoreError> for SaveError {
    fn from(error: IntentStoreError) -> Self {
        Self::IntentStore(error)
    }
}

impl From<WriteError> for SaveError {
    fn from(error: WriteError) -> Self {
        Self::Write(error)
    }
}

impl From<HistoryError> for SaveError {
    fn from(error: HistoryError) -> Self {
        Self::History(error)
    }
}

/// The result delivered only after canonical files and History both complete.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SavedAcknowledgement {
    pub requested_revisions: SaveRevisionVector,
    pub written_revisions: SaveRevisionVector,
    pub checkpoint: CheckpointId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SaveTicketId(u64);

impl SaveTicketId {
    pub const fn value(self) -> u64 {
        self.0
    }
}

type TicketResult = Result<SavedAcknowledgement, SaveError>;

#[derive(Debug, Default)]
struct TicketState {
    result: Mutex<Option<TicketResult>>,
    ready: Condvar,
}

/// An asynchronous result handle for one requested revision vector.
#[derive(Debug, Clone)]
pub struct SaveTicket {
    id: SaveTicketId,
    state: Arc<TicketState>,
}

impl SaveTicket {
    pub const fn id(&self) -> SaveTicketId {
        self.id
    }

    pub fn try_result(&self) -> Option<TicketResult> {
        self.state.result.lock().expect("save ticket lock").clone()
    }

    pub fn wait(&self) -> TicketResult {
        let mut result = self.state.result.lock().expect("save ticket lock");
        while result.is_none() {
            result = self.state.ready.wait(result).expect("save ticket lock");
        }
        result.clone().expect("ticket result is ready")
    }

    pub fn wait_timeout(&self, timeout: Duration) -> TicketResult {
        let result = self.state.result.lock().expect("save ticket lock");
        let (result, _) = self
            .state
            .ready
            .wait_timeout_while(result, timeout, |result| result.is_none())
            .expect("save ticket lock");
        result.clone().unwrap_or(Err(SaveError::TicketTimedOut))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SaveState {
    Clean,
    Dirty,
    Saving,
    Saved,
    Error,
}

/// A synchronized snapshot for application save status.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SaveStatusSnapshot {
    pub state: SaveState,
    pub requested: Option<SaveRevisionVector>,
    pub active: Option<SaveRevisionVector>,
    pub saved_through: Option<SaveRevisionVector>,
    pub error: Option<SaveError>,
}

impl Default for SaveStatusSnapshot {
    fn default() -> Self {
        Self {
            state: SaveState::Clean,
            requested: None,
            active: None,
            saved_through: None,
            error: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CancelOutcome {
    Cancelled,
    TooLate,
    WorkerStopped,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct OpenReconciliation {
    pub completed_checkpoints: usize,
    pub awaiting_file_reconciliation: usize,
    pub saved_through: Option<SaveRevisionVector>,
}

pub trait SaveCoordinator: Send + Sync {
    fn request(&self, request: SaveRequest) -> Result<SaveTicket, SaveError>;
    fn status(&self) -> SaveStatusSnapshot;
    fn reconcile_open(&self) -> Result<OpenReconciliation, SaveError>;
    fn cancel_pending(&self, ticket: SaveTicket) -> CancelOutcome;
}

/// A per-project serial save worker and priority queue.
pub struct ProjectSaveCoordinator {
    sender: mpsc::Sender<Command>,
    status: Arc<Mutex<SaveStatusSnapshot>>,
    next_ticket: AtomicU64,
    worker: Option<JoinHandle<()>>,
}

impl fmt::Debug for ProjectSaveCoordinator {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ProjectSaveCoordinator")
            .field("status", &self.status())
            .finish_non_exhaustive()
    }
}

impl ProjectSaveCoordinator {
    pub fn new(
        project: ProjectId,
        writer: Arc<dyn AtomicWriter>,
        history: Arc<dyn HistoryStore>,
        intents: Arc<dyn CheckpointIntentStore>,
    ) -> Result<Self, SaveError> {
        let (sender, receiver) = mpsc::channel();
        let status = Arc::new(Mutex::new(SaveStatusSnapshot::default()));
        let worker_status = Arc::clone(&status);
        let worker = thread::Builder::new()
            .name(format!("parchmint-save-{:02x}", project.as_bytes()[15]))
            .spawn(move || {
                run_worker(project, writer, history, intents, receiver, worker_status);
            })
            .map_err(|_| SaveError::WorkerStopped)?;
        Ok(Self {
            sender,
            status,
            next_ticket: AtomicU64::new(1),
            worker: Some(worker),
        })
    }
}

impl SaveCoordinator for ProjectSaveCoordinator {
    fn request(&self, request: SaveRequest) -> Result<SaveTicket, SaveError> {
        let id = SaveTicketId(self.next_ticket.fetch_add(1, Ordering::Relaxed));
        let state = Arc::new(TicketState::default());
        let requested = request.revisions.clone();
        let ticket = SaveTicket {
            id,
            state: Arc::clone(&state),
        };
        {
            let mut status = self.status.lock().expect("save status lock");
            status.requested = Some(request.revisions.clone());
            if status.state != SaveState::Saving {
                status.state = SaveState::Dirty;
            }
            status.error = None;
        }
        self.sender
            .send(Command::Request(Box::new(WorkItem {
                request,
                tickets: vec![PendingTicket {
                    id,
                    state,
                    requested,
                }],
            })))
            .map_err(|_| SaveError::WorkerStopped)?;
        Ok(ticket)
    }

    fn status(&self) -> SaveStatusSnapshot {
        self.status.lock().expect("save status lock").clone()
    }

    fn reconcile_open(&self) -> Result<OpenReconciliation, SaveError> {
        let (reply, receive) = mpsc::sync_channel(1);
        self.sender
            .send(Command::Reconcile { reply })
            .map_err(|_| SaveError::WorkerStopped)?;
        receive.recv().map_err(|_| SaveError::WorkerStopped)?
    }

    fn cancel_pending(&self, ticket: SaveTicket) -> CancelOutcome {
        let (reply, receive) = mpsc::sync_channel(1);
        if self
            .sender
            .send(Command::Cancel {
                ticket: ticket.id,
                reply,
            })
            .is_err()
        {
            return CancelOutcome::WorkerStopped;
        }
        receive.recv().unwrap_or(CancelOutcome::WorkerStopped)
    }
}

impl Drop for ProjectSaveCoordinator {
    fn drop(&mut self) {
        let _ = self.sender.send(Command::Shutdown);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

struct PendingTicket {
    id: SaveTicketId,
    state: Arc<TicketState>,
    requested: SaveRevisionVector,
}

struct WorkItem {
    request: SaveRequest,
    tickets: Vec<PendingTicket>,
}

enum Command {
    Request(Box<WorkItem>),
    Cancel {
        ticket: SaveTicketId,
        reply: mpsc::SyncSender<CancelOutcome>,
    },
    Reconcile {
        reply: mpsc::SyncSender<Result<OpenReconciliation, SaveError>>,
    },
    Shutdown,
}

struct WorkerDependencies {
    project: ProjectId,
    writer: Arc<dyn AtomicWriter>,
    history: Arc<dyn HistoryStore>,
    intents: Arc<dyn CheckpointIntentStore>,
    status: Arc<Mutex<SaveStatusSnapshot>>,
}

fn run_worker(
    project: ProjectId,
    writer: Arc<dyn AtomicWriter>,
    history: Arc<dyn HistoryStore>,
    intents: Arc<dyn CheckpointIntentStore>,
    receiver: mpsc::Receiver<Command>,
    status: Arc<Mutex<SaveStatusSnapshot>>,
) {
    let dependencies = WorkerDependencies {
        project,
        writer,
        history,
        intents,
        status,
    };
    let mut queue = VecDeque::new();
    loop {
        let command = if queue.is_empty() {
            match receiver.recv() {
                Ok(command) => Some(command),
                Err(_) => break,
            }
        } else {
            None
        };
        if command.is_some_and(|command| handle_command(command, &mut queue, &dependencies)) {
            stop_pending(&mut queue);
            break;
        }
        while let Ok(command) = receiver.try_recv() {
            if handle_command(command, &mut queue, &dependencies) {
                stop_pending(&mut queue);
                return;
            }
        }
        coalesce_queue(&mut queue);
        let Some(index) = highest_priority_index(&queue) else {
            continue;
        };
        let mut work = queue.remove(index).expect("selected queue item exists");
        mark_saving(&dependencies.status, &work.request.revisions);
        let result = execute_save(&dependencies, &work.request);
        match result {
            Ok(completed) => {
                for ticket in work.tickets.drain(..) {
                    complete_ticket(
                        &ticket.state,
                        Ok(SavedAcknowledgement {
                            requested_revisions: ticket.requested,
                            written_revisions: completed.revisions.clone(),
                            checkpoint: completed.checkpoint,
                        }),
                    );
                }
                mark_saved(&dependencies.status, completed.revisions);
            }
            Err(error) => {
                for ticket in work.tickets.drain(..) {
                    complete_ticket(&ticket.state, Err(error.clone()));
                }
                mark_error(&dependencies.status, error);
            }
        }
    }
}

fn handle_command(
    command: Command,
    queue: &mut VecDeque<WorkItem>,
    dependencies: &WorkerDependencies,
) -> bool {
    match command {
        Command::Request(work) => queue.push_back(*work),
        Command::Cancel { ticket, reply } => {
            let outcome = cancel_queued(queue, ticket);
            let _ = reply.send(outcome);
        }
        Command::Reconcile { reply } => {
            let result = reconcile_pending(dependencies);
            if let Err(error) = &result {
                mark_error(&dependencies.status, error.clone());
            }
            let _ = reply.send(result);
        }
        Command::Shutdown => return true,
    }
    false
}

fn highest_priority_index(queue: &VecDeque<WorkItem>) -> Option<usize> {
    let mut selected = None;
    for (index, work) in queue.iter().enumerate() {
        if selected.is_none_or(|current| work.request.priority > queue[current].request.priority) {
            selected = Some(index);
        }
    }
    selected
}

fn coalesce_queue(queue: &mut VecDeque<WorkItem>) {
    loop {
        let mut pair = None;
        for later in (0..queue.len()).rev() {
            for earlier in 0..later {
                let newer = &queue[later].request;
                let older = &queue[earlier].request;
                if newer.priority >= older.priority && newer.revisions.covers(&older.revisions) {
                    pair = Some((earlier, later));
                    break;
                }
            }
            if pair.is_some() {
                break;
            }
        }
        let Some((earlier, later)) = pair else {
            return;
        };
        let mut superseded = queue.remove(earlier).expect("coalesced item exists");
        let adjusted_later = later - 1;
        queue[adjusted_later]
            .tickets
            .append(&mut superseded.tickets);
    }
}

fn cancel_queued(queue: &mut VecDeque<WorkItem>, ticket: SaveTicketId) -> CancelOutcome {
    for index in 0..queue.len() {
        if let Some(ticket_index) = queue[index]
            .tickets
            .iter()
            .position(|pending| pending.id == ticket)
        {
            let pending = queue[index].tickets.remove(ticket_index);
            complete_ticket(&pending.state, Err(SaveError::Cancelled));
            if queue[index].tickets.is_empty() {
                queue.remove(index);
            }
            return CancelOutcome::Cancelled;
        }
    }
    CancelOutcome::TooLate
}

fn stop_pending(queue: &mut VecDeque<WorkItem>) {
    for work in queue {
        for ticket in &work.tickets {
            complete_ticket(&ticket.state, Err(SaveError::WorkerStopped));
        }
    }
}

fn complete_ticket(state: &TicketState, result: TicketResult) {
    let mut current = state.result.lock().expect("save ticket lock");
    if current.is_none() {
        *current = Some(result);
        state.ready.notify_all();
    }
}

struct CompletedSave {
    revisions: SaveRevisionVector,
    checkpoint: CheckpointId,
}

fn execute_save(
    dependencies: &WorkerDependencies,
    request: &SaveRequest,
) -> Result<CompletedSave, SaveError> {
    request.checkpoint.validate()?;
    let intent_hash = request.checkpoint.intent_hash;
    let pending = dependencies
        .intents
        .pending()?
        .into_iter()
        .find(|intent| intent.intent_hash() == intent_hash);
    let mut intent = if let Some(intent) = pending {
        if intent.project != dependencies.project
            || intent.revisions != request.revisions
            || intent.writes != request.writes
            || intent.checkpoint != request.checkpoint
        {
            return Err(IntentStoreError::Conflict { intent_hash }.into());
        }
        intent
    } else {
        let intent = CheckpointIntent::planned(dependencies.project, request);
        dependencies.intents.persist(intent.clone())?;
        intent
    };

    if matches!(intent.state, CheckpointIntentState::Planned) {
        let staged = dependencies.writer.stage(request.writes.clone())?;
        if !dependencies.writer.validate_staged(&staged).is_valid() {
            let _ = dependencies.writer.abandon(staged);
            return Err(SaveError::InvalidStagedWrite);
        }
        let receipt = dependencies.writer.commit(staged)?;
        intent.state = CheckpointIntentState::FilesCommitted { receipt };
        dependencies.intents.persist(intent.clone())?;
    }
    finish_checkpoint(dependencies, &intent)
}

fn finish_checkpoint(
    dependencies: &WorkerDependencies,
    intent: &CheckpointIntent,
) -> Result<CompletedSave, SaveError> {
    if !matches!(intent.state, CheckpointIntentState::FilesCommitted { .. }) {
        return Err(IntentStoreError::Conflict {
            intent_hash: intent.intent_hash(),
        }
        .into());
    }
    let checkpoint = dependencies.history.checkpoint(intent.checkpoint.clone())?;
    dependencies.intents.complete(CheckpointReceipt {
        project: dependencies.project,
        intent_hash: intent.intent_hash(),
        checkpoint,
        revisions: intent.revisions.clone(),
    })?;
    Ok(CompletedSave {
        revisions: intent.revisions.clone(),
        checkpoint,
    })
}

fn reconcile_pending(dependencies: &WorkerDependencies) -> Result<OpenReconciliation, SaveError> {
    let mut pending = dependencies.intents.pending()?;
    pending.sort_by_key(|intent| intent.revisions.generation);
    let mut result = OpenReconciliation::default();
    for intent in pending {
        if intent.project != dependencies.project {
            continue;
        }
        if matches!(intent.state, CheckpointIntentState::Planned) {
            result.awaiting_file_reconciliation += 1;
            continue;
        }
        let completed = finish_checkpoint(dependencies, &intent)?;
        result.completed_checkpoints += 1;
        result.saved_through = Some(completed.revisions.clone());
        mark_saved(&dependencies.status, completed.revisions);
    }
    Ok(result)
}

fn mark_saving(status: &Mutex<SaveStatusSnapshot>, revisions: &SaveRevisionVector) {
    let mut status = status.lock().expect("save status lock");
    status.state = SaveState::Saving;
    status.active = Some(revisions.clone());
    status.error = None;
}

fn mark_saved(status: &Mutex<SaveStatusSnapshot>, revisions: SaveRevisionVector) {
    let mut status = status.lock().expect("save status lock");
    status.active = None;
    status.error = None;
    if status
        .saved_through
        .as_ref()
        .is_none_or(|saved| revisions.covers(saved))
    {
        status.saved_through = Some(revisions);
    }
    status.state = if status.requested.as_ref().is_none_or(|requested| {
        status
            .saved_through
            .as_ref()
            .is_some_and(|saved| saved.covers(requested))
    }) {
        SaveState::Saved
    } else {
        SaveState::Dirty
    };
}

fn mark_error(status: &Mutex<SaveStatusSnapshot>, error: SaveError) {
    let mut status = status.lock().expect("save status lock");
    status.state = SaveState::Error;
    status.active = None;
    status.error = Some(error);
}

#[cfg(test)]
mod save_coordinator_contract_tests;
