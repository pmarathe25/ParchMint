# `parchmint-test-support`

This development-only crate copies canonical file fixtures into temporary
directories and reads them through the production format codec.

## Interface and implementation

`ScopedProject::from_fixture` copies a fixture and exposes its temporary `root`.
`CanonicalResourceSet` holds encoded fixture bytes and belongs to test support.
`canonical_bytes` reads the project resource set; `canonical_document_bytes`
reads its document resources. Dropping the value removes its temporary directory.
Fixture copying skips Git metadata and keeps ParchMint control files.

See [lib.rs](src/lib.rs) for the fixture helpers. Invalid-input tests supply
invalid bytes to the real parser. Tests construct domain trees directly through
the domain command API.

## Service and UI failures

Actual desktop fault injection belongs to `ProductionControls` in
[`parchmint-desktop`](../../crates/parchmint-desktop/README.md).
The [UI driver](../parchmint-ui-driver/README.md) supplies recovery/autosave clocks
and delayed completion delivery. Tests of individual storage implementations
inject failures through those implementations' file-operation interfaces.
