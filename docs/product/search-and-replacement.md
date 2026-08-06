# Search and replacement

- **SEARCH-001:** `Find` opens local search in the focused editor view. Matches are indicated directly in editor content and local Find is hidden when inactive.
- **SEARCH-002:** Each view has independent local-search state.
- **SEARCH-003:** Enter and Shift+Enter navigate results; Escape closes local search and restores focus. Replacement controls are initially collapsed.
- **SEARCH-004:** Local search supports case-sensitive and whole-word matching and distinguishes the active match.
- **SEARCH-005:** Local replacement participates in document undo.
- **SEARCH-006:** Global Search opens from the Explorer header, replaces Explorer in the left sidebar, and provides an explicit return. Replacement review uses the middle workspace pane.
- **SEARCH-007:** v1 Global Search always searches the entire project and shows no scope selector.
- **SEARCH-008:** Searchable fields are document body, display title, Synopsis, and user-defined metadata.
- **SEARCH-009:** Global search supports case-sensitive and whole-word modes. Regex is deferred.
- **SEARCH-010:** Results stream, are virtualized, grouped by document, identify the match, and navigate the focused editor view.
- **SEARCH-011:** Global replacement modifies editable document bodies only.
- **SEARCH-012:** Global replacement requires a central hierarchy-shaped preview with selection controls for groups, documents, and matches, including indeterminate parent states.
- **SEARCH-013:** Applying global replacement is one composite project operation, one logical project undo, and one history checkpoint.
- **SEARCH-014:** Results are revalidated against current document revisions before navigation or replacement.
