# Spellcheck API contract

## Goal

Define private-engine offline en-US spellcheck boundaries.

## Depends on

- [03 Domain](03-domain.md)
- [17 Editor API](17-editor-api.md)

## Owning crate(s)

[`parchmint-spellcheck-api`](../../docs/architecture/crates/parchmint-spellcheck-api.md)

## Requirements and UI design

- [Spellcheck](../../docs/product/spellcheck.md)
- [Privacy and security](../../docs/product/privacy-and-security.md)

## Work

- Define language, request, result, suggestion, dictionary reload, generation, and cancellation values without engine or operating-system types.

## Stage-specific tests and validation

Verify only en-US is exposed, stale text/dictionary generations are discarded, visible work is prioritized, and contract values contain no engine handles.
