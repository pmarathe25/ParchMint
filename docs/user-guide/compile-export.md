# Compile and export

Compile uses an immutable snapshot and a persisted preset. Empty selection
means the manuscript root. Research is excluded by default and becomes eligible
only through explicit selection or the preset's `research = "all"` rule; the
preview identifies exclusions and reasons before export.

The six stable targets are Markdown, plain text, HTML, PDF, EPUB, and DOCX.
Output is built and structurally validated before same-directory atomic
replacement. Existing destinations are refused by default; replacement keeps
the old file until the final operation. Export cancellation never replaces a
destination.

Intentional degradation is reported using the Stage 07 wording: PDF fallback
has basic text/layout and warns for non-Latin shaping; text normalizes styles
and uses alt text; HTML/EPUB/DOCX preserve more semantics but opaque, tables, and
footnotes may remain visible source. DOCX never silently falls back to Pandoc.
