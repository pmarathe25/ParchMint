# Stage 13 handoff: production workflows and visual system

Status: implementation delivered and verified on the available Linux/Qt 6.8.3 host. It does not claim the platform accessibility/screenshot or permanent-trash evidence required for the full Stage 13 acceptance gate.

Verification working tree: based on `044ccf9fce69b6702c6230c5eb6f93ed1a3ebf6e` with the changes in this handoff; no persisted-format migration or ADR was added.

## State and topology

- No Markdown, TOML, recovery, asset-catalog, or compile-preset format version changed.
- `PaneHost` owns its `TextArea`, `EditorAdapter`, source editor, formatting/find state, and pane-local undo/cursor lifetime. It continues to publish the exact Stage 12 Markdown body to `updatePaneBody`; all transitions retain the bounded flush contract.
- Editor is the default pane view and Source is explicit. Source entry flushes and validates canonical Markdown; diagnostics retain an invalid source buffer and a valid commit returns to the editor.
- `EditorAdapter` now projects underline, groups selected list blocks into one list, applies named next styles after thematic/page/opaque insertion, makes protected replacement consistent for insertion/paste, and sanitizes rich HTML into allowed local semantic runs. Remote images, scripts, frames, embedded objects, SVG, event attributes, and unsafe link schemes are dropped.
- `CardsModel` filters built-in roots before the card grid receives rows. `setPaneView` makes Editor/Outline/Cards a persisted Rust pane preference without replacing the live document reference.

## Commands and workflows

| Command | Shortcut | Result |
| --- | --- | --- |
| `view.editor` | Ctrl/Cmd+1 | Focused pane: Editor |
| `view.outline` | Ctrl/Cmd+2 | Focused pane: Outline |
| `view.cards` | Ctrl/Cmd+3 | Focused pane: Cards |
| Binder F2 / Enter | F2 / Enter | Rename / open current non-root item |

- Binder drag payload: `application/x-parchmint-node-id`; upper/lower/centre affordances map to typed `before`/`after`/`inside`. The domain remains responsible for root and cycle rejection. `OutlineModel` exposes the same MIME type and `moveNode` contract.
- Inspector and Outline buffers are keyed by node ID and commit only to the source node. `include-in-compile` writes canonical metadata and refreshes the projection.
- Row word counts take a live session body when open and canonical bytes otherwise, at a projection refresh. Pane word/character display is local QML work and no longer makes a Rust FFI full-text call for every keypress.
- Export selects the preset actually passed to the background compile worker. Renaming persists the preset; renaming the synthetic default creates a canonical Manuscript preset. Existing progress, cancellation, overwrite confirmation, and warning status remain in place.

## Tokens and icons

- `DesignTokens` now includes neutral base/surface/raised/overlay layers, text hierarchy, outline/accent/danger, type/spacing/radius/elevation/motion scales, and icon-grid values.
- Bundled SVG inventory: `binder`, `inspector`, `pin`, `search`, `close`, `chevron`, `document`. Header/pane actions have accessible names and tooltips and no action button uses a Unicode glyph.
- Editor chrome is neutral, full-bleed around an approximately 72-character reading measure with a calm placeholder and no bright border.

## Verification

Linux, repository-local Qt 6.8.3, Rust 1.97.1:

- `cargo fmt --all -- --check`: passed.
- `cargo test -p parchmint-app --offline`: 31 passed, 2 existing manual measurements ignored.
- `cargo check -p parchmint_bridge --offline`: passed; established GNU ld.bfd fallback warning remains.
- Fresh `build-stage13` CMake build: passed.
- `ctest --test-dir build-stage13 --output-on-failure`: 4/4 passed (smoke, lifecycle/recovery, editor adapter, outline model).
- Direct offscreen smoke and lifecycle smoke: passed after the final QML resource build.
- `qmllint` target exits successfully. It retains existing resource-only singleton and unqualified delegate-access advisories, but no QML load failure occurs in the offscreen smoke.

New adapter tests cover protected next-style boundaries/deletion, paste sanitization, safe links, underline, and grouped lists. Outline tests cover typed drag payload/move. Workspace tests cover view switching and word-count projection.

## Remaining acceptance follow-up

- There is a durable move-to-trash confirmation but no dedicated trash browser/restore surface or permanent empty-trash command. Add a canonical purge lifecycle instead of a decorative button.
- Windows/macOS/X11/Wayland screenshots, high-contrast, reduced-motion, keyboard-only, and screen-reader focus evidence require real platform runs.
- Counts scan at projection refresh and are not the Stage 14 incremental 10k-node solution.
- The current editor preserves the exact Stage 12 Markdown body while using `EditorAdapter` for semantic formatting. A complete bidirectional Markdown-AST/rich-document projection is still needed before every pre-existing Markdown construct is visually rendered WYSIWYG.

Recommended next task: implement the canonical permanent-trash lifecycle and bidirectional Markdown semantic projection while retaining Stage 12 source/flush/conflict safety.
