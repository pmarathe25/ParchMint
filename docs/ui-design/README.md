# ParchMint UI design

This is the visual and interaction-design authority for ParchMint v1. Its native
source is `docs/ui-design/parchmint-ui.penpot`. The [product
specification](../product/README.md) defines behavior and scope;
the [architecture](../architecture/architecture.md) defines ownership and
boundaries. These pages define visual language, layout, component composition,
and interaction presentation where those authorities do not conflict.

## Read by feature

Shared rules:

- [Foundations](foundations.md)
- [Workspace shell](workspace-shell.md)
- [Platform conventions](platform-conventions.md)
- [Shared interaction patterns](shared-interaction-patterns.md)
- [Empty, loading, error, and recovery states](empty-loading-error-recovery.md)

Feature rules:

- [Launcher and project creation](launcher-and-project-creation.md)
- [Editor and tabs](editor-and-tabs.md)
- [Explorer, Inspector, and comments](explorer-inspector-and-comments.md)
- [Cards](cards.md)
- [Search and replace](search-and-replace.md)
- [History and Recently Deleted](history-and-recently-deleted.md)
- [Settings and appearance](settings-and-appearance.md)
- [Spellcheck](spellcheck.md)
- [Word counts](word-counts.md)
- [Export and save states](export-and-save-states.md)

The [screen catalog](screen-catalog.md) contains stable component IDs, screen IDs, fixtures, and Penpot mappings. The native source is [parchmint-ui.penpot](parchmint-ui.penpot).

## Penpot source and stable names

Keep the native source pages in this order:

1. `00 Cover & Current Status`
2. `01 Foundations & Theme Tokens`
3. `02 Components`
4. `03 Launcher & Project Creation`
5. `04 Editor Workspace`
6. `05 Cards Workspace`
7. `06 Search & Replace`
8. `07 Comments & Inspector`
9. `08 History & Recently Deleted`
10. `09 Project Settings & Appearance`
11. `10 Export & Save States`
12. `11 Empty Loading Error Recovery States`
13. `12 Accessibility & Keyboard Focus`
14. `13 Cross-Platform Variants`
15. `14 Prototype Flows`
16. `15 Handoff Inventory`

Page 00 summarizes the current design, product version, unresolved blockers,
and approval state. It is not a decision history. Pages 01–02 contain the
design system and shared components. Pages 03–11 contain product screens and
their states. Page 12 contains accessibility and keyboard reference walks,
page 13 platform and layout references, and page 14 prototype flows. Page 15
inventories the handoff. Page 12 is reference material only; it does not add v1
screen-reader or formal assistive-technology requirements. These pages,
reference boards, and flows document the design; they are not product
destinations.

Use stable shared-component names beginning `PM/` and stable screen names
beginning `PM / Screen /`. Shared component mains and their instances stay in
sync. Every state used by a reference screen must be a component variant or a
documented composition of components. One-off screen changes do not define
shared-component behavior.

The screen catalog lists the exact maintained references. Each reference frame
identifies its theme and scale. Launcher, single- and dual-pane Editor, Cards,
Search, History, Settings/Appearance, Export, and at least one error or dialog
state have both Light and Dark references. Platform references cover native
menu, dialog, shortcut, and font-rendering differences without copying every
screen three times.

Page 14 documents these flows:

1. Create a project and write.
2. Open a companion pane and show one document in two views.
3. Organize the project in the tree and Cards.
4. Add, reply to, and resolve a comment.
5. Use Local Find and Global Search/Replace Preview.
6. Restore a whole-project History checkpoint.
7. Delete an item and restore it from Recently Deleted.
8. Change System appearance to Dark and Light across open windows.
9. Use a spelling suggestion and the project and global dictionaries.
10. Export the entire Manuscript.
11. Handle a save failure and recover after a crash.

## Deferred UI scope

Do not add collaboration, AI writing, import, recursive pane splitting, regex
search, source editing, attachment previews, per-document spellcheck language,
additional spellcheck languages, CJK IME, bidirectional or Arabic editing,
screen-reader support, reduced-motion preference integration, formal
assistive-technology validation, aggregate
group/Research/project word counts, or a user-visible large-document mode.
These and all other deferred features remain outside v1.

Keep the native source and these pages consistent in component names, tokens,
screen composition, and interaction presentation.
