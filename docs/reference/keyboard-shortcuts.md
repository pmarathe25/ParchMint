# Keyboard commands

> Read when changing menus, shortcuts, command-palette entries, or keyboard documentation.

Use Command on macOS and Control on Windows/Linux.

| Shortcut | Action | Available when |
|---|---|---|
| `Ctrl/Cmd+Shift+P` | Command palette | Always |
| `Ctrl/Cmd+N` | New project | Always |
| `Ctrl/Cmd+O` | Open project | Always |
| `Ctrl/Cmd+S` | Save | Project open |
| `Ctrl/Cmd+W` | Close project | Project open |
| `Ctrl/Cmd+Shift+E` | Export | Project open |
| `Ctrl/Cmd+Z` | Undo editor text | Editor focused |
| `Ctrl/Cmd+Shift+Z` | Redo editor text | Editor focused |
| `Ctrl/Cmd+Shift+Up/Down` | Move selected node | Node selected |
| `Ctrl/Cmd+[` / `]` | Outdent / indent | Node selected |
| `Delete` | Move selected node to trash | Node selected |
| `Ctrl+Tab` | Switch split pane | Split active |
| `Ctrl/Cmd+F` | Find in document | Project open |
| `Ctrl/Cmd+Shift+F` | Focus project search | Project open |
| `Ctrl/Cmd+Alt+F` | Preview project replacement | Project open |
| `Ctrl/Cmd+1/2` | Editor / Cards | Project open |
| `Ctrl/Cmd+,` | Settings | Always |
| `Ctrl/Cmd+?` | Keyboard help | Always |

Shortcuts and the palette share stable IDs from
`crates/parchmint-app/src/commands.rs`. Update all surfaces and command tests together.
