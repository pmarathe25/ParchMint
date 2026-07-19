# ADR-0004: Source-aware Rust Markdown semantic AST

Status: Accepted (Stage 01)

## Decision

Use `pulldown-cmark` as the maintained CommonMark/GFM event parser and retain
absolute source ranges in a Rust semantic AST. YAML front matter is parsed with
`serde_yaml` during the spike; Stage 02 must select a maintained YAML strategy
before freezing hostile-input limits because `serde_yaml` is deprecated.

Supported nodes carry semantic fields and source ranges. Unsupported blocks are
source-backed opaque nodes. The Rust AST is the canonical persistence/export
boundary. Qt formats are an editor projection only; Qt Markdown/HTML conversion
is never serialization.

## Parser evaluation

`pulldown-cmark` was selected for maintained CommonMark behavior, offset events,
speed, and small dependency surface. `comrak` offers a larger AST and extension
surface but makes source-preserving unsupported syntax and deterministic output
no simpler. A custom parser was rejected as unnecessary format risk.
