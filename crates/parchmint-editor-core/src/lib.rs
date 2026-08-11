//! ParchMint's shared editor session.
//!
//! The session owns ParchMint identifiers, transactions, comments, anchors,
//! revision mappings, view state, and canonical projection scheduling. The
//! replaceable document engine is private and can only receive ParchMint-owned
//! semantic blocks and edits.

mod document_engine;
pub mod feasibility;
mod projection;
mod semantic_html;

use std::collections::BTreeMap;

use document_engine::{
    DocumentEngine, EngineEdit, EngineError, PositionMapping, PrivateTextEngine,
    SemanticDocumentSnapshot,
};
use projection::{Projection, ProjectionBatch, ProjectionQueue};

pub use parchmint_editor_api::{
    AsyncResult, AtomicBlockKind, BlockFormatKind, BlockId, CanonicalAnchor, CanonicalComment,
    CanonicalDocumentLoad, CanonicalProjection, CommentId, DocumentId, DocumentPosition,
    EditorClipboardContent, EditorCommand, EditorCommandKind, EditorCommandOrigin, EditorError,
    EditorRevision, EditorSelection, EditorViewState, InlineMarkKind, SemanticBlock,
    SemanticBlockKind, SemanticDocument, SemanticInlineMark, SemanticMarkRange,
    StyleCatalogProjection, StyleId, ViewId,
};

const DEFAULT_PROJECTION_CAPACITY: usize = 2;
const ANCHOR_CONTEXT_SCALARS: usize = 16;

/// A monotonic identifier for one document change.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TransactionId(u64);

impl TransactionId {
    /// Returns the session-local numeric identifier.
    pub const fn value(self) -> u64 {
        self.0
    }
}

/// The result of applying one command to an editor session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppliedEditorChange {
    revision: EditorRevision,
    transaction: Option<TransactionId>,
    changed_blocks: Vec<BlockId>,
}

impl AppliedEditorChange {
    /// Returns the revision after the command.
    pub const fn revision(&self) -> EditorRevision {
        self.revision
    }

    /// Returns the transaction for a document-changing command.
    ///
    /// View-local commands do not create transactions.
    pub const fn transaction(&self) -> Option<TransactionId> {
        self.transaction
    }

    /// Reports whether this command changed the shared document.
    pub const fn document_changed(&self) -> bool {
        self.transaction.is_some()
    }

    /// Returns the semantic blocks invalidated by the command.
    pub fn changed_blocks(&self) -> &[BlockId] {
        &self.changed_blocks
    }
}

/// Canonical work drained by a projection consumer.
#[derive(Debug, Clone, PartialEq)]
pub enum CanonicalProjectionWork {
    /// Consecutive revisions coalesced into the newest incremental projection.
    Incremental(CanonicalProjection),
    /// A complete newest-revision snapshot after incremental work overflowed.
    FullSnapshot(CanonicalProjection),
}

impl CanonicalProjectionWork {
    /// Returns the revision represented by this work item.
    pub const fn revision(&self) -> EditorRevision {
        match self {
            Self::Incremental(projection) | Self::FullSnapshot(projection) => projection.revision(),
        }
    }

    /// Returns the canonical projection carried by this work item.
    pub const fn projection(&self) -> &CanonicalProjection {
        match self {
            Self::Incremental(projection) | Self::FullSnapshot(projection) => projection,
        }
    }
}

/// A comment anchor retained independently of engine positions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommentAnchor {
    block: BlockId,
    range: EditorSelection,
    quote: String,
    context_before: String,
    context_after: String,
    orphaned: bool,
}

impl CommentAnchor {
    pub const fn block(&self) -> BlockId {
        self.block
    }

    pub const fn range(&self) -> EditorSelection {
        self.range
    }

    pub fn quote(&self) -> &str {
        &self.quote
    }

    pub fn context_before(&self) -> &str {
        &self.context_before
    }

    pub fn context_after(&self) -> &str {
        &self.context_after
    }

    pub const fn is_orphaned(&self) -> bool {
        self.orphaned
    }
}

#[derive(Debug, Clone)]
struct StoredComment {
    canonical: CanonicalComment,
    anchor: CommentAnchor,
}

#[derive(Debug, Clone)]
struct UndoEntry {
    before: SemanticDocumentSnapshot,
    after: SemanticDocumentSnapshot,
    forward_mapping: PositionMapping,
    changed_blocks: Vec<BlockId>,
}

#[derive(Debug, Clone, Copy)]
struct RevisionMapping {
    revision: EditorRevision,
    mapping: PositionMapping,
}

struct EditorSession<E: DocumentEngine> {
    engine: E,
    document_id: DocumentId,
    primary_block: BlockId,
    revision: EditorRevision,
    next_transaction: u64,
    next_block: u64,
    views: BTreeMap<ViewId, EditorViewState>,
    comments: BTreeMap<CommentId, StoredComment>,
    anchors: Vec<CanonicalAnchor>,
    styles: StyleCatalogProjection,
    undo: Vec<UndoEntry>,
    redo: Vec<UndoEntry>,
    mappings: Vec<RevisionMapping>,
    projections: ProjectionQueue,
}

impl<E: DocumentEngine> EditorSession<E> {
    fn open(mut engine: E, load: CanonicalDocumentLoad) -> Result<Self, EditorError> {
        let CanonicalDocumentLoad {
            document_id,
            body,
            comments,
            anchors,
            styles,
        } = load;
        let primary_block = BlockId::from_bytes(*document_id.as_bytes());
        let document = semantic_html::parse(&body, primary_block).map_err(invalid)?;
        let plain_text = document.plain_text();
        let body_len = plain_text.chars().count();

        validate_comments(&comments, body_len)?;
        validate_anchors(&anchors, body_len)?;

        let mut stored_comments = BTreeMap::new();
        for comment in comments {
            if stored_comments.contains_key(&comment.id) {
                return Err(invalid("duplicate comment id"));
            }
            let anchor = comment_anchor(primary_block, comment.range, &plain_text)?;
            stored_comments.insert(
                comment.id,
                StoredComment {
                    canonical: comment,
                    anchor,
                },
            );
        }

        let next_block = u64::try_from(document.blocks.len())
            .map_err(|_| invalid("semantic block count exceeds the public range"))?;
        engine.load(document).map_err(engine_error)?;

        let mut session = Self {
            engine,
            document_id,
            primary_block,
            revision: EditorRevision::default(),
            next_transaction: 1,
            next_block,
            views: BTreeMap::new(),
            comments: stored_comments,
            anchors,
            styles,
            undo: Vec::new(),
            redo: Vec::new(),
            mappings: Vec::new(),
            projections: ProjectionQueue::new(DEFAULT_PROJECTION_CAPACITY),
        };
        session.offer_projection();
        Ok(session)
    }

    fn attach_view(&mut self, view: ViewId) -> Result<(), EditorError> {
        if self.views.contains_key(&view) {
            return Err(EditorError::ViewAlreadyAttached { view });
        }
        self.views
            .insert(view, EditorViewState::new(EditorSelection::default()));
        Ok(())
    }

    fn detach_view(&mut self, view: ViewId) -> Result<EditorViewState, EditorError> {
        self.views
            .remove(&view)
            .ok_or(EditorError::UnknownView { view })
    }

    fn execute(
        &mut self,
        origin: EditorCommandOrigin,
        command: EditorCommand,
    ) -> Result<AppliedEditorChange, EditorError> {
        let view = origin.view();
        if !self.views.contains_key(&view) {
            return Err(EditorError::UnknownView { view });
        }
        if command.observed_revision() != self.revision {
            return Err(EditorError::StaleCommand {
                observed: command.observed_revision(),
                current: self.revision,
            });
        }

        match command.kind() {
            EditorCommandKind::SetSelection { selection } => {
                self.validate_selection(*selection)?;
                self.views.insert(view, EditorViewState::new(*selection));
                Ok(AppliedEditorChange {
                    revision: self.revision,
                    transaction: None,
                    changed_blocks: Vec::new(),
                })
            }
            EditorCommandKind::InsertText { at, text } => {
                let edit = EngineEdit::new(position(*at)?, 0, text.clone());
                self.apply_new_edit(edit)
            }
            EditorCommandKind::DeleteRange { range } => {
                let (at, removed) = self.validated_range(*range)?;
                self.apply_new_edit(EngineEdit::new(at, removed, String::new()))
            }
            EditorCommandKind::ReplaceRange { range, text } => {
                let (at, removed) = self.validated_range(*range)?;
                self.apply_new_edit(EngineEdit::new(at, removed, text.clone()))
            }
            EditorCommandKind::ToggleInlineMark { range, mark } => {
                let (start, length) = self.validated_range(*range)?;
                let end = start
                    .checked_add(length)
                    .ok_or_else(|| invalid("selection range overflows"))?;
                self.apply_format_change(|engine| {
                    engine.toggle_inline_mark(start, end, mark.semantic())
                })
            }
            EditorCommandKind::SetLink { range, target } => {
                let (start, length) = self.validated_range(*range)?;
                let end = start
                    .checked_add(length)
                    .ok_or_else(|| invalid("selection range overflows"))?;
                if let Some(target) = target {
                    validate_link_target(target)?;
                }
                self.apply_format_change(|engine| engine.set_link(start, end, target.clone()))
            }
            EditorCommandKind::ToggleBlockFormat { range, format } => {
                let (start, length) = self.validated_range(*range)?;
                let end = start
                    .checked_add(length)
                    .ok_or_else(|| invalid("selection range overflows"))?;
                let target = match format {
                    BlockFormatKind::BulletedList => SemanticBlockKind::UnorderedListItem,
                    BlockFormatKind::NumberedList => SemanticBlockKind::OrderedListItem,
                    BlockFormatKind::BlockQuote => SemanticBlockKind::BlockQuote,
                };
                self.apply_format_change(|engine| engine.toggle_block_format(start, end, target))
            }
            EditorCommandKind::InsertAtomicBlock { selection, kind } => {
                self.validate_selection(*selection)?;
                if !selection.is_collapsed() {
                    return Err(invalid(
                        "atomic block insertion requires a collapsed selection",
                    ));
                }
                self.apply_atomic_block(position(selection.head())?, *kind)
            }
            EditorCommandKind::Undo => self.apply_undo(),
            EditorCommandKind::Redo => self.apply_redo(),
            EditorCommandKind::ApplyParagraphStyle { range, style } => {
                let (start, length) = self.validated_range(*range)?;
                let end = start
                    .checked_add(length)
                    .ok_or_else(|| invalid("selection range overflows"))?;
                let style = canonical_style_id(*style);
                self.apply_format_change(|engine| engine.apply_paragraph_style(start, end, style))
            }
        }
    }

    fn apply_new_edit(&mut self, edit: EngineEdit) -> Result<AppliedEditorChange, EditorError> {
        let (id, revision) = self.next_change_identity()?;
        let before = self.engine.snapshot();
        let change = self.engine.apply(edit).map_err(engine_error)?;
        let after = self.engine.snapshot();
        let changed_blocks = change.changed_blocks().to_vec();
        self.finish_change(id, revision, change.mapping())?;
        self.undo.push(UndoEntry {
            before,
            after,
            forward_mapping: change.mapping(),
            changed_blocks: changed_blocks.clone(),
        });
        self.redo.clear();
        Ok(AppliedEditorChange {
            revision: self.revision,
            transaction: Some(id),
            changed_blocks,
        })
    }

    fn apply_format_change(
        &mut self,
        operation: impl FnOnce(&mut E) -> Result<document_engine::EngineChange, EngineError>,
    ) -> Result<AppliedEditorChange, EditorError> {
        let (id, revision) = self.next_change_identity()?;
        let before = self.engine.snapshot();
        let change = operation(&mut self.engine).map_err(engine_error)?;
        let after = self.engine.snapshot();
        let changed_blocks = change.changed_blocks().to_vec();
        self.finish_change(id, revision, change.mapping())?;
        self.undo.push(UndoEntry {
            before,
            after,
            forward_mapping: change.mapping(),
            changed_blocks: changed_blocks.clone(),
        });
        self.redo.clear();
        Ok(AppliedEditorChange {
            revision: self.revision,
            transaction: Some(id),
            changed_blocks,
        })
    }

    fn apply_atomic_block(
        &mut self,
        at: usize,
        kind: AtomicBlockKind,
    ) -> Result<AppliedEditorChange, EditorError> {
        let (id, revision) = self.next_change_identity()?;
        let after_sequence = self
            .next_block
            .checked_add(1)
            .ok_or_else(|| invalid("semantic block id space exhausted"))?;
        let next_block = self
            .next_block
            .checked_add(2)
            .ok_or_else(|| invalid("semantic block id space exhausted"))?;
        let atomic_id = derived_block_id(self.document_id, self.next_block);
        let after_id = derived_block_id(self.document_id, after_sequence);
        let before = self.engine.snapshot();
        let change = self
            .engine
            .insert_atomic_block(at, kind, atomic_id, after_id)
            .map_err(engine_error)?;
        let after = self.engine.snapshot();
        let changed_blocks = change.changed_blocks().to_vec();
        self.finish_change(id, revision, change.mapping())?;
        self.next_block = next_block;
        self.undo.push(UndoEntry {
            before,
            after,
            forward_mapping: change.mapping(),
            changed_blocks: changed_blocks.clone(),
        });
        self.redo.clear();
        Ok(AppliedEditorChange {
            revision: self.revision,
            transaction: Some(id),
            changed_blocks,
        })
    }

    fn apply_undo(&mut self) -> Result<AppliedEditorChange, EditorError> {
        let (id, revision) = self.next_change_identity()?;
        let entry = self.undo.pop().ok_or_else(|| invalid("nothing to undo"))?;
        let mapping = entry.forward_mapping.inverse();
        let changed_blocks = entry.changed_blocks.clone();
        match self.engine.load(entry.before.clone()) {
            Ok(()) => (),
            Err(error) => {
                self.undo.push(entry);
                return Err(engine_error(error));
            }
        }
        self.finish_change(id, revision, mapping)?;
        self.redo.push(entry);
        Ok(AppliedEditorChange {
            revision: self.revision,
            transaction: Some(id),
            changed_blocks,
        })
    }

    fn apply_redo(&mut self) -> Result<AppliedEditorChange, EditorError> {
        let (id, revision) = self.next_change_identity()?;
        let entry = self.redo.pop().ok_or_else(|| invalid("nothing to redo"))?;
        let mapping = entry.forward_mapping;
        let changed_blocks = entry.changed_blocks.clone();
        match self.engine.load(entry.after.clone()) {
            Ok(()) => (),
            Err(error) => {
                self.redo.push(entry);
                return Err(engine_error(error));
            }
        }
        self.finish_change(id, revision, mapping)?;
        self.undo.push(entry);
        Ok(AppliedEditorChange {
            revision: self.revision,
            transaction: Some(id),
            changed_blocks,
        })
    }

    fn finish_change(
        &mut self,
        id: TransactionId,
        revision: EditorRevision,
        mapping: PositionMapping,
    ) -> Result<(), EditorError> {
        self.map_logical_state(mapping)?;
        self.revision = revision;
        self.next_transaction =
            id.0.checked_add(1)
                .expect("transaction capacity was checked before applying the edit");
        self.mappings.push(RevisionMapping { revision, mapping });
        self.offer_projection();
        Ok(())
    }

    fn map_logical_state(&mut self, mapping: PositionMapping) -> Result<(), EditorError> {
        for state in self.views.values_mut() {
            let selection = state.selection();
            *state = EditorViewState::new(map_selection(selection, mapping)?);
        }
        for stored in self.comments.values_mut() {
            if mapping.overlaps(selection_range(stored.anchor.range)?) {
                stored.anchor.orphaned = true;
            }
            stored.anchor.range = map_selection(stored.anchor.range, mapping)?;
            stored.canonical.range = stored.anchor.range;
        }
        for anchor in &mut self.anchors {
            anchor.position = DocumentPosition::from(
                u64::try_from(
                    mapping
                        .map(position(anchor.position)?)
                        .map_err(engine_error)?,
                )
                .map_err(|_| invalid("mapped position exceeds the public range"))?,
            );
        }
        Ok(())
    }

    fn map_position(
        &self,
        from: EditorRevision,
        through: EditorRevision,
        source_position: DocumentPosition,
    ) -> Result<DocumentPosition, EditorError> {
        if from > through || through > self.revision {
            return Err(invalid("revision mapping range is unavailable"));
        }
        let mut mapped = position(source_position)?;
        for revision_mapping in &self.mappings {
            if revision_mapping.revision > from && revision_mapping.revision <= through {
                mapped = revision_mapping.mapping.map(mapped).map_err(engine_error)?;
            }
        }
        Ok(DocumentPosition::from(u64::try_from(mapped).map_err(
            |_| invalid("mapped position exceeds the public range"),
        )?))
    }

    fn validate_selection(&self, selection: EditorSelection) -> Result<(), EditorError> {
        let (_, length) = selection_range(selection)?;
        let start = position(selection.start())?;
        let end = start
            .checked_add(length)
            .ok_or_else(|| invalid("selection range overflows"))?;
        if end > self.engine.scalar_len() {
            return Err(invalid("selection is outside the document"));
        }
        Ok(())
    }

    fn validated_range(&self, selection: EditorSelection) -> Result<(usize, usize), EditorError> {
        self.validate_selection(selection)?;
        selection_range(selection)
    }

    fn next_change_identity(&self) -> Result<(TransactionId, EditorRevision), EditorError> {
        let id = TransactionId(self.next_transaction);
        self.next_transaction
            .checked_add(1)
            .ok_or_else(|| invalid("transaction id space exhausted"))?;
        Ok((id, next_revision(self.revision)?))
    }

    fn projection(&self) -> Projection {
        let comments = self
            .comments
            .values()
            .map(|stored| stored.canonical.clone())
            .collect();
        let mut anchors = self.anchors.clone();
        anchors.sort_by_key(|anchor| (anchor.block, anchor.position));
        Projection {
            document_id: self.document_id,
            revision: self.revision,
            document: self.engine.snapshot(),
            comments,
            anchors,
        }
    }

    fn offer_projection(&mut self) {
        self.projections.offer(self.projection());
    }
}

/// The ParchMint-owned shared document session used by an editor adapter.
pub struct EditorCoreSession {
    inner: EditorSession<PrivateTextEngine>,
}

impl EditorCoreSession {
    /// Opens one canonical document in the private default engine.
    pub fn open(load: CanonicalDocumentLoad) -> Result<Self, EditorError> {
        Ok(Self {
            inner: EditorSession::open(PrivateTextEngine::default(), load)?,
        })
    }

    pub const fn document_id(&self) -> DocumentId {
        self.inner.document_id
    }

    pub const fn revision(&self) -> EditorRevision {
        self.inner.revision
    }

    pub const fn primary_block(&self) -> BlockId {
        self.inner.primary_block
    }

    pub fn attach_view(&mut self, view: ViewId) -> Result<(), EditorError> {
        self.inner.attach_view(view)
    }

    pub fn detach_view(&mut self, view: ViewId) -> Result<EditorViewState, EditorError> {
        self.inner.detach_view(view)
    }

    pub fn execute(
        &mut self,
        origin: EditorCommandOrigin,
        command: EditorCommand,
    ) -> Result<AppliedEditorChange, EditorError> {
        self.inner.execute(origin, command)
    }

    pub fn selection(&self, view: ViewId) -> Result<EditorSelection, EditorError> {
        self.inner
            .views
            .get(&view)
            .map(EditorViewState::selection)
            .ok_or(EditorError::UnknownView { view })
    }

    /// Captures copy data from the current semantic snapshot without exposing
    /// the private engine or canonical persistence tags to the mounted host.
    pub fn selection_clipboard(
        &self,
        view: ViewId,
    ) -> Result<Option<EditorClipboardContent>, EditorError> {
        let selection = self.selection(view)?;
        if selection.is_collapsed() {
            return Ok(None);
        }
        let (start, length) = self.inner.validated_range(selection)?;
        let end = start
            .checked_add(length)
            .ok_or_else(|| invalid("selection range overflows"))?;
        let (plain_text, restricted_html) =
            semantic_html::serialize_selection(&self.inner.engine.snapshot(), start, end);
        Ok(Some(EditorClipboardContent::new(
            self.revision(),
            selection,
            plain_text,
            Some(restricted_html),
        )))
    }

    pub fn set_style_catalog(&mut self, styles: StyleCatalogProjection) {
        self.inner.styles = styles;
    }

    pub fn comment_anchor(&self, comment: CommentId) -> Option<&CommentAnchor> {
        self.inner
            .comments
            .get(&comment)
            .map(|stored| &stored.anchor)
    }

    /// Maps a logical position from one historical revision through a later
    /// revision using the session's ParchMint-owned revision map.
    pub fn map_position(
        &self,
        from: EditorRevision,
        through: EditorRevision,
        position: DocumentPosition,
    ) -> Result<DocumentPosition, EditorError> {
        self.inner.map_position(from, through, position)
    }

    /// Returns a deterministic snapshot of the current shared state.
    pub fn canonical_projection(&self) -> CanonicalProjection {
        self.inner.projection().canonical()
    }

    /// Borrows current text for the crate-owned feasibility layout without
    /// exposing the private document engine.
    pub(crate) fn text_for_feasibility(&self) -> &str {
        self.inner.engine.text()
    }

    /// Drains one bounded projection work item for a background consumer.
    pub fn take_projection_work(&mut self) -> Option<CanonicalProjectionWork> {
        self.inner.projections.take().map(|batch| match batch {
            ProjectionBatch::Incremental(projection) => {
                CanonicalProjectionWork::Incremental(projection.canonical())
            }
            ProjectionBatch::FullSnapshot(projection) => {
                CanonicalProjectionWork::FullSnapshot(projection.canonical())
            }
        })
    }

    /// Produces the current projection without borrowing the session across
    /// asynchronous scheduling. Callers may request only the current revision.
    pub fn project(
        &self,
        through: EditorRevision,
    ) -> Result<AsyncResult<CanonicalProjection>, EditorError> {
        if through != self.inner.revision {
            return Err(invalid("only the current editor revision can be projected"));
        }
        let projection = self.canonical_projection();
        Ok(Box::pin(async move { projection }))
    }
}

fn validate_comments(comments: &[CanonicalComment], body_len: usize) -> Result<(), EditorError> {
    for comment in comments {
        validate_selection_length(comment.range, body_len)?;
    }
    Ok(())
}

fn validate_anchors(anchors: &[CanonicalAnchor], body_len: usize) -> Result<(), EditorError> {
    for anchor in anchors {
        if position(anchor.position)? > body_len {
            return Err(invalid("anchor is outside the document"));
        }
    }
    Ok(())
}

fn validate_selection_length(
    selection: EditorSelection,
    body_len: usize,
) -> Result<(), EditorError> {
    let (start, length) = selection_range(selection)?;
    let end = start
        .checked_add(length)
        .ok_or_else(|| invalid("selection range overflows"))?;
    if end > body_len {
        return Err(invalid("selection is outside the document"));
    }
    Ok(())
}

fn comment_anchor(
    block: BlockId,
    range: EditorSelection,
    body: &str,
) -> Result<CommentAnchor, EditorError> {
    let (start, length) = selection_range(range)?;
    let end = start
        .checked_add(length)
        .ok_or_else(|| invalid("comment range overflows"))?;
    let characters: Vec<char> = body.chars().collect();
    let before = start.saturating_sub(ANCHOR_CONTEXT_SCALARS);
    let after = end
        .saturating_add(ANCHOR_CONTEXT_SCALARS)
        .min(characters.len());
    Ok(CommentAnchor {
        block,
        range,
        quote: characters[start..end].iter().collect(),
        context_before: characters[before..start].iter().collect(),
        context_after: characters[end..after].iter().collect(),
        orphaned: false,
    })
}

fn map_selection(
    selection: EditorSelection,
    mapping: PositionMapping,
) -> Result<EditorSelection, EditorError> {
    let anchor = mapping
        .map(position(selection.anchor())?)
        .map_err(engine_error)?;
    let head = mapping
        .map(position(selection.head())?)
        .map_err(engine_error)?;
    Ok(EditorSelection::new(
        DocumentPosition::from(
            u64::try_from(anchor)
                .map_err(|_| invalid("mapped position exceeds the public range"))?,
        ),
        DocumentPosition::from(
            u64::try_from(head).map_err(|_| invalid("mapped position exceeds the public range"))?,
        ),
    ))
}

fn selection_range(selection: EditorSelection) -> Result<(usize, usize), EditorError> {
    let start = position(selection.start())?;
    let end = position(selection.end())?;
    Ok((start, end - start))
}

fn position(position: DocumentPosition) -> Result<usize, EditorError> {
    usize::try_from(position.value()).map_err(|_| invalid("document position is too large"))
}

fn next_revision(revision: EditorRevision) -> Result<EditorRevision, EditorError> {
    revision
        .value()
        .checked_add(1)
        .map(EditorRevision::from)
        .ok_or_else(|| invalid("editor revision space exhausted"))
}

fn canonical_style_id(style: StyleId) -> String {
    if style == parchmint_editor_api::StyleCatalog::body_id() {
        return "body".into();
    }
    if style == parchmint_editor_api::StyleCatalog::document_title_id() {
        return "document-title".into();
    }
    if style == parchmint_editor_api::StyleCatalog::heading_1_id() {
        return "heading-1".into();
    }
    if style == parchmint_editor_api::StyleCatalog::heading_2_id() {
        return "heading-2".into();
    }
    if style == parchmint_editor_api::StyleCatalog::heading_3_id() {
        return "heading-3".into();
    }
    if style == parchmint_editor_api::StyleCatalog::block_quote_id() {
        return "block-quote".into();
    }
    if style == parchmint_editor_api::StyleCatalog::verse_id() {
        return "verse".into();
    }
    style
        .as_bytes()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn derived_block_id(document: DocumentId, sequence: u64) -> BlockId {
    let mut bytes = *document.as_bytes();
    for (slot, byte) in bytes[8..].iter_mut().zip(sequence.to_be_bytes()) {
        *slot ^= byte;
    }
    BlockId::from_bytes(bytes)
}

fn validate_link_target(target: &str) -> Result<(), EditorError> {
    if target.is_empty()
        || target.starts_with(['/', '\\'])
        || target.starts_with("//")
        || target.contains('\\')
    {
        return Err(invalid("link target is not safe canonical HTML"));
    }
    if let Some((scheme, _)) = target.split_once(':') {
        if !matches!(
            scheme.to_ascii_lowercase().as_str(),
            "http" | "https" | "mailto"
        ) {
            return Err(invalid("link target is not safe canonical HTML"));
        }
    } else if target.split('/').any(|segment| segment == "..") {
        return Err(invalid("link target is not safe canonical HTML"));
    }
    Ok(())
}

fn invalid(reason: &'static str) -> EditorError {
    EditorError::InvalidCommand { reason }
}

fn engine_error(error: EngineError) -> EditorError {
    match error {
        EngineError::InvalidEdit => invalid("text edit is outside the document"),
        EngineError::InvalidSnapshot => invalid("document engine rejected the canonical snapshot"),
    }
}

#[cfg(test)]
mod editor_core_contract_tests;
