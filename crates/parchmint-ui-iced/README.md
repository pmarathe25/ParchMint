# `parchmint-ui-iced`

**Purpose:** Own the Iced event loop, windows, widgets, and temporary UI state.
Project changes, saves, native actions, and rich-text editing use their owning
crate's interfaces through [ui-api](../parchmint-ui-api/README.md).
Use the [visual language](../../docs/design-language.md) for shared surfaces,
controls, typography, spacing, and icons.

## Interface

`run_native_desktop(NativeDesktopStartup)` runs the application. Startup and
lifecycle callbacks use ParchMint values; Iced events and window IDs stay internal.
`interaction-harness` exposes `NativeDesktopHarness` for headless input;
`visual-verification` enables deterministic `capture_visual` PNGs. Normal desktop
builds enable neither feature. The desktop selects Iced's GPU renderer when
available and keeps `tiny-skia` as a fallback. Headless drivers select
`tiny-skia` for deterministic captures.

## Source map

| File | Responsibility |
| --- | --- |
| [shortcut_router.rs](src/shortcut_router.rs) | Resolve every shipped or reassigned shortcut from the shared command catalog; adapt text editing commands to the focused widget |
| [native.rs](src/native.rs) | Route input and completions to the owning window; apply editor commands against the originating view and selection |
| [project_workspace.rs](src/project_workspace.rs) | Explorer, Overview, comments, History, search, settings, export, and recovery presentation; shared tree selection and move validation |
| [editor_workspace.rs](src/editor_workspace.rs) | Tabs, panes, local search, and comment drafts |
| [project_runtime.rs](src/project_runtime.rs) | Resolve IDs from current snapshots and call application ports |
| [iced_project_surface.rs](src/iced_project_surface.rs), [iced_editor_surface.rs](src/iced_editor_surface.rs) | Render workspace state |
| [cards_layout.rs](src/cards_layout.rs), [card_frames.rs](src/card_frames.rs) | Card geometry, text measurement, and enclosing group outlines |
| [components.rs](src/components.rs), [design_tokens.rs](src/design_tokens.rs) | Shared controls and Iced theme mapping |
| [async_service_feeds.rs](src/async_service_feeds.rs) | Search, History previews, and recovery results |
| [history_project.rs](src/history_project.rs) | Compare checkpoint data with the current project on a worker |
| [native/worker_pool.rs](src/native/worker_pool.rs) | Four workers and at most 128 queued blocking jobs; overload errors without blocking input |

Export and restore use desktop-supplied workflow ports. Services with their own
workers retain their own queue limits.

## Drafts and delayed results

New Explorer and Overview items stay as local name placeholders until Enter.
Escape removes the placeholder without a project mutation. Confirming submits
one creation command with its final title; snapshot reconciliation selects the
created node. Unrelated snapshots preserve an unconfirmed name.

Empty tabs remain local UI state. First edits promote them to protected drafts,
retaining input until the editor mounts. Explicit Save chooses a name and location;
closing a changed draft offers Save, Don’t Save, and Cancel. The internal Unfiled
root is never a navigation section. Discarded unfiled drafts do not create
Recently Deleted entries; undiscarded drafts remain available to crash recovery.
Discard uses the ordinary mutation/save lane, so it is durable before the UI
reports it as saved.

Comment drafts retain their originating pane, mount generation, revision, and
selection; failed submission keeps the text. The comments panel indexes threads using
live anchors, including unsaved comments. The anchored popover owns editing.

Styles and metadata managers stage changes locally across snapshots. Save applies
the staged definitions; Cancel asks before discarding only when the draft differs
from its starting state. Built-in styles have
a separate Reset to defaults action, while custom styles can be deleted. Invalid
property values keep the dialog open. Repeated edits to a definition coalesce
before Save. Dictionary controls retain input until returned words confirm
the change; project words use commands and global words use preference ports.

Shared document sessions serve both panes with independent view state. Tab
switches advance mount generations. Delayed results must match their target;
loaded bodies merge without replacing newer outline state. Project revisions
validate structure, while document operations validate document revisions.
Recovery recording can advance document state independently of outline changes.

Project mutations and saves serialize. Save results acknowledge captured revisions
and leave later edits dirty. Stale native completions cannot update closed windows;
close waits for the final save. History resolves IDs from the selected manifest
and compares live drafts, structure, comments, dictionary, and styles. Document
History opens from the clicked tab or outline entry, independently of editor
focus. Document restores replace that document's sessions while retaining other
documents' undo.

Overview has independent section and disclosure state. Groups share the document
card surface and enclose their nested contents. Group frames follow allocated row
heights during animations and remain continuous across virtualized windows.
Creation slots use muted icons, transparent fills, and dashed borders.
Group headings toggle their children; group synopsis and metadata remain fully
visible. Document cards place a compact metadata grid below the synopsis.
Collapsed cards show fields marked visible, including dashes for empty
values; dragging between the named Settings sections changes that visibility with
the same threshold, floating copy, and drop indicator as cards. New fields start
hidden. Expanding a card reveals all details by growing vertically. Cards expand
independently with animated height allocation. Only the viewport and a short
overscan are mounted, and text measurements are cached across redraws. While
dragging, a floating card follows the cursor and a temporary tree previews its
placement.
Explorer keeps its rows stationary and marks the destination. Window-wide release
commits; cancellation discards the preview.
Tabs can move between panes, and either pane accepts Explorer document drops.

Spelling requests contain at most 4,096 scalars around a view's caret, including
large paragraphs. Cancellation is silent; service errors preserve their cause.
The [UI driver](../../tests/parchmint-ui-driver/README.md) checks these paths through
rendered controls and delayed completion delivery.

## Motion

One deadline subscription handles pending spelling, Inspector edits, layout writes,
notifications, autosave, and recovery. Clean windows have no polling timer;
unchanged recovery projections are skipped. Animations schedule their own frames.

`motion.rs` provides interruptible pane resizing, disclosures, card/tab reflow,
and small entrances. Widgets request redraws only during transitions; typing and
pointer dragging remain immediate. Pane children stay mounted during focus and
sidebar changes. Card drop targets use the destination layout while the cards
move, avoiding feedback between animation and drag targeting. Reduced motion
is an application preference. Headless workflows and captures settle motion;
frame-level tests cover intermediate geometry and input. The interaction harness
can also advance a controlled frame clock for full-workspace motion captures.
Incoming panes retain readable text widths; moving cards draw as complete layers,
and drop placeholders stay at their destinations.

The pencil icon pack lives in `assets/pencil/`. The Iced wrapper caches SVG
handles and applies one muted brown per theme. Imported vectors use centered optical
bounds and six translucent contours, reducing both parsing work and inconsistent sizing.
`scripts/normalize-pencil-icons.py` reproduces normalization from the original packs. The corrected additions pack supplies
the four inward arrows shared by card collapse and Exit focus. The main `parchmint-brand.svg` remains separate.

## Keyboard commands and branding

`parchmint-preferences::shortcut_commands` defines command IDs, labels, and
platform defaults and contexts. Editor and Overview commands may share a binding;
global commands conflict with both. Persisted overrides replace or disable entries in that catalog.
`shortcut_router` resolves both defaults and overrides before child widgets handle
keys, then publishes the same semantic command to `NativeDesktop`. Editing
commands use one focused-widget adapter for text fields, scratch editors, and
mounted documents; unconsumed commands fall back to the current project surface.
Disabled defaults are consumed so a widget cannot accidentally execute the old
binding. Settings records combinations, validates conflicts, and applies changes
only after durable preference storage succeeds. Context menus use the effective
bindings for their labels.

`assets/parchmint-brand.svg` is a transparent vector interpretation of the supplied typewriter artwork. The
navigation rail and launcher render the same artwork without an external path.

Location rows are shared by the draft-save tree and History scope menu. History
group filtering keeps only changes whose path includes that group. Notifications
occupy a bottom-right overlay and do not change workspace geometry. Splitter
release and window departure end resizing before hierarchy drag routing.

Selection actions appear below highlighted manuscript text. The Link dialog offers
Web and Document destinations; Document uses the shared hierarchy row controls.
Internal links store stable document IDs and resolve through the project navigation
path, so renaming and moving documents does not break links. Global search groups
can collapse independently without discarding matches or changing result counts.

Overview caches measured row layouts, visible IDs, and cumulative row offsets
across scrolling and hover redraws. Project edits, hierarchy changes, width
changes, and detail expansion invalidate the cache. Finding a viewport now uses
the cached offsets, and mounting reads only its visible IDs. Only visible rows
and a small overscan are mounted. Cards offer expansion only
when their full title, synopsis, or metadata exceeds the compact bounds.
The native Overview uses the shell's current dimensions to construct its grid
before the first paint, so navigation does not wait for a second layout pass to
show the selected section and card fields.
The large-outline unit test has an opt-in release timing report for cached
viewport layout and repeated drag previews:

```console
PARCHMINT_MEASURE_CARDS=1 cargo test --release --locked -j 1 -p parchmint-ui-iced --lib cards_window_bounds_a_large_outline_and_preserves_selected_rows -- --nocapture
```

These timings cover state updates and view construction, not native frame
rendering. Check card dragging in the release app as well.

Style controls enumerate the domain `StyleProperty` schema, which also generates
`StyleProperties` and inheritance merging. Add properties there rather than a
second UI list. Controls display resolved values; Reset removes the local override.
Metadata applicability and text-kind choices likewise use the domain enums.
Editor popovers use `anchored_popover`: fixed document anchors, viewport clamping,
and a continuous pointer region between the anchor and card.
