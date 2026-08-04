# Appearance Matrix

- Light token set/theme ID: `PM/Semantic/Light` (theme `Light`, id `dc55a0ad-9907-801d-8008-6a1fb40bcf7a`, group Appearance)
- Dark token set/theme ID: `PM/Semantic/Dark` (theme `Dark`, id `469ffc7d-964a-806d-8008-6c323f6d3396`, group Appearance)
- System runtime resolution: `System` follows the current OS appearance at
  runtime and is not a third value set; it resolves to Light or Dark. Active
  theme state in the archive is `Appearance/Light` + `Foundation/Default`
  (activeSets: PM/Core, PM/Semantic/Light, PM/Semantic/Common, PM/Layout/Desktop,
  PM/Motion). Explicit Light/Dark overrides persist and outrank later OS changes
  (APPR-002/003).
- Light/Dark semantic parity: 55 semantic color roles + 2 shadow roles in each
  of Light and Dark (57/57 leaves), cross-checked as parity-validated; core
  palette/motion/layout/typography sets are theme-independent.
- Dark manuscript surface confirmed: `dark_manuscript_surface: true` — Dark uses
  fully dark application, sidebar, Inspector, toolbar, editor-chrome, and
  manuscript-canvas surfaces (APPR-005). The prose canvas must not remain a
  light sheet.
- Authored prose styles/export unchanged: Appearance must not alter authored
  styles, canonical HTML/CSS, or export output (APPR-007).
- Open-window propagation specified: APPR-004 — changing appearance updates
  every open ParchMint window without restarting and without entering project
  undo/save/history.

## Theme reference matrix

The locked reference screens (10 screens × Light/Dark, all 1440×900) are
`references/light/*.png` and `references/dark/*.png`. Full status is in
`screen-inventory.csv` (baseline rows).

| Screen/component | Light reference | Dark reference | Focus/contrast/state checks | Theme-specific asset | Notes |
|---|---|---|---|---|---|
| launcher (recent projects) | references/light/launcher-light.png | references/dark/launcher-dark.png | focus ring on recent rows; no color-only state | none | PRJ-002/010 |
| editor-single default | references/light/editor-single-light.png | references/dark/editor-single-dark.png | pane focus via tab strip; toolbar visible | none | WS-001/002, TOOL-002 |
| editor-dual two-manuscript | references/light/editor-dual-light.png | references/dark/editor-dual-dark.png | focused vs open-but-unfocused view distinguishable | none | WS-006, EDIT-001/006/007 |
| cards manuscript default | references/light/cards-light.png | references/dark/cards-dark.png | selection across document/group rows in both themes | none | CARD-002/006 |
| global search query/results | references/light/global-search-light.png | references/dark/global-search-dark.png | search-match highlight distinguishable, not color-only | none | SEARCH-006/009/011 |
| history session list | references/light/history-light.png | references/dark/history-dark.png | restore target + changed-lines highlights visible | none | HIST-007/010 |
| settings appearance | references/light/settings-appearance-light.png | references/dark/settings-appearance-dark.png | System/Light/Dark choices readable in both | none | APPR-001/002/003 |
| export entire manuscript | references/light/export-light.png | references/dark/export-dark.png | progress/success notices in both | none | EXP-001/008 |
| error recovery (recovered-after-crash) | references/light/error-recovery-light.png | references/dark/error-recovery-dark.png | admonition/error surface readable; no color-only | none | SAVE-011/012/013 |
| recently deleted | references/light/recently-deleted-light.png | references/dark/recently-deleted-dark.png | destructive row action + restore distinguishable | none | DEL-001/003 |

## Implementation obligations

- Production components consume semantic tokens (`pm.semantic.*`) rather than
  hard-coded theme-dependent colors (APPR-006); the design-system gate fails
  hard-coded colors.
- Icons are exported token-colored (`fill_token` referenced per icon in
  `assets/icons/icon-index.json`); the app resolves the current appearance's
  semantic color at render time, so icons adapt with no theme-specific asset swap.
- Focus/selection/disabled/error/comment/search-match/save states are
  distinguishable in both themes without relying on color alone (APPR-008).