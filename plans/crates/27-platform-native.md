# Platform native

## Goal

Implement platform services on Windows, macOS, and Linux without a second window system.

## Depends on

- [26 Platform API](26-platform-api.md)

## Owning crate(s)

[`parchmint-platform-native`](../../docs/architecture/crates/parchmint-platform-native.md)

## Requirements and UI design

- [Platform scope](../../docs/product/platform-scope.md)
- [Desktop interaction quality](../../docs/product/desktop-interaction-quality.md)
- [Platform conventions](../../docs/ui-design/platform-conventions.md)

## Work

- Implement native menus, dialogs, clipboard, application paths, external opening, appearance, and private registration with the Iced window adapter.

## Stage-specific tests and validation

Run Windows/macOS/Linux native integration checks for menus, dialogs, clipboard formats, accelerators, appearance, stale callbacks, and nonblocking callback delivery.
