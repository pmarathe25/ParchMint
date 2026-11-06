//! Bounded HTML text extraction. No scripts, stylesheets, or external
//! resources are executed or fetched; preserved source bytes are untouched.

use std::cell::RefCell;

use html5ever::{
    tendril::StrTendril,
    tokenizer::{
        BufferQueue, EndTag, StartTag, Token, TokenSink, TokenSinkResult, Tokenizer, TokenizerOpts,
        states::RawKind,
    },
};
use scraper::{Html, Node};

use crate::research_text::{
    EncodingDeclarations, ExtractionError, MAX_TEXT_BYTES, encoding_label, mime_encoding,
};

// Reject pathological markup before building a tree. The source remains saved
// and the caller reports that it could not be searched.
const MAX_TAG_MARKERS: usize = 16_384;
const MAX_TOKENS: usize = 32_768;
const MAX_OPEN_ELEMENTS: usize = 512;
const PREFLIGHT_CHUNK_BYTES: usize = 8192;
const MAX_MARKUP_BYTES: usize = 4096;
const MAX_MARKUP_WORK: usize = 64 * 1024 * 1024;

/// Bound attribute-heavy tags before a tokenizer can perform duplicate-name
/// checks. Quoted `>` bytes do not end a tag. The squared-length budget also
/// bounds the total work from many individually permitted tags.
fn bound_markup(input: &str) -> Result<(), ExtractionError> {
    #[derive(Clone, Copy)]
    enum State {
        TagName,
        BeforeAttribute,
        AttributeName,
        AfterAttribute,
        BeforeValue,
        Quoted(u8),
        Unquoted,
        AfterQuoted,
    }
    fn name_start(byte: u8) -> bool {
        byte.is_ascii_alphabetic() || byte >= 0x80 || matches!(byte, b'_' | b':')
    }
    let bytes = input.as_bytes();
    let mut index = 0;
    let mut work = 0_usize;
    while index < bytes.len() {
        let Some(&next) = bytes.get(index + 1) else {
            break;
        };
        if bytes[index] != b'<'
            || !(name_start(next)
                || next == b'/' && bytes.get(index + 2).is_some_and(|byte| name_start(*byte)))
        {
            index += 1;
            continue;
        }
        let start = index;
        let mut cursor = index + 1 + usize::from(next == b'/');
        let mut state = State::TagName;
        while cursor < bytes.len() {
            if cursor - start >= MAX_MARKUP_BYTES {
                return Err(ExtractionError::TooLarge);
            }
            let byte = bytes[cursor];
            let whitespace = byte.is_ascii_whitespace();
            if byte == b'>' && !matches!(state, State::Quoted(_)) {
                cursor += 1;
                break;
            }
            state = match state {
                State::TagName if whitespace || byte == b'/' => State::BeforeAttribute,
                State::TagName => State::TagName,
                State::BeforeAttribute if whitespace || byte == b'/' => State::BeforeAttribute,
                State::BeforeAttribute => State::AttributeName,
                State::AttributeName if whitespace => State::AfterAttribute,
                State::AttributeName if byte == b'/' => State::BeforeAttribute,
                State::AttributeName if byte == b'=' => State::BeforeValue,
                State::AttributeName => State::AttributeName,
                State::AfterAttribute if whitespace => State::AfterAttribute,
                State::AfterAttribute if byte == b'/' => State::BeforeAttribute,
                State::AfterAttribute if byte == b'=' => State::BeforeValue,
                State::AfterAttribute => State::AttributeName,
                State::BeforeValue if whitespace => State::BeforeValue,
                State::BeforeValue if matches!(byte, b'\'' | b'"') => State::Quoted(byte),
                State::BeforeValue => State::Unquoted,
                State::Quoted(delimiter) if byte == delimiter => State::AfterQuoted,
                State::Quoted(delimiter) => State::Quoted(delimiter),
                State::Unquoted if whitespace => State::BeforeAttribute,
                State::Unquoted => State::Unquoted,
                State::AfterQuoted if whitespace || byte == b'/' => State::BeforeAttribute,
                State::AfterQuoted => State::AttributeName,
            };
            cursor += 1;
        }
        let length = cursor - start;
        work = work.saturating_add(length.saturating_mul(length));
        if work > MAX_MARKUP_WORK {
            return Err(ExtractionError::TooLarge);
        }
        // Scan every possible element independently: a comment, malformed tag,
        // or quoted string must not conceal a later expensive tag.
        index += 1;
    }
    Ok(())
}

fn void_element(name: &str) -> bool {
    matches!(
        name,
        "area"
            | "base"
            | "br"
            | "col"
            | "embed"
            | "hr"
            | "img"
            | "input"
            | "link"
            | "meta"
            | "param"
            | "source"
            | "track"
            | "wbr"
    )
}

fn block_element(name: &str) -> bool {
    matches!(
        name,
        "address"
            | "article"
            | "aside"
            | "blockquote"
            | "br"
            | "dd"
            | "details"
            | "dialog"
            | "div"
            | "dl"
            | "dt"
            | "fieldset"
            | "figcaption"
            | "figure"
            | "footer"
            | "form"
            | "h1"
            | "h2"
            | "h3"
            | "h4"
            | "h5"
            | "h6"
            | "header"
            | "hr"
            | "li"
            | "main"
            | "nav"
            | "ol"
            | "p"
            | "pre"
            | "section"
            | "table"
            | "td"
            | "th"
            | "tr"
            | "ul"
    )
}

fn closes_paragraph(name: &str) -> bool {
    matches!(
        name,
        "address"
            | "article"
            | "aside"
            | "blockquote"
            | "div"
            | "dl"
            | "fieldset"
            | "footer"
            | "form"
            | "h1"
            | "h2"
            | "h3"
            | "h4"
            | "h5"
            | "h6"
            | "header"
            | "hr"
            | "main"
            | "nav"
            | "ol"
            | "p"
            | "pre"
            | "section"
            | "table"
            | "ul"
    )
}

fn inherently_hidden(name: &str) -> bool {
    matches!(
        name,
        "head"
            | "script"
            | "style"
            | "template"
            | "noscript"
            | "iframe"
            | "noembed"
            | "noframes"
            | "svg"
            | "math"
    )
}

fn inline_style_hidden(style: &str) -> bool {
    style.split(';').any(|declaration| {
        let Some((property, value)) = declaration.split_once(':') else {
            return false;
        };
        let value = value.split('!').next().unwrap_or("").trim();
        property.trim().eq_ignore_ascii_case("display") && value.eq_ignore_ascii_case("none")
            || property.trim().eq_ignore_ascii_case("visibility")
                && value.eq_ignore_ascii_case("hidden")
    })
}

#[derive(Default)]
struct PreflightState {
    tokens: usize,
    open: Vec<String>,
    exceeded: bool,
}

#[derive(Default)]
struct Preflight(RefCell<PreflightState>);

impl TokenSink for Preflight {
    type Handle = ();

    fn process_token(&self, token: Token, _line: u64) -> TokenSinkResult<()> {
        let mut state = self.0.borrow_mut();
        state.tokens = state.tokens.saturating_add(1);
        if state.tokens > MAX_TOKENS {
            state.exceeded = true;
        }
        if let Token::TagToken(tag) = token {
            let name = tag.name.as_ref();
            match tag.kind {
                StartTag => {
                    if closes_paragraph(name)
                        && let Some(index) = state.open.iter().rposition(|open| open == "p")
                    {
                        state.open.truncate(index);
                    }
                    if name == "body"
                        && let Some(index) = state.open.iter().rposition(|open| open == "head")
                    {
                        state.open.truncate(index);
                    }
                    if !void_element(name) {
                        if state.open.len() >= MAX_OPEN_ELEMENTS {
                            state.exceeded = true;
                        } else {
                            state.open.push(name.to_owned());
                        }
                    }
                    // The tree builder normally selects these tokenizer states.
                    return match name {
                        "script" => TokenSinkResult::RawData(RawKind::ScriptData),
                        "style" | "xmp" | "iframe" | "noembed" | "noframes" => {
                            TokenSinkResult::RawData(RawKind::Rawtext)
                        }
                        "title" | "textarea" => TokenSinkResult::RawData(RawKind::Rcdata),
                        "plaintext" => TokenSinkResult::Plaintext,
                        _ => TokenSinkResult::Continue,
                    };
                }
                EndTag => {
                    if let Some(index) = state.open.iter().rposition(|open| open == name) {
                        state.open.truncate(index);
                    }
                }
            }
        }
        TokenSinkResult::Continue
    }
}

fn preflight(input: &str) -> Result<(), ExtractionError> {
    bound_markup(input)?;
    if input
        .bytes()
        .filter(|byte| *byte == b'<')
        .take(MAX_TAG_MARKERS + 1)
        .count()
        > MAX_TAG_MARKERS
    {
        return Err(ExtractionError::TooLarge);
    }
    let queue = BufferQueue::default();
    let tokenizer = Tokenizer::new(Preflight::default(), TokenizerOpts::default());
    let mut start = 0;
    while start < input.len() {
        let mut end = (start + PREFLIGHT_CHUNK_BYTES).min(input.len());
        while !input.is_char_boundary(end) {
            end -= 1;
        }
        queue.push_back(StrTendril::from(&input[start..end]));
        let _ = tokenizer.feed(&queue);
        if tokenizer.sink.0.borrow().exceeded {
            return Err(ExtractionError::TooLarge);
        }
        start = end;
    }
    tokenizer.end();
    if tokenizer.sink.0.borrow().exceeded {
        Err(ExtractionError::TooLarge)
    } else {
        Ok(())
    }
}

fn separate(text: &mut String) -> Result<(), ExtractionError> {
    if !text.is_empty() && !text.ends_with('\n') {
        if text.len() >= MAX_TEXT_BYTES {
            return Err(ExtractionError::TooLarge);
        }
        text.push('\n');
    }
    Ok(())
}

fn html_encoding(input: &str, has_bom: bool) -> Result<EncodingDeclarations, ExtractionError> {
    // HTML encoding declarations must appear within the first 1024 bytes.
    let mut end = input.len().min(1024 - if has_bom { 3 } else { 0 });
    while !input.is_char_boundary(end) {
        end -= 1;
    }
    let document = std::panic::catch_unwind(|| Html::parse_document(&input[..end]))
        .map_err(|_| ExtractionError::InvalidMarkup)?;
    let mut declarations = EncodingDeclarations::default();
    for node in document.tree.root().descendants() {
        let Node::Element(element) = node.value() else {
            continue;
        };
        if element.name() != "head" {
            continue;
        }
        for child in node.children() {
            let Node::Element(meta) = child.value() else {
                continue;
            };
            if meta.name() != "meta" {
                continue;
            }
            if let Some(label) = meta.attr("charset") {
                declarations.add(encoding_label(label)?)?;
            }
            if meta
                .attr("http-equiv")
                .is_some_and(|value| value.trim().eq_ignore_ascii_case("content-type"))
                && let Some(content) = meta.attr("content")
                && let Some(value) = mime_encoding(content)?
            {
                declarations.add(value)?;
            }
        }
    }
    Ok(declarations)
}

pub(super) fn extract(
    input: &str,
    mut encoding: EncodingDeclarations,
    has_bom: bool,
) -> Result<String, ExtractionError> {
    preflight(input)?;
    encoding.merge(html_encoding(input, has_bom)?)?;
    encoding.validate(input, true)?;
    let document = std::panic::catch_unwind(|| Html::parse_document(input))
        .map_err(|_| ExtractionError::InvalidMarkup)?;
    let mut text = String::new();
    let mut pending = vec![(document.tree.root(), false)];
    while let Some((node, exiting)) = pending.pop() {
        match node.value() {
            Node::Element(element) => {
                let name = element.name();
                if exiting {
                    if block_element(name) {
                        separate(&mut text)?;
                    }
                    continue;
                }
                let hidden = inherently_hidden(name)
                    || element.attr("hidden").is_some()
                    || element
                        .attr("aria-hidden")
                        .is_some_and(|value| value.trim().eq_ignore_ascii_case("true"))
                    || element.attr("style").is_some_and(inline_style_hidden);
                if hidden {
                    continue;
                }
                if block_element(name) {
                    separate(&mut text)?;
                }
                pending.push((node, true));
                pending.extend(node.children().rev().map(|child| (child, false)));
            }
            Node::Text(content) if !exiting => {
                if text.len().saturating_add(content.len()) > MAX_TEXT_BYTES {
                    return Err(ExtractionError::TooLarge);
                }
                text.push_str(content);
            }
            Node::Document | Node::Fragment if !exiting => {
                pending.extend(node.children().rev().map(|child| (child, false)));
            }
            _ => {}
        }
        if text.len() > MAX_TEXT_BYTES {
            return Err(ExtractionError::TooLarge);
        }
    }
    Ok(text)
}

/// XHTML has XML parsing rules: unlike HTML, self-closing non-void elements
/// close, and external or custom DTD entities are not resolved.
pub(super) fn extract_xhtml(
    input: &str,
    mut encoding: EncodingDeclarations,
) -> Result<String, ExtractionError> {
    const XHTML_NS: &str = "http://www.w3.org/1999/xhtml";
    bound_markup(input)?;
    let mut reader = quick_xml::Reader::from_str(input);
    if let quick_xml::events::Event::Decl(declaration) = reader
        .read_event()
        .map_err(|_| ExtractionError::InvalidMarkup)?
        && let Some(label) = declaration.encoding()
    {
        let label = label.map_err(|_| ExtractionError::InvalidMarkup)?;
        let label =
            std::str::from_utf8(&label).map_err(|_| ExtractionError::UnsupportedEncoding)?;
        encoding.add(encoding_label(label)?)?;
    }
    encoding.validate(input, false)?;
    if input
        .bytes()
        .filter(|byte| *byte == b'<')
        .take(MAX_TAG_MARKERS + 1)
        .count()
        > MAX_TAG_MARKERS
    {
        return Err(ExtractionError::TooLarge);
    }
    let document = roxmltree::Document::parse_with_options(
        input,
        roxmltree::ParsingOptions {
            allow_dtd: false,
            nodes_limit: MAX_TOKENS as u32,
        },
    )
    .map_err(|error| match error {
        roxmltree::Error::NodesLimitReached => ExtractionError::TooLarge,
        _ => ExtractionError::InvalidMarkup,
    })?;
    let root = document.root_element();
    if root.tag_name().name() != "html" || root.tag_name().namespace() != Some(XHTML_NS) {
        return Err(ExtractionError::InvalidMarkup);
    }
    let mut text = String::new();
    let mut pending = vec![(root, false, 0_usize)];
    while let Some((node, exiting, depth)) = pending.pop() {
        if depth > MAX_OPEN_ELEMENTS {
            return Err(ExtractionError::TooLarge);
        }
        if node.is_element() {
            if node.tag_name().namespace() != Some(XHTML_NS) {
                continue;
            }
            let name = node.tag_name().name();
            if exiting {
                if block_element(name) {
                    separate(&mut text)?;
                }
                continue;
            }
            if inherently_hidden(name)
                || node.attribute("hidden").is_some()
                || node
                    .attribute("aria-hidden")
                    .is_some_and(|value| value.trim().eq_ignore_ascii_case("true"))
                || node.attribute("style").is_some_and(inline_style_hidden)
            {
                continue;
            }
            if block_element(name) {
                separate(&mut text)?;
            }
            pending.push((node, true, depth));
            pending.extend(node.children().rev().map(|child| (child, false, depth + 1)));
        } else if node.is_text() && !exiting {
            let content = node.text().unwrap_or("");
            if text.len().saturating_add(content.len()) > MAX_TEXT_BYTES {
                return Err(ExtractionError::TooLarge);
            }
            text.push_str(content);
        }
        if text.len() > MAX_TEXT_BYTES {
            return Err(ExtractionError::TooLarge);
        }
    }
    Ok(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn extract(input: &str) -> Result<String, ExtractionError> {
        let mut encoding = EncodingDeclarations::default();
        encoding
            .add(crate::research_text::SourceEncoding::Utf8)
            .unwrap();
        super::extract(input, encoding, false)
    }

    fn extract_xhtml(input: &str) -> Result<String, ExtractionError> {
        super::extract_xhtml(input, EncodingDeclarations::default())
    }

    #[test]
    fn extracts_text_entities_and_boundaries_without_scripts_or_hidden_content() {
        let html = r#"<!doctype html><html><head><title>Not body</title><style>p {color:red}</style></head>
            <body><h1>Harbor &amp; café</h1><p>The <b>keeper</b> left<br>at dawn.</p>
            <script>if (x < 4) { document.write("<p>not evidence</p>"); }</script>
            <template><p>template secret</p></template><p hidden>hidden secret</p>
            <div style="display : none !important">style secret</div><p>Last &#x1F30A;.</p></body></html>"#;
        let text = extract(html).unwrap();
        assert!(text.contains("Harbor & café"));
        assert!(text.contains("The keeper left\nat dawn."));
        assert!(text.contains("Last 🌊."));
        for hidden in [
            "Not body",
            "color:red",
            "not evidence",
            "template secret",
            "hidden secret",
            "style secret",
        ] {
            assert!(!text.contains(hidden), "unexpected {hidden}: {text}");
        }
    }

    #[test]
    fn browser_tree_semantics_handle_misnested_and_omitted_end_tags() {
        let text = extract("<div><template></div>HIDDEN</template></div><p>Visible</p>").unwrap();
        assert!(!text.contains("HIDDEN"));
        assert!(text.contains("Visible"));
        let text = extract("<html><head><title>Title</title><body><p>Visible article</p>").unwrap();
        assert!(text.contains("Visible article"));
        let text = extract("<p hidden>Hidden<p>Visible").unwrap();
        assert!(!text.contains("Hidden"));
        assert!(text.contains("Visible"));
        let text = extract("<p>One &amp; two<p>Three").unwrap();
        assert!(text.contains("One & two\nThree"));
    }

    #[test]
    fn rejects_markup_work_and_depth_before_building_a_tree() {
        let nested = format!("{}content", "<div>".repeat(MAX_OPEN_ELEMENTS + 1));
        assert_eq!(extract(&nested), Err(ExtractionError::TooLarge));
        let many = "</unmatched>".repeat(MAX_TAG_MARKERS + 1);
        assert_eq!(extract(&many), Err(ExtractionError::TooLarge));
        let attributes = (0..2000)
            .map(|number| format!(" a{number}=\"\""))
            .collect::<String>();
        let tag = format!("<div{attributes}>content</div>");
        assert_eq!(bound_markup(&tag), Err(ExtractionError::TooLarge));
        assert_eq!(extract(&tag), Err(ExtractionError::TooLarge));
        let after_comment = format!("<!-- \" --><div title=\">\"{attributes}>content</div>");
        assert_eq!(extract(&after_comment), Err(ExtractionError::TooLarge));
        let xhtml_after_comment = format!(
            "<html xmlns=\"http://www.w3.org/1999/xhtml\"><body>{after_comment}</body></html>"
        );
        assert_eq!(
            extract_xhtml(&xhtml_after_comment),
            Err(ExtractionError::TooLarge)
        );
        let malformed_attribute = format!("<div bad\" title=\">\"{attributes}>content</div>");
        assert_eq!(
            extract(&malformed_attribute),
            Err(ExtractionError::TooLarge)
        );
        let many_large_tags = format!("<i data-x='{}'>x</i>", "a".repeat(1000)).repeat(70);
        assert_eq!(
            bound_markup(&many_large_tags),
            Err(ExtractionError::TooLarge)
        );
        assert!(bound_markup("<p title='one > two'>text</p>").is_ok());
        let mut full = "x".repeat(MAX_TEXT_BYTES);
        assert_eq!(separate(&mut full), Err(ExtractionError::TooLarge));
    }

    #[test]
    fn xhtml_uses_xml_self_closing_and_never_resolves_dtd_entities() {
        let xml = r#"<html xmlns="http://www.w3.org/1999/xhtml"><body><p>First<br/>Second<span hidden="hidden">hidden</span></p><p>After</p></body></html>"#;
        let text = extract_xhtml(xml).unwrap();
        assert!(text.contains("First\nSecond"));
        assert!(text.contains("After"));
        assert!(!text.contains("hidden"));
        let external = r#"<!DOCTYPE html [<!ENTITY x SYSTEM "file:///secret">]><html xmlns="http://www.w3.org/1999/xhtml"><body>&x;</body></html>"#;
        assert_eq!(extract_xhtml(external), Err(ExtractionError::InvalidMarkup));
        assert_eq!(
            extract_xhtml("<html><body>missing namespace</body></html>"),
            Err(ExtractionError::InvalidMarkup)
        );
    }
}
