# Stage 03: editor document lifecycle

Difficulty: very hard  
Recommended model: GPT-5.6 Sol, high or max reasoning  
Cost-conscious fallback: GPT-5.6 Terra, max reasoning with frequent focused verification  
Depends on: stages 1–2  
Master references: `PLAN.md` “Content and project-format contract,” “Domain and dependency contract,” “Data-integrity, security, and privacy contract,” and “Cross-stage quality gates”

## Outcome

Deliver the production WYSIWYG editor and the complete safe lifecycle of an open document: canonical Markdown load, semantic editing, styles, undo, autosave, crash recovery, external-change handling, and optional raw source mode. This stage owns the highest data-loss and fidelity risks and must close them before broader UI work proceeds.

## Primary ownership

- Production portions of `crates/parchmint-markdown`
- Document services in `crates/parchmint-app`
- Editor-facing portions of `crates/parchmint-bridge`
- Qt editor adapter under `app/cpp`
- Editor, formatting, style, and source-mode QML components
- Markdown, recovery, clipboard, IME, and editor performance fixtures

## Required work

### 1. Semantic Markdown codec

- Implement the ParchMint semantic AST selected in stage 1 with explicit supported and opaque nodes.
- Parse CommonMark/GitHub constructs, YAML front matter, stable style attributes, alignment divs, super/subscript, and page-break markers.
- Serialize supported nodes deterministically and preserve unknown YAML keys.
- Preserve unsupported blocks as source-backed opaque nodes until explicitly converted.
- Define behavior for unsupported inline syntax, malformed extensions, duplicate IDs, missing style IDs, and invalid front matter.
- Add source spans and diagnostics suitable for raw source mode.

### 2. Production WYSIWYG adapter

- Load semantic blocks into the chosen Qt editor host using semantic Qt format properties, not visual inference alone.
- Implement titles/headings/subheadings, normal paragraphs, bold, italic, super/subscript, alignment, lists, links, images, thematic/scene breaks, named styles, and page breaks.
- Represent opaque blocks and page breaks as protected, visible editor objects with predictable cursor/delete behavior.
- Track selection state, mixed formatting, current paragraph style, and focus-aware command availability.
- Implement keyboard shortcuts, grouped formatting undo, clipboard normalization, and rich/plain paste choices.
- Keep each open document's cursor, selection, scroll, and undo state independent.
- Emit revisioned, incremental dirty information; never serialize the entire document on every keystroke.

### 3. Styles

- Implement built-in and user-defined character/paragraph styles, inheritance, stable IDs, display-name changes, validation, deletion/replacement, and next-style behavior.
- Provide a style manager and a compact style picker with preview.
- Distinguish semantic styles from direct formatting and provide “clear direct formatting.”
- Ensure style renames do not rewrite every document and missing styles degrade visibly but safely.
- Ensure exporters can later resolve a fully computed style without Qt.

### 4. Autosave and recovery

- Maintain document revisions and dirty ranges/blocks.
- Journal edits after the approved debounce and on focus loss before scheduling canonical save.
- Perform canonical serialization and atomic writes off the UI thread.
- Associate every async save with project generation and document revision; discard stale completion.
- Report saving/saved/error states accurately.
- Flush on clean shutdown and compact fulfilled recovery records.
- Detect newer recovery records on startup and support preview, restore, discard, and save-copy.
- Add configurable rotating backups and guaranteed pre-migration snapshots consistent with stage 2.

### 5. External changes and source mode

- Watch open canonical documents for external modification using fingerprints/revisions, not timestamps alone.
- Auto-reload only if there are no local changes.
- For conflicts, offer compare, reload, overwrite, and save-copy without a silent default.
- Implement raw Markdown mode with syntax highlighting and parse diagnostics.
- Preserve the raw buffer if it cannot be parsed; require explicit resolution before returning to WYSIWYG.
- Define undo boundaries when switching modes and when resolving an external change.

### 6. Verification

- Golden round-trip tests for every supported block/inline construct and combinations of them.
- Property tests for repeated parse/serialize cycles and randomized style rename/inheritance operations.
- Failure injection at each journal/canonical-write boundary, including process termination, full disk, permission changes, and stale background saves.
- Clipboard tests for plain text, HTML, and platform rich formats.
- Unicode, bidirectional, grapheme, dead-key, and IME charters on Windows, macOS, and Linux.
- Performance tests for load, typing, formatting, save scheduling, full save, and two-pane simultaneous documents.

## Acceptance gate

- Supported content survives repeated WYSIWYG and raw-source round trips without silent semantic loss.
- Opaque content and unknown front matter survive unless the user explicitly replaces them.
- No acknowledged save is lost under failure injection, and recovery loses no more than the configured debounce interval.
- Dirty local content is never silently overwritten by an external change or stale async save.
- Editor keystroke, formatting, and UI-thread autosave work meet the master budgets on the reference corpus.
- Undo/redo, clipboard, Unicode, IME, styles, page breaks, and links pass the cross-platform test matrix.
- The editor can be used in an isolated harness without the final binder UI.

## Out of scope

- Final binder and outline UX
- Research attachment previews
- Full-text project search
- Compile/export
- Final visual polish and installers

## Handoff

Create `docs/handoffs/03-editor-document-lifecycle.md`. Include the supported Markdown matrix, opaque-node behavior, editor host API, threading/revision rules, recovery format, known Qt/platform quirks, and measured editor/save latency.
