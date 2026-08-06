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

The schema is the source for generated Rust types. A pinned `typify` build tool
generates those types. Contributors edit a schema, then regenerate the types
and validate its fixtures.

## Public API

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

Generated Rust modules provide the remaining API. Each module records its
schema version and source checksum. Schemas contain ParchMint stable IDs in
their serialized form and no library handles. `CanonicalCodec` applies project
rules when it reads or writes project files.

## Implementation

Generation validates every schema before it writes output. It then loads each
JSON fixture through the generated type and encodes it again:

```rust
for schema in schemas_in_stable_order() {
    validate_schema(&schema)?;
    generate_rust(&schema)?;
    for fixture in fixtures_for(&schema) {
        let value = generated_decode(&schema, fixture.bytes())?;
        assert_eq!(generated_encode(&schema, &value)?, fixture.canonical_bytes());
    }
}
assert_generated_files_unchanged()?;
```

Compatible updates add fields with documented defaults. A field rename or
removal creates a new schema version and a migration. Readers reject unknown
values that change meaning and can ignore fields that the schema marks safe.

ParchMint does not generate other-language bindings or define a general
external-program protocol in v1. If either becomes a real product boundary,
its schema belongs in this crate at that time.
