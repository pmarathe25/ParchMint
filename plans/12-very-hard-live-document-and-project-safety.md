# Stage 12: live document and project safety

Difficulty: very hard  
Recommended model: GPT-5.6 Sol, high or max reasoning  
Cost-conscious fallback: GPT-5.6 Terra, max reasoning with failure-injection checkpoints  
Depends on: stage 11  
Master references: `PLAN.md` “Data-integrity, security, and privacy contract”; `plans/10-audit-report.md` §§3, 4.1, 5.1–5.4, and 7 P0.1/P0.3

## Outcome

Connect the tested document lifecycle to the actual panes and make project entry safe on every platform. Typing must survive focus changes, pane operations, project close, normal quit, crashes, external edits, and export. Project paths must come from native dialogs or a validated normalization boundary, and crashed processes must not leave permanent false locks.

The audit remediation already prevents opening one document in two independent pane buffers, shows the backend save state in the status bar, percent-encodes attachment file URLs, and fixes restore/undo and stale sibling-index failures. Preserve those behaviors while replacing the private-buffer workaround with an authoritative live-document session model.

## Primary ownership

- `crates/parchmint-app/src/document.rs` and `workspace.rs`
- Lifecycle/export integration in `crates/parchmint-bridge`
- `app/qml/components/PaneHost.qml`, project/attachment/export dialogs, and application shutdown wiring
- Cross-platform advisory-lock implementation
- Recovery, backup, external-change, path, and bridge integration tests

## Required work

### 1. Authoritative open-document sessions

- Give each open document one revisioned session owned below QML; panes reference that session rather than keeping authoritative private strings.
- Connect editor deltas to `JournalRequest` after the approved debounce and on focus loss, then schedule `CanonicalSaveRequest` off the UI thread.
- Associate all work/completions with project generation, document ID, and revision. A stale completion may not acknowledge, clear dirty state, or overwrite newer content.
- Flush or safely journal before swap, close pane, trash, close/open/create project, export snapshot, and application quit.
- Add explicit Ctrl/Cmd+S as a flush request and expose per-pane plus project-level saving/saved/error state.

### 2. Recovery, backup, and external changes

- Scan recovery records on project open and present preview/restore/discard/save-copy choices before normal editing.
- Isolate corrupt recovery records and report them individually; one bad record must not hide other recoverable documents.
- Rotate canonical backups according to settings and enforce `MAX_DOCUMENT_BYTES` when reading backup sources.
- Poll/watch fingerprints for open canonical files. Auto-reload only clean sessions, using the same parse options; dirty sessions require compare/reload/overwrite/save-copy resolution.
- Update `docs/user-guide/backup-recovery.md` only after the UI behavior is reachable and integration-tested.

### 3. Safe pane/project transitions

- Add a QML-to-backend flush handshake that can veto a destructive transition on journal/canonical-save failure.
- Wire window close and `QCoreApplication::aboutToQuit`; define a bounded clean-shutdown wait and a recovery-safe fallback if canonical saving cannot finish.
- When trashing a node/subtree, resolve open sessions before moving canonical files. Trashed nodes must never remain editable.
- If the same document is requested in the other pane, focus/share the existing session; never create a second stale buffer.
- Compile/export must freeze the latest acknowledged live revisions, including text that has not lost focus.

### 4. Native path workflow

- Use `FolderDialog` for open/create project (parent folder plus validated project name) and `FileDialog` for attachments, export/diagnostics destinations, and onboarding samples.
- Centralize path conversion at the bridge: trim, reject empty/NUL, decode local `file:` URLs, expand a leading home marker where supported, and absolutize relative paths against `QStandardPaths::DocumentsLocation`.
- Validate create/open intent before disk mutation and return friendly distinctions for missing path, regular file, inaccessible parent, malformed URL, and unsupported remote URL.
- Seed dialogs with Documents/recent project locations and preserve platform-native paths without lossy string round trips.

### 5. Crash-safe project locking

- Replace create-new sentinel semantics with an OS advisory lock (`flock`/`fcntl`, `LockFileEx`, and the supported macOS equivalent) held by an open handle so process death releases it.
- Store diagnostic PID/hostname/timestamp metadata only as supplementary information, never as the lock authority.
- Distinguish already-open, read-only fallback, and genuine permission failure. Do not offer destructive lock breaking unless the OS confirms no live owner.
- Test two-process exclusion and crash release on every CI platform where the primitive is available.

## Verification

- Bridge/QML integration tests type without focus loss, then swap, close, export, trash, close project, and quit; canonical/recovery state must contain the latest revision.
- Failure injection covers journal write, canonical write, backup, disk full, permission changes, worker delay, stale completion, and crash at each lifecycle boundary.
- External-change tests cover clean reload and every dirty conflict action.
- Path/lock tests cover plain native paths, `file:` URLs, spaces, `#`, `%`, Unicode, home/relative input, launcher-like CWD `/`, two processes, and killed owners.

## Acceptance gate

- No D1–D4 audit reproduction can lose or silently overwrite acknowledged typing.
- A crash loses no more than the configured debounce interval, and recovery is reachable from the shipping UI.
- Export, trash, project close, and quit include the current editor revision or stop with a visible actionable error.
- Normal GUI flows never depend on a hand-typed path, and relative/home/file-URL inputs resolve predictably when supplied programmatically.
- A crashed process cannot permanently block a project, while two live writers cannot both acquire it.

## Out of scope

- Full WYSIWYG/editor feature activation (stage 13, built on this session boundary)
- Whole-project incremental persistence and 10k-node optimization (stage 14)
- Cloud/network synchronization

## Handoff

Create `docs/handoffs/12-live-document-and-project-safety.md`. Document session ownership, revision/flush state machines, shutdown timing, recovery/conflict UX, path normalization rules, platform lock primitives, failure-injection evidence, and any manual platform gaps.
