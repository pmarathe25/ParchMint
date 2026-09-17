# `parchmint-ui-iced`

**Purpose:** Own the Iced event loop, windows, widgets, and temporary UI state.
Project changes, saves, native actions, and rich-text editing use their owning
crate's interfaces through [ui-api](../parchmint-ui-api/README.md).

## Interface

`run_native_desktop(NativeDesktopStartup)` runs the application. Startup and
lifecycle callbacks use ParchMint values; Iced events and window IDs stay internal.
`interaction-harness` exposes `NativeDesktopHarness` for headless input;
`visual-verification` enables deterministic `capture_visual` PNGs. Normal desktop
builds enable neither feature.

## Source map

| File | Responsibility |
| --- | --- |
| [native.rs](src/native.rs) | Route input and completions to the owning window; apply editor commands against the originating view and selection |
| [project_workspace.rs](src/project_workspace.rs) | Explorer, Overview, comments, History, search, settings, export, and recovery presentation; shared tree selection and move validation |
| [editor_workspace.rs](src/editor_workspace.rs) | Tabs, panes, local search, and comment drafts |
| [project_runtime.rs](src/project_runtime.rs) | Resolve IDs from current snapshots and call application ports |
| [iced_project_surface.rs](src/iced_project_surface.rs), [iced_editor_surface.rs](src/iced_editor_surface.rs) | Render workspace state |
| [cards_layout.rs](src/cards_layout.rs) | Measure wrapped card content for grid layout and virtual scrolling |
| [components.rs](src/components.rs), [design_tokens.rs](src/design_tokens.rs) | Shared controls and Iced theme mapping |
| [async_service_feeds.rs](src/async_service_feeds.rs) | Search, History previews, and recovery results |
| [history_project.rs](src/history_project.rs) | Compare checkpoint data with the current project on a worker |
| [native/worker_pool.rs](src/native/worker_pool.rs) | Four workers and at most 128 queued blocking jobs; overload errors without blocking input |

Export and restore use desktop-supplied workflow ports. Services with their own
workers retain their own queue limits.

## Drafts and delayed results

Empty tabs remain local UI state. First edits promote them to protected drafts,
retaining input until the editor mounts. Explicit Save chooses a name and location;
closing a changed draft offers Save, Don’t Save, and Cancel. The internal Unfiled
root is never a navigation section.

Comment drafts retain their originating pane, mount generation, revision, and
selection; failed submission keeps the text. The comments panel indexes threads using
live anchors, including unsaved comments. The anchored popover owns editing.

Style fields keep local drafts across snapshots. Enter or Apply commits one
property; invalid values keep their draft and show an error. Unchanged values
request no save. Dictionary controls retain input until returned words confirm
the change; project words use commands and global words use preference ports.

Shared document sessions serve both panes with independent view state. Tab
switches advance mount generations. Delayed results must match their target;
loaded bodies merge without replacing newer outline state. Project revisions
validate structure, while document operations validate document revisions.
Recovery recording can advance document state independently of outline changes.

Project mutations and saves serialize. Save results acknowledge captured revisions
and leave later edits dirty. Stale native completions cannot update closed windows;
close waits for the final save. History resolves IDs from the selected manifest
and compares live drafts, structure, comments, dictionary, and styles.

Overview has independent section and disclosure state. Group headings toggle their
contents; synopsis and metadata become editable when clicked. While dragging, a
floating card follows the cursor and a temporary tree previews its placement.
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
