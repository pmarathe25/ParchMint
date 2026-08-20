# `parchmint-platform-native`

## What it does

`parchmint-platform-native` implements ParchMint's platform interfaces on
Windows, macOS, and Linux. It provides menus (including their activations),
dialogs, clipboard formats, application directories, external opening, and
system appearance, including its change events.

`parchmint-ui-iced` creates the windows. The two concrete adapters privately
register each live window so this crate can attach native services to it. This
crate does not create a second window system.

## How it works

```text
iced creates a window
  -> private UI/native registration records the live window capability
  -> native adapter accepts requests carrying that WindowCapability
  -> ParchMint request uses that capability
  -> native implementation returns a ParchMint result
```

Operating-system handles, callbacks, and library values remain inside this
crate and the private `iced` integration. Windows, macOS, and Linux follow
their normal window placement, keyboard shortcuts, and decorations. They share
the same ParchMint request and result types.

## Interface

The executable asks for one bundle of platform implementations:

```rust
pub struct NativePlatform {
    pub dialogs: Arc<dyn DialogService>,
    pub menus: Arc<dyn MenuService>,
    pub menu_activations: Arc<dyn MenuActivationService>,
    pub clipboard: Arc<dyn ClipboardService>,
    pub external_open: Arc<dyn ExternalOpenService>,
    pub appearance: Arc<dyn SystemAppearanceService>,
    pub appearance_events: Arc<dyn SystemAppearanceEventService>,
    pub application_paths: Arc<dyn ApplicationPathService>,
}

impl NativePlatform {
    pub fn initialize() -> Result<Self, PlatformStartupError>;
}
```

The public bundle has no window-creation, single-instance, notification, or
accessibility service. It does not expose a raw native window, shell, or
filesystem handle.

## Implementation

The native implementation owns the registry that validates window capabilities:

```rust
struct CapabilityRegistry {
    state: Arc<Mutex<RegistryState>>,
}

struct RegistryState {
    windows: HashMap<u64, WindowCapability>,
}

impl CapabilityRegistry {
    fn authorize(&self, capability: WindowCapability) -> Result<(), PlatformError> {
        match self
            .state
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .windows
            .get(&capability.window_id())
        {
            Some(registered) if *registered == capability => Ok(()),
            _ => Err(PlatformError::stale_capability(capability)),
        }
    }
}
```

Each operating system has a private implementation of the same interface. The
private registration code records the live ParchMint capability, keyed by
window ID with its exact generation; on Windows and macOS the narrow menu
adapter also retains a validated raw handle next to the capability. No handle
leaves the concrete adapters.

Native calls that run away from the UI update loop may outlive the initiating
window. Before a detached call publishes completion, the adapter reauthorizes
its capability, so a closed or replaced window receives a stale-capability
result rather than a completion for a former window.

```rust
impl CapabilityRegistry {
    fn complete<T>(
        &self,
        capability: WindowCapability,
        sender: CompletionSender<Result<T, PlatformError>>,
        result: Result<T, PlatformError>,
    ) {
        let delivered = match self.authorize(capability) {
            Ok(()) => result,
            Err(error) => Err(error),
        };
        let waker = sender.store(delivered);
        if let Some(waker) = waker {
            waker.wake();
        }
    }
}
```

The crate runs blocking or re-entrant native calls away from the UI update
function. It returns an explicit error when the operating system cannot perform
a request. Callers validate paths returned by dialogs and content read from the
clipboard. External opening receives a validated HTTPS URL; v1 defines no
file-action intent.
