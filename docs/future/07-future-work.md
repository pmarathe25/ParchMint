# ParchMint Future Work

**Status:** Deferred roadmap and extension constraints  
**Version:** 1.2
**Date:** 2026-07-31

## 1. How to use this document

Nothing in this document is v1 scope unless it is promoted through a PRD update, design revision, architecture review, and acceptance-plan change.

The v1 architecture must keep credible extension paths, but implementation agents must not build speculative features or broaden interfaces without a concrete need.

## 2. Editor and workspace

### Recursive split panes

Allow arbitrary left/right/top/bottom editor groups using a binary layout tree. Retain shared `EditorSession` and independent `ViewSession` semantics.

### Top/bottom companion layout

Offer an orientation switch before or independently of recursive splits.

### Group copy and keyboard cut

Extend structured clipboard operations to complete group subtrees.

### Cross-project copy

Map styles, metadata schemas, comments, links, and future assets between projects.

### Quick document switcher and recent navigation

Add fuzzy title/hierarchy navigation and back/forward history.

### Focus and typewriter modes

Hide chrome and maintain active-line positioning without changing document layout/export.

### Visible whitespace and per-view zoom

Render editor-only whitespace marks and independent display scaling.

### Transparent large-document optimization

Optimize ProseMirror rendering internally while preserving the same feature set. Candidate directions include decoration reduction, incremental plugin work, content visibility, block-windowed rendering, or a replacement editor adapter.

A feature-reduced or segmented large-document mode requires a new product decision; it is not implied by this roadmap.

### Alternative GUI/editor

A future frontend may replace Tauri/React/ProseMirror by implementing the same application/editor contracts and canonical format. Reassess Rust-native frameworks only when they can demonstrate one geometry authority, real IME, accessibility, two-view behavior, and large-prose performance.

## 3. Import and Research resources

- Plain-text import.
- Markdown import.
- HTML import.
- DOCX import.
- Bulk manuscript hierarchy import.
- Research note import/conversion.
- Read-only text/Markdown preview.
- Image preview.
- PDF preview.
- Arbitrary attachments.
- Web bookmarks and saved snapshots.
- Copy-into-project and link-in-place policies.
- PDF/image annotations using content-handler-specific anchors.

These features should use `Importer` and `ContentHandler` ports, preview an import plan, and apply as one undoable/history operation.

## 4. Cards and planning

- Multi-column corkboard.
- Saved filters/views.
- Typed metadata such as status, POV, dates, tags, target word count.
- Timeline and relationship views.
- Multiple independent planning arrangements that do not silently alter manuscript order.

## 5. Rich text and review

- Footnotes/endnotes.
- Tables.
- Embedded images with assets, captions, and alt text.
- Arbitrary highlights/colors.
- Per-selection font/size overrides.
- Drop caps.
- Multi-column layout.
- Advanced sections/page numbering/headers/footers.
- Track changes with accept/reject.
- Snapshot-to-snapshot document/project comparisons.
- Review display modes.
- Search comments.
- Project-wide All Comments view.
- Comment authors and asynchronous review exchange.

## 6. Search and analysis

- User-selectable Global Search scopes for Manuscript, Research, both, or a selected subtree.
- Regular-expression search/replace.
- Saved searches.
- Replacement in Synopsis/metadata.
- Structural queries such as missing Synopsis or orphaned comments.
- Language-aware stemming/diacritics options.
- Repetition, sentence, style, and readability analysis.

Search-backend replacement remains possible through `SearchIndex`; current FTS5 indexes are disposable.

## 7. Export and publishing

- Partial export scopes for a selected group/subtree or selected documents.
- Per-group and per-document inclusion overrides with explicit inheritance and preview.
- Generated table of contents.
- In-app export preview.
- DOCX.
- EPUB.
- PDF.
- Markdown, plain text, and LaTeX.
- Submission manuscript templates.
- Print-ready book layout.
- Saved export profiles.
- Cover/front/back matter tooling.

Each new target implements `Exporter` over the neutral export model.

## 8. History, backup, and maintenance

- Partial checkpoint restoration for a selected document, group, or subtree, including scope-specific impact previews.
- Remote backup by pushing the app-managed history.
- Restore project from remote.
- Multiple backup destinations.
- Integrity verification/repair UI.
- History size reporting.
- Optional compaction policies only after a new explicit product decision.

Current requirements deliberately exclude automatic pruning, permanent purge, Duplicate Project, Archive Project, and general Git UI.

A future history backend may replace `git2` behind `HistoryStore`, with logical checkpoint migration and no Git IDs exposed to product code.

## 9. Writing aids

- Smart quotes and language-aware punctuation.
- Automatic em/en dash and ellipsis rules.
- Special-character palette.
- Grammar checking.
- Goals, sessions, and progress analytics.
- Distraction-free session tools.
- AI-assisted writing only after explicit privacy, provenance, offline/network, and user-control decisions.

## 10. Templates

- Project templates.
- Style templates.
- Metadata-field templates.
- Export-profile templates.

Templates must generate new IDs and no copied history.

## 11. Distribution and platform

- Automatic updates.
- App-store distribution.
- Additional CPU architectures.
- Expanded Linux packages, including AppImage only after runtime compatibility is proven.
- Automatic workspace reopening.
- Roaming nonproject preferences.
- Lower production MSRV/toolchain where feasible.

## 12. Collaboration and other clients

- Asynchronous review packages may be considered before real-time collaboration.
- Real-time collaboration would require identity, permissions, presence, CRDT/OT, offline reconciliation, and history integration.
- Mobile/web clients require a separate product and architecture review and must not weaken desktop-first open-file behavior.

## 13. Promotion checklist

Before promoting a feature:

1. Add normative workflows and requirement IDs to the PRD.
2. Update Penpot screens/components/states.
3. Identify canonical format and migration impact.
4. Confirm port/interface changes.
5. Define undo, save, recovery, history, search, export, and accessibility semantics.
6. Add scale/performance budgets.
7. Add cross-platform tests.
8. Update implementation and acceptance plans.
9. Record retained deferrals.

## 14. Features not to infer

Do not infer that ParchMint will add:

- Raw HTML source editing.
- User-facing Git controls.
- Permanent purge/history erasure.
- Automatic history pruning.
- Duplicate Project or Archive Project.
- Real-time collaboration.
- Mobile/web clients.
- AI generation.
- Automatic external rich-text merge.
- A proprietary binary project database as the only source of current content.
