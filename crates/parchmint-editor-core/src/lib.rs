//! ParchMint's shared editor session.
//!
//! The session owns ParchMint identifiers, transactions, comments, anchors,
//! revision mappings, view state, and canonical projection scheduling. The
//! replaceable document engine is private and can only receive ParchMint-owned
//! semantic blocks and edits.

mod document_engine;
mod projection;

use std::collections::BTreeMap;

use document_engine::{
    DocumentEngine, EngineEdit, EngineError, PositionMapping, PrivateTextEngine,
    SemanticBlockSnapshot, SemanticDocumentSnapshot,
};
use projection::{Projection, ProjectionBatch, ProjectionQueue};

pub use parchmint_editor_api::{
    AsyncResult, BlockId, CanonicalAnchor, CanonicalComment, CanonicalDocumentLoad,
    CanonicalProjection, CommentId, DocumentId, DocumentPosition, EditorCommand, EditorCommandKind,
    EditorCommandOrigin, EditorError, EditorRevision, EditorSelection, EditorViewState,
    StyleCatalogProjection, ViewId,
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
    forward: EngineEdit,
    inverse: EngineEdit,
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
        let body_len = body.chars().count();

        validate_comments(&comments, body_len)?;
        validate_anchors(&anchors, body_len)?;

        let mut stored_comments = BTreeMap::new();
        for comment in comments {
            if stored_comments.contains_key(&comment.id) {
                return Err(invalid("duplicate comment id"));
            }
            let anchor = comment_anchor(primary_block, comment.range, &body)?;
            stored_comments.insert(
                comment.id,
                StoredComment {
                    canonical: comment,
                    anchor,
                },
            );
        }

        engine
            .load(SemanticDocumentSnapshot {
                blocks: vec![SemanticBlockSnapshot {
                    id: primary_block,
                    text: body,
                }],
            })
            .map_err(engine_error)?;

        let mut session = Self {
            engine,
            document_id,
            primary_block,
            revision: EditorRevision::default(),
            next_transaction: 1,
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
            EditorCommandKind::Undo => self.apply_undo(),
            EditorCommandKind::Redo => self.apply_redo(),
            EditorCommandKind::ApplyParagraphStyle { .. } => Err(invalid(
                "paragraph style transactions are not available yet",
            )),
        }
    }

    fn apply_new_edit(&mut self, edit: EngineEdit) -> Result<AppliedEditorChange, EditorError> {
        let (id, revision) = self.next_change_identity()?;
        let change = self.engine.apply(edit.clone()).map_err(engine_error)?;
        let inverse = EngineEdit::new(
            edit.at(),
            edit.inserted().chars().count(),
            change.removed_text().to_owned(),
        );
        let changed_blocks = change.changed_blocks().to_vec();
        self.finish_change(id, revision, change.mapping())?;
        self.undo.push(UndoEntry {
            forward: edit,
            inverse,
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
        let change = match self.engine.apply(entry.inverse.clone()) {
            Ok(change) => change,
            Err(error) => {
                self.undo.push(entry);
                return Err(engine_error(error));
            }
        };
        let changed_blocks = change.changed_blocks().to_vec();
        self.finish_change(id, revision, change.mapping())?;
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
        let change = match self.engine.apply(entry.forward.clone()) {
            Ok(change) => change,
            Err(error) => {
                self.redo.push(entry);
                return Err(engine_error(error));
            }
        };
        let changed_blocks = change.changed_blocks().to_vec();
        self.finish_change(id, revision, change.mapping())?;
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
            blocks: self.engine.blocks(),
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
