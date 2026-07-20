# Keyboard shortcuts

This is the current reference for the shortcuts registered in `Main.qml`.
The platform's standard key is used where Qt provides one, so Command replaces
Control on macOS.

| Shortcut | Command | Availability |
|---|---|---|
| `Ctrl/Cmd+Shift+P` | Open command palette | Always |
| `Ctrl/Cmd+N` | New project | Always |
| `Ctrl/Cmd+O` | Open project | Always |
| `Ctrl/Cmd+Z` | Undo structural change | Project open |
| `Ctrl/Cmd+Shift+Z` | Redo structural change | Project open |
| `Ctrl/Cmd+Shift+Up` | Move selected node up | Node selected |
| `Ctrl/Cmd+Shift+Down` | Move selected node down | Node selected |
| `Ctrl/Cmd+]` | Indent selected node | Node selected |
| `Ctrl/Cmd+[` | Outdent selected node | Node selected |
| `Delete` | Move selected node to trash | Node selected |
| `Ctrl/Cmd+Tab` | Focus next split pane | Split workspace |
| `Ctrl/Cmd+Shift+F` | Focus project search | Project open |
| `Ctrl/Cmd+F` | Find/replace in the focused document | Project open |
| `Ctrl/Cmd+Alt+F` | Preview project-wide replacement | Project open |
| `Ctrl/Cmd+,` | Open settings | Always |

Menus, shortcuts, and the palette use stable IDs from the same Rust command
catalog; command availability is filtered from project/selection context.
Menu actions and drag/drop have the same Rust command path as these shortcuts.
Text editing shortcuts are provided by Qt's editor and platform conventions;
they are not duplicated by the binder shortcut layer. New commands must be
added to the registry/UI and this table in the same change.
