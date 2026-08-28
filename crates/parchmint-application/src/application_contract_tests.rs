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
    BlockId, CanonicalComment, CanonicalProjection, CommentId, DocumentId, DurableProjectionBatch,
    EditorPersistenceError, EditorRevision, EditorSelection,
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

struct CountingDocumentLoader {
    reads: Arc<AtomicU64>,
}

impl DocumentSnapshotLoader for CountingDocumentLoader {
    fn load(&self, document: DocumentId) -> Result<DocumentSnapshot, ApplicationError> {
        self.reads.fetch_add(1, Ordering::SeqCst);
        Ok(DocumentSnapshot {
            document_id: document,
            body: format!("body-{:02x}", document.as_bytes()[0]),
            comments: Vec::new(),
            revision: EditorRevision::from(7),
            visibility: DocumentVisibility::Closed,
        })
    }
}

#[test]
fn lazy_document_owner_reads_only_the_selected_body_on_demand() {
    let reads = Arc::new(AtomicU64::new(0));
    let summaries = (1..=64).map(|value| LazyDocumentSummary {
        document_id: DocumentId::from_bytes([value; 16]),
        revision: EditorRevision::from(7),
        visibility: if value == 1 {
            DocumentVisibility::Open
        } else {
            DocumentVisibility::Closed
        },
    });
    let owner = NativeDocumentStateOwner::new_lazy(
        summaries,
        Arc::new(CountingDocumentLoader {
            reads: reads.clone(),
        }),
    )
    .expect("unique summaries should initialize");

    assert!(owner.loaded_snapshots().unwrap().is_empty());
    assert_eq!(owner.summaries().unwrap().len(), 64);
    assert_eq!(reads.load(Ordering::SeqCst), 0);

    let selected = DocumentId::from_bytes([32; 16]);
    assert_eq!(owner.snapshot(selected).unwrap().body, "body-20");
    assert_eq!(reads.load(Ordering::SeqCst), 1);
    assert_eq!(owner.loaded_snapshots().unwrap().len(), 1);
    owner.snapshot(selected).unwrap();
    assert_eq!(reads.load(Ordering::SeqCst), 1);
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
    .expect("sample group is valid")
    .project;
    let project = parchmint_domain::apply_project_command(
        &project,
        project.revision,
        ProjectCommand::create_document(
            parchmint_domain::NodeId::from_bytes(stable_id(5)),
            open_document(),
            group_id(),
            0,
            "Open",
        ),
    )
    .expect("sample open document is valid")
    .project;
    parchmint_domain::apply_project_command(
        &project,
        project.revision,
        ProjectCommand::create_document(
            parchmint_domain::NodeId::from_bytes(stable_id(6)),
            closed_document(),
            group_id(),
            1,
            "Closed",
        ),
    )
    .expect("sample closed document is valid")
    .project
}

fn sample_documents() -> Arc<NativeDocumentStateOwner> {
    Arc::new(NativeDocumentStateOwner::new([
        DocumentSnapshot {
            document_id: open_document(),
            body: "alpha needle".into(),
            comments: Vec::new(),
            revision: EditorRevision::from(0),
            visibility: DocumentVisibility::Open,
        },
        DocumentSnapshot {
            document_id: closed_document(),
            body: "closed needle".into(),
            comments: Vec::new(),
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
fn editor_persistence_constructors_start_with_empty_status_and_queue() {
    let base = RecoveryBaseSnapshot {
        revisions: RecoveryRevisionVector::new(
            parchmint_domain::ProjectRevision::default(),
            BTreeMap::new(),
        ),
        hashes: BTreeMap::from([(ResourceId::Document, hash("base"))]),
    };
    let recovery_only = EditorPersistenceCoordinator::new_recovery_only(
        Arc::new(ProductionJournal::default()),
        base.clone(),
    );
    let with_save = EditorPersistenceCoordinator::new(
        Arc::new(ProductionJournal::default()),
        Arc::new(RecordingSave::default()),
        base,
    );

    for coordinator in [&recovery_only, &with_save] {
        assert_eq!(coordinator.status(), EditorPersistenceStatus::default());
        assert_eq!(coordinator.save_queue_depth(), 0);
        assert_eq!(coordinator.max_save_queue_depth(), 0);
        assert_eq!(coordinator.submitted_save_requests(), 0);
        assert_eq!(coordinator.coalesced_save_requests(), 0);
    }
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
fn production_editor_coordinator_resumes_the_exact_unacknowledged_batch() {
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
    let receipt = unacknowledged.receipt().clone();

    let reopened = EditorPersistenceCoordinator::new_recovery_only(journal, base.clone());
    let resumed = reopened
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
        receipt.durable_through,
        replay.accepted[1].revision_vector()
    );
    assert_eq!(resumed, replay.accepted[1].revision_vector());
    assert_eq!(reopened.frontier().unwrap(), resumed);
}

#[test]
fn production_editor_coordinator_keeps_document_recovery_hashes_distinct() {
    let journal = Arc::new(ProductionJournal::default());
    let first = document_id();
    let second = DocumentId::from_bytes([2; 16]);
    let first_resource = recovery_document_resource_id(first);
    let second_resource = recovery_document_resource_id(second);
    let base = RecoveryBaseSnapshot {
        revisions: RecoveryRevisionVector::new(
            parchmint_domain::ProjectRevision::default(),
            BTreeMap::new(),
        ),
        hashes: BTreeMap::from([
            (first_resource.clone(), hash("first base")),
            (second_resource.clone(), hash("second base")),
        ]),
    };
    let coordinator = EditorPersistenceCoordinator::new_recovery_only(journal.clone(), base);
    let persist = |document, revision, body, generation| {
        let projection = CanonicalProjection::new(
            document,
            EditorRevision::from(revision),
            body,
            vec![],
            vec![],
            2,
        );
        let revisions = parchmint_save::SaveRevisionVector {
            project_revision: parchmint_domain::ProjectRevision::default(),
            open_documents: BTreeMap::from([(
                document,
                parchmint_recovery_api::DocumentRevision::from(revision),
            )]),
            closed_resources: BTreeMap::new(),
            canonical_hashes: BTreeMap::new(),
            generation: parchmint_save::SaveGeneration::from(generation),
        };
        coordinator
            .persist_projection(&projection, &revisions, payload(body))
            .unwrap()
    };
    coordinator
        .acknowledge_recovery(persist(first, 1, "first edit", 1))
        .unwrap();
    coordinator
        .acknowledge_recovery(persist(second, 1, "second edit", 2))
        .unwrap();

    let records = journal.records.lock().unwrap();
    let RecoveryRecord::Complete(first_batch) = &records[0] else {
        panic!("production journal records complete batches")
    };
    let RecoveryRecord::Complete(second_batch) = &records[1] else {
        panic!("production journal records complete batches")
    };
    assert_eq!(first_batch.base_hashes.len(), 1);
    assert_eq!(second_batch.base_hashes.len(), 1);
    assert!(first_batch.base_hashes.contains_key(&first_resource));
    assert!(second_batch.base_hashes.contains_key(&second_resource));
    assert_ne!(
        first_batch.result_hashes[&first_resource],
        second_batch.result_hashes[&second_resource]
    );
}

#[test]
fn production_editor_coordinator_merges_partial_document_frontiers() {
    let journal = Arc::new(ProductionJournal::default());
    let first = document_id();
    let second = DocumentId::from_bytes([2; 16]);
    let base = RecoveryBaseSnapshot {
        revisions: RecoveryRevisionVector::new(
            parchmint_domain::ProjectRevision::default(),
            BTreeMap::new(),
        ),
        hashes: BTreeMap::from([
            (recovery_document_resource_id(first), hash("first base")),
            (recovery_document_resource_id(second), hash("second base")),
        ]),
    };
    let coordinator =
        EditorPersistenceCoordinator::new_recovery_only(journal.clone(), base.clone());
    let persist = |document, revision, body, generation| {
        let projection = CanonicalProjection::new(
            document,
            EditorRevision::from(revision),
            body,
            vec![],
            vec![],
            1,
        );
        let revisions = parchmint_save::SaveRevisionVector {
            project_revision: parchmint_domain::ProjectRevision::default(),
            open_documents: BTreeMap::from([(
                document,
                parchmint_recovery_api::DocumentRevision::from(revision),
            )]),
            closed_resources: BTreeMap::new(),
            canonical_hashes: BTreeMap::new(),
            generation: parchmint_save::SaveGeneration::from(generation),
        };
        coordinator
            .persist_projection(&projection, &revisions, payload(body))
            .unwrap()
    };
    coordinator
        .acknowledge_recovery(persist(first, 1, "first one", 1))
        .unwrap();
    coordinator
        .acknowledge_recovery(persist(second, 7, "second seven", 2))
        .unwrap();
    let third = persist(first, 2, "first two", 3);
    assert_eq!(
        third.batch().documents[&first].first,
        parchmint_recovery_api::DocumentRevision::from(2)
    );
    coordinator.acknowledge_recovery(third).unwrap();

    let expected = RecoveryRevisionVector::new(
        parchmint_domain::ProjectRevision::from(3),
        BTreeMap::from([
            (first, parchmint_recovery_api::DocumentRevision::from(2)),
            (second, parchmint_recovery_api::DocumentRevision::from(7)),
        ]),
    );
    assert_eq!(coordinator.frontier().unwrap(), expected);
    let reopened = EditorPersistenceCoordinator::new_recovery_only(journal, base.clone());
    let replay = reopened.reconcile_recovery(base).unwrap();
    assert_eq!(replay.accepted.len(), 3);
    assert!(replay.isolated.is_empty());
    assert_eq!(reopened.frontier().unwrap(), expected);
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
        durable: parchmint_recovery_api::DurableRevisionVector,
    ) -> Result<DiscardReport, RecoveryError> {
        let mut records = self.records.lock().unwrap();
        let before = records.len();
        records.retain(|record| match record {
            RecoveryRecord::Complete(batch) => {
                batch.project_revision > durable.revisions.project_revision
                    || batch.documents.iter().any(|(document, range)| {
                        durable.revisions.documents.get(document) < Some(&range.last)
                    })
            }
            _ => true,
        });
        Ok(DiscardReport {
            removed_records: before - records.len(),
            retained_records: records.len(),
        })
    }
}

fn document_id() -> DocumentId {
    DocumentId::from_bytes([44; 16])
}

fn recovery_document_resource_id(document: DocumentId) -> ResourceId {
    let document_id = document
        .as_bytes()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    ResourceId::DocumentById { document_id }
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

    assert!(
        result.is_err(),
        "duplicate document identity must be rejected"
    );
    assert_eq!(dispatcher.project().unwrap(), before);
    assert!(dispatcher.project_undo_entries().unwrap().is_empty());
    assert_eq!(documents.snapshots().unwrap().len(), 2);
}

fn assert_authoritative_snapshot_invariants(dispatcher: &NativeProjectCommandDispatcher) {
    let snapshot = dispatcher
        .authored_snapshot()
        .expect("authoritative snapshot should be available");
    let live_documents = snapshot
        .project
        .nodes
        .iter()
        .filter_map(|(_, node)| match node.kind {
            parchmint_domain::NodeKind::Document(document) => Some(document),
            parchmint_domain::NodeKind::Root(_) | parchmint_domain::NodeKind::Group => None,
        })
        .collect::<std::collections::BTreeSet<_>>();
    let summarized = snapshot
        .document_summaries
        .iter()
        .map(|summary| summary.document_id)
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(summarized, live_documents);
    for document in &snapshot.documents {
        assert!(live_documents.contains(&document.document_id));
        let summary = snapshot
            .document_summaries
            .iter()
            .find(|summary| summary.document_id == document.document_id)
            .expect("loaded document has one summary");
        assert_eq!(summary.revision, document.revision);
        assert_eq!(summary.visibility, document.visibility);
    }
}

#[test]
fn authoritative_snapshot_stays_coherent_across_project_operations() {
    let documents = Arc::new(NativeDocumentStateOwner::new([]));
    let dispatcher = NativeProjectCommandDispatcher::new(Project::new(project_id()), documents);
    let group = parchmint_domain::NodeId::from_bytes(stable_id(34));
    let node = parchmint_domain::NodeId::from_bytes(stable_id(35));
    let document = DocumentId::from_bytes(stable_id(36));

    assert_authoritative_snapshot_invariants(&dispatcher);
    wait(dispatcher.execute(ProjectCommand::create_group(
        group,
        parchmint_domain::NodeId::manuscript_root(),
        0,
        "Draft",
    )))
    .unwrap();
    assert_authoritative_snapshot_invariants(&dispatcher);
    wait(dispatcher.execute(ProjectCommand::create_document(
        node, document, group, 0, "Chapter",
    )))
    .unwrap();
    assert_authoritative_snapshot_invariants(&dispatcher);
    dispatcher
        .execute_document(DocumentCommand {
            document_id: document,
            observed_revision: EditorRevision::default(),
            body: "authored body".into(),
        })
        .unwrap();
    assert_authoritative_snapshot_invariants(&dispatcher);
    wait(dispatcher.execute(ProjectCommand::move_node(
        node,
        parchmint_domain::NodeId::research_root(),
        0,
    )))
    .unwrap();
    assert_authoritative_snapshot_invariants(&dispatcher);
    wait(dispatcher.execute(ProjectCommand::delete_node(node))).unwrap();
    let deleted = dispatcher.authored_snapshot().unwrap();
    assert!(deleted.document_summaries.is_empty());
    assert!(deleted.documents.is_empty());
    assert_authoritative_snapshot_invariants(&dispatcher);
    wait(dispatcher.undo()).unwrap();
    assert_authoritative_snapshot_invariants(&dispatcher);
    wait(dispatcher.redo()).unwrap();
    assert_authoritative_snapshot_invariants(&dispatcher);
}

#[test]
fn unchanged_operations_create_neither_dirty_state_nor_checkpoint_groups() {
    let (dispatcher, _) = setup();
    let revision = dispatcher.project().unwrap().revision;

    let rename = wait(dispatcher.execute(ProjectCommand::rename_node(group_id(), "Draft")))
        .expect("same-title rename is accepted as unchanged");
    assert_eq!(rename.revision, revision);
    assert_eq!(rename.events, [ProjectEvent::Unchanged]);
    assert_eq!(rename.checkpoint_group, None);

    let document = dispatcher
        .execute_document(DocumentCommand {
            document_id: open_document(),
            observed_revision: EditorRevision::default(),
            body: "alpha needle".into(),
        })
        .expect("same document body is accepted as unchanged");
    assert_eq!(document.revision, EditorRevision::default());
    dispatcher
        .accept_editor_projection(&CanonicalProjection::new(
            open_document(),
            EditorRevision::default(),
            "alpha needle",
            Vec::new(),
            Vec::new(),
            0,
        ))
        .expect("identical projection is accepted as unchanged");
    let replacement = wait(dispatcher.apply(ReplacementSelection {
        label: "No replacement".into(),
        edits: vec![ReplacementEdit {
            document_id: open_document(),
            observed_revision: EditorRevision::default(),
            expected_body: "alpha needle".into(),
            replacement_body: "alpha needle".into(),
        }],
    }))
    .expect("identity replacement is accepted as unchanged");
    assert_eq!(replacement.events, [ProjectEvent::Unchanged]);
    assert_eq!(replacement.checkpoint_group, None);

    assert!(!dispatcher.has_unsaved_changes().unwrap());
    assert!(dispatcher.pending_checkpoints().unwrap().is_empty());
    assert!(dispatcher.project_undo_entries().unwrap().is_empty());
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
        comments: Vec::new(),
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
                ..Default::default()
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

#[test]
fn durable_delete_points_reopened_tombstone_at_exact_pre_delete_checkpoint() {
    use parchmint_project_format::ProjectFormatCodec;

    let (project, documents, encoding) = persisted_project("Current", "<p>deleted body</p>");
    let deleted_node = project
        .nodes
        .iter()
        .find_map(|(id, node)| {
            matches!(node.kind, parchmint_domain::NodeKind::Document(_)).then_some(*id)
        })
        .expect("fixture document node");
    let owner = Arc::new(NativeDocumentStateOwner::new(documents));
    let commands = Arc::new(NativeProjectCommandDispatcher::new(project, owner.clone()));
    let save = Arc::new(CompletedSave::default());
    let editor = Arc::new(EditorPersistenceCoordinator::new(
        Arc::new(ProductionJournal::default()),
        save.clone(),
        recovery_base_for(&encoding),
    ));
    let coordinator = ProjectPersistenceCoordinator::new(
        commands.clone(),
        owner,
        editor,
        recovery_base_for(&encoding),
        encoding
            .resources
            .iter()
            .map(|(path, resource)| (path.clone(), resource.bytes.clone()))
            .collect(),
        encoding.paths,
    );

    let deleted = coordinator
        .delete_subtrees(DeleteSubtreesWorkflow {
            nodes: vec![deleted_node],
            deleted_at_unix_millis: 42,
        })
        .expect("delete workflow should save both sides of deletion");
    assert_eq!(
        deleted.restoring_checkpoint,
        parchmint_domain::CheckpointId::from_bytes([1; 16])
    );
    let tombstone = commands.project().unwrap().deleted[&deleted_node].clone();
    assert_eq!(
        tombstone.restoring_checkpoint,
        Some(deleted.restoring_checkpoint)
    );

    let requests = save.requests.lock().unwrap();
    assert_eq!(requests.len(), 2);
    assert!(
        requests[0]
            .writes
            .writes
            .iter()
            .any(|write| write.path == "project.toml"),
        "the clean pre-delete state must still become a durable checkpoint"
    );
    assert!(
        requests[0]
            .checkpoint
            .resources
            .keys()
            .any(|path| path.as_str().ends_with(".html"))
    );
    assert!(
        !requests[1]
            .checkpoint
            .resources
            .keys()
            .any(|path| path.as_str().ends_with(".html"))
    );
    let manifest_bytes = requests[1]
        .writes
        .writes
        .iter()
        .find(|write| write.path == "project.toml")
        .expect("post-delete manifest write")
        .bytes
        .clone();
    let codec = ProjectFormatCodec::default();
    let manifest = codec.decode_manifest(&manifest_bytes).unwrap();
    let (reopened, _) = codec
        .decode_domain_project(&manifest, project_id())
        .unwrap()
        .unwrap();
    assert_eq!(
        reopened.deleted[&deleted_node].restoring_checkpoint,
        Some(deleted.restoring_checkpoint)
    );
}

#[test]
fn restoring_a_reopened_tombstone_rehydrates_its_missing_document_before_publishing_tree() {
    let deleted_node = parchmint_domain::NodeId::from_bytes(stable_id(5));
    let original = sample_project();
    let project = parchmint_domain::apply_project_command(
        &original,
        original.revision,
        ProjectCommand::delete_node_from_checkpoint(
            deleted_node,
            42,
            parchmint_domain::CheckpointId::from_bytes([9; 16]),
        ),
    )
    .expect("fixture deletion is valid")
    .project;
    let owner = Arc::new(NativeDocumentStateOwner::new([DocumentSnapshot {
        document_id: closed_document(),
        body: "closed needle".into(),
        comments: Vec::new(),
        revision: EditorRevision::default(),
        visibility: DocumentVisibility::Closed,
    }]));
    let dispatcher = NativeProjectCommandDispatcher::new(project, owner.clone());

    let restored = dispatcher
        .restore_deleted_with_documents(
            deleted_node,
            vec![DocumentSnapshot {
                document_id: open_document(),
                body: "restored body".into(),
                comments: Vec::new(),
                revision: EditorRevision::from(3),
                visibility: DocumentVisibility::Closed,
            }],
        )
        .expect("rehydrated tombstone restoration succeeds");

    assert!(
        restored
            .dirty_resources
            .contains(Resource::Document(open_document()))
    );
    assert_eq!(
        owner.snapshot(open_document()).unwrap().body,
        "restored body"
    );
    assert!(
        dispatcher
            .complete_authored_snapshot()
            .unwrap()
            .project
            .nodes
            .iter()
            .any(|(_, node)| node.kind == parchmint_domain::NodeKind::Document(open_document()))
    );
}

#[test]
fn created_document_has_a_recovery_base_before_its_first_autosave() {
    let (project, documents, encoding) = persisted_project("Current", "<p>current</p>");
    let owner = Arc::new(NativeDocumentStateOwner::new(documents));
    let commands = Arc::new(NativeProjectCommandDispatcher::new(project, owner.clone()));
    let editor = Arc::new(EditorPersistenceCoordinator::new(
        Arc::new(ProductionJournal::default()),
        Arc::new(CompletedSave::default()),
        recovery_base_for(&encoding),
    ));
    let coordinator = ProjectPersistenceCoordinator::new(
        commands,
        owner.clone(),
        editor,
        recovery_base_for(&encoding),
        encoding
            .resources
            .iter()
            .map(|(path, resource)| (path.clone(), resource.bytes.clone()))
            .collect(),
        encoding.paths,
    );
    let document = DocumentId::from_bytes([0x77; 16]);
    coordinator
        .create_document(CreateDocumentWorkflow {
            node: parchmint_domain::NodeId::from_bytes([0x66; 16]),
            document,
            parent: group_id(),
            index: 1,
            title: "New chapter".to_owned(),
        })
        .expect("create document should persist its canonical recovery base");

    let created = owner.snapshot(document).expect("created document snapshot");
    coordinator
        .persist_editor_projection(CanonicalProjection::new(
            document,
            created.revision.next(),
            "<p>new draft</p>",
            Vec::new(),
            Vec::new(),
            0,
        ))
        .expect("first autosave for a created document should have a recovery base");
}

fn project_with_recoverable_comment() -> (
    Arc<ProductionJournal>,
    Arc<NativeDocumentStateOwner>,
    ProjectPersistenceCoordinator,
    DocumentId,
    CanonicalComment,
) {
    let (project, documents, encoding) = persisted_project("Current", "<p>current</p>");
    let document = documents[0].document_id;
    let comment = CanonicalComment::new(
        CommentId::from_bytes([91; 16]),
        EditorSelection::new(0.into(), 9.into()),
        "Recovered note",
        BlockId::from_bytes(*document.as_bytes()),
    );
    let journal = Arc::new(ProductionJournal::default());
    {
        let owner = Arc::new(NativeDocumentStateOwner::new(documents.clone()));
        let commands = Arc::new(NativeProjectCommandDispatcher::new(
            project.clone(),
            owner.clone(),
        ));
        let editor = Arc::new(EditorPersistenceCoordinator::new(
            journal.clone(),
            Arc::new(CompletedSave::default()),
            recovery_base_for(&encoding),
        ));
        let writer = ProjectPersistenceCoordinator::new(
            commands,
            owner,
            editor,
            recovery_base_for(&encoding),
            encoding
                .resources
                .iter()
                .map(|(path, resource)| (path.clone(), resource.bytes.clone()))
                .collect(),
            encoding.paths.clone(),
        );
        writer
            .persist_editor_projection(CanonicalProjection::new(
                document,
                EditorRevision::from(2),
                "<p>recovered</p>",
                vec![comment.clone()],
                Vec::new(),
                0,
            ))
            .expect("prepare durable recovery record");
    }

    let owner = Arc::new(NativeDocumentStateOwner::new(documents));
    let commands = Arc::new(NativeProjectCommandDispatcher::new(project, owner.clone()));
    let editor = Arc::new(EditorPersistenceCoordinator::new(
        journal.clone(),
        Arc::new(CompletedSave::default()),
        recovery_base_for(&encoding),
    ));
    let coordinator = ProjectPersistenceCoordinator::new(
        commands,
        owner.clone(),
        editor,
        recovery_base_for(&encoding),
        encoding
            .resources
            .iter()
            .map(|(path, resource)| (path.clone(), resource.bytes.clone()))
            .collect(),
        encoding.paths,
    );
    (journal, owner, coordinator, document, comment)
}

#[test]
fn recovery_acceptance_restores_body_and_comments_before_a_durable_save_retires_the_journal() {
    let (journal, owner, coordinator, document, comment) = project_with_recoverable_comment();

    let recovery = coordinator.reconcile_recovery().expect("reconcile");
    assert_eq!(
        recovery.affected_documents.get(&document),
        Some(&EditorRevision::from(2))
    );
    let accepted = coordinator
        .accept_recovery(recovery.acceptance.expect("acceptance"))
        .expect("accept");
    assert_eq!(accepted.accepted_records, 1);
    let restored = owner.snapshot(document).expect("restored snapshot");
    assert_eq!(restored.body, "<p>recovered</p>");
    assert_eq!(restored.comments, vec![comment]);

    let (handle, _) = coordinator
        .request_save(PersistenceSaveKind::Restoration)
        .expect("start recovery save");
    coordinator
        .await_save(handle)
        .expect("durable recovery save");
    assert!(
        journal
            .inspect()
            .expect("journal inventory")
            .records
            .is_empty()
    );
}

#[test]
fn recovery_discard_keeps_current_state_and_resets_the_frontier_for_future_edits() {
    let (journal, owner, coordinator, document, _) = project_with_recoverable_comment();

    let recovery = coordinator.reconcile_recovery().expect("reconcile");
    coordinator
        .accept_recovery(recovery.acceptance.expect("first acceptance"))
        .expect("apply recovery before a failed save");
    assert_eq!(
        owner.snapshot(document).expect("recovered snapshot").body,
        "<p>recovered</p>"
    );
    let recovery = coordinator.reconcile_recovery().expect("retry reconcile");
    let discarded = coordinator
        .discard_recovery(recovery.acceptance.expect("acceptance"))
        .expect("discard");
    assert_eq!(discarded.accepted_records, 0);
    assert_eq!(
        owner.snapshot(document).expect("current snapshot").body,
        "<p>current</p>"
    );
    assert!(
        journal
            .inspect()
            .expect("journal inventory")
            .records
            .is_empty()
    );

    coordinator
        .persist_editor_projection(CanonicalProjection::new(
            document,
            EditorRevision::from(2),
            "<p>new edit</p>",
            Vec::new(),
            Vec::new(),
            0,
        ))
        .expect("new edit starts from canonical frontier");
}

#[test]
fn revisioned_save_serializes_only_dirty_canonical_resources_and_advances_after_ack() {
    let (project, documents, encoding) = persisted_project("Current", "<p>current</p>");
    let document = documents[0].document_id;
    let document_path = encoding.paths.documents[&document].as_str().to_owned();
    let baseline_resources = encoding.resources.len();
    let owner = Arc::new(NativeDocumentStateOwner::new(documents));
    let commands = Arc::new(NativeProjectCommandDispatcher::new(project, owner.clone()));
    let save = Arc::new(CompletedSave::default());
    let editor = Arc::new(EditorPersistenceCoordinator::new(
        Arc::new(ProductionJournal::default()),
        save.clone(),
        recovery_base_for(&encoding),
    ));
    let coordinator = ProjectPersistenceCoordinator::new(
        commands,
        owner,
        editor,
        recovery_base_for(&encoding),
        encoding
            .resources
            .iter()
            .map(|(path, resource)| (path.clone(), resource.bytes.clone()))
            .collect(),
        encoding.paths,
    );

    coordinator
        .persist_editor_projection(CanonicalProjection::new(
            document,
            EditorRevision::from(2),
            "<p>changed once</p>",
            Vec::new(),
            Vec::new(),
            0,
        ))
        .expect("dirty projection");
    let (handle, _) = coordinator
        .request_save(PersistenceSaveKind::Final)
        .expect("final save request");
    coordinator.await_save(handle).expect("final save ack");

    let requests = save.requests.lock().unwrap();
    let changed_paths = requests[0]
        .writes
        .writes
        .iter()
        .map(|write| write.path.as_str())
        .collect::<std::collections::BTreeSet<_>>();
    assert!(changed_paths.contains("project.toml"));
    assert!(changed_paths.contains(document_path.as_str()));
    assert_eq!(
        changed_paths.len(),
        3,
        "body, annotations, and frontier manifest"
    );
    assert_eq!(
        requests[0].checkpoint.resources.len(),
        baseline_resources + 1,
        "History still receives the complete resource set"
    );
    drop(requests);

    let (clean_handle, _) = coordinator
        .request_save(PersistenceSaveKind::Final)
        .expect("clean final save request");
    coordinator
        .await_save(clean_handle)
        .expect("clean final save ack");
    let requests = save.requests.lock().unwrap();
    assert!(requests[1].writes.writes.is_empty());
    assert!(requests[1].writes.deletions.is_empty());
    assert_eq!(
        requests[1].checkpoint.resources,
        requests[0].checkpoint.resources
    );
}

#[test]
fn ordinary_save_gating_skips_clean_frontiers_but_named_snapshots_remain_explicit_markers() {
    let (project, documents, encoding) = persisted_project("Current", "<p>current</p>");
    let document = documents[0].document_id;
    let owner = Arc::new(NativeDocumentStateOwner::new(documents));
    let commands = Arc::new(NativeProjectCommandDispatcher::new(project, owner.clone()));
    let save = Arc::new(CompletedSave::default());
    let editor = Arc::new(EditorPersistenceCoordinator::new(
        Arc::new(ProductionJournal::default()),
        save.clone(),
        recovery_base_for(&encoding),
    ));
    let coordinator = ProjectPersistenceCoordinator::new(
        commands,
        owner,
        editor,
        recovery_base_for(&encoding),
        encoding
            .resources
            .iter()
            .map(|(path, resource)| (path.clone(), resource.bytes.clone()))
            .collect(),
        encoding.paths,
    );

    assert!(
        coordinator
            .request_save_if_changed(PersistenceSaveKind::Final)
            .expect("clean final-save check")
            .is_none()
    );
    assert!(save.requests.lock().unwrap().is_empty());

    coordinator
        .create_named_snapshot("Clean marker".into())
        .expect("named snapshot is allowed without authored changes");
    assert_eq!(save.requests.lock().unwrap().len(), 1);
    assert_eq!(
        save.requests.lock().unwrap()[0].checkpoint.category,
        CheckpointCategory::NamedSnapshot
    );

    coordinator
        .persist_editor_projection(CanonicalProjection::new(
            document,
            EditorRevision::from(2),
            "<p>changed</p>",
            Vec::new(),
            Vec::new(),
            0,
        ))
        .expect("authored edit");
    let (handle, _) = coordinator
        .request_save_if_changed(PersistenceSaveKind::Final)
        .expect("dirty final-save check")
        .expect("dirty state starts a save");
    coordinator
        .await_save(handle)
        .expect("dirty save completes");
    assert_eq!(save.requests.lock().unwrap().len(), 2);
    assert!(
        coordinator
            .request_save_if_changed(PersistenceSaveKind::Autosave)
            .expect("clean autosave check")
            .is_none()
    );
    assert_eq!(save.requests.lock().unwrap().len(), 2);
}

#[test]
fn pending_save_does_not_advance_the_incremental_canonical_baseline() {
    let (project, documents, encoding) = persisted_project("Current", "<p>current</p>");
    let document = documents[0].document_id;
    let owner = Arc::new(NativeDocumentStateOwner::new(documents));
    let commands = Arc::new(NativeProjectCommandDispatcher::new(project, owner.clone()));
    let save = Arc::new(RecordingSave::default());
    let editor = Arc::new(EditorPersistenceCoordinator::new(
        Arc::new(ProductionJournal::default()),
        save.clone(),
        recovery_base_for(&encoding),
    ));
    let coordinator = ProjectPersistenceCoordinator::new(
        commands,
        owner,
        editor,
        recovery_base_for(&encoding),
        encoding
            .resources
            .iter()
            .map(|(path, resource)| (path.clone(), resource.bytes.clone()))
            .collect(),
        encoding.paths,
    );
    coordinator
        .persist_editor_projection(CanonicalProjection::new(
            document,
            EditorRevision::from(2),
            "<p>still pending</p>",
            Vec::new(),
            Vec::new(),
            0,
        ))
        .unwrap();

    coordinator
        .request_save(PersistenceSaveKind::Final)
        .unwrap();
    coordinator
        .request_save(PersistenceSaveKind::Final)
        .unwrap();
    let requests = save.requests.lock().unwrap();
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[0].writes, requests[1].writes);
    assert!(!requests[1].writes.writes.is_empty());
}

#[test]
fn one_dirty_document_save_keeps_three_hundred_forty_nine_closed_documents_lazy() {
    fn scaled_id(prefix: u8, ordinal: u16) -> [u8; 16] {
        let mut bytes = [prefix; 16];
        bytes[14..].copy_from_slice(&ordinal.to_be_bytes());
        bytes
    }

    let group = parchmint_domain::NodeId::from_bytes([111; 16]);
    let mut project = Project::new(project_id());
    project = parchmint_domain::apply_project_command(
        &project,
        project.revision,
        ProjectCommand::create_group(
            group,
            parchmint_domain::NodeId::manuscript_root(),
            0,
            "Scale",
        ),
    )
    .unwrap()
    .project;
    let mut bodies = BTreeMap::new();
    let mut summaries = Vec::new();
    for ordinal in 0..350_u16 {
        let node = parchmint_domain::NodeId::from_bytes(scaled_id(112, ordinal));
        let document = DocumentId::from_bytes(scaled_id(113, ordinal));
        project = parchmint_domain::apply_project_command(
            &project,
            project.revision,
            ProjectCommand::create_document(
                node,
                document,
                group,
                ordinal as usize,
                format!("Document {ordinal}"),
            ),
        )
        .unwrap()
        .project;
        bodies.insert(document, "body-71".to_owned());
        summaries.push(LazyDocumentSummary {
            document_id: document,
            revision: EditorRevision::from(7),
            visibility: if ordinal == 0 {
                DocumentVisibility::Open
            } else {
                DocumentVisibility::Closed
            },
        });
    }
    let frontier = parchmint_project_format::CanonicalPersistenceFrontier {
        recovery_project_revision: project.revision.value(),
        document_revisions: summaries
            .iter()
            .map(|summary| (summary.document_id, summary.revision.value()))
            .collect(),
        ..Default::default()
    };
    let encoding = parchmint_project_format::ProjectFormatCodec::default()
        .encode_domain_project_with_frontier(
            &project,
            &bodies,
            &BTreeMap::new(),
            &Default::default(),
            &frontier,
        )
        .unwrap();
    let document_paths = encoding
        .paths
        .documents
        .values()
        .cloned()
        .collect::<std::collections::BTreeSet<_>>();
    let expected_resources = encoding.resources.len() + 1;
    let metadata = encoding
        .resources
        .iter()
        .filter(|(path, _)| !document_paths.contains(*path))
        .map(|(path, resource)| (path.clone(), resource.bytes.clone()))
        .collect();
    let reads = Arc::new(AtomicU64::new(0));
    let owner = Arc::new(
        NativeDocumentStateOwner::new_lazy(
            summaries,
            Arc::new(CountingDocumentLoader {
                reads: reads.clone(),
            }),
        )
        .unwrap(),
    );
    let commands = Arc::new(NativeProjectCommandDispatcher::new(project, owner.clone()));
    let save = Arc::new(RecordingSave::default());
    let editor = Arc::new(EditorPersistenceCoordinator::new(
        Arc::new(ProductionJournal::default()),
        save.clone(),
        recovery_base_for(&encoding),
    ));
    let coordinator = ProjectPersistenceCoordinator::new(
        commands,
        owner.clone(),
        editor,
        recovery_base_for(&encoding),
        metadata,
        encoding.paths,
    );
    let dirty = DocumentId::from_bytes(scaled_id(113, 0));
    coordinator
        .persist_editor_projection(CanonicalProjection::new(
            dirty,
            EditorRevision::from(8),
            "<p>changed at close</p>",
            Vec::new(),
            Vec::new(),
            0,
        ))
        .unwrap();
    coordinator
        .request_save(PersistenceSaveKind::Final)
        .unwrap();

    assert_eq!(reads.load(Ordering::SeqCst), 1);
    assert_eq!(owner.loaded_snapshots().unwrap().len(), 1);
    let requests = save.requests.lock().unwrap();
    assert_eq!(requests[0].writes.writes.len(), 3);
    assert_eq!(requests[0].checkpoint.resources.len(), expected_resources);
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
fn prepared_mixed_forest_normalizes_descendants_and_preserves_order_and_authored_state() {
    use parchmint_domain::{
        MetadataApplicability, MetadataFieldDefinition, MetadataFieldId, MetadataTextKind, NodeId,
        ProjectExportSettings,
    };

    let field = MetadataFieldId::from_bytes(stable_id(51));
    let group = NodeId::from_bytes(stable_id(52));
    let node = NodeId::from_bytes(stable_id(53));
    let document = DocumentId::from_bytes(stable_id(54));
    let sibling = NodeId::from_bytes(stable_id(55));
    let sibling_document = DocumentId::from_bytes(stable_id(56));
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
                emit_titles: Default::default(),
                starts_new_page: true,
            },
        ),
        ProjectCommand::create_document(node, document, group, 0, "Chapter One"),
        ProjectCommand::set_metadata_value(node, field, Some("Revised".into())),
        ProjectCommand::create_document(
            sibling,
            sibling_document,
            NodeId::manuscript_root(),
            1,
            "Interlude",
        ),
    ] {
        project = parchmint_domain::apply_project_command(&project, project.revision, command)
            .unwrap()
            .project;
    }
    let body = "<p data-style-id=\"document-title\">Chapter One</p><p>Body</p>";
    let prepared = project_persistence::prepare_duplicates(
        &project,
        &[
            DocumentSnapshot {
                document_id: document,
                body: body.into(),
                comments: vec![CanonicalComment::new(
                    CommentId::from_bytes(stable_id(57)),
                    EditorSelection::new(0.into(), 11.into()),
                    "Source-only comment",
                    BlockId::from_bytes(*document.as_bytes()),
                )],
                revision: EditorRevision::from(7),
                visibility: DocumentVisibility::Open,
            },
            DocumentSnapshot {
                document_id: sibling_document,
                body: "<p>Interlude body</p>".into(),
                comments: Vec::new(),
                revision: EditorRevision::from(3),
                visibility: DocumentVisibility::Closed,
            },
        ],
        &DuplicateSubtreesWorkflow {
            // Deliberately unordered and redundant: the application restores
            // canonical visible order and omits the selected descendant.
            sources: vec![sibling, node, group],
            parent: NodeId::manuscript_root(),
            index: 1,
        },
    )
    .expect("group subtree can be prepared");

    let copied_group = prepared
        .project
        .nodes
        .get(prepared.created_roots[0])
        .unwrap();
    assert_ne!(prepared.created_roots[0], group);
    assert_eq!(copied_group.title, "Part One");
    assert_eq!(copied_group.synopsis, "Opening movement");
    assert_eq!(copied_group.metadata[&field], "Draft");
    assert!(copied_group.export_settings.excluded);
    assert!(copied_group.export_settings.starts_new_page);
    assert_eq!(prepared.created_roots.len(), 2);
    assert_eq!(prepared.node_ids.len(), 3);
    assert_eq!(prepared.document_ids.len(), 2);
    assert_eq!(
        prepared.project.nodes.children(NodeId::manuscript_root()),
        &[
            group,
            prepared.created_roots[0],
            prepared.created_roots[1],
            sibling,
        ]
    );
    assert_eq!(
        prepared
            .project
            .nodes
            .get(prepared.created_roots[1])
            .unwrap()
            .title,
        "Interlude"
    );
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
    assert!(
        prepared
            .documents
            .iter()
            .find(|snapshot| snapshot.document_id == copied_document)
            .unwrap()
            .comments
            .is_empty()
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
            .duplicate_subtrees(DuplicateSubtreesWorkflow {
                sources: vec![source],
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
        .duplicate_subtrees(DuplicateSubtreesWorkflow {
            sources: vec![source],
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
    assert_eq!(Some(entries[0].checkpoint_group), result.checkpoint_group);
    assert_eq!(
        dispatcher.pending_checkpoints().unwrap(),
        vec![result.checkpoint_group.expect("replacement checkpoint")]
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
