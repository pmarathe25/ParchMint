# `parchmint-platform-native`

## What it does

`parchmint-platform-native` implements ParchMint's platform interfaces on
Windows, macOS, and Linux. It provides menus, dialogs, clipboard formats,
application directories, external opening, and system appearance.

`parchmint-ui-iced` creates the windows. The two concrete adapters privately
register each live window so this crate can attach native services to it. This
crate does not create a second window system.

## How it works

```text
iced creates a window
  -> private UI/native registration records its raw handle
  -> native adapter returns a WindowCapability
  -> ParchMint request uses that capability
  -> native implementation returns a ParchMint result
```

Operating-system handles, callbacks, and library values remain inside this
crate and the private `iced` integration. Windows, macOS, and Linux follow
their normal window placement, keyboard shortcuts, and decorations. They share
the same ParchMint request and result types.

## Public API

The executable asks for one bundle of platform implementations:

```rust
pub struct NativePlatform {
    pub dialogs: Arc<dyn DialogService>,
    pub menus: Arc<dyn MenuService>,
    pub clipboard: Arc<dyn ClipboardService>,
    pub external_open: Arc<dyn ExternalOpenService>,
    pub appearance: Arc<dyn SystemAppearanceService>,
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
    windows: HashMap<WindowCapability, WindowScope>,
}

impl CapabilityRegistry {
    fn authorize(&self, capability: WindowCapability) -> Result<WindowScope> {
        self.windows
            .get(&capability)
            .filter(|scope| scope.is_live())
            .cloned()
            .ok_or(PlatformError::StaleCapability)
    }
}
```

Each operating system has a private implementation of the same interface. The
private registration code records a raw window handle next to a ParchMint
capability. It does not let that handle leave the concrete adapters.

```rust
fn complete<T>(callback: NativeCallback<T>, windows: &WindowRegistry) {
    if let Some(window) = windows.live(callback.window, callback.generation) {
        window.send(callback.value.into_parchmint_value());
    }
}
```

The crate runs blocking or re-entrant native calls away from the UI update
function. It returns an explicit error when the operating system cannot perform
a request. Callers validate paths returned by dialogs and content read from the
clipboard. External opening receives a checked URL or file action.
