# ParchMint codebase audit — findings, bugs, and remediation

Date: 2026-07-20
Scope: full-stack audit of the working tree at commit `c10d7fa` (all nine stage
plans implemented), against the nine product requirements and PLAN.md, plus a
UX/design review of the shipped main window.

Method: static review of every crate and QML/C++ source, targeted dynamic
reproduction with a scratch harness driving `parchmint_app::ProjectWorkspace`,
full Rust test run (55 tests pass), and `ctest` (3/3 Qt tests pass). Every
critical claim below was reproduced or verified in source.

---

## 1. Executive summary

The **architecture and Rust foundations are genuinely good**: the open
TOML/Markdown project format is real and documented (`docs/format/`), atomic
writes are correct, the domain invariants are enforced, compile ordering is
truly binder-ordered, CI runs on Linux/macOS/Windows, and all 55 Rust tests +
3 Qt tests pass.

However, the **running application is far from the PLAN.md contract**. A large
fraction of the best machinery — the WYSIWYG editor adapter, the formatting
toolbar, the style manager, the source editor, autosave, crash recovery,
rotating backups, external-change detection, drag/drop reordering, trash
restore, the outline/cards planning views, and subtree word counts — is
**implemented and unit-tested but never wired into the shipping UI**. The app
that actually runs is a plain-text Markdown editor with a binder, an
inspector, and hand-typed path fields.

Three clusters of issues dominate:

1. **Data loss (critical).** The editor saves only on focus loss; swap panes,
   close project, and quit silently discard unsaved text; the same document in
   both panes overwrites itself; external edits are silently clobbered. The
   tested autosave/journal/recovery/backup layer in `document.rs` is dead code.
2. **Format corruption (critical).** The Markdown serializer drops code-block
   closing fences and emits alignment divs its own parser rejects, so editing
   those blocks corrupts documents on save. EPUB images are packaged at a path
   the XHTML never resolves. PDF export is a Latin-1 plain-text fallback.
3. **Scale (critical).** Every structural command clones and revalidates the
   whole project, then rewrites **every** document file on disk — on the UI
   thread. The 10k-node/10M-word target is unachievable by orders of magnitude.

---

## 2. Requirements compliance matrix

| # | Requirement | Verdict | Notes |
|---|---|---|---|
| 1 | Native desktop app, not web | ✅ **Met** | Qt 6 Quick + CXX-Qt, no WebView/browser. `qt_import_plugins` even excludes network/TLS plugins. |
| 2 | Fast; handles large volumes of text | ❌ **Not met** | Full-project clone+validate+rewrite per command (§4.4); per-keystroke full-text FFI word counts; no incremental saves; parser has no size/nesting limits. Editor benchmark exists only for the unused `EditorAdapter`. |
| 3 | Arbitrary sections + easy reordering | ⚠️ **Partial** | Nesting, indent/outdent, move up/down work via menus/keyboard. **Drag/drop does not exist** despite a complete backend (`moveNode` is unreachable; `OutlineModel` has no drag flags/mime; no QML `DropArea`). PLAN requires drag/drop *and* keyboard. |
| 4 | WYSIWYG editor w/ formatting + custom styles | ❌ **Not met in the running app** | `EditorAdapter`, `FormattingBar`, `StylePicker`, `StyleManager`, `SourceEditor`, `MarkdownHighlighter` are **never instantiated** by any loaded QML. The only editor is a `TextArea` with `TextEdit.PlainText` (PaneHost.qml). Domain/style infrastructure exists underneath. |
| 5 | Open format storage | ✅ **Met (format) / ⚠️ codec buggy** | Plain directory of TOML + Markdown, fully documented. But the Markdown codec has round-trip corruption bugs (§4.2). |
| 6 | Synopses + high-level summary view | ⚠️ **Partial** | Synopses editable in Inspector/OutlineView. **Outline and Cards views are unreachable**: no view-switching command/UI exists; `PaneView::Cards` is never assigned anywhere; outline appears only when selecting a built-in root or after closing a pane. Word-count column is a hardcoded "—". |
| 7 | Clean, modern UI (Material-inspired) | ⚠️ **Partial** | Material style is applied and DesignTokens exist, but the token set is two colors + spacing; heavy teal header/footer, unicode-glyph icons, boxed editor, fake controls ("Include in compile" is decorative). See §6. |
| 8 | Research notes + split panel reference | ✅ **Mostly met** | Research tree, attachments with safe preview, two symmetric panes, pin/swap/focus all work. Gaps: attachment import uses a typed path field (no file dialog); same-document-in-both-panes data loss (§4.1). |
| 9 | Cross-platform (Win/macOS/Linux) | ✅ **Met (infra)** | CI matrix builds/tests on all three OSes; packaging dirs for all three; desktop file, macOS bundle, Windows executable properties present. Not yet validated end-to-end on real Windows/macOS hardware by this audit. |

---

## 3. The reported bug: project/file creation fails with permission errors

Reproduced with a scratch binary calling `ProjectWorkspace::create/open` exactly
as the bridge does. Root causes, in order of likelihood for the reporter:

1. **All paths are hand-typed into raw `TextField`s; there is no file/folder
   dialog anywhere in the app** (verified: zero `FileDialog`/`FolderDialog`
   in `app/`). Users type relative paths or `~/…`.
2. **Relative paths resolve against the process CWD.** Installed via
   `packaging/linux/org.parchmint.ParchMint.desktop` (`Exec=parchmint %F`, no
   `Path=`), the CWD is whatever the launcher uses — frequently `/`.
   `create_dir_all("MyNovel")` from `/` → **"could not create project
   directory: Permission denied (os error 13)"** (reproduced).
3. **`~` is never expanded.** `create("~/MyNovel")` from `/` → permission
   denied; from a writable CWD it silently creates a **literal directory named
   `~`** containing the project (reproduced — created `/home/pranav/~/MyNovel`
   during testing).
4. **Stale lock file after a crash.** `AdvisoryLock` is a plain file
   (`.parchmint/open.lock`) created with `create_new(true)`. If the process
   dies, the file survives and every subsequent open fails with "project is
   already open for writing" (reproduced by `touch`ing the lock). There is no
   PID check and no real OS lock (flock/fcntl/LockFileEx), which would
   auto-release on process death.
5. Minor: opening a regular file as a project reports the confusing "could not
   create project directory: Not a directory"; empty path and nonexistent path
   produce raw `io::Error` strings; `file://` URLs (what Qt dialogs return)
   are not parsed at all.

**Fixes (§7 P0):** add `FolderDialog`/`FileDialog` for project open/create,
attachment import, export destination, and diagnostics; normalize paths in the
bridge (expand `~`, make relative paths absolute against
`QStandardPaths::DocumentsLocation`, strip `file://`, reject empty); replace
the lock file with an OS advisory lock (or store PID/hostname/timestamp and
offer "stale lock — break it?" recovery); validate before touching disk and
surface friendly errors.

---

## 4. Bug findings by severity

### 4.1 Critical — data loss in the running app

| ID | Bug | Evidence |
|---|---|---|
| D1 | **Unsaved editor text is silently discarded.** `PaneHost.qml` saves only on focus loss (`onActiveFocusChanged`). *Swap panes* (menu doesn't steal focus) triggers `reloadBody()` and overwrites both buffers; *Close Project* and app quit drop the workspace without flushing; *Export* compiles from the workspace, so current typing is silently excluded. | PaneHost.qml:28-33, 130-135; backend.rs:919-943 (`close_project`); main.cpp (no `aboutToQuit`). |
| D2 | **Autosave, crash recovery, rotating backups, external-change detection are implemented, tested — and 100% unwired.** `document.rs` (~1350 lines: `JournalRequest`, `RecoveryStore`, `CanonicalSaveRequest`, `poll_external_change`, fault injection) is referenced only by its own tests. A crash loses everything since the last focus loss. `docs/user-guide/backup-recovery.md` documents this unshipped behavior as if live. | grep: all 62 references inside document.rs; backend.rs imports none. |
| D3 | **Same document open in both panes overwrites itself.** No same-node guard in `open_node_in_pane`; each pane keeps a private buffer; stale pane's focus-loss save wins. | workspace.rs:453+; PaneHost.qml `loadedNode` guard. |
| D4 | **External edits silently overwritten on save.** `save_document_body` does `set_body` + `save_document` with no on-disk fingerprint comparison (the fingerprint machinery sits unused in document.rs). | workspace.rs:717-731. |
| D5 | **Markdown serializer corrupts documents on edit.** `CodeBlock` serialization **omits the closing fence** (markdown lib.rs:1346-1352 — verified in source and by agent reproduction): after an edit, the code block and *everything after it* re-parse as one opaque block. Alignment divs serialize as `:::{...}` with no space, which the parser itself rejects → block degrades to paragraph + opaque error. Both paths untested (tests only edit paragraphs). | markdown lib.rs:1346, 1400 vs 394. |
| D6 | **Escape asymmetry doubles backslashes every save.** Serializer escapes (`escape_text`), parser never unescapes. `\*` → `\\*` → `\\\\*` … on each edit-save cycle; same defect for link titles, attribute values, and link destinations (`my file.png` → `my%20file.png` → re-parses as a *different* destination). Also `split_trailing_attributes` strips any trailing `{x}` from plain text (data loss), and `1 < 2 > 0` makes a paragraph permanently opaque. | markdown lib.rs:865, 1508, 1519, 842-863; all agent-reproduced. |
| D7 | **EPUB images are always broken.** Body XHTML lives at `OEBPS/text/book.xhtml` and references `assets/<name>` → resolves to `OEBPS/text/assets/…`, but files are packaged at `OEBPS/assets/…`. Must be `../assets/<name>`. (Verified in source: compile lib.rs:1853, 2137, 2164, 2180.) Also: nav anchors mismatch whenever headings sit inside alignment blocks; every EPUB image emits a spurious "copy beside the HTML file" warning. | compile lib.rs; §4.5. |
| D8 | **No parser input limits.** No size cap, no nesting cap, recursive descent — 100k-deep `<sup>` nesting did not finish in 60 s; deeper input risks stack overflow, and release builds use `panic = "abort"`, so any panic kills the app. PLAN explicitly requires these defenses. | markdown lib.rs parse_delimited; Cargo.toml profile. |

### 4.2 Major — correctness

- **Restore-then-undo is broken** (domain): undo of `Restore` to a non-original
  parent errors with "node is absent from parent" — undo-stack corruption on a
  supported path (domain lib.rs:754-779).
- **`RelativeProjectPath` backslash traversal on Unix**: validation runs before
  `\`→`/` normalization, so `..\..\escape.md` validates then becomes
  `../../escape.md` (domain lib.rs:65-77). Contract violation.
- **Every command cancels a running export** — including binder selection
  clicks (`bump()` → `cancel_export`, backend.rs:1372). The revision-stamp
  check at completion already made this unnecessary.
- **Stale exports still write the destination file**: the stamp is checked only
  to discard the *status*, after `atomic_write` has replaced the file.
- **Re-exporting to the same path always fails**: the UI only ever uses
  `CollisionPolicy::Fail`; `ReplaceFile` is unreachable.
- **DOCX defects**: all ordered lists share `numId=1` (numbering continues
  across lists); nested inline formatting dropped (bold-inside-italic loses the
  outer); non-PNG/JPEG images declared `application/octet-stream`; double
  `<w:pStyle>` when a heading also has a paragraph style.
- **PDF export is a Latin-1 plain-text fallback**: all non-ASCII becomes `?`
  (one warning *per line*, unbounded, before dedup — millions for CJK); hard
  coded 90-char wrap and 11/14 pt type. The styled Qt PDF renderer
  (`pdf_renderer.cpp`) is compiled but never declared/called from Rust — dead
  code.
- **Markdown export escaping holes**: `]` in link labels unescaped; titles and
  attribute values emitted with literal `\"`; code fences don't guard against
  ```` ``` ```` in content.
- **Emphasis parser corrupts nesting** (`*a **b** c*` → three flat nodes);
  nested lists flatten permanently on edit; list continuation lines split the
  list; reference links become literal text; double-backtick code spans broken.
- **Front-matter fragility**: CRLF front matter not recognized (silently
  becomes body); empty FM and EOF-without-trailing-newline are hard errors —
  common benign files become unopenable.
- **Trashed node stays editable in its pane**; a later focus-loss save silently
  relocates the file into `trash/`.
- **Editor adapter landmines** (zero production impact today, all must be fixed
  before wiring it up): scene-break insertion leaks the protected object format
  into the following paragraph (exports as `thematic_break` forever);
  page-break/opaque insertion leaks the protected char format into typed text;
  consecutive list items round-trip as single-item lists (numbering restarts);
  underline is silently dropped on save; paste can delete protected objects
  that formatting refuses to touch; HTML paste can embed remote images (QTextDocument
  fetches them — awkward for a no-network app).
- **`pane_attachment_url`** only percent-encodes spaces; `#`, `%`, non-ASCII
  break the URL.
- **Zip writer**: sizes saturate at `u32::MAX` with no ZIP64 — silent
  corruption above 4 GiB.

### 4.3 Major — missing/unreachable features

| Feature | State |
|---|---|
| WYSIWYG editing, formatting toolbar, style picker/manager, Markdown source mode | Fully implemented backend + QML components, **never instantiated**. The product edits raw Markdown in a plain `TextArea`. |
| Drag/drop reorder | Backend complete (`moveNode`, `DropPlacement`); model has no drag/drop overrides; zero QML drag wiring. Unreachable. |
| Outline / Cards planning views | Components exist; **no command, menu, or control switches pane views**; `PaneView::Cards` is never assigned in Rust. CardsView additionally wastes its first cell on the hidden root (`visible: !isRoot` still occupies a GridView slot). |
| Trash browse / restore / empty | `restoreNode` invokable exists; no trash view, no restore UI, no empty-trash — despite the trash dialog promising recovery "until you explicitly empty it". |
| "Include in compile" | Inspector checkbox is decorative: hardcoded `checked: true`, no handler, no backend property. |
| Word counts (section/subtree/manuscript/research/project) | Index totals implemented; **no invokable exposes them**; outline "Words" column is a hardcoded "—". |
| Compile preset selection | UI silently uses `compile_presets().first()`; no picker. |
| Compile progress | `compile_with_progress` exists; the worker calls plain `compile` — no progress for large compiles. |
| Tags/notes in inspector | Placeholder label: "Tags and notes will be available here." |

### 4.4 Critical — performance vs the 10k-node / 10M-word target

1. **Every structural command rewrites the entire project on disk.**
   `execute_persisted` → `ProjectStorage::save` rewrites all manifests **and
   loops over every document file** with an atomic write each (storage
   lib.rs:391-394; no dirty check). A rename or synopsis commit on the stress
   project = 10k file rewrites, on the UI thread.
2. **Every command clones and revalidates the whole project.**
   `Project::execute` = full `clone()` + full `validate()` (domain
   lib.rs:572). Measured by the audit agent: clean O(n²) — ~5.5 s at 3,000
   sequential creates, extrapolating to ~60 s at 10k nodes. `validate()` itself
   is O(nodes × depth) with per-node `BTreeSet` allocations; a 50k-deep tree
   did not finish in 120 s.
3. **Word counts ship the entire document across the FFI on every keystroke**
   (`onTextChanged` → `textStatistics(text)` rescans char-by-char) — dead at
   250k words, catastrophic at 10M.
4. **First search = synchronous full index rebuild on the UI thread**; the
   bridge never uses the worker-based `rebuild_search_index`. The 300 ms
   first-results gate is unachievable cold at scale.
5. **Whole-model reset per interaction**: every command/selection rebuilds the
   full `BinderSnapshot` and `OutlineModel` does `beginResetModel`, then
   re-fetches each visible row role-by-role through `invokeMethod`.
6. Whole-buffer compile pipeline (~4-6× peak memory at 60-80 MB of text);
   bitwise CRC32 (~8× slower than table-based); cancellation only checked
   between documents — a single huge render cannot be interrupted.
7. Unbounded recursion in `walk_active`, `append_rows`, `clone_subtree`,
   `copy_subtree_bodies` — deep trees overflow the stack even during
   *validation*, so saves themselves fail.

### 4.5 Minor

- One-way `text:` bindings on editable Inspector/OutlineView fields: after the
  first edit the binding breaks; the field then shows stale values for other
  nodes and can write stale text into the **wrong node** (InspectorPane.qml:16,
  OutlineView.qml:41-44).
- Pin toggle binding breaks on click; shows wrong state when the backend
  rejects pinning an empty pane.
- `save_status`/`status` are never displayed; no unsaved indicator exists.
- Markdown validation pass runs a full pulldown-cmark parse purely to discard
  the result (2× parse cost).
- `RecoveryStore::scan` aborts on one corrupt record; backup source read
  ignores `MAX_DOCUMENT_BYTES`; auto-reload reparses with wrong options
  (latent, unwired).
- Stale tombstone sibling index can strand a trashed node until siblings are
  restored first.
- Export TOCTOU: existence check precedes render; a file appearing in between
  is clobbered despite `Fail` policy.
- Onboarding "sample project" path field has the same no-dialog/no-normalization
  problems as project creation.

---

## 5. UX improvement opportunities

In priority order (P0 items are the ones the user called out):

1. **File pickers everywhere (P0).** `FolderDialog` for open/create project
   (create = pick parent folder + name field), `FileDialog` for attachment
   import, `FileDialog` (save mode) with per-format extension for export
   destination, for diagnostics export, and for the onboarding sample project.
   Convert `file://` URLs to local paths in the bridge.
2. **Path normalization (P0).** Expand `~`, absolutize relative paths against
   the Documents folder, trim whitespace, reject empties with friendly text —
   before any disk access.
3. **Welcome screen (P1).** Replace the modal-only onboarding with a proper
   start view: New / Open / sample project + clickable recent-projects list
   (recent projects already persist in `Settings` but are buried in a menu
   dialog). This is what Scrivener/VSCode/Obsidian all do.
4. **Autosave + visible save state (P0).** Wire `document.rs`: debounced
   journal, focus-loss flush, flush before *any* destructive action (swap,
   close pane/project, quit), crash-recovery prompt on open, rotating backups,
   external-change conflict dialog. Show "Unsaved/Saved" per pane in the
   status bar.
5. **Wire the WYSIWYG editor (P0).** Instantiate `EditorAdapter` +
   `FormattingBar` + `StylePicker` + `StyleManager` + `SourceEditor` in
   `PaneHost` (after fixing the §4.2 adapter landmines), keeping the raw
   Markdown mode as a toggle.
6. **Drag/drop reorder (P1).** Add drag flags/mime/`moveRows` to
   `OutlineModel` and `Drag`/`DropArea` wiring in the binder (the backend
   `moveNode` is ready). Keep keyboard/menu methods.
7. **View switcher (P1).** Segmented Editor/Outline/Cards control in each pane
   header + commands `view.editor/outline/cards` in the catalog; assign
   `PaneView::Cards` (currently never set). This unlocks the required
   "plan the whole novel from synopses" workflow.
8. **Real trash flow (P1).** Trash section in the binder with restore/empty;
   wire `restoreNode`.
9. **Real "Include in compile" (P1).** Bind to document metadata with an
   invokable, reflect in compile (the pipeline already honors the flag).
10. **Word counts (P2).** Expose `search_totals` via an invokable; show per-row
    counts in OutlineView (replace the "—"); project totals in the status bar;
    compute selection counts incrementally instead of full-text FFI per
    keystroke.
11. **Export dialog (P2).** Compile-preset picker, overwrite confirmation
    (`ReplaceFile`), progress reporting, "reveal in file manager" on success;
    don't cancel exports on unrelated commands.
12. **Recent-file safety niceties (P2).** Reopen last project on launch
    (optional), warn when opening a project already locked, and offer stale-lock
    recovery.
13. **Keyboard polish (P2).** F2 rename in binder, Enter to open, arrows to
    navigate tree, Ctrl+S explicit flush (even if autosaved — users expect it).

---

## 6. Design / theme review (from the shipped UI)

What the screenshot shows, and how to move it toward the VSCode/Zed/Obsidian
class of "sleek":

**Observed issues**

- **Accent color is doing too much.** The teal `#2e7d6e` fills the entire
  header toolbar *and* the footer status bar. Modern dark UIs keep chrome
  neutral (near-black surfaces) and reserve the accent for primary actions,
  focus rings, links, and selection. The current look reads "2012 Android
  Holo", not 2026 desktop.
- **Search/filter fields in the header** have low-contrast placeholder text on
  the teal fill and no icons; the filter field belongs to the binder, not the
  global header.
- **Unicode-glyph buttons** (`☰ ⓘ ⌕ × ● ○ ↗`) render inconsistently across
  platforms/fonts (risk of tofu boxes on Windows) and have inconsistent visual
  weight. Use bundled SVG icons with a consistent 16-18 px grid.
- **The editor sits in a harsh bright-bordered box** floating on a tinted
  background. Writing apps should be full-bleed: no border, centered
  ~68-76ch measure, generous line height (1.5-1.6), calm placeholder.
- **The pane header** shows a cryptic "○ editor" (pin glyph + view name) that
  looks like a disabled tab; close/find buttons are tiny and unlabeled
  visually.
- **The binder** is a flat text list: no Manuscript/Research section headers,
  no expand/collapse chevrons, no type icons, no status/label affordances;
  depth is conveyed by padding alone. A permanent empty first cell exists in
  CardsView.
- **The inspector** stacks label+field pairs with uneven density, a decorative
  checkbox, a placeholder sentence ("Tags and notes will be available here."),
  and a huge mobile-style gray pill for "Move to Trash" — the most visually
  dominant control in the panel is a destructive, rarely-used action.
- **Footer** duplicates the teal fill and shows status strings but never the
  save state; the "%1 visible · %1 selected" readout is low-value.
- **Left screen edge shows a column of stray glyphs** in the capture — verify
  whether this is a rendering/compositing artifact of the window or background
  bleed-through (could not confirm from code; worth a manual check on X11 and
  Wayland).
- **No density/spacing scale in practice**: `DesignTokens` defines only 2
  colors + 5 spacings + 2 radii, and most components hardcode pixel sizes
  (11/20/22 px) and paddings instead of using tokens.

**Recommended direction**

1. **Neutral chrome, accent for intent.** Dark theme: base `#17181a`,
   surface `#1e2023`, raised `#26292d`, hairline `rgba(255,255,255,.08)`;
   header/footer become thin neutral bars. Keep the teal (it's a good brand
   hue) for primary buttons, focus rings, active view switcher, links.
2. **Expand DesignTokens** into a real scale: surface/surfaceRaised/overlay,
   onSurface/onSurfaceMuted, outline, accent/accentMuted/danger, plus type
   scale (caption/label/body/title/headline) and use them everywhere — delete
   hardcoded pixel sizes.
3. **Editor as the hero.** Full-bleed page, centered max-width text column,
   1.55 line height, 17-18 px body (scaled), subtle current-line highlight,
   optional typewriter mode later. Placeholder: dim, italic, centered.
4. **Binder as a real tree.** "MANUSCRIPT" / "RESEARCH" group headers,
   chevrons, document/folder icons, hover + selected states (accent-tinted
   8% fill + 2 px accent leading bar), status dot / label chip at row right,
   inline rename (F2), filter field moved into the binder header with a
   magnifier icon.
5. **Pane header as a tab bar.** Document title (not the view name) as the
   tab label, view switcher segmented control (Editor | Outline | Cards) at
   right, pin/close as proper icon buttons with tooltips.
6. **Inspector as metadata card.** Grouped sections (Synopsis / Metadata /
   Statistics), caption-style labels, label & status as combo chips, synopsis
   in a calm bordered multiline field, live word counts, danger action demoted
   to a small text button at the bottom ("Move to Trash", red on hover).
7. **Status bar.** 22-24 px neutral strip: save state (● Unsaved / Saved),
   project word count, selection count, export progress; remove teal fill.
8. **Elevation & radius.** One consistent radius (8 px panes / 6 px controls),
   1 px hairlines instead of `palette.mid` rectangles at 35% opacity, dialogs
   with a soft shadow and scrim.
9. **Motion.** 120-160 ms fade/slide on pane split and dialog open (respecting
   reduced-motion) — subtle, but it's the difference between "tool" and
   "product".

---

## 7. Prioritized remediation plan

**P0 — stop losing/corrupting user data (week 1-2):**

1. Flush editor buffers before swap/close/quit; add debounced autosave by
   wiring `document.rs` (journal → canonical save → backup rotation); recovery
   prompt on project open; external-change fingerprint check on save.
2. Fix the Markdown serializer: code-block closing fence, `::: ` alignment
   spacing; add edit-then-reparse tests for **every** block type; make the
   inline parser escape-aware (text/titles/attributes/destinations); gate
   trailing-attribute stripping on valid attributes; fix `<` comparison false
   opaqueness; CRLF/EOF/empty front-matter handling.
3. File dialogs + path normalization (fixes the reported permission errors);
   OS-backed project lock with stale-lock recovery.
4. EPUB `../assets/` image path fix + nav anchor alignment; add packaging
   tests that open the produced EPUB and resolve every `src`/`href`.

**P1 — ship the features that exist but are unreachable (week 3-5):**

5. Instantiate the WYSIWYG editor stack in `PaneHost` (fix adapter landmines
   first: format leaks after break insertion, list round-trip, underline,
   protected-object paste).
6. Drag/drop reorder; view switcher (Editor/Outline/Cards); trash view +
   restore/empty; real "Include in compile"; preset picker + export overwrite.
7. Same-document-in-two-panes guard (or shared buffer model); fix one-way
   bindings in Inspector/OutlineView; stop canceling exports on every command;
   stale export must not write the destination.

**P2 — meet the scale contract (week 6-8):**

8. Incremental persistence: dirty-tracking so `save()` only writes changed
   documents/manifests; apply-validate-in-place (rollback on failure) instead
   of full-project clone; make validation depth-iterative with shared visited
   sets.
9. Move index rebuilds and searches off the UI thread; incremental word
   counting; expose subtree totals; avoid full-model resets (delta updates).
10. Parser size/nesting caps + iterative inline parsing; streaming renderers
    and table-based CRC32; cancellable render loops; ZIP64.

**P3 — polish (ongoing):**

11. Design-token expansion and the §6 visual refresh; SVG icon set; welcome
    screen.
12. Test gaps: plain-text export, EPUB packaging/nav, DOCX XML well-formedness,
    QML harness (qmltest), bridge-level integration tests, fuzz/property tests
    for markdown + paths, automated perf gates (the ignored stress tests should
    run in nightly with assertions).
13. Fix `docs/user-guide/backup-recovery.md` to match shipped behavior (or
    ship the behavior it describes — preferred).

---

## 8. What is genuinely solid (keep)

- Open, documented, human-inspectable project format (`docs/format/`) with
  versioned manifests, unknown-key preservation, and canonical atomic writes.
- Domain invariants: acyclic forest, deterministic order, stable IDs, built-in
  style protection, acyclic style inheritance — all enforced and tested.
- Compile: true binder-ordered DFS, selection/research inclusion rules,
  separators, provenance-tracked generated headings; real EPUB/DOCX containers
  with structural self-validation; collision-safe writes.
- Security posture: path-escape and symlink defenses in storage (modulo the
  backslash hole), attachment size/type limits, HTML href scheme allowlist,
  no network in normal operation, diagnostics contain no content/paths.
- CI on all three OSes with fmt/clippy/tests/qmllint/ctest + cargo-deny;
  nightly fuzz smoke and benchmarks.
- Tested document-lifecycle design (journal-before-save, stale-stamp rejection)
  — it only needs to be wired in.
