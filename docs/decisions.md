# Current technical decisions

This page gives the reasons for choices that affect the whole codebase. It
describes the current design and does not add product requirements.

| Choice | Why |
| --- | --- |
| Rust and `iced` for the first desktop UI | `iced` provides a conventional, independent desktop application framework for the whole product. `parchmint-ui-api` keeps the rest of the application independent of it. |
| A custom `iced` editor widget | ParchMint needs a WYSIWYG editor with two independent views of one document. The built-in `iced` text editor is a plain-text control, so ParchMint owns a virtualized rich-text widget. |
| ParchMint-owned editor session | ParchMint owns stable IDs, transactions, comments, anchors, undo order, revision mapping, and project-file projection. Two panes share this session without sharing selection or scroll state. |
| GPL-3.0-or-later for ParchMint | The project uses the GNU General Public License version 3 or later. Dependency review must confirm that each shipped dependency is compatible with it. |
| JSON Schema with checked-in Rust bindings | `parchmint-contracts` owns only the durable JSON boundaries ParchMint uses. Checked-in Rust bindings are verified against schema checksums and regeneration diffs; `typify` is not a build dependency. |
| Small crates with ParchMint-owned APIs | Core code does not import GUI, database, Git, editor-engine, or operating-system library types. |
| Deterministic project files hold the current project | A project remains readable and portable without ParchMint's private databases. |
| libgit2 stores project checkpoints | ParchMint includes the Git functionality it needs. Users do not have to install Git. |
| SQLite FTS5 powers search | It searches large projects quickly. ParchMint rebuilds the index from project files. |
| Workspace layout is application data keyed by project ID | Opening tabs, scrolling, or resizing panes does not change project files or create History checkpoints. |
| Slow work runs outside the UI loop | Typing and painting stay responsive while files, History, search, spellcheck, and export run. |
