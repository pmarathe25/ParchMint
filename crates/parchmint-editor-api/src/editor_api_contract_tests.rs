//! Native contract tests for the engine-neutral editor boundary.
//!
//! The in-memory adapter is a narrow executable fixture, not an editor engine.
//! It proves that the public API can preserve the required session semantics
//! using ParchMint values alone.

use std::{
    collections::BTreeMap,
    future::Future,
    sync::{Arc, Mutex, mpsc},
    task::{Context, Poll, Waker},
};

use parchmint_contracts::generated::RecoveryRecordV1;
use parchmint_recovery_api::{
    CompactionReport, ContentHash, DiscardReport, DocumentRevision, ProjectRevision,
    RecoveryBaseSnapshot, RecoveryBatch, RecoveryError, RecoveryInventory, RecoveryJournal,
    RecoveryReceipt, RecoveryRecord, RecoveryRecordSummary, RecoveryReplay, RecoveryRevisionVector,
    ResourceId, VersionedRecoveryPayload,
};
use parchmint_save::{SaveGeneration, SaveRevisionVector};
use serde_json::json;

use super::*;

#[test]
fn two_views_share_document_history_and_keep_selection_state_independent() {
    let adapter = adapter();
    let session = wait(adapter.open(load()));
    let first = view(1);
    let second = view(2);
    attach(&*adapter, session.clone(), first);
    attach(&*adapter, session.clone(), second);

    adapter
        .execute(session.clone(), origin(first), insert("alpha", 0))
        .unwrap();
    adapter
        .execute(session.clone(), origin(first), select(0, 5))
        .unwrap();
    adapter
        .execute(session.clone(), origin(second), insert(" beta", 1))
        .unwrap();
    adapter
        .execute(session.clone(), origin(first), undo(2))
        .unwrap();

    let projection = wait(adapter.project(session.clone(), revision(3))).unwrap();
    assert_eq!(projection.body(), "alpha");
    assert_eq!(projection.revision(), revision(3));
    assert_eq!(
        adapter.selection(session.clone(), first).unwrap(),
        selection(0, 5)
    );
    assert_eq!(adapter.selection(session, second).unwrap(), selection(0, 0));
}

#[test]
fn stale_commands_are_rejected_without_mutating_revision_or_projection() {
    let adapter = adapter();
    let session = wait(adapter.open(load()));
    let first = view(1);
    attach(&*adapter, session.clone(), first);

    adapter
        .execute(session.clone(), origin(first), insert("alpha", 0))
        .unwrap();
    let before = wait(adapter.project(session.clone(), revision(1))).unwrap();
    let error = adapter
        .execute(session.clone(), origin(first), insert("stale", 0))
        .unwrap_err();

    assert!(error.is_stale_command());
    assert_eq!(wait(adapter.project(session, revision(1))).unwrap(), before);
}

#[test]
fn view_presentation_is_scoped_without_changing_the_document_revision() {
    let adapter = adapter();
    let session = wait(adapter.open(load()));
    let first = view(1);
    let second = view(2);
    attach(&*adapter, session.clone(), first);
    attach(&*adapter, session.clone(), second);

    assert_eq!(
        adapter.selection_geometry(session.clone(), first).unwrap(),
        None
    );
    adapter
        .set_style_catalog(session.clone(), style_catalog())
        .unwrap();
    adapter
        .execute(session.clone(), origin(first), select_at(0, 5, 0))
        .unwrap();
    let geometry = adapter
        .selection_geometry(session.clone(), first)
        .unwrap()
        .expect("a non-empty selection has geometry");
    assert_eq!(geometry.selection(), selection(0, 5));
    assert!(geometry.rectangles().iter().all(|rect| rect.is_finite()));
    adapter
        .set_search_decorations(session.clone(), first, vec![search_decoration(0, 5)])
        .unwrap();
    adapter
        .set_spellcheck_decorations(session.clone(), second, vec![spellcheck_decoration(6, 10)])
        .unwrap();
    assert_eq!(
        wait(adapter.project(session.clone(), revision(0)))
            .unwrap()
            .revision(),
        revision(0)
    );
    let first_state = adapter.detach_view(session.clone(), first).unwrap();
    let second_state = adapter.detach_view(session, second).unwrap();
    assert_eq!(first_state.search_decorations(), &[search_decoration(0, 5)]);
    assert!(first_state.spellcheck_decorations().is_empty());
    assert_eq!(
        second_state.spellcheck_decorations(),
        &[spellcheck_decoration(6, 10)]
    );
    assert!(second_state.search_decorations().is_empty());
}

#[test]
fn projections_are_deterministic_canonical_snapshots() {
    let adapter = adapter();
    let session = wait(adapter.open(load_with_comments_and_anchors()));
    let first = view(1);
    attach(&*adapter, session.clone(), first);
    adapter
        .execute(session.clone(), origin(first), insert("alpha", 0))
        .unwrap();

    let left = wait(adapter.project(session.clone(), revision(1))).unwrap();
    let right = wait(adapter.project(session, revision(1))).unwrap();
    assert_eq!(left, right);
    assert_eq!(left.document_id(), document_id());
    assert_eq!(left.word_count(), 1);
    assert!(left.comments()[0].id < left.comments()[1].id);
    assert!(left.anchors()[0].block < left.anchors()[1].block);
}

#[test]
fn events_and_close_follow_the_session_contract() {
    let adapter = adapter();
    let session = wait(adapter.open(load()));
    let first = view(1);
    let events = adapter.events(session.clone());
    attach(&*adapter, session.clone(), first);
    adapter
        .execute(session.clone(), origin(first), insert("alpha", 0))
        .unwrap();
    adapter.detach_view(session.clone(), first).unwrap();
    wait(adapter.close(session.clone()));
    wait(adapter.close(session.clone()));

    assert_eq!(
        events.collect::<Vec<_>>(),
        vec![
            event_attached(first),
            event_changed(revision(1)),
            event_detached(first),
            EditorEvent::Closed,
        ]
    );
    assert!(matches!(adapter.selection(session.clone(), first), Err(error) if error.is_closed()));
    assert!(matches!(adapter.detach_view(session, first), Err(error) if error.is_closed()));
}

#[test]
fn persistence_coordinator_public_contract_keeps_receipt_separate_from_frontier_ack() {
    let journal = Arc::new(ContractJournal::default());
    let base = recovery_base("base");
    let coordinator = EditorPersistenceCoordinator::new_recovery_only(journal, base.clone());
    let projection =
        CanonicalProjection::new(document_id(), revision(1), "next", vec![], vec![], 1);
    let durable = coordinator
        .persist_projection(&projection, &save_vector(1), payload("next"))
        .unwrap();

    assert_eq!(coordinator.frontier().unwrap(), base.revisions);
    assert_eq!(
        durable.receipt().durable_through,
        durable.batch().revision_vector()
    );
    let receipt = durable.receipt().clone();
    let frontier = coordinator.acknowledge_recovery(durable).unwrap();
    assert_eq!(receipt.durable_through, frontier);
    assert_eq!(coordinator.frontier().unwrap(), frontier);
}

#[test]
fn persistence_coordinator_public_contract_reconciles_unacknowledged_batches_exactly() {
    let journal = Arc::new(ContractJournal::default());
    let base = recovery_base("base");
    let coordinator =
        EditorPersistenceCoordinator::new_recovery_only(journal.clone(), base.clone());
    let first = CanonicalProjection::new(document_id(), revision(1), "one", vec![], vec![], 1);
    let second = CanonicalProjection::new(document_id(), revision(2), "one two", vec![], vec![], 2);
    coordinator
        .acknowledge_recovery(
            coordinator
                .persist_projection(&first, &save_vector(1), payload("one"))
                .unwrap(),
        )
        .unwrap();
    let unacknowledged = coordinator
        .persist_projection(&second, &save_vector(2), payload("one two"))
        .unwrap();
    let original_receipt = unacknowledged.receipt().clone();

    let reopened = EditorPersistenceCoordinator::new_recovery_only(journal, base.clone());
    let resumed_frontier = reopened
        .resume_recovery_acknowledgement(base.clone(), unacknowledged)
        .unwrap();
    let replay = reopened.reconcile_recovery(base).unwrap();
    assert_eq!(replay.accepted.len(), 2);
    assert!(replay.isolated.is_empty());
    assert_eq!(replay.accepted[0].payload, payload("one"));
    assert_eq!(replay.accepted[1].payload, payload("one two"));
    assert_eq!(
        replay.accepted[0].result_hashes,
        replay.accepted[1].base_hashes
    );
    assert_eq!(
        original_receipt.durable_through,
        replay.accepted[1].revision_vector()
    );
    assert_eq!(resumed_frontier, replay.accepted[1].revision_vector());
    assert_eq!(
        reopened.frontier().unwrap(),
        replay.accepted[1].revision_vector()
    );
}

#[test]
fn persistence_coordinator_keeps_distinct_recovery_hashes_for_multiple_documents() {
    let journal = Arc::new(ContractJournal::default());
    let first = document_id();
    let second = DocumentId::from_bytes([2; 16]);
    let first_resource = document_resource_id(first);
    let second_resource = document_resource_id(second);
    let base = RecoveryBaseSnapshot {
        revisions: RecoveryRevisionVector::new(ProjectRevision::default(), BTreeMap::new()),
        hashes: BTreeMap::from([
            (first_resource.clone(), hash("first base")),
            (second_resource.clone(), hash("second base")),
        ]),
    };
    let coordinator = EditorPersistenceCoordinator::new_recovery_only(journal.clone(), base);
    let projection = CanonicalProjection::new(first, revision(1), "first edit", vec![], vec![], 2);
    let revisions = SaveRevisionVector {
        project_revision: ProjectRevision::default(),
        open_documents: BTreeMap::from([(first, DocumentRevision::from(1))]),
        closed_resources: BTreeMap::new(),
        canonical_hashes: BTreeMap::new(),
        generation: SaveGeneration::from(1),
    };

    let durable = coordinator
        .persist_projection(&projection, &revisions, payload("first edit"))
        .expect("first document recovery should become durable");
    coordinator
        .acknowledge_recovery(durable)
        .expect("durable recovery should advance the frontier");
    let second_projection =
        CanonicalProjection::new(second, revision(1), "second edit", vec![], vec![], 2);
    let second_revisions = SaveRevisionVector {
        project_revision: ProjectRevision::default(),
        open_documents: BTreeMap::from([(second, DocumentRevision::from(1))]),
        closed_resources: BTreeMap::new(),
        canonical_hashes: BTreeMap::new(),
        generation: SaveGeneration::from(2),
    };
    let durable = coordinator
        .persist_projection(
            &second_projection,
            &second_revisions,
            payload("second edit"),
        )
        .expect("second document recovery should become durable");
    coordinator
        .acknowledge_recovery(durable)
        .expect("second durable recovery should advance the frontier");

    let records = journal.records.lock().unwrap();
    let RecoveryRecord::Complete(first_batch) = &records[0] else {
        panic!("contract journal records complete batches")
    };
    let RecoveryRecord::Complete(second_batch) = &records[1] else {
        panic!("contract journal records complete batches")
    };
    assert_eq!(first_batch.base_hashes.len(), 1);
    assert_eq!(second_batch.base_hashes.len(), 1);
    assert!(first_batch.base_hashes.contains_key(&first_resource));
    assert!(second_batch.base_hashes.contains_key(&second_resource));
    assert_ne!(first_resource, second_resource);
    assert_ne!(
        first_batch.result_hashes[&first_resource],
        second_batch.result_hashes[&second_resource]
    );
}

#[test]
fn persistence_coordinator_merges_partial_document_frontiers_during_ack_and_replay() {
    let journal = Arc::new(ContractJournal::default());
    let first = document_id();
    let second = DocumentId::from_bytes([2; 16]);
    let first_resource = document_resource_id(first);
    let second_resource = document_resource_id(second);
    let base = RecoveryBaseSnapshot {
        revisions: RecoveryRevisionVector::new(ProjectRevision::default(), BTreeMap::new()),
        hashes: BTreeMap::from([
            (first_resource, hash("first base")),
            (second_resource, hash("second base")),
        ]),
    };
    let coordinator =
        EditorPersistenceCoordinator::new_recovery_only(journal.clone(), base.clone());

    let persist = |coordinator: &EditorPersistenceCoordinator,
                   document,
                   revision,
                   body,
                   generation| {
        let projection = CanonicalProjection::new(document, revision, body, vec![], vec![], 1);
        let revisions = SaveRevisionVector {
            project_revision: ProjectRevision::default(),
            open_documents: BTreeMap::from([(document, DocumentRevision::from(revision.value()))]),
            closed_resources: BTreeMap::new(),
            canonical_hashes: BTreeMap::new(),
            generation: SaveGeneration::from(generation),
        };
        coordinator
            .persist_projection(&projection, &revisions, payload(body))
            .unwrap()
    };

    coordinator
        .acknowledge_recovery(persist(&coordinator, first, revision(1), "first one", 1))
        .unwrap();
    coordinator
        .acknowledge_recovery(persist(
            &coordinator,
            second,
            revision(7),
            "second seven",
            2,
        ))
        .unwrap();
    let third = persist(&coordinator, first, revision(2), "first two", 3);
    assert_eq!(
        third.batch().documents[&first].first,
        DocumentRevision::from(2)
    );
    coordinator.acknowledge_recovery(third).unwrap();

    let expected = RecoveryRevisionVector::new(
        ProjectRevision::from(3),
        BTreeMap::from([
            (first, DocumentRevision::from(2)),
            (second, DocumentRevision::from(7)),
        ]),
    );
    assert_eq!(coordinator.frontier().unwrap(), expected);

    let reopened = EditorPersistenceCoordinator::new_recovery_only(journal, base.clone());
    let replay = reopened.reconcile_recovery(base).unwrap();
    assert_eq!(replay.accepted.len(), 3);
    assert!(replay.isolated.is_empty());
    assert_eq!(reopened.frontier().unwrap(), expected);
}

#[derive(Debug, Default)]
struct ContractJournal {
    records: Mutex<Vec<RecoveryRecord>>,
}

impl RecoveryJournal for ContractJournal {
    fn append(&self, batch: RecoveryBatch) -> Result<RecoveryReceipt, RecoveryError> {
        let mut records = self.records.lock().unwrap();
        let previous = records.last().and_then(|record| match record {
            RecoveryRecord::Complete(batch) => Some(batch),
            _ => None,
        });
        batch.validate()?;
        if let Some(previous) = previous
            && batch.project_revision != previous.project_revision.next()
        {
            return Err(RecoveryError::NonConsecutiveProjectRevision {
                expected: previous.project_revision.next(),
                actual: batch.project_revision,
            });
        }
        let frontier = batch.revision_vector();
        records.push(RecoveryRecord::Complete(batch));
        let RecoveryRecord::Complete(batch) = records.last().expect("record appended") else {
            unreachable!("contract journal stores complete records")
        };
        assert_eq!(batch.revision_vector(), frontier);
        Ok(RecoveryReceipt::for_batch(batch))
    }

    fn flush_through(
        &self,
        target: RecoveryRevisionVector,
    ) -> Result<RecoveryReceipt, RecoveryError> {
        if self
            .records
            .lock()
            .unwrap()
            .iter()
            .any(|record| matches!(record, RecoveryRecord::Complete(batch) if batch.revision_vector() == target))
        {
            let records = self.records.lock().unwrap();
            let record = records.iter().find(|record| {
                matches!(record, RecoveryRecord::Complete(batch) if batch.revision_vector() == target)
            });
            let Some(RecoveryRecord::Complete(batch)) = record else {
                return Err(RecoveryError::UnknownRevisionVector);
            };
            Ok(RecoveryReceipt::for_batch(batch))
        } else {
            Err(RecoveryError::UnknownRevisionVector)
        }
    }

    fn inspect(&self) -> Result<RecoveryInventory, RecoveryError> {
        let records = self.records.lock().unwrap();
        Ok(RecoveryInventory {
            records: records
                .iter()
                .enumerate()
                .map(|(position, record)| RecoveryRecordSummary {
                    position,
                    project_revision: match record {
                        RecoveryRecord::Complete(batch) => Some(batch.project_revision),
                        _ => None,
                    },
                })
                .collect(),
            durable_through: records.last().and_then(|record| match record {
                RecoveryRecord::Complete(batch) => Some(batch.revision_vector()),
                _ => None,
            }),
        })
    }

    fn replay(&self, base: RecoveryBaseSnapshot) -> Result<RecoveryReplay, RecoveryError> {
        Ok(parchmint_recovery_api::replay_records(
            &base,
            self.records.lock().unwrap().clone(),
        ))
    }

    fn compact(
        &self,
        _durable: parchmint_recovery_api::DurableRevisionVector,
    ) -> Result<CompactionReport, RecoveryError> {
        Ok(CompactionReport {
            removed_records: 0,
            retained_records: self.records.lock().unwrap().len(),
        })
    }

    fn discard_through(
        &self,
        _durable: parchmint_recovery_api::DurableRevisionVector,
    ) -> Result<DiscardReport, RecoveryError> {
        Ok(DiscardReport {
            removed_records: 0,
            retained_records: self.records.lock().unwrap().len(),
        })
    }
}

fn recovery_base(body: &str) -> RecoveryBaseSnapshot {
    RecoveryBaseSnapshot {
        revisions: RecoveryRevisionVector::new(ProjectRevision::default(), BTreeMap::new()),
        hashes: BTreeMap::from([(ResourceId::Document, hash(body))]),
    }
}

fn save_vector(revision: u64) -> SaveRevisionVector {
    SaveRevisionVector {
        project_revision: ProjectRevision::default(),
        open_documents: BTreeMap::from([(document_id(), DocumentRevision::from(revision))]),
        closed_resources: BTreeMap::new(),
        canonical_hashes: BTreeMap::from([(
            ResourceId::Document,
            hash(if revision == 1 { "next" } else { "one two" }),
        )]),
        generation: SaveGeneration::from(revision),
    }
}

fn payload(body: &str) -> VersionedRecoveryPayload {
    VersionedRecoveryPayload::V1(RecoveryRecordV1 {
        schema: "parchmint.recovery-record/v1".into(),
        record_id: format!("contract-{body}"),
        operations: vec![json!({ "body": body })],
    })
}

fn hash(body: &str) -> ContentHash {
    ContentHash::of_bytes(body.as_bytes())
}

fn attach(adapter: &dyn EditorAdapter, session: SharedEditorSession, view: ViewId) {
    adapter.attach_view(session, view, host(view)).unwrap();
}

fn wait<T>(future: impl Future<Output = T>) -> T {
    let mut future = Box::pin(future);
    let waker = Waker::noop();
    let mut context = Context::from_waker(waker);
    match future.as_mut().poll(&mut context) {
        Poll::Ready(value) => value,
        Poll::Pending => panic!("native contract adapter must settle immediately"),
    }
}

fn adapter() -> Box<dyn EditorAdapter> {
    Box::new(NativeEditorAdapter::default())
}

fn load() -> CanonicalDocumentLoad {
    CanonicalDocumentLoad::new(document_id(), "")
}

fn load_with_comments_and_anchors() -> CanonicalDocumentLoad {
    let mut load = load();
    load.comments.push(CanonicalComment::new(
        CommentId::from_bytes([4; 16]),
        selection(0, 0),
        "later note",
        BlockId::from_bytes([1; 16]),
    ));
    load.comments.push(CanonicalComment::new(
        CommentId::from_bytes([3; 16]),
        selection(0, 0),
        "first note",
        BlockId::from_bytes([1; 16]),
    ));
    load.anchors = vec![
        CanonicalAnchor {
            block: BlockId::from_bytes([2; 16]),
            position: DocumentPosition::from(2),
        },
        CanonicalAnchor {
            block: BlockId::from_bytes([1; 16]),
            position: DocumentPosition::from(1),
        },
    ];
    load
}

fn host(view: ViewId) -> ViewHostCapability {
    ViewHostCapability::new(view.as_bytes()[15] as u64)
}

fn view(last_byte: u8) -> ViewId {
    ViewId::from_bytes([last_byte; 16])
}

fn document_id() -> DocumentId {
    DocumentId::from_bytes([1; 16])
}

fn revision(value: u64) -> EditorRevision {
    EditorRevision::from(value)
}

fn origin(view: ViewId) -> EditorCommandOrigin {
    EditorCommandOrigin::new(view)
}

fn insert(text: &str, observed_revision: u64) -> EditorCommand {
    EditorCommand::new(
        revision(observed_revision),
        EditorCommandKind::InsertText {
            at: DocumentPosition::from(u64::MAX),
            text: text.into(),
        },
    )
}

fn select(start: u64, end: u64) -> EditorCommand {
    select_at(start, end, 1)
}

fn select_at(start: u64, end: u64, observed_revision: u64) -> EditorCommand {
    EditorCommand::new(
        revision(observed_revision),
        EditorCommandKind::SetSelection {
            selection: selection(start, end),
        },
    )
}

fn undo(observed_revision: u64) -> EditorCommand {
    EditorCommand::new(revision(observed_revision), EditorCommandKind::Undo)
}

fn selection(start: u64, end: u64) -> EditorSelection {
    EditorSelection::new(start.into(), end.into())
}

fn style_catalog() -> StyleCatalogProjection {
    StyleCatalogProjection::new(StyleCatalog::default())
}

fn search_decoration(start: u64, end: u64) -> SearchDecoration {
    SearchDecoration::new(selection(start, end))
}

fn spellcheck_decoration(start: u64, end: u64) -> SpellcheckDecoration {
    SpellcheckDecoration::new(selection(start, end))
}

fn event_attached(view: ViewId) -> EditorEvent {
    EditorEvent::ViewAttached { view }
}

fn event_changed(revision: EditorRevision) -> EditorEvent {
    EditorEvent::DocumentChanged { revision }
}

fn event_detached(view: ViewId) -> EditorEvent {
    EditorEvent::ViewDetached { view }
}

#[derive(Default)]
struct NativeEditorAdapter {
    inner: Arc<Mutex<NativeEditorState>>,
}

#[derive(Default)]
struct NativeEditorState {
    next_session: u64,
    sessions: BTreeMap<SharedEditorSession, NativeSession>,
}

struct NativeSession {
    load: CanonicalDocumentLoad,
    revision: EditorRevision,
    undo: Vec<String>,
    redo: Vec<String>,
    views: BTreeMap<ViewId, EditorViewState>,
    projections: BTreeMap<EditorRevision, CanonicalProjection>,
    closed: bool,
    subscribers: Vec<mpsc::Sender<EditorEvent>>,
}

impl NativeSession {
    fn new(load: CanonicalDocumentLoad) -> Self {
        let projection = projection(&load, revision(0));
        Self {
            load,
            revision: revision(0),
            undo: Vec::new(),
            redo: Vec::new(),
            views: BTreeMap::new(),
            projections: BTreeMap::from([(revision(0), projection)]),
            closed: false,
            subscribers: Vec::new(),
        }
    }

    fn require_open(&self) -> Result<(), EditorError> {
        if self.closed {
            Err(EditorError::Closed)
        } else {
            Ok(())
        }
    }

    fn publish(&mut self, event: EditorEvent) {
        self.subscribers
            .retain(|sender| sender.send(event.clone()).is_ok());
    }

    fn record_document_change(&mut self) {
        self.revision = self.revision.next();
        self.projections
            .insert(self.revision, projection(&self.load, self.revision));
        self.publish(event_changed(self.revision));
    }
}

impl EditorAdapter for NativeEditorAdapter {
    fn open(&self, load: CanonicalDocumentLoad) -> AsyncResult<SharedEditorSession> {
        let mut state = self
            .inner
            .lock()
            .expect("native editor state is not poisoned");
        state.next_session = state.next_session.saturating_add(1);
        let session = SharedEditorSession::new(state.next_session);
        state
            .sessions
            .insert(session.clone(), NativeSession::new(load));
        Box::pin(async move { session })
    }

    fn attach_view(
        &self,
        session: SharedEditorSession,
        view: ViewId,
        _host: ViewHostCapability,
    ) -> Result<(), EditorError> {
        self.with_session(session, |state| {
            state.require_open()?;
            if state.views.contains_key(&view) {
                return Err(EditorError::ViewAlreadyAttached { view });
            }
            state
                .views
                .insert(view, EditorViewState::new(selection(0, 0)));
            state.publish(event_attached(view));
            Ok(())
        })
    }

    fn detach_view(
        &self,
        session: SharedEditorSession,
        view: ViewId,
    ) -> Result<EditorViewState, EditorError> {
        self.with_session(session, |state| {
            state.require_open()?;
            let view_state = state
                .views
                .remove(&view)
                .ok_or(EditorError::UnknownView { view })?;
            state.publish(event_detached(view));
            Ok(view_state)
        })
    }

    fn execute(
        &self,
        session: SharedEditorSession,
        origin: EditorCommandOrigin,
        command: EditorCommand,
    ) -> Result<(), EditorError> {
        self.with_session(session, |state| {
            state.require_open()?;
            if !state.views.contains_key(&origin.view()) {
                return Err(EditorError::UnknownView {
                    view: origin.view(),
                });
            }
            if command.observed_revision() != state.revision {
                return Err(EditorError::StaleCommand {
                    observed: command.observed_revision(),
                    current: state.revision,
                });
            }
            match command.kind() {
                EditorCommandKind::InsertText { at, text } => {
                    if at.value() != u64::MAX
                        && at.value() != state.load.body.chars().count() as u64
                    {
                        return Err(EditorError::InvalidCommand {
                            reason: "native fixture supports insertion at the document end only",
                        });
                    }
                    state.undo.push(state.load.body.clone());
                    state.redo.clear();
                    state.load.body.push_str(text);
                    state.record_document_change();
                }
                EditorCommandKind::SetSelection { selection } => {
                    state
                        .views
                        .get_mut(&origin.view())
                        .expect("origin attachment was checked")
                        .selection = *selection;
                }
                EditorCommandKind::Undo => {
                    let prior = state.undo.pop().ok_or(EditorError::InvalidCommand {
                        reason: "nothing to undo",
                    })?;
                    state.redo.push(state.load.body.clone());
                    state.load.body = prior;
                    state.record_document_change();
                }
                EditorCommandKind::Redo => {
                    let next = state.redo.pop().ok_or(EditorError::InvalidCommand {
                        reason: "nothing to redo",
                    })?;
                    state.undo.push(state.load.body.clone());
                    state.load.body = next;
                    state.record_document_change();
                }
                EditorCommandKind::DeleteRange { .. }
                | EditorCommandKind::ReplaceRange { .. }
                | EditorCommandKind::ReplaceRangeWithSemanticText { .. }
                | EditorCommandKind::ReplaceRangeWithSemanticFragment { .. }
                | EditorCommandKind::ApplyParagraphStyle { .. } => {
                    return Err(EditorError::InvalidCommand {
                        reason: "native fixture does not model this command",
                    });
                }
                EditorCommandKind::ToggleInlineMark { .. }
                | EditorCommandKind::SetLink { .. }
                | EditorCommandKind::ToggleBlockFormat { .. }
                | EditorCommandKind::InsertAtomicBlock { .. }
                | EditorCommandKind::SplitBlock { .. }
                | EditorCommandKind::InsertSoftBreak { .. }
                | EditorCommandKind::AdjustListDepth { .. }
                | EditorCommandKind::CreateComment { .. }
                | EditorCommandKind::ReplyToComment { .. }
                | EditorCommandKind::SetCommentResolved { .. }
                | EditorCommandKind::DeleteCommentThread { .. }
                | EditorCommandKind::DeleteCommentMessage { .. }
                | EditorCommandKind::EditCommentMessage { .. }
                | EditorCommandKind::ReattachComment { .. }
                | EditorCommandKind::ConvertCommentToDocument { .. } => {
                    state.record_document_change();
                }
            }
            Ok(())
        })
    }

    fn selection(
        &self,
        session: SharedEditorSession,
        view: ViewId,
    ) -> Result<EditorSelection, EditorError> {
        self.with_session(session, |state| {
            state.require_open()?;
            state
                .views
                .get(&view)
                .map(EditorViewState::selection)
                .ok_or(EditorError::UnknownView { view })
        })
    }

    fn selection_clipboard(
        &self,
        session: SharedEditorSession,
        view: ViewId,
    ) -> Result<Option<EditorClipboardContent>, EditorError> {
        self.with_session(session, |state| {
            state.require_open()?;
            let selection = state
                .views
                .get(&view)
                .map(EditorViewState::selection)
                .ok_or(EditorError::UnknownView { view })?;
            if selection.is_collapsed() {
                return Ok(None);
            }
            let text = state.load.body.chars().collect::<Vec<_>>();
            let start = usize::try_from(selection.start().value()).map_err(|_| {
                EditorError::InvalidCommand {
                    reason: "selection start exceeds the fixture range",
                }
            })?;
            let end = usize::try_from(selection.end().value()).map_err(|_| {
                EditorError::InvalidCommand {
                    reason: "selection end exceeds the fixture range",
                }
            })?;
            let selected = text
                .get(start..end)
                .ok_or(EditorError::InvalidCommand {
                    reason: "selection is outside the fixture document",
                })?
                .iter()
                .collect::<String>();
            Ok(Some(EditorClipboardContent::new(
                state.revision,
                selection,
                selected,
                None,
            )))
        })
    }

    fn selection_geometry(
        &self,
        session: SharedEditorSession,
        view: ViewId,
    ) -> Result<Option<SelectionGeometry>, EditorError> {
        self.with_session(session, |state| {
            state.require_open()?;
            let selection = state
                .views
                .get(&view)
                .map(EditorViewState::selection)
                .ok_or(EditorError::UnknownView { view })?;
            if selection.is_collapsed() {
                return Ok(None);
            }
            Ok(Some(SelectionGeometry::new(
                selection,
                vec![SelectionRectangle {
                    x: selection.start().value() as f32,
                    y: 0.0,
                    width: (selection.end().value() - selection.start().value()) as f32,
                    height: 1.0,
                }],
            )))
        })
    }

    fn set_style_catalog(
        &self,
        session: SharedEditorSession,
        styles: StyleCatalogProjection,
    ) -> Result<(), EditorError> {
        self.with_session(session, |state| {
            state.require_open()?;
            state.load.styles = styles;
            Ok(())
        })
    }

    fn set_search_decorations(
        &self,
        session: SharedEditorSession,
        view: ViewId,
        decorations: Vec<SearchDecoration>,
    ) -> Result<(), EditorError> {
        self.with_session(session, |state| {
            state.require_open()?;
            state
                .views
                .get_mut(&view)
                .ok_or(EditorError::UnknownView { view })?
                .search_decorations = decorations;
            Ok(())
        })
    }

    fn set_spellcheck_decorations(
        &self,
        session: SharedEditorSession,
        view: ViewId,
        decorations: Vec<SpellcheckDecoration>,
    ) -> Result<(), EditorError> {
        self.with_session(session, |state| {
            state.require_open()?;
            state
                .views
                .get_mut(&view)
                .ok_or(EditorError::UnknownView { view })?
                .spellcheck_decorations = decorations;
            Ok(())
        })
    }

    fn apply_composite_project_edit(
        &self,
        session: SharedEditorSession,
        operation: ProjectDocumentOperation,
    ) -> Result<(), EditorError> {
        self.with_session(session, |state| {
            state.require_open()?;
            if operation.observed_revision != state.revision {
                return Err(EditorError::StaleCommand {
                    observed: operation.observed_revision,
                    current: state.revision,
                });
            }
            if operation.replacement.document_id != state.load.document_id {
                return Err(EditorError::DocumentMismatch {
                    expected: state.load.document_id,
                    received: operation.replacement.document_id,
                });
            }
            state.load = operation.replacement;
            state.undo.clear();
            state.redo.clear();
            state.record_document_change();
            Ok(())
        })
    }

    fn project(
        &self,
        session: SharedEditorSession,
        through: EditorRevision,
    ) -> AsyncResult<Result<CanonicalProjection, EditorError>> {
        let projection = self.with_session(session, |state| {
            state.require_open()?;
            state
                .projections
                .get(&through)
                .cloned()
                .ok_or(EditorError::InvalidCommand {
                    reason: "requested projection revision is unavailable",
                })
        });
        Box::pin(async move { projection })
    }

    fn events(&self, session: SharedEditorSession) -> EventStream<EditorEvent> {
        let (sender, receiver) = mpsc::channel();
        if let Ok(mut state) = self.inner.lock()
            && let Some(session_state) = state.sessions.get_mut(&session)
            && !session_state.closed
        {
            session_state.subscribers.push(sender);
        }
        EventStream::from_receiver(receiver)
    }

    fn close(&self, session: SharedEditorSession) -> AsyncResult<()> {
        let result = self.with_session(session, |state| {
            if !state.closed {
                state.views.clear();
                state.closed = true;
                state.publish(EditorEvent::Closed);
                state.subscribers.clear();
            }
            Ok(())
        });
        Box::pin(async move { result.expect("close requires a known session") })
    }

    fn capabilities(&self) -> EditorCapabilities {
        EditorCapabilities::default()
    }
}

impl NativeEditorAdapter {
    fn with_session<T>(
        &self,
        session: SharedEditorSession,
        operation: impl FnOnce(&mut NativeSession) -> Result<T, EditorError>,
    ) -> Result<T, EditorError> {
        let mut state = self
            .inner
            .lock()
            .expect("native editor state is not poisoned");
        let session = state
            .sessions
            .get_mut(&session)
            .ok_or(EditorError::UnknownSession)?;
        operation(session)
    }
}

fn projection(load: &CanonicalDocumentLoad, revision: EditorRevision) -> CanonicalProjection {
    CanonicalProjection::new(
        load.document_id,
        revision,
        &load.body,
        canonical_comments(&load.comments),
        canonical_anchors(&load.anchors),
        load.body.split_whitespace().count(),
    )
}

fn canonical_comments(comments: &[CanonicalComment]) -> Vec<CanonicalComment> {
    let mut comments = comments.to_vec();
    comments.sort_by_key(|comment| comment.id);
    comments
}

fn canonical_anchors(anchors: &[CanonicalAnchor]) -> Vec<CanonicalAnchor> {
    let mut anchors = anchors.to_vec();
    anchors.sort_by_key(|anchor| (anchor.block, anchor.position));
    anchors
}
