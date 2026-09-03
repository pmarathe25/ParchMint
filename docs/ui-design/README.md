# ParchMint UI design

These pages describe ParchMint's visual language, layout, components, and
interaction patterns. The editable design source is
[parchmint-ui.penpot](parchmint-ui.penpot). The [user guide](../user-guide.md)
explains the application from an author's perspective, and the
[architecture](../architecture/architecture.md) explains implementation
ownership and boundaries.

## Read by feature

Shared rules:

- [Foundations](foundations.md)
- [Platform conventions](platform-conventions.md)
- [Shared interaction patterns](shared-interaction-patterns.md)

Surfaces:

- [Workspace shell](surfaces/workspace-shell.md)
- [Empty, loading, error, and recovery states](surfaces/empty-loading-error-recovery.md)
- [Launcher and project creation](surfaces/launcher-and-project-creation.md)
- [Editor and tabs](surfaces/editor-and-tabs.md)
- [Explorer, Inspector, and comments](surfaces/explorer-inspector-and-comments.md)
- [Cards](surfaces/cards.md)
- [Search and replace](surfaces/search-and-replace.md)
- [History and Recently Deleted](surfaces/history-and-recently-deleted.md)
- [Settings and appearance](surfaces/settings-and-appearance.md)
- [Spellcheck](surfaces/spellcheck.md)
- [Word counts](surfaces/word-counts.md)
- [Export and save states](surfaces/export-and-save-states.md)

The [screen catalog](screen-catalog.md) maps stable component and screen IDs to
Penpot objects and visual-test fixtures.

## Design source

The Penpot file contains the theme tokens, shared components, product screens,
keyboard and platform references, and prototype flows. Production code derives
typed tokens and icons from that file. Checked-in image references under
`tests/parchmint-ui-verification/references/` capture the screens used by visual
tests.

Shared-component names begin `PM/`; screen names begin `PM / Screen /`. A
reference screen composes shared components and their documented variants.
Light and Dark references use the same structure and semantic roles.
