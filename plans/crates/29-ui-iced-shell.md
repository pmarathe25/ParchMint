# UI Iced shell

## Goal

Implement the non-editor desktop shell in `parchmint-ui-iced`.

## Depends on

- [22 Headless backend integration](../integration/22-headless-backend-integration.md)
- [23 Design system](23-design-system.md)
- [24 Preferences](24-preferences.md)
- [25 Workspace state](25-workspace-state.md)
- [26 Platform API](26-platform-api.md)
- [27 Platform native](27-platform-native.md)
- [28 UI API](28-ui-api.md)

## Owning crate(s)

[`parchmint-ui-iced`](../../docs/architecture/crates/parchmint-ui-iced.md)

## Requirements and UI design

- [Workspace shell](../../docs/product/workspace-shell.md)
- [Launcher and project creation](../../docs/product/launcher-and-project-creation.md)
- [Appearance](../../docs/product/appearance.md)
- [Desktop interaction quality](../../docs/product/desktop-interaction-quality.md)
- [Workspace shell UI](../../docs/ui-design/workspace-shell.md)
- [Foundations](../../docs/ui-design/foundations.md)
- [Platform conventions](../../docs/ui-design/platform-conventions.md)
- [Shared interaction patterns](../../docs/ui-design/shared-interaction-patterns.md)
- [Launcher and project creation UI](../../docs/ui-design/launcher-and-project-creation.md)
- [Empty, loading, error, and recovery states](../../docs/ui-design/empty-loading-error-recovery.md)
- [Screen catalog](../../docs/ui-design/screen-catalog.md)

## Work

- Implement window-scoped messages, task completion filtering, launcher shell, ribbon, Explorer/Inspector geometry, panes, menus, dialogs, focus, and System/Light/Dark appearance.

## Stage-specific tests and validation

Run Light/Dark visual fixtures for launcher and shell states plus keyboard focus, scaling, multi-window theme propagation, stale completion, and UI-loop nonblocking checks on all platforms.
