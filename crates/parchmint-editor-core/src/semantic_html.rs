use std::collections::BTreeMap;

use crate::document_engine::{EngineMark, SemanticBlockSnapshot, SemanticDocumentSnapshot};
use crate::{BlockId, SemanticBlockKind, SemanticInlineMark};

pub(super) fn parse(
    body: &str,
    primary: BlockId,
) -> Result<SemanticDocumentSnapshot, &'static str> {
    if !body.contains('<') {
        return Ok(SemanticDocumentSnapshot {
            blocks: vec![SemanticBlockSnapshot {
                id: primary,
                kind: SemanticBlockKind::Paragraph,
                attributes: BTreeMap::new(),
                text: body.to_owned(),
                marks: Vec::new(),
                list_depth: 0,
            }],
            canonical_html: false,
        });
    }

    let mut blocks = Vec::new();
    let mut current: Option<SemanticBlockSnapshot> = None;
    let mut open_marks: Vec<(String, usize, SemanticInlineMark)> = Vec::new();
    let mut lists = Vec::new();
    let mut cursor = 0usize;
    while cursor < body.len() {
        if body.as_bytes()[cursor] != b'<' {
            let next = body[cursor..]
                .find('<')
                .map_or(body.len(), |offset| cursor + offset);
            let decoded = decode_entities(&body[cursor..next])?;
            if !decoded.is_empty() {
                ensure_block(
                    &mut current,
                    primary,
                    blocks.len(),
                    SemanticBlockKind::Paragraph,
                    BTreeMap::new(),
                );
                current.as_mut().expect("block").text.push_str(&decoded);
            }
            cursor = next;
            continue;
        }
        let end = find_tag_end(body, cursor + 1)?;
        let raw = body[cursor + 1..end].trim();
        if raw.starts_with('!') || raw.starts_with('?') {
            return Err("unsupported canonical HTML declaration");
        }
        let closing = raw.starts_with('/');
        let token = raw.trim_start_matches('/').trim_end_matches('/').trim();
        let name_end = token.find(char::is_whitespace).unwrap_or(token.len());
        let name = token[..name_end].to_ascii_lowercase();
        let attributes = if closing {
            BTreeMap::new()
        } else {
            parse_attributes(&name, &token[name_end..])?
        };
        match (closing, name.as_str()) {
            (false, "p" | "h1" | "h2" | "h3" | "blockquote" | "li") => {
                let nested_paragraph = name == "p"
                    && current.as_ref().is_some_and(|block| {
                        block.text.is_empty()
                            && matches!(
                                block.kind,
                                SemanticBlockKind::BlockQuote
                                    | SemanticBlockKind::UnorderedListItem
                                    | SemanticBlockKind::OrderedListItem
                            )
                    });
                if !nested_paragraph {
                    finish_block(&mut current, &mut open_marks, &mut blocks);
                    let kind = block_kind(&name, lists.last().map(String::as_str));
                    ensure_block(&mut current, primary, blocks.len(), kind, attributes);
                    if name == "li"
                        && let Some(block) = current.as_mut()
                    {
                        block.list_depth = lists.len().saturating_sub(1);
                    }
                }
            }
            (true, "p" | "h1" | "h2" | "h3" | "blockquote" | "li") => {
                finish_block(&mut current, &mut open_marks, &mut blocks);
            }
            (false, "ul" | "ol") => {
                if !lists.is_empty()
                    && !current.as_ref().is_some_and(|block| {
                        matches!(
                            block.kind,
                            SemanticBlockKind::UnorderedListItem
                                | SemanticBlockKind::OrderedListItem
                        )
                    })
                {
                    return Err("nested canonical list is outside a list item");
                }
                lists.push(name);
            }
            (true, "ul" | "ol") => {
                if lists.pop().as_deref() != Some(name.as_str()) {
                    return Err("mismatched canonical HTML list");
                }
            }
            (false, "br") => {
                ensure_block(
                    &mut current,
                    primary,
                    blocks.len(),
                    SemanticBlockKind::Paragraph,
                    BTreeMap::new(),
                );
                current.as_mut().expect("block").text.push('\n');
            }
            (false, "hr") => {
                finish_block(&mut current, &mut open_marks, &mut blocks);
                let kind = match attributes.get("data-kind").map(String::as_str) {
                    Some("page-break") => SemanticBlockKind::PageBreak,
                    _ => SemanticBlockKind::SceneBreak,
                };
                ensure_block(&mut current, primary, blocks.len(), kind, attributes);
                finish_block(&mut current, &mut open_marks, &mut blocks);
            }
            (false, "strong" | "em" | "u" | "s" | "sup" | "sub" | "a" | "span") => {
                ensure_block(
                    &mut current,
                    primary,
                    blocks.len(),
                    SemanticBlockKind::Paragraph,
                    BTreeMap::new(),
                );
                let Some(mark) = inline_mark(&name, &attributes)? else {
                    cursor = end + 1;
                    continue;
                };
                let start = current.as_ref().expect("block").text.chars().count();
                open_marks.push((name, start, mark));
            }
            (true, "strong" | "em" | "u" | "s" | "sup" | "sub" | "a" | "span") => {
                let block = current
                    .as_mut()
                    .ok_or("inline element outside a text block")?;
                let index = open_marks
                    .iter()
                    .rposition(|(tag, _, _)| tag == &name)
                    .ok_or("mismatched canonical inline element")?;
                let (_, start, mark) = open_marks.remove(index);
                let finish = block.text.chars().count();
                if start < finish {
                    block.marks.push(EngineMark {
                        start,
                        end: finish,
                        mark,
                    });
                }
            }
            _ => return Err("unsupported canonical HTML element"),
        }
        cursor = end + 1;
    }
    finish_block(&mut current, &mut open_marks, &mut blocks);
    if !lists.is_empty() || !open_marks.is_empty() {
        return Err("unclosed canonical HTML element");
    }
    if blocks.is_empty() {
        return Err("canonical HTML has no semantic blocks");
    }
    Ok(SemanticDocumentSnapshot {
        blocks,
        canonical_html: true,
    })
}

pub(super) fn serialize(document: &SemanticDocumentSnapshot) -> String {
    if !document.canonical_html {
        return document.plain_text();
    }
    let mut output = String::new();
    let mut index = 0usize;
    while index < document.blocks.len() {
        let block = &document.blocks[index];
        if is_list_item(block.kind) {
            if block.list_depth != 0 {
                return String::new();
            }
            render_list(&document.blocks, &mut index, 0, &mut output);
            continue;
        }
        let tag = match block.kind {
            SemanticBlockKind::Paragraph => "p",
            SemanticBlockKind::Heading1 => "h1",
            SemanticBlockKind::Heading2 => "h2",
            SemanticBlockKind::Heading3 => "h3",
            SemanticBlockKind::BlockQuote => "blockquote",
            SemanticBlockKind::UnorderedListItem | SemanticBlockKind::OrderedListItem => {
                unreachable!()
            }
            SemanticBlockKind::SceneBreak | SemanticBlockKind::PageBreak => {
                output.push_str("<hr");
                write_attributes(&block.attributes, &mut output);
                output.push('>');
                index += 1;
                continue;
            }
        };
        output.push('<');
        output.push_str(tag);
        write_attributes(&block.attributes, &mut output);
        output.push('>');
        render_inline(block, &mut output);
        output.push_str("</");
        output.push_str(tag);
        output.push('>');
        index += 1;
    }
    output
}

fn render_list(
    blocks: &[SemanticBlockSnapshot],
    index: &mut usize,
    depth: usize,
    output: &mut String,
) {
    let container = list_tag(blocks[*index].kind);
    output.push('<');
    output.push_str(container);
    output.push('>');
    while *index < blocks.len() {
        let block = &blocks[*index];
        if !is_list_item(block.kind)
            || block.list_depth != depth
            || list_tag(block.kind) != container
        {
            break;
        }
        output.push_str("<li");
        write_attributes(&block.attributes, output);
        output.push('>');
        render_inline(block, output);
        *index += 1;
        while *index < blocks.len()
            && is_list_item(blocks[*index].kind)
            && blocks[*index].list_depth == depth + 1
        {
            render_list(blocks, index, depth + 1, output);
        }
        output.push_str("</li>");
        if *index < blocks.len()
            && is_list_item(blocks[*index].kind)
            && blocks[*index].list_depth > depth
        {
            break;
        }
    }
    output.push_str("</");
    output.push_str(container);
    output.push('>');
}

fn is_list_item(kind: SemanticBlockKind) -> bool {
    matches!(
        kind,
        SemanticBlockKind::UnorderedListItem | SemanticBlockKind::OrderedListItem
    )
}

fn list_tag(kind: SemanticBlockKind) -> &'static str {
    match kind {
        SemanticBlockKind::UnorderedListItem => "ul",
        SemanticBlockKind::OrderedListItem => "ol",
        _ => unreachable!(),
    }
}

/// Serializes one scalar range as plain text and deterministic restricted HTML.
/// The caller validates the range against `document.plain_text()` first.
pub(super) fn serialize_selection(
    document: &SemanticDocumentSnapshot,
    start: usize,
    end: usize,
) -> (String, String) {
    let plain = document.plain_text().chars().collect::<Vec<_>>()[start..end]
        .iter()
        .collect::<String>();
    let mut blocks = Vec::new();
    let mut cursor = 0usize;

    for (index, block) in document.blocks.iter().enumerate() {
        let atomic = matches!(
            block.kind,
            SemanticBlockKind::SceneBreak | SemanticBlockKind::PageBreak
        );
        let content_len = if atomic {
            1
        } else {
            block.text.chars().count()
        };
        let block_start = cursor;
        let block_end = block_start + content_len;
        let content_start = start.max(block_start).min(block_end);
        let content_end = end.max(block_start).min(block_end);
        let has_content = content_start < content_end;
        let includes_leading_boundary = index > 0
            && start <= block_start.saturating_sub(1)
            && block_start.saturating_sub(1) < end;
        let includes_trailing_boundary =
            index + 1 < document.blocks.len() && start <= block_end && block_end < end;

        if has_content || includes_leading_boundary || includes_trailing_boundary {
            if atomic && has_content {
                blocks.push(block.clone());
            } else if atomic {
                blocks.push(empty_selection_block(block.id));
            } else {
                let local_start = content_start.saturating_sub(block_start);
                let local_end = content_end.saturating_sub(block_start);
                let text = block.text.chars().collect::<Vec<_>>()[local_start..local_end]
                    .iter()
                    .collect::<String>();
                let marks = block
                    .marks
                    .iter()
                    .filter_map(|mark| {
                        let mark_start = mark.start.max(local_start);
                        let mark_end = mark.end.min(local_end);
                        (mark_start < mark_end).then(|| EngineMark {
                            start: mark_start - local_start,
                            end: mark_end - local_start,
                            mark: mark.mark.clone(),
                        })
                    })
                    .collect();
                blocks.push(SemanticBlockSnapshot {
                    id: block.id,
                    kind: block.kind,
                    attributes: block.attributes.clone(),
                    text,
                    marks,
                    list_depth: block.list_depth,
                });
            }
        }

        cursor = block_end + usize::from(index + 1 < document.blocks.len());
    }

    let html = serialize(&SemanticDocumentSnapshot {
        blocks,
        canonical_html: true,
    });
    debug_assert_eq!(
        parse(&html, document.blocks[0].id)
            .map(|parsed| parsed.plain_text())
            .as_deref(),
        Ok(plain.as_str())
    );
    (plain, html)
}

fn empty_selection_block(id: BlockId) -> SemanticBlockSnapshot {
    SemanticBlockSnapshot {
        id,
        kind: SemanticBlockKind::Paragraph,
        attributes: BTreeMap::new(),
        text: String::new(),
        marks: Vec::new(),
        list_depth: 0,
    }
}

fn render_inline(block: &SemanticBlockSnapshot, output: &mut String) {
    let characters: Vec<char> = block.text.chars().collect();
    let mut active: Vec<SemanticInlineMark> = Vec::new();
    for offset in 0..=characters.len() {
        let mut next: Vec<_> = block
            .marks
            .iter()
            .filter(|mark| mark.start <= offset && offset < mark.end)
            .map(|mark| mark.mark.clone())
            .collect();
        next.sort();
        next.dedup();
        if next != active {
            for mark in active.iter().rev() {
                output.push_str(close_tag(mark));
            }
            for mark in &next {
                output.push_str(&open_tag(mark));
            }
            active = next;
        }
        if let Some(character) = characters.get(offset) {
            match character {
                '\n' => output.push_str("<br>"),
                '&' => output.push_str("&amp;"),
                '<' => output.push_str("&lt;"),
                '>' => output.push_str("&gt;"),
                _ => output.push(*character),
            }
        }
    }
}

fn open_tag(mark: &SemanticInlineMark) -> String {
    match mark {
        SemanticInlineMark::Bold => "<strong>".into(),
        SemanticInlineMark::Italic => "<em>".into(),
        SemanticInlineMark::Underline => "<u>".into(),
        SemanticInlineMark::Strikethrough => "<s>".into(),
        SemanticInlineMark::SmallCaps => "<span data-semantic=\"small-caps\">".into(),
        SemanticInlineMark::Superscript => "<sup>".into(),
        SemanticInlineMark::Subscript => "<sub>".into(),
        SemanticInlineMark::Link(href) => format!("<a href=\"{}\">", escape_attribute(href)),
    }
}

fn close_tag(mark: &SemanticInlineMark) -> &'static str {
    match mark {
        SemanticInlineMark::Bold => "</strong>",
        SemanticInlineMark::Italic => "</em>",
        SemanticInlineMark::Underline => "</u>",
        SemanticInlineMark::Strikethrough => "</s>",
        SemanticInlineMark::SmallCaps => "</span>",
        SemanticInlineMark::Superscript => "</sup>",
        SemanticInlineMark::Subscript => "</sub>",
        SemanticInlineMark::Link(_) => "</a>",
    }
}

fn block_kind(tag: &str, list: Option<&str>) -> SemanticBlockKind {
    match tag {
        "h1" => SemanticBlockKind::Heading1,
        "h2" => SemanticBlockKind::Heading2,
        "h3" => SemanticBlockKind::Heading3,
        "blockquote" => SemanticBlockKind::BlockQuote,
        "li" if list == Some("ol") => SemanticBlockKind::OrderedListItem,
        "li" => SemanticBlockKind::UnorderedListItem,
        _ => SemanticBlockKind::Paragraph,
    }
}

fn inline_mark(
    tag: &str,
    attributes: &BTreeMap<String, String>,
) -> Result<Option<SemanticInlineMark>, &'static str> {
    Ok(match tag {
        "strong" => Some(SemanticInlineMark::Bold),
        "em" => Some(SemanticInlineMark::Italic),
        "u" => Some(SemanticInlineMark::Underline),
        "s" => Some(SemanticInlineMark::Strikethrough),
        "sup" => Some(SemanticInlineMark::Superscript),
        "sub" => Some(SemanticInlineMark::Subscript),
        "a" => Some(SemanticInlineMark::Link(
            attributes
                .get("href")
                .cloned()
                .ok_or("canonical link has no href")?,
        )),
        "span" if attributes.get("data-semantic").map(String::as_str) == Some("small-caps") => {
            Some(SemanticInlineMark::SmallCaps)
        }
        "span" => None,
        _ => None,
    })
}

fn ensure_block(
    current: &mut Option<SemanticBlockSnapshot>,
    primary: BlockId,
    index: usize,
    kind: SemanticBlockKind,
    attributes: BTreeMap<String, String>,
) {
    if current.is_none() {
        let mut bytes = *primary.as_bytes();
        let index = index as u64;
        for (slot, byte) in bytes[8..].iter_mut().zip(index.to_be_bytes()) {
            *slot ^= byte;
        }
        *current = Some(SemanticBlockSnapshot {
            id: BlockId::from_bytes(bytes),
            kind,
            attributes,
            text: String::new(),
            marks: Vec::new(),
            list_depth: 0,
        });
    }
}

fn finish_block(
    current: &mut Option<SemanticBlockSnapshot>,
    open_marks: &mut Vec<(String, usize, SemanticInlineMark)>,
    blocks: &mut Vec<SemanticBlockSnapshot>,
) {
    if let Some(mut block) = current.take() {
        let end = block.text.chars().count();
        for (_, start, mark) in open_marks.drain(..) {
            if start < end {
                block.marks.push(EngineMark { start, end, mark });
            }
        }
        blocks.push(block);
    }
}

fn find_tag_end(html: &str, start: usize) -> Result<usize, &'static str> {
    let mut quote = None;
    for (offset, character) in html[start..].char_indices() {
        match (quote, character) {
            (Some(current), value) if current == value => quote = None,
            (None, '\'' | '"') => quote = Some(character),
            (None, '>') => return Ok(start + offset),
            _ => {}
        }
    }
    Err("unterminated canonical HTML element")
}

fn parse_attributes(tag: &str, mut source: &str) -> Result<BTreeMap<String, String>, &'static str> {
    let mut attributes = BTreeMap::new();
    while !source.trim_start().is_empty() {
        source = source.trim_start();
        let name_end = source
            .find(|character: char| character.is_whitespace() || character == '=')
            .unwrap_or(source.len());
        let name = source[..name_end].to_ascii_lowercase();
        source = source[name_end..].trim_start();
        source = source
            .strip_prefix('=')
            .ok_or("canonical HTML attribute has no value")?
            .trim_start();
        let quote = source
            .chars()
            .next()
            .filter(|value| matches!(value, '\'' | '"'))
            .ok_or("canonical HTML attribute is not quoted")?;
        let rest = &source[quote.len_utf8()..];
        let end = rest
            .find(quote)
            .ok_or("unterminated canonical HTML attribute")?;
        let value = decode_entities(&rest[..end])?;
        source = &rest[end + quote.len_utf8()..];
        let valid = match (tag, name.as_str()) {
            ("a", "href") => is_safe_href(&value),
            ("span", "data-semantic") => value == "small-caps",
            ("hr", "data-kind") => matches!(value.as_str(), "scene-break" | "page-break"),
            (_, "data-block-id" | "data-style-id") => is_safe_identifier(&value),
            _ => false,
        };
        if !valid || attributes.insert(name, value).is_some() {
            return Err("unsupported or duplicate canonical HTML attribute");
        }
    }
    Ok(attributes)
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

fn is_safe_identifier(value: &str) -> bool {
    !value.is_empty()
        && value.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.')
        })
}

fn decode_entities(source: &str) -> Result<String, &'static str> {
    let mut output = String::new();
    let mut rest = source;
    while let Some(start) = rest.find('&') {
        output.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        let end = after
            .find(';')
            .ok_or("unterminated canonical HTML entity")?;
        let entity = &after[..end];
        let character = match entity {
            "amp" => '&',
            "lt" => '<',
            "gt" => '>',
            "quot" => '"',
            "apos" | "#39" => '\'',
            _ if entity.starts_with("#x") => char::from_u32(
                u32::from_str_radix(&entity[2..], 16)
                    .map_err(|_| "invalid canonical HTML entity")?,
            )
            .ok_or("invalid canonical HTML entity")?,
            _ if entity.starts_with('#') => char::from_u32(
                entity[1..]
                    .parse()
                    .map_err(|_| "invalid canonical HTML entity")?,
            )
            .ok_or("invalid canonical HTML entity")?,
            _ => return Err("unsupported canonical HTML entity"),
        };
        output.push(character);
        rest = &after[end + 1..];
    }
    output.push_str(rest);
    Ok(output)
}

fn write_attributes(attributes: &BTreeMap<String, String>, output: &mut String) {
    for (name, value) in attributes {
        output.push(' ');
        output.push_str(name);
        output.push_str("=\"");
        output.push_str(&escape_attribute(value));
        output.push('"');
    }
}

fn escape_attribute(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('"', "&quot;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}
