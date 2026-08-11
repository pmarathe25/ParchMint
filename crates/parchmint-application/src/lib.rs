//! Application command ownership, undo routing, and revision capture.

mod editor_persistence;
mod project_persistence;

pub use editor_persistence::{EditorPersistenceCoordinator, EditorPersistenceStatus};
pub use project_persistence::*;

use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    error::Error,
    fmt,
    future::Future,
    pin::Pin,
    sync::{Arc, Mutex, MutexGuard},
};

pub use parchmint_domain::{
    DocumentId, DomainError, Project, ProjectCommand, ProjectId, ProjectOperationId,
    ProjectRevision, Resource, ResourceSet,
};
pub use parchmint_editor_api::{CanonicalProjection, EditorRevision};

const PROJECT_UNDO_LIMIT: usize = 100;
const PROJECT_UNDO_BYTE_LIMIT: usize = 64 * 1024 * 1024;

/// A future returned by application boundaries that may later become asynchronous.
pub type AppFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FocusTarget {
    Editor(DocumentId),
    Comment(DocumentId),
    Tree,
    Cards,
    Settings,
    Inspector,
    TextInput,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UndoDomain {
    Document(DocumentId),
    Project,
    TextInput,
}

impl FocusTarget {
    pub const fn undo_domain(self) -> UndoDomain {
        match self {
            Self::Editor(document) | Self::Comment(document) => UndoDomain::Document(document),
            Self::Tree | Self::Cards | Self::Settings | Self::Inspector => UndoDomain::Project,
            Self::TextInput => UndoDomain::TextInput,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UndoResetReason {
    ProjectClosed,
    RecoveryAccepted,
    MigrationCompleted,
    HistoryRestored,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CheckpointGroupId(u64);

impl CheckpointGroupId {
    pub const fn value(self) -> u64 {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RevisionRange {
    pub before: ProjectRevision,
    pub after: ProjectRevision,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProjectEvent {
    Executed,
    Undone,
    Redone,
    GlobalReplacementApplied { documents: usize },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DocumentVisibility {
    Open,
    Hidden,
    Closed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocumentSnapshot {
    pub document_id: DocumentId,
    pub body: String,
    pub revision: EditorRevision,
    pub visibility: DocumentVisibility,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocumentCommand {
    pub document_id: DocumentId,
    pub observed_revision: EditorRevision,
    pub body: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocumentCommandResult {
    pub document_id: DocumentId,
    pub revision: EditorRevision,
    pub opened_session: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplacementEdit {
    pub document_id: DocumentId,
    pub observed_revision: EditorRevision,
    pub expected_body: String,
    pub replacement_body: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplacementSelection {
    pub label: String,
    pub edits: Vec<ReplacementEdit>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocumentPatch {
    document_id: DocumentId,
    before: String,
    after: String,
}

impl DocumentPatch {
    pub fn new(document_id: DocumentId, before: String, after: String) -> Self {
        Self {
            document_id,
            before,
            after,
        }
    }

    pub const fn document_id(&self) -> DocumentId {
        self.document_id
    }

    pub fn before(&self) -> &str {
        &self.before
    }

    pub fn after(&self) -> &str {
        &self.after
    }
}

/// The complete forward and inverse data for one composite document operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocumentPatchSet {
    patches: Vec<DocumentPatch>,
}

impl DocumentPatchSet {
    pub fn try_from_patches(
        patches: impl IntoIterator<Item = DocumentPatch>,
    ) -> Result<Self, ApplicationError> {
        let patches = patches.into_iter().collect::<Vec<_>>();
        let mut seen = BTreeSet::new();
        for patch in &patches {
            if !seen.insert(patch.document_id) {
                return Err(ApplicationError::DuplicateDocument {
                    document: patch.document_id,
                });
            }
        }
        Ok(Self { patches })
    }

    pub fn patches(&self) -> &[DocumentPatch] {
        &self.patches
    }

    pub fn affected_documents(&self) -> impl Iterator<Item = DocumentId> + '_ {
        self.patches.iter().map(|patch| patch.document_id)
    }

    pub fn len(&self) -> usize {
        self.patches.len()
    }

    pub fn is_empty(&self) -> bool {
        self.patches.is_empty()
    }

    fn byte_cost(&self) -> usize {
        self.patches
            .iter()
            .map(|patch| patch.before.len() + patch.after.len())
            .sum()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PatchDirection {
    Forward,
    Inverse,
}

/// The document owner boundary used by the application coordinator.
///
/// `prepare_composite` is read-only. `apply_composite` must validate every
/// source state and publish all affected documents atomically, or publish none.
pub trait DocumentStateOwner: Send + Sync {
    fn execute(&self, command: DocumentCommand) -> Result<DocumentCommandResult, ApplicationError>;
    fn undo(&self, document: DocumentId) -> Result<DocumentCommandResult, ApplicationError>;
    fn redo(&self, document: DocumentId) -> Result<DocumentCommandResult, ApplicationError>;
    fn prepare_composite(
        &self,
        edits: &[ReplacementEdit],
    ) -> Result<DocumentPatchSet, ApplicationError>;
    fn apply_composite(
        &self,
        operation: ProjectOperationId,
        patch: &DocumentPatchSet,
        direction: PatchDirection,
    ) -> Result<BTreeMap<DocumentId, EditorRevision>, ApplicationError>;
    fn revisions(&self) -> Result<DocumentRevisionVector, ApplicationError>;
    fn reset_undo(&self, reason: UndoResetReason) -> Result<(), ApplicationError>;
    fn accept_projection(
        &self,
        document: DocumentId,
        revision: EditorRevision,
        body: String,
    ) -> Result<(), ApplicationError>;
    fn insert_document(&self, document: DocumentSnapshot) -> Result<(), ApplicationError>;
    fn replace_documents(&self, documents: Vec<DocumentSnapshot>) -> Result<(), ApplicationError>;
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DocumentRevisionVector {
    pub open: BTreeMap<DocumentId, EditorRevision>,
    pub closed: BTreeMap<DocumentId, EditorRevision>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ProjectPatch {
    Domain(ProjectCommand),
    Documents {
        patch: DocumentPatchSet,
        direction: PatchDirection,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct ProjectUndoEntry {
    pub operation_id: ProjectOperationId,
    pub label: String,
    pub forward: ProjectPatch,
    pub inverse: ProjectPatch,
    pub revisions: RevisionRange,
    pub affected: ResourceSet,
    pub byte_cost: usize,
    pub checkpoint_group: CheckpointGroupId,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProjectUndoState {
    pub can_undo: bool,
    pub can_redo: bool,
    pub undo_label: Option<String>,
    pub redo_label: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectCommandResult {
    pub operation_id: ProjectOperationId,
    pub revision: ProjectRevision,
    pub dirty_resources: ResourceSet,
    pub events: Vec<ProjectEvent>,
    pub checkpoint_group: CheckpointGroupId,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplacementPreview {
    pub affected_documents: Vec<DocumentId>,
    pub inverse_bytes: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RevisionedSaveRequest {
    pub project_id: ProjectId,
    pub project_revision: ProjectRevision,
    pub open_documents: BTreeMap<DocumentId, EditorRevision>,
    pub closed_documents: BTreeMap<DocumentId, EditorRevision>,
    pub dirty_resources: ResourceSet,
    pub checkpoint_groups: Vec<CheckpointGroupId>,
    pub generation: u64,
    pub mutation_generation: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FocusedUndoResult {
    Project(ProjectRevision),
    Document {
        document_id: DocumentId,
        revision: EditorRevision,
    },
    NativeTextInput,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApplicationError {
    Domain(DomainError),
    MissingDocument {
        document: DocumentId,
    },
    DuplicateDocument {
        document: DocumentId,
    },
    StaleDocument {
        document: DocumentId,
        observed: EditorRevision,
        current: EditorRevision,
    },
    ReplacementChanged {
        document: DocumentId,
    },
    CompositeApplyFailed {
        document: DocumentId,
    },
    ProjectUndoEmpty,
    ProjectRedoEmpty,
    DocumentUndoEmpty {
        document: DocumentId,
    },
    DocumentRedoEmpty {
        document: DocumentId,
    },
    StaleSaveAcknowledgement,
    StateUnavailable,
}

impl fmt::Display for ApplicationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Domain(error) => write!(formatter, "project command failed: {error}"),
            Self::MissingDocument { document } => {
                write!(formatter, "document {document:?} is missing")
            }
            Self::DuplicateDocument { document } => {
                write!(formatter, "document {document:?} appears more than once")
            }
            Self::StaleDocument {
                document,
                observed,
                current,
            } => write!(
                formatter,
                "document {document:?} observed revision {} but is at {}",
                observed.value(),
                current.value()
            ),
            Self::ReplacementChanged { document } => {
                write!(formatter, "replacement source changed for {document:?}")
            }
            Self::CompositeApplyFailed { document } => {
                write!(formatter, "composite apply failed at {document:?}")
            }
            Self::ProjectUndoEmpty => formatter.write_str("project undo is empty"),
            Self::ProjectRedoEmpty => formatter.write_str("project redo is empty"),
            Self::DocumentUndoEmpty { document } => {
                write!(formatter, "document undo is empty for {document:?}")
            }
            Self::DocumentRedoEmpty { document } => {
                write!(formatter, "document redo is empty for {document:?}")
            }
            Self::StaleSaveAcknowledgement => {
                formatter.write_str("save acknowledgement does not match a captured frontier")
            }
            Self::StateUnavailable => formatter.write_str("application state is unavailable"),
        }
    }
}

impl Error for ApplicationError {}

impl From<DomainError> for ApplicationError {
    fn from(error: DomainError) -> Self {
        Self::Domain(error)
    }
}

pub trait ProjectCommandDispatcher: Send + Sync {
    fn execute(
        &self,
        command: ProjectCommand,
    ) -> AppFuture<'_, Result<ProjectCommandResult, ApplicationError>>;
    fn undo(&self) -> AppFuture<'_, Result<ProjectCommandResult, ApplicationError>>;
    fn redo(&self) -> AppFuture<'_, Result<ProjectCommandResult, ApplicationError>>;
    fn undo_state(&self) -> ProjectUndoState;
    fn reset_undo(&self, reason: UndoResetReason);
}

pub trait GlobalReplacement: Send + Sync {
    fn preview(
        &self,
        selection: ReplacementSelection,
    ) -> AppFuture<'_, Result<ReplacementPreview, ApplicationError>>;
    fn apply(
        &self,
        selection: ReplacementSelection,
    ) -> AppFuture<'_, Result<ProjectCommandResult, ApplicationError>>;
}

#[derive(Debug, Clone)]
struct DocumentRecord {
    body: String,
    revision: EditorRevision,
    visibility: DocumentVisibility,
    undo: Vec<String>,
    redo: Vec<String>,
    project_boundaries: Vec<ProjectOperationId>,
}

#[derive(Debug, Default)]
struct NativeDocumentState {
    documents: BTreeMap<DocumentId, DocumentRecord>,
    #[cfg(test)]
    fail_composite_at: Option<DocumentId>,
}

/// Deterministic native document owner used until the editor integration stage.
///
/// Closed documents become hidden open sessions before a document command.
/// Composite project edits retain their visibility and do not touch document undo.
#[derive(Debug, Default)]
pub struct NativeDocumentStateOwner {
    state: Mutex<NativeDocumentState>,
}

impl NativeDocumentStateOwner {
    pub fn new(documents: impl IntoIterator<Item = DocumentSnapshot>) -> Self {
        let documents = documents
            .into_iter()
            .map(|document| {
                (
                    document.document_id,
                    DocumentRecord {
                        body: document.body,
                        revision: document.revision,
                        visibility: document.visibility,
                        undo: Vec::new(),
                        redo: Vec::new(),
                        project_boundaries: Vec::new(),
                    },
                )
            })
            .collect();
        Self {
            state: Mutex::new(NativeDocumentState {
                documents,
                #[cfg(test)]
                fail_composite_at: None,
            }),
        }
    }

    pub fn snapshot(&self, document: DocumentId) -> Result<DocumentSnapshot, ApplicationError> {
        let state = lock(&self.state)?;
        let record = state
            .documents
            .get(&document)
            .ok_or(ApplicationError::MissingDocument { document })?;
        Ok(DocumentSnapshot {
            document_id: document,
            body: record.body.clone(),
            revision: record.revision,
            visibility: record.visibility,
        })
    }

    pub fn snapshots(&self) -> Result<Vec<DocumentSnapshot>, ApplicationError> {
        let state = lock(&self.state)?;
        Ok(state
            .documents
            .iter()
            .map(|(document, record)| DocumentSnapshot {
                document_id: *document,
                body: record.body.clone(),
                revision: record.revision,
                visibility: record.visibility,
            })
            .collect())
    }

    pub fn document_undo_len(&self, document: DocumentId) -> Result<usize, ApplicationError> {
        let state = lock(&self.state)?;
        Ok(state
            .documents
            .get(&document)
            .ok_or(ApplicationError::MissingDocument { document })?
            .undo
            .len())
    }

    pub fn project_boundary_count(&self, document: DocumentId) -> Result<usize, ApplicationError> {
        let state = lock(&self.state)?;
        Ok(state
            .documents
            .get(&document)
            .ok_or(ApplicationError::MissingDocument { document })?
            .project_boundaries
            .len())
    }

    #[cfg(test)]
    fn fail_next_composite_at(&self, document: DocumentId) {
        self.state
            .lock()
            .expect("document state lock")
            .fail_composite_at = Some(document);
    }
}

impl DocumentStateOwner for NativeDocumentStateOwner {
    fn execute(&self, command: DocumentCommand) -> Result<DocumentCommandResult, ApplicationError> {
        let mut state = lock(&self.state)?;
        let record = state.documents.get_mut(&command.document_id).ok_or(
            ApplicationError::MissingDocument {
                document: command.document_id,
            },
        )?;
        if record.revision != command.observed_revision {
            return Err(ApplicationError::StaleDocument {
                document: command.document_id,
                observed: command.observed_revision,
                current: record.revision,
            });
        }
        let opened_session = record.visibility == DocumentVisibility::Closed;
        let previous = std::mem::replace(&mut record.body, command.body);
        if opened_session {
            record.visibility = DocumentVisibility::Hidden;
        }
        record.revision = record.revision.next();
        record.undo.push(previous);
        record.redo.clear();
        Ok(DocumentCommandResult {
            document_id: command.document_id,
            revision: record.revision,
            opened_session,
        })
    }

    fn undo(&self, document: DocumentId) -> Result<DocumentCommandResult, ApplicationError> {
        let mut state = lock(&self.state)?;
        let record = state
            .documents
            .get_mut(&document)
            .ok_or(ApplicationError::MissingDocument { document })?;
        let previous = record
            .undo
            .pop()
            .ok_or(ApplicationError::DocumentUndoEmpty { document })?;
        let current = std::mem::replace(&mut record.body, previous);
        record.redo.push(current);
        record.revision = record.revision.next();
        Ok(DocumentCommandResult {
            document_id: document,
            revision: record.revision,
            opened_session: false,
        })
    }

    fn redo(&self, document: DocumentId) -> Result<DocumentCommandResult, ApplicationError> {
        let mut state = lock(&self.state)?;
        let record = state
            .documents
            .get_mut(&document)
            .ok_or(ApplicationError::MissingDocument { document })?;
        let next = record
            .redo
            .pop()
            .ok_or(ApplicationError::DocumentRedoEmpty { document })?;
        let current = std::mem::replace(&mut record.body, next);
        record.undo.push(current);
        record.revision = record.revision.next();
        Ok(DocumentCommandResult {
            document_id: document,
            revision: record.revision,
            opened_session: false,
        })
    }

    fn prepare_composite(
        &self,
        edits: &[ReplacementEdit],
    ) -> Result<DocumentPatchSet, ApplicationError> {
        let state = lock(&self.state)?;
        let mut patches = Vec::with_capacity(edits.len());
        for edit in edits {
            let record = state.documents.get(&edit.document_id).ok_or(
                ApplicationError::MissingDocument {
                    document: edit.document_id,
                },
            )?;
            if record.revision != edit.observed_revision {
                return Err(ApplicationError::StaleDocument {
                    document: edit.document_id,
                    observed: edit.observed_revision,
                    current: record.revision,
                });
            }
            if record.body != edit.expected_body {
                return Err(ApplicationError::ReplacementChanged {
                    document: edit.document_id,
                });
            }
            patches.push(DocumentPatch::new(
                edit.document_id,
                record.body.clone(),
                edit.replacement_body.clone(),
            ));
        }
        DocumentPatchSet::try_from_patches(patches)
    }

    fn apply_composite(
        &self,
        operation: ProjectOperationId,
        patch: &DocumentPatchSet,
        direction: PatchDirection,
    ) -> Result<BTreeMap<DocumentId, EditorRevision>, ApplicationError> {
        let mut state = lock(&self.state)?;
        let mut draft = state.documents.clone();
        for change in &patch.patches {
            #[cfg(test)]
            if state.fail_composite_at == Some(change.document_id) {
                state.fail_composite_at = None;
                return Err(ApplicationError::CompositeApplyFailed {
                    document: change.document_id,
                });
            }
            let record =
                draft
                    .get_mut(&change.document_id)
                    .ok_or(ApplicationError::MissingDocument {
                        document: change.document_id,
                    })?;
            let (expected, replacement) = match direction {
                PatchDirection::Forward => (&change.before, &change.after),
                PatchDirection::Inverse => (&change.after, &change.before),
            };
            if &record.body != expected {
                return Err(ApplicationError::ReplacementChanged {
                    document: change.document_id,
                });
            }
            record.body.clone_from(replacement);
            record.revision = record.revision.next();
            record.project_boundaries.push(operation);
        }
        let revisions = patch
            .patches
            .iter()
            .map(|change| (change.document_id, draft[&change.document_id].revision))
            .collect();
        state.documents = draft;
        Ok(revisions)
    }

    fn revisions(&self) -> Result<DocumentRevisionVector, ApplicationError> {
        let state = lock(&self.state)?;
        let mut revisions = DocumentRevisionVector::default();
        for (document, record) in &state.documents {
            match record.visibility {
                DocumentVisibility::Open | DocumentVisibility::Hidden => {
                    revisions.open.insert(*document, record.revision);
                }
                DocumentVisibility::Closed => {
                    revisions.closed.insert(*document, record.revision);
                }
            }
        }
        Ok(revisions)
    }

    fn reset_undo(&self, _reason: UndoResetReason) -> Result<(), ApplicationError> {
        let mut state = lock(&self.state)?;
        for record in state.documents.values_mut() {
            record.undo.clear();
            record.redo.clear();
        }
        Ok(())
    }

    fn accept_projection(
        &self,
        document: DocumentId,
        revision: EditorRevision,
        body: String,
    ) -> Result<(), ApplicationError> {
        let mut state = lock(&self.state)?;
        let record = state
            .documents
            .get_mut(&document)
            .ok_or(ApplicationError::MissingDocument { document })?;
        if revision < record.revision || (revision == record.revision && body != record.body) {
            return Err(ApplicationError::StaleDocument {
                document,
                observed: revision,
                current: record.revision,
            });
        }
        record.revision = revision;
        record.body = body;
        Ok(())
    }

    fn insert_document(&self, document: DocumentSnapshot) -> Result<(), ApplicationError> {
        let mut state = lock(&self.state)?;
        if state.documents.contains_key(&document.document_id) {
            return Err(ApplicationError::DuplicateDocument {
                document: document.document_id,
            });
        }
        state.documents.insert(
            document.document_id,
            DocumentRecord {
                body: document.body,
                revision: document.revision,
                visibility: document.visibility,
                undo: Vec::new(),
                redo: Vec::new(),
                project_boundaries: Vec::new(),
            },
        );
        Ok(())
    }

    fn replace_documents(&self, documents: Vec<DocumentSnapshot>) -> Result<(), ApplicationError> {
        let mut replacement = BTreeMap::new();
        for document in documents {
            if replacement
                .insert(
                    document.document_id,
                    DocumentRecord {
                        body: document.body,
                        revision: document.revision,
                        visibility: document.visibility,
                        undo: Vec::new(),
                        redo: Vec::new(),
                        project_boundaries: Vec::new(),
                    },
                )
                .is_some()
            {
                return Err(ApplicationError::DuplicateDocument {
                    document: document.document_id,
                });
            }
        }
        lock(&self.state)?.documents = replacement;
        Ok(())
    }
}

struct DispatcherState {
    project: Project,
    undo: VecDeque<ProjectUndoEntry>,
    redo: Vec<ProjectUndoEntry>,
    undo_bytes: usize,
    dirty: BTreeMap<Resource, u64>,
    pending_checkpoints: Vec<(CheckpointGroupId, u64)>,
    captured_saves: BTreeMap<u64, RevisionedSaveRequest>,
    next_operation: u64,
    next_checkpoint: u64,
    save_generation: u64,
    mutation_generation: u64,
}

impl DispatcherState {
    fn operation_id(&mut self) -> ProjectOperationId {
        self.next_operation = self.next_operation.saturating_add(1);
        let mut bytes = [0_u8; 16];
        bytes[8..].copy_from_slice(&self.next_operation.to_be_bytes());
        ProjectOperationId::from_bytes(bytes)
    }

    fn checkpoint_group(&mut self) -> CheckpointGroupId {
        self.next_checkpoint = self.next_checkpoint.saturating_add(1);
        CheckpointGroupId(self.next_checkpoint)
    }

    fn stage_checkpoint(&mut self, mutation_generation: u64) -> CheckpointGroupId {
        let checkpoint = self.checkpoint_group();
        self.pending_checkpoints
            .push((checkpoint, mutation_generation));
        checkpoint
    }

    fn mark_dirty(&mut self, resources: &ResourceSet) -> u64 {
        self.mutation_generation = self.mutation_generation.saturating_add(1);
        let generation = self.mutation_generation;
        for resource in resources.iter() {
            self.dirty.insert(*resource, generation);
        }
        generation
    }

    fn mark_document_dirty(&mut self, document: DocumentId) -> u64 {
        self.mutation_generation = self.mutation_generation.saturating_add(1);
        let generation = self.mutation_generation;
        self.dirty.insert(Resource::Document(document), generation);
        generation
    }

    fn push_undo(&mut self, entry: ProjectUndoEntry) {
        self.undo_bytes = self.undo_bytes.saturating_add(entry.byte_cost);
        self.undo.push_back(entry);
        while self.undo.len() > PROJECT_UNDO_LIMIT || self.undo_bytes > PROJECT_UNDO_BYTE_LIMIT {
            let Some(evicted) = self.undo.pop_front() else {
                break;
            };
            self.undo_bytes = self.undo_bytes.saturating_sub(evicted.byte_cost);
        }
    }
}

pub struct NativeProjectCommandDispatcher {
    state: Mutex<DispatcherState>,
    documents: Arc<dyn DocumentStateOwner>,
}

impl fmt::Debug for NativeProjectCommandDispatcher {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("NativeProjectCommandDispatcher")
            .finish_non_exhaustive()
    }
}

impl NativeProjectCommandDispatcher {
    pub fn new(project: Project, documents: Arc<dyn DocumentStateOwner>) -> Self {
        Self {
            state: Mutex::new(DispatcherState {
                project,
                undo: VecDeque::new(),
                redo: Vec::new(),
                undo_bytes: 0,
                dirty: BTreeMap::new(),
                pending_checkpoints: Vec::new(),
                captured_saves: BTreeMap::new(),
                next_operation: 0,
                next_checkpoint: 0,
                save_generation: 0,
                mutation_generation: 0,
            }),
            documents,
        }
    }

    pub fn project(&self) -> Result<Project, ApplicationError> {
        Ok(lock(&self.state)?.project.clone())
    }

    pub fn project_undo_entries(&self) -> Result<Vec<ProjectUndoEntry>, ApplicationError> {
        Ok(lock(&self.state)?.undo.iter().cloned().collect())
    }

    pub fn pending_checkpoints(&self) -> Result<Vec<CheckpointGroupId>, ApplicationError> {
        Ok(lock(&self.state)?
            .pending_checkpoints
            .iter()
            .map(|(checkpoint, _)| *checkpoint)
            .collect())
    }

    pub fn execute_document(
        &self,
        command: DocumentCommand,
    ) -> Result<DocumentCommandResult, ApplicationError> {
        let mut state = lock(&self.state)?;
        let result = self.documents.execute(command)?;
        let mutation = state.mark_document_dirty(result.document_id);
        state.stage_checkpoint(mutation);
        Ok(result)
    }

    pub fn accept_editor_projection(
        &self,
        projection: &CanonicalProjection,
    ) -> Result<(), ApplicationError> {
        self.documents.accept_projection(
            projection.document_id(),
            projection.revision(),
            projection.body().to_owned(),
        )?;
        let mut state = lock(&self.state)?;
        let mutation = state.mark_document_dirty(projection.document_id());
        state.stage_checkpoint(mutation);
        Ok(())
    }

    pub fn undo_focused(&self, focus: FocusTarget) -> Result<FocusedUndoResult, ApplicationError> {
        self.edit_focused(focus, HistoryDirection::Undo)
    }

    pub fn redo_focused(&self, focus: FocusTarget) -> Result<FocusedUndoResult, ApplicationError> {
        self.edit_focused(focus, HistoryDirection::Redo)
    }

    fn edit_focused(
        &self,
        focus: FocusTarget,
        direction: HistoryDirection,
    ) -> Result<FocusedUndoResult, ApplicationError> {
        match focus.undo_domain() {
            UndoDomain::Project => direction
                .edit_project(self)
                .map(|result| FocusedUndoResult::Project(result.revision)),
            UndoDomain::Document(document) => {
                let mut state = lock(&self.state)?;
                let result = direction.edit_document(self.documents.as_ref(), document)?;
                let mutation = state.mark_document_dirty(document);
                state.stage_checkpoint(mutation);
                Ok(FocusedUndoResult::Document {
                    document_id: document,
                    revision: result.revision,
                })
            }
            UndoDomain::TextInput => Ok(FocusedUndoResult::NativeTextInput),
        }
    }

    pub fn capture_save_request(&self) -> Result<RevisionedSaveRequest, ApplicationError> {
        let mut state = lock(&self.state)?;
        let revisions = self.documents.revisions()?;
        state.save_generation = state.save_generation.saturating_add(1);
        let mut dirty_resources = ResourceSet::default();
        for resource in state.dirty.keys() {
            dirty_resources.insert(*resource);
        }
        let request = RevisionedSaveRequest {
            project_id: state.project.id,
            project_revision: state.project.revision,
            open_documents: revisions.open,
            closed_documents: revisions.closed,
            dirty_resources,
            checkpoint_groups: state
                .pending_checkpoints
                .iter()
                .map(|(checkpoint, _)| *checkpoint)
                .collect(),
            generation: state.save_generation,
            mutation_generation: state.mutation_generation,
        };
        state
            .captured_saves
            .insert(request.generation, request.clone());
        Ok(request)
    }

    pub(crate) fn recovery_revision_request(
        &self,
    ) -> Result<RevisionedSaveRequest, ApplicationError> {
        let state = lock(&self.state)?;
        let revisions = self.documents.revisions()?;
        let mut dirty_resources = ResourceSet::default();
        for resource in state.dirty.keys() {
            dirty_resources.insert(*resource);
        }
        Ok(RevisionedSaveRequest {
            project_id: state.project.id,
            project_revision: state.project.revision,
            open_documents: revisions.open,
            closed_documents: revisions.closed,
            dirty_resources,
            checkpoint_groups: state
                .pending_checkpoints
                .iter()
                .map(|(checkpoint, _)| *checkpoint)
                .collect(),
            generation: state.mutation_generation,
            mutation_generation: state.mutation_generation,
        })
    }

    /// Retires only resources and checkpoint groups unchanged since this
    /// exact captured save frontier. Later mutations remain dirty.
    pub fn acknowledge_save(
        &self,
        saved: &RevisionedSaveRequest,
    ) -> Result<ResourceSet, ApplicationError> {
        let mut state = lock(&self.state)?;
        if state.captured_saves.remove(&saved.generation).as_ref() != Some(saved) {
            return Err(ApplicationError::StaleSaveAcknowledgement);
        }
        state.dirty.retain(|resource, generation| {
            !saved.dirty_resources.contains(*resource) || *generation > saved.mutation_generation
        });
        state
            .pending_checkpoints
            .retain(|(_, generation)| *generation > saved.mutation_generation);
        let mut remaining = ResourceSet::default();
        for resource in state.dirty.keys() {
            remaining.insert(*resource);
        }
        Ok(remaining)
    }

    /// Publishes a fully validated restored snapshot at one application
    /// boundary. Document replacement succeeds before the project becomes
    /// visible; all remaining state changes are infallible while locked.
    pub fn publish_restored_state(
        &self,
        project: Project,
        documents: Vec<DocumentSnapshot>,
    ) -> Result<(), ApplicationError> {
        project.validate()?;
        let mut state = lock(&self.state)?;
        self.documents.replace_documents(documents)?;
        state.project = project;
        state.undo.clear();
        state.redo.clear();
        state.undo_bytes = 0;
        state.dirty.clear();
        state.pending_checkpoints.clear();
        state.captured_saves.clear();
        Ok(())
    }

    fn execute_now(
        &self,
        command: ProjectCommand,
    ) -> Result<ProjectCommandResult, ApplicationError> {
        let mut state = lock(&self.state)?;
        let before = state.project.revision;
        let forward = command.clone();
        let applied = parchmint_domain::apply_project_command(&state.project, before, command)?;

        // A document node and its canonical editor state are one application
        // mutation. The project lock remains held until both are published, so
        // snapshot queries can never observe a node without its default body.
        if let ProjectCommand::CreateDocument { document_id, .. } = &forward {
            self.documents.insert_document(DocumentSnapshot {
                document_id: *document_id,
                body: "<p></p>".to_owned(),
                revision: EditorRevision::default(),
                visibility: DocumentVisibility::Open,
            })?;
        }
        let operation_id = state.operation_id();
        let mutation = state.mark_dirty(&applied.changed_resources);
        let checkpoint_group = state.stage_checkpoint(mutation);
        let label = command_label(&forward).to_owned();
        let inverse = applied.inverse.clone();
        let byte_cost = patch_byte_cost(&forward) + patch_byte_cost(&inverse);
        let entry = ProjectUndoEntry {
            operation_id,
            label,
            forward: ProjectPatch::Domain(forward),
            inverse: ProjectPatch::Domain(inverse),
            revisions: RevisionRange {
                before,
                after: applied.project.revision,
            },
            affected: applied.changed_resources.clone(),
            byte_cost,
            checkpoint_group,
        };
        state.project = applied.project;
        state.redo.clear();
        state.push_undo(entry);
        Ok(ProjectCommandResult {
            operation_id,
            revision: state.project.revision,
            dirty_resources: applied.changed_resources,
            events: vec![ProjectEvent::Executed],
            checkpoint_group,
        })
    }

    fn apply_replacement_now(
        &self,
        selection: ReplacementSelection,
    ) -> Result<ProjectCommandResult, ApplicationError> {
        let mut state = lock(&self.state)?;
        let patch = self.documents.prepare_composite(&selection.edits)?;
        let before = state.project.revision;
        let after = before.next();
        let operation_id = state.operation_id();
        let affected = affected_documents(&patch);
        let forward = ProjectPatch::Documents {
            patch: patch.clone(),
            direction: PatchDirection::Forward,
        };
        let inverse = ProjectPatch::Documents {
            patch: patch.clone(),
            direction: PatchDirection::Inverse,
        };
        let byte_cost = patch.byte_cost();

        // The complete forward and inverse patches exist before this single
        // atomic publication point. All remaining application updates are
        // infallible while the project lock is held.
        self.documents
            .apply_composite(operation_id, &patch, PatchDirection::Forward)?;
        state.project.revision = after;
        let mutation = state.mark_dirty(&affected);
        state.redo.clear();
        let checkpoint_group = state.stage_checkpoint(mutation);
        let entry = ProjectUndoEntry {
            operation_id,
            label: selection.label,
            forward,
            inverse,
            revisions: RevisionRange { before, after },
            affected: affected.clone(),
            byte_cost,
            checkpoint_group,
        };
        state.push_undo(entry);
        Ok(ProjectCommandResult {
            operation_id,
            revision: after,
            dirty_resources: affected,
            events: vec![ProjectEvent::GlobalReplacementApplied {
                documents: patch.len(),
            }],
            checkpoint_group,
        })
    }

    fn undo_now(&self) -> Result<ProjectCommandResult, ApplicationError> {
        let mut state = lock(&self.state)?;
        let entry = state
            .undo
            .back()
            .cloned()
            .ok_or(ApplicationError::ProjectUndoEmpty)?;
        let operation_id = state.operation_id();
        let (project, affected) = self.apply_patch(&state.project, operation_id, &entry.inverse)?;
        let removed = state.undo.pop_back().expect("undo entry was observed");
        state.undo_bytes = state.undo_bytes.saturating_sub(removed.byte_cost);
        state.project = project;
        let mutation = state.mark_dirty(&affected);
        state.redo.push(entry);
        let checkpoint_group = state.stage_checkpoint(mutation);
        Ok(ProjectCommandResult {
            operation_id,
            revision: state.project.revision,
            dirty_resources: affected,
            events: vec![ProjectEvent::Undone],
            checkpoint_group,
        })
    }

    fn redo_now(&self) -> Result<ProjectCommandResult, ApplicationError> {
        let mut state = lock(&self.state)?;
        let entry = state
            .redo
            .last()
            .cloned()
            .ok_or(ApplicationError::ProjectRedoEmpty)?;
        let operation_id = state.operation_id();
        let (project, affected) = self.apply_patch(&state.project, operation_id, &entry.forward)?;
        state.redo.pop();
        state.project = project;
        let mutation = state.mark_dirty(&affected);
        let checkpoint_group = state.stage_checkpoint(mutation);
        state.push_undo(entry);
        Ok(ProjectCommandResult {
            operation_id,
            revision: state.project.revision,
            dirty_resources: affected,
            events: vec![ProjectEvent::Redone],
            checkpoint_group,
        })
    }

    fn apply_patch(
        &self,
        project: &Project,
        operation_id: ProjectOperationId,
        patch: &ProjectPatch,
    ) -> Result<(Project, ResourceSet), ApplicationError> {
        match patch {
            ProjectPatch::Domain(command) => {
                let applied = parchmint_domain::apply_project_command(
                    project,
                    project.revision,
                    command.clone(),
                )?;
                Ok((applied.project, applied.changed_resources))
            }
            ProjectPatch::Documents { patch, direction } => {
                let affected = affected_documents(patch);
                self.documents
                    .apply_composite(operation_id, patch, *direction)?;
                let mut project = project.clone();
                project.revision = project.revision.next();
                Ok((project, affected))
            }
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum HistoryDirection {
    Undo,
    Redo,
}

impl HistoryDirection {
    fn edit_project(
        self,
        dispatcher: &NativeProjectCommandDispatcher,
    ) -> Result<ProjectCommandResult, ApplicationError> {
        match self {
            Self::Undo => dispatcher.undo_now(),
            Self::Redo => dispatcher.redo_now(),
        }
    }

    fn edit_document(
        self,
        documents: &dyn DocumentStateOwner,
        document: DocumentId,
    ) -> Result<DocumentCommandResult, ApplicationError> {
        match self {
            Self::Undo => documents.undo(document),
            Self::Redo => documents.redo(document),
        }
    }
}

impl ProjectCommandDispatcher for NativeProjectCommandDispatcher {
    fn execute(
        &self,
        command: ProjectCommand,
    ) -> AppFuture<'_, Result<ProjectCommandResult, ApplicationError>> {
        Box::pin(async move { self.execute_now(command) })
    }

    fn undo(&self) -> AppFuture<'_, Result<ProjectCommandResult, ApplicationError>> {
        Box::pin(async move { self.undo_now() })
    }

    fn redo(&self) -> AppFuture<'_, Result<ProjectCommandResult, ApplicationError>> {
        Box::pin(async move { self.redo_now() })
    }

    fn undo_state(&self) -> ProjectUndoState {
        let Ok(state) = self.state.lock() else {
            return ProjectUndoState::default();
        };
        ProjectUndoState {
            can_undo: !state.undo.is_empty(),
            can_redo: !state.redo.is_empty(),
            undo_label: state.undo.back().map(|entry| entry.label.clone()),
            redo_label: state.redo.last().map(|entry| entry.label.clone()),
        }
    }

    fn reset_undo(&self, reason: UndoResetReason) {
        if let Ok(mut state) = self.state.lock()
            && self.documents.reset_undo(reason).is_ok()
        {
            state.undo.clear();
            state.redo.clear();
            state.undo_bytes = 0;
        }
    }
}

impl GlobalReplacement for NativeProjectCommandDispatcher {
    fn preview(
        &self,
        selection: ReplacementSelection,
    ) -> AppFuture<'_, Result<ReplacementPreview, ApplicationError>> {
        Box::pin(async move {
            self.documents
                .prepare_composite(&selection.edits)
                .map(|patch| ReplacementPreview {
                    affected_documents: patch.affected_documents().collect(),
                    inverse_bytes: patch.byte_cost(),
                })
        })
    }

    fn apply(
        &self,
        selection: ReplacementSelection,
    ) -> AppFuture<'_, Result<ProjectCommandResult, ApplicationError>> {
        Box::pin(async move { self.apply_replacement_now(selection) })
    }
}

fn lock<T>(mutex: &Mutex<T>) -> Result<MutexGuard<'_, T>, ApplicationError> {
    mutex.lock().map_err(|_| ApplicationError::StateUnavailable)
}

fn patch_byte_cost(command: &ProjectCommand) -> usize {
    format!("{command:?}").len()
}

fn affected_documents(patch: &DocumentPatchSet) -> ResourceSet {
    patch
        .affected_documents()
        .fold(ResourceSet::default(), |mut resources, document| {
            resources.insert(Resource::Document(document));
            resources
        })
}

fn command_label(command: &ProjectCommand) -> &'static str {
    match command {
        ProjectCommand::CreateGroup { .. } => "Create Group",
        ProjectCommand::CreateDocument { .. } => "Create Document",
        ProjectCommand::DeleteNode { .. } => "Delete",
        ProjectCommand::RestoreDeleted { .. } => "Restore Deleted",
        ProjectCommand::MoveNode { .. } => "Move",
        ProjectCommand::RenameNode { .. } => "Rename",
        ProjectCommand::CopyNode { .. } => "Copy",
        ProjectCommand::SetSynopsis { .. } => "Edit Synopsis",
        ProjectCommand::SetNodeExportSettings { .. } => "Edit Export Settings",
        ProjectCommand::SetMetadataValue { .. } => "Edit Metadata",
        ProjectCommand::UpsertMetadataField { .. } => "Edit Metadata Field",
        ProjectCommand::DeleteMetadataField { .. } => "Delete Metadata Field",
        ProjectCommand::MoveMetadataField { .. } => "Move Metadata Field",
        ProjectCommand::UpsertStyle { .. } => "Edit Style",
        ProjectCommand::DeleteStyle { .. } => "Delete Style",
        ProjectCommand::AddDictionaryWord { .. } => "Add Dictionary Word",
        ProjectCommand::RemoveDictionaryWord { .. } => "Remove Dictionary Word",
        ProjectCommand::SetProjectExportSettings { .. } => "Edit Project Export Settings",
        ProjectCommand::RestoreState(_) => "Restore Project State",
    }
}

#[cfg(test)]
mod application_contract_tests;
