# ParchMint Design Artifact Handoff Contract

**Status:** Current handoff contract  
**Version:** 1.2  
**Date:** 2026-07-31

## 1. Purpose

This contract defines the immutable design package required before implementation. It makes the approved Penpot design reproducible, inspectable, versioned, theme-complete, and usable with or without live Penpot MCP access.

The approved design is a visual and interaction source of truth below the product specification. It is not permission to change product behavior or architecture silently.

## 2. Canonical paths

Design source artifacts and design interpretation intentionally use different roots:

- `design/handoff/<design-version>/`: immutable product-owner-approved source handoff.
- `docs/design/reconciliation/<design-version>/`: implementation agent's reviewed mapping of that handoff.

This split is canonical and must not be collapsed.

## 3. Required handoff directory

```text
design/handoff/<design-version>/
├── README.md
├── design-manifest.yaml
├── parchmint-ui.penpot
├── tokens/
│   ├── tokens.json                 # or Penpot multifile export
│   └── README.md
├── assets/
│   ├── icons/*.svg
│   └── illustrations/*.svg|png
├── references/
│   ├── light/
│   │   ├── launcher-1440x900.png
│   │   ├── editor-single-1440x900.png
│   │   └── ...
│   └── dark/
│       ├── launcher-1440x900.png
│       ├── editor-single-1440x900.png
│       └── ...
├── specs/
│   ├── interaction-spec.md
│   ├── component-matrix.csv
│   ├── screen-inventory.csv
│   ├── keyboard-focus-map.md
│   ├── appearance-matrix.md
│   ├── cross-platform-variants.md
│   └── known-deviations.md
└── checksums.sha256
```

All paths are relative to the handoff root. The package must be committed to the repository.

## 4. Required artifacts

### 4.1 Native `.penpot` export

Provide the complete approved file as `parchmint-ui.penpot`, including all pages, components/variants, Light/Dark token bindings, prototype interactions, and required libraries/assets. It must not depend on private unavailable assets.

### 4.2 Design-token export

Export all token sets and themes in JSON. Preserve Penpot set/theme structure and semantic descriptions.

Required categories:

- Color roles for Light and Dark.
- Typography.
- Spacing/dimensions.
- Radius.
- Border/stroke.
- Shadow/elevation.
- Motion.
- Z-layer/overlay intent.

The export must contain a complete Light set and a complete Dark set. System is runtime resolution, not a third visual token set.

Generated CSS is not a substitute for source token JSON.

### 4.3 Assets

- Export vector icons as optimized SVG where possible.
- Preserve `viewBox` and accessibility/title intent.
- Avoid text converted to paths unless necessary.
- Use PNG only for raster content/reference screenshots.
- Exclude unused library assets.
- Use stable lowercase kebab-case filenames.

### 4.4 Reference snapshots

Provide PNGs for every manifest screen with `reference: true`.

Each record identifies screen ID, Penpot board ID, dimensions, scale, platform, theme, deterministic fixture/state, and baseline/reference-only status.

Core screens require Light and Dark references. Native font rendering may differ later, but both semantic designs must be explicit.

### 4.5 Interaction specification

Document behavior not safely inferred from static boards:

- Click/double-click/keyboard activation.
- Drag/drop and invalid targets.
- Focus transfer and restoration.
- Pane/Inspector context and toolbar targeting.
- Local/global search transitions.
- Editor context-menu Add Comment, comment-highlight geometry, and Comments-panel commands.
- Spellcheck underline/menu anchoring and dictionary actions.
- Appearance System/Light/Dark selection and open-window propagation.
- Collapse/expand behavior.
- Dialog initial focus and close behavior.
- Loading, save, error, recovery, and restoration transitions.
- Reduced motion.

Each interaction cites requirement and component/screen IDs.

### 4.6 Component matrix

`component-matrix.csv` contains:

```text
component_id,penpot_component_id,name,variants,states,themes,requirements,implementation_target,notes
```

Use stable `PM/...` names. `implementation_target` may be blank before reconciliation.

### 4.7 Screen inventory

`screen-inventory.csv` contains:

```text
screen_id,penpot_page_id,penpot_board_id,name,reference_width,reference_height,platform,theme,state,requirements,reference_image
```

### 4.8 Keyboard/focus map

Document focus order, shortcuts, roles, accessible names/states, dialog behavior, focus restoration, tree levels, tab semantics, and appearance-independent focus visibility.

### 4.9 Appearance matrix

`appearance-matrix.md` records:

- Semantic token-set names and Penpot theme IDs.
- System → Light/Dark runtime resolution intent.
- Required Light/Dark reference pairs.
- Components with intentional theme-specific asset variants.
- Contrast/focus/state checks.
- Confirmation that authored prose styles/export do not change with appearance.
- Confirmation that Dark uses a dark manuscript canvas.

### 4.10 Known deviations

Record missing states, prototype/export limitations, approximated components, accessibility concerns, and any conflict with product/architecture. An empty `No known deviations` file is acceptable; omission is not.

Do not maintain a permanent design-decisions file. Current approved visual behavior belongs in the design brief, components, interaction spec, and appearance matrix.

### 4.11 Checksums

Generate SHA-256 hashes for every exported file. The manifest records its version and checksum file. The checksum file need not hash itself.

## 5. `design-manifest.yaml`

Use `templates/design-manifest.yaml`. It identifies:

- Handoff version/status.
- Penpot version/file/page IDs.
- Product-spec and design-brief versions.
- Token/theme files and IDs.
- Assets.
- Screens/references.
- Components/IDs.
- Prototype flows.
- Platform variants.
- Appearance coverage.
- Known deviations.
- Checksums.

S00 rejects or blocks a handoff with missing files, invalid checksums, incomplete Light/Dark coverage, or mismatched governing versions.

## 6. Consumption order

1. Live Penpot MCP access when available.
2. `.penpot` export.
3. Token JSON.
4. SVG/PNG assets.
5. Manifest, component, interaction, keyboard/focus, appearance, and platform specs.
6. Reference PNGs.
7. Generated HTML/CSS only as disposable explanatory input.

Implementation may proceed without live MCP when the export pack is complete.

## 7. Reusable versus reference-only

Directly reusable after validation:

- Token JSON transformed into generated semantic CSS variables.
- SVG/PNG assets.
- Stable component/screen IDs.
- Interaction/accessibility annotations.
- Reference screenshots.

Reference-only unless reviewed/reimplemented:

- MCP-generated HTML/CSS.
- Penpot Inspect snippets.
- Prototype data models.
- Layer hierarchy as application state.
- Absolute-position values better represented with flex/grid.

## 8. Reconciliation before implementation

The first UI deliverable is:

```text
docs/design/reconciliation/<handoff-version>/
├── design-reconciliation.md
├── implementation-map.yaml
├── visual-regression-plan.md
├── open-issues.yaml
├── work-breakdown.md
└── approval.yaml
```

It covers handoff validation, theme/token import, assets, component/screen/interaction mapping, accessibility, platform differences, visual baselines, conflicts, and work breakdown.

No broad UI implementation begins until `approval.yaml` is committed with `status: approved`. The Orchestrator stops at G10.

## 9. Token import pipeline

A deterministic script such as `scripts/design/import-penpot-tokens` must:

- Read approved token exports.
- Validate supported types/references and complete Light/Dark semantic roles.
- Normalize names to stable ParchMint IDs.
- Generate CSS custom properties and TypeScript metadata.
- Preserve descriptions.
- Fail on unresolved references, duplicate normalized names, incomplete theme roles, or theme-dependent values embedded in component code.
- Produce deterministic output with source handoff/version headers.

Generated files are committed and never edited manually. CI regenerates and fails on drift.

## 10. Visual validation

For each reference screen:

1. Load the same deterministic fixture/state.
2. Set specified dimensions, scale, platform, and theme.
3. Capture implementation screenshot.
4. Compare automatically and visually.
5. Record current approved deviations in the reconciliation/release package.

Pixel comparison is diagnostic, not the sole criterion. Layout hierarchy, spacing, component state, focus, information architecture, and semantic theme intent must match.

## 11. Change management

A later approved Penpot revision creates a new handoff directory; it never overwrites an approved one. Update manifest/checksums, export tokens/assets/references, and run design-change reconciliation.

Superseded handoff directories may be removed when they are no longer needed by an active implementation branch; they are not required as a permanent design history.

## 12. Penpot MCP

MCP may inspect layout/tokens/styles, map components, export assets, generate prototype code, and audit consistency. Record live file/page connection details in the handoff README, but production must not depend permanently on a live Penpot service.
