# Design Implementation Work Breakdown

## S50 design-system and shell

| Ownership | Requirements/design inputs | Bounded output | Verification |
|---|---|---|---|
| `packages/design-system` | tokens, icons, PM primitives | deterministic token/icon importer, generated CSS/TS/manifests, semantic primitive components | importer dirty diff, source checksums, Light/Dark component fixtures |
| `desktop-ui/workspace-shell` | PM shell, Explorer, Inspector, status, Appearance | mock-service launcher and all navigable workspace shells; System/Light/Dark propagation | keyboard/focus fixture, 20 reference captures, no hard-coded colors |
| `desktop-ui/hierarchy-cards` | PM Tree/Card family | virtualized hierarchy/cards, visual drag preview and selection only | deep-tree and Cards fixtures; platform modifier contract |
| `desktop-ui/dialog-feedback` | launcher/export/recovery/shared feedback | dialogs, menus, loading/error/recovery presentations against mocks | focus restoration, reduced-motion and live-region fixtures |

S50 must not implement any blocking variant in ISSUE-001 through ISSUE-005, persistent project mutation, native dialog behavior, history/search/spellcheck adapters, or the editor's shared-state mechanism.

## Feature slices

| Slice / owner | Requirement IDs / Penpot state | Dependencies | Test tier and platforms |
|---|---|---|---|
| S55 editor feasibility | EDIT-002, EDIT-006–010; editor-same-document-two-views | S30 contracts, visual EditorPane map | native packaged editor gates on Windows/macOS/Linux; choose shared state/projection only after proof |
| S60 editor foundation | FMT-001–018, TOOL-001–005, PM Editor family | S55 selected mechanism | Tier A/B plus native editor accessibility/performance evidence |
| S65 spellcheck | SPELL-001–006, DictionarySettings, underline/menu | S60 editor contract, S40 preferences | three-platform native spelling/latency; ISSUE-004 disposition required |
| S70 history/deletion | HIST-001–010, DEL-001–007, PM history/deletion family | S40 HistoryStore | Tier A/B, recovery/history fault checks; ISSUE-003 disposition required |
| S80 search | SEARCH-001–014, GlobalSearchPanel/ReplacePreview | S40 SearchIndex | Tier A/B indexing/replace checks; ISSUE-001 disposition required |
| S100 planned feature slices | comments, metadata, styles, export, project actions | S30/S40/S50/S60/S65/S70/S80 as applicable | each production slice has a sealed independent test charter and Light/Dark visual fixtures |

## Theme-sensitive inventory

All PM components are theme-sensitive because they consume semantic roles. Mandatory paired review surfaces are launcher, editor single/dual, Cards, Global Search, History, Appearance settings, Export, recovery dialog, and Recently Deleted. Also test focus, selection, disabled, warning/error, comment anchor, search match, spellcheck underline, save status, menu/dialog elevation, and the fully dark manuscript canvas. There are no approved theme-specific asset variants; icon fill resolves from the active semantic token.

## Open blockers

`open-issues.yaml` ISSUE-001 through ISSUE-005 must be resolved before G10 approval. Their affected later owners must treat the product/architecture authority as controlling until a revised approved handoff or G20 decision exists. ISSUE-006 and ISSUE-007 remain recorded evidence limits and later validation work, not permission to weaken requirements.
