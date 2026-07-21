# Backups and recovery

ParchMint keeps one live, revisioned copy of each open document below the user
interface. Typing is copied into that session immediately. A recovery journal
is written after 750 ms without another edit, and is forced before pane swaps,
pane or project close, trash, export, normal quit, and when an editor loses
focus. Ctrl+S on Windows and Linux, or Cmd+S on macOS, flushes all open
documents. The window and each editor show Saved, Saving, Unsaved, or an
actionable save error.

When a project opens with newer recovery data, editing is held behind a dialog.
For each record you can preview and restore it, discard only that record, or
save its Markdown body to a separate file. A damaged record is shown and
discarded separately; it does not hide healthy recovery records. ParchMint does
not consume recovery records when the project was opened using the read-only
fallback, because another process may still own and update them.

Canonical saves use same-directory temporary files, flush, and atomic replace
where supported. Before replacing a document, ParchMint keeps up to ten bounded
prior canonical versions under `.parchmint/backups/<document-id>/`. These are
local safety data rather than part of the portable canonical project. Migration
also creates an idempotent pre-migration backup before changing canonical
files.

Open canonical files are checked for outside changes. A clean editor reloads
the disk version automatically using the same Markdown rules used at project
open. If you have local changes, ParchMint shows your version beside the disk
version and requires one of three explicit choices: reload the disk version,
overwrite it with your journaled live version, or save your version as a copy
and then reload. It never silently overwrites either side.

If canonical saving fails during a destructive transition, the transition is
stopped and the editor stays open. Normal shutdown waits up to three seconds;
if canonical saving cannot finish but every current revision has a recovery
journal, shutdown may continue with a visible recovery-safe status. The final
application shutdown hook repeats that bounded attempt for platform quit paths
which bypass the window, then makes one last journal-only attempt if needed.
