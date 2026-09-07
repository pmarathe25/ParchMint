# `parchmint-ui-iced`

This crate owns ParchMint's Iced event loop, windows, widgets, and temporary UI
state. Project commands, persistence, native services, and rich-text sessions
run through their respective crate interfaces.

## Interface

`run_native_desktop(NativeDesktopStartup)` runs the application. Startup and
lifecycle callbacks use ParchMint values. The concrete UI converts those values
to Iced events and window IDs internally.

The optional `interaction-harness` feature exposes `NativeDesktopHarness` for
headless input. `visual-verification` enables `capture_visual` for deterministic
PNG captures. The default desktop enables neither feature.

## Implementation

- [native.rs](src/native.rs) routes input and completions to the owning window.
  The native driver applies editor commands during the input event, preserving
  the original view and selection even if focus changes immediately afterward.
- [project_workspace.rs](src/project_workspace.rs) owns Explorer, Cards,
  Inspector, History, search, settings, export, and recovery presentation.
  Explorer and Cards share selection normalization and move validation.
- [editor_workspace.rs](src/editor_workspace.rs) owns tabs, pane state, local
  search, and comment drafts. Drafts retain their originating pane, mount
  generation, revision, and selection; failed submissions keep their text.
- [project_runtime.rs](src/project_runtime.rs) resolves UI identifiers from the
  current session snapshot and calls application ports. Project revision checks
  protect structure; document revisions are validated by the document operation.
  Recovery recording can advance document state without changing the outline.
- `iced_project_surface` and `iced_editor_surface` render workspace state.
  `design_tokens` maps the shared token catalog to an Iced theme.
- [async_service_feeds.rs](src/async_service_feeds.rs) handles search, History,
  export, and recovery results. Search accepts only the active query generation.
- [native/worker_pool.rs](src/native/worker_pool.rs) runs blocking UI work on
  four workers with at most 128 queued jobs. Submission reports overload without
  blocking input. Services with their own workers keep their own limits.

Comment navigation reads the live session’s current anchors, including unsaved
comments and positions shifted by editing.

A document session is shared across panes; each view keeps independent selection
and viewport state. Tab switches advance mount generations, and delayed view
results are ignored when their target no longer matches. Loading a document
merges matching bodies without replacing newer outline state.

Project mutations and their saves are serialized. Save results acknowledge only
the captured revisions, leaving later edits dirty. A completed native call can
outlive its window, but its stale completion cannot update that window. Close
waits for the final save. History resolves document identities from the selected
checkpoint’s manifest.
[history_project.rs](src/history_project.rs) compares the project on a worker,
including live editor drafts, outline changes, comments, dictionary, and styles.

The [UI driver](../../tests/parchmint-ui-driver/README.md) verifies these paths
through rendered controls, including delayed completion delivery and visible
application failures. Tests define supported behavior.

Spellcheck requests contain at most 4096 scalars around a view’s caret.
Large paragraphs use the same bounded path; cancelled checks are silent, and
service errors retain their cause through the spellcheck interface.
