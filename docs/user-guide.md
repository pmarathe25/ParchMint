# User guide

ParchMint keeps your novel in a local project folder. The workspace combines
rich-text writing, an outline, planning details, search, History, and export.
See [installation](install.md) for release downloads and setup.

Shortcuts use **Ctrl** on Windows and Linux or **Command** on macOS, called the
*primary modifier* below.

ParchMint opens your most recent project directly. On first launch it opens
**My Writing**, a local workspace in the application-data directory. Use the
project title menu to create or open another project. The comments sidebar starts
hidden; the comments button in the status bar reveals it. Opening the companion pane for the first
time creates an empty tab.

## Create or open a project

ParchMint reopens your most recently opened project. If it cannot open, the app
opens the local **My Writing** workspace.

Click the project title at the top left to create or open another project, or
choose a recent project. Narrow windows show the project icon instead. Deleted
project folders are removed from recents when the chooser opens.

Choose **Create Project**, enter a title, and choose a folder. New projects contain
one Manuscript document, **Untitled Document**, and an empty Research section.
Each project has one writable session; opening it again focuses its existing window.

## Navigate the workspace

| Area | Use |
| --- | --- |
| Explorer | Navigate documents while writing in Editor |
| Editor | Write and format documents in one or two panes |
| Overview | Create the project structure, edit planning details, and rearrange cards |
| Comments | Find and navigate document comments |

The top bar switches between **Editor** and **Overview**. Icons at the top right
open History, Recently Deleted, Export, and Settings. Hover to see their names;
the active screen has an underline. The status bar shows word counts, save state,
and sidebar controls.

Drag dividers to resize sidebars and panes. Press **F6** to move focus between
workspace regions.

## Organize documents

In Overview, use **+** beside Manuscript, Research, or a group to create a document
or group there. Enter a name, then write its synopsis. Click any synopsis or
metadata value to edit it; click elsewhere or press Escape to finish. Right-click a group’s header or the space around its cards to create items
inside it; right-click the Overview background to create at the section root.
Explorer’s right-click menu also supports creation. Groups can contain groups
and documents. Choose **Rename** from an item’s context menu or press **F2**;
Enter confirms and Escape cancels.

Drag rows before or after siblings, into a group, or between Manuscript and
Research. **Shift** selects a range; the primary modifier adds individual rows.
Copy and paste duplicates selected documents. Cut and paste moves them.

A single click opens a document as a replaceable preview tab. Double-click,
press **Enter**, edit, or deliberately open it to keep the tab. Manuscript opens
in the primary pane; Research normally opens in the companion pane. Closing a
tab leaves its document in the project.

Use the split button at the right end of the tab bar to open the companion pane.
The button stays at the right edge and is highlighted while the companion is open.
It keeps its tabs when hidden; the first opening creates an empty tab.
Drag tabs between panes, or drop an Explorer document into either pane to open it.
Tabs fit their titles; the overflow button lists documents that do not fit.
Each pane’s upper-right button expands that pane into a temporary focus view.
Click it again to restore the split and sidebars. When both panes show the same document, edits,
comments, undo, saves, and word counts are shared; cursors, selections, scrolling,
and local searches remain independent.

Each editor pane has a **+** button for a new tab (primary modifier plus **T**).
Start writing before choosing a location. **Save** (primary modifier plus **S**)
asks for a name and a location in Manuscript or Research. Closing a changed draft
offers Save, Don’t Save, and Cancel. Empty, unchanged tabs close without creating
project documents. Drafts receive autosave and recovery protection; their words
are excluded from manuscript totals and export until filed in Manuscript.

Overview has its own Manuscript/Research switch and group disclosures. Collapsing
Explorer groups does not hide cards. Sibling documents share a grid row; groups
remain separate headings. Click synopsis or metadata text to edit it, drag a title
to reorganize, or double-click a document title to open Editor.
Cards show the full synopsis and configured metadata. Their word counts come
from document text; groups total their descendants, including collapsed groups.

After naming a new card, focus moves to its synopsis. While writing a synopsis,
press the primary modifier plus **Enter** to add the next document in the same
group; add **Shift** to create a group.

Click a group heading to expand or collapse it. Its synopsis and metadata sit
above its children inside the expanded group.
Drag onto the left or right half of a document card to insert before or after it.
In a single column, use the upper or lower half. A group’s upper and lower edges
place items before or after it; its middle accepts items inside the group and expands it.
The dragged card follows the cursor while surrounding cards preview its placement.
Explorer keeps its rows in place and marks the proposed destination. Release to keep the move; press **Escape** or leave the drop area to
restore the original layout. Groups opened for the preview also return to their
previous state when cancelled. Modifier selection, cut and paste, and Undo work
with the shared Explorer selection.

## Write and format

The formatting controls in the top bar act on the focused editor pane. They provide paragraph styles, font family and size, paragraph alignment, line spacing, bold,
italic, underline, strikethrough, lists, block quotes, links, and comments.
The list button inserts bullets; its adjacent arrow offers bulleted and numbered lists.
Hover a link to see its destination, Ctrl+click (Command+click on macOS) to open it,
or choose **Copy link address** from its context menu.
List icons apply bulleted or numbered lists; the break menu contains page
and scene breaks. Hover over an icon to see its action. **B/I/U/S** shows
active formatting. With no selection, a formatting toggle applies to subsequent
typing.

Font family and size apply to selected text or subsequent typing. Their controls show the effective font at the cursor, including inherited style values. Choose
**Style default** for family or **Auto** for size to inherit the paragraph style.
Copy and cut currently place plain text on the system clipboard. Rich-text paste
retains supported formatting, including ParchMint's font attributes.

Word counts update as you write. In Editor, the status bar labels the current
Document or text Selection count. In Overview, Selected totals the selected
chapters and groups without counting overlapping selections twice. Manuscript
shows all manuscript document text; Research is excluded. Synopsis and metadata never contribute to word counts.

Create and edit paragraph styles from **Manage Styles** at the bottom of the style dropdown.
Manage metadata with **Fields…** in Overview; new fields appear on outline rows.
Settings contains appearance and dictionary controls. Style changes apply throughout
the project. Typography, spacing, and
pagination have separate groups. Press **Enter** or **Apply** to save a property;
leave it blank to inherit. Dropdowns control inheritance, alignment, and on/off
properties. The metadata editor also controls field display order and applicability.

## Comment on writing

Select text or place the cursor, then click the speech-bubble icon (**Add comment**) in the toolbar or
**Add Comment** in the editor context menu. The card beside the text supports
replies, message edits and deletion, resolving or reopening, and thread deletion.
The Comments panel lists document comments; select one to reveal its location.
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

Comparisons show the saved version on the left and current writing on the right,
with matching paragraphs aligned as you scroll. Changed words are highlighted.
Formatting-only changes show HTML differences; style changes show CSS. Restoring a checkpoint replaces the
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
formatting or export. Enable **Reduce motion** here to make pane, card, tab, and
menu transitions immediate. This setting is saved for all windows.

## Export a manuscript

Open **Export**, choose an output HTML file, review title, page-break, and
numbering options, then select **Export**. The result is one self-contained HTML
file containing the whole Manuscript. Research, Synopsis, metadata, and comments
are excluded. Use **Open** or **Reveal** after completion to inspect the file.

## Move or back up a project

Close the project and copy its complete folder, including hidden files. This
preserves current writing, saved History, and recovery data. Open the copied
folder from the project title menu on the destination computer.

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
