# ADR-0004: Source-aware Rust Markdown semantic AST

Status: Accepted

## Decision

Use `pulldown-cmark` as the maintained CommonMark/GFM event parser and retain
absolute source ranges in a Rust semantic AST. YAML front matter is parsed with
`serde_yaml` during the spike. ADR-0009 subsequently selected the maintained
`noyalib` compatibility surface and the codec now enforces hostile-input limits.

Supported nodes carry semantic fields and source ranges. Unsupported blocks are
source-backed opaque nodes. The Rust AST is the canonical persistence/export
boundary. Qt formats are an editor projection only; Qt Markdown/HTML conversion
is never serialization.

## Parser evaluation

`pulldown-cmark` was selected for maintained CommonMark behavior, offset events,
speed, and small dependency surface. `comrak` offers a larger AST and extension
surface but makes source-preserving unsupported syntax and deterministic output
no simpler. A custom parser was rejected as unnecessary format risk.
