# ADR-0011: contained DOCX package writer

Status: Accepted

## Context

Version 1 promises DOCX export with headings, named styles, lists, links,
images, Unicode, and page breaks. The locked offline dependency set has no
maintained Rust DOCX writer that can be proven against that complete semantic
matrix. Adding an unreviewed crate or silently shelling out to Pandoc would
make a local export depend on a mutable external toolchain.

## Decision

Use a narrowly contained, deterministic OOXML package writer in
`parchmint-compile`. It writes only the required WordprocessingML parts,
relationships, styles, numbering, supported image assets, and fixed metadata
timestamps. It is fed only by the compile IR; it is not a Markdown parser and
does not own project state.

The writer structurally validates its own store-mode ZIP package before atomic
destination replacement. Release validation must open golden DOCX fixtures in
current Word and LibreOffice on all supported platforms. A later maintained Rust writer
may replace this implementation only after a semantic equivalence suite passes.

Pandoc remains an explicit user-selected integration opportunity; it is not a
core dependency and is never invoked as a fallback.

## Consequences

This keeps normal export offline and deterministic. Source-backed tables,
footnotes, and opaque Markdown are visibly retained as text with warnings,
rather than being claimed as native OOXML constructs. Advanced typography,
tracked changes, embedded fonts, and arbitrary image formats stay out of scope.
