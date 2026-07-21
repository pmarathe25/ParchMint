# Export fidelity

> Read when changing compile IR, exporters, warnings, or user-facing format promises.

“Text fallback” and “visible source” are deliberate degradations. Exporters warn
when a target cannot preserve a construct.

| Feature | Markdown | Text | HTML | PDF | EPUB | DOCX |
|---|---|---|---|---|---|---|
| Headings, lists, links, Unicode | Native | Normalized | Native | Shaped Qt text | XHTML | OOXML |
| Bold, italic, strike, super/subscript | Native | Text | Native | Rich text | XHTML | Runs |
| Named styles, alignment | Attributes | Text | CSS | CSS projection | CSS | Styles |
| Images | `asset:` reference | Alt text | Embedded/safe relative | Embedded | Packaged | Packaged supported MIME |
| Scene/page breaks | Markers | `***` / form feed | Semantic | Native page break | XHTML/CSS | Native break |
| Tables, footnotes, opaque source | Source | Visible source | Warning block | Visible source | Warning block | Visible warning |

## Destination contract

1. Render and validate a destination-adjacent temporary artifact.
2. Recheck project generation/revision and collision policy.
3. Commit atomically.

Cancellation, validation failure, collision, or stale work never changes the
requested destination. Consumer validation uses browsers/readers, LibreOffice,
Word, EPUBCheck, and Qt PDF viewers as defined by the
[release process](../release/process.md).
