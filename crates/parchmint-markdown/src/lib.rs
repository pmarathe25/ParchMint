#![allow(missing_docs)]
//! Source-aware ParchMint Markdown 1 semantic codec.
//!
//! The codec deliberately keeps the original source for every unmodified block.
//! Once a block is edited its semantic representation is emitted with the stable
//! ParchMint spelling. Unsupported constructs remain protected opaque nodes.

use noyalib::compat::serde_yaml::{self as yaml, Mapping, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::ops::Range;
use thiserror::Error;

pub const PAGE_BREAK_MARKER: &str = "<!-- parchmint:page-break -->";

/// Resource bounds for parsing untrusted canonical Markdown.  These values are
/// deliberately expressed in bytes/items rather than wall-clock time so the
/// same document has the same outcome on every supported platform.
#[derive(Clone, Debug)]
pub struct ParseOptions {
    /// Style IDs known to the currently open project. Empty means unchecked.
    pub known_style_ids: BTreeSet<String>,
    /// Largest accepted source document, including front matter.
    pub max_document_bytes: usize,
    /// Largest number of semantic blocks, including blocks in fenced divs.
    pub max_blocks: usize,
    /// Maximum nested inline or fenced-div depth.
    pub max_inline_depth: usize,
    /// Maximum delimiter/escape inspections performed by the inline scanner.
    pub max_delimiter_scans: usize,
    /// Maximum diagnostics retained for one document.
    pub max_diagnostics: usize,
}

impl Default for ParseOptions {
    fn default() -> Self {
        Self {
            known_style_ids: BTreeSet::new(),
            max_document_bytes: 16 * 1024 * 1024,
            max_blocks: 100_000,
            max_inline_depth: 64,
            max_delimiter_scans: 1_000_000,
            max_diagnostics: 1_024,
        }
    }
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
    /// Nested list boundaries and continuation paragraphs.  A list item is a
    /// container, not merely a line: retaining this distinction keeps a
    /// changed parent list from flattening its children on save.
    pub children: Vec<BlockNode>,
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
    has_front_matter: bool,
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
        let mut budget = ParseBudget::new(options, source.len())?;
        let ParsedFrontMatter {
            has_front_matter,
            raw_front_matter,
            front_matter,
            body,
            body_offset,
            mut diagnostics,
        } = parse_front_matter(source, &budget)?;
        let mut blocks = scan_blocks(body, body_offset, &mut diagnostics, &mut budget, 0)?;
        validate_extensions(&mut blocks, options, &mut diagnostics, &mut budget)?;
        Ok(Self {
            has_front_matter,
            raw_front_matter,
            front_matter,
            blocks,
            diagnostics,
        })
    }

    /// Parses a storage-owned body which has no YAML front matter.
    pub fn parse_body(body: &str, options: &ParseOptions) -> Result<Self, MarkdownError> {
        let mut budget = ParseBudget::new(options, body.len())?;
        let mut diagnostics = Vec::new();
        let mut blocks = scan_blocks(body, 0, &mut diagnostics, &mut budget, 0)?;
        validate_extensions(&mut blocks, options, &mut diagnostics, &mut budget)?;
        Ok(Self {
            has_front_matter: false,
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
        if self.has_front_matter {
            format!("---\n{}---\n{body}", self.raw_front_matter)
        } else {
            body
        }
    }
}

#[derive(Debug, Error)]
pub enum MarkdownError {
    #[error("front matter opened at byte 0 but has no closing delimiter")]
    UnclosedFrontMatter,
    #[error("invalid YAML front matter: {0}")]
    InvalidFrontMatter(String),
    #[error("Markdown exceeds the configured {kind} limit ({limit})")]
    ResourceLimit { kind: &'static str, limit: usize },
    #[error("block index {index} is outside document with {count} blocks")]
    BlockIndex { index: usize, count: usize },
}

/// Mutable accounting is intentionally shared by all recursive scanners.  The
/// configured depth makes recursion bounded; counters ensure long unmatched
/// delimiter runs cannot turn parsing into quadratic work.
struct ParseBudget<'a> {
    options: &'a ParseOptions,
    blocks: usize,
    delimiter_scans: usize,
}

impl<'a> ParseBudget<'a> {
    fn new(options: &'a ParseOptions, bytes: usize) -> Result<Self, MarkdownError> {
        if bytes > options.max_document_bytes {
            return Err(MarkdownError::ResourceLimit {
                kind: "document-byte",
                limit: options.max_document_bytes,
            });
        }
        Ok(Self {
            options,
            blocks: 0,
            delimiter_scans: 0,
        })
    }

    fn block(&mut self) -> Result<(), MarkdownError> {
        self.blocks += 1;
        if self.blocks > self.options.max_blocks {
            return Err(MarkdownError::ResourceLimit {
                kind: "block-count",
                limit: self.options.max_blocks,
            });
        }
        Ok(())
    }

    fn depth(&self, depth: usize) -> Result<(), MarkdownError> {
        if depth > self.options.max_inline_depth {
            return Err(MarkdownError::ResourceLimit {
                kind: "inline-depth",
                limit: self.options.max_inline_depth,
            });
        }
        Ok(())
    }

    fn delimiter_scan(&mut self) -> Result<(), MarkdownError> {
        self.delimiter_scans += 1;
        if self.delimiter_scans > self.options.max_delimiter_scans {
            return Err(MarkdownError::ResourceLimit {
                kind: "delimiter-scan",
                limit: self.options.max_delimiter_scans,
            });
        }
        Ok(())
    }

    fn diagnostic(&self, count: usize) -> Result<(), MarkdownError> {
        if count >= self.options.max_diagnostics {
            return Err(MarkdownError::ResourceLimit {
                kind: "diagnostic-count",
                limit: self.options.max_diagnostics,
            });
        }
        Ok(())
    }
}

struct ParsedFrontMatter<'a> {
    has_front_matter: bool,
    raw_front_matter: String,
    front_matter: Mapping,
    body: &'a str,
    body_offset: usize,
    diagnostics: Vec<Diagnostic>,
}

fn parse_front_matter<'a>(
    source: &'a str,
    budget: &ParseBudget<'_>,
) -> Result<ParsedFrontMatter<'a>, MarkdownError> {
    let opening_len = if source.starts_with("---\r\n") {
        5
    } else if source.starts_with("---\n") {
        4
    } else {
        return Ok(ParsedFrontMatter {
            has_front_matter: false,
            raw_front_matter: String::new(),
            front_matter: Mapping::new(),
            body: source,
            body_offset: 0,
            diagnostics: Vec::new(),
        });
    };
    let tail = &source[opening_len..];
    let mut line_start = 0;
    let mut closing = None;
    for line in tail.split_inclusive('\n') {
        let line_end = line_start + line.len();
        if line.trim_end_matches(['\r', '\n']) == "---" {
            closing = Some((line_start, line_end));
            break;
        }
        line_start = line_end;
    }
    let (closing_start, closing_end) = closing.ok_or(MarkdownError::UnclosedFrontMatter)?;
    let raw = &tail[..closing_start];
    let mapping = if raw.trim().is_empty() {
        Mapping::new()
    } else {
        let value: Value = yaml::from_str(raw)
            .map_err(|error| MarkdownError::InvalidFrontMatter(error.to_string()))?;
        value
            .as_mapping()
            .cloned()
            .ok_or_else(|| MarkdownError::InvalidFrontMatter("root must be a mapping".into()))?
    };
    let mut diagnostics = Vec::new();
    let mut keys = BTreeSet::new();
    let mut cursor = opening_len;
    for line in raw.lines() {
        let trimmed = line.trim_start();
        if trimmed.len() == line.len()
            && let Some((key, _)) = trimmed.split_once(':')
            && !key.trim().is_empty()
            && !keys.insert(key.trim())
        {
            budget.diagnostic(diagnostics.len())?;
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
    let offset = opening_len + closing_end;
    Ok(ParsedFrontMatter {
        has_front_matter: true,
        raw_front_matter: raw.to_owned(),
        front_matter: mapping,
        body: &source[offset..],
        body_offset: offset,
        diagnostics,
    })
}

#[allow(clippy::too_many_lines)]
fn scan_blocks(
    body: &str,
    offset: usize,
    diagnostics: &mut Vec<Diagnostic>,
    budget: &mut ParseBudget<'_>,
    depth: usize,
) -> Result<Vec<Block>, MarkdownError> {
    budget.depth(depth)?;
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
            scan_div(body, &lines, line, offset, diagnostics, budget, depth + 1)?
        } else if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            scan_fence(body, &lines, line, offset, diagnostics, budget)?
        } else if let Some((level, heading)) = heading_line(trimmed) {
            let (content, attributes) = split_trailing_attributes(heading);
            (
                line,
                BlockNode::Heading {
                    level,
                    content: parse_inlines(content, budget)?,
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
                    content: parse_inlines(content, budget)?,
                    attributes,
                },
            )
        } else if is_thematic_break(trimmed) {
            (line, BlockNode::ThematicBreak)
        } else if is_list_line(trimmed) {
            scan_list(body, &lines, line, budget, depth + 1)?
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
            if has_unsupported_inline_html(content) || has_reference_link(content, budget)? {
                let source = raw.to_owned();
                (
                    last,
                    BlockNode::Opaque {
                        reason: if has_unsupported_inline_html(content) {
                            "unsupported inline HTML".into()
                        } else {
                            "reference links are preserved as opaque source".into()
                        },
                        source,
                    },
                )
            } else {
                let (content, attributes) = split_trailing_attributes(content);
                (
                    last,
                    BlockNode::Paragraph {
                        content: parse_inlines(content, budget)?,
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
        budget.block()?;
        result.push(Block {
            kind: block_kind(&node),
            range: offset + start..offset + block_end,
            node,
            source,
            changed: false,
        });
        line = trailing_line + 1;
    }
    Ok(result)
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
    budget: &mut ParseBudget<'_>,
    depth: usize,
) -> Result<(usize, BlockNode), MarkdownError> {
    let opening = body[lines[first].clone()].trim();
    let Some(relative_close) = lines[first + 1..]
        .iter()
        .position(|range| body[range.clone()].trim() == ":::")
    else {
        let source = slice_lines(body, lines, first, lines.len() - 1).to_owned();
        budget.diagnostic(diagnostics.len())?;
        diagnostics.push(Diagnostic {
            severity: DiagnosticSeverity::Error,
            code: "unclosed-fenced-div",
            message: "fenced div has no closing `:::`".into(),
            range: offset + lines[first].start..offset + lines[first].end,
        });
        return Ok((
            lines.len() - 1,
            BlockNode::Opaque {
                reason: "malformed fenced div".into(),
                source,
            },
        ));
    };
    let close = first + 1 + relative_close;
    let source = slice_lines(body, lines, first, close).to_owned();
    let attributes = parse_attributes(opening.trim_start_matches(':').trim()).unwrap_or_default();
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
        return Ok((
            close,
            BlockNode::Opaque {
                reason: "explicit opaque extension".into(),
                source,
            },
        ));
    }
    let Some(alignment) = alignment else {
        budget.diagnostic(diagnostics.len())?;
        diagnostics.push(Diagnostic {
            severity: DiagnosticSeverity::Warning,
            code: "unsupported-fenced-div",
            message: "unknown or malformed fenced div is protected as opaque source".into(),
            range: offset + lines[first].start..offset + lines[close].end,
        });
        return Ok((
            close,
            BlockNode::Opaque {
                reason: "unsupported fenced div".into(),
                source,
            },
        ));
    };
    let inner_start = lines[first].end;
    let inner_end = lines[close].start;
    let children = scan_blocks(
        &body[inner_start..inner_end],
        offset + inner_start,
        diagnostics,
        budget,
        depth,
    )?;
    Ok((
        close,
        BlockNode::Alignment {
            alignment,
            attributes,
            children,
        },
    ))
}

fn scan_fence(
    body: &str,
    lines: &[Range<usize>],
    first: usize,
    offset: usize,
    diagnostics: &mut Vec<Diagnostic>,
    budget: &ParseBudget<'_>,
) -> Result<(usize, BlockNode), MarkdownError> {
    let opening = body[lines[first].clone()].trim_end();
    let marker_char = opening.as_bytes().first().copied().unwrap_or(b'`');
    let marker_len = opening
        .bytes()
        .take_while(|byte| *byte == marker_char)
        .count();
    let close = lines[first + 1..]
        .iter()
        .position(|range| {
            let candidate = body[range.clone()].trim_start();
            candidate
                .bytes()
                .take_while(|byte| *byte == marker_char)
                .count()
                >= marker_len
                && candidate[marker_len..].trim().is_empty()
        })
        .map(|relative| first + 1 + relative);
    let last = close.unwrap_or(lines.len() - 1);
    let source = slice_lines(body, lines, first, last).to_owned();
    if close.is_none() {
        budget.diagnostic(diagnostics.len())?;
        diagnostics.push(Diagnostic {
            severity: DiagnosticSeverity::Error,
            code: "unclosed-code-fence",
            message: "code fence is not closed".into(),
            range: offset + lines[first].start..offset + lines[first].end,
        });
        return Ok((
            last,
            BlockNode::Opaque {
                reason: "malformed code fence".into(),
                source,
            },
        ));
    }
    let info = opening[marker_len..].trim().to_owned();
    if info.starts_with("{=parchmint-opaque") {
        return Ok((
            last,
            BlockNode::Opaque {
                reason: "explicit opaque extension".into(),
                source,
            },
        ));
    }
    let text = body[lines[first].end..lines[last].start].to_owned();
    Ok((last, BlockNode::CodeBlock { info, text }))
}

fn scan_list(
    body: &str,
    lines: &[Range<usize>],
    first: usize,
    budget: &mut ParseBudget<'_>,
    depth: usize,
) -> Result<(usize, BlockNode), MarkdownError> {
    let mut items = Vec::new();
    let first_line = body[lines[first].clone()].trim_end_matches(['\r', '\n']);
    let base_indent = indentation(first_line);
    let Some((first_prefix, _)) = split_list_marker(first_line) else {
        unreachable!()
    };
    let ordered = first_prefix
        .as_bytes()
        .first()
        .is_some_and(u8::is_ascii_digit);
    let start_number = if ordered {
        first_prefix
            .trim_end_matches(['.', ')'])
            .parse()
            .unwrap_or(1)
    } else {
        1
    };
    let mut index = first;
    let mut last = first;
    while index < lines.len() {
        let line = body[lines[index].clone()].trim_end_matches(['\r', '\n']);
        if indentation(line) != base_indent {
            break;
        }
        let Some((prefix, content)) = split_list_marker(line) else {
            break;
        };
        let is_ordered = prefix.as_bytes().first().is_some_and(u8::is_ascii_digit);
        if is_ordered != ordered {
            break;
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
        index += 1;
        let nested_start = index;
        while index < lines.len() {
            let candidate = body[lines[index].clone()].trim_end_matches(['\r', '\n']);
            if candidate.trim().is_empty() {
                let next = lines
                    .get(index + 1)
                    .map(|range| body[range.clone()].trim_end_matches(['\r', '\n']));
                if next.is_some_and(|next| indentation(next) > base_indent) {
                    index += 1;
                    continue;
                }
                break;
            }
            if indentation(candidate) <= base_indent {
                break;
            }
            index += 1;
        }
        let children = if nested_start == index {
            Vec::new()
        } else {
            let fragment =
                deindent_list_fragment(body, lines, nested_start, index, base_indent + 2);
            scan_blocks(&fragment, 0, &mut Vec::new(), budget, depth + 1)?
                .into_iter()
                .map(|block| block.node)
                .collect()
        };
        items.push(ListItem {
            checked,
            content: parse_inlines(content, budget)?,
            children,
        });
        last = index.saturating_sub(1).max(first);
    }
    Ok((
        last,
        BlockNode::List {
            ordered,
            start: start_number,
            items,
        },
    ))
}

fn indentation(line: &str) -> usize {
    line.len() - line.trim_start_matches([' ', '\t']).len()
}

fn deindent_list_fragment(
    body: &str,
    lines: &[Range<usize>],
    first: usize,
    end: usize,
    amount: usize,
) -> String {
    let mut result = String::new();
    for range in &lines[first..end] {
        let line = &body[range.clone()];
        let mut index = 0;
        for (removed, character) in line.chars().enumerate() {
            if removed == amount || !matches!(character, ' ' | '\t') {
                break;
            }
            index += character.len_utf8();
        }
        result.push_str(&line[index..]);
    }
    result
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
        let looks_like_tag = rest[1..].chars().next().is_some_and(|character| {
            character.is_ascii_alphabetic() || matches!(character, '/' | '!' | '?')
        });
        if looks_like_tag && rest.find('>').is_some() {
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
        && let Some(attributes) = parse_attributes(&trimmed[start + 1..])
        && attributes != Attributes::default()
    {
        return (&trimmed[..start], attributes);
    }
    (text, Attributes::default())
}

/// Parses only the documented Pandoc-compatible subset.  In particular,
/// malformed quotes are rejected as a whole: consuming a plausible prefix
/// would otherwise silently delete user text from a changed paragraph.
fn parse_attributes(source: &str) -> Option<Attributes> {
    let source = source.trim();
    let source = source.strip_prefix('{')?.strip_suffix('}')?;
    let mut result = Attributes::default();
    for token in split_attribute_tokens(source)? {
        if let Some(id) = token.strip_prefix('#') {
            if !valid_attribute_name(id) || result.id.is_some() {
                return None;
            }
            result.id = Some(id.to_owned());
        } else if let Some(class) = token.strip_prefix('.') {
            if !valid_attribute_name(class) {
                return None;
            }
            result.classes.push(class.to_owned());
        } else if let Some((key, value)) = token.split_once('=') {
            if !valid_attribute_name(key)
                || !value.starts_with('"')
                || !value.ends_with('"')
                || value.len() < 2
            {
                return None;
            }
            let value = decode_escapes(&value[1..value.len() - 1], &['\\', '"'])?;
            if key == "style-id" {
                if result.style_id.is_some() {
                    return None;
                }
                result.style_id = Some(value);
            } else if result.extra.insert(key.to_owned(), value).is_some() {
                return None;
            }
        } else {
            return None;
        }
    }
    Some(result)
}

fn valid_attribute_name(value: &str) -> bool {
    !value.is_empty()
        && value.bytes().enumerate().all(|(index, byte)| {
            byte.is_ascii_alphanumeric()
                || matches!(byte, b'-' | b'_' | b':' | b'.')
                || (index > 0 && byte == b'+')
        })
}

fn split_attribute_tokens(source: &str) -> Option<Vec<String>> {
    let mut result = Vec::new();
    let mut current = String::new();
    let mut quoted = false;
    let mut escaped = false;
    for character in source.chars() {
        if escaped {
            if !quoted || !matches!(character, '\\' | '"') {
                return None;
            }
            current.push(character);
            escaped = false;
            continue;
        }
        match character {
            '\\' if quoted => {
                current.push(character);
                escaped = true;
            }
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
    if quoted || escaped {
        return None;
    }
    if !current.is_empty() {
        result.push(current);
    }
    Some(result)
}

fn decode_escapes(value: &str, accepted: &[char]) -> Option<String> {
    let mut output = String::with_capacity(value.len());
    let mut chars = value.chars();
    while let Some(character) = chars.next() {
        if character != '\\' {
            output.push(character);
            continue;
        }
        let next = chars.next()?;
        if !accepted.contains(&next) {
            return None;
        }
        output.push(next);
    }
    Some(output)
}

fn parse_inlines(source: &str, budget: &mut ParseBudget<'_>) -> Result<Vec<Inline>, MarkdownError> {
    parse_inlines_at_depth(source, budget, 0)
}

fn parse_inlines_at_depth(
    source: &str,
    budget: &mut ParseBudget<'_>,
    depth: usize,
) -> Result<Vec<Inline>, MarkdownError> {
    let mut parser = InlineParser {
        source,
        position: 0,
        budget,
    };
    let (values, _) = parser.segment(None, depth)?;
    Ok(values)
}

struct InlineParser<'source, 'budget, 'options> {
    source: &'source str,
    position: usize,
    budget: &'budget mut ParseBudget<'options>,
}

impl InlineParser<'_, '_, '_> {
    fn segment(
        &mut self,
        terminator: Option<&str>,
        depth: usize,
    ) -> Result<(Vec<Inline>, bool), MarkdownError> {
        self.budget.depth(depth)?;
        let mut values = Vec::new();
        while self.position < self.source.len() {
            self.budget.delimiter_scan()?;
            if let Some(terminator) = terminator
                && self.is_closing(terminator)
            {
                self.position += terminator.len();
                return Ok((values, true));
            }
            if self.remaining().starts_with("  \n") || self.remaining().starts_with("\\\n") {
                self.position += 2;
                values.push(Inline::HardBreak);
                continue;
            }
            if self.remaining().starts_with('\n') {
                self.position += 1;
                values.push(Inline::SoftBreak);
                continue;
            }
            if self.remaining().starts_with('\\') {
                self.consume_escape(&mut values);
                continue;
            }
            if self.remaining().starts_with('`')
                && let Some(code) = self.code_span()?
            {
                values.push(Inline::Code(code));
                continue;
            }
            if let Some(link) = self.autolink() {
                values.push(link);
                continue;
            }
            if let Some(link) = self.styled_or_link(depth)? {
                values.push(link);
                continue;
            }
            let mut node = self.delimited("**", depth, Inline::Strong)?;
            if node.is_none() {
                node = self.delimited("__", depth, Inline::Strong)?;
            }
            if node.is_none() {
                node = self.delimited("~~", depth, Inline::Strikethrough)?;
            }
            if node.is_none() {
                node = self.delimited("*", depth, Inline::Emphasis)?;
            }
            if node.is_none() {
                node = self.delimited("_", depth, Inline::Emphasis)?;
            }
            if node.is_none() {
                node = self.delimited("<sup>", depth, Inline::Superscript)?;
            }
            if node.is_none() {
                node = self.delimited("<sub>", depth, Inline::Subscript)?;
            }
            if let Some(node) = node {
                values.push(node);
                continue;
            }
            self.push_current(&mut values);
        }
        Ok((values, false))
    }

    fn remaining(&self) -> &str {
        &self.source[self.position..]
    }

    fn is_closing(&self, marker: &str) -> bool {
        if !self.remaining().starts_with(marker) {
            return false;
        }
        // A single `*` must not close an outer emphasis span at the start of
        // a nested strong delimiter (`*a **b** c*`).
        !(marker.len() == 1
            && matches!(marker, "*" | "_")
            && self.remaining().starts_with(&format!("{marker}{marker}")))
    }

    fn push_current(&mut self, values: &mut Vec<Inline>) {
        let character = self.remaining().chars().next().expect("position is valid");
        self.position += character.len_utf8();
        push_text(values, &character.to_string());
    }

    fn consume_escape(&mut self, values: &mut Vec<Inline>) {
        self.position += 1;
        let Some(character) = self.remaining().chars().next() else {
            push_text(values, "\\");
            return;
        };
        if matches!(
            character,
            '\\' | '*' | '_' | '~' | '`' | '[' | ']' | '!' | '(' | ')' | '"'
        ) {
            self.position += character.len_utf8();
            push_text(values, &character.to_string());
        } else {
            push_text(values, "\\");
        }
    }

    fn delimited(
        &mut self,
        marker: &str,
        depth: usize,
        constructor: impl Fn(Vec<Inline>) -> Inline,
    ) -> Result<Option<Inline>, MarkdownError> {
        if !self.remaining().starts_with(marker) {
            return Ok(None);
        }
        let checkpoint = self.position;
        self.position += marker.len();
        let (children, closed) = self.segment(Some(marker), depth + 1)?;
        if closed && !children.is_empty() {
            Ok(Some(constructor(children)))
        } else {
            self.position = checkpoint;
            Ok(None)
        }
    }

    fn code_span(&mut self) -> Result<Option<String>, MarkdownError> {
        let checkpoint = self.position;
        let count = self
            .remaining()
            .bytes()
            .take_while(|byte| *byte == b'`')
            .count();
        self.position += count;
        while self.position < self.source.len() {
            self.budget.delimiter_scan()?;
            if self.remaining().starts_with('`') {
                let closing = self
                    .remaining()
                    .bytes()
                    .take_while(|byte| *byte == b'`')
                    .count();
                if closing == count {
                    let raw = &self.source[checkpoint + count..self.position];
                    self.position += count;
                    let normalized = raw.replace(['\r', '\n'], " ");
                    return Ok(Some(normalize_code_span(&normalized)));
                }
                self.position += closing;
                continue;
            }
            self.push_current(&mut Vec::new());
        }
        self.position = checkpoint;
        Ok(None)
    }

    fn autolink(&mut self) -> Option<Inline> {
        if !self.remaining().starts_with('<') {
            return None;
        }
        let end = self.remaining().find('>')?;
        let destination = self.remaining()[1..end].to_owned();
        if !(destination.starts_with("http://")
            || destination.starts_with("https://")
            || destination.starts_with("mailto:"))
        {
            return None;
        }
        self.position += end + 1;
        let label = destination.strip_prefix("mailto:").unwrap_or(&destination);
        Some(Inline::Link {
            label: vec![Inline::Text(label.to_owned())],
            destination,
            title: None,
        })
    }

    fn styled_or_link(&mut self, depth: usize) -> Result<Option<Inline>, MarkdownError> {
        let checkpoint = self.position;
        let image = self.remaining().starts_with("![");
        let open = if image {
            2
        } else if self.remaining().starts_with('[') {
            1
        } else {
            return Ok(None);
        };
        self.position += open;
        let Some(label_end) = self.find_unescaped(']')? else {
            self.position = checkpoint;
            return Ok(None);
        };
        let label = &self.source[checkpoint + open..label_end];
        self.position = label_end + 1;
        if !image
            && self.remaining().starts_with('{')
            && let Some(end) = self.remaining().find('}')
        {
            let attributes = parse_attributes(&self.remaining()[..=end]);
            if let Some(attributes) = attributes.filter(|attributes| {
                attributes
                    .classes
                    .iter()
                    .any(|class| class == "parchmint-style")
            }) {
                let children = parse_inlines_at_depth(label, self.budget, depth + 1)?;
                self.position += end + 1;
                return Ok(Some(Inline::Styled {
                    children,
                    attributes,
                }));
            }
        }
        if !self.remaining().starts_with('(') {
            self.position = checkpoint;
            return Ok(None);
        }
        self.position += 1;
        let content_start = self.position;
        let Some(content_end) = self.find_link_close()? else {
            self.position = checkpoint;
            return Ok(None);
        };
        let raw = &self.source[content_start..content_end];
        let Some((destination, title)) = parse_link_target(raw) else {
            self.position = checkpoint;
            return Ok(None);
        };
        self.position = content_end + 1;
        if image {
            let Some(alt) = decode_escapes(label, &['\\', '[', ']']) else {
                self.position = checkpoint;
                return Ok(None);
            };
            return Ok(Some(Inline::Image {
                alt,
                destination,
                title,
            }));
        }
        let children = parse_inlines_at_depth(label, self.budget, depth + 1)?;
        let mut node = Inline::Link {
            label: children,
            destination,
            title,
        };
        if self.remaining().starts_with('{')
            && let Some(end) = self.remaining().find('}')
            && let Some(attributes) =
                parse_attributes(&self.remaining()[..=end]).filter(|attributes| {
                    attributes
                        .classes
                        .iter()
                        .any(|class| class == "parchmint-style")
                })
        {
            let Inline::Link { label, .. } = node else {
                unreachable!()
            };
            node = Inline::Styled {
                children: label,
                attributes,
            };
            self.position += end + 1;
        }
        let _ = depth;
        Ok(Some(node))
    }

    fn find_unescaped(&mut self, needle: char) -> Result<Option<usize>, MarkdownError> {
        let mut escaped = false;
        let mut cursor = self.position;
        while cursor < self.source.len() {
            self.budget.delimiter_scan()?;
            let character = self.source[cursor..]
                .chars()
                .next()
                .expect("cursor is valid");
            if !escaped && character == needle {
                return Ok(Some(cursor));
            }
            escaped = !escaped && character == '\\';
            if character != '\\' {
                escaped = false;
            }
            cursor += character.len_utf8();
        }
        Ok(None)
    }

    fn find_link_close(&mut self) -> Result<Option<usize>, MarkdownError> {
        let mut depth = 0usize;
        let mut escaped = false;
        let mut quoted = false;
        let mut cursor = self.position;
        while cursor < self.source.len() {
            self.budget.delimiter_scan()?;
            let character = self.source[cursor..]
                .chars()
                .next()
                .expect("cursor is valid");
            if escaped {
                escaped = false;
                cursor += character.len_utf8();
                continue;
            }
            if character == '\\' {
                escaped = true;
                cursor += 1;
                continue;
            }
            if character == '"' {
                quoted = !quoted;
                cursor += 1;
                continue;
            }
            if !quoted {
                if character == '(' {
                    depth += 1;
                }
                if character == ')' {
                    if depth == 0 {
                        return Ok(Some(cursor));
                    }
                    depth -= 1;
                }
            }
            cursor += character.len_utf8();
        }
        Ok(None)
    }
}

fn normalize_code_span(value: &str) -> String {
    if value.len() >= 2 && value.starts_with(' ') && value.ends_with(' ') && value.trim() != "" {
        value[1..value.len() - 1].to_owned()
    } else {
        value.to_owned()
    }
}

fn parse_link_target(raw: &str) -> Option<(String, Option<String>)> {
    let mut escaped = false;
    let mut split = None;
    for (index, character) in raw.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        if character == '\\' {
            escaped = true;
            continue;
        }
        if character.is_ascii_whitespace() {
            split = Some(index);
            break;
        }
    }
    let (destination, title) = match split {
        None => (raw, None),
        Some(index) => {
            let destination = &raw[..index];
            let rest = raw[index..].trim();
            if !rest.starts_with('"') || !rest.ends_with('"') || rest.len() < 2 {
                return None;
            }
            (
                destination,
                Some(decode_escapes(&rest[1..rest.len() - 1], &['\\', '"'])?),
            )
        }
    };
    Some((decode_escapes(destination, &['\\', '(', ')', ' '])?, title))
}

fn has_reference_link(source: &str, budget: &mut ParseBudget<'_>) -> Result<bool, MarkdownError> {
    let bytes = source.as_bytes();
    let mut cursor = 0;
    while cursor < bytes.len() {
        budget.delimiter_scan()?;
        if bytes[cursor] == b'\\' {
            cursor = cursor.saturating_add(2);
            continue;
        }
        if bytes[cursor] == b'['
            && let Some(close) = source[cursor + 1..].find(']')
        {
            let after = cursor + close + 2;
            if bytes.get(after) == Some(&b'[') {
                return Ok(true);
            }
        }
        cursor += 1;
    }
    Ok(false)
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
    budget: &mut ParseBudget<'_>,
) -> Result<(), MarkdownError> {
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
                budget.diagnostic(diagnostics.len())?;
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
            check_style(attributes, &block.range, options, diagnostics, budget)?;
        }
        validate_inline_styles(&block.node, &block.range, options, diagnostics, budget)?;
        if block.node.is_opaque() {
            let message = match &block.node {
                BlockNode::Opaque { reason, .. } => reason.clone(),
                _ => unreachable!(),
            };
            budget.diagnostic(diagnostics.len())?;
            diagnostics.push(Diagnostic {
                severity: DiagnosticSeverity::Warning,
                code: "opaque-block",
                message,
                range: block.range.clone(),
            });
        }
    }
    Ok(())
}

fn validate_inline_styles(
    node: &BlockNode,
    range: &Range<usize>,
    options: &ParseOptions,
    diagnostics: &mut Vec<Diagnostic>,
    budget: &mut ParseBudget<'_>,
) -> Result<(), MarkdownError> {
    let inlines = match node {
        BlockNode::Paragraph { content, .. } | BlockNode::Heading { content, .. } => content,
        BlockNode::List { items, .. } => {
            for item in items {
                walk_inline_styles(&item.content, range, options, diagnostics, budget)?;
            }
            return Ok(());
        }
        _ => return Ok(()),
    };
    walk_inline_styles(inlines, range, options, diagnostics, budget)
}

fn walk_inline_styles(
    values: &[Inline],
    range: &Range<usize>,
    options: &ParseOptions,
    diagnostics: &mut Vec<Diagnostic>,
    budget: &mut ParseBudget<'_>,
) -> Result<(), MarkdownError> {
    for value in values {
        match value {
            Inline::Styled {
                children,
                attributes,
            } => {
                check_style(attributes, range, options, diagnostics, budget)?;
                walk_inline_styles(children, range, options, diagnostics, budget)?;
            }
            Inline::Emphasis(children)
            | Inline::Strong(children)
            | Inline::Strikethrough(children)
            | Inline::Superscript(children)
            | Inline::Subscript(children) => {
                walk_inline_styles(children, range, options, diagnostics, budget)?;
            }
            Inline::Link { label, .. } => {
                walk_inline_styles(label, range, options, diagnostics, budget)?;
            }
            _ => {}
        }
    }
    Ok(())
}

fn check_style(
    attributes: &Attributes,
    range: &Range<usize>,
    options: &ParseOptions,
    diagnostics: &mut Vec<Diagnostic>,
    budget: &mut ParseBudget<'_>,
) -> Result<(), MarkdownError> {
    if attributes
        .classes
        .iter()
        .any(|class| class == "parchmint-style")
    {
        match &attributes.style_id {
            None => {
                budget.diagnostic(diagnostics.len())?;
                diagnostics.push(Diagnostic {
                    severity: DiagnosticSeverity::Warning,
                    code: "missing-style-id",
                    message: "ParchMint style attribute has no stable style-id".into(),
                    range: range.clone(),
                });
            }
            Some(id)
                if !options.known_style_ids.is_empty() && !options.known_style_ids.contains(id) =>
            {
                budget.diagnostic(diagnostics.len())?;
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
    Ok(())
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
            let fence = markdown_fence(text);
            format!(
                "{fence}{info}\n{}{}{fence}\n\n",
                text,
                if text.ends_with('\n') { "" } else { "\n" },
            )
        }
        BlockNode::List {
            ordered,
            start,
            items,
        } => format!("{}\n", serialize_list(*ordered, *start, items, 0)),
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
            let mut output = format!("::: {}\n", serialize_attributes(&attributes, false));
            output.push_str(&children.iter().map(Block::serialize).collect::<String>());
            output.push_str(":::\n\n");
            output
        }
        BlockNode::PageBreak => format!("{PAGE_BREAK_MARKER}\n\n"),
    }
}

fn serialize_list(ordered: bool, start: u64, items: &[ListItem], indent: usize) -> String {
    let mut output = String::new();
    let prefix = " ".repeat(indent);
    for (index, item) in items.iter().enumerate() {
        let marker = if ordered {
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
            "{prefix}{marker} {task}{}",
            serialize_inlines(&item.content)
        );
        for child in &item.children {
            match child {
                BlockNode::List {
                    ordered,
                    start,
                    items,
                } => {
                    output.push_str(&serialize_list(*ordered, *start, items, indent + 2));
                }
                other => {
                    let rendered = serialize_node(other);
                    for line in rendered.lines().filter(|line| !line.is_empty()) {
                        let _ = writeln!(output, "{prefix}  {line}");
                    }
                }
            }
        }
    }
    output
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
                let fence = inline_code_fence(text);
                let padded = text.starts_with([' ', '`']) || text.ends_with([' ', '`']);
                if padded {
                    let _ = write!(output, "{fence} {text} {fence}");
                } else {
                    let _ = write!(output, "{fence}{text}{fence}");
                }
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
                    let _ = write!(output, " \"{}\"", escape_title(title));
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
                    let _ = write!(output, " \"{}\"", escape_title(title));
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
        if matches!(character, '\\' | '*' | '_' | '~' | '`' | '[' | ']' | '!') {
            output.push('\\');
        }
        output.push(character);
    }
    output
}

fn escape_destination(value: &str) -> String {
    let mut output = String::with_capacity(value.len());
    for character in value.chars() {
        if matches!(character, '\\' | '(' | ')' | ' ') {
            output.push('\\');
        }
        output.push(character);
    }
    output
}

fn escape_attribute(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

fn escape_title(value: &str) -> String {
    escape_attribute(value)
}

fn inline_code_fence(value: &str) -> String {
    "`".repeat(max_backtick_run(value).saturating_add(1).max(1))
}

fn markdown_fence(value: &str) -> String {
    "`".repeat(max_backtick_run(value).saturating_add(1).max(3))
}

fn max_backtick_run(value: &str) -> usize {
    let mut longest = 0;
    let mut current = 0;
    for byte in value.bytes() {
        if byte == b'`' {
            current += 1;
            longest = longest.max(current);
        } else {
            current = 0;
        }
    }
    longest
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
    fn changed_code_and_alignment_blocks_reparse_without_corruption() {
        let mut code = Document::parse_body(
            "```rust\nfn main() {}\n```\n\nAfter.\n",
            &ParseOptions::default(),
        )
        .unwrap();
        let code_node = code.blocks()[0].node.clone();
        code.replace_block(0, code_node).unwrap();
        let saved = code.serialize_body();
        assert!(saved.contains("fn main() {}\n```\n\nAfter."));
        let reparsed = Document::parse_body(&saved, &ParseOptions::default()).unwrap();
        assert!(matches!(
            reparsed.blocks()[0].node,
            BlockNode::CodeBlock { .. }
        ));
        assert!(matches!(
            reparsed.blocks()[1].node,
            BlockNode::Paragraph { .. }
        ));

        let mut alignment = Document::parse_body(
            "::: {.parchmint-align align=\"center\"}\nCentered.\n:::\n",
            &ParseOptions::default(),
        )
        .unwrap();
        let alignment_node = alignment.blocks()[0].node.clone();
        alignment.replace_block(0, alignment_node).unwrap();
        let saved = alignment.serialize_body();
        assert!(saved.starts_with("::: {.parchmint-align align=\"center\"}\n"));
        let reparsed = Document::parse_body(&saved, &ParseOptions::default()).unwrap();
        assert!(matches!(
            reparsed.blocks()[0].node,
            BlockNode::Alignment { .. }
        ));
    }

    #[test]
    fn benign_front_matter_variants_are_accepted() {
        let crlf = "---\r\ntitle: Novel\r\n---\r\nBody.\r\n";
        let parsed = Document::parse(crlf).unwrap();
        assert_eq!(
            parsed.front_matter()["title"],
            Value::String("Novel".into())
        );

        let empty = Document::parse("---\n---\nBody.\n").unwrap();
        assert!(empty.front_matter().is_empty());
        assert_eq!(empty.serialize(), "---\n---\nBody.\n");

        let eof = Document::parse("---\ntitle: Novel\n---").unwrap();
        assert_eq!(eof.front_matter()["title"], Value::String("Novel".into()));
        assert!(eof.serialize_body().is_empty());
    }

    #[test]
    fn plain_braces_and_comparisons_remain_editable_text() {
        let document =
            Document::parse_body("Keep {x}\n\n1 < 2 > 0\n", &ParseOptions::default()).unwrap();
        assert!(
            document
                .blocks()
                .iter()
                .all(|block| !block.node.is_opaque())
        );
        let BlockNode::Paragraph {
            content,
            attributes,
        } = &document.blocks()[0].node
        else {
            panic!("expected paragraph")
        };
        assert_eq!(content, &[Inline::Text("Keep {x}".into())]);
        assert_eq!(attributes, &Attributes::default());
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
            ..ParseOptions::default()
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

    #[test]
    fn changed_supported_blocks_and_inlines_reach_a_semantic_fixed_point() {
        let cases = [
            "Paragraph with *emphasis*, **strong**, ~~strike~~, `code`, <sup>up</sup>, <sub>down</sub>, [link\\] label](a\\(b\\) c \"a \\\"title\\\"\") and [style **inner**]{.parchmint-style style-id=\"style\"}.\\n",
            "# Heading {#heading .chapter}\n\n",
            "> Quoted source\n\n",
            "````rust\ncontains ``` and `` ticks\n````\n\n",
            "3. first\n4. second\n\n",
            "- parent\n  - child\n  - second child\n  continued paragraph\n- sibling\n\n",
            "---\n\n",
            "::: {.parchmint-align align=\"center\"}\nCentered *child*.\n:::\n\n",
            "<!-- parchmint:page-break -->\n\n",
        ];
        for source in cases {
            let mut document = Document::parse_body(source, &ParseOptions::default()).unwrap();
            let expected = document.blocks()[0].node.clone();
            document.replace_block(0, expected.clone()).unwrap();
            let mut saved = document.serialize_body();
            for _ in 0..20 {
                let mut reparsed = Document::parse_body(&saved, &ParseOptions::default()).unwrap();
                assert_eq!(reparsed.blocks()[0].node, expected, "{source}");
                let replacement = reparsed.blocks()[0].node.clone();
                reparsed.replace_block(0, replacement).unwrap();
                saved = reparsed.serialize_body();
            }
        }
    }

    #[test]
    fn inline_escapes_variable_code_spans_and_links_are_lossless_when_changed() {
        let source = r#"\*literal\* \[brackets\] \\ path [label\]](a\(b\)\ c%20d \"a \\\"title\\\"\") and ``a ` b``
"#;
        let mut document = Document::parse_body(source, &ParseOptions::default()).unwrap();
        let expected = document.blocks()[0].node.clone();
        for _ in 0..20 {
            document.replace_block(0, expected.clone()).unwrap();
            document =
                Document::parse_body(&document.serialize_body(), &ParseOptions::default()).unwrap();
            assert_eq!(document.blocks()[0].node, expected);
        }
    }

    #[test]
    fn malformed_attributes_and_reference_links_remain_visible_source() {
        let malformed = Document::parse_body(
            "Keep {#valid title=\"unterminated}\n",
            &ParseOptions::default(),
        )
        .unwrap();
        let BlockNode::Paragraph {
            content,
            attributes,
        } = &malformed.blocks()[0].node
        else {
            panic!()
        };
        assert_eq!(
            content,
            &[Inline::Text("Keep {#valid title=\"unterminated}".into())]
        );
        assert_eq!(attributes, &Attributes::default());

        let references =
            Document::parse_body("[chapter][ref]\n", &ParseOptions::default()).unwrap();
        assert!(references.blocks()[0].node.is_opaque());
        assert_eq!(references.serialize_body(), "[chapter][ref]\n");
    }

    #[test]
    fn parser_limits_are_typed_and_bound_recursive_input() {
        let byte_limited = ParseOptions {
            max_document_bytes: 3,
            ..ParseOptions::default()
        };
        assert!(matches!(
            Document::parse_body("four", &byte_limited),
            Err(MarkdownError::ResourceLimit {
                kind: "document-byte",
                ..
            })
        ));

        let depth_limited = ParseOptions {
            max_inline_depth: 4,
            ..ParseOptions::default()
        };
        let source = "*".repeat(20) + "text" + &"*".repeat(20);
        assert!(matches!(
            Document::parse_body(&source, &depth_limited),
            Err(MarkdownError::ResourceLimit {
                kind: "inline-depth",
                ..
            })
        ));

        let delimiter_limited = ParseOptions {
            max_delimiter_scans: 8,
            ..ParseOptions::default()
        };
        assert!(matches!(
            Document::parse_body("x *x *x *x *x *x *x *x *x *x", &delimiter_limited),
            Err(MarkdownError::ResourceLimit {
                kind: "delimiter-scan",
                ..
            })
        ));
    }
}
