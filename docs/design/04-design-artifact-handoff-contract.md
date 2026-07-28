# ParchMint Design Artifact Handoff Contract

**Status:** Final handoff contract  
**Version:** 1.0  
**Date:** 2026-07-28

## 1. Purpose

This contract defines the design package that the implementation agent must receive after the Penpot design is approved. The goal is to make the design reproducible, inspectable, versioned, and useful whether the agent consumes it live through Penpot MCP or from exported files.

The approved design is a visual and interaction source of truth below the PRD. It is not permission to change product behavior or architecture silently.

## 2. Required handoff directory

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
│   ├── launcher-1440x900.png
│   ├── editor-single-1440x900.png
│   ├── editor-dual-1440x900.png
│   ├── cards-1440x900.png
│   ├── search-1440x900.png
│   ├── history-1440x900.png
│   └── ...
├── specs/
│   ├── interaction-spec.md
│   ├── component-matrix.csv
│   ├── screen-inventory.csv
│   ├── keyboard-focus-map.md
│   ├── cross-platform-variants.md
│   ├── design-decisions.md
│   └── known-deviations.md
└── checksums.sha256
```

All paths are relative to the handoff root. The design package must be committed to the project repository.

## 3. Required artifacts

### 3.1 Native `.penpot` export

Provide the complete approved file as `parchmint-ui.penpot`.

The file must contain:

- All pages.
- Components and variants.
- Token bindings.
- Prototype interactions.
- Libraries used by the file or a self-contained copy of required assets.
- No private/unavailable external asset dependency.

The `.penpot` file is an inspectable ZIP/JSON-based source artifact. The implementation agent may inspect it directly or through Penpot MCP.

### 3.2 Design-token export

Export all token sets and themes from Penpot in JSON. Single-file and multifile exports are both acceptable; preserve Penpot’s set/theme structure.

Tokens must include descriptions or naming sufficient to distinguish semantic purpose. Do not export a flat list of visual values with ambiguous names.

Required token categories:

- Color.
- Typography.
- Spacing.
- Dimensions.
- Radius.
- Border/stroke.
- Shadow/elevation.
- Motion.
- Z-layer/overlay intent where needed.

The implementation will normalize the export and generate CSS custom properties. Generated CSS is not a substitute for preserving the original token JSON.

### 3.3 Assets

- Export vector icons as optimized SVG where possible.
- Preserve viewBox and accessibility/title intent.
- Avoid text converted to paths unless necessary.
- Use PNG only for genuinely raster content or reference screenshots.
- Do not export unused design-library assets.
- Asset filenames must be stable, lowercase, and kebab-case.

### 3.4 Reference snapshots

Provide PNG reference images for every screen/state designated `reference: true` in the manifest.

Each snapshot record must include:

- Screen ID.
- Penpot board ID.
- Pixel dimensions.
- Scale factor.
- Platform variant if applicable.
- Theme/density.
- Data fixture/state.
- Whether it is a visual-regression baseline or explanatory reference only.

Snapshots must use stable deterministic sample content from the handoff, not random text.

### 3.5 Interaction specification

`interaction-spec.md` documents behavior that cannot be inferred safely from static boards:

- Click/double-click/keyboard activation.
- Drag/drop targets and invalid states.
- Focus transfer.
- Pane/Inspector context changes.
- Toolbar targeting.
- Search-bar animation and focus restoration.
- Comment-affordance positioning.
- Collapse/expand behavior.
- Dialog initial focus and close behavior.
- Loading, save, error, recovery, and restoration transitions.
- Reduced-motion behavior.

Each interaction cites PRD requirement IDs and component/screen IDs.

### 3.6 Component matrix

`component-matrix.csv` contains at least:

```text
component_id,penpot_component_id,name,variants,states,requirements,implementation_target,notes
```

Use stable `PM/...` names. `implementation_target` may be blank before reconciliation.

### 3.7 Screen inventory

`screen-inventory.csv` contains:

```text
screen_id,penpot_page_id,penpot_board_id,name,reference_width,reference_height,platform,theme,state,requirements,reference_image
```

### 3.8 Decision log

`design-decisions.md` records decisions that were not explicitly specified, such as:

- Visual hierarchy choices.
- Typography and density rationale.
- Panel dimensions.
- History presentation.
- Cards density behavior.
- Component grouping.
- Intentional native-platform differences.

It must distinguish product decisions from visual decisions. Unapproved product changes are logged as questions, not decisions.

### 3.9 Known deviations

`known-deviations.md` records:

- Missing screens or states.
- Prototype limitations.
- Penpot/MCP export limitations.
- Components represented approximately.
- Accessibility concerns needing implementation validation.
- Any conflict with PRD or architecture.

An empty file stating `No known deviations` is acceptable; omission is not.

### 3.10 Checksums

Generate SHA-256 hashes for all exported handoff files. The design manifest records its own version and the checksum file.

## 4. `design-manifest.yaml`

Use the template under `templates/design-manifest.yaml`.

The manifest identifies:

- Handoff version and status.
- Penpot version and file/page IDs.
- Product-spec version.
- Design-agent identity/tooling if desired.
- Token files.
- Assets.
- Screens and reference images.
- Components and Penpot IDs.
- Prototype flows.
- Platform variants.
- Known deviations.
- Checksums.

The implementation agent must reject or flag a handoff whose manifest points to missing files or mismatched product-spec versions.

## 5. Preferred consumption order

1. **Live Penpot MCP access** to the approved file, when available.
2. **`.penpot` export** for full structured source and long-term reproducibility.
3. **Token JSON** for code generation.
4. **SVG assets** for direct integration.
5. **Manifest/component/interaction documents** for mapping.
6. **Reference PNGs** for visual validation.
7. **Generated HTML/CSS snippets** only as disposable explanatory input.

The implementation may proceed without live MCP if the exported pack is complete.

## 6. Directly reusable versus reference-only outputs

### Directly reusable after validation

- Token JSON transformed into generated CSS variables.
- SVG/PNG assets.
- Stable component/screen IDs.
- Interaction and accessibility annotations.
- Reference screenshots.

### Reference-only unless reviewed and rewritten

- MCP-generated HTML/CSS.
- Penpot Inspect snippets.
- Prototype data models.
- Layer hierarchy as an application state model.
- Absolute-position layout values that should be expressed with flex/grid.

The implementation agent must not paste generated design code wholesale into production without semantic, accessibility, responsiveness, and maintainability review.

## 7. Design reconciliation before implementation

The first UI implementation deliverable is `docs/design/design-reconciliation.md` using the provided template.

It must include:

1. Handoff validation results.
2. Token import plan.
3. Asset import plan.
4. Penpot-to-code component map.
5. Screen-to-route/state map.
6. Interaction implementation map.
7. Accessibility interpretation.
8. Platform-specific variations.
9. Planned visual-regression baselines.
10. Conflicts, omissions, and proposed resolutions.
11. Intentional deviations requiring approval.

No broad UI implementation should begin before this report is reviewed.

## 8. Token import pipeline

The repository should include a deterministic script such as:

```text
scripts/design/import-penpot-tokens
```

It must:

- Read the approved token export.
- Validate supported token types and references.
- Normalize names into stable ParchMint token IDs.
- Generate CSS custom properties and TypeScript token metadata.
- Preserve comments/descriptions where useful.
- Fail on unresolved references or duplicate normalized names.
- Produce deterministic output.
- Record source handoff/version in the generated header.

Generated files are committed but never edited manually.

## 9. Visual validation

For each reference screen:

1. Load the same deterministic fixture and state.
2. Set the specified window size and scale.
3. Capture the implementation screenshot.
4. Compare automatically and visually.
5. Record approved deviations.

Pixel comparison is a diagnostic, not the sole acceptance criterion. Native font rendering, accessibility fixes, and platform controls can justify differences. Layout hierarchy, spacing, component state, focus, and information architecture must still match.

## 10. Change management

A later Penpot revision creates a new handoff directory; it does not overwrite an approved one.

For each revision:

- Increment design version.
- Update manifest and checksums.
- Export tokens/assets again.
- Record changed components/screens.
- Generate a design-diff summary.
- Update code mapping and visual baselines deliberately.

## 11. Penpot MCP usage

The design and implementation agents may use Penpot MCP to:

- Inspect pages, layout, components, tokens, and styles.
- Export selected assets.
- Map components to code.
- Generate prototype HTML/CSS for analysis.
- Audit naming and token consistency.
- Validate design-to-code translation.

The live open file/page and MCP connection details should be recorded in the handoff README, but no implementation should depend permanently on a live Penpot service.

## 12. Current official Penpot references

Use current official Penpot documentation when tooling behavior changes:

- Penpot MCP: <https://help.penpot.app/mcp/>
- Design tokens and token import/export: <https://help.penpot.app/user-guide/design-systems/design-tokens/>
- `.penpot` file format: <https://help.penpot.app/user-guide/export-import/penpot-file-format/>
- MCP design-file structure practices: <https://help.penpot.app/mcp/design-file-structure-best-practices/>

Penpot MCP can inspect layout/tokens/styles, map components to code, export assets, generate prototype HTML/CSS, and validate design-to-code translation. The `.penpot` format is an open ZIP archive containing JSON metadata and media objects. Token exports are JSON and may be single-file or multifile.
