# ParchMint Design Reconciliation

**Handoff version:** 1.0.1 (draft candidate; exact product-owner approval pending)
**Product specification version:** 2.2
**Architecture version:** 1.4
**Design brief version:** 2.5
**Manifest checksum:** `0d53ded15bcf035dcc7a0ac3ee1afe8909831550ca5a0e815504b7d39f905b14`

## 1. Validation result

- Manifest/schema: parsed as schema version 2; status is draft and governing versions match the dispatch.
- All paths/checksums: regenerated after the refreshed native source and Light/Dark reference pair were installed.
- `.penpot` source: refreshed user-exported archive from live file `2be68822-842f-8175-8008-65eef13b0227`, SHA-256 `2c41059ee5b6b5eb2099d1cc5e090dd42c91d0835ff97d66104b46610d20a35d`. ZIP integrity passes; Research and Harbor Notes are visible in Explorer while the two obsolete Harbor Notes pane tabs remain hidden.
- Light/Dark tokens: complete; 57 semantic leaves per appearance with validated role parity, 282 total leaves, and no dangling aliases.
- Appearance: System resolves at runtime to Light/Dark, all open windows update, authored styles/export stay unchanged, and Dark has a fully dark manuscript canvas.
- References: all 20 candidate baseline PNGs decode at 1440x900 and form ten complete Light/Dark pairs. The refreshed dual pair visibly shows both Explorer roots plus separate Chapter One/Chapter Two panes; Dark has readable fully dark surfaces.
- Inventory: 79 component rows, 87 screen/reference rows, 980 indexed SVGs, source/font inventories, interaction/focus/platform/fixture specs, and known deviations were cross-checked.

## 2. Token import

S50 must consume only `tokens/tokens.json` through the deterministic plan in `implementation-map.yaml`: resolve aliases, normalize names, normalize only the documented legacy `sourcesanspro` identifier, emit sorted CSS custom properties and typed metadata with handoff/version/checksum headers, and reject unresolved aliases, duplicate names, incomplete semantic roles, or Light/Dark parity failure. System is an adapter selection, not a generated third set. Generated output is committed and CI reruns the importer and fails on a dirty diff. Production components consume semantic tokens only; hard-coded theme-dependent colors fail the design-system gate.

## 3. Asset import

The icon index is the asset source; each of its 980 SVG checksums and viewBoxes must verify before deterministic copy into the generated design-system asset package. The import preserves SVG/vector and accessibility intent, emits a sorted source-checksum manifest, and rejects unindexed or altered files. Icons are token-colored at render time, so no theme-specific icon swap is approved. Fonts are not copied from the handoff: later packaging must satisfy the inventory's license/platform plan; authored prose fonts remain project data, not UI tokens.

## 4. Component and screen interpretation

`implementation-map.yaml` maps all 79 component-matrix entries by stable family and target, and maps the 87 screen/reference inventory records to nine workspace-state families. It keeps project mutations with `ProjectCommandDispatcher`, history with `HistoryStore`, search with `SearchIndex`, spellcheck with `SpellcheckService`, editor content behind the ParchMint editor contract, and native dialogs/appearance/accelerators behind platform adapters. It expressly separates shared document/history authority from per-view cursor, selection, scroll, focus, composition, and local-find state.

The traceability update contains only frozen Penpot board/component UUIDs, derives direct mappings from both inventories, and leaves all non-design columns and the 259 requirement IDs/order intact. Every remaining unmapped row has an explicit deferred, nonvisual/state, native/release, performance, or security disposition in the repair evidence; that does not assert a design mapping for a nonvisual system requirement.

## 5. Interaction interpretation

Editor focus controls the toolbar and Inspector context; no focused-pane outline is permitted. Whenever Explorer is shown, both Manuscript and Research roots remain visible even when no Research document is open; Global Search replacement and a deliberately collapsed/hidden Explorer are the only exceptions. Global Search replaces Explorer, searches the entire project, and places replacement review in the central workspace. Comments are created from the editor context menu or Comments panel only. Spellcheck decoration/menu remains inline and word-anchored. Appearance propagation is application preference state only. Dialogs establish documented initial focus and restore focus on close. Save/recovery must retain normal shell context where applicable and honor reduced motion. The five conflicting handoff variants are not implementation instructions; see `open-issues.yaml`.

## 6. Accessibility interpretation

Implement the declared F6 region order and surface tab orders; expose roles, names, selected/pressed/expanded states, tree levels, tabs, dialogs, radio choices, and live-region progress. Focus, selection, disabled, error, comment, search, and save state require non-color cues in both themes; icon-only controls require names and tooltips. Native screen reader, IME, clipboard, menu, file-picker, and spelling behavior is not proven here and remains a three-platform validation obligation.

## 7. Visual regression

`visual-regression-plan.md` gives every approved Light/Dark reference a deterministic capture tuple, fixture, platform, scale, tolerance, automated comparison, and human review. System is tested behaviorally by each resolved appearance, not with a duplicate image. Native chrome/dialog/font differences are masked or reviewed semantically rather than normalized into a shared pixel baseline.

## 8. Conflicts and omissions

## 1.0.0 → 1.0.1 candidate impact

The complete 1.0.1 candidate retains the 79 component rows, expands the screen
inventory to 87 rows to include the seven Page 13 reference boards, and keeps
stable component/screen IDs, token sets, SVG sources, and unaffected references
from 1.0.0. It replaces the native source and the Light/Dark dual-editor and
recovery references with the authoritative S10 repair exports. It removes the
unsupported Global Search, Export, History restore, and spellcheck states;
defines the resolved-comments filter as narrowing one continuous unsectioned
list; and corrects `Entire Manual centroid` to `Entire Manuscript`.

ISSUE-001 through ISSUE-005 and ISSUE-010 are resolved in `open-issues.yaml`.
The Explorer screen repair is source-verified and directly reviewed in the
refreshed Light/Dark references. Source/font deviations and unproven native
behavior remain nonblocking evidence limits. This
reconciliation remains pending until the product owner approves this exact
checksum-valid candidate.

## 9. Readiness

- Ready to request fresh validation: deterministic token/asset import, shared shell mapping, workspace state decomposition, fixture plan, traceability mappings, and honest nonvisual dispositions are complete.
- Resolved design/product conflicts: ISSUE-001 through ISSUE-005; their rejected variants are not implementation instructions.
- Required approval action: the product owner must review and approve this exact
  checksum-valid 1.0.1 handoff and reconciliation, then set `approval.yaml` to
  `approved` before G10 advances.
