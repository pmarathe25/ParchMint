# ParchMint v1 product specification

ParchMint is a local-first desktop application for planning and writing novels on Windows, macOS, and Linux. This specification defines observable v1 behavior and scope.

This specification covers behavior, state, information architecture, data, and acceptance outcomes. The [UI design documentation](../ui-design/README.md) covers visual styling, spacing, iconography, and component composition unless presentation changes workflow, capability, or other observable behavior.

Normative terms:

- **Must / must not:** required for v1.
- **Should / should not:** expected unless the product owner approves an update to this specification.
- **May:** optional behavior that does not change the required experience.
- **Deferred:** explicitly not part of v1.

Requirement IDs are stable. Designs and tests must cite them.

## Product goals

ParchMint v1 must:

1. Let a solo novelist create and manage multiple independent projects.
2. Organize each project into strictly ordered, arbitrarily nested Manuscript and Research hierarchies.
3. Provide a responsive semantic WYSIWYG editor suitable for long-form prose.
4. Provide a Cards view over the same hierarchy and metadata for planning and reordering.
5. Keep canonical authored data in open, inspectable, Git-friendly files.
6. Autosave and retain every completed autosave checkpoint without interrupting typing.
7. Provide comments, metadata, search, word counts, recovery, spellcheck, and one initial export format.
8. Provide usable Light and Dark appearances, including a System-following option.
9. Ship as one coherent first release for Windows, macOS, and Linux.
10. Keep the GUI/editor, history, search, spellcheck, persistence, and export implementations replaceable behind ParchMint-owned contracts.

## Target user

ParchMint v1 serves a **solo novelist** who may work on multiple projects and does not need real-time collaboration. It prioritizes sustained writing, structural planning, predictable data ownership, and low latency over publishing-suite complexity.

## Product principles

1. **Local first:** Core writing, saving, history, search, spellcheck, and export work offline.
2. **Open authored data:** Current content remains readable without ParchMint caches or databases.
3. **No silent data loss:** Save, deletion, restoration, and errors preserve recoverable states.
4. **Semantic formatting:** Named styles represent document roles; direct decoration is limited.
5. **Same behavior at every supported document size:** Internal optimizations may be transparent, but feature availability may not change because a document is large.
6. **Responsive by construction:** Storage, Git, SQLite, spellcheck, export, and project-wide analysis never block the UI thread.
7. **Implementation details stay hidden:** Filenames, Git, object IDs, SQLite, and editor-engine internals are not normal product concepts.
8. **Appearance is not authored content:** Light/Dark/System choices never alter document styles or export output.
9. **Design and implementation remain traceable:** Every major screen and acceptance test maps to requirement IDs.

## Requirement pages

All linked pages are part of this product specification. Read the pages that match the feature being implemented, plus any shared platform, performance, security, flow, or release pages that constrain it.

### Shared product rules

- [v1 scope](scope.md)
- [Platform scope](platform-scope.md)
- [Project model](project-model.md)
- [Canonical project data](canonical-project-data.md)
- [Scale and performance](scale-and-performance.md)
- [Desktop interaction quality](desktop-interaction-quality.md)
- [Privacy and security](privacy-and-security.md)
- [Canonical user flows](canonical-user-flows.md)
- [Release gates](release-gates.md)

### Features

- [Launcher and project creation](launcher-and-project-creation.md)
- [Workspace shell](workspace-shell.md)
- [Appearance](appearance.md)
- [Explorer and hierarchy](explorer-and-hierarchy.md)
- [Editor panes and tabs](editor-panes-and-tabs.md)
- [Rich text and semantic styles](rich-text-and-semantic-styles.md)
- [Formatting toolbar](formatting-toolbar.md)
- [Titles](titles.md)
- [Synopsis and metadata](synopsis-and-metadata.md)
- [Comments and annotations](comments-and-annotations.md)
- [Cards](cards.md)
- [Search and replacement](search-and-replacement.md)
- [Undo and redo](undo-and-redo.md)
- [Save, recovery, and closing](save-recovery-and-closing.md)
- [History and snapshots](history-and-snapshots.md)
- [Deletion and Recently Deleted](deletion-and-recently-deleted.md)
- [Research](research.md)
- [Word counts](word-counts.md)
- [Spellcheck](spellcheck.md)
- [Export](export.md)

[Future work](future-work.md) records non-v1 directions. It does not add v1 scope.

## Related documents

- [Architecture](../architecture/architecture.md)
- [UI design](../ui-design/README.md)
- [Implementation plans](../../plans/README.md)
