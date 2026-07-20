# Export support matrix

This matrix is the Stage 07 contract. “Text fallback” and “visible source” are
intentional format degradations, not omitted behavior; exporters issue warnings
where fidelity is reduced.

| Feature | Markdown | Text | HTML | PDF fallback | EPUB | DOCX |
|---|---|---|---|---|---|---|
| Headings/titles, lists, links, Unicode | Native | Normalized | Native | Basic | XHTML | OOXML |
| Bold/italic/strike, super/subscript | Native | Text only | Native | Text fallback | XHTML | OOXML |
| Named styles/alignment | ParchMint attributes | Text only | CSS | Basic text fallback | CSS | Styles/justification |
| Images | `asset:` reference | Alt text | Embedded/safe relative | Alt text warning | Packaged | PNG/JPEG package |
| Scene/page breaks | Markers | `***`/form feed | Semantic `hr` | Pages/basic marker | XHTML/CSS | Native break |
| Tables/footnotes/opaque source | Preserved source | Visible source | Preformatted warning | Text warning | Preformatted warning | Visible text warning |

Automated structural assertions live in the compile crate for all six format
validators and deterministic ZIP bytes. Consumer opening in browsers, readers,
LibreOffice, Word, EPUBCheck, and native Qt PDF remains a platform charter item
for Stage 09.
