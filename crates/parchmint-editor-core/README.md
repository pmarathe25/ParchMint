# `parchmint-editor-core`

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

`prepare_projection` captures an immutable revision and its semantic document.
Its `canonical` method serializes that captured revision on demand. Each undo
and redo stack retains at most 256 operations and approximately 64 MiB of
payload, except that the latest operation remains undoable even if it alone
exceeds that size. Entries retain changed paragraphs and omit unchanged prefix
and suffix paragraphs. Formatting toggled at a collapsed caret applies to
subsequent typing without creating a document revision until text is entered.

## Interface

`EditorCoreSession` opens a document, manages logical views, applies commands,
and reports changed blocks through `AppliedEditorChange`.
`PreparedEditorProjection` holds an immutable revision for later serialization.

See [the source](src/lib.rs) for method signatures.

Application crates use [parchmint-editor-api](../parchmint-editor-api/README.md).
The private `DocumentEngine` interface in [document_engine.rs](src/document_engine.rs)
separates rich-text storage from session revisions, IDs, undo, and anchors.

## Implementation

The document engine stays behind `DocumentEngine`. Its types, identifiers,
transactions, undo records, storage, and serialization remain private.
ParchMint does not save an engine's format or use engine IDs as ParchMint IDs.
ParchMint applies its own forward and inverse position mappings.

Undo reconstructs the before-edit document from the retained paragraph changes
and reverses the stored position mapping. Redo applies the after-edit paragraphs
and the forward mapping.

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
