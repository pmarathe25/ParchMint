# Workspace shell

- **WS-001:** The compact top mode control must switch between `Editor` and `Cards` without changing underlying data; History, Recently Deleted, Export, and Project Settings remain separate persistent destinations.
- **WS-002:** Editor mode must show a collapsible/resizable left sidebar, central editor area, and collapsible/resizable right Inspector.
- **WS-003:** The left sidebar must provide Explorer and Global Search panels.
- **WS-004:** The Inspector must provide Synopsis, Metadata, and Comments sections as applicable.
- **WS-005:** The primary editor pane fills the central area when the companion is closed.
- **WS-006:** The optional companion pane opens on the right in v1.
- **WS-007:** Layout widths, split ratio, collapsed states, tabs, active view, scroll positions, and current mode must restore per project without entering authored history.
- **WS-008:** Clicking a tree node or Card sets the Inspector context to that node.
- **WS-009:** Focusing or clicking an editor view sets the Inspector context to that editor's document, even if the tree retains another selection.
- **WS-010:** Focus, selection, open-tab state, and active context must be distinguishable and usable from the keyboard. Color alone is insufficient.
- **WS-011:** The minimum supported application-window size is 1280 × 720 logical pixels. ParchMint must prevent smaller window sizes and must not substitute a mobile or feature-reduced layout.
- **WS-012:** Every project workspace must provide persistent, mutually exclusive navigation to Editor, Cards, project History, Recently Deleted, Export, and Project Settings. Global Search is entered from the Explorer header and replaces Explorer in the left sidebar.
- **WS-013:** The bottom status bar must provide keyboard-accessible controls to show or hide Explorer and Inspector in addition to word count and save status. Pane controls expose pressed state.
- **WS-014:** Completed project actions and failures provide a brief top-of-workspace notification. Notifications are also available from a bounded, session-only drawer in the status area; non-actionable notices expire while failures remain until dismissed.
- **WS-014:** Applicable Synopsis and metadata values in Inspector must be editable in place; Comments remain available only for document context.
- **WS-015:** Inspector sections, Explorer roots, grouped Global Search results, and comparable Cards groups must expose consistent expand/collapse behavior and state.
