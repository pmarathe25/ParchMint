# Export

> Read when selecting compile scope, presets, formats, or destinations.

Compile follows binder preorder using an immutable project snapshot and a
persisted preset. Empty selection means the manuscript root. Research is
excluded unless the selected scope or preset includes it.

ParchMint exports Markdown, plain text, HTML, PDF, EPUB, and DOCX. Output is
built and validated beside the destination before commit. Existing files are
refused unless overwrite is explicitly confirmed. Cancellation, failure, or a
stale project revision leaves the requested destination unchanged.

Formats preserve different features. Review the
[export fidelity table](../reference/export-fidelity.md) before relying on
styles, images, opaque source, tables, or footnotes in a target format.
