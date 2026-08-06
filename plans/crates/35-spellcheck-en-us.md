# Spellcheck engine evaluation and implementation

## Goal

Select and implement the private offline en-US spellcheck engine behind the existing contract.

## Depends on

- [18 Spellcheck API contract](18-spellcheck-api-contract.md)
- [24 Preferences](24-preferences.md)
- [34 Editor save and recovery integration](../integration/34-editor-save-recovery-integration.md)

## Owning crate(s)

[`parchmint-spellcheck-en-us`](../../docs/architecture/crates/parchmint-spellcheck-en-us.md)

## Requirements and UI design

- [Spellcheck](../../docs/product/spellcheck.md)
- [Privacy and security](../../docs/product/privacy-and-security.md)

## Work

- Evaluate `spellbook` as the private candidate. Select it only if it provides compatible licensing, bundled offline dictionaries, ranked suggestions, project/global dictionary handling, cancellation, viewport/recent-change bounds, cross-platform correctness, and required latency.
- Keep engine types private, send no prose to a network service, and preserve saved dictionary changes when engine reload fails.

## Stage-specific tests and validation

Test suggestions and dictionary actions, stale-result rejection, engine-reload failure, offline operation, latency, and Windows/macOS/Linux correctness. A failed candidate blocks release until another candidate passes or the product requirement changes.
