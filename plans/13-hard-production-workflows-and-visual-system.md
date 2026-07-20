# Stage 13: production workflows and visual system

Difficulty: hard  
Recommended model: GPT-5.6 Terra, high or xhigh reasoning  
Escalate to Sol when: editor-adapter fidelity or shared-session integration remains unresolved  
Depends on: stages 11–12  
Master references: `PLAN.md` “Product contract”; `plans/10-audit-report.md` §§4.2–6 and 7 P1/P3.11

## Outcome

Make the already-built product features reachable in the shipping window and present them as one coherent desktop writing environment. The default editor becomes semantic WYSIWYG with an explicit source mode, planning/trash/compile workflows are complete, editable projections cannot write stale values, and the visual system uses neutral chrome plus restrained brand accent.

The audit remediation already exposes save status and guards duplicate document panes. Stage 12 supplies the authoritative live-document session that this stage must bind to.

## Primary ownership

- `app/cpp/editor_adapter.*`, `semantic_object_renderer.*`, and Qt tests
- QML components under `app/qml/components/` and `Main.qml`
- Command catalog and bridge invokables/models
- `OutlineModel` drag/drop and incremental role surface (functional slice only; scale deltas finish in stage 14)
- Accessibility, keyboard, icon, theme, and QML integration tests

## Required work

### 1. Harden and activate the semantic editor

- Fix protected-format leakage after thematic/page/opaque insertion and ensure the following typed paragraph uses the correct next style.
- Preserve list grouping/numbering and underline; make protected-object deletion/paste policy consistent with formatting policy.
- Sanitize rich HTML paste into allowed local semantic runs and block remote resource loads.
- Bind one `EditorAdapter` per live pane session, instantiate `FormattingBar`, `StylePicker`, `StyleManager`, and `SourceEditor`, and keep cursor/scroll/undo state per pane.
- Default to WYSIWYG. Provide a visible source toggle with parse diagnostics and the stage-12 conflict/flush rules.

### 2. Planning and binder workflows

- Add an Editor/Outline/Cards segmented switcher to each pane header and matching commands/shortcuts; set `PaneView::Cards` through the production bridge.
- Remove built-in roots from card layout rather than hiding delegates that still consume cells.
- Implement binder drag/drop with model flags, typed MIME payload, before/after/inside affordances, auto-expand, cycle rejection, and keyboard parity.
- Add a trash section/view with restore and explicit empty-trash confirmation. Show unavailable-parent recovery choices.
- Make F2 rename, Enter open, arrow navigation, multi-select, and pin state reliable across model refreshes.

### 3. Metadata, statistics, and compile UI

- Bind “Include in compile” to the canonical metadata flag and reflect it immediately in previews/exports.
- Replace stale one-way editable bindings in Inspector/Outline with focus-aware edit buffers keyed by node ID; commits must target the node that originated the edit.
- Expose document/subtree/manuscript/research/project totals and display per-row/project counts. Selection counts must come from the live session without full-text FFI rescans on every keypress.
- Add compile preset selection/editing, progress, overwrite confirmation, cancellation, warning summary, and reveal-in-file-manager on success.
- Replace placeholder tags/notes with canonical controls or remove the promise until implemented.

### 4. Welcome and information architecture

- Replace modal-only onboarding with a start view containing New, Open, sample project, and validated recent-project entries.
- Move binder filtering into the binder header; keep global search and command palette in global chrome.
- Show document titles in pane headers, clear view state, save/error state, and accessible tooltips for every icon action.
- Demote destructive inspector actions and group Synopsis, Metadata, and Statistics into clear sections.

### 5. Visual system and accessibility

- Expand `DesignTokens` to neutral base/surface/raised/overlay, text hierarchy, outline, accent, danger, typography, spacing, radius, elevation, and motion scales.
- Convert header/footer to thin neutral bars; reserve teal for selection, focus, links, and primary actions.
- Make the editor full-bleed with a centered 68–76ch measure, scalable 1.5–1.6 line height, calm placeholder, and no bright container border.
- Replace Unicode glyph buttons with bundled, theme-aware SVG icons on a consistent grid.
- Add selected/hover/focus states, binder section headers/chevrons/type icons, density consistency, reduced-motion handling, and high-contrast/focus-order verification.

## Verification

- Qt adapter tests cover every semantic block/inline, insertion boundary, protected paste/delete, lists, underline, source transitions, and two-pane state.
- QML tests exercise view switching, drag/drop and keyboard reorder, trash restore/empty, include flag, metadata recycling, counts, preset/overwrite flow, welcome/recent paths, and pin rejection.
- Run `qmllint`, accessibility/focus audits, keyboard-only charters, and screenshots at supported themes/scales on Windows, macOS, X11, and Wayland.
- Confirm the app makes no network request during editor load/paste/preview.

## Acceptance gate

- The shipping pane uses the semantic WYSIWYG adapter and every required formatting/style/source action is reachable.
- Outline and Cards are directly selectable, drag/drop and keyboard reordering agree, and trash/include/count/preset workflows persist correctly.
- Recycling or switching selection cannot leave stale inspector/outline values or commit to the wrong node.
- The visual/accessibility review has no unresolved critical issue, no platform-dependent Unicode action icon, and no decorative fake control.
- All product requirements except the stage-14 scale budgets are met in the running application.

## Out of scope

- Whole-project persistence/search/model performance redesign (stage 14)
- Import from proprietary writing formats
- Cloud collaboration or sync

## Handoff

Create `docs/handoffs/13-production-workflows-and-visual-system.md`. Include the live editor/QML topology, command/view map, drag/drop contract, metadata/count/compile surfaces, token/icon inventory, screenshots and accessibility results, platform quirks, and deferred scale concerns.
