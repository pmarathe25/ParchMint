//! Sanitization for untrusted clipboard content.

use crate::{DocumentPosition, EditorSelection};

/// Supported formatting retained from rich clipboard content.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PasteMarkKind {
    Bold,
    Italic,
    Underline,
    Strikethrough,
    Link(String),
}

/// One retained mark over scalar positions in sanitized text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PasteMark {
    pub range: EditorSelection,
    pub kind: PasteMarkKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PasteBlockKind {
    Paragraph,
    UnorderedListItem,
    OrderedListItem,
    BlockQuote,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PasteBlock {
    pub range: EditorSelection,
    pub kind: PasteBlockKind,
    pub list_depth: usize,
}

/// Clipboard content after unsupported and unsafe markup has been removed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SanitizedPaste {
    text: String,
    marks: Vec<PasteMark>,
    blocks: Vec<PasteBlock>,
    unsafe_content_removed: bool,
    omitted_images: usize,
}

impl SanitizedPaste {
    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn marks(&self) -> &[PasteMark] {
        &self.marks
    }

    pub fn blocks(&self) -> &[PasteBlock] {
        &self.blocks
    }

    pub const fn unsafe_content_removed(&self) -> bool {
        self.unsafe_content_removed
    }

    pub const fn omitted_images(&self) -> usize {
        self.omitted_images
    }
}

/// Plain or rich text supplied by an untrusted clipboard.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PasteSource<'a> {
    PlainText(&'a str),
    RichHtml(&'a str),
}

/// Sanitizes clipboard input before it reaches the editor core.
pub fn sanitize_paste(source: PasteSource<'_>) -> SanitizedPaste {
    match source {
        PasteSource::PlainText(text) => SanitizedPaste {
            text: normalize_newlines(text),
            marks: Vec::new(),
            blocks: Vec::new(),
            unsafe_content_removed: false,
            omitted_images: 0,
        },
        PasteSource::RichHtml(html) => sanitize_html(html),
    }
}

fn sanitize_html(html: &str) -> SanitizedPaste {
    let lower = html.to_ascii_lowercase();
    let mut text = String::new();
    let mut marks = Vec::new();
    let mut blocks = Vec::new();
    let mut open_block: Option<(PasteBlockKind, usize, usize)> = None;
    let mut lists: Vec<String> = Vec::new();
    let mut quote_depth = 0usize;
    let mut open_marks: Vec<(String, usize, PasteMarkKind)> = Vec::new();
    let mut unsafe_content_removed = false;
    let mut omitted_images = 0;
    let mut cursor = 0;

    while cursor < html.len() {
        if html.as_bytes()[cursor] != b'<' {
            let next = html[cursor..]
                .find('<')
                .map_or(html.len(), |offset| cursor + offset);
            text.push_str(&decode_entities(&html[cursor..next]));
            cursor = next;
            continue;
        }
        let Some(relative_end) = html[cursor..].find('>') else {
            text.push_str(&decode_entities(&html[cursor..]));
            break;
        };
        let end = cursor + relative_end;
        let raw_tag = html[cursor + 1..end].trim();
        let normalized = raw_tag.to_ascii_lowercase();
        let closing = normalized.starts_with('/');
        let name = normalized
            .trim_start_matches('/')
            .split_ascii_whitespace()
            .next()
            .unwrap_or_default()
            .trim_end_matches('/');

        if matches!(name, "script" | "style") && !closing {
            unsafe_content_removed = true;
            let closing_tag = format!("</{name}>");
            if let Some(relative_close) = lower[end + 1..].find(&closing_tag) {
                cursor = end + 1 + relative_close + closing_tag.len();
            } else {
                cursor = html.len();
            }
            continue;
        }
        if name != "a"
            && matches!(
                name,
                "br" | "p"
                    | "div"
                    | "h1"
                    | "h2"
                    | "h3"
                    | "blockquote"
                    | "ul"
                    | "ol"
                    | "li"
                    | "strong"
                    | "b"
                    | "em"
                    | "i"
                    | "u"
                    | "s"
                    | "strike"
                    | "del"
            )
            && tag_has_attributes(raw_tag, name)
        {
            unsafe_content_removed = true;
        }
        match (closing, name) {
            (false, "img") => omitted_images += 1,
            (_, "br") => push_line_break(&mut text),
            (false, "blockquote") => {
                finish_paste_block(&mut open_block, text.chars().count(), &mut blocks);
                push_paragraph_break(&mut text);
                quote_depth += 1;
                open_block = Some((PasteBlockKind::BlockQuote, text.chars().count(), 0));
            }
            (true, "blockquote") => {
                finish_paste_block(&mut open_block, text.chars().count(), &mut blocks);
                quote_depth = quote_depth.saturating_sub(1);
                push_paragraph_break(&mut text);
            }
            (false, "ul" | "ol") => {
                finish_paste_block(&mut open_block, text.chars().count(), &mut blocks);
                lists.push(name.into());
            }
            (true, "ul" | "ol") => {
                if lists.pop().as_deref() != Some(name) {
                    unsafe_content_removed = true;
                }
            }
            (false, "li") => {
                finish_paste_block(&mut open_block, text.chars().count(), &mut blocks);
                push_paragraph_break(&mut text);
                let previous_depth = blocks.last().and_then(|block: &PasteBlock| {
                    matches!(
                        block.kind,
                        PasteBlockKind::UnorderedListItem | PasteBlockKind::OrderedListItem
                    )
                    .then_some(block.list_depth)
                });
                let requested_depth = lists.len().saturating_sub(1);
                let list_depth = previous_depth
                    .map(|depth| requested_depth.min(depth.saturating_add(1)))
                    .unwrap_or(0);
                if list_depth != requested_depth {
                    unsafe_content_removed = true;
                }
                let kind = if lists.last().map(String::as_str) == Some("ol") {
                    PasteBlockKind::OrderedListItem
                } else {
                    PasteBlockKind::UnorderedListItem
                };
                open_block = Some((kind, text.chars().count(), list_depth));
            }
            (true, "li") => {
                finish_paste_block(&mut open_block, text.chars().count(), &mut blocks);
                push_paragraph_break(&mut text);
            }
            (false, "p" | "div" | "h1" | "h2" | "h3") => {
                let keep_empty_quote = quote_depth > 0
                    && open_block
                        .as_ref()
                        .is_some_and(|(_, start, _)| *start == text.chars().count());
                if !keep_empty_quote {
                    finish_paste_block(&mut open_block, text.chars().count(), &mut blocks);
                    push_paragraph_break(&mut text);
                    open_block = Some((
                        if quote_depth > 0 {
                            PasteBlockKind::BlockQuote
                        } else {
                            PasteBlockKind::Paragraph
                        },
                        text.chars().count(),
                        0,
                    ));
                }
            }
            (true, "p" | "div" | "h1" | "h2" | "h3") => {
                finish_paste_block(&mut open_block, text.chars().count(), &mut blocks);
                push_paragraph_break(&mut text);
            }
            (false, "strong" | "b") => {
                if tag_has_attributes(raw_tag, name) {
                    unsafe_content_removed = true;
                }
                open_marks.push((name.into(), text.chars().count(), PasteMarkKind::Bold));
            }
            (false, "em" | "i") => {
                if tag_has_attributes(raw_tag, name) {
                    unsafe_content_removed = true;
                }
                open_marks.push((name.into(), text.chars().count(), PasteMarkKind::Italic));
            }
            (false, "u") => {
                if tag_has_attributes(raw_tag, name) {
                    unsafe_content_removed = true;
                }
                open_marks.push((name.into(), text.chars().count(), PasteMarkKind::Underline));
            }
            (false, "s" | "strike" | "del") => {
                if tag_has_attributes(raw_tag, name) {
                    unsafe_content_removed = true;
                }
                open_marks.push((
                    name.into(),
                    text.chars().count(),
                    PasteMarkKind::Strikethrough,
                ));
            }
            (false, "a") => {
                if let Some(link) = safe_href(raw_tag) {
                    if raw_tag.matches('=').count() != 1 {
                        unsafe_content_removed = true;
                    }
                    open_marks.push((name.into(), text.chars().count(), PasteMarkKind::Link(link)));
                } else {
                    unsafe_content_removed = true;
                }
            }
            (true, "strong" | "b" | "em" | "i" | "u" | "s" | "strike" | "del" | "a") => {
                close_mark(name, text.chars().count(), &mut open_marks, &mut marks);
            }
            _ => unsafe_content_removed = true,
        }
        cursor = end + 1;
    }

    finish_paste_block(&mut open_block, text.chars().count(), &mut blocks);

    let trimmed = text.trim_matches('\n').to_owned();
    let removed_prefix = text
        .chars()
        .take_while(|character| *character == '\n')
        .count();
    let trimmed_len = trimmed.chars().count();
    marks.retain_mut(|mark| {
        let start = mark
            .range
            .start()
            .value()
            .saturating_sub(removed_prefix as u64);
        let end = mark
            .range
            .end()
            .value()
            .saturating_sub(removed_prefix as u64);
        if start >= end || end > trimmed_len as u64 {
            return false;
        }
        mark.range =
            EditorSelection::new(DocumentPosition::from(start), DocumentPosition::from(end));
        true
    });
    blocks.retain_mut(|block| {
        let start = block
            .range
            .start()
            .value()
            .saturating_sub(removed_prefix as u64);
        let end = block
            .range
            .end()
            .value()
            .saturating_sub(removed_prefix as u64);
        if start > end || end > trimmed_len as u64 {
            return false;
        }
        block.range = EditorSelection::new(start.into(), end.into());
        true
    });
    SanitizedPaste {
        text: trimmed,
        marks,
        blocks,
        unsafe_content_removed,
        omitted_images,
    }
}

fn finish_paste_block(
    open: &mut Option<(PasteBlockKind, usize, usize)>,
    end: usize,
    output: &mut Vec<PasteBlock>,
) {
    if let Some((kind, start, list_depth)) = open.take()
        && start <= end
    {
        output.push(PasteBlock {
            range: EditorSelection::new((start as u64).into(), (end as u64).into()),
            kind,
            list_depth,
        });
    }
}

fn tag_has_attributes(raw: &str, name: &str) -> bool {
    raw.trim_start_matches('/')
        .strip_prefix(name)
        .is_some_and(|rest| !rest.trim_matches('/').trim().is_empty())
}

fn close_mark(
    name: &str,
    end: usize,
    open: &mut Vec<(String, usize, PasteMarkKind)>,
    output: &mut Vec<PasteMark>,
) {
    let Some(index) = open.iter().rposition(|(tag, _, _)| tags_match(tag, name)) else {
        return;
    };
    let (_, start, kind) = open.remove(index);
    if start < end {
        output.push(PasteMark {
            range: EditorSelection::new(
                DocumentPosition::from(start as u64),
                DocumentPosition::from(end as u64),
            ),
            kind,
        });
    }
}

fn tags_match(open: &str, close: &str) -> bool {
    open == close
        || matches!(
            (open, close),
            ("strong", "b")
                | ("b", "strong")
                | ("em", "i")
                | ("i", "em")
                | ("s", "strike")
                | ("strike", "s")
                | ("s", "del")
                | ("del", "s")
        )
}

fn safe_href(tag: &str) -> Option<String> {
    let lower = tag.to_ascii_lowercase();
    let href = lower.find("href")?;
    let after_name = tag.get(href + 4..)?.trim_start();
    let value = after_name.strip_prefix('=')?.trim_start();
    let (candidate, _) = if let Some(rest) = value.strip_prefix('"') {
        let end = rest.find('"')?;
        (&rest[..end], &rest[end + 1..])
    } else if let Some(rest) = value.strip_prefix('\'') {
        let end = rest.find('\'')?;
        (&rest[..end], &rest[end + 1..])
    } else {
        let end = value.find(char::is_whitespace).unwrap_or(value.len());
        (&value[..end], &value[end..])
    };
    let normalized = candidate.trim();
    let lowercase = normalized.to_ascii_lowercase();
    if lowercase.starts_with("https://")
        || lowercase.starts_with("http://")
        || lowercase.starts_with("mailto:")
        || lowercase.starts_with('#')
    {
        Some(normalized.to_owned())
    } else {
        None
    }
}

fn decode_entities(text: &str) -> String {
    text.replace("&nbsp;", " ")
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
}

fn normalize_newlines(text: &str) -> String {
    text.replace("\r\n", "\n").replace('\r', "\n")
}

fn push_line_break(text: &mut String) {
    if !text.ends_with('\n') {
        text.push('\n');
    }
}

fn push_paragraph_break(text: &mut String) {
    if text.is_empty() {
        return;
    }
    while text.ends_with("\n\n\n") {
        text.pop();
    }
    if text.ends_with("\n\n") {
        return;
    }
    if text.ends_with('\n') {
        text.push('\n');
    } else {
        text.push_str("\n\n");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn selection(start: u64, end: u64) -> EditorSelection {
        EditorSelection::new(start.into(), end.into())
    }

    #[test]
    fn rich_paste_removes_unsafe_content_and_retains_supported_structure() {
        let sanitized = sanitize_paste(PasteSource::RichHtml(
            "<p><strong>Keep</strong> <a href=\"javascript:drop()\">link</a></p><script>drop()</script><img src=x><p>Next</p>",
        ));

        assert_eq!(sanitized.text(), "Keep link\n\nNext");
        assert_eq!(sanitized.omitted_images(), 1);
        assert!(sanitized.unsafe_content_removed());
        assert_eq!(
            sanitized.marks(),
            &[PasteMark {
                range: selection(0, 4),
                kind: PasteMarkKind::Bold,
            }]
        );
        assert_eq!(
            sanitized.blocks(),
            &[
                PasteBlock {
                    range: selection(0, 9),
                    kind: PasteBlockKind::Paragraph,
                    list_depth: 0,
                },
                PasteBlock {
                    range: selection(11, 15),
                    kind: PasteBlockKind::Paragraph,
                    list_depth: 0,
                },
            ]
        );

        let structured = sanitize_paste(PasteSource::RichHtml(
            "<ul><li>top<ul><li><em>nested</em></li></ul></li></ul><blockquote>q<br>x</blockquote>",
        ));
        assert_eq!(
            structured
                .blocks()
                .iter()
                .map(|block| (block.kind, block.list_depth))
                .collect::<Vec<_>>(),
            vec![
                (PasteBlockKind::UnorderedListItem, 0),
                (PasteBlockKind::UnorderedListItem, 1),
                (PasteBlockKind::BlockQuote, 0),
            ]
        );
        assert_eq!(structured.marks().len(), 1);
    }

    #[test]
    fn plain_paste_normalizes_newlines_without_marking_content_unsafe() {
        let sanitized = sanitize_paste(PasteSource::PlainText("one\r\n\r\ntwo"));

        assert_eq!(sanitized.text(), "one\n\ntwo");
        assert!(sanitized.marks().is_empty());
        assert!(sanitized.blocks().is_empty());
        assert!(!sanitized.unsafe_content_removed());
        assert_eq!(sanitized.omitted_images(), 0);
    }
}
