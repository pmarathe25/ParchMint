//! Engine-neutral contracts for shared ParchMint editor sessions and views.
//!
//! One [`SharedEditorSession`] owns one open document and its document-level
//! state. Attached views share that state and its undo history, but retain
//! their own selection, viewport, focus, scroll position, and decorations.
//! No type in this crate exposes an editor-engine document, transaction, or
//! GUI handle.

use std::{
    collections::BTreeMap,
    error::Error,
    fmt,
    future::Future,
    pin::Pin,
    sync::{Arc, Mutex, mpsc},
};

use parchmint_recovery_api::{
    EditorRevisionRange, RecoveryBaseSnapshot, RecoveryBatch, RecoveryError, RecoveryInventory,
    RecoveryJournal, RecoveryReceipt, RecoveryRevisionVector, ResourceId, VersionedRecoveryPayload,
};
use parchmint_save::{
    CancelOutcome, SaveCoordinator, SaveError, SaveRequest, SaveRevisionVector, SaveTicket,
};
use sha2::{Digest, Sha256};

pub use parchmint_domain::{
    BlockId, CommentId, DocumentId, ProjectOperationId, StyleCatalog, StyleId, ViewId,
};

/// A `Send` future returned by an editor operation that may settle away from
/// the UI loop.
pub type AsyncResult<T> = Pin<Box<dyn Future<Output = T> + Send + 'static>>;

/// An event receiver for one editor session.
///
/// Iteration waits for the next event. Dropping the stream only stops this
/// subscriber; it does not change the session.
pub struct EventStream<T> {
    receiver: mpsc::Receiver<T>,
}

impl<T> EventStream<T> {
    /// Builds a stream from an implementation-owned event receiver.
    pub fn from_receiver(receiver: mpsc::Receiver<T>) -> Self {
        Self { receiver }
    }
}

impl<T> Iterator for EventStream<T> {
    type Item = T;

    fn next(&mut self) -> Option<Self::Item> {
        self.receiver.recv().ok()
    }
}

/// A capability for one editor session created by an [`EditorAdapter`].
///
/// The token is opaque to callers. Adapters reject unknown and closed-session
/// tokens.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SharedEditorSession(u64);

impl SharedEditorSession {
    /// Creates a token for use by an adapter implementation.
    pub const fn new(token: u64) -> Self {
        Self(token)
    }
}

/// An opaque capability identifying one mounted editor host.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ViewHostCapability(u64);

impl ViewHostCapability {
    /// Creates an adapter-owned host capability.
    pub const fn new(token: u64) -> Self {
        Self(token)
    }
}

/// One monotonic revision of an open document.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EditorRevision(u64);

impl EditorRevision {
    pub const fn value(self) -> u64 {
        self.0
    }

    pub const fn next(self) -> Self {
        Self(self.0.saturating_add(1))
    }
}

impl From<u64> for EditorRevision {
    fn from(value: u64) -> Self {
        Self(value)
    }
}

/// A UTF-8 scalar position in the session's canonical document body.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DocumentPosition(u64);

impl DocumentPosition {
    pub const fn value(self) -> u64 {
        self.0
    }
}

impl From<u64> for DocumentPosition {
    fn from(value: u64) -> Self {
        Self(value)
    }
}

/// A directional document selection. A collapsed selection has equal anchor
/// and head positions.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct EditorSelection {
    anchor: DocumentPosition,
    head: DocumentPosition,
}

impl EditorSelection {
    pub const fn new(anchor: DocumentPosition, head: DocumentPosition) -> Self {
        Self { anchor, head }
    }

    pub const fn anchor(self) -> DocumentPosition {
        self.anchor
    }

    pub const fn head(self) -> DocumentPosition {
        self.head
    }

    pub const fn is_collapsed(self) -> bool {
        self.anchor.value() == self.head.value()
    }

    pub const fn start(self) -> DocumentPosition {
        if self.anchor.value() <= self.head.value() {
            self.anchor
        } else {
            self.head
        }
    }

    pub const fn end(self) -> DocumentPosition {
        if self.anchor.value() <= self.head.value() {
            self.head
        } else {
            self.anchor
        }
    }
}

/// A finite rectangle in host layout coordinates.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SelectionRectangle {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl SelectionRectangle {
    pub fn is_finite(self) -> bool {
        self.x.is_finite()
            && self.y.is_finite()
            && self.width.is_finite()
            && self.height.is_finite()
    }
}

/// Host-layout geometry for a non-collapsed selection.
#[derive(Debug, Clone, PartialEq)]
pub struct SelectionGeometry {
    selection: EditorSelection,
    rectangles: Vec<SelectionRectangle>,
}

impl SelectionGeometry {
    pub fn new(selection: EditorSelection, rectangles: Vec<SelectionRectangle>) -> Self {
        Self {
            selection,
            rectangles,
        }
    }

    pub const fn selection(&self) -> EditorSelection {
        self.selection
    }

    pub fn rectangles(&self) -> &[SelectionRectangle] {
        &self.rectangles
    }
}

/// A ParchMint search highlight belonging to one attached view.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchDecoration {
    range: EditorSelection,
}

impl SearchDecoration {
    pub const fn new(range: EditorSelection) -> Self {
        Self { range }
    }

    pub const fn range(&self) -> EditorSelection {
        self.range
    }
}

/// A ParchMint spelling underline belonging to one attached view.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpellcheckDecoration {
    range: EditorSelection,
}

impl SpellcheckDecoration {
    pub const fn new(range: EditorSelection) -> Self {
        Self { range }
    }

    pub const fn range(&self) -> EditorSelection {
        self.range
    }
}

/// A style catalog supplied independently from one document body.
///
/// Updating this value redraws mounted occurrences without creating a document
/// edit or a document-undo entry.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct StyleCatalogProjection {
    catalog: StyleCatalog,
}

impl StyleCatalogProjection {
    pub fn new(catalog: StyleCatalog) -> Self {
        Self { catalog }
    }

    pub fn catalog(&self) -> &StyleCatalog {
        &self.catalog
    }
}

/// One comment retained with a canonical editor document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanonicalComment {
    pub id: CommentId,
    pub range: EditorSelection,
    pub body: String,
}

/// One stable ParchMint anchor retained with a canonical editor document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanonicalAnchor {
    pub block: BlockId,
    pub position: DocumentPosition,
}

/// ParchMint-owned data used to open one document session.
#[derive(Debug, Clone, PartialEq)]
pub struct CanonicalDocumentLoad {
    pub document_id: DocumentId,
    pub body: String,
    pub comments: Vec<CanonicalComment>,
    pub anchors: Vec<CanonicalAnchor>,
    pub styles: StyleCatalogProjection,
}

impl CanonicalDocumentLoad {
    pub fn new(document_id: DocumentId, body: impl Into<String>) -> Self {
        Self {
            document_id,
            body: body.into(),
            comments: Vec::new(),
            anchors: Vec::new(),
            styles: StyleCatalogProjection::default(),
        }
    }
}

/// A deterministic canonical snapshot of exactly one editor revision.
#[derive(Debug, Clone, PartialEq)]
pub struct CanonicalProjection {
    document_id: DocumentId,
    revision: EditorRevision,
    body: String,
    comments: Vec<CanonicalComment>,
    anchors: Vec<CanonicalAnchor>,
    word_count: usize,
}

/// A recovery batch whose bytes are durable but whose process-local frontier
/// has not yet been advanced.
///
/// Keeping this token between [`EditorPersistenceCoordinator::persist_projection`]
/// and [`EditorPersistenceCoordinator::acknowledge_recovery`] makes the crash
/// boundary explicit: a termination after persistence can replay this exact
/// batch and resume acknowledgement without inventing another batch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DurableProjectionBatch {
    batch: RecoveryBatch,
    receipt: RecoveryReceipt,
}

impl DurableProjectionBatch {
    pub fn new(
        batch: RecoveryBatch,
        receipt: RecoveryReceipt,
    ) -> Result<Self, EditorPersistenceError> {
        if !receipt.authenticates(&batch) {
            return Err(EditorPersistenceError::Recovery(
                RecoveryError::UnknownRevisionVector,
            ));
        }
        Ok(Self { batch, receipt })
    }

    pub fn batch(&self) -> &RecoveryBatch {
        &self.batch
    }

    pub fn receipt(&self) -> &RecoveryReceipt {
        &self.receipt
    }
}

/// Errors raised while joining an editor projection to persistence services.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditorPersistenceError {
    Projection(EditorError),
    Recovery(RecoveryError),
    RecoveryIsolation(parchmint_recovery_api::RecoveryIsolationReason),
    Save(SaveError),
    RevisionMismatch,
    StateUnavailable,
}

impl fmt::Display for EditorPersistenceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Projection(error) => write!(formatter, "editor projection failed: {error}"),
            Self::Recovery(error) => write!(formatter, "editor recovery failed: {error}"),
            Self::RecoveryIsolation(reason) => {
                write!(formatter, "editor recovery replay isolated: {reason:?}")
            }
            Self::Save(error) => write!(formatter, "editor save failed: {error}"),
            Self::RevisionMismatch => {
                formatter.write_str("editor projection revision mismatched save vector")
            }
            Self::StateUnavailable => {
                formatter.write_str("editor persistence frontier unavailable")
            }
        }
    }
}

impl Error for EditorPersistenceError {}

impl From<RecoveryError> for EditorPersistenceError {
    fn from(error: RecoveryError) -> Self {
        Self::Recovery(error)
    }
}

impl From<SaveError> for EditorPersistenceError {
    fn from(error: SaveError) -> Self {
        Self::Save(error)
    }
}

/// Production-owned coordination between editor projections, recovery, and
/// the revisioned save service. Desktop graph assembly remains a later seam.
pub struct EditorPersistenceCoordinator {
    recovery: Arc<dyn RecoveryJournal>,
    save: Option<Arc<dyn SaveCoordinator>>,
    frontier: Mutex<RecoveryFrontier>,
}

#[derive(Debug, Clone)]
struct RecoveryFrontier {
    revisions: RecoveryRevisionVector,
    hashes: BTreeMap<ResourceId, parchmint_recovery_api::ContentHash>,
}

impl fmt::Debug for EditorPersistenceCoordinator {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("EditorPersistenceCoordinator")
            .field("frontier", &self.frontier())
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
        }
    }

    /// Builds a recovery-only coordinator for contract and recovery replay
    /// consumers that do not own a save worker. The production desktop graph
    /// uses [`Self::new`] with its project-scoped save worker.
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
        }
    }

    /// Appends a projection and returns only after its exact revision vector
    /// has a durable receipt. The in-memory frontier is intentionally unchanged.
    pub fn persist_projection(
        &self,
        projection: &CanonicalProjection,
        revisions: &SaveRevisionVector,
        payload: VersionedRecoveryPayload,
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
        let Some(base_hash) = frontier.hashes.get(&ResourceId::Document).copied() else {
            return Err(RecoveryError::MissingBaseHash {
                resource: ResourceId::Document,
            }
            .into());
        };
        let result_hash = content_hash(projection.body().as_bytes());
        let batch = RecoveryBatch {
            project_revision: frontier.revisions.project_revision.next(),
            documents: BTreeMap::from([(
                projection.document_id(),
                EditorRevisionRange::new(previous.next(), last)?,
            )]),
            base_hashes: BTreeMap::from([(ResourceId::Document, base_hash)]),
            result_hashes: BTreeMap::from([(ResourceId::Document, result_hash)]),
            payload,
        };
        batch.validate_after(None)?;
        let receipt = self.recovery.append(batch.clone())?;
        if receipt.durable_through != batch.revision_vector() {
            return Err(RecoveryError::UnknownRevisionVector.into());
        }
        DurableProjectionBatch::new(batch, receipt)
    }

    /// Returns the durable recovery inventory without changing the process
    /// frontier. Applications expose this alongside reconciliation results so
    /// isolation is diagnosable rather than reduced to one generic error.
    pub fn recovery_inventory(&self) -> Result<RecoveryInventory, EditorPersistenceError> {
        self.recovery
            .inspect()
            .map_err(EditorPersistenceError::Recovery)
    }

    /// Advances the process-local frontier only for the exact durable batch.
    pub fn acknowledge_recovery(
        &self,
        durable: DurableProjectionBatch,
    ) -> Result<RecoveryRevisionVector, EditorPersistenceError> {
        if !durable.receipt.authenticates(&durable.batch) {
            return Err(RecoveryError::UnknownRevisionVector.into());
        }
        let mut frontier = self
            .frontier
            .lock()
            .map_err(|_| EditorPersistenceError::StateUnavailable)?;
        if durable.batch.project_revision != frontier.revisions.project_revision.next()
            || durable.batch.base_hashes != frontier.hashes
            || durable.batch.documents.iter().any(|(document, range)| {
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
                actual: durable.batch.project_revision,
            }
            .into());
        }
        frontier.revisions.project_revision = durable.batch.project_revision;
        frontier.revisions.documents = durable.batch.revision_vector().documents;
        frontier.hashes = durable.batch.result_hashes;
        Ok(frontier.revisions.clone())
    }

    /// Replays durable records and promotes the accepted endpoint as the
    /// frontier, including a batch whose acknowledgement was interrupted.
    pub fn reconcile_recovery(
        &self,
        base: RecoveryBaseSnapshot,
    ) -> Result<parchmint_recovery_api::RecoveryReplay, EditorPersistenceError> {
        let replay = self.recovery.replay(base)?;
        if let Some(last) = replay.accepted.last() {
            let mut frontier = self
                .frontier
                .lock()
                .map_err(|_| EditorPersistenceError::StateUnavailable)?;
            frontier.revisions.project_revision = last.project_revision;
            frontier.revisions.documents = last.revision_vector().documents;
            frontier.hashes = last.result_hashes.clone();
        }
        Ok(replay)
    }

    /// Reconciles records preceding an interrupted batch, then acknowledges
    /// that exact durable batch using its original receipt identity.
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
            frontier.revisions.project_revision = batch.project_revision;
            frontier.revisions.documents = batch.revision_vector().documents;
            frontier.hashes = batch.result_hashes.clone();
        }
        drop(frontier);
        self.acknowledge_recovery(durable)
    }

    /// Submits the already encoded revisioned save request only when it covers
    /// the projection being persisted.
    pub fn submit_save(
        &self,
        projection: &CanonicalProjection,
        request: SaveRequest,
    ) -> Result<SaveTicket, EditorPersistenceError> {
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
        Ok(save.request(request)?)
    }

    pub fn cancel_save(&self, ticket: SaveTicket) -> CancelOutcome {
        self.save
            .as_ref()
            .map_or(CancelOutcome::WorkerStopped, |save| {
                save.cancel_pending(ticket)
            })
    }

    pub fn frontier(&self) -> Option<RecoveryRevisionVector> {
        self.frontier
            .lock()
            .ok()
            .map(|frontier| frontier.revisions.clone())
    }
}

fn content_hash(bytes: &[u8]) -> parchmint_recovery_api::ContentHash {
    parchmint_recovery_api::ContentHash::from_bytes(Sha256::digest(bytes).into())
}

impl CanonicalProjection {
    pub fn new(
        document_id: DocumentId,
        revision: EditorRevision,
        body: impl Into<String>,
        comments: Vec<CanonicalComment>,
        anchors: Vec<CanonicalAnchor>,
        word_count: usize,
    ) -> Self {
        Self {
            document_id,
            revision,
            body: body.into(),
            comments,
            anchors,
            word_count,
        }
    }

    pub const fn document_id(&self) -> DocumentId {
        self.document_id
    }

    pub const fn revision(&self) -> EditorRevision {
        self.revision
    }

    pub fn body(&self) -> &str {
        &self.body
    }

    pub fn comments(&self) -> &[CanonicalComment] {
        &self.comments
    }

    pub fn anchors(&self) -> &[CanonicalAnchor] {
        &self.anchors
    }

    pub const fn word_count(&self) -> usize {
        self.word_count
    }
}

/// The attached view that observed and initiated a document command.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EditorCommandOrigin {
    view: ViewId,
}

impl EditorCommandOrigin {
    pub const fn new(view: ViewId) -> Self {
        Self { view }
    }

    pub const fn view(self) -> ViewId {
        self.view
    }
}

/// A ParchMint document command, never an editor-engine transaction.
///
/// Document-changing commands advance the shared session revision exactly
/// once. `SetSelection` changes only the originating view's selection and
/// leaves the shared revision unchanged.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditorCommandKind {
    InsertText {
        at: DocumentPosition,
        text: String,
    },
    DeleteRange {
        range: EditorSelection,
    },
    ReplaceRange {
        range: EditorSelection,
        text: String,
    },
    SetSelection {
        selection: EditorSelection,
    },
    ApplyParagraphStyle {
        range: EditorSelection,
        style: StyleId,
    },
    Undo,
    Redo,
}

/// One revision-checked document command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditorCommand {
    observed_revision: EditorRevision,
    kind: EditorCommandKind,
}

impl EditorCommand {
    pub const fn new(observed_revision: EditorRevision, kind: EditorCommandKind) -> Self {
        Self {
            observed_revision,
            kind,
        }
    }

    pub const fn observed_revision(&self) -> EditorRevision {
        self.observed_revision
    }

    pub const fn kind(&self) -> &EditorCommandKind {
        &self.kind
    }
}

/// A project-wide document replacement applied at an editor-session boundary.
/// It does not create a document-undo entry.
#[derive(Debug, Clone, PartialEq)]
pub struct ProjectDocumentOperation {
    pub operation_id: ProjectOperationId,
    pub observed_revision: EditorRevision,
    pub replacement: CanonicalDocumentLoad,
}

/// Per-view state returned when a view is detached. This state is never shared
/// between attached views.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditorViewState {
    selection: EditorSelection,
    search_decorations: Vec<SearchDecoration>,
    spellcheck_decorations: Vec<SpellcheckDecoration>,
}

impl EditorViewState {
    pub const fn new(selection: EditorSelection) -> Self {
        Self {
            selection,
            search_decorations: Vec::new(),
            spellcheck_decorations: Vec::new(),
        }
    }

    pub const fn selection(&self) -> EditorSelection {
        self.selection
    }

    pub fn search_decorations(&self) -> &[SearchDecoration] {
        &self.search_decorations
    }

    pub fn spellcheck_decorations(&self) -> &[SpellcheckDecoration] {
        &self.spellcheck_decorations
    }
}

/// An event emitted in session order to each subscriber.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditorEvent {
    ViewAttached { view: ViewId },
    DocumentChanged { revision: EditorRevision },
    ViewDetached { view: ViewId },
    Closed,
}

/// Features available from one adapter implementation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EditorCapabilities {
    pub supports_two_views: bool,
    pub supports_selection_geometry: bool,
    pub supports_search_decorations: bool,
    pub supports_spellcheck_decorations: bool,
}

impl Default for EditorCapabilities {
    fn default() -> Self {
        Self {
            supports_two_views: true,
            supports_selection_geometry: true,
            supports_search_decorations: true,
            supports_spellcheck_decorations: true,
        }
    }
}

/// A rejected editor operation that leaves session state unchanged.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditorError {
    UnknownSession,
    Closed,
    ViewAlreadyAttached {
        view: ViewId,
    },
    UnknownView {
        view: ViewId,
    },
    StaleCommand {
        observed: EditorRevision,
        current: EditorRevision,
    },
    InvalidCommand {
        reason: &'static str,
    },
    DocumentMismatch {
        expected: DocumentId,
        received: DocumentId,
    },
}

impl EditorError {
    pub const fn is_stale_command(&self) -> bool {
        matches!(self, Self::StaleCommand { .. })
    }

    pub const fn is_closed(&self) -> bool {
        matches!(self, Self::Closed)
    }
}

impl fmt::Display for EditorError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownSession => formatter.write_str("unknown editor session"),
            Self::Closed => formatter.write_str("editor session is closed"),
            Self::ViewAlreadyAttached { view } => {
                write!(formatter, "view {view:?} is already attached")
            }
            Self::UnknownView { view } => write!(formatter, "view {view:?} is not attached"),
            Self::StaleCommand { observed, current } => write!(
                formatter,
                "editor command observed revision {} but session is at {}",
                observed.value(),
                current.value()
            ),
            Self::InvalidCommand { reason } => {
                write!(formatter, "invalid editor command: {reason}")
            }
            Self::DocumentMismatch { expected, received } => write!(
                formatter,
                "project operation targets {received:?}, not session document {expected:?}"
            ),
        }
    }
}

impl Error for EditorError {}

/// The engine-neutral editor boundary used by the application and UI.
///
/// A command is applied only when its observed revision equals the current
/// session revision. Successful document changes, undo, redo, and project
/// operations advance the revision exactly once. `close` is idempotent: it
/// settles outstanding work, detaches all views, emits `Closed`, and makes all
/// subsequent synchronous session operations return [`EditorError::Closed`].
/// Projection requests outside the adapter's retained revision budget return
/// [`EditorError`]; they never panic or silently substitute another revision.
pub trait EditorAdapter: Send + Sync {
    fn open(&self, load: CanonicalDocumentLoad) -> AsyncResult<SharedEditorSession>;

    fn attach_view(
        &self,
        session: SharedEditorSession,
        view: ViewId,
        host: ViewHostCapability,
    ) -> Result<(), EditorError>;

    fn detach_view(
        &self,
        session: SharedEditorSession,
        view: ViewId,
    ) -> Result<EditorViewState, EditorError>;

    fn execute(
        &self,
        session: SharedEditorSession,
        origin: EditorCommandOrigin,
        command: EditorCommand,
    ) -> Result<(), EditorError>;

    fn selection(
        &self,
        session: SharedEditorSession,
        view: ViewId,
    ) -> Result<EditorSelection, EditorError>;

    fn selection_geometry(
        &self,
        session: SharedEditorSession,
        view: ViewId,
    ) -> Result<Option<SelectionGeometry>, EditorError>;

    fn set_style_catalog(
        &self,
        session: SharedEditorSession,
        styles: StyleCatalogProjection,
    ) -> Result<(), EditorError>;

    fn set_search_decorations(
        &self,
        session: SharedEditorSession,
        view: ViewId,
        decorations: Vec<SearchDecoration>,
    ) -> Result<(), EditorError>;

    fn set_spellcheck_decorations(
        &self,
        session: SharedEditorSession,
        view: ViewId,
        decorations: Vec<SpellcheckDecoration>,
    ) -> Result<(), EditorError>;

    fn apply_composite_project_edit(
        &self,
        session: SharedEditorSession,
        operation: ProjectDocumentOperation,
    ) -> Result<(), EditorError>;

    fn project(
        &self,
        session: SharedEditorSession,
        through: EditorRevision,
    ) -> AsyncResult<Result<CanonicalProjection, EditorError>>;

    fn events(&self, session: SharedEditorSession) -> EventStream<EditorEvent>;
    fn close(&self, session: SharedEditorSession) -> AsyncResult<()>;
    fn capabilities(&self) -> EditorCapabilities;
}

#[cfg(test)]
mod editor_api_contract_tests;
