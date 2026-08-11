use std::{
    collections::BTreeMap,
    future::Future,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    task::{Context, Poll, Waker},
};

use parchmint_contracts::generated::RecoveryRecordV1;
use parchmint_editor_api::{
    CanonicalProjection, DocumentId, DurableProjectionBatch, EditorPersistenceError, EditorRevision,
};
use parchmint_project_repository::AtomicWritePlan;
use parchmint_recovery_api::{
    CompactionReport, ContentHash, DiscardReport, RecoveryBaseSnapshot, RecoveryBatch,
    RecoveryError, RecoveryInventory, RecoveryJournal, RecoveryReceipt, RecoveryRecord,
    RecoveryRecordSummary, RecoveryReplay, RecoveryRevisionVector, ResourceId,
    VersionedRecoveryPayload,
};
use parchmint_save::{
    CheckpointCategory, CheckpointInput, CheckpointIntentHash, SaveCoordinator, SaveError,
    SavePriority, SaveRequest, SaveStatusSnapshot, SaveTicket, SavedAcknowledgement,
};
use serde_json::json;
use sha2::{Digest, Sha256};

use super::*;

fn wait<T>(future: impl Future<Output = T>) -> T {
    let mut context = Context::from_waker(Waker::noop());
    let mut future = Box::pin(future);
    loop {
        match future.as_mut().poll(&mut context) {
            Poll::Ready(value) => return value,
            Poll::Pending => std::thread::yield_now(),
        }
    }
}

fn stable_id(value: u8) -> [u8; 16] {
    [value; 16]
}

fn project_id() -> ProjectId {
    ProjectId::from_bytes(stable_id(1))
}

fn group_id() -> parchmint_domain::NodeId {
    parchmint_domain::NodeId::from_bytes(stable_id(2))
}

fn open_document() -> DocumentId {
    DocumentId::from_bytes(stable_id(3))
}

fn closed_document() -> DocumentId {
    DocumentId::from_bytes(stable_id(4))
}

fn sample_project() -> Project {
    let project = Project::new(project_id());
    parchmint_domain::apply_project_command(
        &project,
        project.revision,
        ProjectCommand::create_group(
            group_id(),
            parchmint_domain::NodeId::manuscript_root(),
            0,
            "Draft",
        ),
    )
    .expect("sample group is valid")
    .project
}

fn sample_documents() -> Arc<NativeDocumentStateOwner> {
    Arc::new(NativeDocumentStateOwner::new([
        DocumentSnapshot {
            document_id: open_document(),
            body: "alpha needle".into(),
            revision: EditorRevision::from(0),
            visibility: DocumentVisibility::Open,
        },
        DocumentSnapshot {
            document_id: closed_document(),
            body: "closed needle".into(),
            revision: EditorRevision::from(0),
            visibility: DocumentVisibility::Closed,
        },
    ]))
}

fn setup() -> (
    NativeProjectCommandDispatcher,
    Arc<NativeDocumentStateOwner>,
) {
    let documents = sample_documents();
    let dispatcher = NativeProjectCommandDispatcher::new(sample_project(), documents.clone());
    (dispatcher, documents)
}

#[test]
fn production_editor_coordinator_routes_projections_and_tracks_newer_dirty_frontier() {
    let journal = Arc::new(ProductionJournal::default());
    let base = RecoveryBaseSnapshot {
        revisions: RecoveryRevisionVector::new(
            parchmint_domain::ProjectRevision::default(),
            BTreeMap::new(),
        ),
        hashes: BTreeMap::from([(ResourceId::Document, hash("base"))]),
    };
    let coordinator =
        EditorPersistenceCoordinator::new_recovery_only(journal.clone(), base.clone());
    let first = CanonicalProjection::new(
        document_id(),
        EditorRevision::from(1),
        "one",
        vec![],
        vec![],
        1,
    );
    let second = CanonicalProjection::new(
        document_id(),
        EditorRevision::from(2),
        "one two",
        vec![],
        vec![],
        2,
    );

    let first_durable = coordinator
        .persist_projection(&first, &save_vector(1), payload("one"))
        .expect("first projection routes through production coordinator");
    assert_eq!(coordinator.status().state, parchmint_save::SaveState::Dirty);
    assert_eq!(coordinator.status().requested, Some(save_vector(1)));
    assert_eq!(coordinator.frontier().unwrap(), base.revisions);
    coordinator
        .acknowledge_recovery(first_durable.clone())
        .unwrap();
    let fabricated = SavedAcknowledgement {
        ticket_id: SaveTicket::pending(999).id(),
        requested_revisions: save_vector(1),
        written_revisions: save_vector(1),
        checkpoint: parchmint_domain::CheckpointId::from_bytes([1; 16]),
    };
    assert!(coordinator.acknowledge_save(&fabricated).is_err());

    let second_durable = coordinator
        .persist_projection(&second, &save_vector(2), payload("one two"))
        .expect("newer projection routes through production coordinator");
    assert_eq!(coordinator.status().state, parchmint_save::SaveState::Dirty);
    assert_eq!(coordinator.status().requested, Some(save_vector(2)));
    assert!(matches!(
        DurableProjectionBatch::new(
            second_durable.batch().clone(),
            first_durable.receipt().clone()
        ),
        Err(EditorPersistenceError::Recovery(
            RecoveryError::UnknownRevisionVector
        ))
    ));
    coordinator.acknowledge_recovery(second_durable).unwrap();
    assert_eq!(coordinator.frontier().unwrap().project_revision.value(), 2);
    let reopened = EditorPersistenceCoordinator::new_recovery_only(journal, base.clone());
    let replay = reopened.reconcile_recovery(base).unwrap();
    assert_eq!(replay.accepted.len(), 2);
    assert_eq!(reopened.status().state, parchmint_save::SaveState::Dirty);
}

#[test]
fn production_reconciliation_exposes_retained_inventory_and_exact_isolation_reason() {
    let journal = Arc::new(ProductionJournal::default());
    let base = RecoveryBaseSnapshot {
        revisions: RecoveryRevisionVector::new(
            parchmint_domain::ProjectRevision::default(),
            BTreeMap::new(),
        ),
        hashes: BTreeMap::from([(ResourceId::Document, hash("base"))]),
    };
    let coordinator =
        EditorPersistenceCoordinator::new_recovery_only(journal.clone(), base.clone());
    let projection = CanonicalProjection::new(
        document_id(),
        EditorRevision::from(1),
        "one",
        vec![],
        vec![],
        1,
    );
    coordinator
        .persist_projection(&projection, &save_vector(1), payload("one"))
        .expect("persist valid recovery record");
    journal
        .records
        .lock()
        .unwrap()
        .push(RecoveryRecord::UnknownVersion {
            project_revision: Some(parchmint_domain::ProjectRevision::from(2)),
            version: "v9".into(),
        });

    let replay = coordinator
        .reconcile_recovery(base)
        .expect("reconcile retained records");
    assert_eq!(replay.accepted.len(), 1);
    let isolation = replay.isolation.expect("isolation reason");
    assert_eq!(isolation.position, 1);
    assert_eq!(
        isolation.reason,
        parchmint_recovery_api::RecoveryIsolationReason::UnknownVersion {
            version: "v9".into()
        }
    );
    let status = coordinator.status();
    assert_eq!(status.state, parchmint_save::SaveState::Error);
    assert_eq!(status.recovery_retained_records, 2);
    assert_eq!(status.recovery_inventory.as_ref().unwrap().records.len(), 2);
    assert_eq!(status.recovery_isolation, Some(isolation.clone()));
    assert_eq!(
        status.error,
        Some(EditorPersistenceError::RecoveryIsolation(isolation.reason))
    );
}

#[test]
fn production_editor_coordinator_coalesces_repeated_save_requests_and_bounds_queue() {
    let journal = Arc::new(ProductionJournal::default());
    let save = Arc::new(RecordingSave::default());
    let coordinator = EditorPersistenceCoordinator::new(
        journal,
        save.clone(),
        RecoveryBaseSnapshot {
            revisions: RecoveryRevisionVector::new(
                parchmint_domain::ProjectRevision::default(),
                BTreeMap::new(),
            ),
            hashes: BTreeMap::from([(ResourceId::Document, hash("base"))]),
        },
    );
    let projection = CanonicalProjection::new(
        document_id(),
        EditorRevision::from(1),
        "one",
        vec![],
        vec![],
        1,
    );
    let request = save_request(1);
    let first_ticket = coordinator.submit_save(&projection, request).unwrap();
    let mut latest_ticket = first_ticket.clone();
    for revision in 2..=8 {
        let projection = CanonicalProjection::new(
            document_id(),
            EditorRevision::from(revision),
            format!("revision-{revision}"),
            vec![],
            vec![],
            1,
        );
        latest_ticket = coordinator
            .submit_save(&projection, save_request(revision))
            .unwrap();
    }
    assert_eq!(save.requests.lock().unwrap().len(), 8);
    assert_eq!(coordinator.save_queue_depth(), 1);
    assert_eq!(coordinator.max_save_queue_depth(), 1);
    assert_eq!(coordinator.submitted_save_requests(), 8);
    assert_eq!(coordinator.coalesced_save_requests(), 7);
    assert!(
        coordinator
            .acknowledge_save(&SavedAcknowledgement {
                ticket_id: first_ticket.id(),
                requested_revisions: save_vector(1),
                written_revisions: save_vector(1),
                checkpoint: parchmint_domain::CheckpointId::from_bytes([1; 16]),
            })
            .is_err()
    );
    assert!(
        coordinator
            .acknowledge_save(&SavedAcknowledgement {
                ticket_id: latest_ticket.id(),
                requested_revisions: save_vector(1),
                written_revisions: save_vector(1),
                checkpoint: parchmint_domain::CheckpointId::from_bytes([1; 16]),
            })
            .is_err()
    );
    assert!(
        coordinator
            .acknowledge_save(&SavedAcknowledgement {
                ticket_id: latest_ticket.id(),
                requested_revisions: save_vector(8),
                written_revisions: save_vector(8),
                checkpoint: parchmint_domain::CheckpointId::from_bytes([8; 16]),
            })
            .is_err()
    );
}

#[derive(Debug, Default)]
struct RecordingSave {
    requests: Mutex<Vec<SaveRequest>>,
}

#[derive(Debug, Default)]
struct CompletedSave {
    requests: Mutex<Vec<SaveRequest>>,
    next: AtomicU64,
}

impl SaveCoordinator for CompletedSave {
    fn request(&self, request: SaveRequest) -> Result<SaveTicket, SaveError> {
        let id = self.next.fetch_add(1, Ordering::Relaxed) + 1;
        self.requests.lock().unwrap().push(request.clone());
        let acknowledgement = SavedAcknowledgement {
            ticket_id: SaveTicket::pending(id).id(),
            requested_revisions: request.revisions.clone(),
            written_revisions: request.revisions,
            checkpoint: parchmint_domain::CheckpointId::from_bytes([id as u8; 16]),
        };
        Ok(SaveTicket::completed(id, acknowledgement))
    }

    fn status(&self) -> SaveStatusSnapshot {
        SaveStatusSnapshot::default()
    }

    fn reconcile_open(&self) -> Result<parchmint_save::OpenReconciliation, SaveError> {
        Ok(parchmint_save::OpenReconciliation::default())
    }

    fn cancel_pending(&self, _ticket: SaveTicket) -> parchmint_save::CancelOutcome {
        parchmint_save::CancelOutcome::TooLate
    }
}

#[derive(Debug, Default)]
struct FailingSave;

impl SaveCoordinator for FailingSave {
    fn request(&self, _request: SaveRequest) -> Result<SaveTicket, SaveError> {
        Err(SaveError::WorkerStopped)
    }

    fn status(&self) -> SaveStatusSnapshot {
        SaveStatusSnapshot::default()
    }

    fn reconcile_open(&self) -> Result<parchmint_save::OpenReconciliation, SaveError> {
        Ok(parchmint_save::OpenReconciliation::default())
    }

    fn cancel_pending(&self, _ticket: SaveTicket) -> parchmint_save::CancelOutcome {
        parchmint_save::CancelOutcome::WorkerStopped
    }
}

impl SaveCoordinator for RecordingSave {
    fn request(&self, request: SaveRequest) -> Result<SaveTicket, SaveError> {
        self.requests.lock().unwrap().push(request);
        Ok(SaveTicket::pending(
            self.requests.lock().unwrap().len() as u64
        ))
    }

    fn status(&self) -> SaveStatusSnapshot {
        SaveStatusSnapshot::default()
    }

    fn reconcile_open(&self) -> Result<parchmint_save::OpenReconciliation, SaveError> {
        Ok(parchmint_save::OpenReconciliation::default())
    }

    fn cancel_pending(&self, _ticket: SaveTicket) -> parchmint_save::CancelOutcome {
        parchmint_save::CancelOutcome::Cancelled
    }
}

#[derive(Debug, Default)]
struct ProductionJournal {
    records: Mutex<Vec<RecoveryRecord>>,
}

impl RecoveryJournal for ProductionJournal {
    fn append(&self, batch: RecoveryBatch) -> Result<RecoveryReceipt, RecoveryError> {
        let mut records = self.records.lock().unwrap();
        batch.validate_after(records.last().and_then(|record| match record {
            RecoveryRecord::Complete(batch) => Some(batch),
            _ => None,
        }))?;
        let receipt = RecoveryReceipt::for_batch(&batch);
        records.push(RecoveryRecord::Complete(batch));
        Ok(receipt)
    }

    fn flush_through(
        &self,
        target: RecoveryRevisionVector,
    ) -> Result<RecoveryReceipt, RecoveryError> {
        self.records
            .lock()
            .unwrap()
            .iter()
            .find_map(|record| match record {
                RecoveryRecord::Complete(batch) if batch.revision_vector() == target => {
                    Some(Ok(RecoveryReceipt::for_batch(batch)))
                }
                _ => None,
            })
            .unwrap_or(Err(RecoveryError::UnknownRevisionVector))
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

fn document_id() -> DocumentId {
    DocumentId::from_bytes([44; 16])
}

fn hash(value: &str) -> ContentHash {
    ContentHash::from_bytes(Sha256::digest(value.as_bytes()).into())
}

fn save_vector(revision: u64) -> parchmint_save::SaveRevisionVector {
    parchmint_save::SaveRevisionVector {
        project_revision: parchmint_domain::ProjectRevision::default(),
        open_documents: BTreeMap::from([(
            document_id(),
            parchmint_recovery_api::DocumentRevision::from(revision),
        )]),
        closed_resources: BTreeMap::new(),
        canonical_hashes: BTreeMap::new(),
        generation: parchmint_save::SaveGeneration::from(revision),
    }
}

fn save_request(revision: u64) -> SaveRequest {
    let revisions = save_vector(revision);
    SaveRequest::new(
        revisions,
        AtomicWritePlan::new(Vec::new()),
        CheckpointInput {
            intent_hash: CheckpointIntentHash::from_bytes([revision as u8; 32]),
            resources: BTreeMap::new(),
            category: CheckpointCategory::Autosave,
            affected_documents: vec![document_id()],
            name: None,
        },
        SavePriority::Autosave,
    )
}

fn payload(body: &str) -> VersionedRecoveryPayload {
    VersionedRecoveryPayload::V1(RecoveryRecordV1 {
        schema: "parchmint.recovery-record/v1".into(),
        record_id: format!("application-{body}"),
        operations: vec![json!({ "body": body })],
    })
}

fn replacement() -> ReplacementSelection {
    ReplacementSelection {
        label: "Replace All".into(),
        edits: vec![
            ReplacementEdit {
                document_id: open_document(),
                observed_revision: EditorRevision::from(0),
                expected_body: "alpha needle".into(),
                replacement_body: "alpha replaced".into(),
            },
            ReplacementEdit {
                document_id: closed_document(),
                observed_revision: EditorRevision::from(0),
                expected_body: "closed needle".into(),
                replacement_body: "closed replaced".into(),
            },
        ],
    }
}

#[test]
fn focus_routes_to_exactly_one_undo_owner() {
    let document = open_document();
    assert_eq!(
        FocusTarget::Editor(document).undo_domain(),
        UndoDomain::Document(document)
    );
    assert_eq!(
        FocusTarget::Comment(document).undo_domain(),
        UndoDomain::Document(document)
    );
    for focus in [
        FocusTarget::Tree,
        FocusTarget::Cards,
        FocusTarget::Settings,
        FocusTarget::Inspector,
    ] {
        assert_eq!(focus.undo_domain(), UndoDomain::Project);
    }
    assert_eq!(FocusTarget::TextInput.undo_domain(), UndoDomain::TextInput);
}

#[test]
fn project_commands_undo_redo_with_new_revisions_checkpoints_and_redo_invalidation() {
    let (dispatcher, documents) = setup();
    let initial = dispatcher.project().unwrap().revision;
    let execute = wait(dispatcher.execute(ProjectCommand::rename_node(group_id(), "Final")))
        .expect("rename succeeds");
    assert_eq!(dispatcher.project_undo_entries().unwrap().len(), 1);
    assert_eq!(documents.document_undo_len(open_document()).unwrap(), 0);

    let undo = wait(dispatcher.undo()).expect("undo succeeds");
    assert!(dispatcher.undo_state().can_redo);
    let redo = wait(dispatcher.redo()).expect("redo succeeds");

    assert_eq!(execute.revision, initial.next());
    assert_eq!(undo.revision, execute.revision.next());
    assert_eq!(redo.revision, undo.revision.next());
    assert_eq!(dispatcher.pending_checkpoints().unwrap().len(), 3);
    assert_eq!(
        dispatcher
            .project()
            .unwrap()
            .nodes
            .get(group_id())
            .unwrap()
            .title,
        "Final"
    );

    wait(dispatcher.undo()).unwrap();
    wait(dispatcher.execute(ProjectCommand::rename_node(group_id(), "Published"))).unwrap();
    assert!(!dispatcher.undo_state().can_redo);
    assert_eq!(dispatcher.project_undo_entries().unwrap().len(), 1);
}

#[test]
fn create_document_publishes_tree_and_default_body_at_one_boundary() {
    let (dispatcher, documents) = setup();
    let node = parchmint_domain::NodeId::from_bytes(stable_id(31));
    let document = DocumentId::from_bytes(stable_id(32));

    let result = wait(dispatcher.execute(ProjectCommand::create_document(
        node,
        document,
        group_id(),
        0,
        "New Chapter",
    )))
    .expect("document creation succeeds");

    assert_eq!(
        dispatcher.project().unwrap().nodes.get(node).unwrap().kind,
        parchmint_domain::NodeKind::Document(document)
    );
    assert_eq!(documents.snapshot(document).unwrap().body, "<p></p>");
    assert_eq!(
        documents.snapshot(document).unwrap().visibility,
        DocumentVisibility::Open
    );
    assert!(result.dirty_resources.contains(Resource::Manifest));
}

#[test]
fn create_document_state_failure_keeps_the_project_tree_unchanged() {
    let (dispatcher, documents) = setup();
    let before = dispatcher.project().unwrap();
    let node = parchmint_domain::NodeId::from_bytes(stable_id(33));

    let result = wait(dispatcher.execute(ProjectCommand::create_document(
        node,
        open_document(),
        group_id(),
        0,
        "Conflicting Chapter",
    )));

    assert!(matches!(
        result,
        Err(ApplicationError::DuplicateDocument { document }) if document == open_document()
    ));
    assert_eq!(dispatcher.project().unwrap(), before);
    assert!(dispatcher.project_undo_entries().unwrap().is_empty());
    assert_eq!(documents.snapshots().unwrap().len(), 2);
}

fn persisted_project(
    title: &str,
    body: &str,
) -> (
    Project,
    Vec<DocumentSnapshot>,
    parchmint_project_format::CanonicalProjectEncoding,
) {
    let node = parchmint_domain::NodeId::from_bytes(stable_id(41));
    let document = DocumentId::from_bytes(stable_id(42));
    let project = Project::new(project_id());
    let project = parchmint_domain::apply_project_command(
        &project,
        project.revision,
        ProjectCommand::create_group(
            group_id(),
            parchmint_domain::NodeId::manuscript_root(),
            0,
            "Draft",
        ),
    )
    .unwrap()
    .project;
    let mut project = parchmint_domain::apply_project_command(
        &project,
        project.revision,
        ProjectCommand::create_document(node, document, group_id(), 0, "Chapter"),
    )
    .unwrap()
    .project;
    project.display_title = title.to_owned();
    let documents = vec![DocumentSnapshot {
        document_id: document,
        body: body.to_owned(),
        revision: EditorRevision::from(1),
        visibility: DocumentVisibility::Open,
    }];
    let encoding = parchmint_project_format::ProjectFormatCodec::default()
        .encode_domain_project_with_frontier(
            &project,
            &BTreeMap::from([(document, body.to_owned())]),
            &BTreeMap::new(),
            &Default::default(),
            &parchmint_project_format::CanonicalPersistenceFrontier {
                recovery_project_revision: 1,
                document_revisions: BTreeMap::from([(document, 1)]),
            },
        )
        .unwrap();
    (project, documents, encoding)
}

fn recovery_base_for(
    encoding: &parchmint_project_format::CanonicalProjectEncoding,
) -> RecoveryBaseSnapshot {
    RecoveryBaseSnapshot {
        revisions: RecoveryRevisionVector::new(
            parchmint_domain::ProjectRevision::from(
                encoding.persistence_frontier.recovery_project_revision,
            ),
            encoding
                .persistence_frontier
                .document_revisions
                .iter()
                .map(|(document, revision)| {
                    (
                        *document,
                        parchmint_recovery_api::DocumentRevision::from(*revision),
                    )
                })
                .collect(),
        ),
        hashes: encoding
            .resources
            .values()
            .map(|resource| (resource.resource.clone(), resource.hash))
            .collect(),
    }
}

fn restore_plan(
    encoding: &parchmint_project_format::CanonicalProjectEncoding,
) -> parchmint_history_api::RestorePlan {
    parchmint_history_api::RestorePlan::new(
        parchmint_domain::CheckpointId::from_bytes([77; 16]),
        encoding
            .resources
            .iter()
            .map(|(path, resource)| (path.clone(), resource.hash))
            .collect(),
        AtomicWritePlan::new(
            encoding
                .resources
                .values()
                .map(|resource| parchmint_project_repository::StagedResource {
                    path: resource.path.as_str().to_owned(),
                    bytes: resource.bytes.clone(),
                })
                .collect(),
        ),
    )
    .unwrap()
}

#[test]
fn history_restore_rehydrates_authoritative_state_after_restoration_checkpoint() {
    let (current_project, current_documents, current_encoding) =
        persisted_project("Current", "<p>current</p>");
    let (_, _, historical_encoding) = persisted_project("Historical", "<p>historical</p>");
    let documents = Arc::new(NativeDocumentStateOwner::new(current_documents));
    let commands = Arc::new(NativeProjectCommandDispatcher::new(
        current_project.clone(),
        documents.clone(),
    ));
    let save = Arc::new(CompletedSave::default());
    let editor = Arc::new(EditorPersistenceCoordinator::new(
        Arc::new(ProductionJournal::default()),
        save.clone(),
        recovery_base_for(&current_encoding),
    ));
    let coordinator = ProjectPersistenceCoordinator::new(
        commands.clone(),
        documents.clone(),
        editor,
        recovery_base_for(&current_encoding),
        current_encoding
            .resources
            .iter()
            .map(|(path, resource)| (path.clone(), resource.bytes.clone()))
            .collect(),
        current_encoding.paths,
    );

    let restored = coordinator
        .restore_history(restore_plan(&historical_encoding))
        .expect("History restore succeeds");

    assert_eq!(
        restored.source,
        parchmint_domain::CheckpointId::from_bytes([77; 16])
    );
    assert_eq!(commands.project().unwrap().display_title, "Historical");
    assert_eq!(
        commands.project().unwrap().revision,
        current_project.revision.next()
    );
    assert_eq!(documents.snapshots().unwrap()[0].body, "<p>historical</p>");
    assert!(!commands.undo_state().can_undo);
    let requests = save.requests.lock().unwrap();
    assert_eq!(requests.len(), 1);
    assert_eq!(
        requests[0].checkpoint.category,
        CheckpointCategory::Restoration
    );
}

#[test]
fn history_restore_save_failure_keeps_current_in_memory_project_open() {
    let (current_project, current_documents, current_encoding) =
        persisted_project("Current", "<p>current</p>");
    let (_, _, historical_encoding) = persisted_project("Historical", "<p>historical</p>");
    let documents = Arc::new(NativeDocumentStateOwner::new(current_documents));
    let commands = Arc::new(NativeProjectCommandDispatcher::new(
        current_project.clone(),
        documents.clone(),
    ));
    let editor = Arc::new(EditorPersistenceCoordinator::new(
        Arc::new(ProductionJournal::default()),
        Arc::new(FailingSave),
        recovery_base_for(&current_encoding),
    ));
    let coordinator = ProjectPersistenceCoordinator::new(
        commands.clone(),
        documents.clone(),
        editor,
        recovery_base_for(&current_encoding),
        current_encoding
            .resources
            .iter()
            .map(|(path, resource)| (path.clone(), resource.bytes.clone()))
            .collect(),
        current_encoding.paths,
    );

    assert!(
        coordinator
            .restore_history(restore_plan(&historical_encoding))
            .is_err()
    );
    assert_eq!(commands.project().unwrap(), current_project);
    assert_eq!(documents.snapshots().unwrap()[0].body, "<p>current</p>");
}

#[test]
fn prepared_group_duplicate_preserves_authored_subtree_with_fresh_ids_and_bodies() {
    use parchmint_domain::{
        MetadataApplicability, MetadataFieldDefinition, MetadataFieldId, MetadataTextKind, NodeId,
        ProjectExportSettings,
    };

    let field = MetadataFieldId::from_bytes(stable_id(51));
    let group = NodeId::from_bytes(stable_id(52));
    let node = NodeId::from_bytes(stable_id(53));
    let document = DocumentId::from_bytes(stable_id(54));
    let mut project = Project::new(project_id());
    for command in [
        ProjectCommand::upsert_metadata_field(MetadataFieldDefinition {
            id: field,
            label: "Status".into(),
            description: Some("Editorial status".into()),
            applicability: MetadataApplicability::GroupsAndDocuments,
            text_kind: MetadataTextKind::SingleLine,
            default_value: None,
            visible_on_cards: true,
        }),
        ProjectCommand::create_group(group, NodeId::manuscript_root(), 0, "Part One"),
        ProjectCommand::set_synopsis(group, "Opening movement"),
        ProjectCommand::set_metadata_value(group, field, Some("Draft".into())),
        ProjectCommand::set_node_export_settings(
            group,
            ProjectExportSettings {
                excluded: true,
                starts_new_page: true,
            },
        ),
        ProjectCommand::create_document(node, document, group, 0, "Chapter One"),
        ProjectCommand::set_metadata_value(node, field, Some("Revised".into())),
    ] {
        project = parchmint_domain::apply_project_command(&project, project.revision, command)
            .unwrap()
            .project;
    }
    let body = "<p data-style-id=\"document-title\">Chapter One</p><p>Body</p>";
    let prepared = project_persistence::prepare_duplicate(
        &project,
        &[DocumentSnapshot {
            document_id: document,
            body: body.into(),
            revision: EditorRevision::from(7),
            visibility: DocumentVisibility::Open,
        }],
        &DuplicateSubtreeWorkflow {
            source: group,
            parent: NodeId::research_root(),
            index: 0,
        },
    )
    .expect("group subtree can be prepared");

    let copied_group = prepared.project.nodes.get(prepared.created_root).unwrap();
    assert_ne!(prepared.created_root, group);
    assert_eq!(copied_group.title, "Part One");
    assert_eq!(copied_group.synopsis, "Opening movement");
    assert_eq!(copied_group.metadata[&field], "Draft");
    assert!(copied_group.export_settings.excluded);
    assert!(copied_group.export_settings.starts_new_page);
    let copied_node = prepared.node_ids[&node];
    let copied_document = prepared.document_ids[&document];
    assert_ne!(copied_node, node);
    assert_ne!(copied_document, document);
    assert_eq!(
        prepared.project.nodes.get(copied_node).unwrap().title,
        "Chapter One"
    );
    assert_eq!(
        prepared
            .documents
            .iter()
            .find(|snapshot| snapshot.document_id == copied_document)
            .unwrap()
            .body,
        body
    );
}

#[test]
fn duplicate_save_failure_publishes_no_partial_project_or_document_state() {
    let (current_project, current_documents, current_encoding) =
        persisted_project("Current", "<p>current</p>");
    let documents = Arc::new(NativeDocumentStateOwner::new(current_documents));
    let commands = Arc::new(NativeProjectCommandDispatcher::new(
        current_project.clone(),
        documents.clone(),
    ));
    let editor = Arc::new(EditorPersistenceCoordinator::new(
        Arc::new(ProductionJournal::default()),
        Arc::new(FailingSave),
        recovery_base_for(&current_encoding),
    ));
    let coordinator = ProjectPersistenceCoordinator::new(
        commands.clone(),
        documents.clone(),
        editor,
        recovery_base_for(&current_encoding),
        current_encoding
            .resources
            .iter()
            .map(|(path, resource)| (path.clone(), resource.bytes.clone()))
            .collect(),
        current_encoding.paths,
    );
    let source = *current_project.nodes.children(group_id()).first().unwrap();

    assert!(
        coordinator
            .duplicate_subtree(DuplicateSubtreeWorkflow {
                source,
                parent: group_id(),
                index: 1,
            })
            .is_err()
    );
    assert_eq!(commands.project().unwrap(), current_project);
    assert_eq!(documents.snapshots().unwrap().len(), 1);
    assert_eq!(documents.snapshots().unwrap()[0].body, "<p>current</p>");
}

#[test]
fn duplicate_does_not_create_an_annotation_sidecar_for_the_fresh_document() {
    let (current_project, current_documents, mut current_encoding) =
        persisted_project("Current", "<p>current</p>");
    let source = *current_project.nodes.children(group_id()).first().unwrap();
    let source_document = match current_project.nodes.get(source).unwrap().kind {
        parchmint_domain::NodeKind::Document(document) => document,
        _ => unreachable!(),
    };
    let document_text = project_persistence::stable_id_text(source_document.as_bytes());
    let annotation_path = parchmint_project_format::CanonicalRelativePath::parse(format!(
        "annotations/{document_text}.json"
    ))
    .unwrap();
    let annotation_bytes = format!(
        "{{\"document_id\":\"{document_text}\",\"schema\":\"parchmint.annotation-sidecar/v1\",\"threads\":[]}}\n"
    )
    .into_bytes();
    current_encoding.resources.insert(
        annotation_path.clone(),
        parchmint_project_format::CanonicalBytes {
            resource: parchmint_project_format::ResourceId::Annotations {
                document_id: document_text.clone(),
            },
            path: annotation_path,
            hash: ContentHash::from_bytes(Sha256::digest(&annotation_bytes).into()),
            bytes: annotation_bytes,
        },
    );
    let documents = Arc::new(NativeDocumentStateOwner::new(current_documents));
    let commands = Arc::new(NativeProjectCommandDispatcher::new(
        current_project,
        documents.clone(),
    ));
    let save = Arc::new(CompletedSave::default());
    let editor = Arc::new(EditorPersistenceCoordinator::new(
        Arc::new(ProductionJournal::default()),
        save.clone(),
        recovery_base_for(&current_encoding),
    ));
    let coordinator = ProjectPersistenceCoordinator::new(
        commands,
        documents,
        editor,
        recovery_base_for(&current_encoding),
        current_encoding
            .resources
            .iter()
            .map(|(path, resource)| (path.clone(), resource.bytes.clone()))
            .collect(),
        current_encoding.paths,
    );

    let result = coordinator
        .duplicate_subtree(DuplicateSubtreeWorkflow {
            source,
            parent: group_id(),
            index: 1,
        })
        .expect("duplicate is durable");
    let fresh_document = result.document_ids[&source_document];
    let fresh_text = project_persistence::stable_id_text(fresh_document.as_bytes());
    let requests = save.requests.lock().unwrap();
    assert!(
        requests[0]
            .writes
            .writes
            .iter()
            .any(|write| write.path == format!("annotations/{document_text}.json"))
    );
    assert!(
        !requests[0]
            .writes
            .writes
            .iter()
            .any(|write| write.path == format!("annotations/{fresh_text}.json"))
    );
}

#[test]
fn move_workflow_uses_domain_move_validation_and_a_structural_save() {
    let (current_project, current_documents, current_encoding) =
        persisted_project("Current", "<p>current</p>");
    let source = *current_project.nodes.children(group_id()).first().unwrap();
    let documents = Arc::new(NativeDocumentStateOwner::new(current_documents));
    let commands = Arc::new(NativeProjectCommandDispatcher::new(
        current_project,
        documents.clone(),
    ));
    let save = Arc::new(CompletedSave::default());
    let editor = Arc::new(EditorPersistenceCoordinator::new(
        Arc::new(ProductionJournal::default()),
        save.clone(),
        recovery_base_for(&current_encoding),
    ));
    let coordinator = ProjectPersistenceCoordinator::new(
        commands.clone(),
        documents,
        editor,
        recovery_base_for(&current_encoding),
        current_encoding
            .resources
            .iter()
            .map(|(path, resource)| (path.clone(), resource.bytes.clone()))
            .collect(),
        current_encoding.paths,
    );

    coordinator
        .move_nodes(MoveNodesWorkflow {
            moves: vec![MoveNodeWorkflow {
                node: source,
                parent: parchmint_domain::NodeId::research_root(),
                index: 0,
            }],
        })
        .expect("move saves structurally");
    assert_eq!(
        commands.project().unwrap().nodes.parent(source),
        Some(parchmint_domain::NodeId::research_root())
    );
    assert_eq!(
        save.requests.lock().unwrap()[0].checkpoint.category,
        CheckpointCategory::StructuralChange
    );

    let before_invalid = commands.project().unwrap();
    assert!(
        coordinator
            .move_nodes(MoveNodesWorkflow {
                moves: vec![MoveNodeWorkflow {
                    node: source,
                    parent: source,
                    index: 0,
                }],
            })
            .is_err()
    );
    assert_eq!(commands.project().unwrap(), before_invalid);
    assert_eq!(save.requests.lock().unwrap().len(), 1);
}

#[test]
fn unopened_document_commands_create_a_hidden_session_and_use_document_undo() {
    let (dispatcher, documents) = setup();
    let result = dispatcher
        .execute_document(DocumentCommand {
            document_id: closed_document(),
            observed_revision: EditorRevision::from(0),
            body: "edited while unopened".into(),
        })
        .unwrap();
    assert!(result.opened_session);
    assert_eq!(documents.document_undo_len(closed_document()).unwrap(), 1);
    assert_eq!(
        documents.snapshot(closed_document()).unwrap().visibility,
        DocumentVisibility::Hidden
    );
    let save = dispatcher.capture_save_request().unwrap();
    assert_eq!(save.open_documents[&closed_document()].value(), 1);
    assert!(!save.closed_documents.contains_key(&closed_document()));

    let undo = dispatcher
        .undo_focused(FocusTarget::Editor(closed_document()))
        .unwrap();
    assert!(matches!(undo, FocusedUndoResult::Document { .. }));
    assert_eq!(
        documents.snapshot(closed_document()).unwrap().body,
        "closed needle"
    );
    dispatcher
        .redo_focused(FocusTarget::Comment(closed_document()))
        .unwrap();
    assert_eq!(
        documents.snapshot(closed_document()).unwrap().body,
        "edited while unopened"
    );
    assert_eq!(
        dispatcher.undo_focused(FocusTarget::TextInput).unwrap(),
        FocusedUndoResult::NativeTextInput
    );
}

#[test]
fn save_requests_capture_open_and_closed_revisions_at_one_boundary() {
    let (dispatcher, documents) = setup();
    wait(GlobalReplacement::apply(
        &dispatcher,
        ReplacementSelection {
            label: "Closed replacement".into(),
            edits: vec![ReplacementEdit {
                document_id: closed_document(),
                observed_revision: EditorRevision::from(0),
                expected_body: "closed needle".into(),
                replacement_body: "closed changed".into(),
            }],
        },
    ))
    .unwrap();
    dispatcher
        .execute_document(DocumentCommand {
            document_id: open_document(),
            observed_revision: EditorRevision::from(0),
            body: "open changed".into(),
        })
        .unwrap();

    let captured = dispatcher.capture_save_request().unwrap();
    dispatcher
        .execute_document(DocumentCommand {
            document_id: open_document(),
            observed_revision: EditorRevision::from(1),
            body: "open changed again".into(),
        })
        .unwrap();
    let current = dispatcher.capture_save_request().unwrap();

    assert_eq!(captured.generation, 1);
    assert_eq!(captured.open_documents[&open_document()].value(), 1);
    assert_eq!(captured.closed_documents[&closed_document()].value(), 1);
    assert_eq!(current.generation, 2);
    assert_eq!(current.open_documents[&open_document()].value(), 2);
    assert_eq!(
        documents
            .snapshot(open_document())
            .unwrap()
            .revision
            .value(),
        2
    );
    assert_eq!(captured.checkpoint_groups.len(), 2);
}

#[test]
fn save_acknowledgement_retires_only_the_exact_captured_dirty_frontier() {
    let (dispatcher, _) = setup();
    dispatcher
        .execute_document(DocumentCommand {
            document_id: open_document(),
            observed_revision: EditorRevision::from(0),
            body: "revision one".into(),
        })
        .unwrap();
    let revision_one = dispatcher.capture_save_request().unwrap();
    dispatcher
        .execute_document(DocumentCommand {
            document_id: open_document(),
            observed_revision: EditorRevision::from(1),
            body: "revision two".into(),
        })
        .unwrap();
    let revision_two = dispatcher.capture_save_request().unwrap();

    let after_one = dispatcher.acknowledge_save(&revision_one).unwrap();
    assert!(after_one.contains(Resource::Document(open_document())));
    assert_eq!(dispatcher.pending_checkpoints().unwrap().len(), 1);
    assert!(matches!(
        dispatcher.acknowledge_save(&revision_one),
        Err(ApplicationError::StaleSaveAcknowledgement)
    ));

    let after_two = dispatcher.acknowledge_save(&revision_two).unwrap();
    assert!(after_two.iter().next().is_none());
    assert!(dispatcher.pending_checkpoints().unwrap().is_empty());
}

#[test]
fn global_replacement_has_one_inverse_project_undo_and_checkpoint() {
    let (dispatcher, documents) = setup();
    let initial = dispatcher.project().unwrap().revision;
    let preview = wait(GlobalReplacement::preview(&dispatcher, replacement())).unwrap();
    let result = wait(GlobalReplacement::apply(&dispatcher, replacement())).unwrap();
    let entries = dispatcher.project_undo_entries().unwrap();

    assert_eq!(preview.affected_documents.len(), 2);
    assert_eq!(entries.len(), 1);
    assert!(matches!(
        entries[0].forward,
        ProjectPatch::Documents {
            direction: PatchDirection::Forward,
            ..
        }
    ));
    assert!(matches!(
        entries[0].inverse,
        ProjectPatch::Documents {
            direction: PatchDirection::Inverse,
            ..
        }
    ));
    assert_eq!(entries[0].checkpoint_group, result.checkpoint_group);
    assert_eq!(
        dispatcher.pending_checkpoints().unwrap(),
        vec![result.checkpoint_group]
    );
    assert_eq!(documents.document_undo_len(open_document()).unwrap(), 0);
    assert_eq!(documents.document_undo_len(closed_document()).unwrap(), 0);
    assert_eq!(
        documents.project_boundary_count(open_document()).unwrap(),
        1
    );
    assert_eq!(
        documents.project_boundary_count(closed_document()).unwrap(),
        1
    );

    assert_eq!(result.revision, initial.next());
    let undo = wait(dispatcher.undo()).unwrap();
    assert_eq!(undo.revision, result.revision.next());
    assert_eq!(
        documents.snapshot(open_document()).unwrap().body,
        "alpha needle"
    );
    assert_eq!(
        documents.snapshot(closed_document()).unwrap().body,
        "closed needle"
    );
    let redo = wait(dispatcher.redo()).unwrap();
    assert_eq!(redo.revision, undo.revision.next());
    assert_eq!(
        documents.snapshot(open_document()).unwrap().body,
        "alpha replaced"
    );
    assert_eq!(
        documents.snapshot(closed_document()).unwrap().body,
        "closed replaced"
    );
    assert_eq!(
        documents.project_boundary_count(open_document()).unwrap(),
        3
    );
    assert_eq!(dispatcher.pending_checkpoints().unwrap().len(), 3);
}

#[test]
fn composite_apply_failure_rolls_back_open_and_closed_documents_before_publish() {
    let (dispatcher, documents) = setup();
    let project_before = dispatcher.project().unwrap();
    let open_before = documents.snapshot(open_document()).unwrap();
    let closed_before = documents.snapshot(closed_document()).unwrap();
    documents.fail_next_composite_at(closed_document());

    let result = wait(GlobalReplacement::apply(&dispatcher, replacement()));

    assert!(matches!(
        result,
        Err(ApplicationError::CompositeApplyFailed { document })
            if document == closed_document()
    ));
    assert_eq!(documents.snapshot(open_document()).unwrap(), open_before);
    assert_eq!(
        documents.snapshot(closed_document()).unwrap(),
        closed_before
    );
    assert_eq!(dispatcher.project().unwrap(), project_before);
    assert!(dispatcher.project_undo_entries().unwrap().is_empty());
    assert!(dispatcher.pending_checkpoints().unwrap().is_empty());
    assert_eq!(
        documents.project_boundary_count(open_document()).unwrap(),
        0
    );
    assert_eq!(
        documents.project_boundary_count(closed_document()).unwrap(),
        0
    );
}

#[test]
fn composite_validation_prepares_every_inverse_before_any_publication() {
    let (dispatcher, documents) = setup();
    let project_before = dispatcher.project().unwrap();
    let open_before = documents.snapshot(open_document()).unwrap();
    let selection = ReplacementSelection {
        label: "Invalid replacement".into(),
        edits: vec![
            replacement().edits[0].clone(),
            ReplacementEdit {
                document_id: DocumentId::from_bytes(stable_id(99)),
                observed_revision: EditorRevision::from(0),
                expected_body: "missing".into(),
                replacement_body: "invalid".into(),
            },
        ],
    };

    assert!(matches!(
        wait(GlobalReplacement::apply(&dispatcher, selection)),
        Err(ApplicationError::MissingDocument { .. })
    ));
    let duplicate = replacement().edits[0].clone();
    assert!(matches!(
        wait(GlobalReplacement::apply(
            &dispatcher,
            ReplacementSelection {
                label: "Duplicate replacement".into(),
                edits: vec![duplicate.clone(), duplicate],
            }
        )),
        Err(ApplicationError::DuplicateDocument { document }) if document == open_document()
    ));
    assert_eq!(documents.snapshot(open_document()).unwrap(), open_before);
    assert_eq!(dispatcher.project().unwrap(), project_before);
    assert!(dispatcher.project_undo_entries().unwrap().is_empty());
    assert!(dispatcher.pending_checkpoints().unwrap().is_empty());
    assert_eq!(
        documents.project_boundary_count(open_document()).unwrap(),
        0
    );
}

#[test]
fn recovery_migration_restore_and_close_reset_both_undo_owners() {
    for reason in [
        UndoResetReason::RecoveryAccepted,
        UndoResetReason::MigrationCompleted,
        UndoResetReason::HistoryRestored,
        UndoResetReason::ProjectClosed,
    ] {
        let (dispatcher, documents) = setup();
        wait(dispatcher.execute(ProjectCommand::rename_node(group_id(), "Changed"))).unwrap();
        dispatcher
            .execute_document(DocumentCommand {
                document_id: open_document(),
                observed_revision: EditorRevision::from(0),
                body: "changed".into(),
            })
            .unwrap();
        wait(dispatcher.undo()).unwrap();
        dispatcher
            .undo_focused(FocusTarget::Editor(open_document()))
            .unwrap();

        dispatcher.reset_undo(reason);

        assert!(!dispatcher.undo_state().can_undo, "{reason:?}");
        assert!(!dispatcher.undo_state().can_redo, "{reason:?}");
        assert_eq!(documents.document_undo_len(open_document()).unwrap(), 0);
        assert!(matches!(
            dispatcher.undo_focused(FocusTarget::Editor(open_document())),
            Err(ApplicationError::DocumentUndoEmpty { .. })
        ));
        assert!(matches!(
            dispatcher.redo_focused(FocusTarget::Editor(open_document())),
            Err(ApplicationError::DocumentRedoEmpty { .. })
        ));
    }
}
