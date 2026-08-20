//! Application-owned editor projection, save, and recovery coordination.
//!
//! This seam is constructible before the desktop service graph; Stage 38 owns
//! only the final production graph assembly.

use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};

use parchmint_editor_api::{
    CanonicalProjection, DurableProjectionBatch, EditorError, EditorPersistenceError,
};
use parchmint_recovery_api::{
    EditorRevisionRange, RecoveryBaseSnapshot, RecoveryBatch, RecoveryError, RecoveryInventory,
    RecoveryIsolation, RecoveryJournal, RecoveryReplay, RecoveryRevisionVector, ResourceId,
    VersionedRecoveryPayload,
};
use parchmint_save::{
    CancelOutcome, SaveCoordinator, SaveRequest, SaveRevisionVector, SaveState, SaveTicket,
    SaveTicketId, SavedAcknowledgement,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditorPersistenceStatus {
    pub state: SaveState,
    pub requested: Option<SaveRevisionVector>,
    pub active: Option<SaveRevisionVector>,
    pub saved_through: Option<SaveRevisionVector>,
    pub error: Option<EditorPersistenceError>,
    pub recovery_inventory: Option<RecoveryInventory>,
    pub recovery_retained_records: usize,
    pub recovery_isolation: Option<RecoveryIsolation>,
}

impl Default for EditorPersistenceStatus {
    fn default() -> Self {
        Self {
            state: SaveState::Clean,
            requested: None,
            active: None,
            saved_through: None,
            error: None,
            recovery_inventory: None,
            recovery_retained_records: 0,
            recovery_isolation: None,
        }
    }
}

#[derive(Debug)]
struct SaveQueue {
    latest: Option<(SaveRevisionVector, SaveTicket)>,
    in_flight: std::collections::BTreeMap<SaveTicketId, (SaveRevisionVector, SaveTicket)>,
    max_depth: usize,
    submitted: usize,
    coalesced: usize,
}

impl SaveQueue {
    fn new() -> Self {
        Self {
            latest: None,
            in_flight: Default::default(),
            max_depth: 0,
            submitted: 0,
            coalesced: 0,
        }
    }
}

/// Application-owned public seam joining real editor projections to recovery
/// and revisioned saves. The desktop graph supplies one coordinator per live
/// project lease.
pub struct EditorPersistenceCoordinator {
    recovery: Arc<dyn RecoveryJournal>,
    save: Option<Arc<dyn SaveCoordinator>>,
    frontier: Mutex<RecoveryFrontier>,
    status: Mutex<EditorPersistenceStatus>,
    queue: Mutex<SaveQueue>,
}

#[derive(Debug, Clone)]
struct RecoveryFrontier {
    revisions: RecoveryRevisionVector,
    hashes: BTreeMap<ResourceId, parchmint_recovery_api::ContentHash>,
}

fn advance_frontier(frontier: &mut RecoveryFrontier, batch: &RecoveryBatch) {
    frontier.revisions.project_revision = batch.project_revision;
    frontier
        .revisions
        .documents
        .extend(batch.revision_vector().documents);
    frontier.hashes.extend(batch.result_hashes.clone());
}

impl std::fmt::Debug for EditorPersistenceCoordinator {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("EditorPersistenceCoordinator")
            .field("status", &self.status())
            .field("save_queue_depth", &self.save_queue_depth())
            .finish_non_exhaustive()
    }
}

impl EditorPersistenceCoordinator {
    pub fn new(
        recovery: Arc<dyn RecoveryJournal>,
        save: Arc<dyn SaveCoordinator>,
        base: RecoveryBaseSnapshot,
    ) -> Self {
        Self {
            recovery,
            save: Some(save),
            frontier: Mutex::new(RecoveryFrontier {
                revisions: base.revisions,
                hashes: base.hashes,
            }),
            status: Mutex::new(EditorPersistenceStatus::default()),
            queue: Mutex::new(SaveQueue::new()),
        }
    }

    pub fn new_recovery_only(
        recovery: Arc<dyn RecoveryJournal>,
        base: RecoveryBaseSnapshot,
    ) -> Self {
        Self {
            recovery,
            save: None,
            frontier: Mutex::new(RecoveryFrontier {
                revisions: base.revisions,
                hashes: base.hashes,
            }),
            status: Mutex::new(EditorPersistenceStatus::default()),
            queue: Mutex::new(SaveQueue::new()),
        }
    }

    pub fn persist_projection(
        &self,
        projection: &CanonicalProjection,
        revisions: &SaveRevisionVector,
        payload: VersionedRecoveryPayload,
    ) -> Result<DurableProjectionBatch, EditorPersistenceError> {
        let durable = match self.persist_projection_record(
            projection,
            revisions,
            payload,
            content_hash(projection.body().as_bytes()),
        ) {
            Ok(durable) => durable,
            Err(error) => {
                self.mark_error(error.clone());
                return Err(error);
            }
        };
        let mut status = self
            .status
            .lock()
            .map_err(|_| EditorPersistenceError::StateUnavailable)?;
        let inventory = self.recovery_inventory()?;
        self.refresh_recovery_status(&mut status, inventory);
        status.recovery_isolation = None;
        status.requested = Some(revisions.clone());
        if status.state != SaveState::Error {
            status.error = None;
        }
        if status.state != SaveState::Saving && status.state != SaveState::Error {
            status.state = if status
                .saved_through
                .as_ref()
                .is_some_and(|saved| saved.covers(revisions))
            {
                SaveState::Saved
            } else {
                SaveState::Dirty
            };
        }
        Ok(durable)
    }

    pub fn persist_projection_with_document_hash(
        &self,
        projection: &CanonicalProjection,
        revisions: &SaveRevisionVector,
        payload: VersionedRecoveryPayload,
        result_hash: parchmint_recovery_api::ContentHash,
    ) -> Result<DurableProjectionBatch, EditorPersistenceError> {
        let durable =
            match self.persist_projection_record(projection, revisions, payload, result_hash) {
                Ok(durable) => durable,
                Err(error) => {
                    self.mark_error(error.clone());
                    return Err(error);
                }
            };
        let mut status = self
            .status
            .lock()
            .map_err(|_| EditorPersistenceError::StateUnavailable)?;
        let inventory = self.recovery_inventory()?;
        self.refresh_recovery_status(&mut status, inventory);
        status.recovery_isolation = None;
        Ok(durable)
    }

    pub fn acknowledge_recovery(
        &self,
        durable: DurableProjectionBatch,
    ) -> Result<RecoveryRevisionVector, EditorPersistenceError> {
        if !durable.receipt().authenticates(durable.batch()) {
            return Err(RecoveryError::UnknownRevisionVector.into());
        }
        let mut frontier = self
            .frontier
            .lock()
            .map_err(|_| EditorPersistenceError::StateUnavailable)?;
        if durable.batch().project_revision != frontier.revisions.project_revision.next()
            || durable
                .batch()
                .base_hashes
                .iter()
                .any(|(resource, hash)| frontier.hashes.get(resource) != Some(hash))
            || durable.batch().documents.iter().any(|(document, range)| {
                range.first
                    != frontier
                        .revisions
                        .documents
                        .get(document)
                        .copied()
                        .unwrap_or_default()
                        .next()
            })
        {
            return Err(RecoveryError::NonConsecutiveProjectRevision {
                expected: frontier.revisions.project_revision.next(),
                actual: durable.batch().project_revision,
            }
            .into());
        }
        advance_frontier(&mut frontier, durable.batch());
        Ok(frontier.revisions.clone())
    }

    pub fn reconcile_recovery(
        &self,
        base: RecoveryBaseSnapshot,
    ) -> Result<RecoveryReplay, EditorPersistenceError> {
        let replay = self.recovery.replay(base)?;
        if !replay.accepted.is_empty() {
            let mut frontier = self
                .frontier
                .lock()
                .map_err(|_| EditorPersistenceError::StateUnavailable)?;
            for batch in &replay.accepted {
                advance_frontier(&mut frontier, batch);
            }
        }
        let inventory = self.recovery_inventory()?;
        {
            let mut status = self.status.lock().expect("editor persistence status lock");
            self.refresh_recovery_status(&mut status, inventory);
            status.recovery_isolation = replay.isolation.clone();
        }
        if let Some(isolation) = &replay.isolation {
            self.mark_error(EditorPersistenceError::RecoveryIsolation(
                isolation.reason.clone(),
            ));
        } else if !replay.accepted.is_empty() {
            let mut status = self.status.lock().expect("editor persistence status lock");
            status.state = SaveState::Dirty;
            status.active = None;
            status.error = None;
        }
        Ok(replay)
    }

    pub fn discard_reconciled_recovery(
        &self,
        base: RecoveryBaseSnapshot,
        replay: &RecoveryReplay,
    ) -> Result<parchmint_recovery_api::DiscardReport, EditorPersistenceError> {
        let Some(endpoint) = replay
            .accepted
            .last()
            .map(parchmint_recovery_api::RecoveryBatch::revision_vector)
        else {
            return Err(RecoveryError::UnknownRevisionVector.into());
        };
        let observed = self.recovery.replay(base.clone())?;
        if &observed != replay {
            return Err(RecoveryError::UnknownRevisionVector.into());
        }
        let report = {
            let mut frontier = self
                .frontier
                .lock()
                .map_err(|_| EditorPersistenceError::StateUnavailable)?;
            if frontier.revisions != endpoint {
                return Err(RecoveryError::UnknownRevisionVector.into());
            }
            let report = self
                .recovery
                .discard_through(parchmint_recovery_api::DurableRevisionVector::new(endpoint))?;
            frontier.revisions = base.revisions;
            frontier.hashes = base.hashes;
            report
        };
        let inventory = self.recovery_inventory()?;
        let mut status = self
            .status
            .lock()
            .map_err(|_| EditorPersistenceError::StateUnavailable)?;
        status.state = SaveState::Saved;
        status.active = None;
        status.error = None;
        self.refresh_recovery_status(&mut status, inventory);
        status.recovery_isolation = None;
        Ok(report)
    }

    pub fn resume_recovery_acknowledgement(
        &self,
        base: RecoveryBaseSnapshot,
        durable: DurableProjectionBatch,
    ) -> Result<RecoveryRevisionVector, EditorPersistenceError> {
        let replay = self.recovery.replay(base.clone())?;
        let target = durable.receipt().durable_through.clone();
        let Some(index) = replay
            .accepted
            .iter()
            .position(|batch| batch == durable.batch() && batch.revision_vector() == target)
        else {
            return Err(RecoveryError::UnknownRevisionVector.into());
        };
        let mut frontier = self
            .frontier
            .lock()
            .map_err(|_| EditorPersistenceError::StateUnavailable)?;
        frontier.revisions = base.revisions;
        frontier.hashes = base.hashes;
        for batch in &replay.accepted[..index] {
            advance_frontier(&mut frontier, batch);
        }
        drop(frontier);
        self.acknowledge_recovery(durable)
    }

    pub fn submit_save(
        &self,
        projection: &CanonicalProjection,
        request: SaveRequest,
    ) -> Result<SaveTicket, EditorPersistenceError> {
        let ticket = {
            let mut queue = self
                .queue
                .lock()
                .map_err(|_| EditorPersistenceError::StateUnavailable)?;
            let covering_ticket = queue.latest.as_ref().and_then(|(queued, ticket)| {
                (ticket.try_result().is_none() && queued.covers(&request.revisions))
                    .then(|| ticket.clone())
            });
            if let Some(ticket) = covering_ticket {
                queue.coalesced += 1;
                ticket
            } else {
                if let Some(ticket) = queue.latest.as_ref().map(|(_, ticket)| ticket.clone())
                    && ticket.try_result().is_none()
                {
                    if self.cancel_save(ticket.clone()) == CancelOutcome::Cancelled {
                        queue.in_flight.remove(&ticket.id());
                    }
                    queue.coalesced += 1;
                }
                if request
                    .revisions
                    .open_documents
                    .get(&projection.document_id())
                    != Some(&parchmint_recovery_api::DocumentRevision::from(
                        projection.revision().value(),
                    ))
                {
                    return Err(EditorPersistenceError::RevisionMismatch);
                }
                let Some(save) = self.save.as_ref() else {
                    return Err(EditorPersistenceError::StateUnavailable);
                };
                let ticket = save.request(request.clone())?;
                queue.latest = Some((request.revisions.clone(), ticket.clone()));
                queue
                    .in_flight
                    .insert(ticket.id(), (request.revisions.clone(), ticket.clone()));
                queue.max_depth = queue.max_depth.max(1);
                queue.submitted += 1;
                ticket
            }
        };
        let mut status = self
            .status
            .lock()
            .map_err(|_| EditorPersistenceError::StateUnavailable)?;
        status.requested = Some(request.revisions.clone());
        status.active = Some(request.revisions);
        status.state = SaveState::Saving;
        status.error = None;
        Ok(ticket)
    }

    pub fn mark_projection_failure(&self, error: EditorError) {
        self.mark_error(EditorPersistenceError::Projection(error));
    }

    pub fn mark_error(&self, error: EditorPersistenceError) {
        let mut status = self.status.lock().expect("editor persistence status lock");
        status.state = SaveState::Error;
        status.active = None;
        status.error = Some(error);
    }

    pub fn retire_recovery_through(
        &self,
        base: &parchmint_recovery_api::RecoveryBaseSnapshot,
    ) -> Result<(), EditorPersistenceError> {
        self.recovery
            .discard_through(parchmint_recovery_api::DurableRevisionVector::new(
                base.revisions.clone(),
            ))?;
        Ok(())
    }

    pub fn acknowledge_save(
        &self,
        acknowledgement: &SavedAcknowledgement,
    ) -> Result<(), EditorPersistenceError> {
        let mut queue = self
            .queue
            .lock()
            .map_err(|_| EditorPersistenceError::StateUnavailable)?;
        let (requested, ticket) = queue
            .in_flight
            .remove(&acknowledgement.ticket_id)
            .ok_or(EditorPersistenceError::RevisionMismatch)?;
        if acknowledgement.ticket_id != ticket.id()
            || acknowledgement.requested_revisions != requested
            || ticket.try_result().as_ref() != Some(&Ok(acknowledgement.clone()))
            || !acknowledgement
                .written_revisions
                .covers(&acknowledgement.requested_revisions)
        {
            return Err(EditorPersistenceError::RevisionMismatch);
        }
        if queue
            .latest
            .as_ref()
            .is_some_and(|(_, latest)| latest.id() == acknowledgement.ticket_id)
        {
            queue.latest = None;
        }
        let active = queue
            .latest
            .as_ref()
            .map(|(revisions, _)| revisions.clone());
        drop(queue);
        let mut status = self
            .status
            .lock()
            .map_err(|_| EditorPersistenceError::StateUnavailable)?;
        status.active = active;
        status.saved_through = Some(acknowledgement.written_revisions.clone());
        status.error = None;
        status.state = if status
            .requested
            .as_ref()
            .is_none_or(|requested| acknowledgement.written_revisions.covers(requested))
        {
            SaveState::Saved
        } else {
            SaveState::Dirty
        };
        Ok(())
    }

    pub fn status(&self) -> EditorPersistenceStatus {
        self.status
            .lock()
            .expect("editor persistence status lock")
            .clone()
    }

    pub fn save_queue_depth(&self) -> usize {
        usize::from(
            self.queue
                .lock()
                .expect("editor persistence save queue lock")
                .latest
                .is_some(),
        )
    }

    pub fn max_save_queue_depth(&self) -> usize {
        self.queue
            .lock()
            .expect("editor persistence save queue lock")
            .max_depth
    }

    pub fn submitted_save_requests(&self) -> usize {
        self.queue
            .lock()
            .expect("editor persistence save queue lock")
            .submitted
    }

    pub fn coalesced_save_requests(&self) -> usize {
        self.queue
            .lock()
            .expect("editor persistence save queue lock")
            .coalesced
    }

    pub fn frontier(&self) -> Option<RecoveryRevisionVector> {
        self.frontier
            .lock()
            .ok()
            .map(|frontier| frontier.revisions.clone())
    }

    pub fn register_document_base(
        &self,
        document: parchmint_domain::DocumentId,
        revision: parchmint_recovery_api::DocumentRevision,
        hash: parchmint_recovery_api::ContentHash,
    ) -> Result<(), EditorPersistenceError> {
        let mut frontier = self
            .frontier
            .lock()
            .map_err(|_| EditorPersistenceError::StateUnavailable)?;
        let resource = document_resource_id(document);
        if let Some(existing) = frontier.hashes.get(&resource) {
            return (*existing == hash)
                .then_some(())
                .ok_or(EditorPersistenceError::RevisionMismatch);
        }
        if frontier
            .revisions
            .documents
            .get(&document)
            .is_some_and(|known| *known != revision)
        {
            return Err(EditorPersistenceError::RevisionMismatch);
        }
        frontier.revisions.documents.insert(document, revision);
        frontier.hashes.insert(resource, hash);
        Ok(())
    }

    fn persist_projection_record(
        &self,
        projection: &CanonicalProjection,
        revisions: &SaveRevisionVector,
        payload: VersionedRecoveryPayload,
        result_hash: parchmint_recovery_api::ContentHash,
    ) -> Result<DurableProjectionBatch, EditorPersistenceError> {
        let frontier = self
            .frontier
            .lock()
            .map_err(|_| EditorPersistenceError::StateUnavailable)?
            .clone();
        let Some(requested) = revisions.open_documents.get(&projection.document_id()) else {
            return Err(EditorPersistenceError::RevisionMismatch);
        };
        if *requested
            < parchmint_recovery_api::DocumentRevision::from(projection.revision().value())
        {
            return Err(EditorPersistenceError::RevisionMismatch);
        }
        let previous = frontier
            .revisions
            .documents
            .get(&projection.document_id())
            .copied()
            .unwrap_or_default();
        let last = parchmint_recovery_api::DocumentRevision::from(projection.revision().value());
        if last <= previous {
            return Err(EditorPersistenceError::RevisionMismatch);
        }
        let exact_resource = document_resource_id(projection.document_id());
        let document_resource = if frontier.hashes.contains_key(&exact_resource) {
            exact_resource
        } else {
            ResourceId::Document
        };
        if !frontier.hashes.contains_key(&document_resource) {
            return Err(RecoveryError::MissingBaseHash {
                resource: document_resource,
            }
            .into());
        }
        let batch = RecoveryBatch {
            project_revision: frontier.revisions.project_revision.next(),
            documents: BTreeMap::from([(
                projection.document_id(),
                EditorRevisionRange::new(previous.next(), last)?,
            )]),
            base_hashes: BTreeMap::from([(
                document_resource.clone(),
                frontier.hashes[&document_resource],
            )]),
            result_hashes: BTreeMap::from([(document_resource, result_hash)]),
            payload,
        };
        batch.validate_after(None)?;
        let receipt = self.recovery.append(batch.clone())?;
        if receipt.durable_through != batch.revision_vector() {
            return Err(RecoveryError::UnknownRevisionVector.into());
        }
        DurableProjectionBatch::new(batch, receipt)
    }

    fn recovery_inventory(&self) -> Result<RecoveryInventory, EditorPersistenceError> {
        self.recovery
            .inspect()
            .map_err(EditorPersistenceError::Recovery)
    }

    fn cancel_save(&self, ticket: SaveTicket) -> CancelOutcome {
        self.save
            .as_ref()
            .map_or(CancelOutcome::WorkerStopped, |save| {
                save.cancel_pending(ticket)
            })
    }

    fn refresh_recovery_status(
        &self,
        status: &mut EditorPersistenceStatus,
        inventory: RecoveryInventory,
    ) {
        status.recovery_retained_records = inventory.records.len();
        status.recovery_inventory = Some(inventory);
    }
}

fn content_hash(bytes: &[u8]) -> parchmint_recovery_api::ContentHash {
    parchmint_recovery_api::ContentHash::of_bytes(bytes)
}

fn document_resource_id(document: parchmint_domain::DocumentId) -> ResourceId {
    let document_id = document
        .as_bytes()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    ResourceId::DocumentById { document_id }
}
