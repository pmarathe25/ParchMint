//! Application-owned editor projection, save, and recovery coordination.
//!
//! This seam is constructible before the desktop service graph; Stage 38 owns
//! only the final production graph assembly.

use std::sync::{Arc, Mutex};

use parchmint_editor_api::{
    CanonicalProjection, DurableProjectionBatch, EditorError,
    EditorPersistenceCoordinator as RecoveryCoordinator, EditorPersistenceError,
};
use parchmint_recovery_api::{
    RecoveryBaseSnapshot, RecoveryInventory, RecoveryIsolation, RecoveryJournal, RecoveryReplay,
    RecoveryRevisionVector, VersionedRecoveryPayload,
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

/// Application-owned public seam joining real editor projections to recovery
/// and revisioned saves. The desktop graph supplies one coordinator per live
/// project lease.
pub struct EditorPersistenceCoordinator {
    recovery: RecoveryCoordinator,
    status: Mutex<EditorPersistenceStatus>,
    queue: Mutex<SaveQueue>,
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
            recovery: RecoveryCoordinator::new(recovery, save.clone(), base),
            status: Mutex::new(EditorPersistenceStatus::default()),
            queue: Mutex::new(SaveQueue {
                latest: None,
                in_flight: Default::default(),
                max_depth: 0,
                submitted: 0,
                coalesced: 0,
            }),
        }
    }

    pub fn new_recovery_only(
        recovery: Arc<dyn RecoveryJournal>,
        base: RecoveryBaseSnapshot,
    ) -> Self {
        Self {
            recovery: RecoveryCoordinator::new_recovery_only(recovery, base),
            status: Mutex::new(EditorPersistenceStatus::default()),
            queue: Mutex::new(SaveQueue {
                latest: None,
                in_flight: Default::default(),
                max_depth: 0,
                submitted: 0,
                coalesced: 0,
            }),
        }
    }

    pub fn persist_projection(
        &self,
        projection: &CanonicalProjection,
        revisions: &SaveRevisionVector,
        payload: VersionedRecoveryPayload,
    ) -> Result<DurableProjectionBatch, EditorPersistenceError> {
        let durable = match self
            .recovery
            .persist_projection(projection, revisions, payload)
        {
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
        let inventory = self.recovery.recovery_inventory()?;
        status.recovery_retained_records = inventory.records.len();
        status.recovery_inventory = Some(inventory);
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
        let durable = match self.recovery.persist_projection_with_document_hash(
            projection,
            revisions,
            payload,
            result_hash,
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
        let inventory = self.recovery.recovery_inventory()?;
        status.recovery_retained_records = inventory.records.len();
        status.recovery_inventory = Some(inventory);
        status.recovery_isolation = None;
        Ok(durable)
    }

    pub fn acknowledge_recovery(
        &self,
        durable: DurableProjectionBatch,
    ) -> Result<RecoveryRevisionVector, EditorPersistenceError> {
        self.recovery.acknowledge_recovery(durable)
    }

    pub fn reconcile_recovery(
        &self,
        base: RecoveryBaseSnapshot,
    ) -> Result<RecoveryReplay, EditorPersistenceError> {
        let replay = self.recovery.reconcile_recovery(base)?;
        let inventory = self.recovery.recovery_inventory()?;
        {
            let mut status = self.status.lock().expect("editor persistence status lock");
            status.recovery_retained_records = inventory.records.len();
            status.recovery_inventory = Some(inventory);
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
        let report = self.recovery.discard_reconciled_recovery(base, replay)?;
        let inventory = self.recovery.recovery_inventory()?;
        let mut status = self
            .status
            .lock()
            .map_err(|_| EditorPersistenceError::StateUnavailable)?;
        status.state = SaveState::Saved;
        status.active = None;
        status.error = None;
        status.recovery_retained_records = inventory.records.len();
        status.recovery_inventory = Some(inventory);
        status.recovery_isolation = None;
        Ok(report)
    }

    pub fn resume_recovery_acknowledgement(
        &self,
        base: RecoveryBaseSnapshot,
        durable: DurableProjectionBatch,
    ) -> Result<RecoveryRevisionVector, EditorPersistenceError> {
        self.recovery.resume_recovery_acknowledgement(base, durable)
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
                    if self.recovery.cancel_save(ticket.clone()) == CancelOutcome::Cancelled {
                        queue.in_flight.remove(&ticket.id());
                    }
                    queue.coalesced += 1;
                }
                let ticket = self.recovery.submit_save(projection, request.clone())?;
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
        self.recovery.retire_recovery_through(base)
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
        self.recovery.frontier()
    }

    pub fn register_document_base(
        &self,
        document: parchmint_domain::DocumentId,
        revision: parchmint_recovery_api::DocumentRevision,
        hash: parchmint_recovery_api::ContentHash,
    ) -> Result<(), EditorPersistenceError> {
        self.recovery
            .register_document_base(document, revision, hash)
    }
}
