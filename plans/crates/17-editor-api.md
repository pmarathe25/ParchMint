# Editor API

## Goal

Define the engine-neutral editor contract for shared sessions and mounted views.

## Depends on

- [03 Domain](03-domain.md)
- [04 Project format](04-project-format.md)

## Owning crate(s)

[`parchmint-editor-api`](../../docs/architecture/crates/parchmint-editor-api.md)

## Requirements and UI design

- [Editor panes and tabs](../../docs/product/editor-panes-and-tabs.md)
- [Rich text and semantic styles](../../docs/product/rich-text-and-semantic-styles.md)

## Work

- Define session, view attachment, commands, selection geometry, decorations, projection, events, and close contracts using ParchMint values only.

## Stage-specific tests and validation

Run API contract tests that attach two views to one session, keep view state independent, share undo, reject stale commands, and retain deterministic projections.
