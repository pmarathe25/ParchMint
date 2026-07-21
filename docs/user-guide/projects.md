# Projects

> Read when organizing manuscript, research, metadata, panes, or search.

## Binder and planning

Manuscript and research roots contain arbitrarily nested groups and documents.
Select a node to edit its title, synopsis, label, and status. Move nodes with
drag/drop or move/indent/outdent commands. Binder preorder determines compile
order; structural undo does not affect editor text undo.

## Research and panes

Research notes and attachments stay under the research root and are excluded
from manuscript compile unless explicitly selected by scope or preset. The two
panes are symmetric: pin research in one while writing or navigating in the
other. Focus determines which pane receives view and editor commands.

Attachments are copied into the project under UUID-derived names. Display names
are metadata. Escaped, symlinked, oversized, missing, or active-content assets
are rejected or reported.

## Search and counts

Project search covers titles, synopses, manuscript, and research. Counts use
Unicode-aware rules shared with compile. The local SQLite index is rebuildable
and never authoritative.
