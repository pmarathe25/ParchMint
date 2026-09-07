# `parchmint-contracts`

`parchmint-contracts` defines the durable JSON shapes that ParchMint reads and
writes across versions. It covers document annotation sidecars and recovery
records.

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

The schema is the source of truth for the bindings in `src/generated.rs`.
Contributors edit a schema, update the bindings, and keep its fixtures valid.
Tests check that the bindings and schemas stay in sync.

## Interface

`descriptor` and `validate_fixture` identify and check versioned JSON records.
`generated` contains the annotation and recovery bindings; `AnnotationThread`,
`AnnotationMessage`, `AnnotationAnchor`, and `AnnotationValue` preserve annotation content.

See [the source](src/lib.rs) for method signatures.

Generated Rust bindings (`generated::*`) provide the remaining API: one
versioned type per schema (`AnnotationSidecarV1` and `RecoveryRecordV1`). A
single `SCHEMA_MANIFEST` constant records each schema's version and source
checksum. Schemas carry ParchMint stable IDs in serialized
text form (strings), not as typed library handles. The hand-written
`AnnotationThread`, `AnnotationMessage`, `AnnotationAnchor`, and
`AnnotationValue` types model lossless annotation sidecar content; the
project-format crate round-trips them into the annotation sidecar. Project-file
rules live in `parchmint-project-format`'s `CanonicalCodec`, outside this
crate.

## Implementation

Every schema change creates a new version: the new schema and its fixtures sit
beside the old ones, and readers of the old version keep working. The generated
bindings reject unknown fields outright (`deny_unknown_fields`), so
forward-compatible additions with documented defaults, reader-side migrations,
and fields the schema marks safe to ignore are not implemented yet.

ParchMint does not generate other-language bindings or define a general
external-program protocol in v1. If either becomes a real product boundary,
its schema belongs in this crate at that time.
