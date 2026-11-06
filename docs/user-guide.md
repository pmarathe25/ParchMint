# User guide

ParchMint keeps your novel in a local project folder. The workspace combines
rich-text writing, an outline, planning details, search, History, and export.
See [installation](install.md) for release downloads and setup.

Shortcuts use **Ctrl** on Windows and Linux or **Command** on macOS, called the
*primary modifier* below.

ParchMint opens your most recent project directly. On first launch it opens
**My Writing**, a local workspace in the application-data directory. Use the
project title menu to create or open another project. The assistant and Notes
sidebar starts hidden; the notes button in the status bar reveals it.

## Create or open a project

ParchMint reopens your most recently opened project. If it cannot open, the app
opens the local **My Writing** workspace.

Click the project title at the top left to create or open another project, or
choose a recent project. Narrow windows show the project icon instead. Deleted
project folders are removed from recents when the chooser opens.

Choose **Create Project**, enter a title, and choose a folder. New projects contain
one Manuscript document, **Untitled Document**, and empty Lore and Research sections.
Each project has one writable session; opening it again focuses its existing window.

## Navigate the workspace

| Area | Use |
| --- | --- |
| Explorer | Navigate documents while writing in Editor |
| Editor | Write and format documents in tabs |
| Overview | Create the project structure, edit planning details, and rearrange cards |
| Assistant and Notes | Ask questions, check continuity, research sources, and navigate annotations |

The icon rail on the left opens **Editor**, **Overview**, **History**,
**Recently Deleted**, **Export**, and **Settings**. Hover an icon for its label.
The ParchMint logo opens project creation, opening, and recent projects.
The project name beside it opens the same menu when Explorer is visible.
The logo remains available when Explorer is collapsed. The status bar shows
counts, save state, and sidebar controls.

Drag dividers to resize sidebars. Press **F6** to move focus between
workspace regions.

Layout, open tabs, scroll positions, and expanded groups are remembered when you
save content, use **Save**, or close the project. Navigating alone does not save
the project or create a History revision.

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

Drag rows before or after siblings, into a group, or between Manuscript, Lore,
and Research. **Shift** selects a range; the primary modifier adds individual rows.
Copy and paste duplicates selected documents. Cut and paste moves them.

A single click opens a document as a replaceable preview tab. Double-click,
press **Enter**, edit, or deliberately open it to keep the tab. Manuscript, Lore,
and Research documents open in the same editor. Closing a tab leaves its document
in the project. Use Explorer to rename, copy, move, or delete documents.

Tabs fit their titles; the overflow button lists documents that do not fit.
The editor's upper-right button expands it into a temporary focus view.
Focus view hides the navigation rail, tabs, breadcrumbs,
sidebars, and status bar. The native titlebar also hides when the window is
maximized; a floating window keeps its titlebar. Fullscreen remains fullscreen.
**F11** also toggles focus view.
Choose **Exit focus** in the top bar or press **Escape** to restore the layout.
The editor has a **+** button for a new tab (primary modifier plus **T**).
Start writing before choosing a location. **Save** (primary modifier plus **S**)
asks for a name and a location in Manuscript, Lore, or Research. Expand folders in the
location tree to choose a parent. Tab moves between locations; Left and Right
collapse or expand the focused folder. Closing a changed draft
offers Save, Don’t Save, and Cancel. Empty, unchanged tabs close without creating
project documents. Drafts receive autosave and recovery protection; their words
are excluded from manuscript totals and export until filed in Manuscript.
Choosing **Don’t Save** discards the draft without adding it to Recently Deleted.

Overview has its own section switch and group disclosures. Collapsing
Explorer groups does not hide cards. Explorer hides while Overview is open.
Document cards sit beside each other when space permits. Each group is a larger
card enclosing its nested groups and documents. Click synopsis or metadata text to edit it, drag a title
to reorganize, or double-click a document title to open Editor.
Collapsed cards have a fixed size, with a synopsis preview and the fields chosen for
collapsed cards beside it; missing values appear as dashes. **Expand details**
grows the card vertically to show the full synopsis and every field. Metadata stays
in its separate column with field names above values. Several cards can stay expanded
at once; each expands smoothly in place. Creation placeholders match the card size
with muted icons and dashed outlines. Card word counts come from document text;
groups total their descendants, including collapsed groups.

After naming a new card, focus moves to its synopsis. While writing a synopsis,
press the primary modifier plus **Enter** to add the next document in the same
group; add **Shift** to create a group.

Click a group heading to expand or collapse its children. Its synopsis and metadata
always remain fully visible above the children. Wide groups arrange metadata in up
to three columns beside the synopsis; narrower groups use fewer columns.
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
styles, bold, italic, lists, links, and notes stay directly accessible. The list
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
shows all manuscript document text; Lore and Research are excluded. Synopsis and metadata never contribute to word counts.

Create and edit paragraph styles from **Manage Styles** at the bottom of the style dropdown.
Manage metadata with **Fields…** in Overview; new fields appear on outline rows.
Settings contains appearance and dictionary controls. In the Styles and Fields dialogs,
**Save** applies your changes; **Cancel** asks before discarding changes only when
the dialog has changes to lose. Editing a field
or pressing **Enter** updates the dialog's draft. The style preview remains visible
while the properties scroll. Built-in styles show effective values and have a separate
**Reset to defaults** button. Custom styles inherit from their selected parent;
**Reset** beside an overridden property restores inheritance. Use **+ New** to add
an entry and the trash icon to delete a custom style or metadata field. The metadata
editor also controls field order and applicability. Drag fields across the divider
between **Shown on collapsed cards** and **Hidden until expanded** to change their
card visibility. New fields start in the hidden section. Expanded cards show all fields.

## Add notes to writing

Select text and choose **Add note** in the small toolbar beside the selection.
You can also choose **Add note** from the editor context menu or use its keyboard
shortcut. Type in the popover and press **Enter** to add the note; use
**Shift+Enter** or **Alt+Enter** for a line break. The note stays anchored to the
selected text, outside exported manuscripts.

Hover anywhere along an annotated line between the selection's endpoints to see
its notes. Click a note body in the popover or **Notes** sidebar to edit it.
Press **Enter** to save, **Escape** to cancel, or **Shift+Enter** or **Alt+Enter**
to insert a line break. The sidebar shows a card for each note, with its quoted
text set apart from the note body. Click the quote or card margin to open the
annotated passage in the editor. The trash button deletes a note after
confirmation. Selecting a group shows notes from all its documents, labeled by
document title. Older projects with comment threads retain every saved message
as a separate note at the original anchor.

A draft stays attached to its original document and selection while you consult
Research. If that document changes before submission, ParchMint keeps the draft
and asks you to select its text again.

## Use the assistant

Open the sidebar with the status-bar notes button and choose **Ask**, **Check**,
**Research**, or **Notes**. In **Settings → AI assistant**, choose OpenAI,
Anthropic, or an OpenAI-compatible endpoint, enter a model for that provider,
and save your own API key. Each provider requires its own account and key;
OpenAI credits do not pay for other providers. Keys are
kept in the operating system's credential store, or in memory for the current
session if you choose that option. A compatible local endpoint can be used
without a key. Different providers and models support different capabilities.
Each assistant mode has its own conversations. Use the conversation selector to
switch, **+** to start another, and **⋯** to rename or delete one. Drafts,
context restrictions, and pending requests stay with their conversation. A new
conversation inherits the current context restrictions; its history starts empty.
OpenAI Ask and Research and Anthropic Ask can show text while it is generated.
For compatible endpoints, enable **Stream responses** if the provider supports
it. Citations and Research save actions become available after the response
finishes. Anthropic Research shows completed responses.
Press Enter to add a line to a question, and Ctrl+Enter (⌘+Enter on macOS) to
send it. While a request is running, you can draft the next question or use
**Stop** to cancel the current request.

**Ask** selects relevant Manuscript, Lore, and Research passages, including
current open-editor text. Retrieval uses text matches, so it can miss relevant
material that uses different words. It can also follow a small number of names
from relevant identity statements; these are search hints, not established canon.
When space permits, the assistant also receives immediately adjacent authored
paragraphs. Those paragraphs can provide context but may describe different events.
In Ask or Check, open **Project context**
and select a document or group in Explorer to pin or exclude it. Pinned material
is prioritized within the context limit; exclusions also apply to notes and take
precedence over pins. In **Scope**, choose the selected item to restrict Ask to
that document or group. For Check it restricts comparison evidence; the check
target is also included. **Editor selection** captures the selected text at send time;
Check also includes its target in that mode. Select up to 32 KiB of text before
sending. To read notes, choose a document or project scope instead. Pins outside
the limit have no effect, and a check cannot include an
excluded target. While context is limited or an exclusion is set, earlier
conversation turns are not sent to the provider, so they cannot reintroduce
material outside that boundary. Context choices are retained locally
for each mode and can be removed in the same control. Ask "Read my notes" for the
current document, or "Read notes across the project" to include notes from other
documents. Requests with ambiguous restrictions are retained for clarification.
For other questions, enter the question, open **Project context**, and select
**Current** or **Project** under **Notes · this question**. The selection
applies to one request and respects context limits and exclusions. Editing the
question, changing its context, or changing project structure clears or
invalidates that selection. Selection-only context cannot include notes;
**By request** restores the default behavior.
After a colon, use a short question such as "what do they say about Marin?" or
the scope "across the project"; other forms need clarification. A request can
include up to 16 note messages, each limited to 1,200 characters;
notes and other excerpts also share a 64 KiB context limit. ParchMint indicates
when material is omitted, and tells the assistant its answer is not exhaustive.
To create a note, select a passage and ask, for example, "Add a note here:
check the tide". Note-derived answers are not sent as conversation history; ask
to read notes again when a follow-up needs them. If the document
changes before a requested note is created, the proposed note remains visible
for review. Conversations, drafts, and proposed notes are stored locally in the
application-data directory, separately from the project and its backups. Research
import previews are retained there as well. An
interrupted request is shown when you reopen the project and is not sent again
automatically. Deleting a conversation removes its saved conversation, draft,
and proposed notes. **Check** reviews the
selected document or group against related project passages; it is not an
exhaustive continuity guarantee. When the provider returns verifiable findings,
expand a finding to inspect both quoted passages and open their sources. A
provider response that cannot be validated remains visible as an unstructured
check. Mark a finding **Intentional**, **Dismiss**, or **Resolved** to record
your decision in the project. Decisions participate in project Undo and Redo.
Changed evidence requires another check; matching a later finding to a decision
is conservative and may require reviewing it again. Saving the first decision
upgrades the project format, so older ParchMint versions cannot open it.
For an open, unformatted paragraph in the checked material, **Propose edit**
requests a replacement and shows it beside the original. **Apply** changes only
that passage and can be undone as one editor change; **Discard** leaves it alone.
The proposal is available only for the current editor session and is discarded
when the project closes. A changed source or context requires a new check.
**Context** reveals the excerpts
sent with an answer or check, including passages the model did not cite. Source
links open the cited document and, for newly retrieved authored passages, select
the matching text when its revision is still current. Older citations and
extracted original sources open the associated document. Changed evidence is
marked and cannot be navigated as if it still matched. After following a source,
**Back to writing** returns to the original tab and, when its editor session and
revision still match, restores its selection and scroll position. If the writing
changed in the meantime, ParchMint opens the current text without applying the
old position.

**Research** sends your query to a web-enabled provider without project
excerpts. Review the answer and its sources before saving a summary, source links,
or one original source to a Research group. A paused Anthropic search can make up
to two additional requests to finish; each request may incur provider charges.
An original downloads an unchanged public HTTPS PDF or text source up to
64 MiB and stores it in the project. The assistant can search its UTF-8 plain
text, Markdown, HTML, or XHTML content
when the original is at most 8 MiB and its markup is within the extraction work limits.
HTML is extracted as text without executing scripts or rendering styles, so it
may differ from the page as displayed; inspect the original when a detail matters.
For non-ASCII HTML, a UTF-8 declaration or byte-order mark is required; older
saved sources may not retain their original HTTP charset. Conflicting or
unsupported encodings and XHTML that requires DTD-defined entities cannot be
extracted. PDF originals are saved but not yet searchable. The assistant reports
when an original cannot be searched. Select its Research document and use
**Save original** in the Research panel to save a copy elsewhere. Source-link
saves retain links and access date without copying the source. If web search is
unavailable, you can enter a source link manually. If an import was interrupted,
check Research before continuing; changing the recovered preview starts a new
import.

For the supported priced OpenAI model, ParchMint estimates its own usage and
reserves an amount before sending a request. Configure the local limit, warning,
and UTC reset day under **Usage settings**. This estimate is not your account
balance and does not include spending by other app instances, apps, or devices. Other models may
have no available estimate or local dollar limit. Stopping a request cannot
guarantee the provider will not charge for work already submitted.

## Search and replace

Press the primary modifier plus **F** for Find in the editor. **Enter** and
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

History adds a revision only when authored content or project settings change.
Named milestones can mark an unchanged version. Internal save counters do not
create revisions.

Choose **History** in the navigation rail to open project history.
For one document, right-click its Explorer entry or Overview card and choose
**History**. The page names the document and its location; switching panes does not change that target.
Document timelines include project milestones. The scope dropdown above the
timeline opens an expandable tree to select **Entire project**, a group, or a document.
Group views show changes within that branch; select a document or the whole project to restore. Opening History does
not save a draft.

Selecting a checkpoint compares that saved version with the current project,
including unsaved writing. Changed documents and groups appear in collapsible
sections matching the outline. Renames strike through the old title. Unchanged
items and lines are omitted. Content appears side by side, with changed words
highlighted; metadata, synopsis, notes, and export changes have separate compact
sections.
Formatting-only changes show HTML differences; project style changes show CSS.
**Restore project** replaces the whole project. **Restore document** replaces
only the selected document's text, formatting, and notes; its name, location,
planning details, other documents, and project settings stay unchanged. Both
actions ask for confirmation and create a new History entry if they change the
current project. A document cannot
be restored from a version that predates it.

Notifications appear at the bottom right without moving the workspace. Popovers
can be dismissed and expire after five seconds. Errors stay available in
**Notifications** until dismissed or cleared.

## Restore deleted or interrupted work

Delete removes an item from the outline. Ctrl/Cmd+Shift+Delete deletes the selected documents or groups. Use Undo immediately, or open
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
file containing the whole Manuscript. Lore, Research, Synopsis, metadata, and notes
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
| Close the current editor tab | Ctrl+W | Command+W |
| Close the current window | Ctrl+Shift+W | Command+Shift+W |
| Undo | Ctrl+Z | Command+Z |
| Redo | Ctrl+Y | Command+Shift+Z |
| Local Find | Ctrl+F | Command+F |
| Global Search | Ctrl+Shift+F | Command+Shift+F |
| Bold / italic / underline | Ctrl+B / Ctrl+I / Ctrl+U | Command+B / Command+I / Command+U |
| Add or edit a link | Ctrl+K | Command+K |
| Move between workspace regions | F6 | F6 |

Additional configurable shortcuts include Ctrl/Cmd+G / Ctrl/Cmd+Shift+G for the
next/previous local search match.
Ctrl/Cmd+Alt+F opens Fields.

Select manuscript text to add a comment or link from the small toolbar below it.
Links can point to a web address or a document chosen from the project tree. Internal
links follow documents through renames and moves. Search result headings collapse
or expand each document's matches; their breadcrumb shows where the document lives.
