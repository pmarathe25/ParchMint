# Export HTML

## Goal

Render a deterministic self-contained HTML5 export.

## Depends on

- [15 Export API](15-export-api.md)

## Owning crate(s)

[`parchmint-export-html`](../../docs/architecture/crates/parchmint-export-html.md)

## Requirements and UI design

- [Export](../../docs/product/export.md)
- [Privacy and security](../../docs/product/privacy-and-security.md)

## Work

- Render validated plans in order with escaped text/attributes, project CSS, structural page breaks, checked links, and chunked output writes.

## Stage-specific tests and validation

Run golden-byte exports, escaping and unsupported-link tests, duplicate-title suppression, no-script/remote-content tests, and cancellation during chunked writing.
