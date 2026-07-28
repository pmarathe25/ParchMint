# ParchMint Penpot Design Brief

**Status:** Final design-agent brief  
**Version:** 1.0  
**Date:** 2026-07-28

## 1. Copyable prompt for the design agent

Use the following prompt with Codex connected to the Penpot MCP server:

---

You are designing **ParchMint**, a cross-platform desktop application for solo novelists. Read `01-product-specification.md`, `AGENTS.md`, and `04-design-artifact-handoff-contract.md` completely before modifying Penpot.

Create a coherent, implementation-ready Penpot design system and full v1 desktop UI. ParchMint will be implemented with Tauri, React, and ProseMirror, but the design must describe semantic components, states, tokens, layout, focus, and interaction rather than framework-specific DOM structure.

Do not change product scope. Do not add collaboration, AI writing, import, recursive pane splitting, regex search, source editing, attachment previews, a user-visible large-document mode, or other deferred features. Record unclear product behavior as a design question instead of inventing it.

Design for Windows, macOS, and Linux from the beginning. Use native OS title bars rather than a custom title bar. Keep the core content layout consistent while documenting platform-specific menu labels, shortcut notation, controls, and dialogs.

Build the token system, component library, screens, error/empty/loading states, keyboard focus states, and prototype flows specified below. Everything should derive from tokens and reusable components. Use stable names beginning with `PM/`. Annotate components with requirement IDs from the PRD.

The design should feel calm, editorial, precise, and suitable for long writing sessions. Prioritize prose legibility, hierarchy clarity, low visual noise, obvious focus, and efficient keyboard use. Avoid decorative skeuomorphism, excessive gradients, dense office-suite chrome, and floating controls that obscure prose.

Produce one fully specified default visual theme. Organize tokens so alternate themes can be added later without redesigning components. Do not make a second theme a v1 implementation requirement.

At completion, prepare the full handoff described in `04-design-artifact-handoff-contract.md`, including the `.penpot` file, token JSON, SVG assets, reference images, design manifest, component matrix, interaction specification, decision log, and known deviations.

---

## 2. Design objectives

1. Keep the editor visually dominant.
2. Make project structure understandable at a glance.
3. Let users switch between writing and structural planning without feeling they entered a different application.
4. Make focus and active document unambiguous when the tree, two editor panes, and Inspector show different nodes.
5. Keep comments and metadata available without covering prose.
6. Make history/recovery feel approachable without exposing Git.
7. Preserve large-project usability through virtualized, compact, information-dense list/card designs.
8. Ensure every primary action has a keyboard path and visible focus state.
9. Design one coherent shell that can adapt cleanly to Windows, macOS, and Linux conventions.

## 3. Recommended visual direction

The following is direction, not a mandatory brand lock:

- Calm editorial surfaces with strong text hierarchy.
- Neutral base colors with one restrained accent.
- Comfortable prose measure and generous line-height.
- Subtle borders and elevation used to clarify regions, not decorate them.
- Minimal persistent toolbar chrome.
- A clearly differentiated document canvas and application chrome.
- Iconography should be simple, geometric, and readable at desktop toolbar sizes.
- Do not imitate Word’s ribbon or Scrivener’s visual style directly.

The design agent should propose typography and color tokens, then document the rationale. Body prose typography in the editor is controlled by project document styles and must remain conceptually separate from application UI typography.

## 4. Penpot file organization

Create these pages in order:

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

Use meaningful board names and stable layer names. Avoid names such as `Rectangle 34`, `Group 18`, or `Frame Copy` in approved components.

## 5. Design tokens

At minimum, define token sets for:

### Color

- Application canvas.
- Sidebars and panels.
- Document canvas.
- Elevated surfaces and menus.
- Primary/secondary/disabled text.
- Borders and dividers.
- Accent and accent-hover/pressed.
- Selection, focus ring, search match, active search match.
- Comment highlight and resolved/orphaned states.
- Success/saving/warning/error.
- Destructive actions.

### Typography

- UI body, compact body, labels, headings, tabs, menus, code/path text.
- Line heights and font weights.
- Editor UI typography only; project prose styles are separate.

### Spacing and size

- Spacing scale.
- Panel padding.
- Toolbar height.
- Tab height.
- Tree row heights.
- Card density variants.
- Icon sizes.
- Minimum click targets.
- Sidebar min/default/max widths.
- Inspector min/default/max widths.
- Companion-pane minimum width.
- Corner radius.
- Divider/splitter widths.

### Effects

- Focus ring.
- Menu/dialog shadows.
- Selected/active/inactive elevation where used.
- Motion durations and easing.

### Themes and modes

Define token grouping so platform and density variants can be added. If one default theme is designed, do not create hard-coded colors outside tokens.

## 6. Component library

Use stable component names beginning with `PM/`. Include all states and relevant variants.

### Shell and navigation

- `PM/ModeSwitch`
- `PM/Sidebar`
- `PM/Inspector`
- `PM/Splitter`
- `PM/Toolbar`
- `PM/ToolbarGroup`
- `PM/TabBar`
- `PM/Tab`
- `PM/StatusBar`
- `PM/PanelHeader`
- `PM/Breadcrumb`

### Explorer

- `PM/Tree`
- `PM/TreeRow/Group`
- `PM/TreeRow/Document`
- `PM/TreeRow/Root`
- `PM/Tree/InsertionMarker`
- `PM/Tree/CutState`
- `PM/Tree/MultiSelection`
- `PM/InlineRename`
- `PM/ContextMenu`

### Editor

- `PM/EditorPane`
- `PM/EditorCanvas`
- `PM/FormattingToolbar`
- `PM/StyleSelect`
- `PM/SelectionCommentAffordance`
- `PM/LocalFindBar`
- `PM/SearchMatch`
- `PM/AtomicBreak/Scene`
- `PM/AtomicBreak/Page`
- `PM/EditorLoading`
- `PM/SaveIndicator`

The prose inside the editor should be represented with realistic semantic samples: title, body paragraphs, headings, block quotes, verse, lists, links, inline marks, literal tabs, scene breaks, page breaks, and comments.

### Inspector and comments

- `PM/InspectorSection`
- `PM/SynopsisEditor`
- `PM/MetadataField`
- `PM/CommentThread`
- `PM/CommentMessage`
- `PM/CommentReplyComposer`
- `PM/CommentAnchorState`
- `PM/OrphanedComment`

### Cards

- `PM/Card`
- `PM/Card/Group`
- `PM/Card/Document`
- `PM/Card/DensityCompact`
- `PM/Card/DensitySynopsis`
- `PM/Card/DensityDetailed`
- `PM/Card/InsertionMarker`
- `PM/Card/MetadataValue`

### Search and replacement

- `PM/GlobalSearchPanel`
- `PM/SearchScopeControl`
- `PM/SearchResultGroup`
- `PM/SearchResult`
- `PM/ReplacePreview`
- `PM/ReplacePreviewRow`
- `PM/ReplacementSelectionControl`

### History and deletion

- `PM/HistoryTimeline`
- `PM/HistoryEntry`
- `PM/NamedSnapshot`
- `PM/HistoryPreview`
- `PM/HistoryCompare`
- `PM/RestoreDialog`
- `PM/RecentlyDeletedList`
- `PM/DeletedItem`

### Project/settings/export

- `PM/LauncherProjectCard`
- `PM/NewProjectDialog`
- `PM/SettingsNav`
- `PM/StyleEditor`
- `PM/MetadataDefinitionEditor`
- `PM/ExportDialog`
- `PM/ProgressState`
- `PM/ErrorBanner`
- `PM/RecoveryDialog`

### General controls

Buttons, icon buttons, text fields, multiline fields, select, checkbox, radio, segmented control, disclosure, tooltip, menu, dialog, toast/banner, empty state, loading state, and focus-visible variants.

## 7. Required screens and states

### Launcher

- First launch/no recent projects.
- Recent projects.
- Missing/moved project.
- Locked/already-open project.
- Create Project dialog.
- Open Project native-dialog handoff.

### Editor — single pane

- Default project with Explorer and Inspector open.
- Both sidebars collapsed.
- Explorer collapsed only.
- Inspector collapsed only.
- Long document with comments.
- Local search open.
- Local replace expanded.
- Toolbar collapsed and expanded.

### Editor — companion pane

- Manuscript left, Research right.
- Two Manuscript documents.
- Same document in both panes at different positions.
- Focus in left versus focus in right; Inspector and toolbar target changes.
- Companion final tab closing.

### Explorer states

- Deep nesting.
- Multi-selection.
- Range selection.
- Cut state.
- Dragging multiple nodes.
- Valid/invalid drop targets.
- Cross-section move.
- Inline rename.
- Empty Manuscript or Research root.

### Cards

- Manuscript default.
- Research selected.
- Deep expanded hierarchy.
- Collapsed groups.
- All density levels.
- Editing Synopsis/metadata.
- Drag/reorder/multi-select.

### Comments and Inspector

- Unresolved thread.
- Collapsed replies.
- Expanded replies/composer.
- Resolved thread filter.
- Orphaned comment.
- Group Inspector with no Comments panel.
- Metadata definitions absent versus many fields.

### Search

- Query entry.
- Streaming results.
- No results.
- Scoped subtree.
- Search result navigation.
- Global replacement preview with excluded matches/documents.
- Stale/deleted result notification.

### Save/recovery/history

- Dirty.
- Saving.
- Saved.
- Save error with retry.
- Recovered changes after crash.
- History grouped by session/date.
- Named snapshot.
- Document restore.
- Group restore.
- Project restore.
- Recently Deleted and fallback restore location.

### Settings

- General project settings.
- Styles list/editor and inheritance.
- Delete unused custom style.
- Replace in-use style before deletion.
- Metadata field list/editor/applicability/Card visibility.
- Global and project dictionaries if spellcheck is included.

### Export

- Entire manuscript.
- Selected group.
- Selected documents.
- Inherited include/title/page-break controls.
- Numbering option.
- Successful export actions.
- Export failure.

### Error and edge states

- History unavailable but current project readable.
- Search index rebuilding.
- Project format migration.
- Corrupt canonical file.
- Unsupported pasted image.
- Lost comment anchor.
- Window below recommended size.
- Offline state should look normal because no network is required.

## 8. Layout studies

Design at least:

- 1280 × 720 — minimum supported workspace study.
- 1440 × 900 — primary reference.
- 1920 × 1080 — wide workspace.
- 2560 × 1440 at scaled UI — high-DPI study.

Do not design a separate tablet/mobile layout.

At narrow sizes:

1. Preserve a usable editor width.
2. Allow sidebars to collapse.
3. Collapse Inspector before crushing prose.
4. Keep the focused pane and toolbar controls accessible.
5. Do not convert the application into a mobile drawer/navigation pattern.

Document exact min/default/max panel dimensions in annotations and tokens.

## 9. Cross-platform variants

Create a comparison board for:

- Command notation: Ctrl on Windows/Linux, Command on macOS.
- Native menu placement.
- Native dialog handoff.
- Context menu conventions.
- Scrollbar expectations.
- Window chrome exclusion.
- Default focus-ring behavior.

Use shared app components wherever possible. Do not create three unrelated visual designs.

## 10. Accessibility requirements in the design

Annotate:

- Keyboard order.
- Focus-visible states.
- Accessible names for icon-only controls.
- Tree levels/expanded/selected states.
- Active versus selected distinction.
- Tab semantics and close buttons.
- Search-result counts and streamed updates.
- Save/error announcements.
- Comment anchor/thread relationship.
- Dialog initial focus and destructive confirmation.
- Minimum hit targets.
- Text contrast.
- Reduced-motion alternatives.

Show at least one complete keyboard-focus walkthrough for Editor mode and one for History/Recently Deleted.

## 11. Prototype flows

Create clickable flows for:

1. Create project → initial document → type → Saved.
2. Create nested group/document → drag reorder → cross-section move.
3. Open Research in companion → focus panes → Inspector follows focus.
4. Open same document twice → independent view positions.
5. Select text → add comment → reply → resolve → navigate back.
6. Switch to Cards → edit Synopsis → reorder → return to Editor.
7. Global search → open result → replacement preview → apply.
8. Create named snapshot → delete group → Recently Deleted → restore.
9. Export selected scope.
10. Save failure → retry.

## 12. Handoff quality gate

Before declaring design complete:

- Every required screen/state is present or explicitly marked not applicable.
- Every production component derives from tokens.
- Approved components have stable names.
- No unresolved generic layer names remain in exported production components.
- Requirement IDs are attached to screens/components/interactions.
- Keyboard/focus/error/loading states exist.
- Cross-platform variants are documented.
- The artifact package passes `04-design-artifact-handoff-contract.md`.
- A design decision log records choices that are not directly dictated by the PRD.
