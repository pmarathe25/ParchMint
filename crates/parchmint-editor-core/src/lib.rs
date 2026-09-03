//! ParchMint's shared editor session.
//!
//! The session owns ParchMint identifiers, transactions, comments, anchors,
//! revision mappings, view state, and canonical projection scheduling. The
//! replaceable document engine is private and can only receive ParchMint-owned
//! semantic blocks and edits.

mod document_engine;
pub mod paste;
mod projection;
mod semantic_html;

use std::collections::BTreeMap;

use document_engine::{
    DocumentEngine, EngineEdit, EngineError, PositionMapping, PrivateTextEngine,
    SemanticDocumentSnapshot,
};
use projection::{Projection, ProjectionBatch, ProjectionQueue};

pub use parchmint_editor_api::{
    AnnotationValue, AsyncResult, AtomicBlockKind, BlockFormatKind, BlockId, CanonicalAnchor,
    CanonicalComment, CanonicalCommentAnchor, CanonicalCommentMessage, CanonicalDocumentLoad,
    CanonicalProjection, CommentId, DocumentId, DocumentPosition, EditorClipboardContent,
    EditorCommand, EditorCommandKind, EditorCommandOrigin, EditorError, EditorRevision,
    EditorSelection, EditorViewState, InlineMarkKind, ListDepthChange, SemanticBlock,
    SemanticBlockKind, SemanticDocument, SemanticFragment, SemanticFragmentBlock,
    SemanticInlineMark, SemanticMarkRange, StyleCatalog, StyleCatalogProjection, StyleId, ViewId,
};

const DEFAULT_PROJECTION_CAPACITY: usize = 2;
const ANCHOR_CONTEXT_SCALARS: usize = 16;
const MAX_SEMANTIC_FRAGMENT_BLOCKS: usize = 4_096;
const MAX_SEMANTIC_FRAGMENT_SCALARS: usize = 1_000_000;

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
    anchor: Option<CommentAnchor>,
}

#[derive(Debug, Clone)]
struct UndoEntry {
    before: SemanticDocumentSnapshot,
    after: SemanticDocumentSnapshot,
    before_comments: BTreeMap<CommentId, StoredComment>,
    after_comments: BTreeMap<CommentId, StoredComment>,
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
            revision,
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
            let anchor = stored_anchor(&comment.anchor);
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
            revision,
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
            EditorCommandKind::ReplaceRangeWithSemanticText { range, text, marks } => {
                let (at, removed) = self.validated_range(*range)?;
                let text_len = text.chars().count();
                let mut engine_marks = Vec::with_capacity(marks.len());
                for mark in marks {
                    let start = position(mark.range().start())?;
                    let end = position(mark.range().end())?;
                    if start >= end || end > text_len {
                        return Err(invalid("pasted semantic mark is outside inserted text"));
                    }
                    if let SemanticInlineMark::Link(target) = mark.mark() {
                        validate_link_target(target)?;
                    }
                    engine_marks.push(document_engine::EngineMark {
                        start,
                        end,
                        mark: mark.mark().clone(),
                    });
                }
                self.apply_new_semantic_edit(
                    EngineEdit::new(at, removed, text.clone()),
                    engine_marks,
                )
            }
            EditorCommandKind::ReplaceRangeWithSemanticFragment { range, fragment } => {
                let (start, length) = self.validated_range(*range)?;
                let end = start
                    .checked_add(length)
                    .ok_or_else(|| invalid("selection range overflows"))?;
                let blocks = validate_semantic_fragment(fragment)?;
                self.apply_new_fragment(view, start, end, blocks)
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
            EditorCommandKind::SplitBlock { selection } => {
                self.validate_selection(*selection)?;
                self.apply_split_block(view, *selection)
            }
            EditorCommandKind::InsertSoftBreak { selection } => {
                let (at, removed) = self.validated_range(*selection)?;
                self.apply_new_edit(EngineEdit::new(at, removed, "\n".into()))
            }
            EditorCommandKind::AdjustListDepth { range, change } => {
                let (start, length) = self.validated_range(*range)?;
                let end = start
                    .checked_add(length)
                    .ok_or_else(|| invalid("selection range overflows"))?;
                self.apply_optional_format_change(|engine| {
                    engine.adjust_list_depth(start, end, *change)
                })
            }
            EditorCommandKind::CreateComment { comment } => {
                self.apply_annotation_change(|session| session.create_comment(comment.clone()))
            }
            EditorCommandKind::ReplyToComment { thread, message } => {
                self.apply_annotation_change(|session| {
                    session.reply_to_comment(*thread, message.clone())
                })
            }
            EditorCommandKind::SetCommentResolved { thread, resolved } => self
                .apply_annotation_change(|session| {
                    session.set_comment_resolved(*thread, *resolved)
                }),
            EditorCommandKind::DeleteCommentThread { thread } => {
                self.apply_annotation_change(|session| session.delete_comment_thread(*thread))
            }
            EditorCommandKind::DeleteCommentMessage { thread, message } => self
                .apply_annotation_change(|session| {
                    session.delete_comment_message(*thread, *message)
                }),
            EditorCommandKind::EditCommentMessage {
                thread,
                message,
                body,
            } => self.apply_annotation_change(|session| {
                session.edit_comment_message(*thread, *message, body)
            }),
            EditorCommandKind::ReattachComment { thread, range } => {
                self.apply_annotation_change(|session| session.reattach_comment(*thread, *range))
            }
            EditorCommandKind::ConvertCommentToDocument { thread } => {
                self.apply_annotation_change(|session| session.convert_comment_to_document(*thread))
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
        let before_comments = self.comments.clone();
        let change = self.engine.apply(edit).map_err(engine_error)?;
        let after = self.engine.snapshot();
        let changed_blocks = change.changed_blocks().to_vec();
        self.finish_change(id, revision, change.mapping())?;
        self.undo.push(UndoEntry {
            before,
            after,
            before_comments,
            after_comments: self.comments.clone(),
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

    fn apply_new_semantic_edit(
        &mut self,
        edit: EngineEdit,
        marks: Vec<document_engine::EngineMark>,
    ) -> Result<AppliedEditorChange, EditorError> {
        let (id, revision) = self.next_change_identity()?;
        let before = self.engine.snapshot();
        let before_comments = self.comments.clone();
        let change = self
            .engine
            .replace_with_marks(edit, marks)
            .map_err(engine_error)?;
        let after = self.engine.snapshot();
        let changed_blocks = change.changed_blocks().to_vec();
        self.finish_change(id, revision, change.mapping())?;
        self.undo.push(UndoEntry {
            before,
            after,
            before_comments,
            after_comments: self.comments.clone(),
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

    fn apply_new_fragment(
        &mut self,
        view: ViewId,
        start: usize,
        end: usize,
        blocks: Vec<document_engine::EngineFragmentBlock>,
    ) -> Result<AppliedEditorChange, EditorError> {
        let fresh_count = blocks
            .len()
            .checked_add(1)
            .ok_or_else(|| invalid("semantic fragment block count overflow"))?;
        let next_block = self
            .next_block
            .checked_add(
                u64::try_from(fresh_count)
                    .map_err(|_| invalid("semantic fragment block count exceeds id space"))?,
            )
            .ok_or_else(|| invalid("semantic block id space exhausted"))?;
        let fresh_ids = (0..fresh_count)
            .map(|offset| {
                let sequence = self
                    .next_block
                    .checked_add(
                        u64::try_from(offset)
                            .map_err(|_| invalid("semantic fragment id offset overflow"))?,
                    )
                    .ok_or_else(|| invalid("semantic block id space exhausted"))?;
                Ok(derived_block_id(self.document_id, sequence))
            })
            .collect::<Result<Vec<_>, EditorError>>()?;
        let (id, revision) = self.next_change_identity()?;
        let before = self.engine.snapshot();
        let before_comments = self.comments.clone();
        let change = self
            .engine
            .replace_with_fragment(start, end, blocks, fresh_ids)
            .map_err(engine_error)?;
        let after = self.engine.snapshot();
        let changed_blocks = change.changed_blocks().to_vec();
        self.finish_change(id, revision, change.mapping())?;
        let caret = if start == end {
            change.mapping().inserted_end().map_err(engine_error)?
        } else {
            change.mapping().map(end).map_err(engine_error)?
        };
        self.views.insert(
            view,
            EditorViewState::new(EditorSelection::new(
                DocumentPosition::from(
                    u64::try_from(caret).map_err(|_| invalid("document position overflow"))?,
                ),
                DocumentPosition::from(
                    u64::try_from(caret).map_err(|_| invalid("document position overflow"))?,
                ),
            )),
        );
        self.next_block = next_block;
        self.undo.push(UndoEntry {
            before,
            after,
            before_comments,
            after_comments: self.comments.clone(),
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
        let before_comments = self.comments.clone();
        let change = operation(&mut self.engine).map_err(engine_error)?;
        let after = self.engine.snapshot();
        let changed_blocks = change.changed_blocks().to_vec();
        self.finish_change(id, revision, change.mapping())?;
        self.undo.push(UndoEntry {
            before,
            after,
            before_comments,
            after_comments: self.comments.clone(),
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

    fn apply_optional_format_change(
        &mut self,
        operation: impl FnOnce(&mut E) -> Result<Option<document_engine::EngineChange>, EngineError>,
    ) -> Result<AppliedEditorChange, EditorError> {
        let before = self.engine.snapshot();
        let before_comments = self.comments.clone();
        let Some(change) = operation(&mut self.engine).map_err(engine_error)? else {
            return Ok(AppliedEditorChange {
                revision: self.revision,
                transaction: None,
                changed_blocks: Vec::new(),
            });
        };
        let (id, revision) = self.next_change_identity()?;
        let after = self.engine.snapshot();
        let changed_blocks = change.changed_blocks().to_vec();
        self.finish_change(id, revision, change.mapping())?;
        self.undo.push(UndoEntry {
            before,
            after,
            before_comments,
            after_comments: self.comments.clone(),
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

    fn apply_split_block(
        &mut self,
        view: ViewId,
        selection: EditorSelection,
    ) -> Result<AppliedEditorChange, EditorError> {
        let (start, length) = self.validated_range(selection)?;
        let end = start
            .checked_add(length)
            .ok_or_else(|| invalid("selection range overflows"))?;
        let after_id = derived_block_id(self.document_id, self.next_block);
        let (id, revision) = self.next_change_identity()?;
        let before = self.engine.snapshot();
        let before_comments = self.comments.clone();
        let change = self
            .engine
            .split_block(start, end, after_id)
            .map_err(engine_error)?;
        let after = self.engine.snapshot();
        let changed_blocks = change.changed_blocks().to_vec();
        self.finish_change(id, revision, change.mapping())?;
        if change.mapping() != PositionMapping::identity() {
            self.next_block = self
                .next_block
                .checked_add(1)
                .ok_or_else(|| invalid("semantic block id space exhausted"))?;
            let caret = DocumentPosition::from(
                u64::try_from(start + 1).map_err(|_| invalid("document position overflow"))?,
            );
            self.views.insert(
                view,
                EditorViewState::new(EditorSelection::new(caret, caret)),
            );
        }
        self.undo.push(UndoEntry {
            before,
            after,
            before_comments,
            after_comments: self.comments.clone(),
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
        let before_comments = self.comments.clone();
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
            before_comments,
            after_comments: self.comments.clone(),
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
        self.comments = entry.before_comments.clone();
        self.finish_change_without_comment_mapping(id, revision, mapping)?;
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
        self.comments = entry.after_comments.clone();
        self.finish_change_without_comment_mapping(id, revision, mapping)?;
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

    fn finish_change_without_comment_mapping(
        &mut self,
        id: TransactionId,
        revision: EditorRevision,
        mapping: PositionMapping,
    ) -> Result<(), EditorError> {
        for state in self.views.values_mut() {
            *state = EditorViewState::new(map_selection(state.selection(), mapping)?);
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
        self.revision = revision;
        self.next_transaction = id.0.checked_add(1).expect("transaction capacity checked");
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
            let Some(anchor) = stored.anchor.as_mut() else {
                continue;
            };
            if anchor.orphaned {
                continue;
            }
            if mapping.overlaps(selection_range(anchor.range)?) {
                anchor.orphaned = true;
            }
            anchor.range = map_selection(anchor.range, mapping)?;
            update_canonical_anchor(&mut stored.canonical.anchor, anchor);
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

    fn apply_annotation_change(
        &mut self,
        operation: impl FnOnce(&mut Self) -> Result<(), EditorError>,
    ) -> Result<AppliedEditorChange, EditorError> {
        let (id, revision) = self.next_change_identity()?;
        let before = self.engine.snapshot();
        let before_comments = self.comments.clone();
        if let Err(error) = operation(self) {
            self.comments = before_comments;
            return Err(error);
        }
        let after = self.engine.snapshot();
        self.revision = revision;
        self.next_transaction = id.0.checked_add(1).expect("transaction capacity checked");
        self.mappings.push(RevisionMapping {
            revision,
            mapping: PositionMapping::identity(),
        });
        self.offer_projection();
        self.undo.push(UndoEntry {
            before,
            after,
            before_comments,
            after_comments: self.comments.clone(),
            forward_mapping: PositionMapping::identity(),
            changed_blocks: Vec::new(),
        });
        self.redo.clear();
        Ok(AppliedEditorChange {
            revision,
            transaction: Some(id),
            changed_blocks: Vec::new(),
        })
    }

    fn create_comment(&mut self, mut comment: CanonicalComment) -> Result<(), EditorError> {
        if self.comments.contains_key(&comment.id) {
            return Err(invalid("duplicate comment id"));
        }
        validate_comment(&comment, self.engine.scalar_len())?;
        if let CanonicalCommentAnchor::Text {
            range,
            unknown_fields,
            ..
        } = &comment.anchor
        {
            let text = self.engine.snapshot().plain_text();
            let captured = comment_anchor(self.primary_block, *range, &text)?;
            comment.anchor = CanonicalCommentAnchor::Text {
                block: captured.block,
                range: captured.range,
                quote: captured.quote,
                context_before: captured.context_before,
                context_after: captured.context_after,
                orphaned: false,
                unknown_fields: unknown_fields.clone(),
            };
        }
        let anchor = stored_anchor(&comment.anchor);
        self.comments.insert(
            comment.id,
            StoredComment {
                canonical: comment,
                anchor,
            },
        );
        Ok(())
    }

    fn reply_to_comment(
        &mut self,
        thread: CommentId,
        message: CanonicalCommentMessage,
    ) -> Result<(), EditorError> {
        validate_message(&message)?;
        if self.comments.values().any(|stored| {
            stored
                .canonical
                .messages
                .iter()
                .any(|current| current.id == message.id)
        }) {
            return Err(invalid("duplicate comment message id"));
        }
        self.comments
            .get_mut(&thread)
            .ok_or_else(|| invalid("unknown comment thread"))?
            .canonical
            .messages
            .push(message);
        Ok(())
    }

    fn set_comment_resolved(
        &mut self,
        thread: CommentId,
        resolved: bool,
    ) -> Result<(), EditorError> {
        let stored = self
            .comments
            .get_mut(&thread)
            .ok_or_else(|| invalid("unknown comment thread"))?;
        if stored.canonical.resolved == resolved {
            return Err(invalid("comment resolved state is unchanged"));
        }
        stored.canonical.resolved = resolved;
        Ok(())
    }

    fn delete_comment_thread(&mut self, thread: CommentId) -> Result<(), EditorError> {
        self.comments
            .remove(&thread)
            .map(|_| ())
            .ok_or_else(|| invalid("unknown comment thread"))
    }

    fn delete_comment_message(
        &mut self,
        thread: CommentId,
        message: CommentId,
    ) -> Result<(), EditorError> {
        let stored = self
            .comments
            .get_mut(&thread)
            .ok_or_else(|| invalid("unknown comment thread"))?;
        let before = stored.canonical.messages.len();
        stored
            .canonical
            .messages
            .retain(|current| current.id != message);
        if before == stored.canonical.messages.len() {
            return Err(invalid("unknown comment message"));
        }
        if stored.canonical.messages.is_empty() {
            self.comments.remove(&thread);
        }
        Ok(())
    }

    fn edit_comment_message(
        &mut self,
        thread: CommentId,
        message: CommentId,
        body: &str,
    ) -> Result<(), EditorError> {
        if body.trim().is_empty() {
            return Err(invalid("comment body must not be empty"));
        }
        let stored = self
            .comments
            .get_mut(&thread)
            .ok_or_else(|| invalid("unknown comment thread"))?;
        let stored_message = stored
            .canonical
            .messages
            .iter_mut()
            .find(|current| current.id == message)
            .ok_or_else(|| invalid("unknown comment message"))?;
        if stored_message.body == body {
            return Err(invalid("comment body is unchanged"));
        }
        stored_message.body = body.to_owned();
        Ok(())
    }

    fn reattach_comment(
        &mut self,
        thread: CommentId,
        range: EditorSelection,
    ) -> Result<(), EditorError> {
        if range.is_collapsed() {
            return Err(invalid(
                "comment reattachment requires a non-empty selection",
            ));
        }
        self.validate_selection(range)?;
        let text = self.engine.snapshot().plain_text();
        let anchor = comment_anchor(self.primary_block, range, &text)?;
        let stored = self
            .comments
            .get_mut(&thread)
            .ok_or_else(|| invalid("unknown comment thread"))?;
        update_canonical_anchor(&mut stored.canonical.anchor, &anchor);
        stored.anchor = Some(anchor);
        Ok(())
    }

    fn convert_comment_to_document(&mut self, thread: CommentId) -> Result<(), EditorError> {
        let stored = self
            .comments
            .get_mut(&thread)
            .ok_or_else(|| invalid("unknown comment thread"))?;
        let CanonicalCommentAnchor::Text {
            orphaned: true,
            unknown_fields,
            ..
        } = &stored.canonical.anchor
        else {
            return Err(invalid(
                "only an orphaned comment can become document-level",
            ));
        };
        stored.canonical.anchor = CanonicalCommentAnchor::Document {
            unknown_fields: unknown_fields.clone(),
        };
        stored.anchor = None;
        Ok(())
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

    pub fn active_style(&self, view: ViewId) -> Result<StyleId, EditorError> {
        let position = position(self.selection(view)?.head())?;
        let snapshot = self.inner.engine.snapshot();
        let mut offset = 0usize;
        for block in &snapshot.blocks {
            let length = if matches!(
                block.kind,
                SemanticBlockKind::SceneBreak | SemanticBlockKind::PageBreak
            ) {
                1
            } else {
                block.text.chars().count()
            };
            let end = offset
                .checked_add(length)
                .ok_or_else(|| invalid("document position overflow"))?;
            if position <= end {
                let assigned = block
                    .attributes
                    .get("data-style-id")
                    .and_then(|value| style_id_from_canonical(value));
                return Ok(assigned
                    .filter(|style| self.inner.styles.catalog().get(*style).is_some())
                    .unwrap_or_else(|| default_style_for_block(block.kind)));
            }
            offset = end
                .checked_add(1)
                .ok_or_else(|| invalid("document position overflow"))?;
        }
        Err(invalid("selection is outside semantic blocks"))
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

    pub fn style_catalog(&self) -> &StyleCatalogProjection {
        &self.inner.styles
    }

    pub fn comment_anchor(&self, comment: CommentId) -> Option<&CommentAnchor> {
        self.inner
            .comments
            .get(&comment)
            .and_then(|stored| stored.anchor.as_ref())
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
    let mut message_ids = std::collections::BTreeSet::new();
    for comment in comments {
        validate_comment(comment, body_len)?;
        for message in &comment.messages {
            if !message_ids.insert(message.id) {
                return Err(invalid("duplicate comment message id"));
            }
        }
    }
    Ok(())
}

fn validate_comment(comment: &CanonicalComment, body_len: usize) -> Result<(), EditorError> {
    if comment.messages.is_empty() {
        return Err(invalid("comment thread requires a message"));
    }
    for message in &comment.messages {
        validate_message(message)?;
    }
    match &comment.anchor {
        CanonicalCommentAnchor::Document { .. } => Ok(()),
        CanonicalCommentAnchor::Text {
            range,
            quote,
            context_before,
            context_after,
            orphaned,
            ..
        } => {
            if !orphaned {
                validate_selection_length(*range, body_len)?;
            }
            validate_annotation_text(quote)?;
            validate_annotation_text(context_before)?;
            validate_annotation_text(context_after)
        }
    }
}

fn validate_message(message: &CanonicalCommentMessage) -> Result<(), EditorError> {
    if message.body.trim().is_empty() {
        return Err(invalid("comment body must contain text"));
    }
    validate_annotation_text(&message.body)
}

fn validate_annotation_text(text: &str) -> Result<(), EditorError> {
    if text
        .chars()
        .any(|character| character != '\n' && character.is_control())
    {
        return Err(invalid(
            "comment text contains unsupported control characters",
        ));
    }
    Ok(())
}

fn stored_anchor(anchor: &CanonicalCommentAnchor) -> Option<CommentAnchor> {
    match anchor {
        CanonicalCommentAnchor::Document { .. } => None,
        CanonicalCommentAnchor::Text {
            block,
            range,
            quote,
            context_before,
            context_after,
            orphaned,
            ..
        } => Some(CommentAnchor {
            block: *block,
            range: *range,
            quote: quote.clone(),
            context_before: context_before.clone(),
            context_after: context_after.clone(),
            orphaned: *orphaned,
        }),
    }
}

fn update_canonical_anchor(target: &mut CanonicalCommentAnchor, source: &CommentAnchor) {
    let unknown_fields = match target {
        CanonicalCommentAnchor::Document { unknown_fields }
        | CanonicalCommentAnchor::Text { unknown_fields, .. } => std::mem::take(unknown_fields),
    };
    *target = CanonicalCommentAnchor::Text {
        block: source.block,
        range: source.range,
        quote: source.quote.clone(),
        context_before: source.context_before.clone(),
        context_after: source.context_after.clone(),
        orphaned: source.orphaned,
        unknown_fields,
    };
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

fn validate_semantic_fragment(
    fragment: &SemanticFragment,
) -> Result<Vec<document_engine::EngineFragmentBlock>, EditorError> {
    if fragment.blocks().is_empty() {
        return Err(invalid("semantic fragment has no blocks"));
    }
    if fragment.blocks().len() > MAX_SEMANTIC_FRAGMENT_BLOCKS
        || fragment.scalar_len() > MAX_SEMANTIC_FRAGMENT_SCALARS
    {
        return Err(invalid(
            "semantic fragment exceeds the editor resource limit",
        ));
    }
    let mut output = Vec::with_capacity(fragment.blocks().len());
    let mut previous_list: Option<(SemanticBlockKind, usize)> = None;
    for block in fragment.blocks() {
        let list = matches!(
            block.kind(),
            SemanticBlockKind::UnorderedListItem | SemanticBlockKind::OrderedListItem
        );
        if !matches!(
            block.kind(),
            SemanticBlockKind::Paragraph
                | SemanticBlockKind::UnorderedListItem
                | SemanticBlockKind::OrderedListItem
                | SemanticBlockKind::BlockQuote
        ) {
            return Err(invalid(
                "semantic fragment contains an unsupported block kind",
            ));
        }
        if !list && block.list_depth() != 0 {
            return Err(invalid("non-list semantic fragment block has list depth"));
        }
        if list {
            match previous_list {
                None if block.list_depth() != 0 => {
                    return Err(invalid("semantic fragment list starts below depth zero"));
                }
                Some((_, previous_depth))
                    if block.list_depth() > previous_depth.saturating_add(1) =>
                {
                    return Err(invalid("semantic fragment skips a list depth"));
                }
                _ => {}
            }
            previous_list = Some((block.kind(), block.list_depth()));
        } else {
            previous_list = None;
        }
        if block.text().chars().any(|character| {
            character == '\u{fffc}'
                || character == '\r'
                || character.is_control() && !matches!(character, '\n' | '\t')
        }) {
            return Err(invalid(
                "semantic fragment contains unsupported text controls",
            ));
        }
        let text_len = block.text().chars().count();
        let mut marks = Vec::with_capacity(block.marks().len());
        for mark in block.marks() {
            let start = position(mark.range().start())?;
            let end = position(mark.range().end())?;
            if start >= end || end > text_len {
                return Err(invalid("semantic fragment mark is outside its block"));
            }
            if let SemanticInlineMark::Link(target) = mark.mark() {
                validate_link_target(target)?;
            }
            marks.push(document_engine::EngineMark {
                start,
                end,
                mark: mark.mark().clone(),
            });
        }
        output.push(document_engine::EngineFragmentBlock {
            kind: block.kind(),
            text: block.text().to_owned(),
            marks,
            list_depth: block.list_depth(),
        });
    }
    Ok(output)
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

fn default_style_for_block(kind: SemanticBlockKind) -> StyleId {
    match kind {
        SemanticBlockKind::Heading1 => StyleCatalog::heading_1_id(),
        SemanticBlockKind::Heading2 => StyleCatalog::heading_2_id(),
        SemanticBlockKind::Heading3 => StyleCatalog::heading_3_id(),
        SemanticBlockKind::BlockQuote => StyleCatalog::block_quote_id(),
        _ => StyleCatalog::body_id(),
    }
}

fn style_id_from_canonical(value: &str) -> Option<StyleId> {
    let reserved = match value {
        "body" => Some(StyleCatalog::body_id()),
        "document-title" => Some(StyleCatalog::document_title_id()),
        "heading-1" => Some(StyleCatalog::heading_1_id()),
        "heading-2" => Some(StyleCatalog::heading_2_id()),
        "heading-3" => Some(StyleCatalog::heading_3_id()),
        "block-quote" => Some(StyleCatalog::block_quote_id()),
        "verse" => Some(StyleCatalog::verse_id()),
        _ => None,
    };
    if reserved.is_some() {
        return reserved;
    }
    if value.len() != 32 {
        return None;
    }
    let mut bytes = [0u8; 16];
    for (index, byte) in bytes.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&value[index * 2..index * 2 + 2], 16).ok()?;
    }
    Some(StyleId::from_bytes(bytes))
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
