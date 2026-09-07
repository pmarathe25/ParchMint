# `parchmint-test-support`

**Purpose:** Copy canonical fixtures into temporary project directories and read
them through the production format codec. This crate is for development only.

## Interface

`ScopedProject::from_fixture` copies a fixture and exposes its temporary `root`;
dropping it removes the directory. Copies skip Git metadata and retain ParchMint
control files. `CanonicalResourceSet` holds encoded fixture bytes.
`canonical_bytes` reads all project resources; `canonical_document_bytes` reads
document resources. See [lib.rs](src/lib.rs).

Invalid-input tests feed invalid bytes to the real parser. Tests build domain
trees through domain commands.

## Failure controls

Desktop fault injection uses `ProductionControls` in
[desktop](../../crates/parchmint-desktop/README.md). The
[UI driver](../parchmint-ui-driver/README.md) supplies clocks and delayed
completion delivery. Storage unit tests inject failures through the relevant
implementation's file-operation interfaces.
