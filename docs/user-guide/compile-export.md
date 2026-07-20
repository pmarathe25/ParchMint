# Compile and export

Compile uses an immutable snapshot and a persisted preset. Empty selection
means the manuscript root. Research is excluded by default and becomes eligible
only through explicit selection or the preset's `research = "all"` rule; the
preview identifies exclusions and reasons before export.

The six stable targets are Markdown, plain text, HTML, PDF, EPUB, and DOCX.
Output is built and structurally validated in a same-directory temporary file
before the UI commits it. Existing destinations are refused by default. If a
chosen destination exists, ParchMint asks for explicit overwrite confirmation;
the old file remains intact until the replacement is fully validated. Export
cancellation or a changed project revision never replaces a destination.

PDF uses the native Qt text/PDF renderer with Unicode shaping, semantic HTML
styles, images, margins, and page breaks. Text normalizes styles and uses alt
text; HTML/EPUB/DOCX preserve more semantics but opaque, tables, and footnotes
may remain visible source. DOCX never silently falls back to Pandoc.
