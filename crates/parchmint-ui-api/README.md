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

The application API owns user commands and observable application state; the
UI contract groups the interfaces a desktop UI needs, defines per-session
project ports and their authorization, and starts the UI runtime.

## Interface

```rust
pub struct ExitCode(i32); // SUCCESS, new(value), value()

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

pub struct ProjectSessionRegistry { /* .. */ }

impl ProjectSessionRegistry {
    pub fn new() -> Self;
    pub fn register(&mut self, session_id: u64) -> ProjectSessionCapability;
    pub fn retire(&mut self, capability: ProjectSessionCapability) -> bool;
    pub fn is_current(&self, capability: ProjectSessionCapability) -> bool;
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

pub fn apply_appearance_events(
    snapshots: &[ThemeSnapshot],
    windows: &[WindowCapability],
    apply: impl FnMut(WindowCapability, ThemeSnapshot),
);
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

The interface also includes the session-scoped project ports:
`ProjectSnapshotQuery` (with `snapshot()` and the off-loop
`snapshot_for_export()`), `ProjectSaveStatus`, `ProjectPersistencePort`,
`ProjectWorkflowPort`, `ProjectHistoryMaintenancePort`, and
`ProjectExportPort`, alongside `ProjectSessionAuthority` and
`StaleProjectSession`. `ProjectUiPorts` groups them behind one exact session
capability; `access()` re-checks the generation and returns a short-lived
`ProjectUiAccess` of typed borrows, which callers reacquire for each user
action or asynchronous task.

## Implementation

This crate contains shared types, the `DesktopUi` trait, the project-session
registry and authority, and the session-scoped port access. It has no
production UI implementation. `parchmint-ui-iced` implements the trait, and
`parchmint-desktop` selects that implementation when it builds the production
service graph.

A replacement GUI adds another implementation crate and changes the production
constructor in `parchmint-desktop`. Domain, application, storage, history,
search, export, spellcheck, and platform contracts remain unchanged.
