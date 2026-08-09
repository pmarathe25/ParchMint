# `parchmint-ui-api`

## What it does

This crate defines the contract between the desktop executable and a graphical
interface. It lets the executable start a UI without importing `iced` or any
other GUI framework.

The contract contains startup data, the application and platform services the
UI may call, and the result returned when the UI exits. It contains no widgets,
layout code, renderer types, event-loop types, or raw window handles.

## How it works

```text
parchmint-desktop
  -> builds UiStartup and UiPorts
  -> calls DesktopUi::run
       -> parchmint-ui-iced in the production build
       -> a small fake UI in startup tests
```

The application API owns user commands and observable application state. The
UI contract only groups those interfaces for a desktop UI and starts the UI
runtime.

## Public API

```rust
pub trait DesktopUi: Send {
    fn run(
        self: Box<Self>,
        startup: UiStartup,
        ports: UiPorts,
    ) -> Result<ExitCode, UiError>;
}

pub struct UiStartup {
    pub appearance: ThemeSnapshot,
    pub sessions: ProjectSessionRegistry,
    pub initial_project: Option<RequestedProjectPath>,
}

pub struct UiPorts {
    pub application: ApplicationServices,
    pub editor: Arc<dyn EditorAdapter>,
    pub spellcheck: Arc<dyn SpellcheckService>,
    pub platform: PlatformServices,
    pub preferences: Arc<dyn PreferenceService>,
    pub appearance: Arc<dyn AppearanceService>,
    pub workspace_state: Arc<dyn WorkspaceStateStore>,
}
```

All fields use ParchMint-owned types or interfaces from other contract crates.
A GUI implementation converts them to its own tasks, subscriptions, windows,
and events internally. It registers any concrete window with the concrete
platform adapter through private integration code. That registration does not
belong to this public contract.

`ProjectSessionRegistry` issues a ParchMint capability for each logical project
session. Recreating a session advances its generation, which makes delayed work
that holds the previous capability stale.

The UI applies each numbered appearance event to every registered
`WindowCapability` in stable logical window-ID order before it applies the next
event. The callback receives the full capability, including its generation, so
the native adapter can reject a closed or recreated window without receiving a
widget or native-window type.

## Implementation

This crate contains shared types and the `DesktopUi` trait. It has no production
UI implementation. `parchmint-ui-iced` implements the trait, and
`parchmint-desktop` selects that implementation when it builds the production
service graph.

A replacement GUI adds another implementation crate and changes the production
constructor in `parchmint-desktop`. Domain, application, storage, history,
search, export, spellcheck, and platform contracts remain unchanged.
