# ParchMint Penpot Design Brief

**Status:** Current operational design brief; final handoff pending  
**Version:** 2.4  
**Date:** 2026-07-31

## 1. Authority and scope

Use this brief to create or remediate the ParchMint v1 Penpot design.

Read first:

1. `AGENTS.md`
2. `docs/product/product-specification.md`
3. `delivery/design-handoff-contract.md`
4. The latest draft or approved `design-manifest.yaml`

The product specification controls behavior. This brief controls visual language, layout, component composition, and interaction presentation where it does not conflict with the product specification. Record unresolved product conflicts in the handoff's `known-deviations.md`; do not resolve them silently or maintain a separate historical decision log.

Do not add deferred features such as collaboration, AI writing, import, recursive pane splitting, regex search, source editing, attachment previews, per-document spellcheck language, aggregate group/Research/project word counts, or a user-visible large-document mode.

The final handoff must include the remediated Light and Dark designs and the Appearance setting. Do not mark the handoff approved until both themes, required states, and checksum inventory are complete.

## 2. Design character

ParchMint is a calm, editorial desktop application for sustained writing.

- Keep prose dominant and application chrome quiet.
- Use Material Design as a reference for interaction states and icon metaphors while retaining compact desktop density.
- Prefer flat surfaces, restrained tonal state layers, 4 px corners, and minimal elevation.
- Use charcoal neutrals and a restrained mint accent.
- Separate application UI typography from project-controlled prose styles.
- Use spacing, hierarchy, selection, and familiar icons before explanatory text.
- Keep text for authored content, field labels, counts, unfamiliar/destructive actions, menus, and confirmations.
- Make interactive, editable, selected, focused, disabled, warning, error, and read-only states recognizable without relying on hover or color alone.

## 3. Penpot file structure

Keep these pages in this order:

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

The Cover shows current design status, product-spec version, handoff version, unresolved blockers, and approval state. It is not a historical decisions page.

Use stable screen names beginning with `PM / Screen /` and stable shared component names beginning with `PM/`. Update component mains and instances rather than patching individual boards.

## 4. Light/Dark token system

Create one semantic token model with two complete value sets: Light and Dark. The application resolves System to one of those sets at runtime.

### 4.1 Required semantic color roles

- Application, sidebar, Inspector, document/manuscript, elevated, menu, dialog, read-only, and overlay surfaces.
- Primary, secondary, disabled, inverse, path/code, placeholder, and link text.
- Borders, section separators, splitters, focus rings, and scrims.
- Accent default/hover/pressed/selected states.
- Focused and unfocused tabs/panes.
- Search matches and active matches.
- Comment anchors/statuses.
- Save dirty/saving/saved/error.
- Warning, error, destructive, and success states.
- Selection in focused and unfocused contexts.
- Spellcheck underline and spelling-menu states.

Dark appearance must use fully dark application and manuscript surfaces. Do not retain a light document sheet inside dark chrome.

### 4.2 Typography

Define UI body, compact body, label, heading, tab, menu, path/code, and status styles with explicit size, weight, and line height. Project prose typography remains separate and is not replaced by application appearance tokens.

### 4.3 Spacing and geometry

Define:

- Compact spacing scale and symmetric control padding.
- 52 px workspace ribbon and 32 px status bar.
- Toolbar, tab, tree-row, menu-row, and card dimensions.
- 20 px core icons in 32–36 px controls.
- Minimum pointer targets and focus-ring offsets.
- Sidebar, Inspector, companion-pane, and splitter limits.
- 4 px default radius with explicit menu/dialog exceptions.

### 4.4 Effects and motion

Define focus, selected, pressed, menu, dialog, tooltip, and error effects. Honor reduced motion. Avoid nonessential movement.

### 4.5 Token discipline

- Production components must bind semantic tokens; do not hard-code theme-dependent values.
- Light and Dark components share structure, variants, and interaction states.
- Token names describe purpose rather than color value.
- Reference frames must identify theme and scale.
- Appearance changes never alter project prose style definitions or export output.

## 5. Shared design concepts

### 5.1 Workspace shell

- Use one top ribbon across project destinations in this order: Editor, Cards, History, Recently Deleted, Export, Settings. Global Search is a sidebar panel, not a ribbon destination.
- Render destinations as one mutually exclusive selector. The current destination uses a restrained mint state layer/indicator without a hard outline.
- Use familiar icon-only controls with accessible names/tooltips; Recently Deleted uses the shared trash icon.
- Workspace body begins below the 52 px ribbon and ends above the 32 px status bar.
- Explorer is left, working surface middle, Inspector right. The formatting toolbar spans editor panes only.
- Do not outline the focused editor pane. Communicate focus through tab-strip state and accessible state.
- Status bar: Explorer visibility at left; Inspector visibility at far right; each uses selected mint treatment while shown. Contextual document History belongs in the status bar.
- Settings, Export, and ordinary empty/loading/error states use the available main pane with centered outer margins; nested surfaces are reserved for true admonitions, dialogs, or confirmations.

### 5.2 Controls and icons

- Use symmetric padding. Icon-button bounds must not retain label space.
- Use one Material-aligned monochrome icon family with consistent optical size/stroke.
- Use the same trash icon for Recently Deleted and destructive row actions.
- Icon-only controls have accessible names and tooltips.
- Editable values use explicit text, multiline, select, checkbox, or radio controls. Plain text must not imply editability.
- Placeholder text is grey and italic. Multiline fields wrap and grow/scroll intentionally.
- Read-only information is visibly distinct from editable controls in both themes.

### 5.3 Disclosure sections and lists

`PM/InspectorSection` is the shared disclosure pattern for Inspector sections, Manuscript/Research roots, grouped Search results, and comparable Cards groups.

It contains only the disclosure row, necessary content, and a short bottom separator. It has no outer fill/outline, no top separator, and no bottom separator on the final item. Center disclosure icons and prevent clipping/overlap.

### 5.4 Context menus

All context menus compose shared surface/item/divider primitives. Menu families own only action order, labels, icons, and intentional states. Use relevant icons, compact symmetric padding, correct elevation, and guaranteed overlay ordering.

## 6. Feature presentation

### 6.1 Editor, formatting, and tabs

- One always-visible formatting toolbar targets the focused view.
- Toolbar: style select; styled B/I/U/S glyphs; split list control; block quote; link; Scene Break; Page Break. No Add Comment.
- Every populated primary/companion pane shows a tab strip, including a one-tab Research companion.
- Tabs are 32 px high with a fixed close region. Long titles show the longest possible prefix plus ellipsis without crossing the close region.
- Only the active tab in the focused pane uses mint. The active tab in the unfocused pane uses neutral selected treatment.
- On overflow, visible tabs shrink uniformly to a minimum preserving first character, ellipsis, and close control. Full title remains in accessible name/tooltip.
- Local Find appears below focused-pane tabs. Local Replace begins collapsed behind a selected-state toggle.
- Search matches appear on text, not cards.
- Comment creation remains in the editor context menu and Comments panel. Anchors are actual highlights, never explanatory placeholders.
- Show spellcheck as an in-place underline and anchor its spelling menu to the word.

### 6.2 Explorer and Inspector

- Use compact non-overlapping tree rows without checkboxes or an active-document rail.
- Reveal each pane's active document. Use stronger selection for focused-pane document and quieter treatment for another pane's active document.
- Manuscript and Research are independent collapsible instances of the shared section pattern.
- Explorer context menu includes applicable create/open/open-in-companion/rename/copy/cut/delete actions.
- Global Search button sits at the right edge of the Explorer header and replaces Explorer when activated.
- Document Inspector contains collapsible Synopsis, Metadata, and Comments. Group Inspector omits Comments.
- Synopsis and metadata values use separate outlined editable fields. Metadata order follows Settings.

### 6.3 Comments

- All threads appear in one continuous list without separate resolved/unresolved sections.
- Thread header exposes per-thread status and Resolve/Reopen using shared button styles.
- Reply disclosure remains available when replies are collapsed; expanded replies are chronological above an in-thread composer.
- Orphaned comments use a centered remediation admonition and remain navigable.

### 6.4 Cards

- Cards is a compact vertical projection of the complete hierarchy with indentation for nesting.
- Group/document Cards display title and Synopsis; configured metadata appears in a read-only right area using short separators rather than chips/tiles.
- Metadata is edited in Inspector.
- No implicit `Status: Draft`.
- Card body and disclosure expand/collapse groups. Use a clear drag insertion marker.

### 6.5 Search and replacement

- Global Search occupies the left pane and has an explicit return to Explorer.
- No search-scope selector in v1.
- Query field plus case-sensitive/whole-word icon toggles. Optional replacement field begins replacement review.
- Group results by document using shared disclosure. Use inline prefix/highlight/suffix so highlight never covers adjacent text.
- Result activation focuses the relevant editor view and scrolls/highlights the match.
- Global Replace Preview fills the middle workspace pane while Search remains left and Inspector right. It is not a modal/card.
- Mirror the hierarchy and use consistent group/document/match selection with indeterminate parents.

### 6.6 History and Recently Deleted

- History uses list/detail. Selecting an entry immediately updates a read-only side-by-side checkpoint/current comparison.
- Highlight word-level changes inside changed lines. Do not add Preview/Compare buttons or leading icons.
- Restore confirmation names the checkpoint and whole-project impact; no scope selector.
- Recently Deleted reuses list/detail but shows one formatted deleted-content preview plus restore-location information, never a diff.

### 6.7 Settings, appearance, dictionaries, and export

- Settings uses the full main pane with list/detail editors.
- Include a clearly labeled Appearance setting with a three-option System/Light/Dark control. The selected state is visible and programmatically exposed. Do not add a quick toggle elsewhere.
- Provide reference states showing an operating-system change while System is selected and an explicit Light/Dark override.
- Custom style rows have trailing trash; reserved rows show `Reserved` in the same column and no trash.
- Metadata rows show drag handle and trailing trash; list order is display order.
- Dictionary settings provide project language, project dictionary add/remove, and global dictionary add/remove controls. Do not show per-document language controls.
- Export uses full main pane with fixed Entire Manuscript summary, title/page-break controls, numbering, destination, progress, success, and failure. No partial-scope or inclusion controls and no redundant outer card.

### 6.8 Word counts

- Status bar shows selection count when text is selected and active-document count otherwise.
- Manuscript total remains available in the normal workspace, placed consistently without adding group/Research/project totals.

### 6.9 Empty, loading, error, and recovery

- Use the full available content region with centered outer margins.
- Preserve normal shell, pane context, and tab strip when the state occurs inside an editor.
- Use nested surfaces only for warnings, recovery choices, destructive confirmation, or remediation.
- Center admonitions and prevent clipping.
- ParchMint is offline by design; do not create an offline warning.

## 7. Required component families

The live Penpot library and final component matrix are exhaustive. At minimum provide stable families for:

| Area | Stable components |
|---|---|
| Shell | `PM/WorkspaceTopBar`, `PM/Sidebar`, `PM/Inspector`, `PM/Splitter`, `PM/StatusBar`, `PM/AppearanceChoice` |
| Explorer | `PM/Tree`, root/group/document rows, insertion marker, cut state, multi-selection, inline rename, context menu |
| Editor | pane, tab strip/tab, canvas, formatting toolbar, style select, local find/replace, search match, editor context menu, spellcheck underline/menu, Scene/Page Break |
| Inspector/comments | section, Synopsis editor, metadata field, comment thread/message/reply/toggle/anchor/orphan state |
| Cards | group/document Card, metadata value, insertion marker |
| Search | sidebar, query controls, result group/row, replace preview hierarchy/selection |
| History | list row, filter, comparison, restore confirmation, Recently Deleted preview |
| Settings | navigation, style/metadata editors, Appearance control, language/dictionary controls |
| Export/save/recovery | export form/progress/result, save states, recovery/admonition/dialog states |
| Shared | button, icon button, text field, multiline field, select, checkbox/radio, menu, tooltip, dialog, snackbar/live region |

Every variant/state used in a reference screen must exist as a component variant or documented composed state.

## 8. Required screens and reference coverage

At minimum, provide deterministic references for:

- Launcher and New Project.
- Editor single and dual pane.
- Same document in both panes with independent selections.
- Cards.
- Global Search and Global Replace Preview.
- Comments including empty/resolved/orphaned.
- History and Recently Deleted.
- Settings: styles, metadata, Appearance, dictionaries.
- Export.
- Save error, recovery, empty/loading/error.
- Keyboard/focus and minimum 1280×720.

For core screens, provide both Light and Dark references. At minimum both themes are required for launcher, editor single/dual, Cards, Search, History, Settings/Appearance, Export, and one error/dialog state.

Cross-platform variants focus on native menu/dialog/shortcut or font-rendering differences; do not duplicate every board unnecessarily.

## 9. Accessibility and focus

- Annotate roles, names, states, levels, focus order, and keyboard actions.
- Show focused versus selected versus active-document versus open-tab states.
- Provide dialog initial focus, trap, close, and restoration behavior.
- Include screen-reader labels for icon-only controls and full truncated tab titles.
- Validate contrast and focus in both themes.
- Include reduced-motion alternatives.

## 10. Prototype flows

Prototype and document:

1. Create project and write.
2. Open companion and same-document dual view.
3. Organize tree and Cards.
4. Add/reply/resolve a comment.
5. Local Find and Global Search/Replace Preview.
6. History whole-project restore.
7. Delete and restore through Recently Deleted.
8. Change Appearance System → Dark → Light and show all-open-window intent.
9. Spellcheck suggestion and project/global dictionary actions.
10. Export Entire Manuscript.
11. Save failure and crash recovery.

## 11. Handoff readiness

The design is ready only when:

- Both semantic theme sets are complete and bound.
- Core Light/Dark references exist.
- Appearance setting and System behavior are specified.
- All required screens/states/components are inventoried.
- Interaction, keyboard/focus, cross-platform, appearance, and known-deviation documents are complete.
- Manifest paths/checksums validate.
- Product-spec version matches.
- The product owner marks the handoff approved.
