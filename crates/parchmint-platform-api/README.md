# `parchmint-platform-api`

**Purpose:** Define native menus, dialogs, clipboard access, external links,
application directories, and system appearance through ParchMint values.

## Window identity

`WindowCapability` is an opaque identity for a privately registered live window
and, when required, its project session. Native adapters check its generation
before window-scoped calls. A `WindowResult<T>` returns both the value and the
initiating capability so callers can reject results for closed or replaced windows.
Operations that can wait return asynchronously.

## Interface

- `DialogService`, `MenuService`, `ClipboardService`, and `ExternalOpenService`
  perform native actions.
- `ApplicationPathService` supplies application directories.
- `SystemAppearanceService` supplies current system appearance.
- `MenuActivationService` and `SystemAppearanceEventService` return pull-based
  `MenuActivationStream` and `SystemAppearanceEventStream` values.

See [lib.rs](src/lib.rs) for signatures. External-link requests contain a checked
URL and action. Receiving crates validate dialog paths and clipboard content.

## Implementation boundary

[platform-native](../parchmint-platform-native/README.md) owns capability
registration and handle checks. [ui-iced](../parchmint-ui-iced/README.md) creates
windows and owns the event loop; the project filesystem owns write locks.
The API exposes no raw window, filesystem, shell, or network handles.
