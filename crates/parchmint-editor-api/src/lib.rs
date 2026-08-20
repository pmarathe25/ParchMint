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

pub use parchmint_contracts::AnnotationValue;
use parchmint_recovery_api::{
    EditorRevisionRange, RecoveryBaseSnapshot, RecoveryBatch, RecoveryError, RecoveryInventory,
    RecoveryJournal, RecoveryReceipt, RecoveryRevisionVector, ResourceId, VersionedRecoveryPayload,
};
use parchmint_save::{
    CancelOutcome, SaveCoordinator, SaveError, SaveRequest, SaveRevisionVector, SaveTicket,
};

pub use parchmint_domain::{
    BlockId, CommentId, DocumentId, ProjectOperationId, StyleCatalog, StyleDefinition, StyleId,
    StyleProperties, StyleRole, TextAlignment, ViewId,
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

/// A UTF-8 scalar position in rendered semantic document text.
///
/// Canonical HTML tags and attributes never contribute to this coordinate.
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

/// Immutable clipboard data captured from one exact semantic selection.
///
/// `restricted_html` uses only the editor's canonical safe subset. Hosts may
/// write only `plain_text` when their platform clipboard contract does not yet
/// support rich output. The captured revision and range let a later cut
/// completion reject intervening edits instead of deleting newer content.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditorClipboardContent {
    revision: EditorRevision,
    selection: EditorSelection,
    plain_text: String,
    restricted_html: Option<String>,
}

impl EditorClipboardContent {
    pub fn new(
        revision: EditorRevision,
        selection: EditorSelection,
        plain_text: impl Into<String>,
        restricted_html: Option<String>,
    ) -> Self {
        Self {
            revision,
            selection,
            plain_text: plain_text.into(),
            restricted_html,
        }
    }

    pub const fn revision(&self) -> EditorRevision {
        self.revision
    }

    pub const fn selection(&self) -> EditorSelection {
        self.selection
    }

    pub fn plain_text(&self) -> &str {
        &self.plain_text
    }

    pub fn restricted_html(&self) -> Option<&str> {
        self.restricted_html.as_deref()
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

/// A live comment anchor belonging to one attached view.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommentDecoration {
    comment: CommentId,
    range: EditorSelection,
    active: bool,
}

impl CommentDecoration {
    pub const fn new(comment: CommentId, range: EditorSelection, active: bool) -> Self {
        Self {
            comment,
            range,
            active,
        }
    }

    pub const fn comment(&self) -> CommentId {
        self.comment
    }

    pub const fn range(&self) -> EditorSelection {
        self.range
    }

    pub const fn active(&self) -> bool {
        self.active
    }
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
/// One plain-text message in chronological thread order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanonicalCommentMessage {
    pub id: CommentId,
    pub body: String,
    pub unknown_fields: BTreeMap<String, AnnotationValue>,
}

/// A durable comment location. Orphaned text anchors retain their evidence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CanonicalCommentAnchor {
    Document {
        unknown_fields: BTreeMap<String, AnnotationValue>,
    },
    Text {
        block: BlockId,
        range: EditorSelection,
        quote: String,
        context_before: String,
        context_after: String,
        orphaned: bool,
        unknown_fields: BTreeMap<String, AnnotationValue>,
    },
}

/// One durable comment thread retained with a canonical editor document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanonicalComment {
    pub id: CommentId,
    pub messages: Vec<CanonicalCommentMessage>,
    pub resolved: bool,
    pub anchor: CanonicalCommentAnchor,
    pub unknown_fields: BTreeMap<String, AnnotationValue>,
}

impl CanonicalComment {
    pub fn new(
        id: CommentId,
        range: EditorSelection,
        body: impl Into<String>,
        block: BlockId,
    ) -> Self {
        Self {
            id,
            messages: vec![CanonicalCommentMessage {
                id,
                body: body.into(),
                unknown_fields: BTreeMap::new(),
            }],
            resolved: false,
            anchor: CanonicalCommentAnchor::Text {
                block,
                range,
                quote: String::new(),
                context_before: String::new(),
                context_after: String::new(),
                orphaned: false,
                unknown_fields: BTreeMap::new(),
            },
            unknown_fields: BTreeMap::new(),
        }
    }
}

/// One stable ParchMint anchor retained with a canonical editor document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanonicalAnchor {
    pub block: BlockId,
    pub position: DocumentPosition,
}

/// ParchMint-owned data used to open one document session. `body` is the
/// restricted canonical HTML persistence form; adapters expose its parsed
/// semantic content for editing and rendering.
#[derive(Debug, Clone, PartialEq)]
pub struct CanonicalDocumentLoad {
    pub document_id: DocumentId,
    pub revision: EditorRevision,
    pub body: String,
    pub comments: Vec<CanonicalComment>,
    pub anchors: Vec<CanonicalAnchor>,
    pub styles: StyleCatalogProjection,
}

/// A supported inline mark in the semantic editor projection.
///
/// Document positions address the rendered UTF-8 scalar text, never bytes or
/// characters in the canonical HTML serialization.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum SemanticInlineMark {
    Bold,
    Italic,
    Underline,
    Strikethrough,
    SmallCaps,
    Superscript,
    Subscript,
    Link(String),
}

/// Inline marks that can be toggled without an associated value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InlineMarkKind {
    Bold,
    Italic,
    Underline,
    Strikethrough,
    SmallCaps,
    Superscript,
    Subscript,
}

impl InlineMarkKind {
    pub const fn semantic(self) -> SemanticInlineMark {
        match self {
            Self::Bold => SemanticInlineMark::Bold,
            Self::Italic => SemanticInlineMark::Italic,
            Self::Underline => SemanticInlineMark::Underline,
            Self::Strikethrough => SemanticInlineMark::Strikethrough,
            Self::SmallCaps => SemanticInlineMark::SmallCaps,
            Self::Superscript => SemanticInlineMark::Superscript,
            Self::Subscript => SemanticInlineMark::Subscript,
        }
    }
}

/// One marked range relative to the start of a semantic block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticMarkRange {
    range: EditorSelection,
    mark: SemanticInlineMark,
}

impl SemanticMarkRange {
    pub const fn new(range: EditorSelection, mark: SemanticInlineMark) -> Self {
        Self { range, mark }
    }

    pub const fn range(&self) -> EditorSelection {
        self.range
    }

    pub const fn mark(&self) -> &SemanticInlineMark {
        &self.mark
    }
}

/// The structural role of one rendered text block.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SemanticBlockKind {
    Paragraph,
    Heading1,
    Heading2,
    Heading3,
    BlockQuote,
    UnorderedListItem,
    OrderedListItem,
    SceneBreak,
    PageBreak,
}

/// Block formats toggled over one or more selected text blocks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockFormatKind {
    BulletedList,
    NumberedList,
    BlockQuote,
}

/// Non-text structural blocks represented canonically by restricted `<hr>`
/// elements with an authoritative `data-kind` value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AtomicBlockKind {
    SceneBreak,
    PageBreak,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListDepthChange {
    Indent,
    Outdent,
}

/// One WYSIWYG block projected independently from canonical HTML.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticBlock {
    id: BlockId,
    kind: SemanticBlockKind,
    paragraph_style: Option<String>,
    text: String,
    marks: Vec<SemanticMarkRange>,
    list_depth: usize,
}

impl SemanticBlock {
    pub fn new(
        id: BlockId,
        kind: SemanticBlockKind,
        paragraph_style: Option<String>,
        text: impl Into<String>,
        marks: Vec<SemanticMarkRange>,
    ) -> Self {
        Self {
            id,
            kind,
            paragraph_style,
            text: text.into(),
            marks,
            list_depth: 0,
        }
    }

    pub const fn id(&self) -> BlockId {
        self.id
    }

    pub const fn kind(&self) -> SemanticBlockKind {
        self.kind
    }

    pub fn paragraph_style(&self) -> Option<&str> {
        self.paragraph_style.as_deref()
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn marks(&self) -> &[SemanticMarkRange] {
        &self.marks
    }

    pub const fn list_depth(&self) -> usize {
        self.list_depth
    }

    pub fn with_list_depth(mut self, list_depth: usize) -> Self {
        self.list_depth = list_depth;
        self
    }
}

/// Renderable semantic content for one exact editor revision.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SemanticDocument {
    blocks: Vec<SemanticBlock>,
}

impl SemanticDocument {
    pub fn new(blocks: Vec<SemanticBlock>) -> Self {
        Self { blocks }
    }

    pub fn blocks(&self) -> &[SemanticBlock] {
        &self.blocks
    }

    /// Plain rendered text with one scalar paragraph boundary between blocks.
    pub fn plain_text(&self) -> String {
        self.blocks
            .iter()
            .map(|block| match block.kind() {
                SemanticBlockKind::SceneBreak | SemanticBlockKind::PageBreak => "\u{fffc}",
                _ => block.text(),
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}

/// One sanitized non-atomic block in a semantic clipboard fragment.
/// Newlines in `text` are soft breaks; block boundaries are represented by
/// separate values in [`SemanticFragment`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticFragmentBlock {
    kind: SemanticBlockKind,
    text: String,
    marks: Vec<SemanticMarkRange>,
    list_depth: usize,
}

impl SemanticFragmentBlock {
    pub fn new(
        kind: SemanticBlockKind,
        text: impl Into<String>,
        marks: Vec<SemanticMarkRange>,
    ) -> Self {
        Self {
            kind,
            text: text.into(),
            marks,
            list_depth: 0,
        }
    }

    pub const fn kind(&self) -> SemanticBlockKind {
        self.kind
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn marks(&self) -> &[SemanticMarkRange] {
        &self.marks
    }

    pub const fn list_depth(&self) -> usize {
        self.list_depth
    }

    pub fn with_list_depth(mut self, list_depth: usize) -> Self {
        self.list_depth = list_depth;
        self
    }
}

/// Sanitized semantic clipboard content inserted as one revision.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticFragment {
    blocks: Vec<SemanticFragmentBlock>,
}

impl SemanticFragment {
    pub fn new(blocks: Vec<SemanticFragmentBlock>) -> Self {
        Self { blocks }
    }

    pub fn blocks(&self) -> &[SemanticFragmentBlock] {
        &self.blocks
    }

    pub fn scalar_len(&self) -> usize {
        self.blocks
            .iter()
            .map(|block| block.text.chars().count())
            .sum::<usize>()
            .saturating_add(self.blocks.len().saturating_sub(1))
    }
}

impl CanonicalDocumentLoad {
    pub fn new(document_id: DocumentId, body: impl Into<String>) -> Self {
        Self {
            document_id,
            revision: EditorRevision::default(),
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
    semantic: SemanticDocument,
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

fn advance_frontier(frontier: &mut RecoveryFrontier, batch: &RecoveryBatch) {
    frontier.revisions.project_revision = batch.project_revision;
    frontier
        .revisions
        .documents
        .extend(batch.revision_vector().documents);
    frontier.hashes.extend(batch.result_hashes.clone());
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
        self.persist_projection_with_document_hash(
            projection,
            revisions,
            payload,
            content_hash(projection.body().as_bytes()),
        )
    }

    /// Persists a projection whose logical document hash also covers
    /// application-owned sidecars such as annotations.
    pub fn persist_projection_with_document_hash(
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
            // Old single-document recovery bases remain readable until the
            // next canonical save upgrades their identity.
            ResourceId::Document
        };
        if !frontier.hashes.contains_key(&document_resource) {
            return Err(RecoveryError::MissingBaseHash {
                resource: document_resource,
            }
            .into());
        }
        let base_hash = frontier.hashes[&document_resource];
        let batch = RecoveryBatch {
            project_revision: frontier.revisions.project_revision.next(),
            documents: BTreeMap::from([(
                projection.document_id(),
                EditorRevisionRange::new(previous.next(), last)?,
            )]),
            base_hashes: BTreeMap::from([(document_resource.clone(), base_hash)]),
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
            || durable
                .batch
                .base_hashes
                .iter()
                .any(|(resource, hash)| frontier.hashes.get(resource) != Some(hash))
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
        advance_frontier(&mut frontier, &durable.batch);
        Ok(frontier.revisions.clone())
    }

    /// Replays durable records and promotes the accepted endpoint as the
    /// frontier, including a batch whose acknowledgement was interrupted.
    pub fn reconcile_recovery(
        &self,
        base: RecoveryBaseSnapshot,
    ) -> Result<parchmint_recovery_api::RecoveryReplay, EditorPersistenceError> {
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
        Ok(replay)
    }

    /// Deliberately rejects the exact replay previously reconciled from
    /// `base`, removes that accepted journal prefix, and restores the
    /// process-local frontier to the canonical durable base.
    ///
    /// This is a user disposition, not a save acknowledgement. Replaying the
    /// journal again before deletion prevents a stale recovery choice from
    /// discarding records that arrived after it was presented.
    pub fn discard_reconciled_recovery(
        &self,
        base: RecoveryBaseSnapshot,
        replay: &parchmint_recovery_api::RecoveryReplay,
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
        Ok(report)
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
            advance_frontier(&mut frontier, batch);
        }
        drop(frontier);
        self.acknowledge_recovery(durable)
    }

    /// Removes only the recovery prefix represented by a completed canonical
    /// save. Later document revisions remain in the journal.
    pub fn retire_recovery_through(
        &self,
        base: &RecoveryBaseSnapshot,
    ) -> Result<(), EditorPersistenceError> {
        self.recovery
            .discard_through(parchmint_recovery_api::DurableRevisionVector::new(
                base.revisions.clone(),
            ))?;
        Ok(())
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

    /// Supplies the exact canonical base hash when a summary-only document is
    /// first materialized by the owning project session.
    pub fn register_document_base(
        &self,
        document: DocumentId,
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
}

fn content_hash(bytes: &[u8]) -> parchmint_recovery_api::ContentHash {
    parchmint_recovery_api::ContentHash::of_bytes(bytes)
}

fn document_resource_id(document: DocumentId) -> ResourceId {
    let document_id = document
        .as_bytes()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    ResourceId::DocumentById { document_id }
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
        let body = body.into();
        Self {
            document_id,
            revision,
            semantic: SemanticDocument::new(vec![SemanticBlock::new(
                BlockId::from_bytes(*document_id.as_bytes()),
                SemanticBlockKind::Paragraph,
                None,
                body.clone(),
                Vec::new(),
            )]),
            body,
            comments,
            anchors,
            word_count,
        }
    }

    /// Builds a canonical persistence projection with a separate semantic
    /// rendering projection.
    pub fn new_semantic(
        document_id: DocumentId,
        revision: EditorRevision,
        body: impl Into<String>,
        semantic: SemanticDocument,
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
            semantic,
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

    pub const fn semantic(&self) -> &SemanticDocument {
        &self.semantic
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
    /// Replaces one scalar range and applies relative semantic marks as one
    /// revision and undo transaction. Mark ranges are relative to `text`.
    ReplaceRangeWithSemanticText {
        range: EditorSelection,
        text: String,
        marks: Vec<SemanticMarkRange>,
    },
    /// Replaces a range with a sanitized multi-block semantic fragment as one
    /// revision and undo transaction.
    ReplaceRangeWithSemanticFragment {
        range: EditorSelection,
        fragment: SemanticFragment,
    },
    SetSelection {
        selection: EditorSelection,
    },
    ApplyParagraphStyle {
        range: EditorSelection,
        style: StyleId,
    },
    ToggleInlineMark {
        range: EditorSelection,
        mark: InlineMarkKind,
    },
    /// Applies or updates a link over a non-empty range. `None` removes link
    /// formatting from the selected text.
    SetLink {
        range: EditorSelection,
        target: Option<String>,
    },
    ToggleBlockFormat {
        range: EditorSelection,
        format: BlockFormatKind,
    },
    InsertAtomicBlock {
        selection: EditorSelection,
        kind: AtomicBlockKind,
    },
    SplitBlock {
        selection: EditorSelection,
    },
    InsertSoftBreak {
        selection: EditorSelection,
    },
    AdjustListDepth {
        range: EditorSelection,
        change: ListDepthChange,
    },
    CreateComment {
        comment: CanonicalComment,
    },
    ReplyToComment {
        thread: CommentId,
        message: CanonicalCommentMessage,
    },
    SetCommentResolved {
        thread: CommentId,
        resolved: bool,
    },
    DeleteCommentThread {
        thread: CommentId,
    },
    DeleteCommentMessage {
        thread: CommentId,
        message: CommentId,
    },
    EditCommentMessage {
        thread: CommentId,
        message: CommentId,
        body: String,
    },
    ReattachComment {
        thread: CommentId,
        range: EditorSelection,
    },
    ConvertCommentToDocument {
        thread: CommentId,
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

    /// Captures a non-empty selection from the authoritative semantic session.
    /// A collapsed selection returns `Ok(None)` and is a safe clipboard no-op.
    fn selection_clipboard(
        &self,
        session: SharedEditorSession,
        view: ViewId,
    ) -> Result<Option<EditorClipboardContent>, EditorError>;

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
