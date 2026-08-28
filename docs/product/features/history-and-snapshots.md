# History and snapshots

- **HIST-001:** Every completed save that changes canonical project content is retained indefinitely; v1 performs no automatic pruning. Repeated autosave, explicit, or structural saves of an unchanged project reuse the current checkpoint instead of creating an empty version. Typing into Inspector title, Synopsis, or metadata fields is coalesced after a short idle pause so one editing burst creates one meaningful checkpoint rather than one checkpoint per keystroke.
- **HIST-002:** Git is hidden from ordinary UI.
- **HIST-003:** The project root uses one app-managed linear `main` history.
- **HIST-004:** Checkpoints include project manifest, documents, styles, metadata, Synopsis, project dictionary, annotations, and deletion tombstones, but exclude caches, indexes, recovery files, appearance, global dictionary, and workspace layout.
- **HIST-005:** History distinguishes autosave, explicit save, structural, named snapshot, and restoration events.
- **HIST-006:** Users may create named snapshots after pending changes are flushed, including a marker when no content changed.
- **HIST-007:** History presents a virtualized writing timeline with visible named-milestone creation and active-document filtering, side-by-side checkpoint-versus-current comparison, and restoration of the entire project. Overflow comparisons use a bounded diff so a large manuscript never blocks the UI. Partial checkpoint restoration is deferred.
- **HIST-008:** Restoration creates a new checkpoint and never rewinds or rewrites existing history.
- **HIST-009:** Current canonical files remain readable if history is missing or damaged; the user may reinitialize history from current state.
- **HIST-010:** History maintenance runs on background workers and does not compete perceptibly with active editing.
- **HIST-011:** Remote push/backup is deferred.
