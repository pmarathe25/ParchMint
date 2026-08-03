# ParchMint v1 Product Specification

**Status:** Current v1 product baseline
**Version:** 2.2
**Date:** 2026-08-02
**Primary audience:** Product owner, design agents, implementation agents, QA agents

## 1. Purpose

ParchMint is a local-first, cross-platform desktop application for planning and writing novels. This document defines the behavior against which the first release must be designed, implemented, and validated.

Requirements in this document define product behavior, state, information architecture, accessibility, data, and acceptance outcomes. Visual styling, spacing, iconography, and component composition belong in `docs/design/penpot-design-brief.md` unless a presentation constraint changes workflow, capability, accessibility, or another observable behavior.

Normative terms:

- **Must / must not:** required for v1.
- **Should / should not:** expected unless the product owner approves a direct update to this specification.
- **May:** optional behavior that does not change the required experience.
- **Deferred:** explicitly not part of v1.

Requirement IDs are stable and must be used in design and test traceability.

## 2. Product goals

ParchMint v1 must:

1. Let a solo novelist create and manage multiple independent projects.
2. Organize each project into strictly ordered, arbitrarily nested Manuscript and Research hierarchies.
3. Provide a responsive semantic WYSIWYG editor suitable for long-form prose.
4. Provide a Cards view over the same hierarchy and metadata for planning and reordering.
5. Keep canonical authored data in open, inspectable, Git-friendly files.
6. Autosave and retain every completed autosave checkpoint without interrupting typing.
7. Provide comments, metadata, search, word counts, recovery, spellcheck, and one initial export format.
8. Provide accessible Light and Dark appearances, including a System-following option.
9. Ship as one coherent first release for Windows, macOS, and Linux.
10. Keep the GUI/editor, history, search, spellcheck, persistence, and export implementations replaceable behind ParchMint-owned contracts.

## 3. Target user

The primary user is a **solo novelist** who may work on multiple projects but does not need real-time collaboration.

The product favors sustained writing, structural planning, predictable data ownership, and low latency over publishing-suite complexity.

## 4. Platform scope

### PLAT-001 — First-class platforms

Windows, macOS, and Linux are first-class v1 targets. A capability is not complete merely because it works on one platform.

### PLAT-002 — Native desktop application

ParchMint must install and run as a desktop application using native windows, menus, dialogs, clipboard integration, and the platform webview runtime selected by the current architecture.

### PLAT-003 — One product model

All three platforms must use the same project format, domain model, feature set, and compatibility rules. Platform conventions may differ without changing authored data.

### PLAT-004 — Mobile and web

Mobile and browser-only clients are out of scope.

### PLAT-005 — Supported versions

Exact minimum operating-system and Linux-distribution versions must be frozen before public beta based on native CI and runtime validation. They may not be chosen solely from framework documentation.

## 5. Product principles

1. **Local first:** core writing, saving, history, search, spellcheck, and export work offline.
2. **Open authored data:** current content remains readable without ParchMint caches or databases.
3. **No silent data loss:** save, deletion, restoration, and errors preserve recoverable states.
4. **Semantic formatting:** named styles represent document roles; direct decoration is limited.
5. **Same behavior at every supported document size:** internal optimizations may be transparent, but feature availability may not change because a document is large.
6. **Responsive by construction:** storage, Git, SQLite, spellcheck, export, and project-wide analysis never block the UI thread.
7. **Implementation details stay hidden:** filenames, Git, object IDs, SQLite, and editor-engine internals are not normal product concepts.
8. **Appearance is not authored content:** Light/Dark/System choices never alter document styles or export output.
9. **Design and implementation remain traceable:** every major screen and acceptance test maps to requirement IDs.

## 6. Domain model

### 6.1 Project

A project is a directory containing canonical files, an app-managed history repository, and disposable derived state.

Each project has:

- Stable project ID.
- Display title.
- Optional author.
- Default language.
- Fixed Manuscript and Research roots.
- Ordered hierarchy.
- Project styles.
- Metadata-field definitions and values.
- Comments and annotations.
- Project dictionary.
- Export settings.
- History, recovery, and workspace state.

Appearance and the global dictionary are application preferences rather than authored project state.

### 6.2 Fixed roots

- **Manuscript:** content included in normal manuscript export.
- **Research:** app-created supporting notes excluded from normal export.

The visible root labels are fixed in v1 and cannot be renamed, deleted, copied, reordered, or moved.

### 6.3 Node types

#### Group

A group may contain groups and documents. It has a title, Synopsis, metadata, ordering, and export settings, but no editable prose body.

#### Document

A document has a title, semantic rich-text body, Synopsis, metadata, comments, ordering, and export settings. It cannot contain children.

## 7. v1 scope summary

### Included

- Launcher and project creation.
- Multiple project windows in one application process where supported.
- Editor and Cards modes.
- Fixed Manuscript and Research trees.
- Arbitrary group nesting and strict order.
- Multi-selection, drag/drop, document copy/paste, and document cut/paste.
- One primary editor pane and one optional companion pane on the right.
- Multiple tabs in each pane.
- The same document open once in each pane with shared content/undo and independent view state.
- Semantic rich-text styles and inline marks.
- Comments with collapsible replies.
- Synopsis and project-defined text metadata.
- Local and global search; body-only global replacement preview.
- Autosave, explicit save, crash recovery, complete checkpoint history, named snapshots, and Recently Deleted.
- App-created Research notes.
- Selection, active-document, and Manuscript word counts.
- Cross-platform spellcheck using the project-default language plus project and global dictionaries.
- System, Light, and Dark application appearance.
- Self-contained HTML export of the entire Manuscript.
- Cross-platform packaging and accessibility validation.

### Explicitly deferred

- Manuscript or Research import.
- External-edit reconciliation while ParchMint is open.
- Research attachments and file previews.
- Recursive editor splits or top/bottom companion layout.
- Regex search.
- User-selectable Global Search scopes.
- Search over comments or a project-wide Comments view.
- Generated table of contents.
- In-app export preview.
- DOCX, EPUB, PDF, Markdown, LaTeX, or print-ready export.
- Partial manuscript export by group/document selection or per-node inclusion overrides.
- Track changes, footnotes/endnotes, tables, embedded images, advanced page layout.
- Project/style/metadata templates.
- Remote history backup.
- General-purpose Git UI.
- Per-document spellcheck language overrides.
- Group, Research, and whole-project aggregate word counts.
- Grammar checking or semantic writing suggestions.
- Real-time collaboration, mobile, web, and AI-assisted writing.
- Any user-visible large-document mode or feature-reduced document mode.

## 8. Functional requirements

### 8.1 Launcher and project creation

- **PRJ-001:** ParchMint must start at a launcher in v1 rather than automatically reopening previous projects.
- **PRJ-002:** The launcher must show recent projects and actions to create or open a project.
- **PRJ-003:** New Project must collect project title, destination, optional author, and default language from the supported spellcheck-language list.
- **PRJ-004:** The suggested directory name may derive from the project title, but later title changes must not rename or move the directory.
- **PRJ-005:** New projects must contain one Manuscript document named `Untitled Document`, open it immediately, and begin Research empty.
- **PRJ-006:** One project opens in one application window; multiple project windows may be open.
- **PRJ-007:** A project may have only one writable ParchMint project session at a time. A second open attempt should focus the existing window when possible or show a safe locked-project message.
- **PRJ-008:** ParchMint must refuse to create a project inside another Git working tree in v1.
- **PRJ-009:** Project directory paths appear only in project-management contexts, not throughout the writing UI.
- **PRJ-010:** Each recent-project entry must present the project name, project directory path, and last-opened date and time. Activating the project name opens the project.

### 8.2 Workspace shell

- **WS-001:** The top mode control must switch between `Editor` and `Cards` without changing underlying data.
- **WS-002:** Editor mode must show a collapsible/resizable left sidebar, central editor area, and collapsible/resizable right Inspector.
- **WS-003:** The left sidebar must provide Explorer and Global Search panels.
- **WS-004:** The Inspector must provide Synopsis, Metadata, and Comments sections as applicable.
- **WS-005:** The primary editor pane fills the central area when the companion is closed.
- **WS-006:** The optional companion pane opens on the right in v1.
- **WS-007:** Layout widths, split ratio, collapsed states, tabs, active view, scroll positions, and current mode must restore per project without entering authored history.
- **WS-008:** Clicking a tree node or Card sets the Inspector context to that node.
- **WS-009:** Focusing or clicking an editor view sets the Inspector context to that editor's document, even if the tree retains another selection.
- **WS-010:** Focus, selection, open-tab state, and active context must be distinguishable, keyboard accessible, and programmatically exposed. Color alone is insufficient.
- **WS-011:** The minimum supported application-window size is 1280 × 720 logical pixels. ParchMint must prevent resizing below that minimum rather than substituting a mobile or feature-reduced layout.
- **WS-012:** Every project workspace must provide persistent, mutually exclusive navigation to Editor, Cards, project History, Recently Deleted, Export, and Project Settings. Global Search is entered from the Explorer header and replaces Explorer in the left sidebar.
- **WS-013:** The bottom status bar must provide keyboard-accessible controls to show or hide Explorer and Inspector in addition to word count, save status, and a contextual document-History action. Pane controls expose pressed state. The History action targets the focused pane's active document and is unavailable when no document is active.
- **WS-014:** Applicable Synopsis and metadata values in Inspector must be editable in place; Comments remain available only for document context.
- **WS-015:** Inspector sections, Explorer roots, grouped Global Search results, and comparable Cards groups must expose consistent expand/collapse behavior and state.

### 8.3 Appearance

- **APPR-001:** Application settings must provide exactly three appearance choices in v1: `System`, `Light`, and `Dark`.
- **APPR-002:** `System` is the default and follows the current operating-system appearance while ParchMint is running.
- **APPR-003:** An explicit Light or Dark choice persists as an application preference and overrides later operating-system changes until changed by the user.
- **APPR-004:** Changing appearance updates every open ParchMint window without restarting and without entering project undo, project save, or project history.
- **APPR-005:** Dark appearance uses fully dark application, sidebar, Inspector, toolbar, editor-chrome, and manuscript-canvas surfaces. It must not leave the prose canvas as a light sheet.
- **APPR-006:** Light and Dark use the same semantic component and layout contracts. Production components must consume semantic tokens rather than hard-coded theme-dependent colors.
- **APPR-007:** Authored project styles, canonical HTML/CSS, and export output must not change when application appearance changes.
- **APPR-008:** Focus, selection, disabled, warning, error, comment, search-match, and save states must remain distinguishable and accessible in both appearances without relying on color alone.
- **APPR-009:** v1 provides the Appearance setting only; a toolbar or status-bar quick toggle is deferred.

### 8.4 Explorer and hierarchy

- **TREE-001:** Explorer must show Manuscript and Research as independent collapsible sections containing their ordered hierarchies.
- **TREE-002:** Users must create, rename, and delete groups and documents under either root.
- **TREE-003:** Groups may contain groups and documents; documents may not have children.
- **TREE-004:** All sibling ordering must be explicit and deterministic.
- **TREE-005:** Drag/drop must reorder siblings, move nodes into groups, and move nodes between Manuscript and Research.
- **TREE-006:** Moving a group must move its complete subtree; cycles must be rejected.
- **TREE-007:** Users must be able to select a contiguous range with Shift and noncontiguous nodes with the platform additive-selection modifier.
- **TREE-008:** Batch operations must normalize ancestor/descendant selections so a selected ancestor subsumes selected descendants.
- **TREE-009:** Batch move, delete, and applicable metadata operations must preserve relative ordering.
- **TREE-010:** `Copy`/`Paste` in Explorer or Cards must duplicate selected documents with fresh IDs and filenames.
- **TREE-011:** Document copies include body, title behavior, styles, Synopsis, metadata, and export settings, but not comments or history identity.
- **TREE-012:** When an original display title matches its first document-title block, the copy suffix is applied to both so synchronization remains active. If they differ, only the display title receives the suffix.
- **TREE-013:** `Cut`/`Paste` moves selected documents. Cut items remain until paste succeeds, appear visually pending, and can be cancelled with Escape.
- **TREE-014:** Group copy and keyboard cut are deferred; groups remain movable through drag/drop.
- **TREE-015:** Cross-project copy is deferred.
- **TREE-016:** Single-click selects; double-click or Enter opens a document. Groups do not open as prose editors.
- **TREE-017:** Manuscript documents open in the primary pane by default; Research documents open in the companion by default.
- **TREE-018:** Dragging a document onto a pane or tab strip opens it in that pane.
- **TREE-019:** Explorer must reveal and distinguish the active document in each editor pane. The focused pane's document is exposed as the primary active context.
- **TREE-020:** Explorer's context menu provides applicable create, open, open-in-companion, rename, copy, cut, and delete actions.

### 8.5 Editor panes and tabs

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
- **EDIT-011:** The contextual document-History action opens project History filtered to changes affecting the focused pane's active document.
- **EDIT-012:** Every populated editor pane keeps its tab strip present and distinguishes active tab, focused pane, dirty tabs, and named close controls. Tabs shrink uniformly on overflow while preserving the first title character, ellipsis, and close control; full titles remain accessible.

### 8.6 Rich text and semantic styles

- **FMT-001:** The editor is WYSIWYG and does not expose raw HTML source editing.
- **FMT-002:** Every text block has one semantic paragraph style referenced through a stable ID.
- **FMT-003:** Reserved styles include Body, Document Title, Heading 1, Heading 2, Heading 3, Block Quote, and Verse.
- **FMT-004:** Users may edit reserved style properties and display names but may not delete reserved styles.
- **FMT-005:** Users may create, rename, inherit, and edit custom project styles.
- **FMT-006:** Style properties include font family/size, weight, italics, alignment, first-line/left/right indentation, line spacing, space before/after, keep-with-next, and page-break-before where meaningful.
- **FMT-007:** Changing a style immediately updates every mounted occurrence without rewriting each document solely because visual properties changed.
- **FMT-008:** Initial inline marks include bold, italic, underline, strikethrough, small caps, superscript, subscript, and links.
- **FMT-009:** Arbitrary per-selection font, size, color, and spacing overrides are deferred.
- **FMT-010:** Scene Break and Page Break are atomic structural nodes, not marker text or paragraph styles.
- **FMT-011:** Scene-break presentation is excluded from word count and search.
- **FMT-012:** Page breaks map to export pagination behavior while remaining atomic editor nodes.
- **FMT-013:** Normal paste retains only supported structure/marks, sanitizes unsafe HTML, and removes unsupported visual styling.
- **FMT-014:** Paste Without Formatting preserves paragraph boundaries but inserts plain text.
- **FMT-015:** Pasted images are rejected or omitted with a clear notification.
- **FMT-016:** Enter creates a new paragraph; after title or heading it defaults to Body; Shift+Enter inserts a line break.
- **FMT-017:** Tab indents list items inside lists. Outside lists, a literal Tab is inserted and preserved.
- **FMT-018:** Empty list/quotation behavior, backspace merging, and atomic-node cursor behavior are documented and tested.
- **FMT-019:** Every deletable custom style supports deletion from its own list entry. Reserved styles do not.

### 8.7 Shared formatting toolbar

- **TOOL-001:** Editor mode has exactly one formatting toolbar, even with two panes.
- **TOOL-002:** The toolbar remains visible whenever Editor mode is active and has no collapsed state in v1.
- **TOOL-003:** Toolbar commands target the focused editor view.
- **TOOL-004:** Toolbar interaction does not lose active editor context for command and undo routing.
- **TOOL-005:** The toolbar exposes style selection, common inline marks, lists/quote, links, Scene Break, and Page Break. Comment creation remains in the editor context menu and Comments panel.

### 8.8 Titles

- **TITLE-001:** Each document has an independent display title and may contain a first block with the reserved Document Title role.
- **TITLE-002:** Renaming the tree/Card display title never edits document body content.
- **TITLE-003:** When the first Document Title block changes, update the display title only if it matched the block's previous value.
- **TITLE-004:** Once display and content titles diverge, later content-title changes do not overwrite the display title.
- **TITLE-005:** If the display title later equals the current content title, synchronization resumes for subsequent content edits.
- **TITLE-006:** Removing or emptying the title block does not blank the display title.
- **TITLE-007:** Only the first reserved title block participates in synchronization.
- **TITLE-008:** Export does not duplicate an existing exported title block.

### 8.9 Synopsis and metadata

- **META-001:** Every group and document has a built-in multiline plain-text Synopsis.
- **META-002:** Synopsis is editable in Inspector and Cards, globally searchable, and excluded from manuscript export.
- **META-003:** Users define arbitrary single-line or multiline plain-text metadata fields in Project Settings.
- **META-004:** Field definitions have stable ID, label, optional description, applicability, optional default, Card visibility, and display order. Direct list reordering changes display order.
- **META-005:** Cards may display only predefined fields; definitions cannot be created or edited from Cards.
- **META-006:** A default value is copied into newly created applicable nodes and is not a live fallback.
- **META-007:** Changing applicability hides existing values without deleting them.
- **META-008:** Deleting a field requires confirmation and removes current values; History can restore them.
- **META-009:** Renaming a field preserves values because stable ID defines identity.
- **META-010:** Metadata-field templates are deferred.
- **META-011:** The metadata-field list supports direct reordering and per-field deletion.

### 8.10 Comments and annotations

- **CMT-001:** Documents support range, cursor-position, and document-level comments.
- **CMT-002:** A comment is a thread with a root message and collapsible chronological replies. Every thread visibly distinguishes unresolved and resolved state without relying on color alone.
- **CMT-003:** Each thread provides an in-thread reply composer. Users can add replies, edit/delete messages, resolve, and reopen. Threads appear in one list without separate resolved/unresolved sections.
- **CMT-004:** Comment bodies are plain text with paragraph breaks.
- **CMT-005:** Comments appear in the active document's Inspector and at editor anchors. A document Inspector always includes Comments or an explicit empty state.
- **CMT-006:** Selecting a comment scrolls to and highlights its anchor in the last-focused view of that document.
- **CMT-007:** The editor context menu provides Add Comment for the current selection or cursor. Selecting text does not add a floating affordance.
- **CMT-008:** With no selection, Add Comment creates a position comment; the Comments panel can create a document-level comment.
- **CMT-009:** Comments are stored in JSON sidecars outside canonical HTML prose.
- **CMT-010:** Text anchors include stable block ID, range, quotation, and context sufficient for conservative reattachment.
- **CMT-011:** Editor changes map anchors; ambiguous recovery or transformation orphans a comment rather than attaching it incorrectly.
- **CMT-012:** Orphaned comments remain visible and can be reattached or converted to document-level.
- **CMT-013:** Comments are not copied when a document is duplicated.
- **CMT-014:** Comments are excluded from export and v1 global search.

### 8.11 Cards

- **CARD-001:** Cards is an alternate projection of the same hierarchy, titles, Synopsis, metadata, ordering, and selection model used by Editor mode.
- **CARD-002:** v1 uses one vertically ordered, virtualized hierarchy rather than a multi-column corkboard.
- **CARD-003:** Cards preserve hierarchy, allow groups to expand/collapse, and expose the current drag destination.
- **CARD-004:** Dropping onto a group moves the node into that group.
- **CARD-005:** Cards supports Manuscript and Research, defaulting to Manuscript.
- **CARD-006:** Group and document Cards display title, Synopsis, and applicable configured metadata. Title and Synopsis may be edited where designed; metadata values are read-only and edited through Inspector.
- **CARD-007:** Explorer and Inspector remain available around Cards.
- **CARD-008:** Activating a document switches to Editor and opens it. Group disclosure or Card body expands/collapses the group without narrowing to a subtree.
- **CARD-009:** Cards and Explorer share multi-selection and applicable move/copy/cut behavior.
- **CARD-010:** A Status value appears only when the project defines and exposes that field.

### 8.12 Search and replacement

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

### 8.13 Undo and redo

- **UNDO-001:** Document undo covers prose, formatting, content-title changes, and comment changes.
- **UNDO-002:** Document undo is shared across both views of the same document.
- **UNDO-003:** Project undo covers tree creation/deletion/move/order, display-title changes, Synopsis/metadata, metadata definitions, style definitions, project-dictionary changes, and global replacement.
- **UNDO-004:** Keyboard focus selects the undo domain. Editor/comment focus uses document undo; tree/Cards/settings/Inspector values use project undo; focused text inputs use text-input undo.
- **UNDO-005:** Interactive document/project undo may reset when the project closes. Durable older states remain available through History.
- **UNDO-006:** A whole-project History restore, completed format migration, or accepted recovery replay resets interactive document and project undo/redo before further editing.
- **UNDO-007:** Undo and redo create new authored states and are saved/checkpointed normally; they never rewrite existing History.

### 8.14 Save, recovery, and closing

- **SAVE-001:** Autosave never blocks the UI thread.
- **SAVE-002:** Request autosave 1.5 seconds after editing becomes idle and at least every 30 seconds during continuous editing.
- **SAVE-003:** Structural changes request immediate asynchronous save/checkpoint.
- **SAVE-004:** Closing a tab, switching projects, or closing a window requests a high-priority save.
- **SAVE-005:** `Save` queues a high-priority save through the current revision but remains nonblocking.
- **SAVE-006:** Status distinguishes dirty, saving, saved-through-revision, and error states.
- **SAVE-007:** A save captures a consistent revision; later edits remain dirty for another save.
- **SAVE-008:** Only dirty canonical resources are serialized and written.
- **SAVE-009:** Canonical writes use crash-safe temporary-write/flush/atomic-replace behavior appropriate to each platform.
- **SAVE-010:** A completed history checkpoint corresponds to successfully written canonical state.
- **SAVE-011:** A high-frequency recovery journal protects changes after the latest completed autosave.
- **SAVE-012:** Recovery data is implementation-specific, versioned, and never the sole copy of completed authored state.
- **SAVE-013:** On save failure, editing remains available, the error persists visibly, recovery remains intact, and the application does not claim Saved.
- **SAVE-014:** A normal close waits asynchronously for final save. Failure keeps the project open with Retry and Cancel Close.

### 8.15 History and snapshots

- **HIST-001:** Every completed autosave checkpoint is retained indefinitely; v1 performs no automatic pruning.
- **HIST-002:** Git is hidden from ordinary UI.
- **HIST-003:** The project root uses one app-managed linear `main` history.
- **HIST-004:** Checkpoints include project manifest, documents, styles, metadata, Synopsis, project dictionary, annotations, and deletion tombstones, but exclude caches, indexes, recovery files, appearance, global dictionary, and workspace layout.
- **HIST-005:** History distinguishes autosave, explicit save, structural, named snapshot, and restoration events.
- **HIST-006:** Users may create named snapshots after pending changes are flushed, including a marker when no content changed.
- **HIST-007:** History supports chronological virtualized browsing, active-document filtering, side-by-side checkpoint-versus-current comparison, and restoration of the entire project. Partial checkpoint restoration is deferred.
- **HIST-008:** Restoration creates a new checkpoint and never rewinds or rewrites existing history.
- **HIST-009:** Current canonical files remain readable if history is missing or damaged; the user may reinitialize history from current state.
- **HIST-010:** History maintenance runs on background workers and does not compete perceptibly with active editing.
- **HIST-011:** Remote push/backup is deferred.

### 8.16 Deletion and Recently Deleted

- **DEL-001:** v1 has no Trash node in the live hierarchy.
- **DEL-002:** Delete removes content from current project state but preserves it in History.
- **DEL-003:** A deletion tombstone records stable node ID, title, type, section, former parent/order, deletion time, and restoring checkpoint.
- **DEL-004:** Recently Deleted lists deleted documents/groups, shows one formatted read-only preview, presents restore location, and can restore complete subtrees.
- **DEL-005:** Restore returns to the old location where possible or the relevant section root when the former parent is gone.
- **DEL-006:** Session project undo immediately reverses deletion while its entry remains available.
- **DEL-007:** v1 provides no purge, Empty Trash, or permanent history-erasure command.

### 8.17 Research

- **RES-001:** v1 Research contains only groups and app-created managed rich-text notes.
- **RES-002:** Managed Research notes use the same editor and canonical format as Manuscript documents.
- **RES-003:** Research is excluded from normal manuscript export.
- **RES-004:** Import, attachments, linked resources, PDFs, images, arbitrary files, and external previews are deferred.

### 8.18 Word counts

- **WORD-001:** Show selection count when text is selected and active-document count otherwise.
- **WORD-002:** Provide a Manuscript total in the normal workspace without requiring every document body to be loaded.
- **WORD-003:** Count exportable titles, headings, and prose; exclude comments, Synopsis, metadata, scene breaks, and page breaks.
- **WORD-004:** Common contractions and ordinary hyphenated compounds count as one; numbers count as words.
- **WORD-005:** Open-document counts update incrementally. The Manuscript total may use disposable derived records rebuilt from canonical documents.

### 8.19 Spellcheck

- **SPELL-001:** Spellcheck is required in v1 on Windows, macOS, and Linux and must provide correct, performant behavior in every supported platform webview.
- **SPELL-002:** Spellcheck uses the project-default language and supports a global dictionary, project dictionary, token-level suggestions, and viewport/recent-change-bounded checking.
- **SPELL-003:** The exact v1 supported-language inventory must be frozen before broad spellcheck UI implementation and must be identical across supported platforms for a given release.
- **SPELL-004:** Spellcheck failure must not block typing or saving. Errors remain visible and recoverable; release is blocked until cross-platform correctness and latency gates pass or this specification is updated by the product owner.
- **SPELL-005:** A misspelled word is decorated in place, and its spelling context menu is anchored to that word with ranked spelling suggestions and applicable project/global dictionary actions.
- **SPELL-006:** Native webview spellcheck may be disabled when the selected ParchMint spellcheck implementation would otherwise produce duplicate or inconsistent decorations or menus.

### 8.20 Export

- **EXP-001:** v1 exports one self-contained HTML5 manuscript.
- **EXP-002:** v1 always exports the entire Manuscript. Partial scope is deferred.
- **EXP-003:** Project defaults, group overrides, and document overrides use Inherit/Enabled/Disabled for title emission and page-break behavior. Per-node inclusion overrides are deferred.
- **EXP-004:** Numbering is an export-run option rather than arbitrary persistent per-node numbering.
- **EXP-005:** Group titles may emit headings despite groups having no body.
- **EXP-006:** The exporter does not duplicate existing document title content.
- **EXP-007:** Research, comments, Synopsis, and metadata are excluded unless a future feature says otherwise.
- **EXP-008:** The export interface identifies Entire Manuscript as fixed v1 scope and contains output path/name, title/page-break controls, numbering, and Export.
- **EXP-009:** After export, the user may open the result or reveal it in the file manager.
- **EXP-010:** Generated TOC and in-app preview are deferred.

## 9. Canonical project requirements

- **DATA-001:** Current authored data uses restricted deterministic HTML5, TOML, CSS, JSON, and UTF-8 text sidecars.
- **DATA-002:** Groups map to directories and documents to HTML files, while the manifest is authoritative for identity, ordering, titles, metadata, and semantics.
- **DATA-003:** Internal filenames are implementation details and never normal UI labels.
- **DATA-004:** Renaming a displayed title does not rename the backing file.
- **DATA-005:** Serialization uses UTF-8, LF, deterministic attribute ordering, stable whitespace/escaping, stable block IDs, and no rewriting of unchanged documents.
- **DATA-006:** Deleting caches and indexes does not break current project functionality.
- **DATA-007:** Deleting history removes old versions but does not damage current authored content.
- **DATA-008:** Canonical paths are relative, normalized, traversal-safe, case-conflict checked, and portable across Windows/macOS/Linux.
- **DATA-009:** ProseMirror JSON is transient editor state and never canonical project format.
- **DATA-010:** SQLite is derived state only and never the sole project store.
- **DATA-011:** The project dictionary is stored in a deterministic, inspectable canonical text representation. The global dictionary is stored in application preferences outside the project.

## 10. Scale and performance

### Required scale

- **PERF-001:** Projects up to 10–20 million words.
- **PERF-002:** Approximately 300–500 Manuscript documents and 25–50 Research documents.
- **PERF-003:** Individual documents up to approximately 250,000 words.
- **PERF-004:** All documents within the supported range receive the same features and interaction model.

### Interactive budgets

- **PERF-005:** Key-to-paint target: p95 ≤16 ms and p99 ≤33 ms under normal load.
- **PERF-006:** No save/history/search/spellcheck/export operation blocks the UI thread for more than 2 ms in one event-loop turn.
- **PERF-007:** Warm first editable viewport target is ≤250 ms for ordinary documents.
- **PERF-008:** At 250,000 words, the release gate is ≤1 second to first editable viewport on agreed reference hardware, with no feature reduction.
- **PERF-009:** Warm indexed global search begins returning results within 200 ms.
- **PERF-010:** Tree/Card movement visibly updates within 100 ms.
- **PERF-011:** Project open does not load every document body.
- **PERF-012:** Search rebuild, history maintenance, export, save, word-count rebuild, and spellcheck run in bounded background work and can be paused/cancelled where appropriate.
- **PERF-013:** Memory stabilizes under repeated open/edit/undo/search/close cycles; closing a document/view reclaims material editor resources.
- **PERF-014:** Transparent optimizations may be used only when selection, accessibility, search, comments, clipboard, and undo semantics remain unchanged.
- **PERF-015:** Editor projection/recovery work must use bounded coalescing and must not accumulate an unbounded backlog during continuous typing.

## 11. Accessibility and international text

- **A11Y-001:** All primary workflows are keyboard accessible.
- **A11Y-002:** Focus is visible and programmatically exposed.
- **A11Y-003:** Screen readers expose windows, mode controls, panes, tabs, tree hierarchy, Cards, headings, paragraphs, lists, comments, toolbar state, save/error state, spellcheck state, appearance choices, and dialogs.
- **A11Y-004:** Color is not the only status/selection/error indicator.
- **A11Y-005:** UI works at 100%, intermediate scaling, and 200% without caret, selection, hit-test, or layout drift.
- **A11Y-006:** Editor input supports grapheme movement, combining marks, emoji, CJK IME composition/candidates, Arabic, bidirectional text, and literal tabs.
- **A11Y-007:** Reduced-motion preferences suppress nonessential animation while preserving state clarity.
- **A11Y-008:** Native interactive accessibility is tested with VoiceOver, Narrator or NVDA, and Orca before release.
- **A11Y-009:** Light and Dark appearances meet applicable text, icon, focus, selection, and state contrast requirements.

## 12. Privacy and security

- **SEC-001:** Core functionality requires no network connection.
- **SEC-002:** The desktop shell loads only bundled local application content in v1 and blocks in-app navigation to remote origins.
- **SEC-003:** Use a strict CSP and least-privilege desktop-shell capabilities bound to the originating application window and project session.
- **SEC-004:** No remote content receives privileged Tauri access.
- **SEC-005:** Project paths and pasted HTML are validated/sanitized.
- **SEC-006:** History and search network features are disabled in v1.
- **SEC-007:** Dependency locks, advisories, license inventory, provenance checks, and SBOM are release artifacts.
- **SEC-008:** Spellcheck dictionaries and language data are bundled or otherwise available offline under compatible licenses; user prose is not sent to a network service.

## 13. Canonical user flows

1. **Create and write:** launch, create project, initial Untitled Document opens, type, autosave, close, reopen.
2. **Organize:** create nested groups/documents, multi-select, drag, duplicate, cut/paste move, cross-section move.
3. **Compare:** open Manuscript in primary and Research or another Manuscript document in companion; focus changes Inspector and toolbar target.
4. **Same document twice:** open one document in both panes, use independent scroll/selection, edit and undo from either view.
5. **Plan in Cards:** edit Synopsis, inspect metadata, expand/collapse, reorder, open a document in Editor.
6. **Comment:** select text, use editor context menu, reply, resolve, navigate, reopen.
7. **Search/replace:** local search in one view; whole-project global search; preview and apply body replacement as one project operation.
8. **Recover:** force termination after unsaved input, replay recovery, verify canonical and history consistency.
9. **Delete/restore:** delete subtree, undo during the session, or restore through Recently Deleted/whole-project History.
10. **Spellcheck:** see an in-place misspelling, select a suggestion, and add/remove words from project/global dictionaries.
11. **Appearance:** switch System/Light/Dark and verify every open window updates without changing project files or export.
12. **Export:** configure title/page-break behavior and generate one self-contained HTML file for the entire Manuscript.
13. **Move platform:** close cleanly, copy project between operating systems, reopen with identical hierarchy/content/history.

## 14. v1 release gates

ParchMint v1 is complete only when:

1. Every must-level requirement is implemented or the current specification is explicitly updated by the product owner.
2. The approved Penpot handoff is reconciled with no unexplained major visual/interaction deviations in Light or Dark.
3. Canonical format golden tests and cross-platform round trips pass.
4. Save, recovery, project undo, history, corruption isolation, deletion, restoration, and composite global-replacement fault tests pass.
5. History, search, spellcheck, editor, persistence, export, and platform adapter contract tests pass.
6. Normal and 250,000-word document fixtures retain the same feature set in one and two views.
7. Performance budgets pass on agreed reference hardware or the specification is updated.
8. Native IME, clipboard, high-DPI, spellcheck, and accessibility validation passes on Windows, macOS, and Linux.
9. System/Light/Dark switching, persistence, contrast, and open-window propagation pass.
10. Installers/packages launch and operate on the supported platform matrix.
11. No required workflow depends on a proprietary project database, installed Git executable, network service, or raw source editing.
