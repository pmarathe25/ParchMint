# Interaction Specification

Covers the interaction, navigation, and transition contracts documented on the
production screens and prototype flows in `parchmint-ui.penpot`. Every recorded
interaction maps to requirement IDs and screen/component IDs from the archive's
plugin metadata; flows are the 11 `PM / Prototype Flow / <flow> / index` boards
on Page 14 (Prototype Flows). All prototype navigations are page-local
`navigate` interactions on the same page; the 14-page surface is documentation,
not a cross-page executable launcher.

Recorded prototype navigations (21 total, 0 cross-page, 0 empty):

| Flow | Page | Trigger (screen) | Destination |
|---|---|---|---|
| create-and-write | 03 | launcher-recent | create-project-dialog |
| create-and-write | 03 | create-project-dialog | terminal create-and-write/editor-ready |
| editor/companion-focus | 04 | editor-dual-manuscript-research-left-focus | editor-dual-two-manuscript |
| editor/companion-focus | 04 | editor-dual-two-manuscript | editor-same-document-two-views |
| editor/local-find | 04 | editor-same-document-two-views | editor-single-default |
| editor/companion-focus | 04 | editor-dual-manuscript-research-right-focus | editor-same-document-two-views |
| organize-hierarchy | 05 | cards-manuscript-default | cards-deep-expanded |
| organize-hierarchy | 05 | cards-deep-expanded | terminal cards-and-organize/editor-return |
| organize-hierarchy | 05 | cards-drag-multiselect | terminal cards-and-organize/editor-return |
| global-search-replace | 06 | search-query-entry | search-streaming-results |
| global-search-replace | 06 | search-streaming-results | search-result-navigation |
| global-search-replace | 06 | search-result-navigation | global-replace-preview-exclusions |
| comment-thread | 07 | comments-unresolved-thread | comments-replies-expanded-composer |
| comment-thread | 07 | comments-replies-expanded-composer | comments-resolved-thread |
| delete-restore | 08 | recently-deleted | recently-deleted-fallback-location |
| history-restore | 08 | history-named-snapshot | history-restore-checkpoint |
| appearance-cycle | 09 | settings-appearance-system | settings-appearance-dark-override |
| appearance-cycle | 09 | settings-appearance-dark-override | settings-appearance-light-override |
| export-entire-manuscript | 10 | export-entire-manuscript | export-progress |
| export-entire-manuscript | 10 | export-progress | export-success |
| save-failure-retry | 10 | close-save-failure | terminal save-failure-retry/editor-saved |

## 1. Launcher & project creation (Page 03)

- **PRJ-001/PRJ-002 — Launch behavior.** ParchMint starts at the Launcher,
  never reopening a previous project. `launcher-first-launch` presents recent
  projects (empty) and Create New / Open Project actions; `launcher-recent`
  lists recent projects with name, directory path, and last-opened date/time.
  Activating a recent project row opens it (PRJ-010). `launcher-missing-project`
  renders a project whose stored path is unavailable with a clear action;
  `launcher-locked-project` covers the single-writable-session rule (PRJ-007).
- **PRJ-003 — Create Project dialog.** `create-project-dialog` collects title,
  destination, optional author, and default-language from the supported
  spellcheck-language list. Initial focus is the Project name field. Activating
  Create enters the terminus `PM / Prototype Terminal / create-and-write /
  editor-ready`: project created, `Untitled Document` open in the editor,
  authoring context focused, status Saved. Escape/cancel returns to the Launcher
  recent list without creating.
- **UI composition.** Project creation uses the standard dialog layer
  (`PM/Dialog`) with centered outer margins and the platform-native
  directory/language behaviors handled by platform adapters.

## 2. Editor workspace (Page 04)

- **WS-001/WS-002 — Layout.** Editor mode shows collapsible/resizable left
  sidebar (Explorer), central editor, and collapsible/resizable right Inspector.
  `editor-single-default` is the canonical single-pane state; focus context is
  `primary`.
- **Color-mapped progressions.** `editor-explorer-collapsed`,
  `editor-inspector-collapsed`, and `editor-both-sidebars-collapsed` render every
  sidebar collapse independently and together (WS-002, WS-005, WS-013).
- **Companion pane (WS-006, EDIT-001, EDIT-004).** `editor-dual-two-manuscript`
  shows the optional right companion; `editor-dual-manuscript-research-left-focus`
  and `...-right-focus` pin the Inspector/toolbar target to the focused view
  (WS-009, TOOL-003). Closing the last companion tab closes the pane and expands
  the primary (EDIT-004).
- **Same document, two views (EDIT-002, EDIT-006, EDIT-007, EDIT-008).**
  `editor-same-document-two-views`: one document open once per pane; content,
  comments, undo history, save state, and word count are shared; each view keeps
  independent cursor, selection, scroll, focus, and local-search state.
- **Local find/replace (SEARCH-001/003/005).** `editor-local-find` and
  `editor-local-replace` are view-local; the same document in two views keeps
  independent local-search state (EDIT-007).
- **Spellcheck (SPELL-001, SPELL-002, SPELL-003).** `editor-spellcheck-suggestions`
  surfaces suggestions inline; language and dictionaries are the project default
  — there is no per-document language control.
- **Explorer context action** `explorer-context-menu-actions` composes the
  applicable create/open/open-in-companion/rename/copy/cut/delete actions per
  node kind (TREE-002, TREE-010, TREE-013).
- **Comment creation** `editor-context-menu-add-comment` — available through the
  editor context menu and Comments panel only; there is no floating
  selection-end affordance (CMT-007, TOOL-005).
- **Editor focus/keyboard default.** Mode switch → toolbar → Explorer → active
  tab → focused editor → Inspector → status; F6 cycles major regions.
- **Reduced motion.** Expand/collapse and pane/gutter feedback use
  tokenized Motion durations (PM/Motion) honoring system reduced-motion
  preference via platform adapters.

## 3. Cards workspace (Page 05)

- **CARD-001/002/006 — Cards modes.** `cards-manuscript-default` is the
  canonical Manuscript view with synopsis density; `cards-research-selected`
  is the Research view with its selection; `cards-density-compact · Production`
  is the compact density. `cards-deep-expanded` and `cards-collapsed-groups`
  render disclosure depth and collapsed groups.
- **Direct manipulation (TREE-005, TREE-007, TREE-009).** `cards-drag-multiselect`
  covers drag reordering, moving between roots, and Shift/command additive
  multi-selection with an insertion marker. Batch operations normalize
  ancestor/descendant selections (TREE-008) and preserve relative order
  (TREE-009).
- **Return to editor.** The organize-hierarchy flow returns to
  `PM / Prototype Terminal / cards-and-organize / editor-return`, preserving the
  hierarchy and selection into the focused editor.

## 4. Global search & replace (Page 06)

- **SEARCH-006/007/009/010/011 — Global Search.** `search-query-entry` is the
  search from the Explorer header replacing Explorer in the sidebar (no ribbon
  destination and no scope selector). `search-streaming-results` shows results as
  they stream; `search-result-navigation` preserves focus through result
  navigation; `search-no-results` is the empty state; `search-stale-deleted-results`
  distinguishes results whose source was deleted. The search index may rebuild
  (`search-index-rebuilding`, SEARCH-010/PERF-012) and announces completion.
- **SEARCH-006/007 — Replace preview.** `global-replace-preview-exclusions`
  previews the global replacement as a workspace state (excluded sections,
  word-level changed lines). The preview is a workspace state, not a dialog; no
  Preview/Compare buttons or leading icons are added.

## 5. Comments & Inspector (Page 07)

- **CMT-001/002/003/004/010/011 — Comment threads.** `comments-unresolved-thread`
  is the unresolved collapsed thread; `comments-replies-collapsed` collapses
  replies; `comments-replies-expanded-composer` shows replies in chronological
  order with the composer; `comments-resolved-thread` shows a resolved filter.
  `comments-orphaned` and `lost-comment-anchor` cover orphaned comments whose
  anchor was lost — handled without losing user content (CMT-011/012).
- **Thread anchor relationship.** The comment thread is programmatically related
  to its anchor; filter and collapse states are announced (accessible).
- **Inspector sections (WS-004, WS-008, WS-014, META-*).** `inspector-group-no-comments`,
  `inspector-no-metadata-fields`, and `inspector-many-metadata-fields` render the
  Synopsis, Metadata, and Comments sections with consistent disclosure
  (WS-015). Proper metadata and Synopsis values are editable in place; comments
  remain document-context only.

## 6. History & Recently Deleted (Page 08)

- **HIST-001/007/010 — History.** `history-session-date-grouped`,
  `history-named-snapshot`, and `history-restore-checkpoint` cover date-grouped
  sessions, named snapshots, and checkpoint restore. Restore is complete-project
  scope (no document/group/subtree restore in v1); the confirmation names the
  checkpoint and the whole-project impact. History access is only through
  `HistoryStore`.
- **DEL-001/003/004/005 — Recently Deleted.** `recently-deleted` is the shared
  trash surface; `recently-deleted-fallback-location` confirms a restorable
  destination when the original parent is unavailable. The same trash icon is
  used for Recently Deleted and destructive row actions.

## 7. Project settings & appearance (Page 09)

- **APPR-001/002/003/004 — Appearance.** `settings-appearance-system`,
  `settings-appearance-dark-override`, and `settings-appearance-light-override`
  are the three choices exactly. System follows the OS at runtime; an explicit
  choice persists and overrides later OS changes. Changing appearance updates
  every open window without restart and never enters project undo/save/history.
- **SPELL-001/002/003.** `settings-dictionaries` manages the project-default
  language and dictionaries.
- **FMT-003/004/005/006/007.** `settings-general`, `settings-metadata-fields`,
  and the style settings screens (`settings-styles-inheritance`,
  `settings-replace-in-use-style`, `settings-delete-unused-style`) edit reserved
  and custom project styles with inheritance and in-use replacement handling.

## 8. Export & save states (Page 10)

- **EXP-001/008 — Export scope.** `export-entire-manuscript` fixes Entire Manual
  centroid as v1 scope: output path/name, title/page-break controls, numbering,
  and Export. `export-project-output-controls`,
  `export-progress`, `export-success`, `export-failure`, and `export-numbering`
  are the accompanying states.
- **SAVE-013/014 — Save failures.** `close-save-failure`
  and `save-failure-retry` retry succeeds into
  `PM / Prototype Terminal / save-failure-retry / editor-saved`; the recovery
  log remains disposable. `corrupt-canonical-file` (DATA-001) and
  `project-format-migration` (DATA-005) define the migration path.
- **A11y.** Dialog initial focus is the scope or error summary as appropriate;
  progress and completion are announced; native file handoff is platform-native
  and documented.

## 9. Appearance flow (Page 09 flow)

- System follows the operating system → explicit Dark updates every open
  window → explicit Light remains Light after operating-system changes.
  Appearance never enters project history (APPR-004) and never alters authored
  styles or export (APPR-007).

## 10. Recovery / error states (Page 11)

- `editor-document-loading`, `empty-manuscript-research-roots`,
  `history-unavailable`, `search-index-rebuilding`, `recovered-after-crash`,
  `lost-comment-anchor`, `corrupt-canonical-file`, `project-format-migration`,
  and `unsupported-pasted-image` define loading, empty, unavailable, and
  recovery surfaces. Recovered content is never the only copy after durable
  save (architecture data-safety rule).

## Keyboard equivalents

Primary keyboard contract is in `keyboard-focus-map.md`. Command-level shortcuts
(native menus) are platform-specific (PLAT-001/004) and documented in
`cross-platform-variants.md`; no shortcut is authored into the prototype.