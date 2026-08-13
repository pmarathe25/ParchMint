# `parchmint-platform-api`

## What it does

`parchmint-platform-api` defines the operating-system features ParchMint uses:
menus and menu activations, dialogs, clipboard access, external links,
application directories, and system appearance, including its change events.

`parchmint-ui-iced` owns the event loop and creates windows. This API does not
create windows or expose raw window handles. It accepts a `WindowCapability`
only after the concrete UI and native adapters privately register a live window.
The project-filesystem crate controls the one-writer project lock.

## How it works

```text
request + capability for a live window
  -> platform interface
  -> native implementation
  -> ParchMint result or event
  -> receiving crate validates returned data
```

A `WindowCapability` identifies a live registered window and, where needed,
its project session. The native adapter checks it before every call. Operations
that can wait return asynchronously. Each callback includes the window's
generation so the UI can ignore a result after the window closes.

## Interface

```rust
pub trait DialogService: Send + Sync {
    fn choose_path(
        &self,
        window: WindowCapability,
        request: PathDialog,
    ) -> AsyncResult<WindowResult<Option<UntrustedPathSelection>>>;
}

pub trait MenuService: Send + Sync {
    fn install(
        &self,
        window: WindowCapability,
        menu: SemanticMenu,
    ) -> AsyncResult<MenuBinding>;
}

pub trait ClipboardService: Send + Sync {
    fn read(
        &self,
        window: WindowCapability,
        formats: ClipboardFormats,
    ) -> AsyncResult<WindowResult<UntrustedClipboardContent>>;

    fn write(
        &self,
        window: WindowCapability,
        content: ClipboardContent,
    ) -> AsyncResult<WindowResult<()>>;
}

pub trait ExternalOpenService: Send + Sync {
    fn open(
        &self,
        window: WindowCapability,
        intent: ValidatedExternalIntent,
    ) -> AsyncResult<WindowResult<()>>;
}
```

Every window-scoped call returns a `WindowResult<T>` that pairs the value with
the exact `WindowCapability` that started the work, so the receiving crate can
ignore a result after the window closes or is replaced.

The remaining interfaces are `ApplicationPathService`,
`SystemAppearanceService`, `MenuActivationService`, and
`SystemAppearanceEventService`. The two subscription services return pull-based
ParchMint-value streams (`MenuActivationStream` and
`SystemAppearanceEventStream`). All of them use ParchMint values and never
return a raw shell, filesystem, network, or operating-system handle.

ParchMint v1 has no public `WindowService`, `SingleInstanceService`,
`NotificationService`, or `AccessibilityBridge`. The project lock handles the
single writable project session rule.

## Implementation boundary

`parchmint-platform-native` owns the live-window capability registry and native
handle checks. This crate contains only the ParchMint interfaces and value
semantics. External-link requests contain a checked URL and action; paths and
clipboard text remain untrusted until the receiving crate validates them.
