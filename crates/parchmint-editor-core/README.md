# `parchmint-editor-core`

## What it does

`parchmint-editor-core` owns the editable state of one open document. It gives
both editor panes the same text, styles, comments, anchors, undo history, and
revision number. It also produces deterministic snapshots for project saves.

This crate contains no `iced` code. It gives the desktop editor a reusable
session model without exposing an editor-engine library to the rest of
ParchMint.

## How it works

```text
Editor command
  -> validate ParchMint IDs and positions
  -> apply one transaction to the document engine
  -> update comments, anchors, undo history, and revision mappings
  -> notify mounted views about changed blocks
  -> queue a deterministic project-file snapshot
```

The core owns stable block and comment IDs, the applied style catalog, and the
durable comment anchors. It also owns the transaction format, document
revision sequence, shared undo order, and the rules that turn a revision into
ParchMint project files. A document engine only stores and edits the rich
text that the core gives it.

The core keeps one logical view record per mounted pane. A record contains the
view's selection. A text change maps these positions for every mounted view.
Per-view search and spellcheck decorations are supplied by the mounted adapter
record, which also separately owns pixel scroll, focus, viewport geometry, and
layout caches. Neither crate copies the editable document into each view.

The core owns the bounded projection queue: offers for consecutive revisions
replace one trailing pending batch, and an overflowing pending set restarts
from a complete snapshot of the newest required revision. Edits never wait on
the queue, but a save cannot be acknowledged for a revision whose projection
was never delivered.

## Interface

`parchmint-editor-iced` uses this crate through a small ParchMint-owned API.
Other application crates use
[`parchmint-editor-api`](../parchmint-editor-api/README.md) instead:

```rust
pub struct EditorCoreSession {
    inner: EditorSession<PrivateTextEngine>,
}

impl EditorCoreSession {
    pub fn open(load: CanonicalDocumentLoad) -> Result<Self, EditorError>;
    pub const fn document_id(&self) -> DocumentId;
    pub const fn revision(&self) -> EditorRevision;
    pub const fn primary_block(&self) -> BlockId;
    pub fn attach_view(&mut self, view: ViewId) -> Result<(), EditorError>;
    pub fn detach_view(&mut self, view: ViewId) -> Result<EditorViewState, EditorError>;
    pub fn execute(
        &mut self,
        origin: EditorCommandOrigin,
        command: EditorCommand,
    ) -> Result<AppliedEditorChange, EditorError>;
    pub fn selection(&self, view: ViewId) -> Result<EditorSelection, EditorError>;
    pub fn selection_clipboard(
        &self,
        view: ViewId,
    ) -> Result<Option<EditorClipboardContent>, EditorError>;
    pub fn active_style(&self, view: ViewId) -> Result<StyleId, EditorError>;
    pub fn set_style_catalog(&mut self, styles: StyleCatalogProjection);
    pub fn style_catalog(&self) -> &StyleCatalogProjection;
    pub fn comment_anchor(&self, comment: CommentId) -> Option<&CommentAnchor>;
    pub fn map_position(
        &self,
        from: EditorRevision,
        through: EditorRevision,
        position: DocumentPosition,
    ) -> Result<DocumentPosition, EditorError>;
    pub fn canonical_projection(&self) -> CanonicalProjection;
    pub fn project(&self, through: EditorRevision)
        -> Result<AsyncResult<CanonicalProjection>, EditorError>;
    pub fn take_projection_work(&mut self) -> Option<CanonicalProjectionWork>;
}
```

Projection requests are pinned: only the session's current revision can be
projected, and `take_projection_work` drains one bounded work item at a time
for a background consumer.

The private `DocumentEngine` seam keeps the document engine replaceable without
giving it control of ParchMint data:

```rust
trait DocumentEngine {
    fn load(&mut self, document: SemanticDocumentSnapshot) -> Result<(), EngineError>;
    fn apply(&mut self, edit: EngineEdit) -> Result<EngineChange, EngineError>;
    fn replace_with_marks(
        &mut self,
        edit: EngineEdit,
        marks: Vec<EngineMark>,
    ) -> Result<EngineChange, EngineError>;
    fn replace_with_fragment(
        &mut self,
        start: usize,
        end: usize,
        blocks: Vec<EngineFragmentBlock>,
        fresh_ids: Vec<BlockId>,
    ) -> Result<EngineChange, EngineError>;
    fn toggle_inline_mark(
        &mut self,
        start: usize,
        end: usize,
        mark: SemanticInlineMark,
    ) -> Result<EngineChange, EngineError>;
    fn set_link(
        &mut self,
        start: usize,
        end: usize,
        target: Option<String>,
    ) -> Result<EngineChange, EngineError>;
    fn toggle_block_format(
        &mut self,
        start: usize,
        end: usize,
        target: SemanticBlockKind,
    ) -> Result<EngineChange, EngineError>;
    fn insert_atomic_block(
        &mut self,
        at: usize,
        kind: AtomicBlockKind,
        atomic_id: BlockId,
        after_id: BlockId,
    ) -> Result<EngineChange, EngineError>;
    fn split_block(
        &mut self,
        start: usize,
        end: usize,
        after_id: BlockId,
    ) -> Result<EngineChange, EngineError>;
    fn adjust_list_depth(
        &mut self,
        start: usize,
        end: usize,
        change: ListDepthChange,
    ) -> Result<Option<EngineChange>, EngineError>;
    fn apply_paragraph_style(
        &mut self,
        start: usize,
        end: usize,
        style: String,
    ) -> Result<EngineChange, EngineError>;
    fn snapshot(&self) -> SemanticDocumentSnapshot;
    fn text(&self) -> &str;
    fn scalar_len(&self) -> usize;
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
```

`EngineEdit`, `EngineChange`, `SemanticDocumentSnapshot`, and
`SemanticBlockSnapshot` are ParchMint-owned private types. They use ParchMint
block and text positions. A future engine can replace the adapter without
changing `EditorAdapter` or the project format.

## Implementation

The document engine stays behind `DocumentEngine`. Its types, identifiers,
transactions, undo records, storage, and serialization remain private.
ParchMint does not save an engine's format or use engine IDs as ParchMint IDs.
ParchMint applies its own forward and inverse mappings through the seam. The
session flow is (simplified):

```rust
fn execute(
    session: &mut EditorSession<E>,
    origin: EditorCommandOrigin,
    command: EditorCommand,
) -> Result<AppliedEditorChange, EditorError> {
    // Rejection happens first: unknown view, stale revision, invalid range.
    let change = session.engine.apply(edit)?;
    session.map_logical_state(change.mapping()); // view selections, comment anchors, anchors
    session.undo.push(snapshot_entry(&change));
    session.redo.clear();
    session.record_revision(change.mapping());   // revision map entry and projection offer
    Ok(applied(transaction_id, revision, change.changed_blocks()))
}
```

Undo restoration reloads the engine's before-edit snapshot and reverses the
stored position mapping; redo reloads the after-edit snapshot with the forward
mapping.

Projection work runs away from the UI loop. Each edit offers a canonical
projection of the session's current state. The queue keeps a bounded pending
set (capacity two) and replaces a trailing incremental batch when the newest
offer is its immediate successor, so consecutive typing coalesces into fewer
pending batches. When the pending set overflows, the next drain returns one
complete `FullSnapshot` of the newest revision. A save pins the exact revision
it needs; the persistence coordinator acknowledges it only after that
projection is delivered.

The engine must preserve stable identity, anchor mapping, two-view undo,
deterministic projection, and large-document behavior through ParchMint-owned
types and rules.
