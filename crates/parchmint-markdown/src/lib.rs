#![allow(missing_docs)]
//! Source-aware ParchMint Markdown 1 semantic codec.
//!
//! The codec deliberately keeps the original source for every unmodified block.
//! Once a block is edited its semantic representation is emitted with the stable
//! ParchMint spelling. Unsupported constructs remain protected opaque nodes.

use noyalib::compat::serde_yaml::{self as yaml, Mapping, Value};
use pulldown_cmark::{Event, Options as CmarkOptions, Parser};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::ops::Range;
use thiserror::Error;

pub const PAGE_BREAK_MARKER: &str = "<!-- parchmint:page-break -->";

#[derive(Clone, Debug, Default)]
pub struct ParseOptions {
    /// Style IDs known to the currently open project. Empty means unchecked.
    pub known_style_ids: BTreeSet<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DiagnosticSeverity {
    Warning,
    Error,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Diagnostic {
    pub severity: DiagnosticSeverity,
    pub code: &'static str,
    pub message: String,
    pub range: Range<usize>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Attributes {
    pub id: Option<String>,
    pub classes: Vec<String>,
    pub style_id: Option<String>,
    pub extra: BTreeMap<String, String>,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Alignment {
    #[default]
    Left,
    Center,
    Right,
    Justify,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Inline {
    Text(String),
    Emphasis(Vec<Self>),
    Strong(Vec<Self>),
    Strikethrough(Vec<Self>),
    Code(String),
    Link {
        label: Vec<Self>,
        destination: String,
        title: Option<String>,
    },
    Image {
        alt: String,
        destination: String,
        title: Option<String>,
    },
    Superscript(Vec<Self>),
    Subscript(Vec<Self>),
    Styled {
        children: Vec<Self>,
        attributes: Attributes,
    },
    SoftBreak,
    HardBreak,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ListItem {
    pub checked: Option<bool>,
    pub content: Vec<Inline>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BlockNode {
    Paragraph {
        content: Vec<Inline>,
        attributes: Attributes,
    },
    Heading {
        level: u8,
        content: Vec<Inline>,
        attributes: Attributes,
    },
    BlockQuote {
        source: String,
    },
    CodeBlock {
        info: String,
        text: String,
    },
    List {
        ordered: bool,
        start: u64,
        items: Vec<ListItem>,
    },
    Table {
        source: String,
    },
    Footnote {
        source: String,
    },
    ThematicBreak,
    Alignment {
        alignment: Alignment,
        attributes: Attributes,
        children: Vec<Block>,
    },
    PageBreak,
    Opaque {
        reason: String,
        source: String,
    },
}

impl BlockNode {
    pub const fn is_opaque(&self) -> bool {
        matches!(self, Self::Opaque { .. })
    }
}

/// Compatibility classification retained from the Stage 01 public boundary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BlockKind {
    Supported,
    PageBreak,
    Opaque,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Block {
    pub kind: BlockKind,
    pub range: Range<usize>,
    pub node: BlockNode,
    source: String,
    changed: bool,
}

impl Block {
    pub fn source(&self) -> &str {
        &self.source
    }

    pub const fn changed(&self) -> bool {
        self.changed
    }

    pub fn replace(&mut self, node: BlockNode) {
        self.kind = block_kind(&node);
        self.node = node;
        self.changed = true;
    }

    fn serialize(&self) -> String {
        if self.changed {
            serialize_node(&self.node)
        } else {
            self.source.clone()
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Document {
    raw_front_matter: String,
    front_matter: Mapping,
    blocks: Vec<Block>,
    diagnostics: Vec<Diagnostic>,
}

impl Document {
    pub fn parse(source: &str) -> Result<Self, MarkdownError> {
        Self::parse_with_options(source, &ParseOptions::default())
    }

    pub fn parse_with_options(source: &str, options: &ParseOptions) -> Result<Self, MarkdownError> {
        let (raw_front_matter, front_matter, body, body_offset, mut diagnostics) =
            parse_front_matter(source)?;
        validate_commonmark(body, body_offset)?;
        let mut blocks = scan_blocks(body, body_offset, &mut diagnostics);
        validate_extensions(&mut blocks, options, &mut diagnostics);
        Ok(Self {
            raw_front_matter,
            front_matter,
            blocks,
            diagnostics,
        })
    }

    /// Parses a storage-owned body which has no YAML front matter.
    pub fn parse_body(body: &str, options: &ParseOptions) -> Result<Self, MarkdownError> {
        validate_commonmark(body, 0)?;
        let mut diagnostics = Vec::new();
        let mut blocks = scan_blocks(body, 0, &mut diagnostics);
        validate_extensions(&mut blocks, options, &mut diagnostics);
        Ok(Self {
            raw_front_matter: String::new(),
            front_matter: Mapping::new(),
            blocks,
            diagnostics,
        })
    }

    pub const fn front_matter(&self) -> &Mapping {
        &self.front_matter
    }

    pub fn blocks(&self) -> &[Block] {
        &self.blocks
    }

    pub fn blocks_mut(&mut self) -> &mut [Block] {
        &mut self.blocks
    }

    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    pub fn replace_block(&mut self, index: usize, node: BlockNode) -> Result<(), MarkdownError> {
        let count = self.blocks.len();
        let block = self
            .blocks
            .get_mut(index)
            .ok_or(MarkdownError::BlockIndex { index, count })?;
        block.replace(node);
        Ok(())
    }

    pub fn insert_block(&mut self, index: usize, node: BlockNode) -> Result<(), MarkdownError> {
        if index > self.blocks.len() {
            return Err(MarkdownError::BlockIndex {
                index,
                count: self.blocks.len(),
            });
        }
        self.blocks.insert(
            index,
            Block {
                kind: block_kind(&node),
                range: 0..0,
                node,
                source: String::new(),
                changed: true,
            },
        );
        Ok(())
    }

    pub fn remove_block(&mut self, index: usize) -> Result<Block, MarkdownError> {
        if index >= self.blocks.len() {
            return Err(MarkdownError::BlockIndex {
                index,
                count: self.blocks.len(),
            });
        }
        Ok(self.blocks.remove(index))
    }

    /// Rewrites semantic references after an explicit style deletion/replacement.
    /// Display-name-only renames never call this method and therefore never churn documents.
    pub fn replace_style_references(&mut self, old: &str, replacement: &str) -> usize {
        let mut changed = 0;
        for block in &mut self.blocks {
            if replace_style_in_node(&mut block.node, old, replacement) {
                block.changed = true;
                changed += 1;
            }
        }
        changed
    }

    pub fn serialize_body(&self) -> String {
        self.blocks.iter().map(Block::serialize).collect()
    }

    pub fn serialize(&self) -> String {
        let body = self.serialize_body();
        if self.raw_front_matter.is_empty() {
            body
        } else {
            format!("---\n{}---\n{body}", self.raw_front_matter)
        }
    }
}

#[derive(Debug, Error)]
pub enum MarkdownError {
    #[error("front matter opened at byte 0 but has no closing delimiter")]
    UnclosedFrontMatter,
    #[error("invalid YAML front matter: {0}")]
    InvalidFrontMatter(String),
    #[error("Markdown parser emitted invalid source range {start}..{end}")]
    InvalidSourceRange { start: usize, end: usize },
    #[error("block index {index} is outside document with {count} blocks")]
    BlockIndex { index: usize, count: usize },
}

fn parse_front_matter(
    source: &str,
) -> Result<(String, Mapping, &str, usize, Vec<Diagnostic>), MarkdownError> {
    if !source.starts_with("---\n") {
        return Ok((String::new(), Mapping::new(), source, 0, Vec::new()));
    }
    let tail = &source[4..];
    let end = tail
        .find("\n---\n")
        .ok_or(MarkdownError::UnclosedFrontMatter)?;
    let raw = &tail[..=end];
    let value: Value = yaml::from_str(raw)
        .map_err(|error| MarkdownError::InvalidFrontMatter(error.to_string()))?;
    let mapping = value
        .as_mapping()
        .cloned()
        .ok_or_else(|| MarkdownError::InvalidFrontMatter("root must be a mapping".into()))?;
    let mut diagnostics = Vec::new();
    let mut keys = BTreeSet::new();
    let mut cursor = 4;
    for line in raw.lines() {
        let trimmed = line.trim_start();
        if trimmed.len() == line.len()
            && let Some((key, _)) = trimmed.split_once(':')
            && !key.trim().is_empty()
            && !keys.insert(key.trim())
        {
            diagnostics.push(Diagnostic {
                severity: DiagnosticSeverity::Warning,
                code: "duplicate-front-matter-key",
                message: format!(
                    "duplicate front-matter key `{}`; the last value is used",
                    key.trim()
                ),
                range: cursor..cursor + line.len(),
            });
        }
        cursor += line.len() + 1;
    }
    let offset = 4 + end + 5;
    Ok((
        raw.to_owned(),
        mapping,
        &source[offset..],
        offset,
        diagnostics,
    ))
}

fn validate_commonmark(body: &str, offset: usize) -> Result<(), MarkdownError> {
    let options = CmarkOptions::ENABLE_GFM
        | CmarkOptions::ENABLE_TABLES
        | CmarkOptions::ENABLE_TASKLISTS
        | CmarkOptions::ENABLE_FOOTNOTES
        | CmarkOptions::ENABLE_HEADING_ATTRIBUTES;
    for (event, range) in Parser::new_ext(body, options).into_offset_iter() {
        if range.end > body.len() || range.start > range.end {
            return Err(MarkdownError::InvalidSourceRange {
                start: offset + range.start,
                end: offset + range.end,
            });
        }
        let _ = matches!(event, Event::Text(_));
    }
    Ok(())
}

#[allow(clippy::too_many_lines)]
fn scan_blocks(body: &str, offset: usize, diagnostics: &mut Vec<Diagnostic>) -> Vec<Block> {
    let lines = line_ranges(body);
    let mut result = Vec::new();
    let mut line = 0;
    while line < lines.len() {
        let Range { start, end } = lines[line].clone();
        let text = &body[start..end];
        if text.trim().is_empty() {
            line += 1;
            continue;
        }
        let trimmed = text.trim_end_matches(['\r', '\n']);
        let (last_line, node) = if trimmed == PAGE_BREAK_MARKER {
            (line, BlockNode::PageBreak)
        } else if trimmed.starts_with("::: ") || trimmed == ":::" {
            scan_div(body, &lines, line, offset, diagnostics)
        } else if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            scan_fence(body, &lines, line, offset, diagnostics)
        } else if let Some((level, heading)) = heading_line(trimmed) {
            let (content, attributes) = split_trailing_attributes(heading);
            (
                line,
                BlockNode::Heading {
                    level,
                    content: parse_inlines(content),
                    attributes,
                },
            )
        } else if line + 1 < lines.len()
            && let Some(level) = setext_level(body[lines[line + 1].clone()].trim())
        {
            let (content, attributes) = split_trailing_attributes(trimmed);
            (
                line + 1,
                BlockNode::Heading {
                    level,
                    content: parse_inlines(content),
                    attributes,
                },
            )
        } else if is_thematic_break(trimmed) {
            (line, BlockNode::ThematicBreak)
        } else if is_list_line(trimmed) {
            scan_list(body, &lines, line)
        } else if trimmed.starts_with('>') {
            let last = consume_while(body, &lines, line, |value| {
                value.trim().is_empty() || value.trim_start().starts_with('>')
            });
            (
                last,
                BlockNode::BlockQuote {
                    source: slice_lines(body, &lines, line, last).to_owned(),
                },
            )
        } else if trimmed.starts_with("[^ ") || trimmed.starts_with("[^") {
            let last = consume_indented_continuation(body, &lines, line);
            (
                last,
                BlockNode::Footnote {
                    source: slice_lines(body, &lines, line, last).to_owned(),
                },
            )
        } else if looks_like_table(body, &lines, line) {
            let last = consume_while(body, &lines, line + 1, |value| {
                !value.trim().is_empty() && value.contains('|')
            });
            (
                last,
                BlockNode::Table {
                    source: slice_lines(body, &lines, line, last).to_owned(),
                },
            )
        } else if trimmed.starts_with('<') && !is_inline_extension_paragraph(trimmed) {
            let last = consume_until_blank(body, &lines, line);
            let source = slice_lines(body, &lines, line, last).to_owned();
            (
                last,
                BlockNode::Opaque {
                    reason: "unsupported HTML block".into(),
                    source,
                },
            )
        } else if text.starts_with("    ") {
            let last = consume_while(body, &lines, line, |value| {
                value.trim().is_empty() || value.starts_with("    ")
            });
            let mut code = String::new();
            for value in &lines[line..=last] {
                let line = &body[value.clone()];
                code.push_str(line.strip_prefix("    ").unwrap_or(line));
            }
            (
                last,
                BlockNode::CodeBlock {
                    info: String::new(),
                    text: code,
                },
            )
        } else {
            let last = consume_paragraph(body, &lines, line);
            let raw = slice_lines(body, &lines, line, last);
            let content = raw.trim_end_matches(['\r', '\n']);
            if has_unsupported_inline_html(content) {
                let source = raw.to_owned();
                (
                    last,
                    BlockNode::Opaque {
                        reason: "unsupported inline HTML".into(),
                        source,
                    },
                )
            } else {
                let (content, attributes) = split_trailing_attributes(content);
                (
                    last,
                    BlockNode::Paragraph {
                        content: parse_inlines(content),
                        attributes,
                    },
                )
            }
        };
        let mut trailing_line = last_line;
        while trailing_line + 1 < lines.len()
            && body[lines[trailing_line + 1].clone()].trim().is_empty()
        {
            trailing_line += 1;
        }
        let block_end = lines[trailing_line].end;
        let source = body[start..block_end].to_owned();
        result.push(Block {
            kind: block_kind(&node),
            range: offset + start..offset + block_end,
            node,
            source,
            changed: false,
        });
        line = trailing_line + 1;
    }
    result
}

fn line_ranges(source: &str) -> Vec<Range<usize>> {
    let mut result = Vec::new();
    let mut start = 0;
    for line in source.split_inclusive('\n') {
        result.push(start..start + line.len());
        start += line.len();
    }
    if start < source.len() {
        result.push(start..source.len());
    }
    result
}

fn slice_lines<'a>(source: &'a str, lines: &[Range<usize>], first: usize, last: usize) -> &'a str {
    &source[lines[first].start..lines[last].end]
}

fn scan_div(
    body: &str,
    lines: &[Range<usize>],
    first: usize,
    offset: usize,
    diagnostics: &mut Vec<Diagnostic>,
) -> (usize, BlockNode) {
    let opening = body[lines[first].clone()].trim();
    let Some(relative_close) = lines[first + 1..]
        .iter()
        .position(|range| body[range.clone()].trim() == ":::")
    else {
        let source = slice_lines(body, lines, first, lines.len() - 1).to_owned();
        diagnostics.push(Diagnostic {
            severity: DiagnosticSeverity::Error,
            code: "unclosed-fenced-div",
            message: "fenced div has no closing `:::`".into(),
            range: offset + lines[first].start..offset + lines[first].end,
        });
        return (
            lines.len() - 1,
            BlockNode::Opaque {
                reason: "malformed fenced div".into(),
                source,
            },
        );
    };
    let close = first + 1 + relative_close;
    let source = slice_lines(body, lines, first, close).to_owned();
    let attributes = parse_attributes(opening.trim_start_matches(':').trim());
    let alignment = attributes
        .extra
        .get("align")
        .and_then(|value| match value.as_str() {
            "left" => Some(Alignment::Left),
            "center" => Some(Alignment::Center),
            "right" => Some(Alignment::Right),
            "justify" => Some(Alignment::Justify),
            _ => None,
        });
    if attributes
        .classes
        .iter()
        .any(|class| class == "parchmint-opaque")
    {
        return (
            close,
            BlockNode::Opaque {
                reason: "explicit opaque extension".into(),
                source,
            },
        );
    }
    let Some(alignment) = alignment else {
        diagnostics.push(Diagnostic {
            severity: DiagnosticSeverity::Warning,
            code: "unsupported-fenced-div",
            message: "unknown or malformed fenced div is protected as opaque source".into(),
            range: offset + lines[first].start..offset + lines[close].end,
        });
        return (
            close,
            BlockNode::Opaque {
                reason: "unsupported fenced div".into(),
                source,
            },
        );
    };
    let inner_start = lines[first].end;
    let inner_end = lines[close].start;
    let children = scan_blocks(
        &body[inner_start..inner_end],
        offset + inner_start,
        diagnostics,
    );
    (
        close,
        BlockNode::Alignment {
            alignment,
            attributes,
            children,
        },
    )
}

fn scan_fence(
    body: &str,
    lines: &[Range<usize>],
    first: usize,
    offset: usize,
    diagnostics: &mut Vec<Diagnostic>,
) -> (usize, BlockNode) {
    let opening = body[lines[first].clone()].trim_end();
    let marker = if opening.starts_with("~~~") {
        "~~~"
    } else {
        "```"
    };
    let close = lines[first + 1..]
        .iter()
        .position(|range| body[range.clone()].trim_start().starts_with(marker))
        .map(|relative| first + 1 + relative);
    let last = close.unwrap_or(lines.len() - 1);
    let source = slice_lines(body, lines, first, last).to_owned();
    if close.is_none() {
        diagnostics.push(Diagnostic {
            severity: DiagnosticSeverity::Error,
            code: "unclosed-code-fence",
            message: "code fence is not closed".into(),
            range: offset + lines[first].start..offset + lines[first].end,
        });
        return (
            last,
            BlockNode::Opaque {
                reason: "malformed code fence".into(),
                source,
            },
        );
    }
    let info = opening.trim_start_matches(marker).trim().to_owned();
    if info.starts_with("{=parchmint-opaque") {
        return (
            last,
            BlockNode::Opaque {
                reason: "explicit opaque extension".into(),
                source,
            },
        );
    }
    let text = body[lines[first].end..lines[last].start].to_owned();
    (last, BlockNode::CodeBlock { info, text })
}

fn scan_list(body: &str, lines: &[Range<usize>], first: usize) -> (usize, BlockNode) {
    let mut items = Vec::new();
    let mut last = first;
    let mut ordered = false;
    let mut start_number = 1;
    for (index, range) in lines.iter().enumerate().skip(first) {
        let line = body[range.clone()].trim_end_matches(['\r', '\n']);
        let Some((prefix, content)) = split_list_marker(line) else {
            break;
        };
        if items.is_empty() {
            ordered = prefix
                .chars()
                .next()
                .is_some_and(|value| value.is_ascii_digit());
            if ordered {
                start_number = prefix.trim_end_matches(['.', ')']).parse().unwrap_or(1);
            }
        }
        let (checked, content) = if let Some(rest) = content.strip_prefix("[ ] ") {
            (Some(false), rest)
        } else if let Some(rest) = content
            .strip_prefix("[x] ")
            .or_else(|| content.strip_prefix("[X] "))
        {
            (Some(true), rest)
        } else {
            (None, content)
        };
        items.push(ListItem {
            checked,
            content: parse_inlines(content),
        });
        last = index;
    }
    (
        last,
        BlockNode::List {
            ordered,
            start: start_number,
            items,
        },
    )
}

fn split_list_marker(line: &str) -> Option<(&str, &str)> {
    let trimmed = line.trim_start();
    if let Some(rest) = trimmed
        .strip_prefix("- ")
        .or_else(|| trimmed.strip_prefix("* "))
        .or_else(|| trimmed.strip_prefix("+ "))
    {
        return Some((&trimmed[..1], rest));
    }
    let marker_end = trimmed.find(['.', ')'])?;
    if marker_end == 0
        || !trimmed[..marker_end]
            .chars()
            .all(|value| value.is_ascii_digit())
    {
        return None;
    }
    let content = trimmed.get(marker_end + 2..)?;
    Some((&trimmed[..=marker_end], content))
}

fn consume_while(
    body: &str,
    lines: &[Range<usize>],
    first: usize,
    predicate: impl Fn(&str) -> bool,
) -> usize {
    let mut last = first.min(lines.len() - 1);
    for (index, range) in lines.iter().enumerate().skip(first) {
        if !predicate(&body[range.clone()]) {
            break;
        }
        last = index;
    }
    last
}

fn consume_until_blank(body: &str, lines: &[Range<usize>], first: usize) -> usize {
    consume_while(body, lines, first, |line| !line.trim().is_empty())
}

fn consume_indented_continuation(body: &str, lines: &[Range<usize>], first: usize) -> usize {
    consume_while(body, lines, first, |line| {
        !line.trim().is_empty() && (line.starts_with(' ') || line.starts_with("[^"))
    })
}

fn consume_paragraph(body: &str, lines: &[Range<usize>], first: usize) -> usize {
    let mut last = first;
    for (index, range) in lines.iter().enumerate().skip(first + 1) {
        let line = body[range.clone()].trim_end_matches(['\r', '\n']);
        if line.trim().is_empty()
            || heading_line(line).is_some()
            || is_list_line(line)
            || line.starts_with("```")
            || line.starts_with("~~~")
            || line.starts_with(":::")
            || line.trim() == PAGE_BREAK_MARKER
            || is_thematic_break(line)
        {
            break;
        }
        last = index;
    }
    last
}

fn heading_line(line: &str) -> Option<(u8, &str)> {
    let count = line.bytes().take_while(|byte| *byte == b'#').count();
    let level = u8::try_from(count).ok()?;
    (1..=6).contains(&count).then(|| {
        line.get(count..)?
            .strip_prefix(' ')
            .map(|text| (level, text))
    })?
}

fn setext_level(line: &str) -> Option<u8> {
    if !line.is_empty() && line.chars().all(|value| value == '=') {
        Some(1)
    } else if !line.is_empty() && line.chars().all(|value| value == '-') {
        Some(2)
    } else {
        None
    }
}

fn is_list_line(line: &str) -> bool {
    split_list_marker(line).is_some()
}

fn is_thematic_break(line: &str) -> bool {
    let compact = line
        .chars()
        .filter(|value| !value.is_whitespace())
        .collect::<String>();
    compact.len() >= 3
        && compact.chars().next().is_some_and(|marker| {
            matches!(marker, '-' | '*' | '_') && compact.chars().all(|value| value == marker)
        })
}

fn looks_like_table(body: &str, lines: &[Range<usize>], line: usize) -> bool {
    if line + 1 >= lines.len() || !body[lines[line].clone()].contains('|') {
        return false;
    }
    let divider = body[lines[line + 1].clone()].trim();
    divider.contains('|')
        && divider
            .split('|')
            .filter(|part| !part.trim().is_empty())
            .all(|part| {
                part.trim()
                    .trim_matches(':')
                    .chars()
                    .all(|value| value == '-')
                    && part.trim().trim_matches(':').len() >= 3
            })
}

fn is_inline_extension_paragraph(line: &str) -> bool {
    (line.contains("<sup>") && line.contains("</sup>"))
        || (line.contains("<sub>") && line.contains("</sub>"))
}

fn has_unsupported_inline_html(source: &str) -> bool {
    let mut rest = source;
    while let Some(start) = rest.find('<') {
        rest = &rest[start..];
        if rest.starts_with("<sup>")
            || rest.starts_with("</sup>")
            || rest.starts_with("<sub>")
            || rest.starts_with("</sub>")
            || rest.starts_with("<http://")
            || rest.starts_with("<https://")
            || rest.starts_with("<mailto:")
        {
            rest = &rest[1..];
            continue;
        }
        if rest.find('>').is_some() {
            return true;
        }
        break;
    }
    false
}

fn split_trailing_attributes(text: &str) -> (&str, Attributes) {
    let trimmed = text.trim_end();
    if let Some(start) = trimmed.rfind(" {")
        && trimmed.ends_with('}')
    {
        return (&trimmed[..start], parse_attributes(&trimmed[start + 1..]));
    }
    (text, Attributes::default())
}

fn parse_attributes(source: &str) -> Attributes {
    let source = source.trim().trim_start_matches('{').trim_end_matches('}');
    let mut result = Attributes::default();
    for token in split_attribute_tokens(source) {
        if let Some(id) = token.strip_prefix('#') {
            result.id = Some(id.to_owned());
        } else if let Some(class) = token.strip_prefix('.') {
            result.classes.push(class.to_owned());
        } else if let Some((key, value)) = token.split_once('=') {
            let value = value.trim_matches('"').to_owned();
            if key == "style-id" {
                result.style_id = Some(value);
            } else {
                result.extra.insert(key.to_owned(), value);
            }
        }
    }
    result
}

fn split_attribute_tokens(source: &str) -> Vec<String> {
    let mut result = Vec::new();
    let mut current = String::new();
    let mut quoted = false;
    for character in source.chars() {
        match character {
            '"' => {
                quoted = !quoted;
                current.push(character);
            }
            value if value.is_whitespace() && !quoted => {
                if !current.is_empty() {
                    result.push(std::mem::take(&mut current));
                }
            }
            value => current.push(value),
        }
    }
    if !current.is_empty() {
        result.push(current);
    }
    result
}

fn parse_inlines(source: &str) -> Vec<Inline> {
    parse_inline_segment(source)
}

fn parse_inline_segment(mut source: &str) -> Vec<Inline> {
    let mut result = Vec::new();
    while !source.is_empty() {
        if let Some((node, consumed)) = parse_delimited(source, "**", "**", Inline::Strong)
            .or_else(|| parse_delimited(source, "__", "__", Inline::Strong))
            .or_else(|| parse_delimited(source, "~~", "~~", Inline::Strikethrough))
            .or_else(|| parse_delimited(source, "*", "*", Inline::Emphasis))
            .or_else(|| parse_delimited(source, "_", "_", Inline::Emphasis))
            .or_else(|| parse_delimited(source, "<sup>", "</sup>", Inline::Superscript))
            .or_else(|| parse_delimited(source, "<sub>", "</sub>", Inline::Subscript))
        {
            result.push(node);
            source = &source[consumed..];
            continue;
        }
        if source.starts_with('`')
            && let Some(end) = source[1..].find('`')
        {
            result.push(Inline::Code(source[1..=end].to_owned()));
            source = &source[end + 2..];
            continue;
        }
        if let Some((node, consumed)) = parse_autolink(source) {
            result.push(node);
            source = &source[consumed..];
            continue;
        }
        if let Some((node, consumed)) =
            parse_styled_span(source).or_else(|| parse_link_or_image(source))
        {
            result.push(node);
            source = &source[consumed..];
            continue;
        }
        if source.starts_with("  \n") || source.starts_with("\\\n") {
            result.push(Inline::HardBreak);
            source = source
                .strip_prefix("  \n")
                .or_else(|| source.strip_prefix("\\\n"))
                .unwrap_or(source);
            continue;
        }
        if source.starts_with('\n') {
            result.push(Inline::SoftBreak);
            source = &source[1..];
            continue;
        }
        let next = next_inline_marker(source).unwrap_or(source.len()).max(1);
        push_text(&mut result, &source[..next]);
        source = &source[next..];
    }
    result
}

fn parse_autolink(source: &str) -> Option<(Inline, usize)> {
    let tail = source.strip_prefix('<')?;
    let end = tail.find('>')?;
    let destination = &tail[..end];
    if !(destination.starts_with("http://")
        || destination.starts_with("https://")
        || destination.starts_with("mailto:"))
    {
        return None;
    }
    let label = destination.strip_prefix("mailto:").unwrap_or(destination);
    Some((
        Inline::Link {
            label: vec![Inline::Text(label.to_owned())],
            destination: destination.to_owned(),
            title: None,
        },
        end + 2,
    ))
}

fn parse_styled_span(source: &str) -> Option<(Inline, usize)> {
    let tail = source.strip_prefix('[')?;
    let label_end = tail.find(']')?;
    let attributes_source = tail[label_end + 1..].strip_prefix('{')?;
    let attributes_end = attributes_source.find('}')?;
    let attributes = parse_attributes(&attributes_source[..attributes_end]);
    if !attributes
        .classes
        .iter()
        .any(|class| class == "parchmint-style")
    {
        return None;
    }
    let consumed = 1 + label_end + 1 + 1 + attributes_end + 1;
    Some((
        Inline::Styled {
            children: parse_inline_segment(&tail[..label_end]),
            attributes,
        },
        consumed,
    ))
}

fn parse_delimited(
    source: &str,
    open: &str,
    close: &str,
    constructor: impl Fn(Vec<Inline>) -> Inline,
) -> Option<(Inline, usize)> {
    let tail = source.strip_prefix(open)?;
    let end = tail.find(close)?;
    if end == 0 {
        return None;
    }
    Some((
        constructor(parse_inline_segment(&tail[..end])),
        open.len() + end + close.len(),
    ))
}

fn parse_link_or_image(source: &str) -> Option<(Inline, usize)> {
    let (image, tail, prefix) = if let Some(tail) = source.strip_prefix("![") {
        (true, tail, 2)
    } else {
        (false, source.strip_prefix('[')?, 1)
    };
    let label_end = tail.find(']')?;
    let label = &tail[..label_end];
    let after_label = &tail[label_end + 1..];
    let destination = after_label.strip_prefix('(')?;
    let destination_end = destination.find(')')?;
    let raw_destination = &destination[..destination_end];
    let (destination, title) = if let Some((destination, title)) = raw_destination.split_once(" \"")
    {
        (destination, Some(title.trim_end_matches('"').to_owned()))
    } else {
        (raw_destination, None)
    };
    let consumed = prefix + label_end + 1 + 1 + destination_end + 1;
    let mut node = if image {
        Inline::Image {
            alt: label.to_owned(),
            destination: destination.to_owned(),
            title,
        }
    } else {
        Inline::Link {
            label: parse_inline_segment(label),
            destination: destination.to_owned(),
            title,
        }
    };
    if !image {
        let rest = &source[consumed..];
        if rest.starts_with('{')
            && let Some(end) = rest.find('}')
        {
            let attributes = parse_attributes(&rest[..=end]);
            if attributes
                .classes
                .iter()
                .any(|class| class == "parchmint-style")
            {
                let Inline::Link {
                    label: children, ..
                } = node
                else {
                    unreachable!()
                };
                node = Inline::Styled {
                    children,
                    attributes,
                };
                return Some((node, consumed + end + 1));
            }
        }
    }
    Some((node, consumed))
}

fn next_inline_marker(source: &str) -> Option<usize> {
    ['*', '_', '~', '`', '[', '!', '<', '\n', '\\']
        .iter()
        .filter_map(|marker| source.find(*marker))
        .min()
}

fn push_text(result: &mut Vec<Inline>, text: &str) {
    if let Some(Inline::Text(previous)) = result.last_mut() {
        previous.push_str(text);
    } else {
        result.push(Inline::Text(text.to_owned()));
    }
}

fn validate_extensions(
    blocks: &mut [Block],
    options: &ParseOptions,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let mut ids = BTreeMap::<String, Range<usize>>::new();
    for block in blocks {
        let attributes = match &block.node {
            BlockNode::Paragraph { attributes, .. }
            | BlockNode::Heading { attributes, .. }
            | BlockNode::Alignment { attributes, .. } => Some(attributes),
            _ => None,
        };
        if let Some(attributes) = attributes {
            if let Some(id) = &attributes.id
                && let Some(previous) = ids.insert(id.clone(), block.range.clone())
            {
                diagnostics.push(Diagnostic {
                    severity: DiagnosticSeverity::Warning,
                    code: "duplicate-id",
                    message: format!(
                        "duplicate anchor `{id}`; first defined at byte {}",
                        previous.start
                    ),
                    range: block.range.clone(),
                });
            }
            check_style(attributes, &block.range, options, diagnostics);
        }
        validate_inline_styles(&block.node, &block.range, options, diagnostics);
        if block.node.is_opaque() {
            let message = match &block.node {
                BlockNode::Opaque { reason, .. } => reason.clone(),
                _ => unreachable!(),
            };
            diagnostics.push(Diagnostic {
                severity: DiagnosticSeverity::Warning,
                code: "opaque-block",
                message,
                range: block.range.clone(),
            });
        }
    }
}

fn validate_inline_styles(
    node: &BlockNode,
    range: &Range<usize>,
    options: &ParseOptions,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let inlines = match node {
        BlockNode::Paragraph { content, .. } | BlockNode::Heading { content, .. } => content,
        BlockNode::List { items, .. } => {
            for item in items {
                walk_inline_styles(&item.content, range, options, diagnostics);
            }
            return;
        }
        _ => return,
    };
    walk_inline_styles(inlines, range, options, diagnostics);
}

fn walk_inline_styles(
    values: &[Inline],
    range: &Range<usize>,
    options: &ParseOptions,
    diagnostics: &mut Vec<Diagnostic>,
) {
    for value in values {
        match value {
            Inline::Styled {
                children,
                attributes,
            } => {
                check_style(attributes, range, options, diagnostics);
                walk_inline_styles(children, range, options, diagnostics);
            }
            Inline::Emphasis(children)
            | Inline::Strong(children)
            | Inline::Strikethrough(children)
            | Inline::Superscript(children)
            | Inline::Subscript(children) => {
                walk_inline_styles(children, range, options, diagnostics);
            }
            Inline::Link { label, .. } => walk_inline_styles(label, range, options, diagnostics),
            _ => {}
        }
    }
}

fn check_style(
    attributes: &Attributes,
    range: &Range<usize>,
    options: &ParseOptions,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if attributes
        .classes
        .iter()
        .any(|class| class == "parchmint-style")
    {
        match &attributes.style_id {
            None => diagnostics.push(Diagnostic {
                severity: DiagnosticSeverity::Warning,
                code: "missing-style-id",
                message: "ParchMint style attribute has no stable style-id".into(),
                range: range.clone(),
            }),
            Some(id)
                if !options.known_style_ids.is_empty() && !options.known_style_ids.contains(id) =>
            {
                diagnostics.push(Diagnostic {
                    severity: DiagnosticSeverity::Warning,
                    code: "unknown-style-id",
                    message: format!("style `{id}` is missing; direct appearance is retained"),
                    range: range.clone(),
                });
            }
            _ => {}
        }
    }
}

fn block_kind(node: &BlockNode) -> BlockKind {
    match node {
        BlockNode::PageBreak => BlockKind::PageBreak,
        BlockNode::Opaque { .. } => BlockKind::Opaque,
        _ => BlockKind::Supported,
    }
}

fn replace_style_in_node(node: &mut BlockNode, old: &str, replacement: &str) -> bool {
    let mut changed = false;
    match node {
        BlockNode::Paragraph {
            content,
            attributes,
        }
        | BlockNode::Heading {
            content,
            attributes,
            ..
        } => {
            changed |= replace_style_attribute(attributes, old, replacement);
            changed |= replace_style_inlines(content, old, replacement);
        }
        BlockNode::List { items, .. } => {
            for item in items {
                changed |= replace_style_inlines(&mut item.content, old, replacement);
            }
        }
        BlockNode::Alignment {
            attributes,
            children,
            ..
        } => {
            changed |= replace_style_attribute(attributes, old, replacement);
            for child in children {
                if replace_style_in_node(&mut child.node, old, replacement) {
                    child.changed = true;
                    changed = true;
                }
            }
        }
        _ => {}
    }
    changed
}

fn replace_style_inlines(values: &mut [Inline], old: &str, replacement: &str) -> bool {
    let mut changed = false;
    for value in values {
        match value {
            Inline::Styled {
                children,
                attributes,
            } => {
                changed |= replace_style_attribute(attributes, old, replacement);
                changed |= replace_style_inlines(children, old, replacement);
            }
            Inline::Emphasis(children)
            | Inline::Strong(children)
            | Inline::Strikethrough(children)
            | Inline::Superscript(children)
            | Inline::Subscript(children) => {
                changed |= replace_style_inlines(children, old, replacement);
            }
            Inline::Link { label, .. } => {
                changed |= replace_style_inlines(label, old, replacement);
            }
            _ => {}
        }
    }
    changed
}

fn replace_style_attribute(attributes: &mut Attributes, old: &str, replacement: &str) -> bool {
    if attributes.style_id.as_deref() == Some(old) {
        attributes.style_id = Some(replacement.to_owned());
        true
    } else {
        false
    }
}

fn serialize_node(node: &BlockNode) -> String {
    match node {
        BlockNode::Paragraph {
            content,
            attributes,
        } => {
            format!(
                "{}{}\n\n",
                serialize_inlines(content),
                serialize_attributes(attributes, true)
            )
        }
        BlockNode::Heading {
            level,
            content,
            attributes,
        } => format!(
            "{} {}{}\n\n",
            "#".repeat(usize::from((*level).clamp(1, 6))),
            serialize_inlines(content),
            serialize_attributes(attributes, true)
        ),
        BlockNode::BlockQuote { source }
        | BlockNode::Table { source }
        | BlockNode::Footnote { source }
        | BlockNode::Opaque { source, .. } => ensure_block_spacing(source),
        BlockNode::CodeBlock { info, text } => {
            format!(
                "```{info}\n{}{}\n\n",
                text,
                if text.ends_with('\n') { "" } else { "\n" }
            )
        }
        BlockNode::List {
            ordered,
            start,
            items,
        } => {
            let mut output = String::new();
            for (index, item) in items.iter().enumerate() {
                let marker = if *ordered {
                    format!("{}.", start + u64::try_from(index).unwrap_or(0))
                } else {
                    "-".into()
                };
                let task = match item.checked {
                    Some(true) => "[x] ",
                    Some(false) => "[ ] ",
                    None => "",
                };
                let _ = writeln!(
                    output,
                    "{marker} {task}{}",
                    serialize_inlines(&item.content)
                );
            }
            output.push('\n');
            output
        }
        BlockNode::ThematicBreak => "---\n\n".into(),
        BlockNode::Alignment {
            alignment,
            attributes,
            children,
        } => {
            let align = match alignment {
                Alignment::Left => "left",
                Alignment::Center => "center",
                Alignment::Right => "right",
                Alignment::Justify => "justify",
            };
            let mut attributes = attributes.clone();
            if !attributes
                .classes
                .iter()
                .any(|value| value == "parchmint-align")
            {
                attributes.classes.push("parchmint-align".into());
            }
            attributes.extra.insert("align".into(), align.into());
            let mut output = format!(":::{}\n", serialize_attributes(&attributes, false));
            output.push_str(&children.iter().map(Block::serialize).collect::<String>());
            output.push_str(":::\n\n");
            output
        }
        BlockNode::PageBreak => format!("{PAGE_BREAK_MARKER}\n\n"),
    }
}

fn serialize_inlines(inlines: &[Inline]) -> String {
    let mut output = String::new();
    for inline in inlines {
        match inline {
            Inline::Text(text) => output.push_str(&escape_text(text)),
            Inline::Emphasis(children) => {
                let _ = write!(output, "*{}*", serialize_inlines(children));
            }
            Inline::Strong(children) => {
                let _ = write!(output, "**{}**", serialize_inlines(children));
            }
            Inline::Strikethrough(children) => {
                let _ = write!(output, "~~{}~~", serialize_inlines(children));
            }
            Inline::Code(text) => {
                let fence = if text.contains('`') { "``" } else { "`" };
                let _ = write!(output, "{fence}{text}{fence}");
            }
            Inline::Link {
                label,
                destination,
                title,
            } => {
                let _ = write!(
                    output,
                    "[{}]({}",
                    serialize_inlines(label),
                    escape_destination(destination)
                );
                if let Some(title) = title {
                    let _ = write!(output, " \"{}\"", title.replace('"', "\\\""));
                }
                output.push(')');
            }
            Inline::Image {
                alt,
                destination,
                title,
            } => {
                let _ = write!(
                    output,
                    "![{}]({}",
                    alt.replace(']', "\\]"),
                    escape_destination(destination)
                );
                if let Some(title) = title {
                    let _ = write!(output, " \"{}\"", title.replace('"', "\\\""));
                }
                output.push(')');
            }
            Inline::Superscript(children) => {
                let _ = write!(output, "<sup>{}</sup>", serialize_inlines(children));
            }
            Inline::Subscript(children) => {
                let _ = write!(output, "<sub>{}</sub>", serialize_inlines(children));
            }
            Inline::Styled {
                children,
                attributes,
            } => {
                let _ = write!(
                    output,
                    "[{}]{}",
                    serialize_inlines(children),
                    serialize_attributes(attributes, false)
                );
            }
            Inline::SoftBreak => output.push('\n'),
            Inline::HardBreak => output.push_str("\\\n"),
        }
    }
    output
}

fn serialize_attributes(attributes: &Attributes, leading_space: bool) -> String {
    if attributes == &Attributes::default() {
        return String::new();
    }
    let mut parts = Vec::new();
    if let Some(id) = &attributes.id {
        parts.push(format!("#{id}"));
    }
    parts.extend(attributes.classes.iter().map(|class| format!(".{class}")));
    if let Some(style) = &attributes.style_id {
        parts.push(format!("style-id=\"{}\"", escape_attribute(style)));
    }
    parts.extend(
        attributes
            .extra
            .iter()
            .map(|(key, value)| format!("{key}=\"{}\"", escape_attribute(value))),
    );
    format!(
        "{}{{{}}}",
        if leading_space { " " } else { "" },
        parts.join(" ")
    )
}

fn escape_text(text: &str) -> String {
    let mut output = String::with_capacity(text.len());
    for character in text.chars() {
        if matches!(character, '\\' | '*' | '_' | '`' | '[' | ']') {
            output.push('\\');
        }
        output.push(character);
    }
    output
}

fn escape_destination(value: &str) -> String {
    value.replace(' ', "%20").replace(')', "%29")
}

fn escape_attribute(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

fn ensure_block_spacing(source: &str) -> String {
    format!("{}\n\n", source.trim_end_matches(['\r', '\n']))
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = include_str!("../../../tests/fixtures/spike/representative.md");
    const SUPPORTED_FIXTURE: &str = include_str!("../../../tests/fixtures/markdown/supported.md");
    const FIELDS_FIXTURE: &str =
        include_str!("../../../tests/fixtures/markdown/front-matter-fields.md");
    const COMBINATIONS_FIXTURE: &str =
        include_str!("../../../tests/fixtures/markdown/pairwise-combinations.md");
    const MALFORMED_FIXTURE: &str =
        include_str!("../../../tests/fixtures/markdown/malformed-extensions.md");

    #[test]
    fn representative_fixture_round_trips_byte_for_byte() {
        let parsed = Document::parse(FIXTURE).unwrap();
        assert_eq!(parsed.serialize(), FIXTURE);
        assert!(parsed.front_matter().contains_key("future-plugin"));
        assert!(
            parsed
                .blocks()
                .iter()
                .any(|block| block.kind == BlockKind::Opaque)
        );
        assert!(
            parsed
                .blocks()
                .iter()
                .any(|block| block.kind == BlockKind::PageBreak)
        );
        assert!(
            parsed
                .blocks()
                .iter()
                .any(|block| matches!(block.node, BlockNode::Alignment { .. }))
        );
    }

    #[test]
    fn repeated_parse_serialize_is_stable() {
        let mut source = FIXTURE.to_owned();
        for _ in 0..20 {
            source = Document::parse(&source).unwrap().serialize();
        }
        assert_eq!(source, FIXTURE);
    }

    #[test]
    fn complete_supported_matrix_golden_round_trips() {
        let document = Document::parse(SUPPORTED_FIXTURE).unwrap();
        assert_eq!(document.serialize(), SUPPORTED_FIXTURE);
        assert!(
            document
                .blocks()
                .iter()
                .any(|block| matches!(block.node, BlockNode::Heading { .. }))
        );
        assert!(
            document
                .blocks()
                .iter()
                .any(|block| matches!(block.node, BlockNode::BlockQuote { .. }))
        );
        assert!(
            document
                .blocks()
                .iter()
                .any(|block| matches!(block.node, BlockNode::List { ordered: false, .. }))
        );
        assert!(document.blocks().iter().any(|block| matches!(
            block.node,
            BlockNode::List {
                ordered: true,
                start: 3,
                ..
            }
        )));
        assert!(
            document
                .blocks()
                .iter()
                .any(|block| matches!(block.node, BlockNode::Table { .. }))
        );
        assert!(
            document
                .blocks()
                .iter()
                .any(|block| matches!(block.node, BlockNode::CodeBlock { .. }))
        );
        assert!(
            document
                .blocks()
                .iter()
                .any(|block| matches!(block.node, BlockNode::Footnote { .. }))
        );
        assert!(
            document
                .blocks()
                .iter()
                .any(|block| matches!(block.node, BlockNode::Alignment { .. }))
        );
        assert!(
            document
                .blocks()
                .iter()
                .any(|block| matches!(block.node, BlockNode::ThematicBreak))
        );
        assert!(
            document
                .blocks()
                .iter()
                .any(|block| matches!(block.node, BlockNode::PageBreak))
        );
    }

    #[test]
    fn changed_nodes_use_deterministic_extension_spelling() {
        let mut document = Document::parse_body("old\n", &ParseOptions::default()).unwrap();
        document
            .replace_block(
                0,
                BlockNode::Paragraph {
                    content: vec![Inline::Styled {
                        children: vec![Inline::Strong(vec![Inline::Text("new".into())])],
                        attributes: Attributes {
                            classes: vec!["parchmint-style".into()],
                            style_id: Some("style-1".into()),
                            extra: BTreeMap::from([("z".into(), "last".into())]),
                            ..Attributes::default()
                        },
                    }],
                    attributes: Attributes::default(),
                },
            )
            .unwrap();
        assert_eq!(
            document.serialize_body(),
            "[**new**]{.parchmint-style style-id=\"style-1\" z=\"last\"}\n\n"
        );
    }

    #[test]
    fn malformed_and_unknown_extensions_are_opaque_with_diagnostics() {
        let source = "::: {.future}\nkeep\n:::\n\n<div>unsafe</div>\n";
        let document = Document::parse_body(source, &ParseOptions::default()).unwrap();
        assert_eq!(document.blocks().len(), 2);
        assert!(document.blocks().iter().all(|block| block.node.is_opaque()));
        assert!(
            document
                .diagnostics()
                .iter()
                .all(|item| item.code == "opaque-block" || item.code == "unsupported-fenced-div")
        );
        assert_eq!(document.serialize_body(), source);
    }

    #[test]
    fn reports_duplicate_ids_and_missing_styles() {
        let source =
            "A. {#same .parchmint-style}\n\nB. {#same .parchmint-style style-id=\"gone\"}\n";
        let options = ParseOptions {
            known_style_ids: BTreeSet::from(["present".into()]),
        };
        let document = Document::parse_body(source, &options).unwrap();
        let codes = document
            .diagnostics()
            .iter()
            .map(|item| item.code)
            .collect::<BTreeSet<_>>();
        assert!(codes.contains("duplicate-id"));
        assert!(codes.contains("missing-style-id"));
        assert!(codes.contains("unknown-style-id"));
    }

    #[test]
    fn styled_inline_is_semantic_and_explicit_replacement_rewrites_only_its_block() {
        let source = "[styled **text**]{.parchmint-style style-id=\"old\"}\n\nUntouched.\n";
        let mut document = Document::parse_body(source, &ParseOptions::default()).unwrap();
        let BlockNode::Paragraph { content, .. } = &document.blocks()[0].node else {
            panic!("expected paragraph")
        };
        assert!(matches!(content[0], Inline::Styled { .. }));
        assert_eq!(document.replace_style_references("old", "new"), 1);
        assert_eq!(
            document.serialize_body(),
            "[styled **text**]{.parchmint-style style-id=\"new\"}\n\nUntouched.\n"
        );
    }

    #[test]
    fn unsupported_inline_html_protects_the_whole_source_block() {
        let source = "Keep <kbd>Ctrl</kbd> exactly.\n";
        let document = Document::parse_body(source, &ParseOptions::default()).unwrap();
        assert!(document.blocks()[0].node.is_opaque());
        assert_eq!(document.serialize_body(), source);
    }

    #[test]
    fn parser_exposes_absolute_source_ranges() {
        let parsed = Document::parse(FIXTURE).unwrap();
        for block in parsed.blocks() {
            assert_eq!(&FIXTURE[block.range.clone()], block.source());
        }
    }

    #[test]
    fn focused_fixture_catalog_covers_metadata_and_pairwise_constructs() {
        let fields = Document::parse(FIELDS_FIXTURE).unwrap();
        assert_eq!(
            fields.front_matter()["status"],
            Value::String("draft".into())
        );
        assert!(fields.front_matter().contains_key("future-plugin"));

        let combinations = Document::parse(COMBINATIONS_FIXTURE).unwrap();
        assert!(
            combinations
                .blocks()
                .iter()
                .any(|block| block.node.is_opaque())
        );
        assert!(
            combinations
                .blocks()
                .iter()
                .any(|block| matches!(block.node, BlockNode::PageBreak))
        );
        assert!(
            combinations
                .blocks()
                .iter()
                .any(|block| matches!(block.node, BlockNode::Alignment { .. }))
        );
        assert_eq!(combinations.serialize(), COMBINATIONS_FIXTURE);
    }

    #[test]
    fn malformed_fixture_preserves_opaque_source_and_hard_front_matter_errors() {
        let malformed = Document::parse(MALFORMED_FIXTURE).unwrap();
        assert!(
            malformed
                .blocks()
                .iter()
                .all(|block| block.node.is_opaque())
        );
        assert_eq!(malformed.serialize(), MALFORMED_FIXTURE);
        let unclosed = include_str!("../../../tests/fixtures/markdown/unclosed-front-matter.md");
        assert!(matches!(
            Document::parse(unclosed),
            Err(MarkdownError::UnclosedFrontMatter)
        ));
    }

    #[test]
    fn newline_and_unicode_fixture_is_stable_across_repeated_saves() {
        let source = include_str!("../../../tests/fixtures/markdown/newline-stability.md");
        let mut saved = Document::parse(source).unwrap().serialize();
        for _ in 0..5 {
            saved = Document::parse(&saved).unwrap().serialize();
        }
        assert_eq!(saved, source);
        assert!(saved.contains("café — 雪"));
    }
}
