# ParchMint v1 Product Specification

**Status:** Product-owner revised v1 product baseline
**Version:** 1.8
**Date:** 2026-07-30
**Primary audience:** Product owner, design agents, implementation agents, QA agents

## 1. Purpose

ParchMint is a local-first, cross-platform desktop application for planning and writing novels. This document defines the behavior against which the first release must be designed, implemented, and validated.

Requirements in this document define product behavior, state, information architecture, accessibility, data, and acceptance outcomes. Visual styling, spacing, iconography, and component composition belong in `03-penpot-design-brief.md` unless a presentation constraint directly changes workflow, capability, accessibility, or another observable behavior.

Normative terms:

- **Must / must not:** required for v1.
- **Should / should not:** expected unless an ADR and product-owner-approved specification change says otherwise.
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
7. Provide comments, metadata, search, word counts, recovery, and one initial export format.
8. Ship as one coherent first release for Windows, macOS, and Linux.
9. Keep the architecture replaceable so the GUI, editor, history backend, search backend, and persistence implementation can evolve independently.

## 3. Target user

The primary user is a **solo novelist** who may work on multiple projects but does not need real-time collaboration.

The product should favor sustained writing, structural planning, predictable data ownership, and low latency over publishing-suite complexity.

## 4. Platform scope

### PLAT-001 — First-class platforms

Windows, macOS, and Linux are first-class v1 targets. A capability is not complete merely because it works on one platform.

### PLAT-002 — Native desktop application

ParchMint must install and run as a desktop application using native windows, menus, dialogs, clipboard integration, and platform webviews through Tauri.

### PLAT-003 — One product model

All three platforms must use the same project format, domain model, feature set, and compatibility rules. Platform-specific conventions may differ without changing authored data.

### PLAT-004 — Mobile and web

Mobile and browser-only clients are out of scope.

### PLAT-005 — Supported versions

Exact minimum operating-system and Linux-distribution versions must be frozen before public beta based on native CI and runtime validation. They may not be chosen solely from framework documentation.

## 5. Product principles

1. **Local first:** core writing, saving, history, search, and export work offline.
2. **Open authored data:** current content remains readable without ParchMint caches or databases.
3. **No silent data loss:** save, deletion, restoration, and errors preserve recoverable states.
4. **Semantic formatting:** named styles represent document roles; direct decoration is limited.
5. **Same behavior at every supported document size:** internal optimizations may be transparent, but feature availability may not change because a document is large.
6. **Responsive by construction:** storage, Git, SQLite, export, and project-wide analysis never block the UI thread.
7. **Implementation details stay hidden:** filenames, Git, object IDs, SQLite, and editor-engine internals are not normal product concepts.
8. **Design and implementation remain traceable:** every major screen and acceptance test maps to requirement IDs.

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
- Export settings.
- History, recovery, and workspace state.

### 6.2 Fixed roots

- **Manuscript:** content included in the normal manuscript export.
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
- Word counts.
- Cross-platform spellcheck with project and global dictionaries.
- Self-contained HTML export.
- Cross-platform packaging and accessibility validation.

### Explicitly deferred

- Manuscript or Research import.
- External-edit reconciliation while ParchMint is open.
- Research attachments and file previews.
- Recursive editor splits or top/bottom companion layout.
- Regex search.
- Search over comments or a project-wide Comments view.
- Generated table of contents.
- In-app export preview.
- DOCX, EPUB, PDF, Markdown, LaTeX, or print-ready export.
- Track changes, footnotes/endnotes, tables, embedded images, advanced page layout.
- Project/style/metadata templates.
- Remote history backup.
- General-purpose Git UI.
- Real-time collaboration, mobile, web, and AI-assisted writing.
- Any user-visible large-document mode or feature-reduced document mode.

## 8. Functional requirements

### 8.1 Launcher and project creation

- **PRJ-001:** ParchMint must start at a launcher in v1 rather than automatically reopening previous projects.
- **PRJ-002:** The launcher must show recent projects and actions to create or open a project.
- **PRJ-003:** New Project must collect project title, destination, optional author, and default language.
- **PRJ-004:** The suggested directory name may derive from the project title, but later title changes must not rename or move the directory.
- **PRJ-005:** New projects must contain one Manuscript document named `Untitled Document`, open it immediately, and begin Research empty.
- **PRJ-006:** One project opens in one application window; multiple project windows may be open.
- **PRJ-007:** A project may have only one writable ParchMint process/window at a time. A second attempt should focus the existing window when possible or show a safe locked-project message.
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
- **WS-009:** Focusing or clicking an editor view sets the Inspector context to that editor’s document, even if the tree retains another selection.
- **WS-010:** Focus, selection, open-tab state, and active context must be distinguishable, keyboard accessible, and programmatically exposed. Color alone is insufficient to communicate any of these states.
- **WS-011:** The minimum supported application-window size is 1280 × 720 logical pixels. ParchMint must prevent resizing below that minimum rather than substituting a mobile layout or feature-reduced workspace.
- **WS-012:** Every project workspace must provide persistent, mutually exclusive navigation to Editor, Cards, Global Search, project History, Recently Deleted, Export, and Project Settings. All destinations remain available while a project is open, and the current destination is exposed programmatically.
- **WS-013:** The bottom status bar must provide keyboard-accessible controls to show or hide Explorer and Inspector in addition to word count, save status, and a contextual document-History action. The History action opens project History filtered to the focused pane's active document and is unavailable when no document is active.
- **WS-014:** Applicable Synopsis and metadata values in the Inspector must be editable in place; Comments remain available only for document context.
- **WS-015:** Inspector sections, Explorer section roots, grouped Global Search results, and comparable Cards groups must expose consistent expand/collapse behavior and state. Inspector sections and the Manuscript and Research roots may be collapsed independently without changing authored state.

### 8.3 Explorer and hierarchy

- **TREE-001:** Explorer must show Manuscript and Research as independent collapsible sections containing their ordered hierarchies.
- **TREE-002:** Users must create, rename, and delete groups and documents under either root.
- **TREE-003:** Groups may contain groups and documents; documents may not have children.
- **TREE-004:** All sibling ordering must be explicit and deterministic.
- **TREE-005:** Drag/drop must reorder siblings, move nodes into groups, and move nodes between Manuscript and Research.
- **TREE-006:** Moving a group must move its complete subtree; cycles must be rejected.
- **TREE-007:** Users must be able to select a contiguous range with Shift and noncontiguous nodes with the platform’s additive-selection modifier.
- **TREE-008:** Batch operations must normalize ancestor/descendant selections so a selected ancestor subsumes selected descendants.
- **TREE-009:** Batch move, delete, and applicable metadata operations must preserve relative ordering.
- **TREE-010:** `Copy`/`Paste` in Explorer or Cards must duplicate selected documents with fresh IDs and filenames.
- **TREE-011:** Document copies must include body, title behavior, styles, Synopsis, metadata, and export settings, but must not include comments or history identity.
- **TREE-012:** When an original display title matches its first document-title block, the copy suffix must be applied to both so synchronization remains active. If they differ, only the display title receives the suffix.
- **TREE-013:** `Cut`/`Paste` must move selected documents. Cut items remain until paste succeeds, appear visually pending, and can be cancelled with Escape.
- **TREE-014:** Group copy and keyboard cut are deferred; groups remain movable through drag/drop.
- **TREE-015:** Cross-project copy is deferred.
- **TREE-016:** Single-click selects; double-click or Enter opens a document. Groups do not open as prose editors.
- **TREE-017:** Manuscript documents open in the primary pane by default; Research documents open in the companion by default.
- **TREE-018:** Dragging a document onto a pane or tab strip opens it in that pane.
- **TREE-019:** Explorer must reveal and distinguish the active document in each editor pane. When two different documents are active, both remain identifiable and the document in the focused pane is exposed as the primary active context.
- **TREE-020:** Explorer’s context menu must provide applicable actions to create a group or document, open the selected document, open it in the companion pane, rename, copy, cut, and delete. Inapplicable actions must be omitted or disabled safely.

### 8.4 Editor panes and tabs

- **EDIT-001:** Each editor pane must support multiple reorderable tabs.
- **EDIT-002:** A document may be open at most once per pane and once in each of the two panes simultaneously.
- **EDIT-003:** Opening an already-open document in a pane focuses its existing tab.
- **EDIT-004:** Closing the last companion tab closes the companion pane and expands the primary.
- **EDIT-005:** Closing a tab never deletes the document.
- **EDIT-006:** The same document in two panes must share body content, formatting, comments, undo history, save state, and word count.
- **EDIT-007:** Each view of the same document must retain independent cursor, selection, scroll position, viewport, focus, and local-search state.
- **EDIT-008:** An edit made in one mounted view must appear in the other mounted view by the next rendered frame under normal load.
- **EDIT-009:** Undo invoked from either view must undo the latest document operation in the shared document history, regardless of which view originated it.
- **EDIT-010:** All supported documents, including documents near 250,000 words, must retain the same two-view and editing capabilities. No size-based feature restrictions are permitted.
- **EDIT-011:** The contextual document-History action in the bottom status bar must target the focused pane's active document and open History filtered to changes affecting that document. This is a filtered view of project History, not a separate history store.
- **EDIT-012:** Every primary and companion editor pane must keep its tab strip present when one or more tabs are open and must distinguish the active tab, focused pane, dirty tabs, and named close controls. Tabs retain their preferred widths while they fit. When their combined preferred widths exceed the available strip, every visible tab shrinks to the same reduced width. The shared minimum must preserve the first title character, an ellipsis, and the close control; the full title remains available through the accessible tab name and tooltip.

### 8.5 Rich text and semantic styles

- **FMT-001:** The editor must be WYSIWYG and must not expose raw HTML source editing.
- **FMT-002:** Every text block must have one semantic paragraph style referenced through a stable ID.
- **FMT-003:** Initial reserved styles must include Body, Document Title, Heading 1, Heading 2, Heading 3, Block Quote, and Verse.
- **FMT-004:** Users may edit reserved style properties and display names but may not delete reserved styles.
- **FMT-005:** Users must create, rename, inherit, and edit custom project styles.
- **FMT-006:** Style properties must include font family/size, weight, italics, alignment, first-line/left/right indentation, line spacing, space before/after, keep-with-next, and page-break-before where meaningful.
- **FMT-007:** Changing a style must immediately update every mounted occurrence without rewriting each document solely because visual properties changed.
- **FMT-008:** Initial inline marks must include bold, italic, underline, strikethrough, small caps, superscript, subscript, and links.
- **FMT-009:** Arbitrary per-selection font, size, color, and spacing overrides are deferred.
- **FMT-010:** Scene Break and Page Break must be atomic structural nodes, not literal marker text or paragraph styles.
- **FMT-011:** A scene break’s visible ornament is presentation; word count and search must not treat it as prose.
- **FMT-012:** Page breaks must map to export pagination behavior while remaining atomic editor nodes.
- **FMT-013:** Normal paste must retain only supported structure/marks, sanitize unsafe HTML, and remove unsupported visual styling.
- **FMT-014:** Paste Without Formatting must preserve paragraph boundaries but insert plain text.
- **FMT-015:** Pasted images must be rejected or omitted with a clear v1 notification.
- **FMT-016:** Enter creates a new paragraph; after title or heading it defaults to Body; Shift+Enter inserts a line break.
- **FMT-017:** Tab indents list items inside lists. Outside lists, a literal Tab must be inserted and preserved faithfully. The UI may discourage but must not prohibit it.
- **FMT-018:** Empty list/quotation behavior, backspace block merging, and atomic-node cursor behavior must be consistent, documented, and covered by editor tests.
- **FMT-019:** Every deletable custom style must support deletion from its own list entry. Reserved styles must not support deletion.

### 8.6 Shared formatting toolbar

- **TOOL-001:** Editor mode must have exactly one formatting toolbar, even with two panes.
- **TOOL-002:** The formatting toolbar must remain visible whenever Editor mode is active and must not have a collapsed state in v1.
- **TOOL-003:** Toolbar commands target the focused editor view.
- **TOOL-004:** Interacting with the toolbar must not lose the active editor context for command and undo routing.
- **TOOL-005:** The toolbar must expose style selection, common inline marks, lists/quote, links, Scene Break, and Page Break. Comment creation must not appear in the formatting toolbar; it remains available through the selection-end affordance and editor context menu defined by CMT-007.

### 8.7 Titles

- **TITLE-001:** Each document has an independent display title and may contain a first block with the reserved Document Title role.
- **TITLE-002:** Renaming the tree/Card display title must never edit document body content.
- **TITLE-003:** When the first Document Title block changes, update the display title only if the display title matched the block’s previous value.
- **TITLE-004:** Once display and content titles diverge, later content-title changes must not overwrite the display title.
- **TITLE-005:** If the user later makes the display title equal the current content title, synchronization resumes for subsequent content edits.
- **TITLE-006:** Removing or emptying the title block must not blank the display title.
- **TITLE-007:** Only the first reserved title block participates in synchronization.
- **TITLE-008:** Export must not duplicate the title by generating one in addition to an existing exported title block.

### 8.8 Synopsis and metadata

- **META-001:** Every group and document must have a built-in multiline plain-text Synopsis.
- **META-002:** Synopsis is editable in the Inspector and Cards, globally searchable, and excluded from manuscript export.
- **META-003:** Users must define arbitrary single-line or multiline plain-text metadata fields in Project Settings.
- **META-004:** Field definitions must have stable IDs, label, optional description, applicability by section/node type, optional default, Card visibility, and display order. Display order is the list order in Project Settings and is changed by direct reordering rather than a numeric order field.
- **META-005:** Cards may choose values only from previously defined fields; field definitions cannot be created or edited from Cards.
- **META-006:** A default value is copied into newly created applicable nodes and is not a live fallback.
- **META-007:** Changing applicability hides existing values without deleting them; restoring applicability reveals them again.
- **META-008:** Deleting a field definition requires confirmation and removes current values; History remains able to restore them.
- **META-009:** Renaming a field preserves values because stable ID, not label, defines identity.
- **META-010:** Metadata-field templates are deferred.
- **META-011:** The metadata-field list must support direct reordering and per-field deletion. List order determines display order, and deletion retains the confirmation and recovery behavior in META-008.

### 8.9 Comments and annotations

- **CMT-001:** Documents must support range, cursor-position, and document-level comments.
- **CMT-002:** A comment is a thread with a root message and collapsible replies. Expanding a thread must render its replies in chronological order inside that thread, and the same disclosure control must collapse them again. Every thread must visibly distinguish unresolved and resolved state without relying on color alone.
- **CMT-003:** Each thread must provide an in-thread reply composer. Users must add replies, edit or delete individual messages, resolve unresolved threads, reopen resolved threads, and filter active-document threads by resolved state. Each visible thread must provide the action applicable to its current state.
- **CMT-004:** Comment bodies are plain text with paragraph breaks.
- **CMT-005:** Comments must appear in the active document's Inspector and be indicated at their editor anchors. A document Inspector always includes Comments and shows either the document's actual threads or an explicit empty state.
- **CMT-006:** Selecting a comment scrolls to and highlights its anchor in the last-focused view of that document.
- **CMT-007:** Selecting text must expose an Add Comment affordance near the selection end, and the context menu must provide Add Comment.
- **CMT-008:** With no selection, Add Comment creates a position comment; the Comments panel can create a document-level comment.
- **CMT-009:** Comments must be stored in JSON sidecars outside the canonical HTML prose.
- **CMT-010:** Text anchors must include stable block ID, range, quotation, and context sufficient for conservative reattachment.
- **CMT-011:** Editor changes must map anchors; ambiguous external or structural recovery must orphan a comment rather than attach it incorrectly.
- **CMT-012:** Orphaned comments remain visible and can be reattached or converted to document-level.
- **CMT-013:** Comments are not copied when a document is duplicated.
- **CMT-014:** Comments are excluded from export and v1 global search.

### 8.10 Cards

- **CARD-001:** Cards is an alternate projection of the same hierarchy, titles, Synopsis, metadata, ordering, and selection model used by Editor mode.
- **CARD-002:** v1 uses one vertically ordered, virtualized hierarchy rather than a multi-column corkboard.
- **CARD-003:** Cards must preserve ancestor/descendant hierarchy, allow groups to expand or collapse, and expose the current drag destination.
- **CARD-004:** Dropping onto a group moves the node into that group.
- **CARD-005:** Cards supports Manuscript and Research, defaulting to Manuscript.
- **CARD-006:** Group and document Cards must display title, Synopsis, and applicable project-configured visible metadata values. Title and Synopsis may be edited from Cards where the Cards interaction exposes them. Metadata values are read-only in Cards and are edited only through the Inspector.
- **CARD-007:** Explorer and Inspector remain available around Cards.
- **CARD-008:** Activating a document from Cards switches to Editor and opens it. Clicking either a group Card’s disclosure control or its Card body expands or collapses that group in place. Cards must retain the full hierarchy and must not narrow the list to the selected group’s subtree.
- **CARD-009:** Cards and Explorer share multi-selection and applicable move/copy/cut behavior.
- **CARD-010:** Cards must preserve hierarchy and display only project-configured visible metadata. A Status value appears only when the project defines that field and marks it visible; Cards must not introduce an implicit default Status field.

### 8.11 Search and replacement

- **SEARCH-001:** `Find` opens local search in the focused editor view. Matches are indicated directly in editor content with an active-match distinction; local Find must not add a separate result list to the editor canvas and is hidden when inactive.
- **SEARCH-002:** Each view has independent local-search state.
- **SEARCH-003:** Enter and Shift+Enter navigate results; Escape closes local search and returns focus. Replacement controls are collapsed initially and can be expanded to reveal a replacement field and Replace action.
- **SEARCH-004:** Local search supports case-sensitive and whole-word matching and distinguishes every match from the active match.
- **SEARCH-005:** Local replacement participates in document undo.
- **SEARCH-006:** Global Search replaces Explorer in the left sidebar and provides an explicit return to Explorer. It supports query, case-sensitive and whole-word controls, plus an optional replacement field and Replace action. Replacement review temporarily replaces the normal editor content in the middle workspace pane.
- **SEARCH-007:** Global scope supports Manuscript, Research, both, entire project, or selected subtree.
- **SEARCH-008:** Searchable fields are document body, display title, Synopsis, and user-defined metadata.
- **SEARCH-009:** Global search supports case-sensitive and whole-word modes. Regex is deferred.
- **SEARCH-010:** Results stream, are virtualized, are grouped by document, identify the matched term in each excerpt, and navigate the focused editor view to the selected match.
- **SEARCH-011:** Global replacement modifies editable document bodies only.
- **SEARCH-012:** Global replacement requires a central preview showing every proposed change. The preview occupies the middle workspace pane where the editor normally appears and must not be presented as a modal dialog, floating card, or pop-up overlay. It follows the Manuscript/Research file-tree hierarchy and provides selection controls for applicable groups, documents, and individual matches; parent selection states reflect partial child selection.
- **SEARCH-013:** Applying global replacement is one composite project operation, one logical undo, and one history checkpoint.
- **SEARCH-014:** Search results must be revalidated against current document revisions before navigation or replacement.

### 8.12 Undo and redo

- **UNDO-001:** Document undo covers prose, formatting, content-title changes, and comment changes.
- **UNDO-002:** Document undo is shared across both views of the same document.
- **UNDO-003:** Project undo covers tree creation/deletion/move/order, display-title changes, Synopsis/metadata, metadata definitions, style definitions, and global replacement.
- **UNDO-004:** Keyboard focus selects the undo domain. Editor/comment focus uses document undo; tree/Cards/settings/Inspector values use project undo; focused text inputs use text-input undo.
- **UNDO-005:** Interactive undo may reset when the application closes; older states remain available through History.

### 8.13 Save, recovery, and closing

- **SAVE-001:** Autosave must never block the UI thread.
- **SAVE-002:** Request autosave 1.5 seconds after editing becomes idle and at least every 30 seconds during continuous editing.
- **SAVE-003:** Structural changes request immediate asynchronous save/checkpoint.
- **SAVE-004:** Closing a tab, switching projects, or closing a window requests a high-priority save.
- **SAVE-005:** `Save` immediately queues a high-priority save through the current revision but remains nonblocking.
- **SAVE-006:** The status must distinguish dirty, saving, saved-through-revision, and error states.
- **SAVE-007:** A save captures a consistent revision; edits that arrive during it remain dirty for a later save.
- **SAVE-008:** Only dirty canonical resources are serialized and written.
- **SAVE-009:** Canonical writes must use crash-safe temporary-write/flush/atomic-replace behavior appropriate to each platform.
- **SAVE-010:** A completed history checkpoint must correspond to successfully written canonical state.
- **SAVE-011:** A high-frequency recovery journal must protect changes after the latest completed autosave.
- **SAVE-012:** Recovery data is implementation-specific, versioned, and never the sole copy of completed authored state.
- **SAVE-013:** On save failure, editing remains available, the error persists visibly, recovery remains intact, and the application must not claim Saved.
- **SAVE-014:** A normal close waits asynchronously for the final save. Failure keeps the project open with Retry and Cancel Close.

### 8.14 History and snapshots

- **HIST-001:** Every completed autosave checkpoint is retained indefinitely; v1 performs no automatic pruning.
- **HIST-002:** Git is entirely hidden from the ordinary user interface.
- **HIST-003:** The project root uses one app-managed linear `main` history.
- **HIST-004:** Checkpoints include project manifest, documents, styles, metadata, Synopsis, annotations, and deletion tombstones, but exclude caches, indexes, recovery files, and workspace layout.
- **HIST-005:** History distinguishes autosave, explicit save, structural, named snapshot, and restoration events.
- **HIST-006:** Users may create named snapshots after pending changes are flushed, including a named marker when no content changed.
- **HIST-007:** History must support chronological virtualized browsing, filtering to changes affecting the active document, a read-only side-by-side checkpoint-versus-current comparison with word-level changes, and document, group-subtree, or whole-project restoration. Selecting an entry updates the comparison directly. Restore confirmation must identify the selected scope and its impact; separate Preview and Compare actions are not required.
- **HIST-008:** Restoration creates a new checkpoint and never rewinds or rewrites existing history.
- **HIST-009:** Current canonical project files must remain readable if Git history is missing or damaged; the user may reinitialize history from current state.
- **HIST-010:** History maintenance runs only on background workers and must not compete perceptibly with active editing.
- **HIST-011:** Remote push/backup is deferred.

### 8.15 Deletion and Recently Deleted

- **DEL-001:** v1 has no Trash node in the live hierarchy.
- **DEL-002:** Delete removes content from the current project state but preserves it in History.
- **DEL-003:** A deletion tombstone records stable node ID, title, type, section, former parent/order, deletion time, and restoring checkpoint.
- **DEL-004:** Recently Deleted lists deleted documents and groups, shows the selected item's formatted contents in one read-only preview rather than a comparison, presents its restore location, and can restore complete subtrees.
- **DEL-005:** Restore returns to the old location where possible or the relevant section root when the former parent is gone.
- **DEL-006:** Session undo must immediately reverse deletion when still available.
- **DEL-007:** v1 provides no purge, Empty Trash, or permanent history-erasure command.

### 8.16 Research

- **RES-001:** v1 Research contains only groups and app-created managed rich-text notes.
- **RES-002:** Managed Research notes use the same editor and canonical document format as Manuscript documents.
- **RES-003:** Research is excluded from normal manuscript export.
- **RES-004:** Import, attachments, linked resources, PDFs, images, arbitrary files, and external previews are deferred.

### 8.17 Word counts and spellcheck

- **WORD-001:** Show selection count when text is selected and active-document count otherwise.
- **WORD-002:** Provide document, group, Manuscript, Research, and useful project totals.
- **WORD-003:** Count exportable titles, headings, and prose; exclude comments, Synopsis, metadata, scene breaks, and page breaks.
- **WORD-004:** Common contractions and ordinary hyphenated compounds count as one; numbers count as words.
- **WORD-005:** Open-document counts update incrementally; closed-document counts may use a disposable cache.
- **SPELL-001:** Spellcheck is required in v1 on Windows, macOS, and Linux and must provide correct, performant behavior in every supported platform webview.
- **SPELL-002:** Spellcheck must support project default language, optional per-document override, global dictionary, project dictionary, context suggestions, and viewport/recent-change bounded checking.
- **SPELL-003:** Spellcheck failure must not block typing or saving. Errors must remain visible and recoverable, and release is blocked until the cross-platform correctness and latency gates pass or the product owner approves a versioned specification change.
- **SPELL-004:** A misspelled word must be decorated in place in the editor, and its spelling context menu must be anchored to that word with suggestions and applicable dictionary/language actions.

### 8.18 Export

- **EXP-001:** v1 exports one self-contained HTML5 manuscript.
- **EXP-002:** Export scope supports entire Manuscript, one group and descendants, or selected documents.
- **EXP-003:** Project defaults, group overrides, and document overrides use Inherit/Enabled/Disabled for inclusion, title emission, and page-break behavior.
- **EXP-004:** Numbering is an export-run option rather than arbitrary persistent per-node numbering.
- **EXP-005:** Group titles may emit headings despite groups having no body.
- **EXP-006:** The exporter must not duplicate existing document title content.
- **EXP-007:** Research, comments, Synopsis, and metadata are excluded unless a future feature says otherwise.
- **EXP-008:** The export dialog contains scope, output path/name, inclusion/title/page-break controls, numbering, and Export.
- **EXP-009:** After export, the user may open the result or reveal it in the file manager.
- **EXP-010:** Generated TOC and in-app preview are deferred.

## 9. Canonical project requirements

- **DATA-001:** Current authored data must use restricted deterministic HTML5, TOML, CSS, and JSON.
- **DATA-002:** Groups should map to directories and documents to HTML files, while the manifest is authoritative for identity, ordering, titles, metadata, and semantics.
- **DATA-003:** Internal filenames are implementation details and are never normal UI labels.
- **DATA-004:** Renaming a displayed title must not rename the backing file.
- **DATA-005:** Serialization uses UTF-8, LF, deterministic attribute ordering, stable whitespace/escaping, stable block IDs, and no rewriting of unchanged documents.
- **DATA-006:** Deleting caches and indexes must not break current project functionality.
- **DATA-007:** Deleting history removes old versions but must not damage current authored content.
- **DATA-008:** Canonical paths must be relative, normalized, traversal-safe, case-conflict checked, and portable across Windows/macOS/Linux.
- **DATA-009:** ProseMirror JSON is transient editor state and must not become the canonical project format.
- **DATA-010:** SQLite is derived state only and must never be the sole project store.

## 10. Scale and performance

### Required scale

- **PERF-001:** Projects up to 10–20 million words.
- **PERF-002:** Approximately 300–500 Manuscript documents and 25–50 Research documents.
- **PERF-003:** Individual documents up to approximately 250,000 words.
- **PERF-004:** All documents within the supported range receive the same features and interaction model.

### Interactive budgets

- **PERF-005:** Key-to-paint target: p95 ≤16 ms and p99 ≤33 ms under normal load.
- **PERF-006:** No save/history/search/export operation may block the UI thread for more than 2 ms in one event-loop turn.
- **PERF-007:** Warm first editable viewport target is ≤250 ms for ordinary project documents.
- **PERF-008:** At the 250,000-word supported maximum, the release gate is ≤1 second to first editable viewport on agreed reference hardware, with no feature reduction.
- **PERF-009:** Warm indexed global search begins returning results within 200 ms.
- **PERF-010:** Tree/Card movement visibly updates within 100 ms.
- **PERF-011:** Project open must not load every document body.
- **PERF-012:** Search-index rebuild, history maintenance, export, save, and word-count rebuild run in bounded background work and can be paused/cancelled where appropriate.
- **PERF-013:** Memory must stabilize under repeated open/edit/undo/search/close cycles; closing a document/view must reclaim material editor resources.
- **PERF-014:** Transparent optimizations such as incremental plugins, worker mirrors, decoration throttling, content visibility, or future bounded rendering may be used without changing user-visible behavior.

## 11. Accessibility and international text

- **A11Y-001:** All primary workflows must be keyboard accessible.
- **A11Y-002:** Focus must be visible and programmatically exposed.
- **A11Y-003:** Screen readers must expose windows, mode controls, panes, tabs, tree hierarchy, Cards, headings, paragraphs, lists, comments, toolbar state, save/error state, and dialogs.
- **A11Y-004:** Color cannot be the only status/selection/error indicator.
- **A11Y-005:** UI must work at 100%, intermediate scaling, and 200% scaling without caret, selection, hit-test, or layout drift.
- **A11Y-006:** Editor input must correctly support grapheme movement, combining marks, emoji, CJK IME composition/candidates, Arabic, bidirectional text, and literal tabs.
- **A11Y-007:** Reduced-motion preferences must suppress nonessential animation while preserving state clarity.
- **A11Y-008:** Native interactive accessibility must be tested with VoiceOver, Narrator or NVDA, and Orca before release.

## 12. Privacy and security

- **SEC-001:** Core functionality requires no network connection.
- **SEC-002:** Tauri must load only bundled local application content in v1.
- **SEC-003:** Use a strict CSP and least-privilege Tauri capabilities.
- **SEC-004:** No remote content may receive privileged Tauri access.
- **SEC-005:** Project paths and pasted HTML must be validated/sanitized.
- **SEC-006:** History and search network features are disabled in v1.
- **SEC-007:** Dependency locks, advisories, license inventory, and SBOM are release artifacts.

## 13. Canonical user flows

1. **Create and write:** launch, create project, initial Untitled Document opens, type, autosave, close, reopen.
2. **Organize:** create nested groups/documents, multi-select, drag, copy/paste duplicate, cut/paste move, cross-section move.
3. **Compare:** open Manuscript in primary, Research or another Manuscript document in companion, focus changes Inspector and toolbar target.
4. **Same document twice:** open one document in both panes, use independent scroll/selection, edit and undo from either view.
5. **Plan in Cards:** edit Synopsis/metadata, expand/collapse, reorder, open a document in Editor.
6. **Comment:** select text, use selection-end affordance, reply, resolve, navigate, reopen.
7. **Search/replace:** local search in one view; global search by scope; preview and apply body replacement.
8. **Recover:** force termination after unsaved input, replay recovery, verify canonical and history consistency.
9. **Delete/restore:** delete subtree, continue editing, restore through Recently Deleted or History.
10. **Export:** configure scope/inheritance and generate self-contained HTML.
11. **Move platform:** close cleanly, copy project between operating systems, reopen with identical hierarchy/content/history.

## 14. v1 release gates

ParchMint v1 is complete only when:

1. Every must-level requirement is implemented or explicitly waived by the product owner in a versioned spec change.
2. The approved Penpot handoff is reconciled with no unexplained major visual/interaction deviations.
3. Canonical format golden tests and cross-platform round trips pass.
4. Save, recovery, history, corruption isolation, deletion, and restoration fault tests pass.
5. `git2` history and SQLite FTS5 adapter contract tests pass.
6. Normal and 250,000-word document fixtures retain the same feature set.
7. Performance budgets pass on agreed reference hardware or an explicit product-owner waiver is recorded.
8. Native IME, clipboard, high-DPI, and accessibility validation passes on Windows, macOS, and Linux.
9. Installers/packages launch and operate on the supported platform matrix.
10. No required workflow depends on a proprietary project database, installed Git executable, network service, or raw source editing.
