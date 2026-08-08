//! Engine-neutral contracts for shared ParchMint editor sessions and views.
//!
//! One [`SharedEditorSession`] owns one open document and its document-level
//! state. Attached views share that state and its undo history, but retain
//! their own selection, viewport, focus, scroll position, and decorations.
//! No type in this crate exposes an editor-engine document, transaction, or
//! GUI handle.

use std::{error::Error, fmt, future::Future, pin::Pin, sync::mpsc};

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
    ) -> AsyncResult<CanonicalProjection>;

    fn events(&self, session: SharedEditorSession) -> EventStream<EditorEvent>;
    fn close(&self, session: SharedEditorSession) -> AsyncResult<()>;
    fn capabilities(&self) -> EditorCapabilities;
}

#[cfg(test)]
mod editor_api_contract_tests;
