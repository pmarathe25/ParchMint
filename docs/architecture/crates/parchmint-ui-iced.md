# `parchmint-ui-iced`

## What it does

`parchmint-ui-iced` implements the `DesktopUi` contract with `iced`. It owns
the `iced` event loop and creates application windows. It draws those windows,
collects input, stores temporary UI state, and starts background tasks. Other
crates handle operating-system features, rich-text editing, and project data.

`iced` types stay in this crate and `parchmint-editor-iced`. Domain,
application, UI, editor, and platform APIs use ParchMint-owned values instead of
GUI widgets, events, tasks, subscriptions, or window handles.

The source code is organized into nine groups: primitives and feedback;
workspace shell; hierarchy and Cards; editor workspace; comments and Inspector;
search and replace; History and deletion; settings, spellcheck, and styles; and
launcher, export, and recovery.

## How it works

```text
widget, editor, platform, or service event
  -> UiMessage scoped to a window and project session
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

When it creates an `iced` window, this crate uses private concrete integration
code to register the window with `parchmint-platform-native`. The integration
keeps raw window handles inside the two concrete adapters. The platform API
receives only the resulting `WindowCapability` for dialogs, menus, clipboard
work, and similar requests.

## Public API

The binary starts the UI through the contract in `parchmint-ui-api` and receives
no `iced` objects back:

```rust
pub struct IcedDesktopUi;

impl DesktopUi for IcedDesktopUi {
    fn run(
        self: Box<Self>,
        startup: UiStartup,
        ports: UiPorts,
    ) -> Result<ExitCode, UiError>;
}
```

The public API uses types defined by ParchMint. The crate uses `iced::Task` and
`iced::Subscription` internally.

Editor geometry fixtures use ParchMint-owned logical rectangles. The private
Iced fixture surface consumes the same workspace state and is tested with the
pinned headless tiny-skia renderer, so pane focus and Light/Dark composition do
not require a native display.

## Implementation

```rust
enum Message {
    Input(ScopedInput),
    Editor(ScopedEditorEvent),
    Platform(ScopedPlatformEvent),
    Appearance(ThemeSnapshot),
    Completed {
        scope: TaskScope,
        generation: u64,
        result: OperationResult,
    },
}

fn update(model: &mut Model, message: Message) -> iced::Task<Message> {
    if !model.accepts(&message) {
        return iced::Task::none();
    }

    let commands = model.apply(message);
    commands.into_iced_tasks()
}
```

An appearance message is applied to every registered window in stable window-ID
order. The UI finishes one numbered snapshot before it accepts the next one.

Each background result names the window, project session, generation, and
revision that requested it. The UI ignores the result if any of those values is
no longer current. A completed file write stays completed even when the window
has since closed. While a final save is running, the window remains open.
