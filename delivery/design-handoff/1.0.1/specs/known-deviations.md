# Known Deviations

## 1. Theme-reference plugin hashes vs packaged reference PNGs

- **Affected IDs:** all 20 locked `PM / Reference / Theme / ...` layers on Page
  15 (Handoff Inventory), including the archived layer
  `8bd94a9b-95ad-805a-8008-6dc1843edd46` (`editor-dual-dark.png`).
- **What differs:** the `pm.reference.sha256` recorded on locked layers and the
  `pm.theme-reference-checksums` page-15 metadata may describe an earlier
  snapshot. The four repaired baselines — Light/Dark `editor-dual` and
  `error-recovery` — are authoritative PNG exports from S10 repair run
  `20260804T230332Z-s10p`, replacing the earlier packaged bytes.
- **Canonical truth:** the packaged PNG bytes and their SHA-256 entries in
  `checksums.sha256` are canonical. Recorded plugin hashes are explanatory
  metadata, not overrides. All 20 packaged PNGs decode at 1440×900.
- **Impact:** no functional impact; implementation consumes the packaged
  reference PNGs and their checksums. Visual regression baselines should be
  generated from `references/light|dark/*.png`.
- **Required owner action:** none for v1 handoff. If reference hashes are
  re-recorded, update Page 15 plugin metadata in a future design revision.

## 2. Legacy font id `sourcesanspro` in component text

- **Affected IDs:** text shapes on component mains and screens referencing
  `fontId: sourcesanspro` (243 paragraph references; e.g., `PM / MetadataField /
  Value`, `PM / Sidebar / Title`, `PM / EmptyState / Body`,
  `PM / StyleActionDialog / Title`), plus semantic typography roles `note`,
  `dialog-title`, `annotation` in `PM/Semantic/Common`.
- **What differs:** the legacy family id remains in authored shapes and token
  values; the design's primary UI family is Source Sans 3 (`pm.font.family.ui`).
- **Impact:** import pipeline must normalize `sourcesanspro` to Source Sans 3
  (`font-inventory.csv` row `pm.type.ui.note`). Visual output is unchanged.
- **Required owner action:** normalize in the deterministic import pipeline;
  no design change required.

## 3. Single authored surface for design-only screens

- **Affected IDs:** the 60 screen boards with `baseline_status: design-only` in
  `screen-inventory.csv`.
- **What differs:** only the 10 core screens have packaged Light/Dark reference
  images; remaining production screens are authored once (Light) and carry
  theme-independent token bindings, so Dark rendering is defined by semantic
  tokens rather than a second board.
- **Impact:** visual-reference tests for Dark must be derived from token
  resolution, not from a second screenshot. Native performance/accessibility
  claims cannot come from headless evidence.
- **Required owner action:** none; documented so test authors generate Dark
  references from the component layer.

## 4. Prototype flows are page-local, not a cross-page launcher

- **Affected IDs:** Page 14 (Prototype Flows) 11 index boards; 21 recorded
  navigations.
- **What differs:** prototype navigations exist only within their page, and the
  cover/flow index boards are documentation surfaces rather than an executable
  cross-page clickable launcher. Flow sequences are carried in plugin metadata
  (`pm.prototype.start`, `pm.prototype`, `pm.prototype.targets`,
  `pm.prototype.transition`) and this handoff's `interaction-spec.md`.
- **Impact:** interaction validation is done from the interaction spec and
  board metadata, not by clicking a launcher prototype in Penpot preview.
- **Required owner action:** none.

## 5. Appearance `System` resolves at runtime

- **Affected IDs:** `settings-appearance-system`, active theme state
  `Appearance/Light`.
- **What differs:** the archive's active theme is Light; `System` is not a
  third value set and resolves to Light or Dark at runtime (APPR-002). The
  packaged references therefore show Light and Dark states, not a "System"
  state.
- **Impact:** no reference image exists for a "System" state; this matches
  APPR-001/002.
- **Required owner action:** none.

## 6. Handoff manifest `dark_manuscript_surface` and layout boards

- **Affected IDs:** `PM / Reference / Layout / *` boards on Page 13.
- **What differs:** layout boards are 1280×720 / 1440×900 / 1920×1080 /
  2560×1440 reference boards; the design brief and product spec (WS-011) define
  the minimum window size and resize behavior. No layout board is a production
  screen; they are reference-only.
- **Impact:** none for implementation beyond the layout behavior contract.
- **Required owner action:** none.
