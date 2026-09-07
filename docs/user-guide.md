# User guide

ParchMint keeps your novel in a local project folder. The workspace combines
rich-text writing, an outline, planning details, search, History, and export.
See [installation](install.md) for release downloads and setup.

Shortcuts use **Ctrl** on Windows and Linux or **Command** on macOS, called the
*primary modifier* below.

## Create or open a project

From the launcher, select **Create Project**, enter a title, choose a folder,
and optionally enter an author name. New projects contain one Manuscript document,
**Untitled Document**, and an empty Research section.

Select a recent project to reopen it, or choose **Open Project** and its folder.
Each project has one writable session; use its existing window if it is already
open.

## Navigate the workspace

| Area | Use |
| --- | --- |
| Explorer | Organize groups and documents under Manuscript and Research |
| Editor | Write and format documents in one or two panes |
| Outline | Review the hierarchy, Synopsis, and metadata as compact rows |
| Inspector | Edit the selected item's title, Synopsis, and metadata; find document comments |

The top ribbon switches between **Editor** and **Outline** and opens History,
Recently Deleted, Export, and Settings. The status bar shows document or selected
word counts, save state, and sidebar controls.

Drag dividers to resize sidebars and panes. Press **F6** to move focus between
workspace regions.

## Organize documents

Select a destination in Explorer, open **+ New**, and choose **Document** or
**Group**. The menu shows the destination; enter a name to finish. Row context
menus offer the same actions. Groups can contain groups and documents; documents
cannot have children.

Drag rows before or after siblings, into a group, or between Manuscript and
Research. **Shift** selects a range; the primary modifier adds individual rows.
Copy and paste duplicates selected documents. Cut and paste moves them.

A single click opens a document as a replaceable preview tab. Double-click,
press **Enter**, edit, or deliberately open it to keep the tab. Manuscript opens
in the primary pane; Research normally opens in the companion pane. Closing a
tab leaves its document in the project.

The pane menu's **Open beside** shows the document in another pane. **Focus pane**
hides or restores sidebars. When both panes show the same document, edits,
comments, undo, saves, and word counts are shared; cursors, selections, scrolling,
and local searches remain independent.

In Outline, select a row to edit its details in Inspector. Drag to reorganize,
or double-click a document to open Editor.

## Write and format

The toolbar acts on the focused editor pane. It provides paragraph styles, bold,
italic, underline, strikethrough, lists, block quotes, links, scene breaks, page
breaks, and comments. **B/I/U/S** shows active formatting. With no selection,
a formatting toggle applies to subsequent typing.

Document and manuscript word counts update as you write. Research does not add
to the manuscript total. Inspector edits the selected item's Synopsis and metadata.

Configure project formatting in **Settings → Styles**. Typography, spacing, and
pagination have separate groups. Press **Enter** or **Apply** to save a property;
leave it blank to inherit. Dropdowns control inheritance, alignment, and on/off
properties. **Settings → Metadata fields** controls fields and their display order.

## Comment on writing

Select text or place the cursor, then choose **Comment** in the toolbar or
**Add Comment** in the editor context menu. The card beside the text supports
replies, message edits and deletion, resolving or reopening, and thread deletion.
Inspector lists document comments; select one to reveal its location.
Comments stay in the project and are excluded from export.

A draft stays attached to its original document and selection while you consult
Research. If that document changes before submission, ParchMint keeps the draft
and asks you to select its text again. Scrolling another pane keeps the toolbar
on the last focused editor; click that pane's text to edit it.

## Search and replace

Press the primary modifier plus **F** for Find in the focused pane. **Enter** and
**Shift+Enter** move between matches; **Escape** closes Find.

Open **Global Search** in the Explorer header or press the primary modifier plus
**Shift+F**. Results are grouped by document; select a result to reveal its match.
Enter replacement text to preview changes and include or exclude matches before
applying one project-wide replacement.

## Save and review History

ParchMint saves after editing becomes idle and during longer writing sessions.
Structural changes request a save immediately. The status bar shows unsaved,
saving, saved, or failed state. Choose **Save** or press the primary modifier
plus **S** to request a save now.

Completed saves appear in **History**. Select a checkpoint to compare it with the
current project, or create a named snapshot for a milestone. Comparisons include
unsaved drafts, added and deleted documents, outline changes, comments, and
project settings. Opening History does not save a draft.

Removed lines use **−** and added lines use **+**. Formatting-only changes show
HTML differences; style changes show CSS. Restoring a checkpoint replaces the
whole project and creates a new History entry, preserving the earlier timeline.

Notification banners can be dismissed and expire after five seconds. Errors stay
available in **Notifications** until dismissed or cleared.

## Restore deleted or interrupted work

Delete removes an item from the outline. Use Undo immediately, or open
**Recently Deleted** to preview and restore it later.

After an interrupted session, ParchMint offers recovery when it finds edits newer
than the last completed save. Choose **Recover changes** to keep them or
**Open last saved version** to discard them. **Show technical details** reveals
records and revision information.

## Spellcheck and appearance

Misspelled words are underlined. Open a word's context menu for suggestions or to
add it to a dictionary. Spellcheck uses bundled en-US words and works offline.

In **Settings → Dictionaries**, select project or global scope to add, remove,
or search words. Project words travel with the project; global words apply to
all projects on this device.

**Settings → Appearance** offers System, Light, and Dark. System follows the OS
appearance. This choice applies to every window without changing manuscript
formatting or export.

## Export a manuscript

Open **Export**, choose an output HTML file, review title, page-break, and
numbering options, then select **Export**. The result is one self-contained HTML
file containing the whole Manuscript. Research, Synopsis, metadata, and comments
are excluded. Use **Open** or **Reveal** after completion to inspect the file.

## Move or back up a project

Close the project and copy its complete folder, including hidden files. This
preserves current writing, saved History, and recovery data. Open the copied
folder from the launcher on the destination computer.

The `.parchmint/cache/` directory can be rebuilt. Window and tab layout lives in
the device's application-data directory and does not travel with the project.

## Keyboard shortcuts

| Action | Windows and Linux | macOS |
| --- | --- | --- |
| Create a project | Ctrl+N | Command+N |
| Open a project | Ctrl+O | Command+O |
| Save | Ctrl+S | Command+S |
| Close the current window | Ctrl+W | Command+W |
| Undo | Ctrl+Z | Command+Z |
| Redo | Ctrl+Y | Command+Shift+Z |
| Local Find | Ctrl+F | Command+F |
| Global Search | Ctrl+Shift+F | Command+Shift+F |
| Bold / italic / underline | Ctrl+B / Ctrl+I / Ctrl+U | Command+B / Command+I / Command+U |
| Add or edit a link | Ctrl+K | Command+K |
| Move between workspace regions | F6 | F6 |
