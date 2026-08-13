# `parchmint-contracts`

## What it does

`parchmint-contracts` defines the durable JSON shapes that ParchMint reads and
writes across versions. It covers document annotation sidecars, recovery
records, and machine-readable CLI output.

The crate does not define every ParchMint file format. The project-format and
export crates own their HTML, TOML, CSS, and text codecs. Those crates keep
golden fixtures for their own formats.

## How it works

```text
JSON Schema source
      |
      +--> generated Rust types
      +--> validated JSON fixtures
      +--> checksum and clean-regeneration check
```

The schema is the source of truth for the generated bindings in
`src/generated.rs`. Contributors edit a schema, update the bindings, and keep
its fixtures valid. A pinned `typify` generation pipeline is planned but is not
yet wired into this crate; today `generated.rs` is hand-maintained and kept
honest by the regeneration-diff test described below.

## Interface

```rust
pub struct ContractDescriptor {
    pub schema_id: &'static str,
    pub schema_version: u32,
    pub source_checksum: &'static str,
}

pub fn descriptor(schema_id: &str) -> Option<&'static ContractDescriptor>;

pub fn validate_fixture(
    descriptor: &ContractDescriptor,
    json: &[u8],
) -> Result<(), ContractError>;
```

```rust
pub enum ContractError {
    Json(serde_json::Error),
    SchemaMismatch { expected: &'static str, actual: String },
}
```

Generated Rust bindings (`generated::*`) provide the remaining API: one
versioned type per schema (`AnnotationSidecarV1`, `RecoveryRecordV1`,
`CliOutputV1`). A single `SCHEMA_MANIFEST` constant records each schema's
version and source checksum. Schemas carry ParchMint stable IDs in serialized
text form (strings), not as typed library handles. The hand-written
`AnnotationThread`, `AnnotationMessage`, `AnnotationAnchor`, and
`AnnotationValue` types model lossless annotation sidecar content; the
project-format crate round-trips them into the annotation sidecar. Project-file
rules live in `parchmint-project-format`'s `CanonicalCodec`, outside this
crate.

## Implementation

Native tests keep the bindings honest. Each fixture is loaded through the
generated type and encoded again, malformed and non-UTF-8 JSON is rejected, and
a freshly rebuilt schema manifest must equal the checked-in constant:

```rust
for contract in CONTRACTS {
    assert_eq!(
        descriptor(contract.descriptor.schema_id).unwrap().source_checksum,
        sha256(contract.schema_file)
    );
    for fixture in fixtures_for(&contract) {
        let value = generated_decode(&contract, fixture.bytes())?;
        let _canonical = generated_encode(&contract, &value)?;
    }
}
assert_eq!(regenerate_manifest_from_schemas(), generated::SCHEMA_MANIFEST);
```

Every schema change creates a new version: the new schema and its fixtures sit
beside the old ones, and readers of the old version keep working. The generated
bindings reject unknown fields outright (`deny_unknown_fields`), so
forward-compatible additions with documented defaults, reader-side migrations,
and fields the schema marks safe to ignore are not implemented yet.

ParchMint does not generate other-language bindings or define a general
external-program protocol in v1. If either becomes a real product boundary,
its schema belongs in this crate at that time.
