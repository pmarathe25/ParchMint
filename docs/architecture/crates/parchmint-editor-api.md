# `parchmint-editor-api`

## What it does

`parchmint-editor-api` defines the interface between ParchMint and its
rich-text editor. The application uses it to open documents, attach views, run
editor commands, observe changes, and create project-file snapshots.

One `SharedEditorSession` represents one open document. The session shares text,
comments, anchors, revision history, and undo across panes. Each attached view
keeps its own cursor, selection, scroll position, viewport, focus, and local
search state.

## How it works

```text
SharedEditorSession: document, comments, anchors, revision, undo/redo
├── primary view
│   ├── core: cursor, selection, local search
│   └── mounted widget: scroll, viewport, focus, layout
└── companion view
    ├── core: cursor, selection, local search
    └── mounted widget: scroll, viewport, focus, layout
```

A command includes the document revision it observed. The session applies a
valid command once, maps positions in attached views and anchors, and advances
the document revision. Undo from either pane acts on this one shared history.

A `CanonicalProjection` is a deterministic snapshot of one revision in the
format ParchMint saves. It includes the document body, comments, and derived
word count. The session remains editable while a projection is built.

## Public API

```rust
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
```

`ViewHostCapability` identifies one mounted editor view. Code outside the editor
cannot inspect the GUI handle behind it. The API exposes no editor-engine
documents, transactions, render trees, engine-native selections, or storage.

`SelectionGeometry` positions comment and spelling menus. Search and spellcheck
decorations belong to one view and can be rebuilt. `close` releases the views
and session after outstanding projection and recovery work settles.
Projection requests outside the retained revision budget fail explicitly, so a
save cannot acknowledge a different revision or crash the persistence worker.

## Implementation boundary

`parchmint-editor-core` owns the concrete session, transaction, view-state, and
projection-queue logic. This crate documents only the contract semantics above;
editor-engine types and scheduling remain behind the core and Iced adapters.
