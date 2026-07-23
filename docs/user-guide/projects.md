# Projects

> Read when organizing manuscript, research, metadata, panes, or search.

## Binder and planning

Manuscript and research roots contain arbitrarily nested groups and documents.
The binder behaves as an ordered file tree: expand sections, create sections or
documents from the context menu, and drag items before, after, or into another
section. Invalid parent or ordering targets are rejected before a drop. Selecting
a document opens or activates a tab in the focused editor pane. The bottom-right
binder control hides or restores the tree. Binder preorder determines compile
order. Use **Properties** for summary, status, label, and compile inclusion.
Structural undo does not affect editor text undo.

Cards is a separate manuscript view. It shows the same nested order with each
section's title and summary. Expand sections or drag cards to reorder and
reparent them; edit a summary from the card context menu.

## Research and panes

Research notes and attachments stay under the research root and are excluded
from manuscript compile unless explicitly selected by scope or preset. Editor
panes can be split repeatedly left, right, up, or down, and each pane can hold
multiple document tabs. Drop a binder item on a pane edge to create a split
containing that item, or in the center to open it as a tab. Closing the last tab
closes its pane when another pane remains. Focus determines which pane receives
binder navigation and editor commands. The single formatting strip targets the
active tab in the focused pane.

Attachments are copied into the project under UUID-derived names. Display names
are metadata. Escaped, symlinked, oversized, missing, or active-content assets
are rejected or reported.

## Search and counts

Project search covers titles, synopses, manuscript, and research. Counts use
Unicode-aware rules shared with compile. Results show document titles and their
human-readable binder context; selecting a result opens it in a tab in the
focused pane. The local SQLite index is rebuildable and never authoritative.
