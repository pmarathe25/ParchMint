# `parchmint-export-html`

## What it does

This crate renders a validated `ExportPlan` as one self-contained HTML5 file.
HTML parsing, escaping, CSS generation, and serialization stay inside this
crate.

## How it works

```text
validated ExportPlan
  -> HTML document and embedded project CSS
  -> escaped semantic items in plan order
  -> structural group headings and page breaks
  -> write the complete file through ExportSink
```

If a document already contains its semantic title block, the renderer does not
emit the same title again.

## Public API

```rust
pub struct HtmlExporter {
    renderer: HtmlPlanRenderer,
}

impl Exporter for HtmlExporter {
    fn plan(&self, request: ExportRequest, project: &ProjectSnapshot)
        -> Result<ExportPlan, ExportError>;
    fn validate(&self, plan: &ExportPlan) -> ExportValidationReport;
    fn export(&self, plan: ExportPlan, sink: ExportSink)
        -> Result<ExportHandle, ExportError>;
    fn cancel(&self, handle: ExportHandle);
}
```

HTML library values and errors are translated before leaving the crate.

## Implementation

The same plan produces the same HTML bytes. Rendering does not read the
application theme, locale, clock, machine paths, or network. The renderer
escapes text and attributes according to where they appear in HTML. It accepts
only supported link schemes.

The renderer omits scripts, event handlers, remote embeds, and executable remote
dependencies. It reads project styles from the `ExportPlan`. It does not read
application theme tokens or editor CSS. Scene and page-break nodes become HTML
structure instead of visible marker text.

The renderer writes the file in small chunks and checks for cancellation between
chunks. A render or output error leaves the project unchanged and reports the
partial file as incomplete. The completed HTML file can display its authored
content without a network connection.

The private renderer writes one semantic item at a time:

```rust
trait HtmlPlanRenderer {
    fn render_item(&self, item: &SemanticExportItem, out: &mut dyn Write)
        -> Result<(), HtmlExportError>;
}
```
