# `parchmint-editor-core`

**Purpose:** Own the editable state shared by all views of one document and
produce deterministic snapshots for saves. This crate is independent of Iced.

## Edit flow

```text
editor command -> validate IDs and positions -> apply one engine transaction
  -> map anchors and view selections -> update undo and revision
  -> report changed blocks -> queue a project-file snapshot
```

The core owns stable block and comment IDs, styles, anchors, revisions, position
mappings, and undo. Its private `DocumentEngine` stores and edits rich text.
Each logical view records a selection; mounted adapters own pixel scroll,
viewport, focus, layout, and search and spellcheck decorations.

## Interface

`EditorCoreSession` opens documents, manages logical views, applies commands, and
reports `AppliedEditorChange`. `prepare_projection` captures an immutable revision
and semantic document in `PreparedEditorProjection`; `canonical` serializes that
revision on demand. See [lib.rs](src/lib.rs) for signatures.

Application callers use [editor-api](../parchmint-editor-api/README.md).
[document_engine.rs](src/document_engine.rs) keeps engine types, IDs,
transactions, undo records, and storage private. Saved files and position
mappings use ParchMint's own representation.

## Undo and projection limits

Each undo and redo stack retains at most 256 operations and approximately 64 MiB
of payload, except that the newest operation remains undoable even if it exceeds
the size budget. Entries retain changed paragraphs and omit unchanged prefixes
and suffixes. Undo restores prior paragraphs and reverses the position mapping;
redo applies new paragraphs and the forward mapping.

Formatting toggles at a collapsed caret affect subsequent typing. They create a
document revision when text is entered.

Projection runs away from the UI loop. The pending queue has capacity two and
coalesces consecutive incremental offers. Overflow makes the next drain return
one `FullSnapshot` of the newest revision. Saves pin the exact revision they need;
application persistence acknowledges it only after delivery. Engine changes must
preserve stable IDs, anchor mapping, shared undo, deterministic output, and
large-document behavior.
