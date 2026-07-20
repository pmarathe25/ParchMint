# Export support matrix

This matrix is the Stage 11 contract. “Text fallback” and “visible source” are
intentional format degradations, not omitted behavior; exporters issue warnings
where fidelity is reduced.

| Feature | Markdown | Text | HTML | Qt PDF | EPUB | DOCX |
|---|---|---|---|---|---|---|
| Headings/titles, lists, links, Unicode | Native | Normalized | Native | Shaped Qt text | XHTML | OOXML |
| Bold/italic/strike, super/subscript | Native | Text only | Native | Rich text | XHTML | OOXML runs |
| Named styles/alignment | ParchMint attributes | Text only | CSS | CSS projection | CSS | Styles/justification |
| Images | `asset:` reference | Alt text | Embedded/safe relative | Embedded Qt HTML | Packaged | PNG/JPEG/GIF/BMP/TIFF package |
| Scene/page breaks | Markers | `***`/form feed | Semantic `hr` | Qt page break | XHTML/CSS | Native break |
| Tables/footnotes/opaque source | Preserved source | Visible source | Preformatted warning | Visible source | Preformatted warning | Visible text warning |

Export work first creates and validates a destination-adjacent temporary
artifact. The UI owner compares the current project generation/revision and
then commits it. A cancelled or stale artifact is dropped; `Fail` installs via
an atomic no-replace operation, and `ReplaceFile` is available only after the
explicit overwrite confirmation. EPUB validation resolves every local
`src`/`href` (including percent escapes and fragments) against archive members.

Automated structural assertions live in the compile crate for all six format
validators. Consumer opening in browsers, readers, LibreOffice, Word, EPUBCheck,
and native Qt PDF remains a platform charter item for Stage 09.
