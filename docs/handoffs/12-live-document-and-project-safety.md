# Stage 12 handoff: live document and project safety

Status: implemented in the working tree and verified on the available Linux
host. The repository's existing Linux/macOS/Windows CI matrix will exercise the
portable Rust path and locking tests; this handoff does not claim a local
Windows or macOS GUI run.

## Working-tree, format, and interface state

- Base commit: `b795638999753db0aa3170e3a8d7c33f30b435fc` (`Harden
  Markdown and export integrity`).
- Verification commit: the final `git log -1` commit containing this handoff.
- Canonical project, Markdown, recovery, asset-catalog, and compile-preset
  format versions remain unchanged. Recovery and rotating backups remain local
  `.parchmint/` safety data; Markdown/TOML remain authoritative.
- No ADR was added or superseded. Stage 12 explicitly required replacing the
  sentinel lock and private pane buffers; the implementation follows the
  accepted Qt/Rust ownership and document-lifecycle ADRs.
- New Rust service interfaces include `DocumentSavePlan`, the opaque
  `PreparedAtomicWrite`, revisioned workspace session accessors, the serial
  `DocumentLifecycleWorker`, isolated `RecoveryScan`, and the programmatic path
  normalization boundary. The CXX-Qt backend adds live-body/flush/status,
  recovery/conflict, read-only fallback, lifecycle polling, and bounded-quit
  invokables. These are source APIs, not persisted-schema changes.

## Session ownership and revision state machine

- `ProjectWorkspace` owns exactly one `DocumentSession` per open `DocumentId`.
  Pane state contains a node/view reference only. Asking the other pane for the
  same document focuses the existing pane/session; it never constructs a
  second authoritative buffer.
- `PaneHost` sends its complete text projection on every `TextArea` change. The
  Rust session immediately stores the body, advances its monotonic `Revision`,
  marks a dirty block range, and publishes `(project generation, document ID,
  revision)` to the worker. Temporary invalid Markdown is still authoritative
  and journaled; it gets an actionable canonical-save veto instead of being
  dropped.
- The normal persistence sequence is `Dirty -> Journaling -> Dirty -> Saving
  -> Saved`. Failures enter `Error(message)`. Per-pane labels expose the
  session state and the status bar folds every open session into project-level
  Saved/Journaling/Saving/Unsaved/Save error state.
- Journals are complete revisioned bodies and become due after the 750 ms
  debounce. Focus loss and every destructive transition force them. Canonical
  serialization is frozen on the owner, while bounded reads, backup creation,
  temporary-file writes, and replacement run on the serial project worker.
- Worker jobs and completions carry the full work stamp. Canonical bytes and
  backup bytes are first written to flushed same-directory temporary files.
  The worker rechecks the current stamp while holding the short replacement
  mutex, so a newly published revision linearizes wholly before or after the
  replacement. Stale prepared artifacts are dropped and cannot write,
  acknowledge, clear dirty state, or update the in-memory project body.
- Compile/export first performs the same flush handshake, then freezes
  `compile_input` at the current acknowledged live revision. The offscreen test
  types into the shipping QML `TextArea` without focus loss and proves the text
  survives swap, close/reopen, Markdown export, recovery restore, trash, and
  project close.

## Transition and shutdown state machine

- Swap, pane close/navigation, structural actions including trash, project
  create/open/close, export, and explicit Ctrl/Cmd+S use a backend flush that
  can veto the operation. Journal/canonical error or timeout leaves the editor
  and project open and reports an actionable error.
- Trash collects the complete subtree, flushes first, performs the durable
  canonical trash command, clears any pane references, and removes the related
  sessions and external-conflict records. Trashed nodes cannot remain editable.
- Ordinary destructive transitions wait at most five seconds for canonical
  acknowledgement. Window close calls `prepareQuit`, which waits at most three
  seconds. If canonical I/O does not finish but every current dirty revision is
  journaled, quit may continue with a recovery-safe status. An
  `aboutToQuit` direct hook repeats the bounded attempt for platform quit paths
  that bypass QML, then makes a final journal-only attempt if needed.
- `DocumentLifecycleWorker::drop` never joins an I/O-stalled thread. This
  preserves the bounded shutdown decision; process teardown releases a still
  active worker after recovery safety has been established.

## Recovery, backups, and external-change UX

- Writable project open scans each `.parchmint/recovery/*.toml` independently.
  Newer candidates are presented one at a time in a modal preview with Restore,
  Discard, and Save Copy. Corrupt/unsupported records are isolated and shown
  individually, so one bad record cannot hide valid recovery. Read-only
  fallback deliberately does not consume another live writer's journals.
- Restore places the recovered body into the normal live session, advances the
  revision, and journals/saves it through the same worker. Save Copy uses an
  atomic destination write and then discards only that chosen record.
- Before a canonical replacement, up to ten prior bounded canonical files are
  rotated under `.parchmint/backups/<document-id>/`. Backup sources are read
  through `MAX_DOCUMENT_BYTES`; an oversized source stops the save instead of
  allocating or copying it.
- Open canonical files are polled every two seconds on the project worker.
  Clean sessions auto-reload using the same project style IDs and parse limits
  as initial open. Dirty sessions keep both bodies and present side-by-side
  Reload Disk, Overwrite with Mine, and Save Copy then Reload actions. The
  canonical fingerprint is rechecked again during save, so an edit racing the
  chosen action cannot be silently overwritten.
- `docs/user-guide/backup-recovery.md` now describes only behavior exercised by
  the shipping QML and offscreen integration test.

## Native path normalization

- Project creation uses a native `FolderDialog` for the parent plus a validated
  single-component project name. Project/sample open uses `FolderDialog`;
  attachments, export, diagnostics, recovery copies, and external-conflict
  copies use `FileDialog`. Dialogs seed from Qt's Documents location, the open
  project, or recent projects. No normal workflow asks for a hand-typed path.
- The bridge obtains Documents and home locations from `QStandardPaths`/Qt,
  then applies one Rust boundary: trim; reject empty or NUL; accept a plain
  native path; decode a local `file:` URL exactly once; reject malformed or
  non-local URLs; expand `~`, `~/`, or `~\`; anchor relative input at Documents
  rather than launcher CWD; and lexically normalize `.`/`..`.
- Intent validation runs before project mutation and distinguishes missing
  input/path, regular-file-as-project, inaccessible or non-directory parent,
  malformed URL, unsupported remote URL, missing/non-file attachment, existing
  destination file, and non-empty project directory. Tests retain spaces,
  literal `#`/`%`, percent-encoded Unicode, and launcher-independent relative
  paths.

## Crash-safe project locking

- `.parchmint/open.lock` is opened and held for the lifetime of the writable
  `OpenProject` using `std::fs::File::try_lock`. Rust 1.97 implements this with
  `flock(LOCK_EX|LOCK_NB)` on supported Unix/macOS targets and `LockFileEx` on
  Windows. Drop explicitly unlocks and closing/process death releases the OS
  authority.
- PID, hostname, and timestamp are rewritten only after acquisition and are
  diagnostic metadata. A stale file is harmless and is never interpreted as a
  live owner. There is no destructive lock-breaking UI.
- `WouldBlock` maps to the already-open path and offers a non-mutating read-only
  fallback. Other lock/open errors remain genuine permission/I/O failures. A
  subprocess test proves two live writers are excluded and a killed owner can
  be reopened immediately; a same-process test proves read-only access remains
  available while the writer owns the lock.

## Failure-injection and verification evidence

- Permanent Rust tests cover journal failure before/after replacement,
  canonical failure before backup/write and after write, simulated disk-full
  and permission failures, delayed stale work, the second pre-commit stamp
  check, bounded backup sources, corrupt recovery isolation, restart restore
  and save-copy, clean external reload, all three dirty-conflict choices, and a
  fingerprint race after conflict resolution.
- Workspace integration covers typing without focus loss through journal,
  canonical acknowledgement, compile freeze, swap, close/reopen, trash, and
  restart. The native `parchmint-lifecycle-smoke` test performs the equivalent
  through the shipping QML editor/backend, verifies the export bytes, recreates
  a crash-like canonical-plus-journal state, verifies the recovery dialog is
  visible, restores/flushed it, then trashes and closes.
- `cargo test --workspace --exclude parchmint_bridge --offline`: passed on
  Linux; 81 passed, 3 ignored manual measurements, no failures.
- `cargo clippy -p parchmint-storage -p parchmint-app -p parchmint_bridge
  --all-targets --offline -- -D warnings` with Qt 6.8.3 environment: passed.
- `just build`: passed with Qt 6.8.3. The existing linker-selection build note
  reports GNU ld.bfd because mold/lld/gold is absent; the native link succeeds.
- `ctest --test-dir build --output-on-failure`: passed all four tests, including
  offscreen UI smoke, live lifecycle/recovery smoke, editor adapter, and outline
  model. The lifecycle/recovery path completes in about 0.6 seconds here.
- `cmake --build build --target qmllint`: passed with only the repository's
  pre-existing unqualified delegate-access advisories.
- `cargo fmt --all` and `git diff --check`: passed.

## Known gaps and next-stage prerequisites

- Only `x86_64-unknown-linux-gnu` and Qt 6.8.3 were installed locally. Windows
  `LockFileEx`, macOS `flock`, native-dialog appearance, OS session-manager quit,
  and platform permission/disk-full behavior still require results from the
  existing CI/platform matrix; no result is inferred here.
- Canonical replacement briefly holds the worker stamp mutex across atomic
  backup/canonical renames and directory flushes. This is the correctness
  linearization point; an editor delta can wait briefly for it. Stage 14 should
  measure this on slow/removable filesystems while preserving the no-stale-write
  invariant.
- Open-file change detection is a bounded two-second poll rather than a native
  filesystem watcher. This is deliberate portability for Stage 12; Stage 14
  may add coalesced native notifications with polling retained as fallback.
- `PaneHost` sends a complete body per text change. It establishes the required
  authoritative session boundary but is not the 10M-word incremental transport;
  Stage 14 owns incremental deltas and scale optimization.

Stage 13 can rely on authoritative live sessions, transition vetoes, reachable
recovery/conflict dialogs, native path dialogs, and crash-releasing project
locks. Its first task should preserve the `PaneHost` live-body/flush contract
while replacing the plain text projection with the production editor and then
exercise every new workflow through the existing lifecycle smoke boundary.
