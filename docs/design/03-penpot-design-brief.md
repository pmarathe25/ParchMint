# ParchMint Penpot Design Brief

**Status:** Current operational design brief
**Version:** 2.3
**Date:** 2026-07-31

## 1. Authority and scope

Use this brief to create or revise the ParchMint v1 Penpot design.

Before changing Penpot, read:

1. `AGENTS.md`
2. `01-product-specification.md`
3. `04-design-artifact-handoff-contract.md`
4. The latest `design-manifest.yaml`

The product specification controls behavior. This brief controls visual language, layout, component composition, and interaction presentation where it does not conflict with the product specification. Record genuine ambiguity as a product question rather than resolving it silently.

Do not add deferred features such as collaboration, AI writing, import, recursive pane splitting, regex search, source editing, attachment previews, or a user-visible large-document mode.

Use stable `PM/` names and requirement IDs. Update shared component mains and their instances instead of patching individual screens. Keep only components and screens that demonstrate a required visual, interaction, accessibility, error, loading, or recovery state.

The design and handoff inventory may be prepared before approval. Do not export the final handoff package until the product owner explicitly approves the design.

## 2. Design character

ParchMint is a calm, editorial desktop application for sustained writing.

- Keep prose visually dominant and application chrome quiet.
- Use Material Design as the reference for interaction states and icon metaphors while retaining compact native-desktop density.
- Prefer flat surfaces, restrained tonal state layers, 4 px corners, and minimal elevation.
- Use one neutral palette with a restrained mint accent.
- Distinguish application UI typography from project-controlled prose styles.
- Use spacing, hierarchy, selection, and familiar icons before adding explanatory text.
- Retain text for authored content, field labels, counts, unfamiliar actions, destructive actions, menus, and confirmations.
- Make interactive, editable, selected, focused, disabled, and read-only states recognizable without relying on hover or color alone. `[WS-010, A11Y-002, A11Y-004]`

## 3. Penpot file structure

Keep these pages in this order:

1. `00 Cover & Decisions`
2. `01 Foundations & Tokens`
3. `02 Components`
4. `03 Launcher & Project Creation`
5. `04 Editor Workspace`
6. `05 Cards Workspace`
7. `06 Search & Replace`
8. `07 Comments & Inspector`
9. `08 History & Recently Deleted`
10. `09 Project Settings`
11. `10 Export & Save States`
12. `11 Empty Loading Error Recovery States`
13. `12 Accessibility & Keyboard Focus`
14. `13 Cross-Platform Variants`
15. `14 Prototype Flows`
16. `15 Handoff Inventory`

Use stable screen names beginning with `PM / Screen /` and meaningful layer names. The Cover records current design status, unresolved product questions, and decisions not dictated by the PRD. Historical rationale belongs in the decision log, not in this brief.

## 4. Foundations and tokens

Create one fully specified default theme. Organize tokens so future themes, platforms, and densities can be added without changing component contracts.

### Color

Define tokens for:

- Application, sidebar, Inspector, document, elevated, and read-only surfaces.
- Primary, secondary, disabled, inverse, path/code, and placeholder text.
- Borders, short section separators, splitters, and focus rings.
- Accent states, selection, focused and unfocused tabs, search matches, comments, save states, warnings, errors, and destructive actions.

### Typography

Define UI body, compact body, label, heading, tab, menu, path/code, and status styles with explicit sizes, weights, and line heights. Project prose typography remains separate.

### Spacing and geometry

Define:

- A compact spacing scale and symmetric control padding.
- Ribbon, toolbar, tab, status-bar, tree-row, menu-row, and card dimensions.
- 20 px core icons in 32–36 px controls.
- Minimum pointer targets and focus-ring offsets.
- Sidebar, Inspector, companion-pane, and splitter limits.
- 4 px default corner radius plus explicit menu/dialog exceptions.

### Effects and motion

Define focus, selected, pressed, menu, dialog, tooltip, and error effects. Honor reduced-motion preferences and avoid nonessential movement.

Do not hard-code values outside tokens in approved components.

## 5. Shared design concepts

### 5.1 Workspace shell

- Use one consistent top ribbon across all project destinations. Its order is Editor, Cards, History, Recently Deleted, Export, and Settings. Global Search is a sidebar panel, not a ribbon destination. `[WS-001, WS-003, WS-012]`
- Render the destinations as one mutually exclusive selector. Inactive controls blend into the ribbon; the current destination uses a restrained mint state layer and indicator without a hard outline.
- Use familiar icon-only controls for ribbon destinations. Recently Deleted uses the shared trash-can icon.
- Place the workspace body immediately below the 52 px ribbon and extend it to the unchanged 32 px status bar.
- Explorer occupies the left column, the working surface the middle column, and Inspector the right column. The formatting toolbar spans only editor panes; Explorer and Inspector extend to the ribbon. `[WS-002–006]`
- Do not outline the focused editor pane. Communicate focus through tab-strip state.
- Keep the Explorer visibility control at the left of the status bar and the Inspector visibility control at the far right under the Inspector. Give each control the same selected mint state treatment as other pressed icon buttons while its pane is visible; remove that treatment when the pane is collapsed. Place contextual document History in the status bar, not in a tab or at the end of the tab strip. `[WS-013, EDIT-011]`
- Project Settings, Export, and ordinary empty/loading/error content use the available main pane with centered outer margins. Use nested containers only for true admonitions, dialogs, or confirmations.

### 5.2 Controls, icons, and text

- Use symmetric horizontal and vertical padding. Icon-button bounds must not retain space from removed labels.
- Use one Material-aligned monochrome icon family with consistent optical size, stroke weight, and vertical centering.
- Use the same trash-can icon for Recently Deleted and every destructive row action.
- Every icon-only control has an accessible name and tooltip. `[A11Y-001–004]`
- Inactive icon buttons blend into their surface; selected, pressed, focused, or destructive states provide the visible control treatment.
- Editable values use outlined text, multiline, select, checkbox, or radio controls. Plain text must not imply editability.
- Placeholder text is grey and italic. Multiline fields wrap and grow or scroll intentionally; text must not clip.
- Read-only information uses neutral styling that is visibly distinct from editable controls.

### 5.3 Disclosure sections and lists

`PM/InspectorSection` is the shared disclosure-section pattern for:

- Inspector sections.
- Manuscript and Research Explorer roots.
- Grouped Search results.
- Comparable Cards groups.

The pattern consists only of the disclosure row, necessary content, and a short bottom separator. It has no outer fill or outline, no top separator, and no bottom separator on the final item in a stack. Center the disclosure icon with its label and keep rows compact, non-overlapping, and unclipped. `[WS-015, TREE-001, SEARCH-010]`

### 5.4 Menus and contextual actions

- Every right-click menu composes the shared `PM/ContextMenuSurface`, `PM/ContextMenuItem`, and `PM/ContextMenuDivider` primitives. Menu families own only action order, labels, icon-path overrides, and intentional hover, disabled, or destructive state. Do not duplicate surface, row, label, padding, radius, elevation, or divider styling in Editor, Explorer, spelling, or future context menus.
- Put a relevant icon before each menu label.
- Use minimal symmetric padding while preserving pointer targets.
- Menus always render above workspace content.
- Use direct manipulation and contextual menus instead of permanent explanatory chrome where the action remains discoverable.

## 6. Feature presentation rules

### 6.1 Editor, formatting, and tabs

- Keep one formatting toolbar above the editor panes and target the focused view. `[TOOL-001–004]`
- The toolbar contains style selection, styled `B`, `I`, `U`, and `S` glyphs, a split list control, block quote, one chain-link icon, Scene Break, and Page Break. It does not contain Add Comment. `[TOOL-005, CMT-007]`
- The list control applies bullets from its main action and exposes other list types from its arrow.
- Every primary and companion pane shows a tab strip whenever it contains at least one tab, including a one-tab Research companion. `[EDIT-001–005, EDIT-012]`
- Tabs are 32 px high. Reserve a fixed trailing region for the close control; the title region must never extend beneath or into it. Long titles render the longest possible prefix followed by an ellipsis inside the remaining title region.
- Only the active tab in the focused pane uses mint. The active tab in an unfocused pane uses muted text and a neutral selected treatment; inactive tabs do not use mint.
- Preserve preferred tab widths while they fit. On overflow, shrink every visible tab to the same width. The minimum is 58 px and preserves the first title character, ellipsis, and close control. The accessible name and tooltip retain the full title. `[EDIT-012]`
- Local Find appears directly below the focused pane's tabs. Its close control is right-aligned.
- Local Replace is collapsed initially behind a Show Replace icon. When expanded, that icon uses the selected mint treatment and reveals a replacement field and Replace action. Matches appear directly on editor text, not as result cards. `[SEARCH-001–005]`
- Comment creation remains in the editor context menu and Comments panel. Selecting text must not create a floating selection-end affordance. Comment anchors are actual text highlights, never explanatory placeholder labels. `[CMT-005–008]`

### 6.2 Explorer and Inspector

- Use compact, non-overlapping tree rows without checkboxes or an active-document rail.
- Reveal every pane's active document in Explorer. Use the stronger selected treatment for the focused pane's document and a quieter treatment for another pane's active document. `[TREE-019]`
- Manuscript and Research are independent collapsible instances of the shared section pattern.
- The Explorer context menu covers applicable create, open, open-in-companion, rename, copy, cut, and delete actions. `[TREE-020]`
- Place the Global Search icon button at the right edge of the Explorer header row, vertically centered with the `EXPLORER` label. Activating it replaces Explorer with Global Search; it is not part of the top ribbon. `[WS-003, WS-012, SEARCH-006]`
- The document Inspector always contains collapsible Synopsis, Metadata, and Comments sections.
- Synopsis and every metadata value use separate outlined editable fields. Multiline Synopsis wraps.
- Metadata field order follows Project Settings.
- Comments shows actual threads or the text `No comments`; group Inspectors omit Comments. `[WS-004, WS-014, CMT-005]`

### 6.3 Comments

- Present all comment threads in one continuous Comments list; do not add separate `Unresolved` or `Resolved` section headings.
- Thread headers show an explicit per-thread Unresolved or Resolved state using a compact status treatment derived from shared control tokens rather than an ad hoc label.
- Unresolved threads expose a shared-button-style Resolve action; resolved threads expose the same button treatment labeled Reopen. Match standard button height, padding, radius, text weight, focus, and disabled behavior. The action remains available when replies are collapsed. `[CMT-002–003]`
- `PM/CommentRepliesToggle` owns the shared disclosure and label used to open or close replies. Keep its arrow inside the control bounds, disable content clipping, and leave at least 12 px between the anchor preview and the toggle.
- Expanded threads render replies chronologically above an in-thread reply field and Send action.
- Orphaned comments use a centered remediation admonition and remain navigable from Inspector. `[CMT-011–012]`

### 6.4 Cards

- Cards is a compact vertical projection of the complete hierarchy. Use indentation for nesting and preserve the full list when groups expand or collapse. `[CARD-001–005, CARD-008]`
- Group and document cards display title and Synopsis. Place Synopsis in the main left area and configured visible metadata in the available right area.
- Metadata values are read-only in Cards and use short separators instead of chips, boxes, or colored tiles. Edit metadata in Inspector. `[CARD-006, META-005]`
- Do not show an implicit `Status: Draft`. A Status value appears only when the project defines and exposes that metadata field. `[CARD-010]`
- Card bodies and disclosures both expand or collapse groups. Use a clear insertion marker for drag destinations.

### 6.5 Search and replacement

- Global Search occupies the left pane and retains an explicit return to Explorer. `[SEARCH-006]`
- Do not show a search-scope dropdown or other scope selector in v1. Global Search always covers the entire project; scoped search is future work. `[SEARCH-007]`
- Use a query field plus case-sensitive and whole-word icon toggles. An optional second field and Replace action start global replacement review.
- Group results by document using the shared disclosure pattern. Separate document headings from result rows and use inline prefix, highlighted match, and suffix elements so highlights never cover adjacent text.
- Activating a result focuses the relevant editor view, scrolls to the match, and highlights it. `[SEARCH-010, SEARCH-014]`
- No-results preserves the current editor context and offers query recovery without a scope control.
- Global Replace Preview fills the middle workspace pane while Search remains left and Inspector remains right. It is never a popup, modal, or floating card. `[SEARCH-011–013]`
- Mirror the file-tree hierarchy and provide consistent selection controls for groups, documents, and matches, including indeterminate parent states.

### 6.6 History and Recently Deleted

- History uses a list/detail layout. Selecting an entry directly updates a read-only, line-aligned side-by-side checkpoint/current comparison.
- Highlight word-level changes inside changed lines. Do not add Preview, Compare, or per-entry leading icons. Restore buttons use text only. `[HIST-007]`
- Restore confirmation has no scope selector. It identifies the selected checkpoint, explains that the entire project will be restored, and uses a single `Restore checkpoint` action. Partial checkpoint restoration is future work. `[HIST-007]`
- Recently Deleted reuses the list/detail structure but never shows a diff. The detail pane shows one formatted, read-only deleted-content preview plus separate restore-location information. `[DEL-004–005]`

### 6.7 Settings, spellcheck, and export

- Settings uses the full main pane with shared list/detail editors.
- Custom style rows show one trailing trash action; reserved styles show `Reserved` in the same trailing column and no trash action. Do not label custom styles `Custom`. `[FMT-004–005, FMT-019]`
- Metadata rows show a drag handle and trailing trash action. Their list order is display order. `[META-004, META-011]`
- Dictionary settings use discrete language, dictionary, add-word, and remove-word controls.
- Spellcheck decorates the misspelled word in place and anchors the spelling menu to it. Do not use a detached explanation card. `[SPELL-001–004]`
- Export uses the full main pane with a fixed Entire Manuscript summary, title/page-break controls, numbering, destination, progress, success, and failure states. Do not show partial-scope or inclusion controls, and do not place the interface inside a second bordered card. `[EXP-001–010]`

### 6.8 Empty, loading, error, and recovery states

- Use the full available application content region with centered outer margins.
- Preserve the normal shell, pane context, and tab strip where the state occurs inside an editor.
- Use nested surfaces only for warnings, recovery choices, destructive confirmation, or remediation.
- Center admonitions and size them so text and actions are never clipped.
- ParchMint is offline by design; do not create an offline warning or dialog.

## 7. Component library

The live Penpot library is the exhaustive source for component names and IDs; the approved handoff component matrix must mirror it. The required stable families are:

| Area | Stable components |
|---|---|
| Shell | `PM/WorkspaceTopBar`, `PM/Sidebar`, `PM/Inspector`, `PM/Splitter`, `PM/StatusBar` |
| Explorer | `PM/Tree`, `PM/TreeRow/Root`, `PM/TreeRow/Group`, `PM/TreeRow/Document`, `PM/Tree/InsertionMarker`, `PM/Tree/CutState`, `PM/Tree/MultiSelection`, `PM/InlineRename`, `PM/ContextMenu` |
| Editor | `PM/EditorPane`, `PM/EditorPaneHeader`, `PM/Tab`, `PM/EditorCanvas`, `PM/FormattingToolbar`, `PM/StyleSelect`, `PM/LocalFindBar`, `PM/SearchMatch`, `PM/EditorContextMenu`, `PM/SpellcheckUnderline`, `PM/SpellingContextMenu`, `PM/AtomicBreak/Scene`, `PM/AtomicBreak/Page` |
| Inspector/comments | `PM/InspectorSection`, `PM/SynopsisEditor`, `PM/MetadataField`, `PM/CommentThread`, `PM/CommentMessage`, `PM/CommentReplyComposer`, `PM/CommentRepliesToggle`, `PM/CommentAnchorState`, `PM/OrphanedComment` |
| Cards | `PM/Card/Group`, `PM/Card/Document`, `PM/Card/InsertionMarker`, `PM/Card/MetadataValue` |
| Search | `PM/GlobalSearchPanel`, `PM/SearchResultGroup`, `PM/SearchResult`, `PM/ReplacePreview`, `PM/ReplacePreviewRow`, `PM/ReplacementSelectionControl` |
| History/deletion | `PM/HistoryTimeline`, `PM/HistoryEntry`, `PM/NamedSnapshot`, `PM/HistoryPreview`, `PM/HistoryCompare`, `PM/RestoreDialog`, `PM/RecentlyDeletedList`, `PM/DeletedItem`, `PM/DeletedItemPreview` |
| Settings/export/recovery | `PM/LauncherProjectCard`, `PM/NewProjectDialog`, `PM/SettingsNav`, `PM/StyleEditor`, `PM/MetadataDefinitionEditor`, `PM/DictionarySettings`, `PM/ExportDialog`, `PM/ProgressState`, `PM/ErrorBanner`, `PM/RecoveryDialog` |
| Controls | `PM/Button`, `PM/TextField`, `PM/MultilineField`, `PM/Select`, `PM/Checkbox`, `PM/Disclosure`, `PM/ContextMenuSurface`, `PM/ContextMenuItem`, `PM/ContextMenuDivider`, `PM/Tooltip`, `PM/Toast`, `PM/EmptyState`, `PM/LoadingState`, `PM/FocusVisible` |

Library rules:

- All production instances derive from tokens.
- Repeated shell and feature blocks use shared components.
- Do not recreate generic parallel owners for component-specific menus, dialogs, toolbars, tab bars, banners, or ribbon selectors.
- A zero-reference component is deleted only after checking whether a required host screen is missing.
- Do not retain unused screen-local copies after their shared component is updated.

## 8. Required reference coverage

The live screen inventory remains authoritative. At minimum, preserve these distinct reference categories:

| Page | Required coverage |
|---|---|
| 03 Launcher | First launch, recent projects, missing project, locked project, New Project, native Open handoff |
| 04 Editor | Sidebar combinations, persistent toolbar, one and many tabs, equal-shrink overflow, long-title truncation, local Find/Replace, editor context menu, semantic content, companion focus, same document twice, companion closed |
| 05 Cards | Manuscript, Research, deep/collapsed hierarchy, density variants, drag/multi-select, Synopsis and metadata presentation |
| 06 Search | Whole-project query, streaming, no results, highlighted grouped results, middle-pane replacement preview, stale result |
| 07 Comments/Inspector | Unresolved, resolved, collapsed replies, expanded replies/composer, orphaned, no Comments, no metadata, many metadata fields |
| 08 History/Deleted | History list and word-level comparison, named snapshot, whole-checkpoint restore confirmation, Recently Deleted formatted preview and fallback location |
| 09 Settings | General, Styles, style replacement, Metadata fields, Dictionary, spellcheck settings |
| 10 Export/Save | Entire-Manuscript export, title/page-break behavior, numbering, progress, success, failure, save status |
| 11 Recovery states | History unavailable, index rebuild, migration, corrupt file, pasted image, lost anchor, document loading, save/recovery errors, minimum-window behavior |
| 12 Accessibility | Focus order, semantic annotations, contrast/targets/motion, keyboard walkthrough |
| 13 Platform/layout | 1280 × 720, 1440 × 900, 1920 × 1080, scaled 2560 × 1440, Windows, macOS, Linux conventions |
| 14 Flows | The ten prototype flows below |
| 15 Handoff | Current completion, questions, gaps, and re-export inventory |

Do not retain separate static screens for behavior that produces no distinct rendered state.

## 9. Layout and platform studies

Design and annotate:

- 1280 × 720 — minimum supported window.
- 1440 × 900 — primary reference.
- 1920 × 1080 — wide workspace.
- 2560 × 1440 with scaled UI — high-DPI reference.

At narrow widths, preserve editor usability, allow sidebars to collapse, collapse Inspector before crushing prose, and keep the focused pane and toolbar operable. Do not introduce a tablet/mobile layout. `[WS-011, A11Y-005]`

Document Windows/Linux Ctrl versus macOS Command notation, native menu placement, native dialog handoff, context-menu conventions, scrollbars, focus rings, and native title-bar exclusion. Use shared app components rather than three unrelated designs. `[PLAT-001–003]`

## 10. Accessibility annotations

Annotate:

- Keyboard order and complete Editor plus History/Recently Deleted focus walkthroughs.
- Focus-visible, selected, active, expanded, dirty, disabled, and error states.
- Accessible names and tooltips for icon-only controls.
- Tree levels, tablist/tab semantics, full tab titles, and named close controls.
- Search result counts, streamed updates, match state, and replace-preview region.
- Save/error announcements.
- Comment anchor/thread relationship, resolution state, and Resolve/Reopen actions.
- Dialog initial focus, Escape/cancel behavior, and destructive consequences.
- Minimum targets, contrast, reduced motion, and 100–200% scaling behavior.

Native assistive-technology validation remains an implementation/release requirement, not a Penpot pass. `[A11Y-001–008]`

## 11. Prototype flows

Maintain clickable flows for:

1. Create project → initial document → type → Saved.
2. Create hierarchy → reorder → cross-section move.
3. Open Research in companion → switch focus → Inspector and toolbar follow.
4. Open the same document twice → preserve independent view positions.
5. Select text → use editor context menu to add comment → reply → Resolve → Reopen/navigate.
6. Switch to Cards → edit Synopsis → reorder → return to Editor.
7. Global Search → open result → middle-pane replacement preview → apply.
8. Create snapshot → delete group → Recently Deleted → restore.
9. Export entire Manuscript.
10. Save failure → retry.

## 12. Quality and handoff gate

Before requesting product-owner approval:

- Every required reference state is present or marked not applicable with a requirement-based explanation.
- Components and screens retain stable `PM/` names and requirement metadata.
- Repeated blocks are shared component instances and no required component has zero references.
- No generic layer names, clipped content, accidental overlap, or hidden obsolete controls remain.
- Keyboard, focus, empty, loading, error, recovery, and platform variants are documented.
- The decision log contains only choices not already dictated by the PRD or this brief.
- The draft handoff inventory identifies re-export scope, unresolved product questions, implementation-only validation gaps, and known deviations.
- The final package satisfies `04-design-artifact-handoff-contract.md`.

Do not export the final handoff package until explicit product-owner approval.
