//! Deterministic, self-contained HTML5 export.
//!
//! This crate accepts the immutable plans produced by `parchmint-export-api`.
//! It deliberately does not consult application state while rendering: all
//! content and project styling comes from the plan.

use parchmint_export_api::{
    ExportCompletion, ExportError, ExportHandle, ExportPlan, ExportProgress, ExportProgressSink,
    ExportRequest, ExportSink, ExportValidationReport, Exporter, ProjectSnapshot,
    SemanticExportItem, TemporaryExport,
};
use std::sync::Arc;

const CHUNK_BYTES: usize = 8 * 1024;

/// Exports immutable plans as one self-contained HTML5 document.
#[derive(Debug, Default)]
pub struct HtmlExporter;

impl HtmlExporter {
    /// Renders a plan into a temporary destination.
    ///
    /// Keeping this synchronous makes cancellation observable at every output
    /// chunk without coupling the format crate to a runtime. Platform code can
    /// run it in its export worker.
    fn render(
        &self,
        plan: &ExportPlan,
        sink: &mut dyn ExportSink,
        handle: &ExportHandle,
        progress: &dyn ExportProgressSink,
    ) -> Result<ExportCompletion, ExportError> {
        let mut output = handle.begin_temporary(sink, plan.target())?;
        write_chunked(
            &mut output,
            "<!doctype html><html><head><meta charset=\"utf-8\"><style>",
        )?;
        write_chunked(&mut output, &sanitize_css(plan.styles().css()))?;
        write_chunked(&mut output, "</style></head><body>")?;

        let total = u64::try_from(plan.items().len()).unwrap_or(u64::MAX);
        progress.report(ExportProgress::Rendering {
            completed: 0,
            total,
        });
        for (index, item) in plan.items().iter().enumerate() {
            let mut rendered = String::new();
            render_item(item, &mut rendered);
            write_chunked(&mut output, &rendered)?;
            progress.report(ExportProgress::Rendering {
                completed: u64::try_from(index + 1).unwrap_or(u64::MAX),
                total,
            });
        }

        write_chunked(&mut output, "</body></html>")?;
        progress.report(ExportProgress::Committing);
        output.finish()
    }
}

impl Exporter for HtmlExporter {
    fn plan(
        &self,
        request: ExportRequest,
        project: &ProjectSnapshot,
    ) -> Result<ExportPlan, ExportError> {
        ExportPlan::build(request, project).map_err(ExportError::Validation)
    }

    fn validate(&self, _: &ExportPlan) -> ExportValidationReport {
        // ExportPlan construction is the validation boundary. The renderer
        // nevertheless sanitizes the serialized HTML and CSS defensively.
        ExportValidationReport::default()
    }

    fn export(
        &self,
        plan: ExportPlan,
        mut sink: Box<dyn ExportSink>,
        handle: ExportHandle,
        progress: Arc<dyn ExportProgressSink>,
    ) -> Result<ExportCompletion, ExportError> {
        self.render(&plan, sink.as_mut(), &handle, progress.as_ref())
    }
}

fn render_item(item: &SemanticExportItem, out: &mut String) {
    match item {
        SemanticExportItem::GroupHeading(heading) => {
            out.push_str("<h1>");
            escape_text(&heading.title, out);
            out.push_str("</h1>");
        }
        SemanticExportItem::Document(document) => {
            let body = sanitize_body(&document.body);
            out.push_str("<article>");
            if document.settings.emit_titles && !has_document_title(&body, &document.title) {
                out.push_str("<h2>");
                escape_text(&document.title, out);
                out.push_str("</h2>");
            }
            out.push_str(&body);
            out.push_str("</article>");
        }
        SemanticExportItem::PageBreak => {
            out.push_str("<div class=\"page-break\" aria-hidden=\"true\"></div>");
        }
    }
}

fn write_chunked(output: &mut TemporaryExport<'_>, rendered: &str) -> Result<(), ExportError> {
    for chunk in rendered.as_bytes().chunks(CHUNK_BYTES) {
        // TemporaryExport observes cancellation while holding the shared state
        // required to prevent a cancelled operation from completing.
        output.write_chunk(chunk)?;
    }
    Ok(())
}

fn sanitize_css(css: &str) -> String {
    let normalized = css.replace("\r\n", "\n").replace('\r', "\n");
    let lowered = normalized.to_ascii_lowercase();
    if [
        "@import",
        "url(",
        "expression(",
        "javascript:",
        "behavior:",
        "</style",
    ]
    .iter()
    .any(|unsafe_fragment| lowered.contains(unsafe_fragment))
    {
        String::new()
    } else {
        normalized
    }
}

fn sanitize_body(body: &str) -> String {
    let mut output = String::new();
    let mut cursor = 0;
    let mut blocked_elements: Vec<String> = Vec::new();
    let mut open_elements: Vec<String> = Vec::new();
    let mut contains_block = false;

    while cursor < body.len() {
        let Some(offset) = body[cursor..].find('<') else {
            if blocked_elements.is_empty() {
                escape_text(&decode_entities(&body[cursor..]), &mut output);
            }
            break;
        };
        let tag_start = cursor + offset;
        if blocked_elements.is_empty() {
            escape_text(&decode_entities(&body[cursor..tag_start]), &mut output);
        }
        let Some(tag_end) = find_tag_end(body, tag_start + 1) else {
            if blocked_elements.is_empty() {
                escape_text(&decode_entities(&body[tag_start..]), &mut output);
            }
            break;
        };
        let Some(token) = parse_tag(&body[tag_start + 1..tag_end]) else {
            if blocked_elements.is_empty() {
                escape_text("<", &mut output);
            }
            cursor = tag_start + 1;
            continue;
        };
        cursor = tag_end + 1;

        if !blocked_elements.is_empty() {
            if token.closing && token.name == *blocked_elements.last().expect("nonempty stack") {
                blocked_elements.pop();
            } else if !token.closing && blocks_contents(&token.name) && !token.self_closing {
                blocked_elements.push(token.name);
            }
            continue;
        }

        if blocks_contents(&token.name) {
            if !token.closing && !token.self_closing {
                blocked_elements.push(token.name);
            }
            continue;
        }
        if !is_allowed_tag(&token.name) {
            continue;
        }
        if token.closing {
            if open_elements.last().is_some_and(|open| open == &token.name) {
                open_elements.pop();
                output.push_str("</");
                output.push_str(&token.name);
                output.push('>');
            }
            continue;
        }

        if token.name == "hr"
            && token
                .attributes
                .iter()
                .any(|(name, value)| name == "data-kind" && value == "page-break")
        {
            output.push_str("<div class=\"page-break\" aria-hidden=\"true\"></div>");
            contains_block = true;
            continue;
        }
        if token.name == "hr"
            && token
                .attributes
                .iter()
                .any(|(name, value)| name == "data-kind" && value == "scene-break")
        {
            output.push_str("<hr class=\"scene-break\" aria-hidden=\"true\">");
            contains_block = true;
            continue;
        }

        if is_block_tag(&token.name) {
            contains_block = true;
        }
        output.push('<');
        output.push_str(&token.name);
        for (name, value) in token.attributes {
            if is_allowed_attribute(&token.name, &name, &value) {
                output.push(' ');
                output.push_str(&name);
                output.push_str("=\"");
                escape_attribute(&value, &mut output);
                output.push('"');
            }
        }
        output.push('>');
        if !is_void_tag(&token.name) && !token.self_closing {
            open_elements.push(token.name);
        }
    }

    while let Some(tag) = open_elements.pop() {
        output.push_str("</");
        output.push_str(&tag);
        output.push('>');
    }

    if output.is_empty() || contains_block {
        output
    } else {
        format!("<p>{output}</p>")
    }
}

fn has_document_title(body: &str, title: &str) -> bool {
    let trimmed = body.trim_start();
    let Some(open_end) = trimmed.find('>') else {
        return false;
    };
    if !trimmed[..open_end].starts_with("<h1") {
        return false;
    }
    let Some(close_start) = trimmed[open_end + 1..].find("</h1>") else {
        return false;
    };
    let content = &trimmed[open_end + 1..open_end + 1 + close_start];
    content == escaped_text(title)
}

#[derive(Debug)]
struct HtmlTag {
    name: String,
    attributes: Vec<(String, String)>,
    closing: bool,
    self_closing: bool,
}

fn find_tag_end(input: &str, start: usize) -> Option<usize> {
    let mut quote = None;
    for (offset, character) in input[start..].char_indices() {
        match (quote, character) {
            (Some(current), character) if character == current => quote = None,
            (None, '\"' | '\'') => quote = Some(character),
            (None, '>') => return Some(start + offset),
            _ => {}
        }
    }
    None
}

fn parse_tag(token: &str) -> Option<HtmlTag> {
    let token = token.trim();
    if token.starts_with('!') || token.starts_with('?') {
        return None;
    }
    let (token, closing) = match token.strip_prefix('/') {
        Some(rest) => (rest.trim(), true),
        None => (token, false),
    };
    let (token, self_closing) = match token.strip_suffix('/') {
        Some(rest) => (rest.trim_end(), true),
        None => (token, false),
    };
    let name_end = token.find(char::is_whitespace).unwrap_or(token.len());
    let name = token[..name_end].to_ascii_lowercase();
    if name.is_empty()
        || !name.as_bytes()[0].is_ascii_alphabetic()
        || !name
            .chars()
            .all(|character| character.is_ascii_alphanumeric())
    {
        return None;
    }
    if closing && !token[name_end..].trim().is_empty() {
        return None;
    }
    Some(HtmlTag {
        name,
        attributes: if closing {
            Vec::new()
        } else {
            parse_attributes(&token[name_end..])
        },
        closing,
        self_closing,
    })
}

fn parse_attributes(input: &str) -> Vec<(String, String)> {
    let mut attributes = Vec::new();
    let mut remaining = input.trim();
    while !remaining.is_empty() {
        let name_end = remaining
            .find(|character: char| character.is_whitespace() || character == '=')
            .unwrap_or(remaining.len());
        let name = remaining[..name_end].to_ascii_lowercase();
        remaining = remaining[name_end..].trim_start();
        let Some(after_equals) = remaining.strip_prefix('=') else {
            break;
        };
        let after_equals = after_equals.trim_start();
        let Some(quote) = after_equals
            .chars()
            .next()
            .filter(|character| matches!(character, '\"' | '\''))
        else {
            break;
        };
        let value_start = quote.len_utf8();
        let Some(value_end) = after_equals[value_start..].find(quote) else {
            break;
        };
        attributes.push((
            name,
            decode_entities(&after_equals[value_start..value_start + value_end]),
        ));
        remaining = after_equals[value_start + value_end + quote.len_utf8()..].trim_start();
    }
    attributes
}

fn is_allowed_tag(tag: &str) -> bool {
    matches!(
        tag,
        "p" | "h1"
            | "h2"
            | "h3"
            | "blockquote"
            | "ul"
            | "ol"
            | "li"
            | "br"
            | "strong"
            | "em"
            | "u"
            | "s"
            | "sup"
            | "sub"
            | "a"
            | "span"
            | "hr"
    )
}

fn is_block_tag(tag: &str) -> bool {
    matches!(
        tag,
        "p" | "h1" | "h2" | "h3" | "blockquote" | "ul" | "ol" | "li" | "hr"
    )
}

fn is_void_tag(tag: &str) -> bool {
    matches!(tag, "br" | "hr")
}

fn blocks_contents(tag: &str) -> bool {
    matches!(
        tag,
        "script" | "style" | "iframe" | "object" | "embed" | "template"
    )
}

fn is_allowed_attribute(tag: &str, name: &str, value: &str) -> bool {
    if name.starts_with("on") || value.chars().any(char::is_control) {
        return false;
    }
    match name {
        "title" | "class" | "id" => true,
        "href" => tag == "a" && is_safe_href(value),
        name if name.starts_with("data-") => true,
        _ => false,
    }
}

fn is_safe_href(value: &str) -> bool {
    if value.is_empty()
        || value.starts_with(['/', '\\'])
        || value.starts_with("//")
        || value.contains('\\')
    {
        return false;
    }
    match value.split_once(':') {
        Some((scheme, _)) => matches!(
            scheme.to_ascii_lowercase().as_str(),
            "http" | "https" | "mailto"
        ),
        None => !value.split('/').any(|segment| segment == ".."),
    }
}

fn decode_entities(text: &str) -> String {
    let mut output = String::new();
    let mut remaining = text;
    while let Some(start) = remaining.find('&') {
        output.push_str(&remaining[..start]);
        let after = &remaining[start + 1..];
        let Some(end) = after.find(';') else {
            output.push('&');
            remaining = after;
            continue;
        };
        let entity = &after[..end];
        let character = match entity {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('\"'),
            "apos" => Some('\''),
            entity if entity.starts_with("#x") || entity.starts_with("#X") => {
                u32::from_str_radix(&entity[2..], 16)
                    .ok()
                    .and_then(char::from_u32)
            }
            entity if entity.starts_with('#') => {
                entity[1..].parse::<u32>().ok().and_then(char::from_u32)
            }
            _ => None,
        };
        if let Some(character) = character {
            output.push(character);
            remaining = &after[end + 1..];
        } else {
            output.push('&');
            remaining = after;
        }
    }
    output.push_str(remaining);
    output
}

fn escaped_text(value: &str) -> String {
    let mut output = String::new();
    escape_text(value, &mut output);
    output
}

fn escape_text(value: &str, output: &mut String) {
    for character in value.chars() {
        match character {
            '&' => output.push_str("&amp;"),
            '<' => output.push_str("&lt;"),
            '>' => output.push_str("&gt;"),
            _ => output.push(character),
        }
    }
}

fn escape_attribute(value: &str, output: &mut String) {
    for character in value.chars() {
        match character {
            '&' => output.push_str("&amp;"),
            '<' => output.push_str("&lt;"),
            '>' => output.push_str("&gt;"),
            '\"' => output.push_str("&quot;"),
            _ => output.push(character),
        }
    }
}

#[cfg(test)]
mod export_html_contract_tests;
