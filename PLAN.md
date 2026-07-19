# ParchMint product and architecture contract

Status: approved for implementation  
Audience: implementation agents and human reviewers
Targets: Windows, macOS, Linux

Implementation is split into ordered agent stages under [`plans/`](plans/00-easy-stage-index.md). The stage plans own task breakdowns, dependencies, model recommendations, file ownership, and acceptance gates. This document contains only decisions and quality requirements shared by more than one stage.

## Document roles

- `PLAN.md` is the product and cross-stage architecture contract.
- [`plans/00-easy-stage-index.md`](plans/00-easy-stage-index.md) defines execution order and agent handoff rules.
- `plans/01` through `plans/09` define stage-specific implementation work.
- `docs/architecture/` records ADRs created during implementation.
- `docs/handoffs/` records evidence and context passed between stage agents.

A stage plan may add detail but must not silently weaken this contract. Changing a locked decision requires an explicit ADR and user approval when it materially changes product scope, data compatibility, licensing, privacy, or platform support.

## Product contract

ParchMint is a fast, native, local-first desktop application for planning, writing, organizing, and exporting novels and other long-form works.

Version 1 must provide:

- Arbitrarily nested manuscript groups and text sections, including Parts, Chapters, Scenes, or user-defined structures.
- Fast reordering and reparenting by drag/drop and keyboard.
- Independent titles and brief synopses for every section.
- Hierarchical outline and compact card views that allow an entire novel to be planned from synopses before body text is written.
- A WYSIWYG editor supporting titles, headings, subheadings, normal paragraphs, bold, italic, superscript, subscript, alignment, lists, links, images, thematic/scene breaks, and page breaks.
- Reusable named paragraph and character styles with stable identity.
- A raw Markdown source mode once lossless WYSIWYG round-tripping is stable.
- Research notes and attachments stored with the project.
- Two symmetric panes so research or another document can remain visible while writing.
- Search across titles, synopses, manuscript, and research.
- Word and character counts for selection, section, subtree, manuscript, research, and project.
- Autosave, crash recovery, rotating backups, and safe external-file-change handling.
- Binder-ordered compilation and export to Markdown, plain text, HTML, PDF, EPUB, and DOCX.
- Light/dark themes, keyboard navigation, Unicode and IME input, high-DPI support, and accessible semantics.
- Responsive use with projects containing at least 10 million words and 10,000 nodes.

Version 1 does not include:

- Browser or mobile clients
- Cloud accounts, synchronization, or real-time collaboration
- A plugin marketplace or public scripting API
- Full desktop-publishing layout or page-accurate live editing
- Track changes or multi-author review
- Screenplay-specific workflows
- Citation management
- Built-in generative AI
- Perfect import of every Markdown, DOCX, HTML, EPUB, or Scrivener construct

Page breaks are semantic compile markers in the editor. They do not imply page-accurate WYSIWYG layout.

## Locked architecture

1. ParchMint is a native application. Do not introduce a browser, WebView, Electron, Tauri, or web frontend.
2. Rust owns the domain model, commands/events, project storage, Markdown codec, indexing, compile pipeline, and application services.
3. Qt 6 owns windowing, platform integration, Qt Quick/QML UI, and the `QTextDocument`-based editing surface.
4. CXX-Qt provides a narrow, typed Rust/Qt boundary.
5. QML never reads or writes project files directly.
6. C++ contains editor and platform adapters, not domain or persistence logic.
7. Domain crates contain no Qt types.
8. File I/O, indexing, serialization, and export run off the UI thread.
9. Background work carries project-generation and document-revision identifiers so stale results cannot overwrite newer state.
10. Qt Markdown conversion is never canonical serialization.
11. SQLite is a disposable search/statistics cache and can be rebuilt entirely from canonical files.
12. The UI uses Qt Quick Controls with a restrained Material-inspired theme and a small shared design-token set.

Expected dependency direction:

```text
QML / Qt Quick UI
        |
Qt editor and platform adapters
        |
CXX-Qt bridge and Qt-facing view models
        |
Rust application services
        |
domain | markdown | storage | index | compile
```

## Domain and dependency contract

- Projects, nodes, documents, styles, assets, and compile presets use stable UUIDs or equivalently stable IDs.
- The node graph is a rooted acyclic forest; every node appears once and has a valid parent or root position.
- Sibling ordering and compile traversal are deterministic.
- Style inheritance is acyclic. Built-in styles always exist and cannot be deleted, though their appearance may be overridden.
- Style references use stable IDs or machine keys, never mutable display names.
- Project paths are relative and cannot escape the project through traversal or symlinks.
- Domain mutations are validated commands that emit explicit events.
- Binder/metadata undo belongs to the Rust command layer; text undo belongs to each open `QTextDocument`.
- UI models consume Rust snapshots/events rather than maintaining a competing mutable project model.
- Each open document owns independent cursor, selection, scroll, and undo state.

## Content and project-format contract

A project is an ordinary directory, not an opaque bundle. Its conceptual layout is:

```text
My Novel/
  parchmint.toml
  outline.toml
  styles.toml
  manuscript/<stable-node-id>.md
  research/<stable-node-id>.md
  assets/<stable-asset-id>-<safe-name>.<ext>
  trash/<stable-node-id>.md
  .parchmint/
    workspace.toml
    index.sqlite
    recovery/
    backups/
```

Canonical, user-authored data consists of the versioned TOML manifests/style definitions, Markdown documents with YAML front matter, assets, and recoverable trash. `.parchmint/` contains local, derived, recovery, or backup state; removing it must not damage the active project or canonical trash.

Titles, synopses, status, labels, tags, and document flags live in Markdown front matter. Hierarchy and order live in `outline.toml`. Do not duplicate titles or synopses in the outline manifest.

ParchMint Markdown 1.0 is:

- CommonMark plus the selected supported GitHub extensions.
- YAML front matter for document metadata.
- Standard Markdown for headings, emphasis, lists, quotes, code, links, images, and thematic breaks.
- `<sup>` and `<sub>` for superscript and subscript.
- A documented Pandoc-compatible attribute subset for stable named character/paragraph styles.
- Fenced-div attributes for paragraph alignment.
- `<!-- parchmint:page-break -->` for page breaks.

Codec invariants:

- Supported content round-trips semantically and deterministically.
- Repeated saves do not churn whitespace or attribute ordering.
- Unknown front-matter keys are preserved.
- Unsupported Markdown blocks are preserved as opaque source nodes until explicitly converted or edited.
- Opaque content is visibly identified and never silently discarded.
- Pasting rich content is sanitized into the supported model; plain-text paste is not interpreted as Markdown automatically.
- The Rust semantic AST, not a Qt HTML/Markdown representation, is the persistence and export boundary.

All schemas and extension syntax must be fully documented under `docs/format/` before format version 1 is frozen.

## User-experience contract

The main window consists of:

- A collapsible left binder for manuscript and research trees.
- A central workspace showing an editor, outline, or cards.
- An optional second pane split horizontally or vertically.
- A collapsible right inspector for synopsis, metadata, notes, and statistics.
- Restrained navigation, formatting, search, and status controls.

Essential operations must be available through keyboard and menus; drag/drop is never the only method. Filtering retains enough ancestor context to explain hierarchy. Research is excluded from manuscript compile by default. Workspace restoration is helpful but never required to open a project.

The UI favors typography, spacing, focus clarity, and subtle elevation over decorative borders. It respects system light/dark preferences, reduced motion, high contrast, and scalable text.

## Data-integrity, security, and privacy contract

- Canonical writes use same-directory temporary files, flush, and atomic replacement where the platform permits it.
- The UI reports “saved” only after canonical replacement succeeds.
- Autosave journals changes before background canonical persistence.
- Crash recovery loses no more than the configured autosave debounce interval.
- Schema migration creates a recoverable backup before canonical mutation.
- External changes never silently overwrite dirty local content.
- Stale async saves, index updates, and exports cannot overwrite newer revisions.
- Trashed documents remain canonical until the user explicitly empties project trash.
- Export failure leaves an existing destination intact.
- Attachment import cannot escape the project, overwrite an existing asset, or execute active content.
- Parsers defend against malformed input, excessive nesting/size, hostile paths, and newer unsupported schema versions.
- ParchMint makes no network request during normal version 1 operation.
- Diagnostics do not transmit data and require explicit user action to export.

## Cross-stage quality gates

Reference hardware is a mainstream 2022-era four-core laptop with SSD and 16 GB RAM. Release targets are:

- Cold launch to usable empty window: at most 1.5 seconds.
- Open a 10,000-node/10-million-word project to usable binder and synopses: at most 3 seconds without waiting for full indexing.
- Structural mutation visible within 100 ms and durably written within 1 second.
- Editor keystroke-to-paint in a 250,000-word section: p95 below 16 ms and p99 below 50 ms.
- Normal formatting visible within 50 ms.
- Autosave work on the UI thread: no continuous block longer than 8 ms.
- Indexed search first results: within 300 ms.
- Binder and outline scrolling: sustained 60 FPS on the stress project.
- Idle stress-project memory with two normal documents open: target below 500 MB, excluding OS file cache.

Quality requirements:

- Windows, macOS, and Linux CI runs formatting, linting, unit/integration tests, golden format tests, and smoke builds.
- Nightly or scheduled CI covers fuzz smoke tests, stress indexing, export validation, and performance trends.
- Domain/format work includes unit, property, golden, malformed-input, and migration tests as applicable.
- Persistence work includes failure injection for termination, full disk, permission changes, and stale work.
- Qt work includes bridge integration and QML/QTest coverage.
- Release validation covers Narrator, VoiceOver, Orca, keyboard-only operation, IME, bidirectional text, high-DPI scaling, and reduced motion.
- User-visible strings are externalized from the beginning, even if version 1 ships only in English.

## Open release decision

The final application/distribution license is undecided. During development, dynamically link Qt components available under acceptable open-source terms, retain notices, and inventory dependencies. Do not statically link Qt or publish release artifacts until an accepted licensing ADR permits it.

## Version 1 definition of done

ParchMint version 1 is complete when a user can, on Windows, macOS, and Linux:

1. Install and launch the native application.
2. Create or open a large human-readable project.
3. Plan a novel by organizing nested sections and editing synopses without opening body text.
4. Reorder the hierarchy without losing content or breaking compile order.
5. Write formatted chapters with reusable styles and lossless Markdown persistence.
6. Keep research visible in a split pane while writing.
7. Search and count manuscript and research content responsively.
8. Close, crash, recover, and handle external edits without silent data loss.
9. Compile selected manuscript content and export every promised version 1 format.
10. Inspect and recover the canonical project without ParchMint or `.parchmint/`.
11. Complete the primary workflows by keyboard and with supported screen readers.
12. Meet the documented security, privacy, performance, packaging, and licensing gates.
