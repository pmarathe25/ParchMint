# Desktop

## Goal

Provide injected desktop/bootstrap plumbing and start one safe project window
per project. Stage 38 assembles the complete production graph.

## Depends on

- [20 Application](20-application.md)
- [24 Preferences](24-preferences.md)
- [27 Platform native](27-platform-native.md)
- [28 UI API](28-ui-api.md)
- [29 UI Iced shell](29-ui-iced-shell.md)

## Owning crate(s)

[`parchmint-desktop`](../../docs/architecture/crates/parchmint-desktop.md)

## Requirements and UI design

- [Launcher and project creation](../../docs/product/launcher-and-project-creation.md)
- [Platform scope](../../docs/product/platform-scope.md)

## Work

- Wire injected application, preference, platform, and UI services; resolve initial appearance, route launch intent, register sessions/windows, focus already-open projects, and retain a window while final save resolves.
- Leave complete production service construction and graph assembly to [Complete application](../integration/38-complete-application.md).

## Stage-specific tests and validation

Test startup failure cleanup, repeated-open focus, locked-project handling across processes, window/session generation filtering, and final-save close behavior.
