# `parchmint-export-html`

**Purpose:** Render a validated `ExportPlan` as one self-contained HTML5 file
that displays authored content offline.

## Interface

`HtmlExporter` implements [Exporter](../parchmint-export-api/README.md).
Only export-API values and errors cross the boundary. Parsing, escaping, CSS
sanitization, and serialization use this crate's own code.
See [lib.rs](src/lib.rs).

## Rendering rules

The same plan produces the same bytes. The renderer embeds project CSS, emits
semantic items in plan order, adds group headings, and represents scene and page
breaks as HTML structure. An existing semantic title block prevents a duplicate
document heading.

Text and attributes are escaped for their HTML context. Only supported link
schemes are accepted. Scripts, event handlers, remote embeds, and executable
remote dependencies are omitted. The renderer reads project styles from the plan
and does not consult the application theme, editor CSS, locale, clock, machine
paths, or network.

Plan construction validates input, so `validate` reports an already-valid plan;
serialization also sanitizes HTML and CSS. Output is written in small chunks,
with cancellation checks between them. Render or write failures mark partial
output incomplete and leave the project unchanged.
