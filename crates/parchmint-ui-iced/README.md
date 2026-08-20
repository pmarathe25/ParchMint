# `parchmint-ui-iced`

## What it does

`parchmint-ui-iced` implements the `DesktopUi` contract with `iced`. It owns
the `iced` event loop and creates application windows. It draws those windows,
collects input, stores temporary UI state, and starts background tasks. Other
crates handle operating-system features, rich-text editing, and project data.

`iced` types stay in this crate and `parchmint-editor-iced`. Domain,
application, UI, editor, and platform APIs use ParchMint-owned values instead of
GUI widgets, events, tasks, subscriptions, or window handles.

The source code has a layered layout: `native` drives the event loop and
creates windows; `project_runtime`, `project_workspace`, and `editor_workspace`
own the project-facing state (hierarchy model, selection normalization, drag
validation, document mounting); `iced_project_surface` and `iced_editor_surface`
render the screens (Cards, comments, Inspector, search, History, deletion,
settings, styles, launcher, export, recovery); `async_service_feeds` backs
spellcheck, search, and export; `focus`, `right_click`, and
`stationary_tooltip` provide interaction plumbing; `design_tokens` and `icons`
build the private theme; and `visual_verification` produces the ref-test
screenshots.

Project-facing views share one hierarchy selection model. Explorer and Cards
therefore normalize ancestor/descendant selections, preserve explicit tree
order, and validate the same drag destinations before emitting an application
command. Cards activation mounts the document through the existing editor
workspace instead of creating a second editor state path.

## How it works

```text
widget, editor, platform, or service event
  -> driver Message routed by raw Iced window id
  -> update presentation state
  -> start required asynchronous work
  -> receive a typed completion
  -> render the affected window
```

The update function changes only small values already in memory. It starts a
background task for file access, History, search, export, spellcheck, and
recovery work.

The mounted editor handles keystrokes directly. It can draw the result while
serialization, file access, and analysis continue in the background.

Background UI tasks use detached native threads. Closing a window or making a
request stale drops its receiver and suppresses publication, but a native call
already running continues to completion. Search and export cancel only where
their service APIs expose cancellation; workers owned by a service that joins
them retain that separate lifecycle. The driver records bounded aggregate
worker duration, active/peak concurrency, and accepted or dropped delivery for
operations; it does not queue, limit, or otherwise schedule them.

The editor workspace keeps one presentation record per pane and one local
search/decorations record per mounted view. Tabs identify documents, while the
mounted `ViewId` identifies independent cursor, selection, scroll, focus, and
local-search state. Switching a tab advances that view's mount generation.
Toolbar and undo messages resolve to the last focused editor view and emit
adapter-facing effects; toolbar focus does not replace the editor target.

Editor completions carry the exact task, request number, view, document,
document revision, and mount generation. A result is ignored when any field
differs from the live request. This prevents spellcheck, comments, or word-count
work from crossing an edit or tab switch even when the UI reuses the same
mounted view.

Project completions use the same rule at project scope. Their ticket contains
the project session, typed task, request number, and captured project revision.
Starting a newer task invalidates the older task in that family. Replacement,
restore, and recovery results also require their captured revision to remain
current. Save and export results may finish for an older captured revision:
the UI reports the completed frontier while leaving later edits dirty or the
export tied to its immutable source revision.

Global Search streams results only for its live query generation. Replacement
review is a middle-pane hierarchy whose group and document controls derive
selected, unselected, or indeterminate state from match leaves. Applying the
selection emits candidates for application-owned revision and text
revalidation; the UI never edits document bodies itself.

History restore stays whole-project and requires an explicit confirmation.
Recently Deleted emits complete-subtree restoration at the former location or
the section-root fallback. Appearance emits an application-preference action,
not a project command. Export, save, close, and recovery retain their failure
state without claiming success or discarding recovery data.

When it creates or destroys an `iced` project window, this crate calls a
ParchMint-owned lifecycle callback. The desktop's private concrete integration
uses that callback to register or retire the `WindowCapability` with
`parchmint-platform-native`. Raw Iced handles stay inside the driver; the
platform API receives only the capability for dialogs, menus, clipboard work,
and similar requests.

## Interface

The desktop crate starts the native driver with ParchMint-owned values and
callbacks. No `iced` object crosses the crate boundary:

```rust
pub fn run_native_desktop(
    startup: NativeDesktopStartup,
) -> Result<(), NativeDesktopError>;
```

The native driver uses an Iced daemon because the launcher and each project use
separate native windows. Its callbacks route launcher project paths through the
desktop session registry and route project close requests through final save.
The crate uses `iced::Task`, `iced::Subscription`, and raw Iced window IDs only
inside the driver.

Editor geometry fixtures use ParchMint-owned logical rectangles. The private
Iced fixture surface consumes the same workspace state and is tested with the
pinned headless tiny-skia renderer, so pane focus and Light/Dark composition do
not require a native display.

Project fixtures use the maintained Screen Catalog fixture IDs for the Editor
workspace (single- and dual-pane), Cards, Global Search, History, Recently
Deleted, Settings Appearance, Export, and error recovery. Each surface is
rendered headlessly in Light and Dark from the same presentation state and
semantic Iced theme palette.

## Implementation

The native driver is a flat message hub. Completion and per-window variants
name the raw Iced window they belong to, and each typed completion carries the
request ticket that produced it:

```rust
enum Message {
    RuntimeEvent {
        window: window::Id,
        event: Event,
        accelerator_fallback: bool,
    },
    ProjectSurface {
        window: window::Id,
        message: ProjectSurfaceMessage,
    },
    SpellcheckFinished {
        window: window::Id,
        ticket: NativeSpellcheckTicket,
        result: Result<SpellcheckResult, String>,
    },
    SearchFinished {
        window: window::Id,
        ticket: ProjectTaskTicket,
        result: Result<Vec<SearchBatchResult>, String>,
    },
    // Export, History, save, clipboard, and recovery completions follow the
    // same flat shape, one variant per typed completion.
}

fn update(&mut self, message: Message) -> iced::Task<Message> {
    // resolve the raw Iced window id, then hand the message to the Shell or
    // ProjectWorkspace, which accept only live-capability or exact-ticket
    // messages
}
```

An appearance message is applied to every registered window in stable window-ID
order. The UI finishes one numbered snapshot before it accepts the next one.

Each background result names the window, project session, generation, and
revision that requested it. The UI ignores the result if any of those values is
no longer current. A completed file write stays completed even when the window
has since closed. While a final save is running, the window remains open.
