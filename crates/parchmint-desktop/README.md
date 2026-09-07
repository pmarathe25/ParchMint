# `parchmint-desktop`

This crate builds the `parchmint` executable and connects the production UI,
editor, storage, and platform services.

## Interface

`DesktopBootstrap::production` assembles the service graph. `run` loads
preferences, opens the launcher or requested project, and enters the UI driver.
The executable accepts a project path, `--help`, `--version`, or the `capture`
command for a native screenshot. See [main.rs](src/main.rs) for capture options.

Tests inject services through `DesktopBootstrap::new` or use
`production_with_controls`. The `interaction-harness` feature enables
`DesktopInteractionHarness`, which drives the production widgets and services
with controlled operating-system responses.

## Implementation

[production](src/production.rs) separates service construction, project-session
ownership, platform callbacks, and UI-facing adapters. Each open project has one
write lease, session, and window. Opening it again focuses that window. Another
process must acquire the same project lock before it can write.

Startup performs file and service work away from the UI loop. Each asynchronous
result belongs to an exact window and session generation. Close keeps the window
open until its final save succeeds; failures leave the draft available.

Normal startup disables fault controls and observation collection. Tests enable
them explicitly; disabled controls take no locks and do not construct observations.

## Diagnose a reported failure

The default `diagnostics` feature writes warnings and errors to a bounded local
log. Debug builds and harness builds also record traces and timing summaries.
See [parchmint-diagnostics](../parchmint-diagnostics/README.md) for overhead and limits.

The log is `logs/parchmint-debug.log` below the application-data directory:

- Linux: `$XDG_DATA_HOME/parchmint`, or `~/.local/share/parchmint`.
- macOS: `~/Library/Application Support/ParchMint`.
- Windows: `%LOCALAPPDATA%/ParchMint/Data`.

Error dialogs show a short explanation. The local log retains the technical
cause without document text. Build with `--no-default-features` to omit logging.
