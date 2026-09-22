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

The icon rail on the left opens **Editor**, **Overview**, **History**,
**Recently Deleted**, **Export**, and **Settings**. Hover an icon for its label.
The project name at the top of Explorer opens project creation, opening, and
recent projects. It hides with Explorer. The status bar shows
counts, save state, and sidebar controls.

Drag dividers to resize sidebars and panes. Press **F6** to move focus between
workspace regions.

## Organize documents

In Overview, each creation placeholder has two halves: the document icon creates
a document; the group icon creates a group in that location. Enter confirms its
name; Escape discards the new item before it is created. Then write its synopsis.
Click any synopsis or metadata value to edit it; click elsewhere or press Escape
to finish. Right-click a group’s header or the space around its cards to create
items inside it; right-click the Overview background to create at the section root.
Explorer’s right-click menu also supports creation. Groups can contain groups
and documents. Choose **Rename** from an item’s context menu or press **F2**;
Enter confirms and Escape cancels.

Drag rows before or after siblings, into a group, or between Manuscript and
Research. **Shift** selects a range; the primary modifier adds individual rows.
Copy and paste duplicates selected documents. Cut and paste moves them.

A single click opens a document as a replaceable preview tab. Double-click,
press **Enter**, edit, or deliberately open it to keep the tab. Manuscript opens
in the primary pane; Research normally opens in the companion pane. Closing a
tab leaves its document in the project. Right-click a tab to close it or move it
to the other pane. Use Explorer to rename, copy, move, or delete documents.

Use the split button at the right end of the tab bar to open the companion pane.
The button stays at the right edge and is highlighted while the companion is open.
It keeps its tabs when hidden; the first opening creates an empty tab.
Drag tabs between panes, or drop an Explorer document into either pane to open it.
Tabs fit their titles; the overflow button lists documents that do not fit.
Each pane’s upper-right button expands that pane into a temporary focus view.
Focus view hides the navigation rail, tabs, breadcrumbs, second-pane control,
sidebars, and status bar. The native titlebar also hides when the window is
maximized; a floating window keeps its titlebar. Fullscreen remains fullscreen.
**F11** also toggles focus view.
Choose **Exit focus** in the top bar or press **Escape** to restore the layout.
When both panes show the same document, edits,
comments, undo, saves, and word counts are shared; cursors, selections, scrolling,
and local searches remain independent.

Each editor pane has a **+** button for a new tab (primary modifier plus **T**).
Start writing before choosing a location. **Save** (primary modifier plus **S**)
asks for a name and a location in Manuscript or Research. Expand folders in the
location tree to choose a parent. Tab moves between locations; Left and Right
collapse or expand the focused folder. Closing a changed draft
offers Save, Don’t Save, and Cancel. Empty, unchanged tabs close without creating
project documents. Drafts receive autosave and recovery protection; their words
are excluded from manuscript totals and export until filed in Manuscript.

Overview has its own Manuscript/Research switch and group disclosures. Collapsing
Explorer groups does not hide cards. Explorer hides while Overview is open.
Document cards sit beside each other when space permits; indentation and group
headings show their nesting. Click synopsis or metadata text to edit it, drag a title
to reorganize, or double-click a document title to open Editor.
Cards show the full synopsis with applicable metadata in a narrow column beside
it. Narrow layouts stack the fields. Their word counts come
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

The formatting controls in the top bar act on the focused editor pane. Paragraph
styles, bold, italic, lists, links, and comments stay directly accessible. The list
button inserts bullets; its adjacent arrow offers bulleted and numbered lists.
When space permits, font family and size, underline, strikethrough, block quotes,
alignment, line spacing, and breaks also appear in the toolbar. In narrower
layouts, **Format** holds these controls. Click outside the panel or
press Escape to dismiss it. Hover over an icon to see its action. Formatting
toggles show their active state and apply to subsequent typing when no text is selected.

Hover a link to see its destination, Ctrl+click (Command+click on macOS) to open it,
or choose **Copy link address** from its context menu.

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
pagination have separate groups. Click outside a field or press **Enter** to save its value;
leave it blank to inherit. Dropdowns control inheritance, alignment, and on/off
properties. The metadata editor also controls field display order and applicability.

## Comment on writing

Select text or place the cursor, then click the speech-bubble icon (**Add comment**) in the toolbar or
**Add Comment** in the editor context menu. The card beside the text supports
replies, message edits and deletion, resolving or reopening, and thread deletion.
The Comments panel lists comments for the selected document. Selecting a group
includes comments from all documents nested inside it, labeled by document title.
Select a comment to open its document and reveal its location.
Right-click a thread for Reply, Resolve/Reopen, or Delete thread. Reply opens
the thread's editing card; deleting a thread asks for confirmation there.
Comments stay in the project and are excluded from export.

A draft stays attached to its original document and selection while you consult
Research. If that document changes before submission, ParchMint keeps the draft
and asks you to select its text again. Scrolling another pane keeps the toolbar
on the last focused editor; click that pane's text to edit it.

## Search and replace

Press the primary modifier plus **F** for Find in the focused pane. **Enter** and
**Shift+Enter** move between matches; **Escape** closes Find. The search button
beside the breadcrumbs below the tabs also opens Find. Both local and global
search focus their query box immediately; Escape dismisses either search.

Open **Global Search** in the Explorer header or press the primary modifier plus
**Shift+F**. Results are grouped by document; select a result to reveal its match.
Expand **Replace** to replace document text. Enter replacement text, review the
eligible matches, and include or exclude matches before applying the replacement.
An empty replacement deletes the matched text. Title, synopsis, and metadata
matches remain searchable but are excluded from replacement.

## Save and review History

ParchMint saves after editing becomes idle and during longer writing sessions.
Structural changes request a save immediately. The status bar shows unsaved,
saving, saved, or failed state. Choose **Save** or press the primary modifier
plus **S** to request a save now. If saving fails while closing, choose **Keep
working**, **Try again**, or **Exit without saving**. Exiting skips the final save;
existing recovery records remain available on reopening.

Choose **History** in the navigation rail to open project history.
For one document, right-click its Explorer entry or Overview card and choose
**History**. The page names the document and its location; switching panes does not change that target.
Document timelines include project milestones. The scope dropdown above the
timeline selects **Entire project** or a single document. Opening History does
not save a draft.

Selecting a checkpoint compares that saved version with the current project,
including unsaved writing. Changed documents and groups appear in collapsible
sections matching the outline. Renames strike through the old title. Unchanged
items and lines are omitted. Content appears side by side, with changed words
highlighted; metadata, synopsis, comments, and export changes have separate compact
sections.
Formatting-only changes show HTML differences; project style changes show CSS.
**Restore project** replaces the whole project. **Restore document** replaces
only the selected document's text, formatting, and comments; its name, location,
planning details, other documents, and project settings stay unchanged. Both
actions ask for confirmation and create a new History entry. A document cannot
be restored from a version that predates it.

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
formatting or export. **Application zoom** adjusts the size of the interface and
editor display from 75% to 150%; **Reset** returns it to 100%. Zoom is saved for
all windows and does not change manuscript formatting or export. At larger zoom
levels, opening one sidebar closes the other when space is limited.
Enable **Reduce motion** here to make pane, card, tab, and
menu transitions immediate. This setting is saved for all windows.

## Export a manuscript

Choose **Export** from the application menu, select an output HTML file, review title, page-break, and
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

Open **Settings → Keyboard shortcuts** to search commands and change a binding.
Click a binding and press the desired combination; Escape cancels recording.
**Clear** disables a shortcut, **Reset** restores one default, and **Reset all**
restores the shipped keymap. Conflicting combinations show the command already
using them. Editor and Overview commands can reuse a combination because they
operate on different pages. Changes apply to all windows and persist across restarts.
Use Control, Alt, or Command with a key, or a function key; ordinary typing and
standard field navigation remain available. The table below lists common defaults.

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
