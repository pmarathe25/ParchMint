# ADR-0009: maintained pure-Rust YAML parser

Status: Accepted (Stage 03)

## Decision

Replace deprecated `serde_yaml` with `noyalib` 0.0.15 through its
`compat-serde-yaml` value/Serde surface. Continue to bound documents at 64 MiB,
front matter at 256 KiB, and value nesting at 64 levels before admitting data to
the project model. Preserve unknown string-keyed mappings.

## Consequences

The format keeps its YAML 1.2 spelling and unknown-key behavior while removing
the unmaintained dependency identified by Stages 01–02. `noyalib` is pure Rust;
ParchMint does not enable native YAML FFI. Its serializer does not append a
newline, so storage adds the delimiter newline explicitly before `---`.
