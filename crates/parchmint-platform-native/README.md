# `parchmint-platform-native`

**Purpose:** Implement ParchMint's [platform interfaces](../parchmint-platform-api/README.md)
on Windows, macOS, and Linux: menus and activations, dialogs, clipboard formats,
application directories, external links, and system appearance events.

## Interface

`NativePlatform::initialize` constructs the service bundle.
`testing::NativeFixture` supplies controlled responses for integration tests.
See [lib.rs](src/lib.rs). Results use ParchMint types and typed errors.

## Native integration

The Iced UI creates windows. Private registration associates each live window ID
and generation with its `WindowCapability`. On Windows and macOS, the menu adapter
also retains a validated raw handle. OS handles and library values stay inside
the concrete UI and native adapters.

Blocking or re-entrant native calls run away from the UI update function. Before
a detached call publishes completion, the adapter checks its capability again.
Closed or replaced windows produce stale-capability errors.

Unsupported or failed operations return explicit errors. Callers validate paths
from dialogs and clipboard content. External opening accepts a validated HTTPS
URL; the platform API has no file-action intent. Window placement, shortcuts,
and decorations follow the host platform.
