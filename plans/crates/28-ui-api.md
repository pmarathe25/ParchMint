# UI API

## Goal

Define the desktop UI boundary without selecting widget or window types.

## Depends on

- [17 Editor API](17-editor-api.md)
- [18 Spellcheck API contract](18-spellcheck-api-contract.md)
- [20 Application](20-application.md)
- [23 Design system](23-design-system.md)
- [24 Preferences](24-preferences.md)
- [25 Workspace state](25-workspace-state.md)
- [26 Platform API](26-platform-api.md)

## Owning crate(s)

[`parchmint-ui-api`](../../docs/architecture/crates/parchmint-ui-api.md)

## Requirements and UI design

- [Workspace shell](../../docs/product/workspace-shell.md)

## Work

- Define `DesktopUi`, startup values, and UI ports using ParchMint services and stable appearance ordering.

## Stage-specific tests and validation

Run fake-UI startup tests and verify every UI port remains framework-neutral and every appearance event applies in stable window-ID order.
