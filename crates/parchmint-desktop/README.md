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

## Diagnosing failures

The production application writes a bounded, rotating debug log at
`logs/parchmint-debug.log` under its application data directory. The log
truncates before it exceeds 1 MiB. It records project and editor effect starts,
completions, ignored stale completions, and the technical causes of
user-visible failures. It records action names, session identifiers, and
revision numbers; it does not record document text.
Each line includes a timestamp and in-process sequence number, so the most
recent failure can be traced back through the preceding operations.

The usual path is:

- Linux: `$XDG_DATA_HOME/parchmint/logs/parchmint-debug.log` (or `~/.local/share` when `XDG_DATA_HOME` is unset)
- macOS: `~/Library/Application Support/ParchMint/logs/parchmint-debug.log`
- Windows: `%LOCALAPPDATA%/ParchMint/Data/logs/parchmint-debug.log`

Authors see a consistent ParchMint error modal for action failures; its detail
is safe to show to users, while the precise backend cause is retained in this
local log.

## Interface

The package builds the `parchmint` executable. The following library types let
tests run the same startup path with injected services and a deterministic UI
driver:

```rust
pub struct LaunchRequest {
    pub project: Option<RequestedProjectPath>,
}

pub struct DesktopBootstrap {
    pub application: ApplicationServices,
    pub project_filesystem: Arc<dyn ProjectFilesystemService>,
    pub preferences: Arc<dyn PreferenceService>,
    pub appearance: Arc<dyn AppearanceService>,
    pub platform: PlatformServices,
    pub ui: Arc<dyn DesktopUi>,
}

impl DesktopBootstrap {
    pub fn production() -> Result<Self, StartupError>;
    pub fn run(self, request: LaunchRequest) -> Result<ExitCode, StartupError>;
    pub fn run_native_capture(
        self,
        request: LaunchRequest,
        capture: NativeCaptureRequest,
    ) -> Result<ExitCode, StartupError>;
}

fn main() -> std::process::ExitCode {
    let exit = parse_process_request(std::env::args_os())
        .and_then(|request| match request {
            ProcessRequest::Run(launch) => DesktopBootstrap::production()?.run(launch),
            ProcessRequest::Capture { launch, capture } => {
                DesktopBootstrap::production()?.run_native_capture(launch, capture)
            }
        })
        .unwrap_or_else(|error| error.report_and_exit());
    process_exit_code(exit)
}
```

The binary accepts an optional project path as its first argument, or the
`capture` subcommand, which performs one verification-only native render
capture and exits unless `--keep-open` is given. Capture runs share the same
production startup graph and override the resolved appearance for output.

`DesktopBootstrap` receives ready-to-use services from the crates that implement
them.

## Implementation

`DesktopBootstrap::run` completes asynchronous preference and system-appearance
startup, initializes the UI, routes the optional launch path, and then enters
the injected UI driver. Production selects the native Iced driver. That driver
opens a launcher window, opens one native window for each registered project,
and blocks in the Iced event loop until every window closes.

Startup registers the first project session and window after every service is
ready. If startup fails earlier, it leaves no partial session or window. Each
window and project session has a generation number. The application ignores a
background result when its generation belongs to a closed window or session.
Project-window close requests run through the final-save lifecycle before Iced
destroys the native window.
