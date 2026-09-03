# ParchMint user guide

ParchMint keeps a novel in a project directory on your computer. The workspace
combines a rich-text editor, a structured outline, planning metadata, search,
saved History, and manuscript export.

## Create or open a project

ParchMint opens on the launcher. Select **Create Project** to choose a project
title and directory and, optionally, enter an author name. A new project opens
with one Manuscript document named **Untitled Document** and an empty Research
section.

Select a recent project on the launcher to reopen it, or select **Open Project**
and choose its directory. ParchMint allows one writable session for a project;
if it is already open, use its existing window.

## Find your way around the workspace

The workspace has three main areas:

- **Explorer:** organizes groups and documents under the Manuscript and
  Research roots.
- **Editor or Cards:** shows the writing surface or a compact outline of the
  same project structure.
- **Inspector:** edits the selected item's title, Synopsis, metadata, and, for
  documents, provides a comments index.

Use the top ribbon to switch between **Editor** and **Cards**. The actions on
the right open History, Recently Deleted, Export, and Settings. The bottom
status bar shows the active document or selected-text word count, current save
state, and controls for the sidebars.

Drag the dividers to resize Explorer, the editor panes, and Inspector. Press
**F6** to move keyboard focus between the main workspace regions.

## Organize a novel

Use **+ New** in Explorer or a row's context menu to create a group or
document. Groups can contain other groups and documents. Documents cannot have
children.

A single click selects a document and opens it as a replaceable preview tab.
Double-click it, press **Enter**, edit it, or deliberately open it to keep the
tab. Manuscript documents normally open in the primary pane; Research notes
normally open in the companion pane. Closing a tab does not delete its
document.

Drag rows to reorder them, move them into a group, or move them between
Manuscript and Research. Use **Shift** to select a range and **Ctrl** on Windows
and Linux or **Command** on macOS to add individual rows to the selection.
Copy and paste duplicates selected documents with new identities. Cut and
paste moves them.

Cards shows the same hierarchy as compact rows with titles, Synopsis text, and
configured metadata. Select a card and edit its details in Inspector. Drag a
card to reorganize the project, or double-click a document card to open it in
Editor.

## Write and format

The formatting toolbar applies to the focused editor pane. It provides
paragraph styles, bold, italic, underline, strikethrough, lists, block quotes,
links, scene breaks, and page breaks. Project styles can be configured in
**Settings > Styles**.

Open a document in the companion pane when you want two documents side by
side. You can also open the same document in both panes. In that case edits,
formatting, comments, undo history, save state, and word count are shared, while
each pane keeps its own cursor, selection, scroll position, and local search.

Inspector edits the selected group's or document's Synopsis and metadata.
Metadata fields and their display order are configured in **Settings >
Metadata fields**.

## Add and review comments

Select text, or place the cursor for a position comment, then open the editor
context menu and choose **Add Comment**. The thread card beside the text lets
you reply, edit or delete messages, resolve or reopen the thread, and delete
the thread.

Inspector lists all comments for the active document. Select a comment there
to reveal its location in the editor. Comments stay with the project but are
excluded from manuscript export.

## Search and replace

Press the primary modifier plus **F** to search within the focused editor view.
Use **Enter** and **Shift+Enter** to move between matches, and **Escape** to
close local search.

Open **Global Search** from the Explorer header or press the primary modifier
plus **Shift+F** to search the whole project. Results are grouped by document.
Selecting a result opens the document and reveals the match. Adding replacement
text opens a preview where you can include or exclude matches before applying
one project-wide replacement.

## Save, recover, and restore

ParchMint saves in the background after editing becomes idle and during longer
writing sessions. Structural changes request a save immediately. The status
bar distinguishes unsaved, saving, saved, and failed states. Use **Save** or
the primary modifier plus **S** when you want to request an immediate save.

Each completed change is recorded in **History**. Select a checkpoint to
compare it with the current document, create a named snapshot for an important
milestone, or restore the complete project to an earlier state. Restoration
creates a new checkpoint and preserves the existing timeline.

Deleting a group or document removes it from the current outline. Use Undo
immediately, or open **Recently Deleted** to preview and restore it later.

If ParchMint finds edits newer than the last completed save after an interrupted
session, it presents a recovery summary before opening the workspace. Choose
**Recover** to apply those edits or **Discard** to open the last completed save.

## Spellcheck and appearance

Misspelled words have an in-place underline. Open the word's context menu to
choose a suggestion or add it to a dictionary. Manage project and global words
in **Settings > Dictionaries**. Spellcheck uses the bundled `en-US` dictionary
and works offline.

Use **Settings > Appearance** to choose System, Light, or Dark. System follows
the operating-system appearance. The setting applies to every open ParchMint
window and does not change manuscript formatting or exported output.

## Export a manuscript

Open **Export** from the top ribbon, choose an output HTML file, review title,
page-break, and numbering options, then select **Export**. ParchMint exports the
entire Manuscript as one self-contained HTML file. Research notes, Synopsis,
metadata, and comments are not included. After a successful export, use
**Open** or **Reveal** to inspect the result.

## Move or back up a project

Close the project, then copy its complete directory. The directory contains the
current project files and ParchMint's saved History. Open the copied directory
from the launcher on the destination computer. The `.parchmint/cache/` data
inside the project can be rebuilt. Recovery data also travels with a complete
copy; window and tab layout is stored in the operating system's application-data
directory and does not travel with the project.

## Keyboard shortcuts

The primary modifier is **Ctrl** on Windows and Linux and **Command** on macOS.

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
