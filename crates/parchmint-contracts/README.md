# `parchmint-contracts`

**Purpose:** Define versioned JSON records for annotation sidecars and recovery.
[Project format](../parchmint-project-format/README.md) owns the HTML, TOML, CSS,
and text codecs and their project-file rules.

## Interface

`descriptor` identifies a contract version; `validate_fixture` checks a JSON
record. [generated.rs](src/generated.rs) defines `AnnotationSidecarV1`,
`RecoveryRecordV1`, and `SCHEMA_MANIFEST`, which records schema identities,
versions, checksums, and top-level fields.

`AnnotationThread`, `AnnotationMessage`, `AnnotationAnchor`, and `AnnotationValue`
preserve annotation content, including unknown nested fields. Serialized stable
IDs are strings. See [lib.rs](src/lib.rs) for the types and validation methods.

## Schema changes

Keep schemas, Rust bindings, checksums, and fixtures in sync. Tests regenerate
the schema manifest and compare it with the checked-in value; fixture tests
exercise decoding and re-encoding. The generated record types reject unknown
top-level fields with `deny_unknown_fields`.

The current contracts are v1. Backwards compatibility is not required; add a
new reader or migration only when there is a supported use for it.
