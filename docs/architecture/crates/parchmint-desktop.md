# `parchmint-desktop`

## What it does

`parchmint-desktop` is the executable users launch. It creates the production
services, starts the native desktop environment, and hands control to the UI.
Its job is to assemble the crates into one running application.

The binary manages process startup and shutdown. Project rules, editor state,
durable files, native platform behavior, and UI widgets live in their own
crates.

## How it works

```text
operating-system launch
  -> parse launch intent
  -> create application-wide services
  -> resolve initial appearance
  -> create the UI
  -> show the launcher
  -> route project opens to the correct window
```

A process supports several project windows for different projects. One open
project has exactly one session and one window. Opening it again from the same
process focuses that window. Each window is registered with its project session
so a UI action uses the correct session. A separate process that tries to open
the same project receives the safe locked-project result from the project
filesystem service.

Startup sends file and operating-system work to background tasks. When the user
closes a project, its window stays open until the final save finishes or reports
an error.

## Public API

The executable exports no Rust library API. The following types let tests run
the startup code without building an installer:

```rust
pub struct LaunchRequest {
    pub project: Option<RequestedProjectPath>,
}

pub struct DesktopBootstrap {
    pub application: ApplicationServices,
    pub project_filesystem: ProjectFilesystemServices,
    pub preferences: Arc<dyn PreferenceService>,
    pub appearance: Arc<dyn AppearanceService>,
    pub platform: PlatformServices,
    pub ui: Box<dyn DesktopUi>,
}

impl DesktopBootstrap {
    pub fn production() -> Result<Self, StartupError>;
    pub fn run(self, request: LaunchRequest) -> Result<ExitCode, StartupError>;
}

fn main() -> ExitCode {
    DesktopBootstrap::production()
        .and_then(|app| app.run(LaunchRequest::from_environment()))
        .unwrap_or_else(StartupError::report_and_exit)
}
```

`DesktopBootstrap` receives ready-to-use services from the crates that implement
them.

## Implementation

```rust
async fn start(
    mut app: DesktopBootstrap,
    request: LaunchRequest,
) -> Result<ExitCode> {
    let preferences = app.preferences.load().await?;
    let system = app.platform.system_appearance().await?;
    let theme = app.appearance.initialize(&preferences, system)?;
    let sessions = ProjectSessionRegistry::new();

    let startup = UiStartup {
        appearance: theme,
        sessions,
        initial_project: request.project,
    };
    let ports = app.ui_ports();
    app.ui.run(startup, ports)
}
```

Startup registers the first project session and window after every service is
ready. If startup fails earlier, it leaves no partial session or window. Each
window and project session has a generation number. The application ignores a
background result when its generation belongs to a closed window or session.
