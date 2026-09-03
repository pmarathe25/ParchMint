use std::{
    collections::BTreeMap,
    error::Error,
    fmt,
    sync::{Arc, Mutex},
    time::{SystemTime, UNIX_EPOCH},
};

use parchmint_contracts::{
    AnnotationAnchor, AnnotationMessage, AnnotationThread, generated::RecoveryRecordV1,
};
use parchmint_domain::{
    CheckpointId, DocumentId, NodeId, NodeKind, Project, ProjectCommand, ProjectRevision, Resource,
    apply_project_command,
};
use parchmint_editor_api::{
    BlockId, CanonicalComment, CanonicalCommentAnchor, CanonicalCommentMessage,
    CanonicalProjection, CommentId, DocumentPosition, EditorPersistenceError, EditorRevision,
    EditorSelection,
};
use parchmint_history_api::{
    CheckpointCategory, CheckpointInput, CheckpointIntentHash, RestorePlan, SnapshotName,
};
use parchmint_project_format::{
    CanonicalAnnotations, CanonicalBytes, CanonicalCodec, CanonicalDocumentUpdate,
    CanonicalDomainUpdate, CanonicalPersistenceFrontier, CanonicalPersistenceFrontierTransition,
    CanonicalProjectEncoding, CanonicalProjectPatch, CanonicalProjectPathMap,
    CanonicalRelativePath, CanonicalResource, CanonicalResourceMetadata, ContentHash, FormatError,
    ProjectFormatCodec,
};
use parchmint_project_repository::{AtomicWritePlan, StagedResource};
use parchmint_recovery_api::{
    RecoveryBaseSnapshot, RecoveryIsolation, RecoveryReplay, VersionedRecoveryPayload,
};
use parchmint_save::{
    ResourceRevision, SaveError, SaveGeneration, SavePriority, SaveRequest, SaveRevisionVector,
    SaveState, SaveTicket,
};
use serde_json::json;
use sha2::{Digest, Sha256};

use crate::{
    ApplicationError, DocumentSnapshot, EditorPersistenceCoordinator, EditorPersistenceStatus,
    NativeDocumentStateOwner, NativeProjectCommandDispatcher, RevisionedSaveRequest,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PersistenceSaveKind {
    Autosave,
    Structural,
    Explicit,
    Final,
    Restoration,
    NamedSnapshot,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PersistenceRevision {
    pub project_revision: ProjectRevision,
    pub documents: BTreeMap<DocumentId, EditorRevision>,
    pub generation: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PersistenceSaveHandle(u64);

impl PersistenceSaveHandle {
    pub const fn value(self) -> u64 {
        self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DurableProjectionAck {
    pub document: DocumentId,
    pub document_revision: EditorRevision,
    pub recovery_project_revision: ProjectRevision,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PersistenceSavedRevision {
    pub requested: PersistenceRevision,
    pub written: PersistenceRevision,
    pub checkpoint: CheckpointId,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreatedDocumentRevision {
    pub document: DocumentId,
    pub revision: PersistenceSavedRevision,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RestoredProjectRevision {
    pub source: CheckpointId,
    pub revision: PersistenceSavedRevision,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreateDocumentWorkflow {
    pub node: NodeId,
    pub document: DocumentId,
    pub parent: NodeId,
    pub index: usize,
    pub title: String,
}

/// One authoritative tree move prepared by the UI from a current snapshot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MoveNodeWorkflow {
    pub node: NodeId,
    pub parent: NodeId,
    pub index: usize,
}

/// A structurally saved set of tree moves.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MoveNodesWorkflow {
    pub moves: Vec<MoveNodeWorkflow>,
}

/// One authoritative deletion request over normalized live subtree roots.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeleteSubtreesWorkflow {
    pub nodes: Vec<NodeId>,
    pub deleted_at_unix_millis: u64,
}

/// A durable deletion whose tombstones point to the pre-delete checkpoint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeletedSubtreesRevision {
    pub restoring_checkpoint: CheckpointId,
    pub revision: PersistenceSavedRevision,
}

/// An application-owned request to clone one complete live subtree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DuplicateSubtreesWorkflow {
    pub sources: Vec<NodeId>,
    pub parent: NodeId,
    pub index: usize,
}

/// Durable duplicate result. Comments, annotations, and source History are
/// intentionally not copied; the duplicate receives only this new structural
/// checkpoint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DuplicatedSubtreesRevision {
    pub created_roots: Vec<NodeId>,
    pub node_ids: BTreeMap<NodeId, NodeId>,
    pub document_ids: BTreeMap<DocumentId, DocumentId>,
    pub revision: PersistenceSavedRevision,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PersistenceStatus {
    pub state: SaveState,
    pub requested: Option<PersistenceRevision>,
    pub active: Option<PersistenceRevision>,
    pub saved_through: Option<PersistenceRevision>,
    pub recovery_retained_records: usize,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RecoveryAcceptance(u64);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PersistenceRecoveryState {
    pub accepted_records: usize,
    pub affected_documents: BTreeMap<DocumentId, EditorRevision>,
    pub isolation: Option<RecoveryIsolation>,
    pub acceptance: Option<RecoveryAcceptance>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProjectPersistenceError {
    Application(String),
    Projection(String),
    Format(String),
    Save(String),
    History(String),
    OperationInProgress,
    UnknownSaveHandle,
    UnknownRecoveryAcceptance,
    StateUnavailable,
}

impl fmt::Display for ProjectPersistenceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Application(reason) => {
                write!(formatter, "application persistence failed: {reason}")
            }
            Self::Projection(reason) => write!(formatter, "editor persistence failed: {reason}"),
            Self::Format(reason) => write!(formatter, "canonical encoding failed: {reason}"),
            Self::Save(reason) => write!(formatter, "project save failed: {reason}"),
            Self::History(reason) => write!(formatter, "History restore failed: {reason}"),
            Self::OperationInProgress => {
                formatter.write_str("another project persistence operation is still pending")
            }
            Self::UnknownSaveHandle => formatter.write_str("save handle is not pending"),
            Self::UnknownRecoveryAcceptance => {
                formatter.write_str("recovery acceptance is not pending")
            }
            Self::StateUnavailable => {
                formatter.write_str("project persistence state is unavailable")
            }
        }
    }
}

impl Error for ProjectPersistenceError {}

impl From<ApplicationError> for ProjectPersistenceError {
    fn from(error: ApplicationError) -> Self {
        Self::Application(error.to_string())
    }
}

impl From<parchmint_domain::DomainError> for ProjectPersistenceError {
    fn from(error: parchmint_domain::DomainError) -> Self {
        Self::Application(error.to_string())
    }
}

impl From<EditorPersistenceError> for ProjectPersistenceError {
    fn from(error: EditorPersistenceError) -> Self {
        Self::Projection(error.to_string())
    }
}

impl From<FormatError> for ProjectPersistenceError {
    fn from(error: FormatError) -> Self {
        Self::Format(error.to_string())
    }
}

impl From<SaveError> for ProjectPersistenceError {
    fn from(error: SaveError) -> Self {
        Self::Save(error.to_string())
    }
}

struct CanonicalState {
    resources: BTreeMap<CanonicalRelativePath, Vec<u8>>,
    complete_resources: BTreeMap<CanonicalRelativePath, CanonicalResourceMetadata>,
    paths: CanonicalProjectPathMap,
    frontier: CanonicalPersistenceFrontier,
}

struct PendingSave {
    ticket: SaveTicket,
    capture: RevisionedSaveRequest,
    patch: CanonicalProjectPatch,
}

#[derive(Clone)]
struct PendingRecovery {
    replay: RecoveryReplay,
}

pub struct ProjectPersistenceCoordinator {
    commands: Arc<NativeProjectCommandDispatcher>,
    documents: Arc<NativeDocumentStateOwner>,
    editor: Arc<EditorPersistenceCoordinator>,
    recovery_base: Mutex<RecoveryBaseSnapshot>,
    canonical: Mutex<CanonicalState>,
    pending_saves: Mutex<BTreeMap<PersistenceSaveHandle, PendingSave>>,
    pending_recovery: Mutex<BTreeMap<RecoveryAcceptance, PendingRecovery>>,
    next_handle: Mutex<u64>,
    workflow: Mutex<()>,
}

impl ProjectPersistenceCoordinator {
    pub fn new(
        commands: Arc<NativeProjectCommandDispatcher>,
        documents: Arc<NativeDocumentStateOwner>,
        editor: Arc<EditorPersistenceCoordinator>,
        recovery_base: RecoveryBaseSnapshot,
        resources: BTreeMap<CanonicalRelativePath, Vec<u8>>,
        paths: CanonicalProjectPathMap,
    ) -> Self {
        let frontier = canonical_frontier(&resources).unwrap_or_default();
        let complete_resources = canonical_resource_metadata(&resources, &paths, &frontier);
        Self {
            commands,
            documents,
            editor,
            recovery_base: Mutex::new(recovery_base),
            canonical: Mutex::new(CanonicalState {
                resources,
                complete_resources,
                paths,
                frontier,
            }),
            pending_saves: Mutex::new(BTreeMap::new()),
            pending_recovery: Mutex::new(BTreeMap::new()),
            next_handle: Mutex::new(1),
            workflow: Mutex::new(()),
        }
    }

    pub fn register_loaded_document_base(
        &self,
        document: DocumentId,
        revision: EditorRevision,
        hash: parchmint_recovery_api::ContentHash,
    ) -> Result<(), ProjectPersistenceError> {
        let revision = parchmint_recovery_api::DocumentRevision::from(revision.value());
        self.editor
            .register_document_base(document, revision, hash)?;
        let mut base = self
            .recovery_base
            .lock()
            .map_err(|_| ProjectPersistenceError::StateUnavailable)?;
        base.revisions.documents.insert(document, revision);
        base.hashes.insert(
            parchmint_recovery_api::ResourceId::DocumentById {
                document_id: stable_id_text(document.as_bytes()),
            },
            hash,
        );
        Ok(())
    }

    fn register_current_document_base(
        &self,
        document: DocumentId,
    ) -> Result<(), ProjectPersistenceError> {
        let snapshot = self.documents.snapshot(document)?;
        let annotations = CanonicalAnnotations::from_typed(
            stable_id_text(document.as_bytes()),
            &snapshot
                .comments
                .iter()
                .map(contract_thread)
                .collect::<Vec<_>>(),
        )?;
        let annotation_bytes = ProjectFormatCodec::default()
            .encode(&CanonicalResource::Annotations(annotations))?
            .bytes;
        self.register_loaded_document_base(
            document,
            snapshot.revision,
            recovery_document_content_hash(snapshot.body.as_bytes(), Some(&annotation_bytes)),
        )
    }

    pub fn persist_editor_projection(
        &self,
        projection: CanonicalProjection,
    ) -> Result<DurableProjectionAck, ProjectPersistenceError> {
        let _workflow = self
            .workflow
            .lock()
            .map_err(|_| ProjectPersistenceError::StateUnavailable)?;
        self.commands.accept_editor_projection(&projection)?;
        let capture = self.commands.recovery_revision_request()?;
        let mut revisions = save_revisions(&capture, None);
        revisions.open_documents.insert(
            projection.document_id(),
            parchmint_recovery_api::DocumentRevision::from(projection.revision().value()),
        );
        revisions
            .closed_resources
            .remove(&parchmint_project_format::ResourceId::DocumentById {
                document_id: stable_id_text(projection.document_id().as_bytes()),
            });
        let record_id = format!(
            "{}-{}-{}",
            stable_id_text(projection.document_id().as_bytes()),
            projection.revision().value(),
            capture.generation
        );
        let annotation_document_id = stable_id_text(projection.document_id().as_bytes());
        let annotations = CanonicalAnnotations::from_typed(
            annotation_document_id.clone(),
            &projection
                .comments()
                .iter()
                .map(contract_thread)
                .collect::<Vec<_>>(),
        )?;
        let annotation_value =
            serde_json::to_value(parchmint_contracts::generated::AnnotationSidecarV1 {
                schema: "parchmint.annotation-sidecar/v1".into(),
                document_id: annotation_document_id,
                threads: annotations.threads().to_vec(),
            })
            .map_err(|error| ProjectPersistenceError::Format(error.to_string()))?;
        let annotation_bytes = ProjectFormatCodec::default()
            .encode(&CanonicalResource::Annotations(annotations))?
            .bytes;
        let payload = VersionedRecoveryPayload::V1(RecoveryRecordV1 {
            schema: "parchmint.recovery-record/v1".into(),
            record_id,
            operations: vec![json!({
                "kind": "replace-document",
                "document_id": stable_id_text(projection.document_id().as_bytes()),
                "revision": projection.revision().value(),
                "body": projection.body(),
                "annotations": annotation_value,
            })],
        });
        let result_hash = recovery_document_content_hash(
            projection.body().as_bytes(),
            Some(annotation_bytes.as_slice()),
        );
        let durable = self.editor.persist_projection_with_document_hash(
            &projection,
            &revisions,
            payload,
            result_hash,
        )?;
        let frontier = self.editor.acknowledge_recovery(durable)?;
        Ok(DurableProjectionAck {
            document: projection.document_id(),
            document_revision: projection.revision(),
            recovery_project_revision: frontier.project_revision,
        })
    }

    pub fn request_save(
        &self,
        kind: PersistenceSaveKind,
    ) -> Result<(PersistenceSaveHandle, PersistenceRevision), ProjectPersistenceError> {
        let _workflow = self
            .workflow
            .lock()
            .map_err(|_| ProjectPersistenceError::StateUnavailable)?;
        self.request_save_inner(kind, None)
    }

    /// Starts an ordinary save only when the captured authored frontier is
    /// dirty. Named snapshots, restoration, and structural workflows use the
    /// unconditional path because their checkpoint is itself meaningful.
    pub fn request_save_if_changed(
        &self,
        kind: PersistenceSaveKind,
    ) -> Result<Option<(PersistenceSaveHandle, PersistenceRevision)>, ProjectPersistenceError> {
        let _workflow = self
            .workflow
            .lock()
            .map_err(|_| ProjectPersistenceError::StateUnavailable)?;
        if !matches!(
            kind,
            PersistenceSaveKind::Autosave
                | PersistenceSaveKind::Explicit
                | PersistenceSaveKind::Final
        ) {
            return self.request_save_inner(kind, None).map(Some);
        }
        let Some((capture, snapshot)) = self.commands.capture_save_state_if_dirty()? else {
            return Ok(None);
        };
        self.request_save_captured(kind, None, capture, snapshot)
            .map(Some)
    }

    pub fn has_unsaved_changes(&self) -> Result<bool, ProjectPersistenceError> {
        self.commands.has_unsaved_changes().map_err(Into::into)
    }

    fn request_save_inner(
        &self,
        kind: PersistenceSaveKind,
        name: Option<SnapshotName>,
    ) -> Result<(PersistenceSaveHandle, PersistenceRevision), ProjectPersistenceError> {
        let (capture, snapshot) = self.commands.capture_save_state()?;
        self.request_save_captured(kind, name, capture, snapshot)
    }

    fn request_save_captured(
        &self,
        kind: PersistenceSaveKind,
        name: Option<SnapshotName>,
        capture: RevisionedSaveRequest,
        snapshot: crate::AuthoredProjectSnapshot,
    ) -> Result<(PersistenceSaveHandle, PersistenceRevision), ProjectPersistenceError> {
        let project = snapshot.project;
        let loaded_documents = snapshot
            .documents
            .into_iter()
            .map(|snapshot| (snapshot.document_id, snapshot))
            .collect::<BTreeMap<_, _>>();
        let dirty_documents = capture
            .dirty_resources
            .iter()
            .filter_map(|resource| match resource {
                Resource::Document(document) => Some(*document),
                Resource::Manifest | Resource::Styles | Resource::Dictionary => None,
            })
            .map(|document| {
                loaded_documents
                    .get(&document)
                    .cloned()
                    .map(|snapshot| (document, snapshot))
                    .ok_or(crate::ApplicationError::MissingDocument { document })
            })
            .collect::<Result<BTreeMap<_, _>, _>>()?;
        let update = CanonicalDomainUpdate {
            manifest: capture.dirty_resources.contains(Resource::Manifest),
            styles: capture.dirty_resources.contains(Resource::Styles),
            dictionary: capture.dirty_resources.contains(Resource::Dictionary),
            documents: dirty_documents
                .iter()
                .map(|(document, snapshot)| {
                    (
                        *document,
                        CanonicalDocumentUpdate {
                            body: snapshot.body.clone(),
                            annotations: snapshot.comments.iter().map(contract_thread).collect(),
                        },
                    )
                })
                .collect(),
        };
        let document_revisions = capture
            .open_documents
            .iter()
            .chain(capture.closed_documents.iter())
            .map(|(document, revision)| (*document, revision.value()))
            .collect();
        let canonical = self
            .canonical
            .lock()
            .map_err(|_| ProjectPersistenceError::StateUnavailable)?;
        let persistence_frontier = CanonicalPersistenceFrontier {
            recovery_project_revision: self
                .editor
                .frontier()
                .ok_or(ProjectPersistenceError::StateUnavailable)?
                .project_revision
                .value(),
            document_revisions,
            ..Default::default()
        };
        let codec = ProjectFormatCodec::default();
        let patch = codec.encode_domain_project_patch(
            &project,
            &update,
            &canonical.resources,
            &canonical.complete_resources,
            &canonical.paths,
            CanonicalPersistenceFrontierTransition {
                previous: &canonical.frontier,
                next: &persistence_frontier,
            },
        );
        let mut patch = match patch {
            Ok(patch) => patch,
            Err(FormatError::InvalidDocument(_)) => {
                let documents = project
                    .nodes
                    .iter()
                    .filter_map(|(_, node)| match node.kind {
                        NodeKind::Document(document) => Some(document),
                        NodeKind::Root(_) | NodeKind::Group => None,
                    })
                    .map(|document| {
                        self.documents
                            .snapshot(document)
                            .map(|snapshot| (document, snapshot))
                    })
                    .collect::<Result<BTreeMap<_, _>, _>>()?;
                let encoding = codec.encode_domain_project_with_annotations(
                    &project,
                    &documents
                        .iter()
                        .map(|(document, snapshot)| (*document, snapshot.body.clone()))
                        .collect(),
                    &documents
                        .iter()
                        .map(|(document, snapshot)| {
                            (
                                *document,
                                snapshot.comments.iter().map(contract_thread).collect(),
                            )
                        })
                        .collect(),
                    &canonical.resources,
                    &canonical.paths,
                    &persistence_frontier,
                )?;
                patch_from_encoding(encoding)
            }
            Err(error) => return Err(error.into()),
        };
        // A named or structural checkpoint can intentionally capture an
        // already-clean project. The filesystem writer requires at least one
        // atomic operation, so rewrite the unchanged manifest to establish
        // the otherwise no-op checkpoint as a durable boundary.
        if matches!(
            kind,
            PersistenceSaveKind::Structural | PersistenceSaveKind::NamedSnapshot
        ) && patch.resources.is_empty()
            && patch.deletions.is_empty()
        {
            let manifest_path = CanonicalRelativePath::parse("project.toml")?;
            let metadata = patch
                .complete_resources
                .get(&manifest_path)
                .ok_or_else(|| {
                    ProjectPersistenceError::Application(
                        "canonical manifest is unavailable for a checkpoint".to_owned(),
                    )
                })?;
            let bytes = canonical
                .resources
                .get(&manifest_path)
                .cloned()
                .ok_or_else(|| {
                    ProjectPersistenceError::Application(
                        "canonical manifest bytes are unavailable for a checkpoint".to_owned(),
                    )
                })?;
            patch.resources.insert(
                manifest_path.clone(),
                CanonicalBytes {
                    resource: metadata.resource.clone(),
                    path: manifest_path,
                    bytes,
                    hash: metadata.hash,
                },
            );
        }
        drop(canonical);
        let revisions = save_revisions_from_patch(&capture, &patch);
        let projection = dirty_documents
            .values()
            .find(|snapshot| capture.open_documents.contains_key(&snapshot.document_id))
            .cloned()
            .or_else(|| {
                loaded_documents
                    .values()
                    .find(|snapshot| capture.open_documents.contains_key(&snapshot.document_id))
                    .cloned()
            })
            // A project can become document-empty after deletion. The editor
            // save bridge still needs a projection token, so use a retained
            // open record without reintroducing it into authored snapshots.
            .or_else(|| {
                self.documents
                    .snapshots()
                    .ok()?
                    .into_iter()
                    .find(|snapshot| capture.open_documents.contains_key(&snapshot.document_id))
            })
            .ok_or_else(|| {
                ProjectPersistenceError::Application(
                    "project has no document available for a revisioned save".into(),
                )
            })?;
        let projection = CanonicalProjection::new(
            projection.document_id,
            projection.revision,
            projection.body,
            Vec::new(),
            Vec::new(),
            0,
        );
        let request =
            materialize_patch_save_request(kind, name, &capture, revisions.clone(), &patch);
        let ticket = self.editor.submit_save(&projection, request)?;
        let handle = {
            let mut next = self
                .next_handle
                .lock()
                .map_err(|_| ProjectPersistenceError::StateUnavailable)?;
            let handle = PersistenceSaveHandle(*next);
            *next = next.saturating_add(1);
            handle
        };
        self.pending_saves
            .lock()
            .map_err(|_| ProjectPersistenceError::StateUnavailable)?
            .insert(
                handle,
                PendingSave {
                    ticket,
                    capture,
                    patch,
                },
            );
        Ok((handle, persistence_revision(&revisions)))
    }

    pub fn await_save(
        &self,
        handle: PersistenceSaveHandle,
    ) -> Result<PersistenceSavedRevision, ProjectPersistenceError> {
        let pending = self
            .pending_saves
            .lock()
            .map_err(|_| ProjectPersistenceError::StateUnavailable)?
            .remove(&handle)
            .ok_or(ProjectPersistenceError::UnknownSaveHandle)?;
        let acknowledgement = match pending.ticket.wait() {
            Ok(acknowledgement) => acknowledgement,
            Err(error) => {
                self.editor
                    .mark_error(EditorPersistenceError::Save(error.clone()));
                return Err(error.into());
            }
        };
        let recovery_base = recovery_base_from_patch(
            &self
                .recovery_base
                .lock()
                .map_err(|_| ProjectPersistenceError::StateUnavailable)?
                .clone(),
            &pending.patch,
        );
        self.editor.retire_recovery_through(&recovery_base)?;
        self.editor.acknowledge_save(&acknowledgement)?;
        self.commands.acknowledge_save(&pending.capture)?;
        let mut canonical = self
            .canonical
            .lock()
            .map_err(|_| ProjectPersistenceError::StateUnavailable)?;
        for path in &pending.patch.deletions {
            canonical.resources.remove(path);
        }
        for (path, resource) in &pending.patch.resources {
            canonical
                .resources
                .insert(path.clone(), resource.bytes.clone());
        }
        canonical.complete_resources = pending.patch.complete_resources;
        canonical.paths = pending.patch.paths;
        canonical.frontier = pending.patch.persistence_frontier;
        *self
            .recovery_base
            .lock()
            .map_err(|_| ProjectPersistenceError::StateUnavailable)? = recovery_base;
        Ok(PersistenceSavedRevision {
            requested: persistence_revision(&acknowledgement.requested_revisions),
            written: persistence_revision(&acknowledgement.written_revisions),
            checkpoint: acknowledgement.checkpoint,
        })
    }

    /// Creates the tree node and default canonical document state as one
    /// in-memory mutation, then commits the resulting structural snapshot.
    /// A save failure leaves the complete new document dirty and recoverable;
    /// it never leaves a node without its document body.
    pub fn create_document(
        &self,
        request: CreateDocumentWorkflow,
    ) -> Result<CreatedDocumentRevision, ProjectPersistenceError> {
        let _workflow = self
            .workflow
            .lock()
            .map_err(|_| ProjectPersistenceError::StateUnavailable)?;
        let command = crate::ProjectCommand::create_document(
            request.node,
            request.document,
            request.parent,
            request.index,
            request.title,
        );
        self.commands.execute_now(command)?;
        let (handle, _) = self.request_save_inner(PersistenceSaveKind::Structural, None)?;
        let revision = self.await_save(handle)?;
        self.register_current_document_base(request.document)?;
        Ok(CreatedDocumentRevision {
            document: request.document,
            revision,
        })
    }

    /// Flushes the exact pre-delete state, associates every new tombstone with
    /// that immutable checkpoint, then durably saves the post-delete project.
    pub fn delete_subtrees(
        &self,
        request: DeleteSubtreesWorkflow,
    ) -> Result<DeletedSubtreesRevision, ProjectPersistenceError> {
        let _workflow = self
            .workflow
            .lock()
            .map_err(|_| ProjectPersistenceError::StateUnavailable)?;
        let current = self.commands.project()?;
        let nodes = normalized_subtree_roots(&current, &request.nodes)?;
        if nodes.is_empty() || nodes.iter().any(|node| node.is_fixed_root()) {
            return Err(ProjectPersistenceError::Application(
                "delete workflow requires at least one non-root subtree".into(),
            ));
        }

        // Validate the full command sequence before publishing any mutation.
        let mut simulated = current;
        for node in &nodes {
            simulated = apply_project_command(
                &simulated,
                simulated.revision,
                ProjectCommand::delete_node(*node),
            )?
            .project;
        }

        let (before_handle, _) = self.request_save_inner(PersistenceSaveKind::Structural, None)?;
        let restoring_checkpoint = self.await_save(before_handle)?.checkpoint;
        for node in nodes {
            self.commands
                .execute_now(ProjectCommand::delete_node_from_checkpoint(
                    node,
                    request.deleted_at_unix_millis,
                    restoring_checkpoint,
                ))?;
        }
        let (after_handle, _) = self.request_save_inner(PersistenceSaveKind::Structural, None)?;
        let revision = self.await_save(after_handle)?;
        Ok(DeletedSubtreesRevision {
            restoring_checkpoint,
            revision,
        })
    }

    /// Restores one tombstoned subtree without rewinding unrelated project
    /// work. Its document bodies are rehydrated from the exact immutable
    /// pre-delete checkpoint before the tree becomes live again.
    pub fn restore_deleted_subtree(
        &self,
        node: NodeId,
        plan: RestorePlan,
    ) -> Result<PersistenceSavedRevision, ProjectPersistenceError> {
        let _workflow = self
            .workflow
            .lock()
            .map_err(|_| ProjectPersistenceError::StateUnavailable)?;
        let current = self.commands.project()?;
        let tombstone = current.deleted.get(&node).ok_or_else(|| {
            ProjectPersistenceError::Application("deleted item is no longer available".into())
        })?;
        let checkpoint = tombstone.restoring_checkpoint.ok_or_else(|| {
            ProjectPersistenceError::History(
                "deleted item has no recoverable pre-delete checkpoint".into(),
            )
        })?;
        if plan.source() != checkpoint {
            return Err(ProjectPersistenceError::History(
                "History returned a checkpoint that does not match the deleted item".into(),
            ));
        }

        let historical_resources = validated_restore_resources(&plan)?;
        let (_, _, historical_frontier, historical_bodies, historical_comments) =
            decode_restored_project(current.id, &historical_resources)?;
        let restored = tombstone
            .subtree
            .iter()
            .filter_map(|snapshot| match snapshot.node.kind {
                NodeKind::Document(document) => Some(document),
                NodeKind::Root(_) | NodeKind::Group => None,
            })
            .map(|document| {
                let body = historical_bodies.get(&document).ok_or_else(|| {
                    ProjectPersistenceError::History(format!(
                        "pre-delete checkpoint does not contain restored document {document:?}"
                    ))
                })?;
                let historical_revision = historical_frontier
                    .document_revisions
                    .get(&document)
                    .copied()
                    .unwrap_or_default();
                let current_revision = self
                    .documents
                    .snapshot(document)
                    .map(|snapshot| snapshot.revision.value())
                    .unwrap_or_default();
                Ok(crate::DocumentSnapshot {
                    document_id: document,
                    body: body.clone(),
                    comments: historical_comments
                        .get(&document)
                        .cloned()
                        .unwrap_or_default(),
                    revision: EditorRevision::from(
                        historical_revision.max(current_revision).saturating_add(1),
                    ),
                    visibility: crate::DocumentVisibility::Closed,
                })
            })
            .collect::<Result<Vec<_>, ProjectPersistenceError>>()?;
        self.commands
            .restore_deleted_with_documents(node, restored)?;
        let (handle, _) = self.request_save_inner(PersistenceSaveKind::Structural, None)?;
        self.await_save(handle)
    }

    /// Applies preflighted domain `MoveNode` commands and durably commits the
    /// resulting hierarchy before returning to the session.
    pub fn move_nodes(
        &self,
        request: MoveNodesWorkflow,
    ) -> Result<PersistenceSavedRevision, ProjectPersistenceError> {
        let _workflow = self
            .workflow
            .lock()
            .map_err(|_| ProjectPersistenceError::StateUnavailable)?;
        if request.moves.is_empty() {
            return Err(ProjectPersistenceError::Application(
                "move workflow requires at least one node".to_owned(),
            ));
        }

        if !self
            .pending_saves
            .lock()
            .map_err(|_| ProjectPersistenceError::StateUnavailable)?
            .is_empty()
        {
            return Err(ProjectPersistenceError::OperationInProgress);
        }
        let current = self.commands.complete_authored_snapshot()?;
        let mut simulated = current.project;
        for movement in &request.moves {
            let command = ProjectCommand::move_node(movement.node, movement.parent, movement.index);
            simulated = apply_project_command(&simulated, simulated.revision, command)?.project;
        }
        self.persist_prepared_state(simulated, current.documents)
    }

    /// Clones one group or document subtree with fresh identities. The entire
    /// canonical image is written before the prepared project and documents
    /// become visible, so save failure cannot publish a partial duplicate.
    pub fn duplicate_subtrees(
        &self,
        request: DuplicateSubtreesWorkflow,
    ) -> Result<DuplicatedSubtreesRevision, ProjectPersistenceError> {
        let _workflow = self
            .workflow
            .lock()
            .map_err(|_| ProjectPersistenceError::StateUnavailable)?;
        if !self
            .pending_saves
            .lock()
            .map_err(|_| ProjectPersistenceError::StateUnavailable)?
            .is_empty()
        {
            return Err(ProjectPersistenceError::OperationInProgress);
        }

        let current = self.commands.complete_authored_snapshot()?;
        let prepared = prepare_duplicates(&current.project, &current.documents, &request)?;
        let revision =
            self.persist_prepared_state(prepared.project.clone(), prepared.documents.clone())?;

        Ok(DuplicatedSubtreesRevision {
            created_roots: prepared.created_roots,
            node_ids: prepared.node_ids,
            document_ids: prepared.document_ids,
            revision,
        })
    }

    fn persist_prepared_state(
        &self,
        project: Project,
        documents: Vec<DocumentSnapshot>,
    ) -> Result<PersistenceSavedRevision, ProjectPersistenceError> {
        let recovery_project_revision = self
            .editor
            .frontier()
            .ok_or(ProjectPersistenceError::StateUnavailable)?
            .project_revision
            .next();
        let frontier = CanonicalPersistenceFrontier {
            recovery_project_revision: recovery_project_revision.value(),
            document_revisions: documents
                .iter()
                .map(|document| (document.document_id, document.revision.value()))
                .collect(),
            ..Default::default()
        };
        let canonical = self
            .canonical
            .lock()
            .map_err(|_| ProjectPersistenceError::StateUnavailable)?;
        let encoding = ProjectFormatCodec::default().encode_domain_project_with_frontier(
            &project,
            &documents
                .iter()
                .map(|document| (document.document_id, document.body.clone()))
                .collect(),
            &canonical.resources,
            &canonical.paths,
            &frontier,
        )?;
        drop(canonical);

        let capture = self.commands.capture_save_request()?;
        let mut revisions = save_revisions(&capture, Some(&encoding));
        revisions.project_revision = project.revision;
        revisions.open_documents.clear();
        revisions.closed_resources.retain(|resource, _| {
            !matches!(
                resource,
                parchmint_project_format::ResourceId::DocumentById { .. }
            )
        });
        for document in &documents {
            match document.visibility {
                crate::DocumentVisibility::Open | crate::DocumentVisibility::Hidden => {
                    revisions.open_documents.insert(
                        document.document_id,
                        parchmint_recovery_api::DocumentRevision::from(document.revision.value()),
                    );
                }
                crate::DocumentVisibility::Closed => {
                    revisions.closed_resources.insert(
                        parchmint_project_format::ResourceId::DocumentById {
                            document_id: stable_id_text(document.document_id.as_bytes()),
                        },
                        ResourceRevision::from(document.revision.value()),
                    );
                }
            }
        }
        let request = materialize_save_request(
            PersistenceSaveKind::Structural,
            None,
            &capture,
            revisions,
            &encoding,
        );
        let projection = documents.first().ok_or_else(|| {
            ProjectPersistenceError::Application("project has no documents".into())
        })?;
        let projection = CanonicalProjection::new(
            projection.document_id,
            projection.revision,
            projection.body.clone(),
            Vec::new(),
            Vec::new(),
            0,
        );
        let ticket = self.editor.submit_save(&projection, request)?;
        let acknowledgement = match ticket.wait() {
            Ok(acknowledgement) => acknowledgement,
            Err(error) => {
                self.editor
                    .mark_error(EditorPersistenceError::Save(error.clone()));
                return Err(error.into());
            }
        };

        let recovery_base = recovery_base_from_encoding(&encoding);
        self.editor.retire_recovery_through(&recovery_base)?;
        self.editor.acknowledge_save(&acknowledgement)?;
        {
            let mut canonical = self
                .canonical
                .lock()
                .map_err(|_| ProjectPersistenceError::StateUnavailable)?;
            canonical.resources = encoding
                .resources
                .iter()
                .map(|(path, resource)| (path.clone(), resource.bytes.clone()))
                .collect();
            canonical.complete_resources = encoding
                .resources
                .iter()
                .map(|(path, resource)| {
                    (
                        path.clone(),
                        CanonicalResourceMetadata {
                            resource: resource.resource.clone(),
                            hash: resource.hash,
                        },
                    )
                })
                .collect();
            canonical.frontier = encoding.persistence_frontier.clone();
            canonical.paths = encoding.paths;
        }
        *self
            .recovery_base
            .lock()
            .map_err(|_| ProjectPersistenceError::StateUnavailable)? = recovery_base;
        self.commands.publish_restored_state(project, documents)?;

        Ok(PersistenceSavedRevision {
            requested: persistence_revision(&acknowledgement.requested_revisions),
            written: persistence_revision(&acknowledgement.written_revisions),
            checkpoint: acknowledgement.checkpoint,
        })
    }

    pub fn create_named_snapshot(
        &self,
        name: String,
    ) -> Result<PersistenceSavedRevision, ProjectPersistenceError> {
        let _workflow = self
            .workflow
            .lock()
            .map_err(|_| ProjectPersistenceError::StateUnavailable)?;
        let name = SnapshotName::new(name)
            .map_err(|error| ProjectPersistenceError::History(error.to_string()))?;
        let (handle, _) =
            self.request_save_inner(PersistenceSaveKind::NamedSnapshot, Some(name))?;
        self.await_save(handle)
    }

    /// Applies a whole-project History plan through the ordinary atomic save
    /// coordinator. The restored state is decoded and validated before any
    /// write, and is published to queries only after canonical files and the
    /// matching restoration checkpoint are durable.
    pub fn restore_history(
        &self,
        plan: RestorePlan,
    ) -> Result<RestoredProjectRevision, ProjectPersistenceError> {
        let _workflow = self
            .workflow
            .lock()
            .map_err(|_| ProjectPersistenceError::StateUnavailable)?;
        if !self
            .pending_saves
            .lock()
            .map_err(|_| ProjectPersistenceError::StateUnavailable)?
            .is_empty()
        {
            return Err(ProjectPersistenceError::OperationInProgress);
        }

        let source = plan.source();
        let restored_resources = validated_restore_resources(&plan)?;
        let current = self.commands.complete_authored_snapshot()?;
        let current_project = current.project;
        let current_documents = current.documents;
        let (mut project, restored_paths, restored_frontier, restored_bodies, restored_comments) =
            decode_restored_project(current_project.id, &restored_resources)?;

        // Domain and document revisions remain monotonic even when authored
        // content comes from an older checkpoint.
        project.revision = current_project.revision.next();
        let current_revisions = current_documents
            .iter()
            .map(|snapshot| (snapshot.document_id, snapshot.revision))
            .collect::<BTreeMap<_, _>>();
        let current_visibility = current_documents
            .iter()
            .map(|snapshot| (snapshot.document_id, snapshot.visibility))
            .collect::<BTreeMap<_, _>>();
        let mut documents = restored_bodies
            .iter()
            .map(|(document, body)| {
                let historical = restored_frontier
                    .document_revisions
                    .get(document)
                    .copied()
                    .unwrap_or_default();
                let current = current_revisions
                    .get(document)
                    .copied()
                    .unwrap_or_default()
                    .value();
                DocumentSnapshot {
                    document_id: *document,
                    body: body.clone(),
                    comments: restored_comments.get(document).cloned().unwrap_or_default(),
                    revision: EditorRevision::from(historical.max(current).saturating_add(1)),
                    visibility: current_visibility
                        .get(document)
                        .copied()
                        .unwrap_or(crate::DocumentVisibility::Closed),
                }
            })
            .collect::<Vec<_>>();
        if !documents
            .iter()
            .any(|document| document.visibility == crate::DocumentVisibility::Open)
            && let Some(first) = documents.first_mut()
        {
            first.visibility = crate::DocumentVisibility::Open;
        }

        let recovery_project_revision = self
            .editor
            .frontier()
            .ok_or(ProjectPersistenceError::StateUnavailable)?
            .project_revision
            .next();
        let frontier = CanonicalPersistenceFrontier {
            recovery_project_revision: recovery_project_revision.value(),
            document_revisions: documents
                .iter()
                .map(|document| (document.document_id, document.revision.value()))
                .collect(),
            ..Default::default()
        };
        let current_paths = self
            .canonical
            .lock()
            .map_err(|_| ProjectPersistenceError::StateUnavailable)?
            .paths
            .clone();
        let mut encoding = ProjectFormatCodec::default().encode_domain_project_with_frontier(
            &project,
            &restored_bodies,
            &restored_resources,
            &current_paths,
            &frontier,
        )?;
        let mut deletions = encoding
            .deletions
            .into_iter()
            .collect::<std::collections::BTreeSet<_>>();
        deletions.extend(plan.deletions().iter().cloned());
        encoding.deletions = deletions.into_iter().collect();
        debug_assert_eq!(encoding.paths, restored_paths);

        let capture = self.commands.capture_save_request()?;
        let mut revisions = save_revisions(&capture, Some(&encoding));
        revisions.project_revision = project.revision;
        revisions.open_documents.clear();
        revisions.closed_resources.retain(|resource, _| {
            !matches!(
                resource,
                parchmint_project_format::ResourceId::DocumentById { .. }
            )
        });
        for document in &documents {
            match document.visibility {
                crate::DocumentVisibility::Open | crate::DocumentVisibility::Hidden => {
                    revisions.open_documents.insert(
                        document.document_id,
                        parchmint_recovery_api::DocumentRevision::from(document.revision.value()),
                    );
                }
                crate::DocumentVisibility::Closed => {
                    revisions.closed_resources.insert(
                        parchmint_project_format::ResourceId::DocumentById {
                            document_id: stable_id_text(document.document_id.as_bytes()),
                        },
                        ResourceRevision::from(document.revision.value()),
                    );
                }
            }
        }
        let request = materialize_save_request(
            PersistenceSaveKind::Restoration,
            None,
            &capture,
            revisions,
            &encoding,
        );
        let projection = documents.first().ok_or_else(|| {
            ProjectPersistenceError::History(
                "restored project does not contain a document".to_owned(),
            )
        })?;
        let projection = CanonicalProjection::new(
            projection.document_id,
            projection.revision,
            projection.body.clone(),
            Vec::new(),
            Vec::new(),
            0,
        );
        let ticket = self.editor.submit_save(&projection, request)?;
        let acknowledgement = match ticket.wait() {
            Ok(acknowledgement) => acknowledgement,
            Err(error) => {
                self.editor
                    .mark_error(EditorPersistenceError::Save(error.clone()));
                return Err(error.into());
            }
        };

        let recovery_base = recovery_base_from_encoding(&encoding);
        self.editor.retire_recovery_through(&recovery_base)?;
        self.editor.acknowledge_save(&acknowledgement)?;
        {
            let mut canonical = self
                .canonical
                .lock()
                .map_err(|_| ProjectPersistenceError::StateUnavailable)?;
            canonical.resources = encoding
                .resources
                .iter()
                .map(|(path, resource)| (path.clone(), resource.bytes.clone()))
                .collect();
            canonical.complete_resources = encoding
                .resources
                .iter()
                .map(|(path, resource)| {
                    (
                        path.clone(),
                        CanonicalResourceMetadata {
                            resource: resource.resource.clone(),
                            hash: resource.hash,
                        },
                    )
                })
                .collect();
            canonical.frontier = encoding.persistence_frontier.clone();
            canonical.paths = encoding.paths;
        }
        *self
            .recovery_base
            .lock()
            .map_err(|_| ProjectPersistenceError::StateUnavailable)? = recovery_base;
        self.commands.publish_restored_state(project, documents)?;

        Ok(RestoredProjectRevision {
            source,
            revision: PersistenceSavedRevision {
                requested: persistence_revision(&acknowledgement.requested_revisions),
                written: persistence_revision(&acknowledgement.written_revisions),
                checkpoint: acknowledgement.checkpoint,
            },
        })
    }

    pub fn status(&self) -> PersistenceStatus {
        persistence_status(self.editor.status())
    }

    pub fn canonical_text(&self, path: &str) -> Result<Option<String>, ProjectPersistenceError> {
        let path = CanonicalRelativePath::parse(path)?;
        self.canonical
            .lock()
            .map_err(|_| ProjectPersistenceError::StateUnavailable)?
            .resources
            .get(&path)
            .map(|bytes| {
                String::from_utf8(bytes.clone()).map_err(|_| {
                    ProjectPersistenceError::Format(format!(
                        "canonical resource {} is not UTF-8",
                        path.as_str()
                    ))
                })
            })
            .transpose()
    }

    pub fn reconcile_recovery(&self) -> Result<PersistenceRecoveryState, ProjectPersistenceError> {
        let base = self
            .recovery_base
            .lock()
            .map_err(|_| ProjectPersistenceError::StateUnavailable)?
            .clone();
        let replay = self.editor.reconcile_recovery(base)?;
        let affected_documents = recovery_affected_documents(&replay);
        self.pending_recovery
            .lock()
            .map_err(|_| ProjectPersistenceError::StateUnavailable)?
            .clear();
        let acceptance = if replay.accepted.is_empty() {
            None
        } else {
            let mut next = self
                .next_handle
                .lock()
                .map_err(|_| ProjectPersistenceError::StateUnavailable)?;
            let acceptance = RecoveryAcceptance(*next);
            *next = next.saturating_add(1);
            self.pending_recovery
                .lock()
                .map_err(|_| ProjectPersistenceError::StateUnavailable)?
                .insert(
                    acceptance,
                    PendingRecovery {
                        replay: replay.clone(),
                    },
                );
            Some(acceptance)
        };
        Ok(PersistenceRecoveryState {
            accepted_records: replay.accepted.len(),
            affected_documents,
            isolation: replay.isolation,
            acceptance,
        })
    }

    pub fn accept_recovery(
        &self,
        acceptance: RecoveryAcceptance,
    ) -> Result<PersistenceRecoveryState, ProjectPersistenceError> {
        let pending = self
            .pending_recovery
            .lock()
            .map_err(|_| ProjectPersistenceError::StateUnavailable)?
            .remove(&acceptance)
            .ok_or(ProjectPersistenceError::UnknownRecoveryAcceptance)?;
        for batch in &pending.replay.accepted {
            let VersionedRecoveryPayload::V1(record) = &batch.payload;
            for operation in &record.operations {
                if operation.get("kind").and_then(serde_json::Value::as_str)
                    != Some("replace-document")
                {
                    continue;
                }
                let document = operation
                    .get("document_id")
                    .and_then(serde_json::Value::as_str)
                    .ok_or(ProjectPersistenceError::UnknownRecoveryAcceptance)?;
                let revision = operation
                    .get("revision")
                    .and_then(serde_json::Value::as_u64)
                    .ok_or(ProjectPersistenceError::UnknownRecoveryAcceptance)?;
                let body = operation
                    .get("body")
                    .and_then(serde_json::Value::as_str)
                    .ok_or(ProjectPersistenceError::UnknownRecoveryAcceptance)?;
                let document_id = DocumentId::from_bytes(parse_stable_id(document)?);
                let comments = match operation.get("annotations") {
                    Some(value) => ProjectFormatCodec::default()
                        .decode_annotations(
                            &serde_json::to_vec(value)
                                .map_err(|_| ProjectPersistenceError::UnknownRecoveryAcceptance)?,
                        )?
                        .typed_threads()?
                        .into_iter()
                        .map(editor_thread_contract)
                        .collect(),
                    None => self.documents.snapshot(document_id)?.comments,
                };
                self.commands
                    .accept_editor_projection(&CanonicalProjection::new(
                        document_id,
                        EditorRevision::from(revision),
                        body,
                        comments,
                        Vec::new(),
                        0,
                    ))?;
            }
        }
        Ok(PersistenceRecoveryState {
            accepted_records: pending.replay.accepted.len(),
            affected_documents: recovery_affected_documents(&pending.replay),
            isolation: pending.replay.isolation,
            acceptance: None,
        })
    }

    pub fn discard_recovery(
        &self,
        acceptance: RecoveryAcceptance,
    ) -> Result<PersistenceRecoveryState, ProjectPersistenceError> {
        let pending = self
            .pending_recovery
            .lock()
            .map_err(|_| ProjectPersistenceError::StateUnavailable)?
            .get(&acceptance)
            .cloned()
            .ok_or(ProjectPersistenceError::UnknownRecoveryAcceptance)?;
        let current = self.commands.complete_authored_snapshot()?;
        let current_project = current.project;
        let current_documents = current.documents;
        let canonical_resources = self
            .canonical
            .lock()
            .map_err(|_| ProjectPersistenceError::StateUnavailable)?
            .resources
            .clone();
        let (canonical_project, _, canonical_frontier, canonical_bodies, canonical_comments) =
            decode_restored_project(current_project.id, &canonical_resources)?;
        if canonical_project.revision != current_project.revision {
            return Err(ProjectPersistenceError::OperationInProgress);
        }
        let visibility = current_documents
            .into_iter()
            .map(|document| (document.document_id, document.visibility))
            .collect::<BTreeMap<_, _>>();
        let canonical_documents = canonical_bodies
            .into_iter()
            .map(|(document, body)| DocumentSnapshot {
                document_id: document,
                body,
                comments: canonical_comments
                    .get(&document)
                    .cloned()
                    .unwrap_or_default(),
                revision: EditorRevision::from(
                    canonical_frontier
                        .document_revisions
                        .get(&document)
                        .copied()
                        .unwrap_or_default(),
                ),
                visibility: visibility
                    .get(&document)
                    .copied()
                    .unwrap_or(crate::DocumentVisibility::Closed),
            })
            .collect::<Vec<_>>();
        let base = self
            .recovery_base
            .lock()
            .map_err(|_| ProjectPersistenceError::StateUnavailable)?
            .clone();
        self.editor
            .discard_reconciled_recovery(base.clone(), &pending.replay)?;
        self.pending_recovery
            .lock()
            .map_err(|_| ProjectPersistenceError::StateUnavailable)?
            .remove(&acceptance);
        self.commands
            .publish_restored_state(canonical_project, canonical_documents)?;
        let replay = self.editor.reconcile_recovery(base)?;
        Ok(PersistenceRecoveryState {
            accepted_records: replay.accepted.len(),
            affected_documents: recovery_affected_documents(&replay),
            isolation: replay.isolation,
            acceptance: None,
        })
    }
}

fn recovery_affected_documents(replay: &RecoveryReplay) -> BTreeMap<DocumentId, EditorRevision> {
    let mut affected = BTreeMap::new();
    for batch in &replay.accepted {
        for (document, range) in &batch.documents {
            affected
                .entry(*document)
                .and_modify(|revision: &mut EditorRevision| {
                    *revision = (*revision).max(EditorRevision::from(range.last.value()));
                })
                .or_insert_with(|| EditorRevision::from(range.last.value()));
        }
    }
    affected
}

pub(crate) struct PreparedDuplicate {
    pub(crate) project: Project,
    pub(crate) documents: Vec<DocumentSnapshot>,
    pub(crate) created_roots: Vec<NodeId>,
    pub(crate) node_ids: BTreeMap<NodeId, NodeId>,
    pub(crate) document_ids: BTreeMap<DocumentId, DocumentId>,
}

pub(crate) fn prepare_duplicates(
    project: &Project,
    documents: &[DocumentSnapshot],
    request: &DuplicateSubtreesWorkflow,
) -> Result<PreparedDuplicate, ProjectPersistenceError> {
    let sources = normalized_copy_sources(project, &request.sources)?;
    if sources.is_empty() {
        return Err(ProjectPersistenceError::Application(
            "copy workflow requires at least one subtree root".to_owned(),
        ));
    }
    if sources.iter().any(|source| source.is_fixed_root()) {
        return Err(ProjectPersistenceError::Application(
            "fixed project roots cannot be copied".to_owned(),
        ));
    }
    if !project
        .nodes
        .get(request.parent)
        .is_some_and(|node| node.kind.can_have_children())
    {
        return Err(ProjectPersistenceError::Application(
            "copy destination is not a live container".to_owned(),
        ));
    }

    let mut preorder = Vec::new();
    for source in &sources {
        collect_subtree_ids(project, *source, &mut preorder);
    }
    let root_ordinals = sources
        .iter()
        .enumerate()
        .map(|(ordinal, source)| (*source, ordinal))
        .collect::<BTreeMap<_, _>>();
    let mut draft = project.clone();
    let mut node_ids = BTreeMap::new();
    let mut document_ids = BTreeMap::new();
    let mut reserved_nodes = std::collections::BTreeSet::new();
    let mut reserved_documents = std::collections::BTreeSet::new();
    for (ordinal, source_id) in preorder.iter().copied().enumerate() {
        let node = project
            .nodes
            .get(source_id)
            .expect("preorder source exists");
        let fresh_node = fresh_node_id(&draft, source_id, ordinal as u64, &reserved_nodes);
        reserved_nodes.insert(fresh_node);
        node_ids.insert(source_id, fresh_node);
        if let NodeKind::Document(document) = node.kind {
            let fresh_document =
                fresh_document_id(&draft, document, ordinal as u64, &reserved_documents);
            reserved_documents.insert(fresh_document);
            document_ids.insert(document, fresh_document);
        }
    }

    let mut duplicated_documents = Vec::new();
    for source_id in preorder {
        let source_node = project
            .nodes
            .get(source_id)
            .expect("preorder source exists");
        let fresh_node = node_ids[&source_id];
        let (parent, index) = if let Some(root_ordinal) = root_ordinals.get(&source_id) {
            (request.parent, request.index.saturating_add(*root_ordinal))
        } else {
            let source_parent = project
                .nodes
                .parent(source_id)
                .expect("non-root subtree child has a parent");
            let parent = node_ids[&source_parent];
            (parent, draft.nodes.children(parent).len())
        };
        let copied_title = match source_node.kind {
            NodeKind::Document(_) => format!("{} Copy", source_node.title),
            NodeKind::Group => source_node.title.clone(),
            NodeKind::Root(_) => unreachable!("fixed roots were rejected"),
        };
        let command = match source_node.kind {
            NodeKind::Group => {
                ProjectCommand::create_group(fresh_node, parent, index, copied_title.clone())
            }
            NodeKind::Document(document) => ProjectCommand::create_document(
                fresh_node,
                document_ids[&document],
                parent,
                index,
                copied_title.clone(),
            ),
            NodeKind::Root(_) => unreachable!("fixed roots were rejected"),
        };
        draft = apply_project_command(&draft, draft.revision, command)?.project;

        if !source_node.synopsis.is_empty() {
            draft = apply_project_command(
                &draft,
                draft.revision,
                ProjectCommand::set_synopsis(fresh_node, source_node.synopsis.clone()),
            )?
            .project;
        }
        if source_node.export_settings != Default::default() {
            draft = apply_project_command(
                &draft,
                draft.revision,
                ProjectCommand::set_node_export_settings(fresh_node, source_node.export_settings),
            )?
            .project;
        }
        let default_metadata = draft
            .nodes
            .get(fresh_node)
            .expect("created duplicate exists")
            .metadata
            .clone();
        for field in default_metadata
            .keys()
            .filter(|field| !source_node.metadata.contains_key(field))
        {
            draft = apply_project_command(
                &draft,
                draft.revision,
                ProjectCommand::set_metadata_value(fresh_node, *field, None),
            )?
            .project;
        }
        for (field, value) in &source_node.metadata {
            draft = apply_project_command(
                &draft,
                draft.revision,
                ProjectCommand::set_metadata_value(fresh_node, *field, Some(value.clone())),
            )?
            .project;
        }

        if let NodeKind::Document(source_document) = source_node.kind {
            let source_snapshot = documents
                .iter()
                .find(|document| document.document_id == source_document)
                .ok_or_else(|| {
                    ProjectPersistenceError::Application(
                        "copy source document body is unavailable".to_owned(),
                    )
                })?;
            duplicated_documents.push(DocumentSnapshot {
                document_id: document_ids[&source_document],
                body: ProjectFormatCodec::default()
                    .decode_document(source_snapshot.body.as_bytes())?
                    .append_copy_suffix_to_matching_title(&source_node.title, " Copy")
                    .as_html()
                    .to_owned(),
                comments: Vec::new(),
                revision: Default::default(),
                visibility: crate::DocumentVisibility::Closed,
            });
        }
    }

    let mut all_documents = documents.to_vec();
    all_documents.extend(duplicated_documents);
    Ok(PreparedDuplicate {
        project: draft,
        documents: all_documents,
        created_roots: sources.iter().map(|source| node_ids[source]).collect(),
        node_ids,
        document_ids,
    })
}

fn normalized_copy_sources(
    project: &Project,
    requested: &[NodeId],
) -> Result<Vec<NodeId>, ProjectPersistenceError> {
    normalized_subtree_roots(project, requested)
}

fn normalized_subtree_roots(
    project: &Project,
    requested: &[NodeId],
) -> Result<Vec<NodeId>, ProjectPersistenceError> {
    let requested = requested
        .iter()
        .copied()
        .collect::<std::collections::BTreeSet<_>>();
    for source in &requested {
        if !project.nodes.contains(*source) {
            return Err(ProjectPersistenceError::Application(
                "subtree source is stale".to_owned(),
            ));
        }
    }

    let mut visible_order = Vec::new();
    for root in [NodeId::manuscript_root(), NodeId::research_root()] {
        collect_subtree_ids(project, root, &mut visible_order);
    }
    Ok(visible_order
        .into_iter()
        .filter(|source| requested.contains(source))
        .filter(|source| {
            let mut parent = project.nodes.parent(*source);
            while let Some(ancestor) = parent {
                if requested.contains(&ancestor) {
                    return false;
                }
                parent = project.nodes.parent(ancestor);
            }
            true
        })
        .collect())
}

fn collect_subtree_ids(project: &Project, node: NodeId, output: &mut Vec<NodeId>) {
    output.push(node);
    for child in project.nodes.children(node) {
        collect_subtree_ids(project, *child, output);
    }
}

fn fresh_node_id(
    project: &Project,
    source: NodeId,
    ordinal: u64,
    reserved: &std::collections::BTreeSet<NodeId>,
) -> NodeId {
    for attempt in 1_u64.. {
        let mut digest = Sha256::new();
        digest.update(b"parchmint duplicate node\0");
        digest.update(project.id.as_bytes());
        digest.update(source.as_bytes());
        digest.update(project.revision.value().to_be_bytes());
        digest.update(ordinal.to_be_bytes());
        digest.update(attempt.to_be_bytes());
        let hash = digest.finalize();
        let mut bytes = [0; 16];
        bytes.copy_from_slice(&hash[..16]);
        let candidate = NodeId::from_bytes(bytes);
        let live = project.nodes.contains(candidate);
        let deleted = project
            .deleted
            .values()
            .flat_map(|tombstone| &tombstone.subtree)
            .any(|snapshot| snapshot.node.id == candidate);
        if !candidate.is_fixed_root() && !live && !deleted && !reserved.contains(&candidate) {
            return candidate;
        }
    }
    unreachable!("finite project cannot exhaust node identity space")
}

fn fresh_document_id(
    project: &Project,
    source: DocumentId,
    ordinal: u64,
    reserved: &std::collections::BTreeSet<DocumentId>,
) -> DocumentId {
    for attempt in 1_u64.. {
        let mut digest = Sha256::new();
        digest.update(b"parchmint duplicate document\0");
        digest.update(project.id.as_bytes());
        digest.update(source.as_bytes());
        digest.update(project.revision.value().to_be_bytes());
        digest.update(ordinal.to_be_bytes());
        digest.update(attempt.to_be_bytes());
        let hash = digest.finalize();
        let mut bytes = [0; 16];
        bytes.copy_from_slice(&hash[..16]);
        let candidate = DocumentId::from_bytes(bytes);
        let used = project.nodes.iter().any(|(_, node)| {
            matches!(node.kind, NodeKind::Document(document) if document == candidate)
        }) || project
            .deleted
            .values()
            .flat_map(|tombstone| &tombstone.subtree)
            .any(|snapshot| {
                matches!(snapshot.node.kind, NodeKind::Document(document) if document == candidate)
            });
        if !used && !reserved.contains(&candidate) {
            return candidate;
        }
    }
    unreachable!("finite project cannot exhaust document identity space")
}

fn validated_restore_resources(
    plan: &RestorePlan,
) -> Result<BTreeMap<CanonicalRelativePath, Vec<u8>>, ProjectPersistenceError> {
    let mut resources = BTreeMap::new();
    for write in &plan.writes().writes {
        let path = CanonicalRelativePath::parse(&write.path)?;
        let expected = plan.resources().get(&path).ok_or_else(|| {
            ProjectPersistenceError::History("restore write is absent from its manifest".into())
        })?;
        let actual = ContentHash::of_bytes(&write.bytes);
        if &actual != expected || resources.insert(path, write.bytes.clone()).is_some() {
            return Err(ProjectPersistenceError::History(
                "restore resources do not match their History manifest".into(),
            ));
        }
    }
    if resources.len() != plan.resources().len() {
        return Err(ProjectPersistenceError::History(
            "restore plan is not a complete resource set".into(),
        ));
    }
    Ok(resources)
}

type RestoredProjectDecode = (
    crate::Project,
    CanonicalProjectPathMap,
    CanonicalPersistenceFrontier,
    BTreeMap<DocumentId, String>,
    BTreeMap<DocumentId, Vec<CanonicalComment>>,
);

fn decode_restored_project(
    project_id: crate::ProjectId,
    resources: &BTreeMap<CanonicalRelativePath, Vec<u8>>,
) -> Result<RestoredProjectDecode, ProjectPersistenceError> {
    let codec = ProjectFormatCodec::default();
    let control = resources
        .get(&CanonicalRelativePath::parse(".parchmint/format-version")?)
        .ok_or_else(|| ProjectPersistenceError::History("format control is missing".into()))?;
    codec.detect(control)?;
    let manifest = resources
        .get(&CanonicalRelativePath::parse("project.toml")?)
        .ok_or_else(|| ProjectPersistenceError::History("project manifest is missing".into()))?;
    let manifest = codec.decode_manifest(manifest)?;
    let styles = resources
        .get(&CanonicalRelativePath::parse("styles.css")?)
        .map(|bytes| codec.decode_styles(bytes))
        .transpose()?;
    let (mut project, paths) = codec
        .decode_domain_project_with_styles(&manifest, styles.as_ref(), project_id)?
        .ok_or_else(|| {
            ProjectPersistenceError::History(
                "checkpoint predates the canonical project structure extension".into(),
            )
        })?;
    let frontier = codec.decode_persistence_frontier(&manifest)?;
    if let Some(dictionary) = resources.get(&CanonicalRelativePath::parse("dictionary.txt")?) {
        let dictionary = codec.decode_dictionary(dictionary)?;
        project.dictionary = Default::default();
        for entry in dictionary.entries() {
            project
                .dictionary
                .insert(entry)
                .map_err(|error| ProjectPersistenceError::Format(error.to_string()))?;
        }
    }
    let mut comments = BTreeMap::new();
    for document in paths.documents.keys() {
        let path = CanonicalRelativePath::parse(format!(
            "annotations/{}.json",
            stable_id_text(document.as_bytes())
        ))?;
        let threads = match resources.get(&path) {
            Some(bytes) => codec
                .decode_annotations(bytes)?
                .typed_threads()?
                .into_iter()
                .map(editor_thread_contract)
                .collect(),
            None => Vec::new(),
        };
        comments.insert(*document, threads);
    }
    let bodies = paths
        .documents
        .iter()
        .map(|(document, path)| {
            let bytes = resources.get(path).ok_or_else(|| {
                ProjectPersistenceError::History(format!(
                    "restored document is missing: {}",
                    path.as_str()
                ))
            })?;
            let body = codec.decode_document(bytes)?.as_html().to_owned();
            Ok((*document, body))
        })
        .collect::<Result<BTreeMap<_, _>, ProjectPersistenceError>>()?;
    Ok((project, paths, frontier, bodies, comments))
}

fn canonical_frontier(
    resources: &BTreeMap<CanonicalRelativePath, Vec<u8>>,
) -> Result<CanonicalPersistenceFrontier, ProjectPersistenceError> {
    let path = CanonicalRelativePath::parse("project.toml")?;
    let Some(manifest) = resources.get(&path) else {
        return Ok(CanonicalPersistenceFrontier::default());
    };
    let codec = ProjectFormatCodec::default();
    let manifest = codec.decode_manifest(manifest)?;
    codec
        .decode_persistence_frontier(&manifest)
        .map_err(Into::into)
}

fn canonical_resource_metadata(
    resources: &BTreeMap<CanonicalRelativePath, Vec<u8>>,
    paths: &CanonicalProjectPathMap,
    frontier: &CanonicalPersistenceFrontier,
) -> BTreeMap<CanonicalRelativePath, CanonicalResourceMetadata> {
    let mut metadata = resources
        .iter()
        .map(|(path, bytes)| {
            let resource = match path.as_str() {
                ".parchmint/format-version" => parchmint_project_format::ResourceId::FormatControl,
                "project.toml" => parchmint_project_format::ResourceId::Manifest,
                "styles.css" => parchmint_project_format::ResourceId::Styles,
                "dictionary.txt" => parchmint_project_format::ResourceId::Dictionary,
                path if path.starts_with("annotations/") && path.ends_with(".json") => {
                    parchmint_project_format::ResourceId::Annotations {
                        document_id: path
                            .trim_start_matches("annotations/")
                            .trim_end_matches(".json")
                            .to_owned(),
                    }
                }
                _ => parchmint_project_format::ResourceId::Document,
            };
            (
                path.clone(),
                CanonicalResourceMetadata {
                    resource,
                    hash: ContentHash::of_bytes(bytes),
                },
            )
        })
        .collect::<BTreeMap<_, _>>();
    for (document, path) in &paths.documents {
        if let Some(summary) = frontier.document_summaries.get(document) {
            metadata.insert(
                path.clone(),
                CanonicalResourceMetadata {
                    resource: parchmint_project_format::ResourceId::DocumentById {
                        document_id: stable_id_text(document.as_bytes()),
                    },
                    hash: summary.content_hash,
                },
            );
        }
    }
    metadata
}

fn patch_from_encoding(encoding: CanonicalProjectEncoding) -> CanonicalProjectPatch {
    CanonicalProjectPatch {
        complete_resources: encoding
            .resources
            .iter()
            .map(|(path, resource)| {
                (
                    path.clone(),
                    CanonicalResourceMetadata {
                        resource: resource.resource.clone(),
                        hash: resource.hash,
                    },
                )
            })
            .collect(),
        resources: encoding.resources,
        paths: encoding.paths,
        persistence_frontier: encoding.persistence_frontier,
        deletions: encoding.deletions,
    }
}

fn recovery_base_from_encoding(encoding: &CanonicalProjectEncoding) -> RecoveryBaseSnapshot {
    let mut hashes = encoding
        .resources
        .values()
        .map(|resource| (resource.resource.clone(), resource.hash))
        .collect::<BTreeMap<_, _>>();
    for document in encoding.resources.values() {
        let parchmint_project_format::ResourceId::DocumentById { document_id } = &document.resource
        else {
            continue;
        };
        let annotations = encoding.resources.values().find_map(|resource| {
            matches!(
                &resource.resource,
                parchmint_project_format::ResourceId::Annotations {
                    document_id: candidate
                } if candidate == document_id
            )
            .then_some(resource.bytes.as_slice())
        });
        hashes.insert(
            document.resource.clone(),
            recovery_document_content_hash(&document.bytes, annotations),
        );
    }
    RecoveryBaseSnapshot {
        revisions: parchmint_recovery_api::RecoveryRevisionVector::new(
            ProjectRevision::from(encoding.persistence_frontier.recovery_project_revision),
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
        hashes,
    }
}

fn recovery_base_from_patch(
    previous: &RecoveryBaseSnapshot,
    patch: &CanonicalProjectPatch,
) -> RecoveryBaseSnapshot {
    let retained = patch
        .complete_resources
        .values()
        .map(|resource| resource.resource.clone())
        .collect::<std::collections::BTreeSet<_>>();
    let mut hashes = previous.hashes.clone();
    hashes.retain(|resource, _| retained.contains(resource));
    for resource in patch.resources.values() {
        if !matches!(
            resource.resource,
            parchmint_project_format::ResourceId::DocumentById { .. }
        ) {
            hashes.insert(resource.resource.clone(), resource.hash);
        }
    }
    for document in patch.resources.values() {
        let parchmint_project_format::ResourceId::DocumentById { document_id } = &document.resource
        else {
            continue;
        };
        let annotations = patch.resources.values().find_map(|resource| {
            matches!(
                &resource.resource,
                parchmint_project_format::ResourceId::Annotations {
                    document_id: candidate
                } if candidate == document_id
            )
            .then_some(resource.bytes.as_slice())
        });
        hashes.insert(
            document.resource.clone(),
            recovery_document_content_hash(&document.bytes, annotations),
        );
    }
    RecoveryBaseSnapshot {
        revisions: parchmint_recovery_api::RecoveryRevisionVector::new(
            ProjectRevision::from(patch.persistence_frontier.recovery_project_revision),
            patch
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
        hashes,
    }
}

fn recovery_document_content_hash(body: &[u8], annotations: Option<&[u8]>) -> ContentHash {
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
    ContentHash::from_bytes(digest.finalize().into())
}

fn save_revisions(
    capture: &RevisionedSaveRequest,
    encoding: Option<&CanonicalProjectEncoding>,
) -> SaveRevisionVector {
    let mut closed_resources = capture
        .closed_documents
        .iter()
        .map(|(document, revision)| {
            (
                parchmint_project_format::ResourceId::DocumentById {
                    document_id: stable_id_text(document.as_bytes()),
                },
                ResourceRevision::from(revision.value()),
            )
        })
        .collect::<BTreeMap<_, _>>();
    for resource in capture.dirty_resources.iter() {
        match resource {
            Resource::Manifest => {
                closed_resources.insert(
                    parchmint_project_format::ResourceId::Manifest,
                    ResourceRevision::from(capture.project_revision.value()),
                );
            }
            Resource::Styles => {
                closed_resources.insert(
                    parchmint_project_format::ResourceId::Styles,
                    ResourceRevision::from(capture.project_revision.value()),
                );
            }
            Resource::Dictionary => {
                closed_resources.insert(
                    parchmint_project_format::ResourceId::Dictionary,
                    ResourceRevision::from(capture.project_revision.value()),
                );
            }
            Resource::Document(_) => {}
        }
    }
    let canonical_hashes = encoding.map_or_else(BTreeMap::new, |encoding| {
        encoding
            .resources
            .values()
            .map(|resource| (resource.resource.clone(), resource.hash))
            .collect()
    });
    SaveRevisionVector {
        project_revision: capture.project_revision,
        open_documents: capture
            .open_documents
            .iter()
            .map(|(document, revision)| {
                (
                    *document,
                    parchmint_recovery_api::DocumentRevision::from(revision.value()),
                )
            })
            .collect(),
        closed_resources,
        canonical_hashes,
        generation: SaveGeneration::from(capture.generation),
    }
}

fn save_revisions_from_patch(
    capture: &RevisionedSaveRequest,
    patch: &CanonicalProjectPatch,
) -> SaveRevisionVector {
    let mut revisions = save_revisions(capture, None);
    revisions.canonical_hashes = patch
        .complete_resources
        .values()
        .map(|resource| (resource.resource.clone(), resource.hash))
        .collect();
    revisions
}

fn materialize_patch_save_request(
    kind: PersistenceSaveKind,
    name: Option<SnapshotName>,
    capture: &RevisionedSaveRequest,
    revisions: SaveRevisionVector,
    patch: &CanonicalProjectPatch,
) -> SaveRequest {
    let writes = AtomicWritePlan::with_deletions(
        patch
            .resources
            .values()
            .map(|resource| StagedResource {
                path: resource.path.as_str().to_owned(),
                bytes: resource.bytes.clone(),
            })
            .collect(),
        patch
            .deletions
            .iter()
            .map(|path| path.as_str().to_owned())
            .collect(),
    );
    let resources = patch
        .complete_resources
        .iter()
        .map(|(path, resource)| (path.clone(), resource.hash))
        .collect();
    let category = match kind {
        PersistenceSaveKind::Autosave => CheckpointCategory::Autosave,
        PersistenceSaveKind::Structural => CheckpointCategory::StructuralChange,
        PersistenceSaveKind::Restoration => CheckpointCategory::Restoration,
        PersistenceSaveKind::NamedSnapshot => CheckpointCategory::NamedSnapshot,
        PersistenceSaveKind::Explicit | PersistenceSaveKind::Final => {
            CheckpointCategory::ExplicitSave
        }
    };
    let priority = match kind {
        PersistenceSaveKind::Autosave => SavePriority::Autosave,
        PersistenceSaveKind::Structural => SavePriority::Structural,
        PersistenceSaveKind::Explicit => SavePriority::Explicit,
        PersistenceSaveKind::Final => SavePriority::Close,
        PersistenceSaveKind::Restoration | PersistenceSaveKind::NamedSnapshot => {
            SavePriority::Explicit
        }
    };
    let affected_documents = if kind == PersistenceSaveKind::Restoration {
        patch
            .persistence_frontier
            .document_revisions
            .keys()
            .copied()
            .collect()
    } else {
        capture
            .dirty_resources
            .iter()
            .filter_map(|resource| match resource {
                Resource::Document(document) => Some(*document),
                Resource::Manifest | Resource::Styles | Resource::Dictionary => None,
            })
            .collect()
    };
    let checkpoint = CheckpointInput {
        intent_hash: checkpoint_patch_intent_hash(kind, capture, patch),
        resources,
        category,
        affected_documents,
        name,
        recorded_at_unix_millis: current_unix_millis(),
    };
    SaveRequest::new(revisions, writes, checkpoint, priority)
}

fn checkpoint_patch_intent_hash(
    kind: PersistenceSaveKind,
    capture: &RevisionedSaveRequest,
    patch: &CanonicalProjectPatch,
) -> CheckpointIntentHash {
    let mut digest = Sha256::new();
    digest.update(capture.project_id.as_bytes());
    digest.update(capture.generation.to_be_bytes());
    digest.update([kind as u8]);
    for (path, resource) in &patch.complete_resources {
        digest.update(path.as_str().as_bytes());
        digest.update(resource.hash.as_bytes());
    }
    for path in &patch.deletions {
        digest.update(b"delete\0");
        digest.update(path.as_str().as_bytes());
    }
    CheckpointIntentHash::from_bytes(digest.finalize().into())
}

fn materialize_save_request(
    kind: PersistenceSaveKind,
    name: Option<SnapshotName>,
    capture: &RevisionedSaveRequest,
    revisions: SaveRevisionVector,
    encoding: &CanonicalProjectEncoding,
) -> SaveRequest {
    let writes = AtomicWritePlan::with_deletions(
        encoding
            .resources
            .values()
            .map(|resource| StagedResource {
                path: resource.path.as_str().to_owned(),
                bytes: resource.bytes.clone(),
            })
            .collect(),
        encoding
            .deletions
            .iter()
            .map(|path| path.as_str().to_owned())
            .collect(),
    );
    let resources = encoding
        .resources
        .iter()
        .map(|(path, resource)| (path.clone(), resource.hash))
        .collect();
    let category = match kind {
        PersistenceSaveKind::Autosave => CheckpointCategory::Autosave,
        PersistenceSaveKind::Structural => CheckpointCategory::StructuralChange,
        PersistenceSaveKind::Restoration => CheckpointCategory::Restoration,
        PersistenceSaveKind::NamedSnapshot => CheckpointCategory::NamedSnapshot,
        PersistenceSaveKind::Explicit | PersistenceSaveKind::Final => {
            CheckpointCategory::ExplicitSave
        }
    };
    let priority = match kind {
        PersistenceSaveKind::Autosave => SavePriority::Autosave,
        PersistenceSaveKind::Structural => SavePriority::Structural,
        PersistenceSaveKind::Explicit => SavePriority::Explicit,
        PersistenceSaveKind::Final => SavePriority::Close,
        PersistenceSaveKind::Restoration => SavePriority::Explicit,
        PersistenceSaveKind::NamedSnapshot => SavePriority::Explicit,
    };
    let affected_documents = if kind == PersistenceSaveKind::Restoration {
        encoding
            .persistence_frontier
            .document_revisions
            .keys()
            .copied()
            .collect()
    } else {
        capture
            .dirty_resources
            .iter()
            .filter_map(|resource| match resource {
                Resource::Document(document) => Some(*document),
                Resource::Manifest | Resource::Styles | Resource::Dictionary => None,
            })
            .collect()
    };
    let checkpoint = CheckpointInput {
        intent_hash: checkpoint_intent_hash(kind, capture, encoding),
        resources,
        category,
        affected_documents,
        name,
        recorded_at_unix_millis: current_unix_millis(),
    };
    SaveRequest::new(revisions, writes, checkpoint, priority)
}

fn current_unix_millis() -> Option<u64> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|duration| u64::try_from(duration.as_millis()).ok())
}

fn checkpoint_intent_hash(
    kind: PersistenceSaveKind,
    capture: &RevisionedSaveRequest,
    encoding: &CanonicalProjectEncoding,
) -> CheckpointIntentHash {
    let mut digest = Sha256::new();
    digest.update(capture.project_id.as_bytes());
    digest.update(capture.generation.to_be_bytes());
    digest.update([kind as u8]);
    for (path, resource) in &encoding.resources {
        digest.update(path.as_str().as_bytes());
        digest.update(resource.hash.as_bytes());
    }
    for path in &encoding.deletions {
        digest.update(b"delete\0");
        digest.update(path.as_str().as_bytes());
    }
    CheckpointIntentHash::from_bytes(digest.finalize().into())
}

fn persistence_revision(revisions: &SaveRevisionVector) -> PersistenceRevision {
    let mut documents = revisions
        .open_documents
        .iter()
        .map(|(document, revision)| (*document, EditorRevision::from(revision.value())))
        .collect::<BTreeMap<_, _>>();
    for (resource, revision) in &revisions.closed_resources {
        if let parchmint_project_format::ResourceId::DocumentById { document_id } = resource
            && let Ok(id) = parse_stable_id(document_id)
        {
            documents.insert(
                DocumentId::from_bytes(id),
                EditorRevision::from(revision.value()),
            );
        }
    }
    PersistenceRevision {
        project_revision: revisions.project_revision,
        documents,
        generation: revisions.generation.value(),
    }
}

fn persistence_status(status: EditorPersistenceStatus) -> PersistenceStatus {
    PersistenceStatus {
        state: status.state,
        requested: status.requested.as_ref().map(persistence_revision),
        active: status.active.as_ref().map(persistence_revision),
        saved_through: status.saved_through.as_ref().map(persistence_revision),
        recovery_retained_records: status.recovery_retained_records,
        error: status.error.map(|error| error.to_string()),
    }
}

pub(crate) fn stable_id_text(bytes: &[u8; 16]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn parse_stable_id(value: &str) -> Result<[u8; 16], ProjectPersistenceError> {
    if value.len() != 32 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(ProjectPersistenceError::UnknownRecoveryAcceptance);
    }
    let mut bytes = [0; 16];
    for (index, byte) in bytes.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&value[index * 2..index * 2 + 2], 16)
            .map_err(|_| ProjectPersistenceError::UnknownRecoveryAcceptance)?;
    }
    Ok(bytes)
}

fn contract_thread(thread: &CanonicalComment) -> AnnotationThread {
    AnnotationThread {
        id: *thread.id.as_bytes(),
        messages: thread
            .messages
            .iter()
            .map(|message| AnnotationMessage {
                id: *message.id.as_bytes(),
                body: message.body.clone(),
                unknown_fields: message.unknown_fields.clone(),
            })
            .collect(),
        resolved: thread.resolved,
        anchor: match &thread.anchor {
            CanonicalCommentAnchor::Document { unknown_fields } => AnnotationAnchor::Document {
                unknown_fields: unknown_fields.clone(),
            },
            CanonicalCommentAnchor::Text {
                block,
                range,
                quote,
                context_before,
                context_after,
                orphaned,
                unknown_fields,
            } => AnnotationAnchor::Text {
                block: *block.as_bytes(),
                start: range.start().value(),
                end: range.end().value(),
                quote: quote.clone(),
                context_before: context_before.clone(),
                context_after: context_after.clone(),
                orphaned: *orphaned,
                unknown_fields: unknown_fields.clone(),
            },
        },
        unknown_fields: thread.unknown_fields.clone(),
    }
}

fn editor_thread_contract(thread: AnnotationThread) -> CanonicalComment {
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
                block: BlockId::from_bytes(block),
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
