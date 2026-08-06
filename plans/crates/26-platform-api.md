# Platform API

## Goal

Define ParchMint-owned contracts for native operating-system features.

## Depends on

- [03 Domain](03-domain.md)

## Owning crate(s)

[`parchmint-platform-api`](../../docs/architecture/crates/parchmint-platform-api.md)

## Requirements and UI design

- [Platform scope](../../docs/product/platform-scope.md)
- [Privacy and security](../../docs/product/privacy-and-security.md)

## Work

- Define window capabilities and asynchronous menu, dialog, clipboard, external-open, application-path, and system-appearance interfaces without raw native handles.

## Stage-specific tests and validation

Verify stale capabilities fail, external-open accepts only validated intents, and dialog/clipboard results remain untrusted until their receiving boundary validates them.
