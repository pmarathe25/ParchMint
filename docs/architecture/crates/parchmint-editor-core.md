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

The core owns stable block, style, comment, and anchor IDs. It also owns the
transaction format, document revision sequence, shared undo order, and the
rules that turn a revision into ParchMint project files. A document engine only
stores and edits the rich text that the core gives it.

The core keeps one logical view record per mounted pane. A record contains the
view's selection and local-search positions. A text change maps these positions
for every mounted view. The `iced` adapter separately owns pixel scroll, focus,
viewport geometry, and layout caches. Neither crate copies the editable
document into each view.

The core owns the bounded projection queue and coalesces obsolete revisions
after consumers advance. When incremental conversion falls behind, it restarts
from a complete snapshot of the newest required revision. A conversion failure
does not stop editing, but the document cannot report a completed save until
projection succeeds.

## External API

`parchmint-editor-iced` uses this crate through a small ParchMint-owned API.
Other application crates use
[`parchmint-editor-api`](parchmint-editor-api.md) instead:

```rust
pub struct EditorCoreSession {
    inner: SessionState,
}

impl EditorCoreSession {
    pub fn open(load: CanonicalDocumentLoad) -> Result<Self, EditorError>;
    pub fn attach_view(&mut self, view: ViewId) -> Result<(), EditorError>;
    pub fn execute(
        &mut self,
        origin: EditorCommandOrigin,
        command: EditorCommand,
    ) -> Result<AppliedEditorChange, EditorError>;
    pub fn selection(&self, view: ViewId) -> Result<EditorSelection, EditorError>;
    pub fn project(&self, through: EditorRevision)
        -> Result<AsyncResult<CanonicalProjection>, EditorError>;
}
```

The private `DocumentEngine` seam keeps the document engine replaceable without
giving it control of ParchMint data:

```rust
trait DocumentEngine: Send {
    fn load(&mut self, document: SemanticDocumentSnapshot)
        -> Result<(), EngineError>;
    fn apply(&mut self, edit: EngineEdit) -> Result<EngineChange, EngineError>;
    fn blocks(&self, range: BlockRange) -> Vec<SemanticBlockSnapshot>;
}

struct EditorSession {
    document: Box<dyn DocumentEngine>,
    ids: StableIdMap,
    comments: CommentStore,
    anchors: AnchorStore,
    undo: DocumentUndo,
    revision: EditorRevision,
    views: HashMap<ViewId, LogicalViewState>,
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
ParchMint applies its own forward and inverse transactions through the seam.

```rust
fn execute(session: &mut EditorSession, command: EditorCommand) -> Result<()> {
    let transaction = Transaction::from_command(command, &session.ids)?;
    let change = session.document.apply(transaction.engine_edit())?;

    session.apply_position_mapping(change.position_mapping());
    session.comments.apply(change.comment_effects());
    session.undo.push(transaction);
    session.revision = session.revision.next();
    session.projection.offer(session.projection_batch(&change));
    Ok(())
}
```

The projection worker runs away from the UI loop. Ordinary edits send the
changed semantic blocks and their revision instead of copying the whole
document. The queue has one bounded pending batch and can combine consecutive
changes. A save pins the exact revision it needs. If incremental conversion can
no longer catch up, the worker restarts from one complete snapshot of the
newest required revision. A projection failure does not stop editing, but the
document cannot report a completed save until projection succeeds.

The engine must preserve stable identity, anchor mapping, two-view undo,
deterministic projection, and large-document behavior through ParchMint-owned
types and rules.
