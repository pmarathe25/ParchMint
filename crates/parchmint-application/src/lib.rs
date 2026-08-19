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
pub use parchmint_editor_api::{
    CanonicalComment, CanonicalCommentAnchor, CanonicalProjection, EditorRevision,
};

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
    Unchanged,
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
    pub comments: Vec<CanonicalComment>,
    pub revision: EditorRevision,
    pub visibility: DocumentVisibility,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LazyDocumentSummary {
    pub document_id: DocumentId,
    pub revision: EditorRevision,
    pub visibility: DocumentVisibility,
}

/// One coherent view of the current authored project and its live documents.
///
/// The dispatcher captures these fields while holding the project operation
/// boundary. Document records retained for undo after deletion are not part of
/// the live authored snapshot.
#[derive(Debug, Clone, PartialEq)]
pub struct AuthoredProjectSnapshot {
    pub project: Project,
    pub document_summaries: Vec<LazyDocumentSummary>,
    pub documents: Vec<DocumentSnapshot>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DocumentStateSnapshot {
    summaries: Vec<LazyDocumentSummary>,
    documents: Vec<DocumentSnapshot>,
}

/// Session-scoped authority for materializing one canonical document body.
pub trait DocumentSnapshotLoader: Send + Sync {
    fn load(&self, document: DocumentId) -> Result<DocumentSnapshot, ApplicationError>;
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
        comments: Vec<CanonicalComment>,
    ) -> Result<bool, ApplicationError>;
    fn insert_document(&self, document: DocumentSnapshot) -> Result<(), ApplicationError>;
    fn replace_documents(&self, documents: Vec<DocumentSnapshot>) -> Result<(), ApplicationError>;
    fn state_snapshot(&self, complete: bool) -> Result<DocumentStateSnapshot, ApplicationError>;
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
    pub checkpoint_group: Option<CheckpointGroupId>,
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
    DocumentLoad {
        document: DocumentId,
        reason: String,
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
            Self::DocumentLoad { document, reason } => {
                write!(formatter, "could not load document {document:?}: {reason}")
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
    comments: Vec<CanonicalComment>,
    revision: EditorRevision,
    visibility: DocumentVisibility,
    undo: Vec<String>,
    redo: Vec<String>,
    project_boundaries: Vec<ProjectOperationId>,
}

#[derive(Default)]
struct NativeDocumentState {
    documents: BTreeMap<DocumentId, DocumentRecord>,
    unloaded: BTreeMap<DocumentId, LazyDocumentSummary>,
    loader: Option<Arc<dyn DocumentSnapshotLoader>>,
    #[cfg(test)]
    fail_composite_at: Option<DocumentId>,
}

/// Deterministic native document owner used until the editor integration stage.
///
/// Closed documents become hidden open sessions before a document command.
/// Composite project edits retain their visibility and do not touch document undo.
#[derive(Default)]
pub struct NativeDocumentStateOwner {
    state: Mutex<NativeDocumentState>,
}

impl fmt::Debug for NativeDocumentStateOwner {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("NativeDocumentStateOwner")
            .finish_non_exhaustive()
    }
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
                        comments: document.comments,
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
                unloaded: BTreeMap::new(),
                loader: None,
                #[cfg(test)]
                fail_composite_at: None,
            }),
        }
    }

    pub fn new_lazy(
        summaries: impl IntoIterator<Item = LazyDocumentSummary>,
        loader: Arc<dyn DocumentSnapshotLoader>,
    ) -> Result<Self, ApplicationError> {
        let mut unloaded = BTreeMap::new();
        for summary in summaries {
            if unloaded.insert(summary.document_id, summary).is_some() {
                return Err(ApplicationError::DuplicateDocument {
                    document: summary.document_id,
                });
            }
        }
        Ok(Self {
            state: Mutex::new(NativeDocumentState {
                documents: BTreeMap::new(),
                unloaded,
                loader: Some(loader),
                #[cfg(test)]
                fail_composite_at: None,
            }),
        })
    }

    fn ensure_loaded(&self, document: DocumentId) -> Result<(), ApplicationError> {
        let (summary, loader) = {
            let state = lock(&self.state)?;
            if state.documents.contains_key(&document) {
                return Ok(());
            }
            let summary = state
                .unloaded
                .get(&document)
                .copied()
                .ok_or(ApplicationError::MissingDocument { document })?;
            let loader = state
                .loader
                .clone()
                .ok_or(ApplicationError::MissingDocument { document })?;
            (summary, loader)
        };
        let mut snapshot = loader.load(document)?;
        if snapshot.document_id != document || snapshot.revision != summary.revision {
            return Err(ApplicationError::DocumentLoad {
                document,
                reason: "canonical body does not match its persisted summary revision".into(),
            });
        }
        snapshot.visibility = summary.visibility;
        let mut state = lock(&self.state)?;
        if state.documents.contains_key(&document) {
            return Ok(());
        }
        state
            .unloaded
            .remove(&document)
            .ok_or(ApplicationError::MissingDocument { document })?;
        state.documents.insert(
            document,
            DocumentRecord {
                body: snapshot.body,
                comments: snapshot.comments,
                revision: snapshot.revision,
                visibility: snapshot.visibility,
                undo: Vec::new(),
                redo: Vec::new(),
                project_boundaries: Vec::new(),
            },
        );
        Ok(())
    }

    pub fn loaded_snapshots(&self) -> Result<Vec<DocumentSnapshot>, ApplicationError> {
        let state = lock(&self.state)?;
        Ok(state
            .documents
            .iter()
            .map(|(document, record)| DocumentSnapshot {
                document_id: *document,
                body: record.body.clone(),
                comments: record.comments.clone(),
                revision: record.revision,
                visibility: record.visibility,
            })
            .collect())
    }

    pub fn summaries(&self) -> Result<Vec<LazyDocumentSummary>, ApplicationError> {
        let state = lock(&self.state)?;
        let loaded = state
            .documents
            .iter()
            .map(|(document, record)| LazyDocumentSummary {
                document_id: *document,
                revision: record.revision,
                visibility: record.visibility,
            });
        Ok(loaded.chain(state.unloaded.values().copied()).collect())
    }

    pub fn snapshot(&self, document: DocumentId) -> Result<DocumentSnapshot, ApplicationError> {
        self.ensure_loaded(document)?;
        let state = lock(&self.state)?;
        let record = state
            .documents
            .get(&document)
            .ok_or(ApplicationError::MissingDocument { document })?;
        Ok(DocumentSnapshot {
            document_id: document,
            body: record.body.clone(),
            comments: record.comments.clone(),
            revision: record.revision,
            visibility: record.visibility,
        })
    }

    pub fn snapshots(&self) -> Result<Vec<DocumentSnapshot>, ApplicationError> {
        let documents = self
            .summaries()?
            .into_iter()
            .map(|summary| summary.document_id)
            .collect::<Vec<_>>();
        for document in documents {
            self.ensure_loaded(document)?;
        }
        self.loaded_snapshots()
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
        self.ensure_loaded(command.document_id)?;
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
        if record.body == command.body {
            if opened_session {
                record.visibility = DocumentVisibility::Hidden;
            }
            return Ok(DocumentCommandResult {
                document_id: command.document_id,
                revision: record.revision,
                opened_session,
            });
        }
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
        self.ensure_loaded(document)?;
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
        self.ensure_loaded(document)?;
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
        for edit in edits {
            self.ensure_loaded(edit.document_id)?;
        }
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
        for change in &patch.patches {
            self.ensure_loaded(change.document_id)?;
        }
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
            for comment in &mut record.comments {
                if let CanonicalCommentAnchor::Text { orphaned, .. } = &mut comment.anchor {
                    *orphaned = true;
                }
            }
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
        for (document, summary) in &state.unloaded {
            match summary.visibility {
                DocumentVisibility::Open | DocumentVisibility::Hidden => {
                    revisions.open.insert(*document, summary.revision);
                }
                DocumentVisibility::Closed => {
                    revisions.closed.insert(*document, summary.revision);
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
        comments: Vec<CanonicalComment>,
    ) -> Result<bool, ApplicationError> {
        self.ensure_loaded(document)?;
        let mut state = lock(&self.state)?;
        let record = state
            .documents
            .get_mut(&document)
            .ok_or(ApplicationError::MissingDocument { document })?;
        if revision < record.revision
            || (revision == record.revision && (body != record.body || comments != record.comments))
        {
            return Err(ApplicationError::StaleDocument {
                document,
                observed: revision,
                current: record.revision,
            });
        }
        let changed =
            revision > record.revision || body != record.body || comments != record.comments;
        record.revision = revision;
        record.body = body;
        record.comments = comments;
        Ok(changed)
    }

    fn insert_document(&self, document: DocumentSnapshot) -> Result<(), ApplicationError> {
        let mut state = lock(&self.state)?;
        if state.documents.contains_key(&document.document_id)
            || state.unloaded.contains_key(&document.document_id)
        {
            return Err(ApplicationError::DuplicateDocument {
                document: document.document_id,
            });
        }
        state.documents.insert(
            document.document_id,
            DocumentRecord {
                body: document.body,
                comments: document.comments,
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
                        comments: document.comments,
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
        let mut state = lock(&self.state)?;
        state.documents = replacement;
        state.unloaded.clear();
        state.loader = None;
        Ok(())
    }

    fn state_snapshot(&self, complete: bool) -> Result<DocumentStateSnapshot, ApplicationError> {
        if complete {
            let documents = NativeDocumentStateOwner::snapshots(self)?;
            let summaries = documents
                .iter()
                .map(|document| LazyDocumentSummary {
                    document_id: document.document_id,
                    revision: document.revision,
                    visibility: document.visibility,
                })
                .collect();
            Ok(DocumentStateSnapshot {
                summaries,
                documents,
            })
        } else {
            let state = lock(&self.state)?;
            let documents = state
                .documents
                .iter()
                .map(|(document, record)| DocumentSnapshot {
                    document_id: *document,
                    body: record.body.clone(),
                    comments: record.comments.clone(),
                    revision: record.revision,
                    visibility: record.visibility,
                })
                .collect::<Vec<_>>();
            let summaries = state
                .documents
                .iter()
                .map(|(document, record)| LazyDocumentSummary {
                    document_id: *document,
                    revision: record.revision,
                    visibility: record.visibility,
                })
                .chain(state.unloaded.values().copied())
                .collect();
            Ok(DocumentStateSnapshot {
                summaries,
                documents,
            })
        }
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

    /// Captures the project tree, full document catalog, and loaded bodies at
    /// one application operation boundary.
    pub fn authored_snapshot(&self) -> Result<AuthoredProjectSnapshot, ApplicationError> {
        self.authored_snapshot_inner(false)
    }

    /// Captures the same authoritative state while materializing every live
    /// document body for whole-project persistence operations.
    pub fn complete_authored_snapshot(&self) -> Result<AuthoredProjectSnapshot, ApplicationError> {
        self.authored_snapshot_inner(true)
    }

    fn authored_snapshot_inner(
        &self,
        complete: bool,
    ) -> Result<AuthoredProjectSnapshot, ApplicationError> {
        let state = lock(&self.state)?;
        let documents = self.documents.state_snapshot(complete)?;
        authored_snapshot(&state.project, documents)
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

    pub fn has_unsaved_changes(&self) -> Result<bool, ApplicationError> {
        Ok(!lock(&self.state)?.dirty.is_empty())
    }

    pub fn execute_document(
        &self,
        command: DocumentCommand,
    ) -> Result<DocumentCommandResult, ApplicationError> {
        let mut state = lock(&self.state)?;
        let observed_revision = command.observed_revision;
        let result = self.documents.execute(command)?;
        if result.revision != observed_revision {
            let mutation = state.mark_document_dirty(result.document_id);
            state.stage_checkpoint(mutation);
        }
        Ok(result)
    }

    pub fn accept_editor_projection(
        &self,
        projection: &CanonicalProjection,
    ) -> Result<(), ApplicationError> {
        let mut state = lock(&self.state)?;
        let changed = self.documents.accept_projection(
            projection.document_id(),
            projection.revision(),
            projection.body().to_owned(),
            projection.comments().to_vec(),
        )?;
        if changed {
            let mutation = state.mark_document_dirty(projection.document_id());
            state.stage_checkpoint(mutation);
        }
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
        self.capture_save_state().map(|(request, _)| request)
    }

    pub(crate) fn capture_save_state(
        &self,
    ) -> Result<(RevisionedSaveRequest, AuthoredProjectSnapshot), ApplicationError> {
        let mut state = lock(&self.state)?;
        self.capture_save_state_locked(&mut state)
    }

    pub(crate) fn capture_save_state_if_dirty(
        &self,
    ) -> Result<Option<(RevisionedSaveRequest, AuthoredProjectSnapshot)>, ApplicationError> {
        let mut state = lock(&self.state)?;
        if state.dirty.is_empty() {
            return Ok(None);
        }
        self.capture_save_state_locked(&mut state).map(Some)
    }

    fn capture_save_state_locked(
        &self,
        state: &mut DispatcherState,
    ) -> Result<(RevisionedSaveRequest, AuthoredProjectSnapshot), ApplicationError> {
        let snapshot = authored_snapshot(&state.project, self.documents.state_snapshot(false)?)?;
        // Recovery/save frontiers retain deleted document revisions until the
        // post-delete checkpoint commits. Authored snapshots filter them out.
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
        Ok((request, snapshot))
    }

    pub(crate) fn recovery_revision_request(
        &self,
    ) -> Result<RevisionedSaveRequest, ApplicationError> {
        let state = lock(&self.state)?;
        let _snapshot = authored_snapshot(&state.project, self.documents.state_snapshot(false)?)?;
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

        if applied.changed_resources.iter().next().is_none() {
            return Ok(ProjectCommandResult {
                operation_id: state.operation_id(),
                revision: before,
                dirty_resources: ResourceSet::default(),
                events: vec![ProjectEvent::Unchanged],
                checkpoint_group: None,
            });
        }

        // A document node and its canonical editor state are one application
        // mutation. The project lock remains held until both are published, so
        // snapshot queries can never observe a node without its default body.
        if let ProjectCommand::CreateDocument { document_id, .. } = &forward {
            self.documents.insert_document(DocumentSnapshot {
                document_id: *document_id,
                body: "<p></p>".to_owned(),
                comments: Vec::new(),
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
            checkpoint_group: Some(checkpoint_group),
        })
    }

    fn apply_replacement_now(
        &self,
        selection: ReplacementSelection,
    ) -> Result<ProjectCommandResult, ApplicationError> {
        let mut state = lock(&self.state)?;
        let patch = self.documents.prepare_composite(&selection.edits)?;
        let before = state.project.revision;
        if patch.is_empty()
            || patch
                .patches()
                .iter()
                .all(|patch| patch.before() == patch.after())
        {
            return Ok(ProjectCommandResult {
                operation_id: state.operation_id(),
                revision: before,
                dirty_resources: ResourceSet::default(),
                events: vec![ProjectEvent::Unchanged],
                checkpoint_group: None,
            });
        }
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
            checkpoint_group: Some(checkpoint_group),
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
            checkpoint_group: Some(checkpoint_group),
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
            checkpoint_group: Some(checkpoint_group),
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

fn authored_snapshot(
    project: &Project,
    documents: DocumentStateSnapshot,
) -> Result<AuthoredProjectSnapshot, ApplicationError> {
    let live_documents = project
        .nodes
        .iter()
        .filter_map(|(_, node)| match node.kind {
            parchmint_domain::NodeKind::Document(document) => Some(document),
            parchmint_domain::NodeKind::Root(_) | parchmint_domain::NodeKind::Group => None,
        })
        .collect::<BTreeSet<_>>();
    let summaries = documents
        .summaries
        .into_iter()
        .filter(|summary| live_documents.contains(&summary.document_id))
        .collect::<Vec<_>>();
    let represented = summaries
        .iter()
        .map(|summary| summary.document_id)
        .collect::<BTreeSet<_>>();
    if let Some(document) = live_documents.difference(&represented).next().copied() {
        return Err(ApplicationError::MissingDocument { document });
    }
    Ok(AuthoredProjectSnapshot {
        project: project.clone(),
        document_summaries: summaries,
        documents: documents
            .documents
            .into_iter()
            .filter(|document| live_documents.contains(&document.document_id))
            .collect(),
    })
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
