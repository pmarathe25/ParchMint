# Editor panes and tabs

- **EDIT-001:** Each editor pane must support multiple reorderable tabs.
- **EDIT-002:** A document may be open at most once per pane and once in each of the two panes simultaneously.
- **EDIT-003:** Opening an already-open document in a pane focuses its existing tab.
- **EDIT-004:** Closing the last companion tab closes the companion pane and expands the primary.
- **EDIT-005:** Closing a tab never deletes the document.
- **EDIT-006:** The same document in two panes shares body content, formatting, comments, undo history, save state, and word count.
- **EDIT-007:** Each view retains independent cursor, selection, scroll position, viewport, focus, and local-search state.
- **EDIT-008:** An edit made in one mounted view appears in the other mounted view by the next rendered frame under normal load.
- **EDIT-009:** Undo invoked from either view undoes the latest document operation in shared document history, regardless of origin.
- **EDIT-010:** All supported documents, including documents near 250,000 words, retain the same two-view and editing capabilities.
- **EDIT-011:** Project History opens from workspace navigation. The status bar does not contain a document-specific History action.
- **EDIT-012:** Every populated editor pane keeps its tab strip present and distinguishes active tab, focused pane, dirty tabs, and named close controls. Tabs shrink uniformly on overflow while preserving the first title character, ellipsis, and close control; full titles remain available in the tooltip.
