# Keyboard and Focus Map

Focus, selection, active document, active pane, and open-but-unfocused states
must remain distinguishable and programmatically exposed in both appearances
(WS-010, A11Y-002, A11Y-003) without relying on color alone or on hover.

## Global focus order (Editor mode)

Mode switch → formatting toolbar → Explorer → active tab → focused editor →
Inspector → status bar. **F6** cycles the major regions. The status bar exposes
Explorer/Inspector show-hide, word count, save status, and the contextual
document-History action (WS-013).

## Focus, selection, and activated states

- **Focused pane** — the target of toolbar commands, key input, and the
  Inspector context (TOOL-003, WS-009). Communicate focus through tab-strip
  state and accessible state; do not outline the focused editor pane.
- **Active document** — the primary active context is the focused pane's
  document; the other open pane retains independent selection/scroll/local
  search (EDIT-007).
- **Active tab** — the currently displayed tab in a pane; distinguish active
  tab, focused pane, dirty state, and close controls (EDIT-012). Tabs shrink
  uniformly on overflow preserving first character, ellipsis, and close control;
  full titles remain in accessible alternative text.
- **Open-but-unfocused** — a mounted view retains independent cursor, selection,
  scroll, and local-search state (EDIT-006/007).

## Per-surface tab order

| Surface / screen | Tab order / initial focus | Notes |
|---|---|---|
| Explorer | Section roots (Manuscript, Research) first; disclosure rows; then tree nodes; context menu on action | Disclosure is a focusable disclosure row (A11Y), not a separate toggle |
| Inspector | Section header(s); then in-place editable Synopsis and Metadata values; Comments section | Read-only metadata values are visibly distinct from editable controls |
| Toolbar | Style selection first, then inline marks, lists/quote, links, breaks | Always visible in Editor mode; no collapsed/scroll scheme (TOOL-002) |
| Formatting toolbar context menu | Items in authored order; close on Esc; restore focus to the focused editor | Same surface family as context menus; consistent overlay ordering |
| Local Find / Global Search | Query field first; then toggles (case/whole-word); result list; footer count | Local find is per-view; Global Search replaces Explorer in the sidebar |
| Comments panel | Thread list; within a thread: replies then composer | Thread is programmatically related to its anchor; filter/collapse announced |
| Create Project dialog | Project name first | Escape/cancel returns to Launcher without creating |
| Export dialog | Scope or error summary first | Progress and completion announced politely; native file handoff documented |
| Save-failure dialog(s) | Safe action first | Authored content remains intact; retry succeeds into editor |
| History restore confirmation | Checkpoint name and whole-project impact; then confirm/cancel | Highlight changed lines at word level, no compare buttons (HIST-007) |
| Recently Deleted | Trash item list, then restore/fallback location confirmation | Restore to former or clear fallback location |
| Appearance settings | System, Light, Dark radio/segmented group | Every open window updates immediately without history entry |

## Keyboard conventions

- Letter/number key access via native menus and shortcuts is platform-specific
  and defined per platform in `cross-platform-variants.md` (PLAT-001/004).
- Tab/Shift+Tab and arrow navigation follow the OS conventions; disclosure and
  list rows use standard toggle semantics.
- Escape closes menus, dialogs, and cached cut state (`CutState`); pending cut
  items remain visible until paste succeeds or Esc cancels (TREE-013).
- F6 cycles major regions (workspace navigation).
- Truncated tab titles always carry full-title alternative text (EDIT-012).

## Light/Dark focus visibility

- Focus rings use tokenized focus styles (`PM/FocusVisible`, A11Y-002) with
  minimum focus-ring offsets (brief §5); they are visible on both light and dark
  surfaces and do not rely on color alone (APPR-008).
- Focus, selection, disabled, warning, error, comment, search-match, and save
  states remain distinguishable in both appearances without color alone
  (APPR-008, A11Y-002, A11Y-003).

## Verified keyboard/accessibility metadata

- `pm.focus-context` (primary/companion) recorded on editor screens; F6 cycle
  pattern recorded on editor-single-default.
- `pm.a11y` notes recorded on search (polite announcements, predictable focus
  restoration), comments (anchor relation, filter/collapse announcements), and
  export (initial focus scope/error, progress and completion announcements).
- `PM / Reference / Accessibility / *` boards on Page 12 record the
  a11y-editor-keyboard-walk, a11y-history-deleted-walk, a11y-motion-targets-contrast,
  and a11y-semantic-annotation-* reference walks.