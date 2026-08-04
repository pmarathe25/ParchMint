# ParchMint Design Reconciliation

**Handoff version:** 1.0.0
**Product specification version:** 2.2
**Architecture version:** 1.4
**Design brief version:** 2.5
**Manifest checksum:** `8d24ec366280e5e411258d2f7877a9ef93b9b25a364e3a5b6560930be3d2f3d3`

## 1. Validation result

- Manifest/schema: parsed as schema version 2; status is approved and governing versions match the dispatch.
- All paths/checksums: passed `build-checksums.py --verify`; manifest and checksum-file SHA-256 match the dispatch.
- `.penpot` source: checksum-valid immutable archive; this reconciliation used its frozen package, not live Penpot.
- Light/Dark tokens: complete; 57 semantic leaves per appearance with validated role parity, 282 total leaves, and no dangling aliases.
- Appearance: System resolves at runtime to Light/Dark, all open windows update, authored styles/export stay unchanged, and Dark has a fully dark manuscript canvas.
- References: all 20 approved baseline PNGs decoded at 1440x900 and form ten complete Light/Dark pairs. I inspected the contact sheet: the shell, launch, editor, cards, search, history, settings, export, recovery, and Recently Deleted states are visibly paired; the Dark editor canvas is dark.
- Inventory: 79 component rows, 80 screen rows, 980 indexed SVGs, source/font inventories, interaction/focus/platform/fixture specs, and known deviations were cross-checked.

## 2. Token import

S50 must consume only `tokens/tokens.json` through the deterministic plan in `implementation-map.yaml`: resolve aliases, normalize names, normalize only the documented legacy `sourcesanspro` identifier, emit sorted CSS custom properties and typed metadata with handoff/version/checksum headers, and reject unresolved aliases, duplicate names, incomplete semantic roles, or Light/Dark parity failure. System is an adapter selection, not a generated third set. Generated output is committed and CI reruns the importer and fails on a dirty diff. Production components consume semantic tokens only; hard-coded theme-dependent colors fail the design-system gate.

## 3. Asset import

The icon index is the asset source; each of its 980 SVG checksums and viewBoxes must verify before deterministic copy into the generated design-system asset package. The import preserves SVG/vector and accessibility intent, emits a sorted source-checksum manifest, and rejects unindexed or altered files. Icons are token-colored at render time, so no theme-specific icon swap is approved. Fonts are not copied from the handoff: later packaging must satisfy the inventory's license/platform plan; authored prose fonts remain project data, not UI tokens.

## 4. Component and screen interpretation

`implementation-map.yaml` maps all 79 component-matrix entries by stable family and target, and maps the 80 screen-inventory records to nine workspace-state families. It keeps project mutations with `ProjectCommandDispatcher`, history with `HistoryStore`, search with `SearchIndex`, spellcheck with `SpellcheckService`, editor content behind the ParchMint editor contract, and native dialogs/appearance/accelerators behind platform adapters. It expressly separates shared document/history authority from per-view cursor, selection, scroll, focus, composition, and local-find state.

The traceability update contains only frozen Penpot board/component UUIDs, derives mappings from both inventories, and leaves all non-design columns and the 259 requirement IDs/order intact. Entries without a visual or interaction mapping are intentionally left blank: this does not assert a design mapping for non-visual system requirements.

## 5. Interaction interpretation

Editor focus controls the toolbar and Inspector context; no focused-pane outline is permitted. Global Search replaces Explorer, searches the entire project, and places replacement review in the central workspace. Comments are created from the editor context menu or Comments panel only. Spellcheck decoration/menu remains inline and word-anchored. Appearance propagation is application preference state only. Dialogs establish documented initial focus and restore focus on close. Save/recovery must retain normal shell context where applicable and honor reduced motion. The five conflicting handoff variants are not implementation instructions; see `open-issues.yaml`.

## 6. Accessibility interpretation

Implement the declared F6 region order and surface tab orders; expose roles, names, selected/pressed/expanded states, tree levels, tabs, dialogs, radio choices, and live-region progress. Focus, selection, disabled, error, comment, search, and save state require non-color cues in both themes; icon-only controls require names and tooltips. Native screen reader, IME, clipboard, menu, file-picker, and spelling behavior is not proven here and remains a three-platform validation obligation.

## 7. Visual regression

`visual-regression-plan.md` gives every approved Light/Dark reference a deterministic capture tuple, fixture, platform, scale, tolerance, automated comparison, and human review. System is tested behaviorally by each resolved appearance, not with a duplicate image. Native chrome/dialog/font differences are masked or reviewed semantically rather than normalized into a shared pixel baseline.

## 8. Conflicts and omissions

Five material unresolved conflicts prevent an implementation interpretation: subtree-scoped Global Search; partial Export; document/group-subtree History restore; per-document spellcheck disablement; and the ambiguous resolved-comment filter. They are G10 blockers in `open-issues.yaml`, not silently dropped variants. The established source/font/reference deviations and unproven native behavior are recorded as nonblocking evidence limits.

## 9. Readiness

- Ready to implement: deterministic token/asset import, shared shell mapping, workspace state decomposition, fixture plan, and traceability are complete.
- Blocked pending design/product clarification: ISSUE-001 through ISSUE-005.
- Required handoff revision: remove or clarify the conflicting component/screen states, then product-owner approval must set `approval.yaml` to `approved`; a behavior change instead requires G20.
