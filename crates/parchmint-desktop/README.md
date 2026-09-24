# `parchmint-desktop`

**Purpose:** Build the `parchmint` executable and connect UI, editor, storage,
and platform services into one production application.

## Interface and lifecycle

`DesktopBootstrap::production` assembles services. `run` loads preferences,
opens the requested or most recent project, and enters the UI driver. First launch opens **My Writing** in the application-data directory. Failed
reopening falls back to that workspace; the project title opens the project menu. The executable
accepts a project path, `--help`, `--version`, and `capture` for native screenshots.
See [main.rs](src/main.rs) for arguments.

[production.rs](src/production.rs) separates service construction, project-session
ownership, platform callbacks, and UI adapters. Each open project has one write
lease, session, and window; opening it again focuses that window. Another process
must acquire the same project lock to write.

Startup service work runs away from the UI loop. Async results carry their window
and session generation. Closing waits for the final save; failure leaves the
window and draft open with retry, keep-working, and exit-without-saving choices.
Exiting without saving releases the project lease and preserves existing recovery
records.

## Test controls and diagnostics

Tests inject services through `DesktopBootstrap::new` or
`production_with_controls`. The `interaction-harness` feature exposes
`DesktopInteractionHarness` for production widgets with controlled OS responses.
Normal startup disables fault controls and observations; disabled paths take no
locks and construct no observations.

The default `diagnostics` feature records warnings and errors. Debug and harness
builds also record traces and timing summaries. The default `gpu-renderer`
feature tries Iced's GPU renderer and falls back to `tiny-skia` when no adapter
is available. Set `ICED_BACKEND=tiny-skia` to select software rendering.
Build with `--no-default-features --features gpu-renderer` to omit logging while
retaining GPU rendering. See [diagnostics](../parchmint-diagnostics/README.md)
for limits.

The opt-in `renderer-verification` feature checks every native software-rendered
frame for stale pixels; see the [renderer guide](../../third_party/iced_tiny_skia-0.14.1/README.md#verify-the-patch).
Use it for visual checks, not benchmarks.

Logs use `logs/parchmint-debug.log` below these application-data directories:

| Platform | Directory |
| --- | --- |
| Linux | `$XDG_DATA_HOME/parchmint`, or `~/.local/share/parchmint` |
| macOS | `~/Library/Application Support/ParchMint` |
| Windows | `%LOCALAPPDATA%/ParchMint/Data` |

Dialogs show a short explanation; logs retain technical causes without document
text. See [packaging](../../packaging/README.md) for isolated release builds.
