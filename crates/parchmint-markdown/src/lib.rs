//! Source-aware semantic Markdown spike.
//!
//! Stage 01 deliberately keeps supported and opaque source slices. Stage 03 can
//! replace the line scanner with a complete semantic codec without changing the
//! public `Document` boundary.

use pulldown_cmark::{Event, Options, Parser, Tag};
use serde_yaml::{Mapping, Value};
use std::ops::Range;
use thiserror::Error;

const PAGE_BREAK: &str = "<!-- parchmint:page-break -->";

/// A parsed ParchMint Markdown document.
#[derive(Clone, Debug, PartialEq)]
pub struct Document {
    raw_front_matter: String,
    front_matter: Mapping,
    blocks: Vec<Block>,
}

impl Document {
    /// Parses YAML front matter and source-backed semantic blocks.
    pub fn parse(source: &str) -> Result<Self, MarkdownError> {
        let (raw_front_matter, front_matter, body, body_offset) = parse_front_matter(source)?;
        validate_commonmark(body, body_offset)?;
        let blocks = scan_blocks(body, body_offset);
        Ok(Self {
            raw_front_matter,
            front_matter,
            blocks,
        })
    }

    /// Returns all front-matter keys, including keys unknown to ParchMint.
    pub const fn front_matter(&self) -> &Mapping {
        &self.front_matter
    }

    /// Returns semantic blocks in source order.
    pub fn blocks(&self) -> &[Block] {
        &self.blocks
    }

    /// Serializes deterministically without consulting Qt conversion APIs.
    pub fn serialize(&self) -> String {
        let body = self.blocks.iter().map(Block::source).collect::<String>();
        if self.raw_front_matter.is_empty() {
            body
        } else {
            format!("---\n{}---\n{body}", self.raw_front_matter)
        }
    }
}

/// A source-backed semantic block.
#[derive(Clone, Debug, PartialEq)]
pub struct Block {
    /// Classification used by the editor adapter.
    pub kind: BlockKind,
    /// Absolute source range for diagnostics and raw-source mode.
    pub range: Range<usize>,
    source: String,
}

impl Block {
    /// Returns the canonical source slice retained for this block.
    pub fn source(&self) -> &str {
        &self.source
    }
}

/// Constructs whose representation is fixed by the initial grammar ADR.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BlockKind {
    /// CommonMark or a documented ParchMint extension understood semantically.
    Supported,
    /// Semantic page-break marker.
    PageBreak,
    /// Fenced block that the WYSIWYG layer must present as protected source.
    Opaque,
}

/// Parse failures carry byte offsets usable by source mode.
#[derive(Debug, Error)]
pub enum MarkdownError {
    /// Opening front matter did not have a closing delimiter.
    #[error("front matter opened at byte 0 but has no closing delimiter")]
    UnclosedFrontMatter,
    /// YAML is malformed or does not contain a mapping.
    #[error("invalid YAML front matter: {0}")]
    InvalidFrontMatter(String),
    /// Parser produced an invalid source range.
    #[error("Markdown parser emitted invalid source range {start}..{end}")]
    InvalidSourceRange {
        /// Invalid absolute start offset.
        start: usize,
        /// Invalid absolute end offset.
        end: usize,
    },
}

fn parse_front_matter(source: &str) -> Result<(String, Mapping, &str, usize), MarkdownError> {
    if !source.starts_with("---\n") {
        return Ok((String::new(), Mapping::new(), source, 0));
    }
    let tail = &source[4..];
    let end = tail
        .find("\n---\n")
        .ok_or(MarkdownError::UnclosedFrontMatter)?;
    let raw = &tail[..=end];
    let value: Value = serde_yaml::from_str(raw)
        .map_err(|error| MarkdownError::InvalidFrontMatter(error.to_string()))?;
    let mapping = value
        .as_mapping()
        .cloned()
        .ok_or_else(|| MarkdownError::InvalidFrontMatter("root must be a mapping".into()))?;
    let offset = 4 + end + 5;
    Ok((raw.to_owned(), mapping, &source[offset..], offset))
}

fn validate_commonmark(body: &str, offset: usize) -> Result<(), MarkdownError> {
    let options = Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TABLES
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_FOOTNOTES;
    for (event, range) in Parser::new_ext(body, options).into_offset_iter() {
        if range.end > body.len() || range.start > range.end {
            return Err(MarkdownError::InvalidSourceRange {
                start: offset + range.start,
                end: offset + range.end,
            });
        }
        // Force the parser to exercise extension tags and HTML events during the
        // spike; source ownership remains in this crate.
        let _semantic_event = matches!(event, Event::Start(Tag::Paragraph));
    }
    Ok(())
}

fn scan_blocks(body: &str, body_offset: usize) -> Vec<Block> {
    let mut blocks = Vec::new();
    let mut start = 0;
    for chunk in body.split_inclusive("\n\n") {
        let trimmed = chunk.trim();
        let kind = if trimmed == PAGE_BREAK {
            BlockKind::PageBreak
        } else if trimmed.starts_with("```{=parchmint-opaque")
            || trimmed.starts_with("::: {.parchmint-opaque")
        {
            BlockKind::Opaque
        } else {
            BlockKind::Supported
        };
        let end = start + chunk.len();
        blocks.push(Block {
            kind,
            range: body_offset + start..body_offset + end,
            source: chunk.to_owned(),
        });
        start = end;
    }
    if body.is_empty() {
        blocks.clear();
    }
    blocks
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = include_str!("../../../tests/fixtures/spike/representative.md");

    #[test]
    fn representative_fixture_round_trips_byte_for_byte() {
        let parsed = Document::parse(FIXTURE).unwrap();
        assert_eq!(parsed.serialize(), FIXTURE);
        assert!(
            parsed
                .front_matter()
                .contains_key(Value::from("future-plugin"))
        );
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
    }

    #[test]
    fn repeated_serialization_is_stable() {
        let once = Document::parse(FIXTURE).unwrap().serialize();
        let twice = Document::parse(&once).unwrap().serialize();
        assert_eq!(once, twice);
    }

    #[test]
    fn parser_exposes_absolute_source_ranges() {
        let parsed = Document::parse(FIXTURE).unwrap();
        for block in parsed.blocks() {
            assert_eq!(&FIXTURE[block.range.clone()], block.source());
        }
    }
}
