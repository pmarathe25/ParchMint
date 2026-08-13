# Undo and redo

- **UNDO-001:** Document undo covers prose, formatting, content-title changes, and comment changes.
- **UNDO-002:** Document undo is shared across both views of the same document.
- **UNDO-003:** Project undo covers tree creation/deletion/move/order, display-title changes, Synopsis/metadata, metadata definitions, style definitions, project-dictionary changes, and global replacement.
- **UNDO-004:** Keyboard focus selects the undo domain. Editor/comment focus uses document undo; tree/Cards/settings/Inspector values use project undo; focused text inputs use text-input undo.
- **UNDO-005:** Interactive document/project undo may reset when the project closes. Durable older states remain available through History.
- **UNDO-006:** A whole-project History restore, completed format migration, or accepted recovery replay resets interactive document and project undo/redo before further editing.
- **UNDO-007:** Undo and redo create new authored states and are saved/checkpointed normally; they never rewrite existing History.
