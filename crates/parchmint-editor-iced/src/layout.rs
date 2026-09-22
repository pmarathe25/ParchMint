use parchmint_editor_api::style_id_from_canonical as parse_style_id;
use std::collections::BTreeMap;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::sync::{Arc, OnceLock};

use parchmint_editor_api::{
    AtomicBlockKind, BlockId, DocumentPosition, EditorSelection, InlineFontFamily,
    SelectionRectangle, SemanticBlockKind, SemanticInlineMark, StyleCatalog,
    StyleCatalogProjection, StyleId, StyleProperties, TextAlignment,
};

const TAB_COLUMNS: f32 = 4.0;
const LAYOUT_CHUNK_SCALARS: usize = 1_024;

/// The visible host area of one mounted editor pane, in logical pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EditorViewport {
    pub width: f32,
    pub height: f32,
}

impl EditorViewport {
    pub fn new(width: f32, height: f32) -> Result<Self, &'static str> {
        if !width.is_finite() || width <= 0.0 {
            return Err("viewport width must be positive and finite");
        }
        if !height.is_finite() || height <= 0.0 {
            return Err("viewport height must be positive and finite");
        }
        Ok(Self { width, height })
    }
}

/// Deterministic logical metrics consumed by both rendering and interaction.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EditorLayoutMetrics {
    pub inset_x: f32,
    pub inset_y: f32,
    pub scalar_width: f32,
    pub line_height: f32,
    pub caret_width: f32,
}

impl Default for EditorLayoutMetrics {
    fn default() -> Self {
        Self {
            // Match the manuscript page rather than the surrounding desktop
            // chrome. These values are shared by painting, hit testing,
            // selection, scrolling, and virtualization.
            inset_x: 54.0,
            inset_y: 32.0,
            scalar_width: 9.0,
            line_height: 29.0,
            caret_width: 1.0,
        }
    }
}

impl EditorLayoutMetrics {
    pub(crate) fn validate(self) -> Result<(), &'static str> {
        if !self.inset_x.is_finite() || !self.inset_y.is_finite() {
            return Err("layout insets must be finite");
        }
        if !self.scalar_width.is_finite() || self.scalar_width <= 0.0 {
            return Err("scalar width must be positive and finite");
        }
        if !self.line_height.is_finite() || self.line_height <= 0.0 {
            return Err("line height must be positive and finite");
        }
        if !self.caret_width.is_finite() || self.caret_width <= 0.0 {
            return Err("caret width must be positive and finite");
        }
        Ok(())
    }
}

/// One semantic block supplied to the viewport cache by the editor host.
#[derive(Debug, Clone, PartialEq)]
pub struct VisibleEditorBlock {
    block: BlockId,
    text: VisibleText,
    document_start: DocumentPosition,
    mark_ranges: Vec<VisibleMarkRange>,
    atomic_nodes: Vec<(DocumentPosition, AtomicBlockKind)>,
    block_spans: Vec<VisibleBlockSpan>,
    layout_lines: Vec<VisibleLayoutLine>,
    scalar_len: u64,
    layout_signature: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct VisibleLayoutChunk {
    scalar_offset: usize,
    scalar_len: usize,
    /// UTF-8 byte range relative to the line, independent of document position.
    text_range: std::ops::Range<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct VisibleLayoutLine {
    start: DocumentPosition,
    end: DocumentPosition,
    segment_index: usize,
    /// UTF-8 byte range within the source paragraph (or plain-text input).
    text_range: std::ops::Range<usize>,
    shape: Arc<LineTextShape>,
    hard_break: Option<DocumentPosition>,
    span_index: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct LineTextShape {
    text_signature: u64,
    chunks: Vec<VisibleLayoutChunk>,
    scalar_len: usize,
}

/// Render directly from immutable paragraph payloads. The public full-text
/// accessor joins them only when requested; layout never needs that copy.
#[derive(Debug, Clone)]
enum VisibleText {
    Plain(Arc<String>),
    Semantic {
        document: parchmint_editor_api::SemanticDocument,
        flattened: OnceLock<Arc<String>>,
    },
}

impl VisibleText {
    fn segment_count(&self) -> usize {
        match self {
            Self::Plain(_) => 1,
            Self::Semantic { document, .. } => document.blocks().len().max(1),
        }
    }

    fn segment(&self, index: usize) -> &str {
        match self {
            Self::Plain(text) => text,
            Self::Semantic { document, .. } => {
                document
                    .blocks()
                    .get(index)
                    .map_or("", |block| match block.kind() {
                        SemanticBlockKind::SceneBreak | SemanticBlockKind::PageBreak => "\u{fffc}",
                        _ => block.text(),
                    })
            }
        }
    }

    fn line(&self, line: &VisibleLayoutLine) -> &str {
        &self.segment(line.segment_index)[line.text_range.clone()]
    }

    fn bytes(&self) -> impl Iterator<Item = u8> + '_ {
        (0..self.segment_count()).flat_map(move |index| {
            self.segment(index)
                .bytes()
                .chain((index + 1 < self.segment_count()).then_some(b'\n'))
        })
    }

    fn as_str(&self) -> &str {
        match self {
            Self::Plain(text) => text,
            Self::Semantic {
                document,
                flattened,
            } => flattened.get_or_init(|| Arc::new(document.plain_text())),
        }
    }
}

impl PartialEq for VisibleText {
    fn eq(&self, other: &Self) -> bool {
        // Cache initialization and unrelated semantic metadata do not affect
        // text equality. Preserve equality between alternate segmentations.
        if self.segment_count() == other.segment_count()
            && (0..self.segment_count()).all(|index| self.segment(index) == other.segment(index))
        {
            return true;
        }
        self.bytes().eq(other.bytes())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct VisibleMarkRange {
    range: EditorSelection,
    mark: SemanticInlineMark,
}

#[derive(Debug, Clone, PartialEq)]
struct VisibleBlockSpan {
    start: DocumentPosition,
    end: DocumentPosition,
    kind: SemanticBlockKind,
    list_depth: usize,
    list_ordinal: usize,
    style: Arc<ResolvedBlockStyle>,
    style_signature: u64,
    marks: std::ops::Range<usize>,
    font_signature: u64,
}

#[derive(Debug, Clone, Default, PartialEq)]
struct ResolvedBlockStyle {
    font_family: Option<EditorFontFamily>,
    font_size_points: Option<f32>,
    weight: Option<u16>,
    italic: Option<bool>,
    text_decoration: Option<parchmint_editor_api::TextDecoration>,
    alignment: Option<TextAlignment>,
    first_line_indent_points: Option<f32>,
    left_indent_points: Option<f32>,
    right_indent_points: Option<f32>,
    line_spacing: Option<f32>,
    space_before_points: Option<f32>,
    space_after_points: Option<f32>,
}

impl VisibleEditorBlock {
    pub fn new(block: BlockId, text: impl Into<String>, document_start: DocumentPosition) -> Self {
        let text = text.into();
        let scalar_len = text.chars().count() as u64;
        let text = VisibleText::Plain(Arc::new(text));
        let layout_lines = build_layout_lines(&text, document_start, &[], None);
        Self {
            block,
            layout_signature: layout_signature(&layout_lines, &[]),
            text,
            document_start,
            mark_ranges: Vec::new(),
            atomic_nodes: Vec::new(),
            block_spans: Vec::new(),
            layout_lines,
            scalar_len,
        }
    }

    pub fn with_bold_ranges(mut self, ranges: Vec<EditorSelection>) -> Self {
        self.mark_ranges
            .extend(ranges.into_iter().map(|range| VisibleMarkRange {
                range,
                mark: SemanticInlineMark::Bold,
            }));
        self
    }

    pub fn from_semantic(
        block: BlockId,
        semantic: &parchmint_editor_api::SemanticDocument,
        document_start: DocumentPosition,
    ) -> Self {
        Self::from_semantic_with_styles(
            block,
            semantic,
            document_start,
            &StyleCatalogProjection::default(),
        )
    }

    pub fn from_semantic_with_styles(
        block: BlockId,
        semantic: &parchmint_editor_api::SemanticDocument,
        document_start: DocumentPosition,
        styles: &StyleCatalogProjection,
    ) -> Self {
        Self::build_semantic(block, semantic, document_start, styles, None)
    }

    pub(crate) fn update_semantic(
        &mut self,
        semantic: &parchmint_editor_api::SemanticDocument,
        styles: &StyleCatalogProjection,
    ) {
        *self = Self::build_semantic(
            self.block,
            semantic,
            self.document_start,
            styles,
            Some(self),
        );
    }

    fn build_semantic(
        block: BlockId,
        semantic: &parchmint_editor_api::SemanticDocument,
        document_start: DocumentPosition,
        styles: &StyleCatalogProjection,
        previous: Option<&Self>,
    ) -> Self {
        let mut offset = document_start.value();
        let mut mark_ranges = Vec::new();
        let mut atomic_nodes = Vec::new();
        let mut block_spans = Vec::new();
        let mut ordered_ordinals: Vec<usize> = Vec::new();
        // Catalog inheritance is identical for blocks using the same style.
        // Keep this cache local so a catalog update cannot leave stale styles.
        let mut resolved_styles = BTreeMap::new();
        for semantic_block in semantic.blocks() {
            let block_start = offset;
            let mark_start = mark_ranges.len();
            let mut font_hash = DefaultHasher::new();
            for mark in semantic_block.marks() {
                match mark.mark() {
                    SemanticInlineMark::FontFamily(family) => {
                        mark.range().start().value().hash(&mut font_hash);
                        mark.range().end().value().hash(&mut font_hash);
                        (0_u8, *family).hash(&mut font_hash);
                    }
                    SemanticInlineMark::FontSize(size) => {
                        mark.range().start().value().hash(&mut font_hash);
                        mark.range().end().value().hash(&mut font_hash);
                        (1_u8, *size).hash(&mut font_hash);
                    }
                    _ => {}
                }
                mark_ranges.push(VisibleMarkRange {
                    range: EditorSelection::new(
                        DocumentPosition::from(offset + mark.range().start().value()),
                        DocumentPosition::from(offset + mark.range().end().value()),
                    ),
                    mark: mark.mark().clone(),
                });
            }
            match semantic_block.kind() {
                SemanticBlockKind::SceneBreak => {
                    atomic_nodes
                        .push((DocumentPosition::from(offset), AtomicBlockKind::SceneBreak));
                }
                SemanticBlockKind::PageBreak => {
                    atomic_nodes.push((DocumentPosition::from(offset), AtomicBlockKind::PageBreak));
                }
                _ => {}
            }
            let scalar_len = semantic_block.scalar_len() as u64;
            let list_ordinal = if semantic_block.kind() == SemanticBlockKind::OrderedListItem {
                let depth = semantic_block.list_depth();
                ordered_ordinals.truncate(depth + 1);
                if ordered_ordinals.len() <= depth {
                    ordered_ordinals.resize(depth + 1, 0);
                }
                ordered_ordinals[depth] += 1;
                ordered_ordinals[depth]
            } else {
                ordered_ordinals.clear();
                0
            };
            let (style, style_signature) = resolved_styles
                .entry(block_style_id(semantic_block))
                .or_insert_with_key(|id| {
                    let style = Arc::new(resolve_block_style(*id, styles.catalog()));
                    let signature = style_layout_signature(&style);
                    (style, signature)
                });
            let mut style = Arc::clone(style);
            let mut style_signature = *style_signature;
            let format = semantic_block.paragraph_format();
            if format != Default::default() {
                let resolved = Arc::make_mut(&mut style);
                if let Some(alignment) = format.alignment {
                    resolved.alignment = Some(alignment);
                }
                if let Some(percent) = format.line_spacing_percent {
                    resolved.line_spacing = Some(f32::from(percent) / 100.0);
                }
                style_signature = style_layout_signature(resolved);
            }
            block_spans.push(VisibleBlockSpan {
                start: DocumentPosition::from(block_start),
                end: DocumentPosition::from(block_start + scalar_len),
                kind: semantic_block.kind(),
                list_depth: semantic_block.list_depth(),
                list_ordinal,
                style,
                style_signature,
                marks: mark_start..mark_ranges.len(),
                font_signature: font_hash.finish(),
            });
            offset += scalar_len + 1;
        }
        let text = VisibleText::Semantic {
            document: semantic.clone(),
            flattened: OnceLock::new(),
        };
        let scalar_len = offset
            .saturating_sub(document_start.value())
            .saturating_sub(1);
        let layout_lines = build_layout_lines(&text, document_start, &block_spans, previous);
        Self {
            block,
            layout_signature: layout_signature(&layout_lines, &block_spans),
            text,
            document_start,
            mark_ranges,
            atomic_nodes,
            block_spans,
            layout_lines,
            scalar_len,
        }
    }

    pub const fn block(&self) -> BlockId {
        self.block
    }

    pub fn text(&self) -> &str {
        self.text.as_str()
    }

    pub const fn document_start(&self) -> DocumentPosition {
        self.document_start
    }
}

fn build_layout_lines(
    text: &VisibleText,
    document_start: DocumentPosition,
    spans: &[VisibleBlockSpan],
    previous: Option<&VisibleEditorBlock>,
) -> Vec<VisibleLayoutLine> {
    let old = previous.map_or(&[][..], |input| input.layout_lines.as_slice());
    let mut old_cursor = 0;
    let mut text_lines = Vec::with_capacity(text.segment_count());
    for segment in 0..text.segment_count() {
        let old_start = old_cursor;
        while old
            .get(old_cursor)
            .is_some_and(|line| line.segment_index == segment)
        {
            old_cursor += 1;
        }
        if old_start != old_cursor
            && previous.is_some_and(|input| {
                std::ptr::eq(text.segment(segment), input.text.segment(segment))
            })
        {
            text_lines.extend(
                old[old_start..old_cursor]
                    .iter()
                    .map(|line| (segment, line.text_range.clone())),
            );
            continue;
        }
        let mut start = 0;
        for line in text.segment(segment).split('\n') {
            let end = start + line.len();
            text_lines.push((segment, start..end));
            start = end + 1;
        }
    }
    let matches = |segment: usize, range: std::ops::Range<usize>, line: &VisibleLayoutLine| {
        let old_text = previous.expect("previous line source").text.line(line);
        let new_text = &text.segment(segment)[range];
        std::ptr::eq(new_text, old_text) || new_text == old_text
    };
    let suffix = text_lines
        .iter()
        .rev()
        .zip(old.iter().rev())
        .take_while(|((segment, range), line)| matches(*segment, range.clone(), line))
        .count();
    let mut lines = Vec::with_capacity(text_lines.len());
    let mut line_start = document_start.value();
    let mut span_cursor = 0;
    for (line_index, (segment_index, text_range)) in text_lines.iter().enumerate() {
        // Soft breaks and empty paragraphs retain the same ordered span lookup
        // as a flattened document. Segment boundaries add one hard break.
        while spans
            .get(span_cursor)
            .is_some_and(|span| span.end.value() < line_start)
        {
            span_cursor += 1;
        }
        let span_index = spans
            .get(span_cursor)
            .filter(|span| span.start.value() <= line_start)
            .map(|_| span_cursor);
        let reusable = old
            .get(line_index)
            .filter(|line| matches(*segment_index, text_range.clone(), line))
            .or_else(|| {
                (line_index >= text_lines.len() - suffix)
                    .then(|| &old[old.len() - (text_lines.len() - line_index)])
            });
        let shape = reusable.map_or_else(
            || {
                Arc::new(build_line_text_shape(
                    &text.segment(*segment_index)[text_range.clone()],
                ))
            },
            |line| Arc::clone(&line.shape),
        );
        let line_end = line_start.saturating_add(shape.scalar_len as u64);
        lines.push(VisibleLayoutLine {
            start: DocumentPosition::from(line_start),
            end: DocumentPosition::from(line_end),
            segment_index: *segment_index,
            text_range: text_range.clone(),
            shape,
            hard_break: (line_index + 1 < text_lines.len())
                .then_some(DocumentPosition::from(line_end)),
            span_index,
        });
        line_start = line_end.saturating_add(1);
    }
    lines
}

fn build_line_text_shape(text: &str) -> LineTextShape {
    let mut chunks = Vec::new();
    let mut chunk_start = 0;
    let mut chunk_offset = 0;
    let mut chunk_len = 0;
    for (offset, character) in text.char_indices() {
        chunk_len += 1;
        if chunk_len == LAYOUT_CHUNK_SCALARS {
            let chunk_end = offset + character.len_utf8();
            chunks.push(VisibleLayoutChunk {
                scalar_offset: chunk_offset,
                scalar_len: chunk_len,
                text_range: chunk_start..chunk_end,
            });
            chunk_start = chunk_end;
            chunk_offset += chunk_len;
            chunk_len = 0;
        }
    }
    if chunk_len > 0 {
        chunks.push(VisibleLayoutChunk {
            scalar_offset: chunk_offset,
            scalar_len: chunk_len,
            text_range: chunk_start..text.len(),
        });
    }
    let mut hash = DefaultHasher::new();
    text.hash(&mut hash);
    LineTextShape {
        chunks,
        scalar_len: chunk_offset + chunk_len,
        text_signature: hash.finish(),
    }
}

fn layout_signature(lines: &[VisibleLayoutLine], spans: &[VisibleBlockSpan]) -> u64 {
    let mut hash = DefaultHasher::new();
    lines.len().hash(&mut hash);
    for line in lines {
        line.shape.text_signature.hash(&mut hash);
    }
    for span in spans {
        span.start.hash(&mut hash);
        span.end.hash(&mut hash);
        hash_span_layout(span, &mut hash);
    }
    hash.finish()
}

fn hash_span_layout(span: &VisibleBlockSpan, hash: &mut impl Hasher) {
    std::mem::discriminant(&span.kind).hash(hash);
    span.list_depth.hash(hash);
    span.list_ordinal.hash(hash);
    span.style_signature.hash(hash);
    span.font_signature.hash(hash);
}

fn style_layout_signature(style: &ResolvedBlockStyle) -> u64 {
    let mut hash = DefaultHasher::new();
    style.font_family.hash(&mut hash);
    style.weight.hash(&mut hash);
    style.italic.hash(&mut hash);
    style.text_decoration.hash(&mut hash);
    style
        .alignment
        .map(|alignment| std::mem::discriminant(&alignment))
        .hash(&mut hash);
    [
        style.font_size_points,
        style.first_line_indent_points,
        style.left_indent_points,
        style.right_indent_points,
        style.line_spacing,
        style.space_before_points,
        style.space_after_points,
    ]
    .map(|value| value.map(f32::to_bits))
    .hash(&mut hash);
    hash.finish()
}

fn line_layout_signature(input: &VisibleEditorBlock, line: &VisibleLayoutLine) -> u64 {
    let mut hash = DefaultHasher::new();
    line.shape.text_signature.hash(&mut hash);
    if let Some(index) = line.span_index {
        hash_span_layout(&input.block_spans[index], &mut hash);
    }
    hash.finish()
}

/// A finite rectangle from the single editor layout result.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EditorRectangle {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl From<EditorRectangle> for SelectionRectangle {
    fn from(value: EditorRectangle) -> Self {
        Self {
            x: value.x,
            y: value.y,
            width: value.width,
            height: value.height,
        }
    }
}

/// One drawable UTF-8 scalar and its authoritative hit-test rectangle.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EditorScalarGeometry {
    pub position: DocumentPosition,
    pub character: char,
    pub bounds: EditorRectangle,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strikethrough: bool,
    pub link: bool,
    pub small_caps: bool,
    pub superscript: bool,
    pub subscript: bool,
    pub block_kind: SemanticBlockKind,
    pub list_depth: usize,
    pub list_marker: Option<usize>,
    pub block_start: bool,
    pub font_size: f32,
    /// Offset within the row used to align mixed-size text baselines.
    pub text_offset_y: f32,
    pub font_weight: u16,
    pub block_italic: bool,
    pub font_family: EditorFontFamily,
    pub atomic: Option<AtomicBlockKind>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EditorFontFamily {
    SansSerif,
    Serif,
    Monospace,
}

impl From<InlineFontFamily> for EditorFontFamily {
    fn from(family: InlineFontFamily) -> Self {
        match family {
            InlineFontFamily::Serif => Self::Serif,
            InlineFontFamily::SansSerif => Self::SansSerif,
            InlineFontFamily::Monospace => Self::Monospace,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct LayoutWork {
    pub materialized_lines: usize,
    pub materialized_chunks: usize,
    pub materialized_scalars: usize,
}

#[derive(Debug, Clone, PartialEq)]
struct LineHeightEntry {
    signature: u64,
    line_index: usize,
    start: DocumentPosition,
    end: DocumentPosition,
    start_y: f32,
    end_y: f32,
    metrics: Arc<LineMetrics>,
}

impl std::ops::Deref for LineHeightEntry {
    type Target = LineMetrics;

    fn deref(&self) -> &Self::Target {
        &self.metrics
    }
}

/// Unchanged lines share their position-independent metrics as one immutable group.
#[derive(Debug, PartialEq)]
struct LineMetrics {
    scalar_len: usize,
    /// Logical advances used by both the canvas positions and the caret map.
    ///
    /// Canvas shapes a proportional font, so a fixed cell for every scalar
    /// causes wide glyphs to paint into their neighbours while spaces become
    /// visibly too wide. Keeping the deterministic advances here makes the
    /// viewport cache, rendering, hit testing, and wrapping use one model.
    scalar_advances: ScalarAdvances,
    /// Scalar offsets that begin a visual row. These are computed once from
    /// word boundaries and consumed by every geometry path.
    wrap_before: Box<[usize]>,
    /// A cursor state at every chunk boundary. Lookup may scan at most one
    /// chunk, keeping deep single-line documents linear to index and bounded
    /// to materialize.
    prefix_cursors: Box<[PrefixCursor]>,
    line_height: f32,
    /// Only mixed-size lines need per-row vertical metrics.
    rows: Box<[RowMetrics]>,
    first_x: f32,
    continuation_x: f32,
    /// Alignment is resolved for each wrapped row, before caret construction.
    row_origins: Box<[f32]>,
    justified_spaces: BTreeMap<usize, f32>,
    chunk_rows: Box<[(usize, usize)]>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct RowMetrics {
    top: f32,
    height: f32,
    font_size: f32,
}

impl LineMetrics {
    fn row_x(&self, row: usize) -> f32 {
        self.row_origins.get(row).copied().unwrap_or(if row == 0 {
            self.first_x
        } else {
            self.continuation_x
        })
    }

    fn advance(&self, offset: usize, metrics: EditorLayoutMetrics) -> Option<f32> {
        self.scalar_advances
            .get(offset, metrics)
            .map(|width| width + self.justified_spaces.get(&offset).copied().unwrap_or(0.0))
    }

    fn row_top(&self, row: usize) -> f32 {
        self.rows.get(row).map_or_else(
            || {
                self.rows
                    .last()
                    .map_or(row as f32 * self.line_height, |last| last.top + last.height)
            },
            |row| row.top,
        )
    }

    fn row_height(&self, row: usize) -> f32 {
        self.rows
            .get(row)
            .map_or(self.line_height, |row| row.height)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct PrefixCursor {
    scalar_offset: usize,
    row: usize,
    x: f32,
}

/// Lossless codes for the deterministic width model, shared across revisions.
/// Store one byte per scalar instead of repeating the scaled f32 advance.
#[derive(Debug, Clone, PartialEq)]
struct ScalarAdvances {
    codes: Box<[u8]>,
    base: f32,
    sizes: Box<[FontSizeRun]>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct FontSizeRun {
    start: usize,
    end: usize,
    base: f32,
}

impl ScalarAdvances {
    fn new(
        characters: &[char],
        span: Option<&VisibleBlockSpan>,
        metrics: EditorLayoutMetrics,
    ) -> Self {
        let family = span
            .and_then(|span| span.style.font_family)
            .unwrap_or(EditorFontFamily::Serif);
        let font_size = span
            .and_then(|span| span.style.font_size_points)
            .map(points_to_pixels)
            .unwrap_or_else(|| default_font_size(span.map(|span| span.kind)));
        Self {
            codes: characters
                .iter()
                .map(|character| scalar_width_code(*character, family))
                .collect(),
            base: metrics.scalar_width * (font_size / 20.0),
            sizes: Box::default(),
        }
    }

    fn apply_fonts(
        &mut self,
        characters: &[char],
        marks: &[VisibleMarkRange],
        start: u64,
        metrics: EditorLayoutMetrics,
    ) {
        let mut sizes = Vec::new();
        for mark in marks {
            let from = mark
                .range
                .start()
                .value()
                .saturating_sub(start)
                .min(characters.len() as u64) as usize;
            let to = mark
                .range
                .end()
                .value()
                .saturating_sub(start)
                .min(characters.len() as u64) as usize;
            if from >= to {
                continue;
            }
            match mark.mark {
                SemanticInlineMark::FontFamily(family) => {
                    for (code, character) in
                        self.codes[from..to].iter_mut().zip(&characters[from..to])
                    {
                        *code = scalar_width_code(*character, family.into());
                    }
                }
                SemanticInlineMark::FontSize(points) => {
                    if sizes.is_empty() {
                        sizes.resize(characters.len(), self.base);
                    }
                    sizes[from..to]
                        .fill(metrics.scalar_width * (points_to_pixels(f32::from(points)) / 20.0));
                }
                _ => {}
            }
        }
        // Discard the temporary dense vector; retain only size transitions.
        let mut runs = Vec::new();
        let mut start = 0;
        while start < sizes.len() {
            let base = sizes[start];
            let end = start
                + sizes[start..]
                    .iter()
                    .take_while(|value| **value == base)
                    .count();
            if base != self.base {
                runs.push(FontSizeRun { start, end, base });
            }
            start = end;
        }
        self.sizes = runs.into();
    }

    fn base_at(&self, index: usize) -> f32 {
        let next = self.sizes.partition_point(|run| run.end <= index);
        self.sizes
            .get(next)
            .filter(|run| run.start <= index)
            .map_or(self.base, |run| run.base)
    }

    fn width(code: u8, base: f32, metrics: EditorLayoutMetrics) -> f32 {
        // Compute the same rounded f32 proportions as the original literals.
        // The compile-time table avoids a division while reading each scalar.
        static PROPORTIONS: [f32; 256] = {
            let mut values = [0.0; 256];
            let mut i = 0;
            while i < values.len() {
                values[i] = i as f32 / 100.0;
                i += 1;
            }
            values
        };
        match code {
            0 => metrics.scalar_width * TAB_COLUMNS,
            // Generic monospace faces use approximately 0.6 em per cell;
            // the proportional body base is only 0.45 em.
            1 => base * (4.0 / 3.0),
            _ => (base * PROPORTIONS[usize::from(code)]).max(metrics.caret_width),
        }
    }

    fn get(&self, index: usize, metrics: EditorLayoutMetrics) -> Option<f32> {
        self.codes
            .get(index)
            .map(|code| Self::width(*code, self.base_at(index), metrics))
    }

    #[cfg(test)]
    fn iter(&self, metrics: EditorLayoutMetrics) -> impl ExactSizeIterator<Item = f32> + '_ {
        self.range(0..self.codes.len(), metrics)
    }

    fn range(
        &self,
        range: std::ops::Range<usize>,
        metrics: EditorLayoutMetrics,
    ) -> impl ExactSizeIterator<Item = f32> + '_ {
        let start = range.start;
        let mut run = self.sizes.partition_point(|run| run.end <= start);
        self.codes[range]
            .iter()
            .enumerate()
            .map(move |(offset, code)| {
                let index = start + offset;
                while self.sizes.get(run).is_some_and(|run| run.end <= index) {
                    run += 1;
                }
                let base = self
                    .sizes
                    .get(run)
                    .filter(|run| run.start <= index)
                    .map_or(self.base, |run| run.base);
                Self::width(*code, base, metrics)
            })
    }
}

/// The one geometry object used for block drawing, hit testing, carets, and selections.
#[derive(Debug, Clone, PartialEq)]
pub struct BlockLayoutGeometry {
    block: BlockId,
    scalars: Arc<[EditorScalarGeometry]>,
    carets: Arc<[(DocumentPosition, EditorRectangle)]>,
    block_kinds: Arc<[(DocumentPosition, DocumentPosition, SemanticBlockKind)]>,
    links: Arc<[(EditorSelection, String)]>,
    document_range: EditorSelection,
    content_height: f32,
    height_index: Arc<Vec<LineHeightEntry>>,
    layout_signature: u64,
    viewport_width: f32,
    pixel_scroll_y: f32,
    metrics: EditorLayoutMetrics,
    work: LayoutWork,
}

impl BlockLayoutGeometry {
    pub(crate) fn build(
        input: &VisibleEditorBlock,
        viewport: EditorViewport,
        pixel_scroll_y: f32,
        metrics: EditorLayoutMetrics,
        previous: Option<&Self>,
    ) -> Result<Self, &'static str> {
        metrics.validate()?;
        if !pixel_scroll_y.is_finite() || pixel_scroll_y < 0.0 {
            return Err("pixel scroll must be nonnegative and finite");
        }
        let compatible = previous.filter(|geometry| {
            geometry.viewport_width == viewport.width && geometry.metrics == metrics
        });
        let height_index = compatible
            .filter(|geometry| {
                geometry.layout_signature == input.layout_signature
                    && geometry.document_range.start() == input.document_start
            })
            .map_or_else(
                || Arc::new(build_height_index(input, viewport, metrics, compatible)),
                |geometry| Arc::clone(&geometry.height_index),
            );
        let content_height = height_index
            .last()
            .map_or(metrics.inset_y * 2.0 + metrics.line_height, |line| {
                line.end_y + metrics.inset_y
            });
        let overscan_top = (pixel_scroll_y - viewport.height).max(0.0);
        let overscan_bottom = pixel_scroll_y + viewport.height * 2.0;
        let first_line = height_index.partition_point(|line| line.end_y < overscan_top);
        // Start near the last viewport's size instead of repeatedly growing
        // and copying these buffers on every edit.
        let mut scalars =
            Vec::with_capacity(previous.map_or(0, |p| p.scalars.len().next_power_of_two()));
        let mut carets =
            Vec::with_capacity(previous.map_or(0, |p| p.carets.len().next_power_of_two()));
        let mut work = LayoutWork::default();
        for entry in height_index.iter().skip(first_line) {
            if entry.start_y > overscan_bottom {
                break;
            }
            let line = &input.layout_lines[entry.line_index];
            work.materialized_lines += 1;
            let span = line.span_index.map(|index| &input.block_spans[index]);
            let first_chunk = entry.chunk_rows.partition_point(|(_, end_row)| {
                entry.start_y + entry.row_top(end_row + 1) < overscan_top
            });
            for (chunk_index, chunk) in line.shape.chunks.iter().enumerate().skip(first_chunk) {
                let (start_row, _) = entry.chunk_rows[chunk_index];
                let chunk_top = entry.start_y + entry.row_top(start_row);
                if chunk_top > overscan_bottom {
                    break;
                }
                work.materialized_chunks += 1;
                work.materialized_scalars += materialize_chunk(
                    input,
                    chunk,
                    span,
                    entry,
                    overscan_top,
                    overscan_bottom,
                    pixel_scroll_y,
                    metrics,
                    &mut scalars,
                    &mut carets,
                )?;
            }
            if line.shape.chunks.is_empty() && line_intersects(entry, overscan_top, overscan_bottom)
            {
                replace_or_push_caret(
                    &mut carets,
                    line.start,
                    caret_rectangle(
                        entry.first_x,
                        entry.start_y - pixel_scroll_y,
                        EditorLayoutMetrics {
                            line_height: entry.row_height(0),
                            ..metrics
                        },
                    ),
                );
            }
            if let Some(position) = line.hard_break {
                let (row, x) = cursor_after_prefix(line.shape.scalar_len, entry, metrics);
                let global_y = entry.start_y + entry.row_top(row);
                if global_y + entry.row_height(row) >= overscan_top && global_y <= overscan_bottom {
                    let y = global_y - pixel_scroll_y;
                    scalars.push(scalar_geometry(
                        input,
                        None,
                        position,
                        '\n',
                        x,
                        y,
                        0.0,
                        metrics,
                        input.mark_ranges.iter(),
                    ));
                    replace_or_push_caret(&mut carets, position, caret_rectangle(x, y, metrics));
                    let next = position
                        .value()
                        .checked_add(1)
                        .ok_or("document position overflowed")?;
                    replace_or_push_caret(
                        &mut carets,
                        DocumentPosition::from(next),
                        caret_rectangle(metrics.inset_x, entry.end_y - pixel_scroll_y, metrics),
                    );
                }
            }
        }
        let first = input.document_start;
        let document_end = input
            .document_start
            .value()
            .checked_add(input.scalar_len)
            .ok_or("document position overflowed")?;

        Ok(Self {
            block: input.block,
            links: input
                .mark_ranges
                .iter()
                .filter_map(|mark| match &mark.mark {
                    SemanticInlineMark::Link(url) => Some((mark.range, url.clone())),
                    _ => None,
                })
                .collect(),
            scalars: scalars.into(),
            carets: carets.into(),
            block_kinds: input
                .block_spans
                .iter()
                .map(|span| (span.start, span.end, span.kind))
                .collect(),
            document_range: EditorSelection::new(first, DocumentPosition::from(document_end)),
            content_height,
            height_index,
            layout_signature: input.layout_signature,
            viewport_width: viewport.width,
            pixel_scroll_y,
            metrics,
            work,
        })
    }

    pub const fn block(&self) -> BlockId {
        self.block
    }

    pub fn draw_scalars(&self) -> &[EditorScalarGeometry] {
        &self.scalars
    }

    pub(crate) fn shared_draw_scalars(&self) -> Arc<[EditorScalarGeometry]> {
        Arc::clone(&self.scalars)
    }

    pub const fn layout_work(&self) -> LayoutWork {
        self.work
    }

    pub(crate) fn link_at(&self, x: f32, y: f32) -> Option<&str> {
        if self.links.is_empty() {
            return None;
        }
        let scalar = self.scalars.iter().find(|scalar| {
            scalar.link
                && x >= scalar.bounds.x
                && x < scalar.bounds.x + scalar.bounds.width
                && y >= scalar.bounds.y
                && y < scalar.bounds.y + scalar.bounds.height
        })?;
        self.links.iter().find_map(|(range, url)| {
            (range.start() <= scalar.position && scalar.position < range.end())
                .then_some(url.as_str())
        })
    }

    pub fn hit_test(&self, x: f32, y: f32) -> Option<DocumentPosition> {
        self.hit_test_caret(x, y).map(|(position, _)| position)
    }

    pub(crate) fn hit_test_caret(
        &self,
        x: f32,
        y: f32,
    ) -> Option<(DocumentPosition, EditorRectangle)> {
        if !x.is_finite() || !y.is_finite() {
            return None;
        }
        self.carets
            .iter()
            .copied()
            .chain(
                self.scalars
                    .iter()
                    .filter(|scalar| scalar.character != '\n')
                    .map(|scalar| {
                        (
                            (scalar.position.value() + 1).into(),
                            EditorRectangle {
                                x: scalar.bounds.x + scalar.bounds.width,
                                width: self.metrics.caret_width,
                                ..scalar.bounds
                            },
                        )
                    }),
            )
            .min_by(|(_, left), (_, right)| {
                vertical_distance(*left, y)
                    .total_cmp(&vertical_distance(*right, y))
                    .then_with(|| (left.x - x).abs().total_cmp(&(right.x - x).abs()))
            })
    }

    pub(crate) fn caret_with_affinity(
        &self,
        position: DocumentPosition,
        upstream: bool,
    ) -> Option<EditorRectangle> {
        let caret = self.caret(position)?;
        if upstream
            && let Some(scalar) = self.scalars.iter().find(|scalar| {
                scalar.position.value() + 1 == position.value()
                    && scalar.character != '\n'
                    && scalar.bounds.y < caret.y
            })
        {
            return Some(EditorRectangle {
                x: scalar.bounds.x + scalar.bounds.width,
                width: self.metrics.caret_width,
                ..scalar.bounds
            });
        }
        Some(caret)
    }

    pub fn caret(&self, position: DocumentPosition) -> Option<EditorRectangle> {
        self.carets
            .binary_search_by_key(&position, |(candidate, _)| *candidate)
            .ok()
            .map(|index| self.carets[index].1)
            .or_else(|| {
                let index = self
                    .height_index
                    .partition_point(|line| line.start <= position)
                    .checked_sub(1)?;
                let line = &self.height_index[index];
                if position > line.end {
                    return None;
                }
                let offset = position.value().saturating_sub(line.start.value()) as usize;
                let (row, x) = cursor_after_prefix(offset, line, self.metrics);
                Some(caret_rectangle(
                    x,
                    line.start_y + line.row_top(row) - self.pixel_scroll_y,
                    EditorLayoutMetrics {
                        line_height: line.row_height(row),
                        ..self.metrics
                    },
                ))
            })
    }

    pub const fn document_range(&self) -> EditorSelection {
        self.document_range
    }

    /// Returns the visible word containing a caret position. This deliberately
    /// uses scalar positions instead of byte offsets so double-click selection
    /// remains correct for Unicode prose.
    pub fn word_selection_at(&self, position: DocumentPosition) -> Option<EditorSelection> {
        let index = self
            .scalars
            .iter()
            .position(|scalar| scalar.position == position)
            .or_else(|| {
                position.value().checked_sub(1).and_then(|previous| {
                    self.scalars
                        .iter()
                        .position(|scalar| scalar.position.value() == previous)
                })
            })?;
        if !word_scalar(self.scalars[index].character) {
            return None;
        }
        let mut start = index;
        while start > 0
            && self.scalars[start - 1].position.value() + 1 == self.scalars[start].position.value()
            && word_scalar(self.scalars[start - 1].character)
        {
            start -= 1;
        }
        let mut end = index + 1;
        while end < self.scalars.len()
            && self.scalars[end - 1].position.value() + 1 == self.scalars[end].position.value()
            && word_scalar(self.scalars[end].character)
        {
            end += 1;
        }
        Some(EditorSelection::new(
            self.scalars[start].position,
            DocumentPosition::from(self.scalars[end - 1].position.value() + 1),
        ))
    }

    /// Returns the semantic block at a caret position for standard
    /// triple-click paragraph selection.
    pub fn paragraph_selection_at(&self, position: DocumentPosition) -> Option<EditorSelection> {
        self.block_kinds
            .iter()
            .find(|(start, end, _)| *start <= position && position <= *end)
            .map(|(start, end, _)| EditorSelection::new(*start, *end))
            .or_else(|| {
                (self.document_range.start() <= position && position <= self.document_range.end())
                    .then_some(self.document_range)
            })
    }

    pub(crate) fn block_kind_at(&self, position: DocumentPosition) -> Option<SemanticBlockKind> {
        self.block_kinds
            .iter()
            .filter(|(start, end, _)| *start <= position && position <= *end)
            .max_by_key(|(start, _, _)| *start)
            .map(|(_, _, kind)| *kind)
    }

    pub fn max_scroll_y(&self, viewport: EditorViewport) -> f32 {
        (self.content_height - viewport.height).max(0.0)
    }

    pub(crate) fn previous_caret(&self, position: DocumentPosition) -> Option<DocumentPosition> {
        (position > self.document_range.start())
            .then(|| DocumentPosition::from(position.value().saturating_sub(1)))
    }

    pub(crate) fn next_caret(&self, position: DocumentPosition) -> Option<DocumentPosition> {
        (position < self.document_range.end())
            .then(|| DocumentPosition::from(position.value().saturating_add(1)))
    }

    pub(crate) fn caret_above(&self, position: DocumentPosition) -> Option<DocumentPosition> {
        self.vertical_caret(position, false)
    }

    pub(crate) fn caret_below(&self, position: DocumentPosition) -> Option<DocumentPosition> {
        self.vertical_caret(position, true)
    }

    pub(crate) fn line_start(&self, position: DocumentPosition) -> Option<DocumentPosition> {
        self.line_edge(position, false)
    }

    pub(crate) fn line_end(&self, position: DocumentPosition) -> Option<DocumentPosition> {
        self.line_edge(position, true)
    }

    pub fn selection_rectangles(&self, selection: EditorSelection) -> Vec<EditorRectangle> {
        let start = selection.start().value();
        let end = selection.end().value();
        self.scalars
            .iter()
            .filter(|scalar| {
                scalar.character != '\n'
                    && scalar.position.value() >= start
                    && scalar.position.value() < end
            })
            .map(|scalar| scalar.bounds)
            .collect()
    }

    fn vertical_caret(&self, position: DocumentPosition, below: bool) -> Option<DocumentPosition> {
        let current = self.caret(position)?;
        let target_y = self
            .carets
            .iter()
            .map(|(_, rectangle)| rectangle.y)
            .filter(|candidate| {
                if below {
                    *candidate > current.y + f32::EPSILON
                } else {
                    *candidate < current.y - f32::EPSILON
                }
            })
            .min_by(|left, right| {
                let left_distance = (*left - current.y).abs();
                let right_distance = (*right - current.y).abs();
                left_distance.total_cmp(&right_distance)
            })?;
        self.carets
            .iter()
            .filter(|(_, rectangle)| (rectangle.y - target_y).abs() <= f32::EPSILON)
            .min_by(|(_, left), (_, right)| {
                (left.x - current.x)
                    .abs()
                    .total_cmp(&(right.x - current.x).abs())
            })
            .map(|(candidate, _)| *candidate)
    }

    fn line_edge(&self, position: DocumentPosition, end: bool) -> Option<DocumentPosition> {
        let current = self.caret(position)?;
        let candidates = self
            .carets
            .iter()
            .filter(|(_, rectangle)| (rectangle.y - current.y).abs() <= f32::EPSILON);
        if end {
            candidates
                .max_by(|(_, left), (_, right)| left.x.total_cmp(&right.x))
                .map(|(candidate, _)| *candidate)
        } else {
            candidates
                .min_by(|(_, left), (_, right)| left.x.total_cmp(&right.x))
                .map(|(candidate, _)| *candidate)
        }
    }
}

fn word_scalar(character: char) -> bool {
    character.is_alphanumeric() || character == '_'
}

fn build_height_index(
    input: &VisibleEditorBlock,
    viewport: EditorViewport,
    metrics: EditorLayoutMetrics,
    previous: Option<&BlockLayoutGeometry>,
) -> Vec<LineHeightEntry> {
    let signatures = input
        .layout_lines
        .iter()
        .map(|line| line_layout_signature(input, line))
        .collect::<Vec<_>>();
    let old = previous.map_or(&[][..], |geometry| geometry.height_index.as_slice());
    let suffix = signatures
        .iter()
        .rev()
        .zip(old.iter().rev())
        .take_while(|(signature, entry)| **signature == entry.signature)
        .count();
    let mut height_index = Vec::with_capacity(input.layout_lines.len());
    let mut y = metrics.inset_y;
    for (line_index, line) in input.layout_lines.iter().enumerate() {
        let span = line.span_index.map(|index| &input.block_spans[index]);
        if span.is_some_and(|span| span.start == line.start) {
            y += points_to_pixels(
                span.and_then(|span| span.style.space_before_points)
                    .unwrap_or(0.0),
            );
        }
        // Positions and vertical offsets may move, but unchanged line metrics
        // remain valid. Match the suffix too, so splits/joins can reuse it.
        let reusable = old
            .get(line_index)
            .filter(|entry| entry.signature == signatures[line_index])
            .or_else(|| {
                (line_index >= signatures.len() - suffix)
                    .then(|| &old[old.len() - (signatures.len() - line_index)])
            });
        let mut entry = reusable.cloned().unwrap_or_else(|| {
            build_line_height(input, line, span, viewport, metrics, signatures[line_index])
        });
        entry.line_index = line_index;
        entry.start = line.start;
        entry.end = line.end;
        entry.start_y = y;
        let rows = entry
            .prefix_cursors
            .last()
            .expect("line end cursor")
            .row
            .saturating_add(1);
        y += entry.row_top(rows);
        if line.hard_break.is_some() && span.is_some_and(|span| span.end == line.end) {
            y += points_to_pixels(
                span.and_then(|span| span.style.space_after_points)
                    .unwrap_or(0.0),
            );
        }
        entry.end_y = y;
        height_index.push(entry);
    }
    height_index
}

fn build_line_height(
    input: &VisibleEditorBlock,
    line: &VisibleLayoutLine,
    span: Option<&VisibleBlockSpan>,
    viewport: EditorViewport,
    metrics: EditorLayoutMetrics,
    signature: u64,
) -> LineHeightEntry {
    let characters = input.text.line(line).chars().collect::<Vec<_>>();
    let mut scalar_advances = ScalarAdvances::new(&characters, span, metrics);
    if let Some(span) = span {
        scalar_advances.apply_fonts(
            &characters,
            &input.mark_ranges[span.marks.clone()],
            line.start.value(),
            metrics,
        );
    }
    let mut first_x = metrics.inset_x + span.map_or(0.0, |span| block_indent(span, metrics));
    let continuation_x = first_x
        - span.map_or(0.0, |span| {
            points_to_pixels(span.style.first_line_indent_points.unwrap_or(0.0))
        });
    let right_indent = span
        .and_then(|span| span.style.right_indent_points)
        .map(points_to_pixels)
        .unwrap_or(0.0);
    let line_height = span
        .and_then(|span| span.style.line_spacing)
        .map_or(metrics.line_height, |spacing| {
            metrics.line_height * spacing.max(0.1)
        });
    let right_edge = (viewport.width - metrics.inset_x - right_indent)
        .max(metrics.inset_x + metrics.scalar_width);
    let wrap_before = word_wrap_offsets(
        &characters,
        &scalar_advances,
        first_x,
        continuation_x,
        right_edge,
        metrics,
    );
    let alignment = span
        .and_then(|span| span.style.alignment)
        .unwrap_or(TextAlignment::Start);
    let mut row_origins = Vec::new();
    let mut justified_spaces = BTreeMap::new();
    if alignment != TextAlignment::Start {
        let mut start = 0;
        for (row, end) in wrap_before
            .iter()
            .copied()
            .chain(std::iter::once(characters.len()))
            .enumerate()
        {
            let left = if row == 0 { first_x } else { continuation_x };
            let trimmed_end = characters[start..end]
                .iter()
                .rposition(|c| !c.is_whitespace())
                .map_or(start, |index| start + index + 1);
            let width = scalar_advances
                .range(start..trimmed_end, metrics)
                .sum::<f32>();
            let remaining = (right_edge - left - width).max(0.0);
            row_origins.push(
                left + match alignment {
                    TextAlignment::Center => remaining * 0.5,
                    TextAlignment::End => remaining,
                    _ => 0.0,
                },
            );
            if alignment == TextAlignment::Justify && row < wrap_before.len() {
                let spaces = (start..trimmed_end)
                    .filter(|&index| characters[index] == ' ')
                    .collect::<Vec<_>>();
                if !spaces.is_empty() {
                    let extra = remaining / spaces.len() as f32;
                    justified_spaces.extend(spaces.into_iter().map(|index| (index, extra)));
                }
            }
            start = end;
        }
        first_x = row_origins[0];
    }
    let rows = if scalar_advances.sizes.is_empty() {
        Box::default()
    } else {
        let mut rows = Vec::with_capacity(wrap_before.len() + 1);
        let default_size = scalar_advances.base / metrics.scalar_width * 20.0;
        let mut start = 0;
        let mut top = 0.0;
        for end in wrap_before
            .iter()
            .copied()
            .chain(std::iter::once(characters.len()))
        {
            let font_size = (start..end)
                .map(|index| scalar_advances.base_at(index) / metrics.scalar_width * 20.0)
                .reduce(f32::max)
                .unwrap_or(default_size);
            let height = (line_height * font_size / default_size).max(font_size * 1.2);
            rows.push(RowMetrics {
                top,
                height,
                font_size,
            });
            top += height;
            start = end;
        }
        rows.into_boxed_slice()
    };
    let mut prefix_cursors = Vec::with_capacity(line.shape.chunks.len().saturating_add(1));
    let mut chunk_rows = Vec::with_capacity(line.shape.chunks.len());
    let mut cursor = PrefixCursor {
        scalar_offset: 0,
        row: 0,
        x: first_x,
    };
    let mut wrap_index = 0_usize;
    for chunk in &line.shape.chunks {
        prefix_cursors.push(cursor);
        let start = cursor.row;
        let end_offset = chunk.scalar_offset.saturating_add(chunk.scalar_len);
        for (offset, advance) in scalar_advances
            .range(chunk.scalar_offset..end_offset, metrics)
            .enumerate()
        {
            let scalar_offset = chunk.scalar_offset + offset;
            if wrap_before.get(wrap_index) == Some(&scalar_offset) {
                cursor.row = cursor.row.saturating_add(1);
                cursor.x = row_origins
                    .get(cursor.row)
                    .copied()
                    .unwrap_or(continuation_x);
                wrap_index += 1;
            }
            cursor.x += advance + justified_spaces.get(&scalar_offset).copied().unwrap_or(0.0);
        }
        cursor.scalar_offset = end_offset;
        chunk_rows.push((start, cursor.row));
    }
    prefix_cursors.push(cursor);
    LineHeightEntry {
        signature,
        line_index: 0,
        start: line.start,
        end: line.end,
        start_y: 0.0,
        end_y: 0.0,
        metrics: Arc::new(LineMetrics {
            scalar_len: line.shape.scalar_len,
            scalar_advances,
            wrap_before: wrap_before.into(),
            prefix_cursors: prefix_cursors.into(),
            line_height,
            rows,
            first_x,
            continuation_x,
            row_origins: row_origins.into_boxed_slice(),
            justified_spaces,
            chunk_rows: chunk_rows.into(),
        }),
    }
}

fn cursor_after_prefix(
    scalar_count: usize,
    entry: &LineHeightEntry,
    metrics: EditorLayoutMetrics,
) -> (usize, f32) {
    let scalar_count = scalar_count.min(entry.scalar_len);
    let checkpoint_index = entry
        .prefix_cursors
        .partition_point(|cursor| cursor.scalar_offset <= scalar_count)
        .saturating_sub(1);
    let checkpoint = entry.prefix_cursors[checkpoint_index];
    let mut row = checkpoint.row;
    let mut x = checkpoint.x;
    let mut wrap_index = entry
        .wrap_before
        .partition_point(|offset| *offset < checkpoint.scalar_offset);
    for scalar_offset in checkpoint.scalar_offset..scalar_count {
        apply_wrap_before(
            scalar_offset,
            entry,
            &mut wrap_index,
            &mut row,
            &mut x,
            metrics,
        );
        x += entry
            .advance(scalar_offset, metrics)
            .expect("prefix advance");
    }
    (row, x)
}

fn apply_wrap_before(
    scalar_offset: usize,
    entry: &LineHeightEntry,
    wrap_index: &mut usize,
    row: &mut usize,
    x: &mut f32,
    _metrics: EditorLayoutMetrics,
) {
    if entry.wrap_before.get(*wrap_index) == Some(&scalar_offset) {
        *row = row.saturating_add(1);
        *x = entry.row_x(*row);
        *wrap_index += 1;
    }
}

fn word_wrap_offsets(
    characters: &[char],
    advances: &ScalarAdvances,
    first_x: f32,
    continuation_x: f32,
    right_edge: f32,
    metrics: EditorLayoutMetrics,
) -> Vec<usize> {
    let mut wraps = Vec::new();
    let mut offset = 0_usize;
    let mut x = first_x;
    while offset < characters.len() {
        if characters[offset].is_whitespace() {
            let advance = advances.get(offset, metrics).expect("whitespace advance");
            if x > continuation_x && x + advance > right_edge {
                wraps.push(offset);
                x = continuation_x;
            }
            x += advance;
            offset += 1;
            continue;
        }

        let word_start = offset;
        while offset < characters.len() && !characters[offset].is_whitespace() {
            offset += 1;
        }
        let word_width = advances.range(word_start..offset, metrics).sum::<f32>();
        if x > continuation_x && x + word_width > right_edge {
            wraps.push(word_start);
            x = continuation_x;
        }
        for (word_offset, advance) in advances.range(word_start..offset, metrics).enumerate() {
            let scalar_offset = word_start + word_offset;
            // Only an individual token wider than a row falls back to scalar
            // breaking. Ordinary words always move as a complete run.
            if x > continuation_x && x + advance > right_edge {
                wraps.push(scalar_offset);
                x = continuation_x;
            }
            x += advance;
        }
    }
    wraps
}

#[allow(clippy::too_many_arguments)]
fn materialize_chunk(
    input: &VisibleEditorBlock,
    chunk: &VisibleLayoutChunk,
    span: Option<&VisibleBlockSpan>,
    entry: &LineHeightEntry,
    overscan_top: f32,
    overscan_bottom: f32,
    pixel_scroll_y: f32,
    metrics: EditorLayoutMetrics,
    scalars: &mut Vec<EditorScalarGeometry>,
    carets: &mut Vec<(DocumentPosition, EditorRectangle)>,
) -> Result<usize, &'static str> {
    let (mut row, mut x) = cursor_after_prefix(chunk.scalar_offset, entry, metrics);
    let mut wrap_index = entry
        .wrap_before
        .partition_point(|offset| *offset < chunk.scalar_offset);
    let mut materialized = 0_usize;
    let line = &input.layout_lines[entry.line_index];
    let text = input.text.line(line);
    let start = entry
        .start
        .value()
        .checked_add(chunk.scalar_offset as u64)
        .ok_or("document position overflowed")?;
    let end = start
        .checked_add(chunk.scalar_len as u64)
        .ok_or("document position overflowed")?;
    // Offscreen formatting must not be scanned again for every visible scalar.
    // Keep overlapping ranges in their original order, including duplicates.
    let marks: Vec<_> = input
        .mark_ranges
        .iter()
        .filter(|mark| mark.range.start().value() < end && start < mark.range.end().value())
        .collect();
    for (offset, character) in text[chunk.text_range.clone()].chars().enumerate() {
        let position = start
            .checked_add(offset as u64)
            .ok_or("document position overflowed")?;
        let width = entry
            .advance(chunk.scalar_offset.saturating_add(offset), metrics)
            .ok_or("layout chunk advance is missing")?;
        let scalar_offset = chunk.scalar_offset.saturating_add(offset);
        apply_wrap_before(
            scalar_offset,
            entry,
            &mut wrap_index,
            &mut row,
            &mut x,
            metrics,
        );
        let global_y = entry.start_y + entry.row_top(row);
        let y = global_y - pixel_scroll_y;
        let row_height = entry.row_height(row);
        let row_metrics = EditorLayoutMetrics {
            line_height: row_height,
            ..metrics
        };
        let visible = global_y + row_height >= overscan_top && global_y <= overscan_bottom;
        if visible {
            materialized += 1;
            replace_or_push_caret(
                carets,
                DocumentPosition::from(position),
                caret_rectangle(x, y, row_metrics),
            );
            let mut scalar = scalar_geometry(
                input,
                span,
                DocumentPosition::from(position),
                character,
                x,
                y,
                width,
                metrics,
                marks.iter().copied(),
            );
            scalar.bounds.height = row_height;
            if let Some(row) = entry.rows.get(row) {
                scalar.text_offset_y = (row.font_size - scalar.font_size).max(0.0) * 0.9;
            }
            scalars.push(scalar);
        }
        x += width;
        if visible {
            replace_or_push_caret(
                carets,
                DocumentPosition::from(
                    position
                        .checked_add(1)
                        .ok_or("document position overflowed")?,
                ),
                caret_rectangle(x, y, row_metrics),
            );
        }
    }
    Ok(materialized)
}

#[allow(clippy::too_many_arguments)]
fn scalar_geometry<'a>(
    input: &VisibleEditorBlock,
    span: Option<&VisibleBlockSpan>,
    position: DocumentPosition,
    character: char,
    x: f32,
    y: f32,
    width: f32,
    metrics: EditorLayoutMetrics,
    marks: impl IntoIterator<Item = &'a VisibleMarkRange>,
) -> EditorScalarGeometry {
    let offset = position.value();
    let mut scalar = EditorScalarGeometry {
        position,
        character,
        bounds: EditorRectangle {
            x,
            y,
            width,
            height: span
                .and_then(|span| span.style.line_spacing)
                .map_or(metrics.line_height, |spacing| {
                    metrics.line_height * spacing.max(0.1)
                }),
        },
        bold: false,
        italic: false,
        underline: span
            .and_then(|span| span.style.text_decoration)
            .is_some_and(|value| value.underline()),
        strikethrough: span
            .and_then(|span| span.style.text_decoration)
            .is_some_and(|value| value.strikethrough()),
        link: false,
        small_caps: false,
        superscript: false,
        subscript: false,
        block_kind: span.map_or(SemanticBlockKind::Paragraph, |span| span.kind),
        list_depth: span.map_or(0, |span| span.list_depth),
        list_marker: span.and_then(|span| {
            (span.start == position).then_some(match span.kind {
                SemanticBlockKind::UnorderedListItem => 0,
                SemanticBlockKind::OrderedListItem => span.list_ordinal,
                _ => return None,
            })
        }),
        block_start: span.is_some_and(|span| span.start == position),
        font_size: span
            .and_then(|span| span.style.font_size_points)
            .map(points_to_pixels)
            .unwrap_or_else(|| default_font_size(span.map(|span| span.kind))),
        font_weight: span
            .and_then(|span| span.style.weight)
            .unwrap_or_else(|| default_font_weight(span.map(|span| span.kind))),
        text_offset_y: 0.0,
        block_italic: span.and_then(|span| span.style.italic).unwrap_or(false),
        font_family: span
            .and_then(|span| span.style.font_family)
            .unwrap_or(EditorFontFamily::Serif),
        atomic: input
            .atomic_nodes
            .iter()
            .find_map(|(candidate, kind)| (*candidate == position).then_some(*kind)),
    };
    for range in marks {
        if range.range.start().value() <= offset && offset < range.range.end().value() {
            match range.mark {
                SemanticInlineMark::Bold => scalar.bold = true,
                SemanticInlineMark::Italic => scalar.italic = true,
                SemanticInlineMark::Underline => scalar.underline = true,
                SemanticInlineMark::Strikethrough => scalar.strikethrough = true,
                SemanticInlineMark::Link(_) => scalar.link = true,
                SemanticInlineMark::SmallCaps => scalar.small_caps = true,
                SemanticInlineMark::Superscript => scalar.superscript = true,
                SemanticInlineMark::Subscript => scalar.subscript = true,
                SemanticInlineMark::FontFamily(family) => scalar.font_family = family.into(),
                SemanticInlineMark::FontSize(size) => {
                    scalar.font_size = points_to_pixels(f32::from(size))
                }
            }
        }
    }
    scalar
}

fn line_intersects(entry: &LineHeightEntry, top: f32, bottom: f32) -> bool {
    entry.end_y >= top && entry.start_y <= bottom
}

fn block_indent(span: &VisibleBlockSpan, metrics: EditorLayoutMetrics) -> f32 {
    let explicit = points_to_pixels(span.style.left_indent_points.unwrap_or(0.0))
        + points_to_pixels(span.style.first_line_indent_points.unwrap_or(0.0));
    match span.kind {
        SemanticBlockKind::UnorderedListItem | SemanticBlockKind::OrderedListItem => {
            explicit + metrics.scalar_width * 3.0 * (span.list_depth.saturating_add(1) as f32)
        }
        // Quotes in the manuscript use their own prose rhythm, not the
        // inspector-style rule and indent that previously shifted them away
        // from the page edge.
        SemanticBlockKind::BlockQuote => explicit,
        _ => explicit,
    }
}

fn points_to_pixels(points: f32) -> f32 {
    points * (4.0 / 3.0)
}

fn editor_font_family(value: &str) -> EditorFontFamily {
    let value = value.to_ascii_lowercase();
    if value.contains("mono") || value.contains("courier") {
        EditorFontFamily::Monospace
    } else if value.contains("serif") && !value.contains("sans") {
        EditorFontFamily::Serif
    } else {
        EditorFontFamily::SansSerif
    }
}

fn default_font_size(kind: Option<SemanticBlockKind>) -> f32 {
    match kind.unwrap_or(SemanticBlockKind::Paragraph) {
        SemanticBlockKind::Heading1 => 24.0,
        SemanticBlockKind::Heading2 => 20.0,
        SemanticBlockKind::Heading3 => 18.0,
        _ => 20.0,
    }
}

fn default_font_weight(_kind: Option<SemanticBlockKind>) -> u16 {
    400
}

/// Encodes the bundled serif and sans proportions without rounding the model.
/// Drawing, caret placement, and wrapping all decode these same width codes.
fn scalar_width_code(character: char, family: EditorFontFamily) -> u8 {
    // The bundled proportional metrics are exact integer percentages. Codes
    // 0 and 1 distinguish tabs and monospace advances, which bypass clamping.
    if character == '\t' {
        return 0;
    }
    if family == EditorFontFamily::Monospace {
        return 1;
    }
    match character {
        ' ' => 52,
        '\u{2009}' | '\u{200A}' => 25,
        '\u{2002}' | '\u{2003}' => 90,
        'i' | 'j' | 'l' | 'I' | '!' | '|' => 57,
        'f' => 65,
        'r' => 87,
        't' | 'J' => 74,
        'a' => 113,
        'b' | 'd' | 'h' | 'n' | 'p' | 'q' | 'u' => 122,
        'c' => 97,
        'e' | 'v' => 107,
        'g' | 'o' => 118,
        'k' => 112,
        'm' => 183,
        's' => 88,
        'w' => 159,
        'x' => 106,
        'y' => 102,
        'z' => 90,
        'A' | 'V' | 'Y' => 145,
        'B' | 'E' | 'F' | 'P' | 'R' => 130,
        'C' | 'D' | 'G' | 'O' | 'Q' => 148,
        'H' | 'K' | 'N' | 'U' => 152,
        'L' => 117,
        'M' => 175,
        'S' | 'T' => 125,
        'W' => 205,
        'X' | 'Z' => 142,
        '0'..='9' => 111,
        '.' | ',' | ':' | ';' | '\'' | '"' | '`' => 52,
        '-' | '_' | '(' | ')' | '[' | ']' | '{' | '}' => 68,
        _ if character.is_ascii_punctuation() => 82,
        _ => 122,
    }
}

fn block_style_id(block: &parchmint_editor_api::SemanticBlock) -> StyleId {
    block
        .paragraph_style()
        .and_then(parse_style_id)
        .unwrap_or_else(|| match block.kind() {
            SemanticBlockKind::Heading1 => StyleCatalog::heading_1_id(),
            SemanticBlockKind::Heading2 => StyleCatalog::heading_2_id(),
            SemanticBlockKind::Heading3 => StyleCatalog::heading_3_id(),
            SemanticBlockKind::BlockQuote => StyleCatalog::block_quote_id(),
            _ => StyleCatalog::body_id(),
        })
}

fn resolve_block_style(style_id: StyleId, catalog: &StyleCatalog) -> ResolvedBlockStyle {
    let mut resolved = ResolvedBlockStyle::default();
    merge_style(&mut resolved, &catalog.resolved_properties(style_id));
    resolved
}

fn merge_style(target: &mut ResolvedBlockStyle, source: &StyleProperties) {
    macro_rules! replace {
        ($field:ident) => {
            if source.$field.is_some() {
                target.$field = source.$field.clone();
            }
        };
    }
    if let Some(family) = &source.font_family {
        target.font_family = Some(editor_font_family(family));
    }
    replace!(font_size_points);
    replace!(weight);
    replace!(italic);
    replace!(text_decoration);
    replace!(alignment);
    replace!(first_line_indent_points);
    replace!(left_indent_points);
    replace!(right_indent_points);
    replace!(line_spacing);
    replace!(space_before_points);
    replace!(space_after_points);
}

fn replace_or_push_caret(
    carets: &mut Vec<(DocumentPosition, EditorRectangle)>,
    position: DocumentPosition,
    rectangle: EditorRectangle,
) {
    // Lines, chunks, and scalars arrive in document order. Only the latest
    // caret can be repeated, at a scalar, wrap, chunk, or paragraph boundary.
    // Keep the following scalar's rectangle when a boundary is visited twice.
    if let Some((last, existing)) = carets.last_mut() {
        debug_assert!(*last <= position, "carets must arrive in document order");
        if *last == position {
            *existing = rectangle;
            return;
        }
    }
    carets.push((position, rectangle));
}

fn caret_rectangle(x: f32, y: f32, metrics: EditorLayoutMetrics) -> EditorRectangle {
    EditorRectangle {
        x,
        y,
        width: metrics.caret_width,
        height: metrics.line_height,
    }
}

// Resolve the visual row first; horizontal whitespace must never select a
// longer neighboring line. Equal distances at row edges prefer the lower row.
fn vertical_distance(rectangle: EditorRectangle, y: f32) -> f32 {
    if y < rectangle.y {
        rectangle.y - y
    } else if y >= rectangle.y + rectangle.height {
        y - rectangle.y - rectangle.height + f32::EPSILON
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use parchmint_editor_api::{
        SemanticBlock, SemanticDocument, SemanticMarkRange, StyleDefinition, StyleRole,
    };

    fn block(value: u8) -> BlockId {
        BlockId::from_bytes([value; 16])
    }

    fn regression_metrics() -> EditorLayoutMetrics {
        EditorLayoutMetrics {
            inset_x: 16.0,
            inset_y: 16.0,
            scalar_width: 8.0,
            line_height: 20.0,
            caret_width: 1.0,
        }
    }

    fn flattened_reference(input: &VisibleEditorBlock, text: String) -> VisibleEditorBlock {
        let mut reference = input.clone();
        reference.text = VisibleText::Plain(Arc::new(text));
        reference.layout_lines = build_layout_lines(
            &reference.text,
            reference.document_start,
            &reference.block_spans,
            None,
        );
        reference.layout_signature =
            layout_signature(&reference.layout_lines, &reference.block_spans);
        reference
    }

    fn assert_text_not_flattened(input: &VisibleEditorBlock) {
        let VisibleText::Semantic { flattened, .. } = &input.text else {
            panic!("expected paragraph-backed text");
        };
        assert!(
            flattened.get().is_none(),
            "rendering must not join the document"
        );
    }

    #[test]
    fn whitespace_hit_testing_stays_on_the_clicked_visual_row() {
        for text in [
            "A long opening line\nx\n\nlast",
            "A sentence that wraps across multiple short rows with héllo 🦀.",
        ] {
            let input = VisibleEditorBlock::new(block(42), text, 0.into());
            let geometry = BlockLayoutGeometry::build(
                &input,
                EditorViewport::new(180.0, 600.0).unwrap(),
                0.0,
                regression_metrics(),
                None,
            )
            .unwrap();
            for (_, caret) in geometry.carets.iter() {
                let row = caret.y;
                let expected = geometry
                    .scalars
                    .iter()
                    .filter(|scalar| scalar.bounds.y == row)
                    .map(|scalar| scalar.position.value() + u64::from(scalar.character != '\n'))
                    .chain(
                        geometry
                            .carets
                            .iter()
                            .filter(|(_, other)| other.y == row)
                            .map(|(position, _)| position.value()),
                    )
                    .max()
                    .unwrap();
                assert_eq!(
                    geometry.hit_test(500.0, row + caret.height * 0.5),
                    Some(expected.into()),
                    "row {row} in {text:?}"
                );
                let (position, hit) = geometry
                    .hit_test_caret(500.0, row + caret.height * 0.5)
                    .unwrap();
                assert_eq!(
                    geometry
                        .caret_with_affinity(position, hit.y < geometry.caret(position).unwrap().y)
                        .unwrap()
                        .y,
                    row
                );
            }
        }
    }

    #[test]
    fn geometry_snapshots_share_buffers_and_keep_previous_frames_immutable() {
        let initial = SemanticDocument::new(vec![SemanticBlock::new(
            block(1),
            SemanticBlockKind::Paragraph,
            None,
            "first\nsecond",
            vec![SemanticMarkRange::new(
                EditorSelection::new(0.into(), 5.into()),
                SemanticInlineMark::Link("https://example.com".into()),
            )],
        )]);
        let mut input = VisibleEditorBlock::from_semantic(block(1), &initial, 20.into());
        let viewport = EditorViewport::new(320.0, 240.0).unwrap();
        let first =
            BlockLayoutGeometry::build(&input, viewport, 0.0, regression_metrics(), None).unwrap();
        let captured = first.clone();
        assert!(Arc::ptr_eq(&first.scalars, &captured.scalars));
        assert!(Arc::ptr_eq(&first.carets, &captured.carets));
        assert!(Arc::ptr_eq(&first.block_kinds, &captured.block_kinds));
        assert!(Arc::ptr_eq(&first.links, &captured.links));
        let semantic = SemanticDocument::new(vec![SemanticBlock::new(
            block(1),
            SemanticBlockKind::Paragraph,
            None,
            "changed\n🦀\n",
            vec![],
        )]);
        input.update_semantic(&semantic, &StyleCatalogProjection::default());
        let next =
            BlockLayoutGeometry::build(&input, viewport, 16.0, regression_metrics(), Some(&first))
                .unwrap();
        assert_eq!(captured, first);
        assert_eq!(
            captured
                .draw_scalars()
                .iter()
                .map(|s| s.character)
                .collect::<String>(),
            "first\nsecond"
        );
        assert!(!Arc::ptr_eq(&next.scalars, &captured.scalars));
        assert_eq!(
            next,
            BlockLayoutGeometry::build(&input, viewport, 16.0, regression_metrics(), None).unwrap()
        );
    }

    #[test]
    fn render_text_and_line_shapes_share_unchanged_paragraphs_without_flattening() {
        let tail = SemanticBlock::from_shared_text(
            block(2),
            SemanticBlockKind::Paragraph,
            None,
            Arc::new(format!("{}\nlast", "é 🦀 tail ".repeat(250))),
            vec![],
        );
        let document = |head: &str| {
            SemanticDocument::new(vec![
                SemanticBlock::new(block(1), SemanticBlockKind::Paragraph, None, head, vec![]),
                tail.clone(),
            ])
        };
        let first = document("head");
        let mut input = VisibleEditorBlock::from_semantic(block(1), &first, 100.into());
        assert!(std::ptr::eq(
            input.text.segment(1),
            first.blocks()[1].text()
        ));
        let old = input.clone();
        let changed = document("changed\nhead");
        input.update_semantic(&changed, &StyleCatalogProjection::default());
        for (previous, current) in old.layout_lines[1..].iter().zip(&input.layout_lines[2..]) {
            assert!(Arc::ptr_eq(&previous.shape, &current.shape));
            assert!(current.start > previous.start);
        }
        let viewport = EditorViewport::new(320.0, 240.0).unwrap();
        for scroll in [0.0, 1_000.0] {
            let actual =
                BlockLayoutGeometry::build(&input, viewport, scroll, regression_metrics(), None)
                    .unwrap();
            let reference = flattened_reference(&input, changed.plain_text());
            let expected = BlockLayoutGeometry::build(
                &reference,
                viewport,
                scroll,
                regression_metrics(),
                None,
            )
            .unwrap();
            assert_eq!(actual, expected);
        }
        assert_text_not_flattened(&old);
        assert_text_not_flattened(&input);
        let unread = input.clone();
        assert_eq!(input.text(), changed.plain_text());
        assert_eq!(
            input, unread,
            "cache initialization does not change equality"
        );
        assert_text_not_flattened(&unread);
        assert_eq!(
            old.text(),
            first.plain_text(),
            "captured text remains immutable"
        );
        let cached_clone = input.clone();
        assert!(
            std::ptr::eq(input.text(), cached_clone.text()),
            "clones share the lazy joined text"
        );
        input.update_semantic(&first, &StyleCatalogProjection::default());
        assert_text_not_flattened(&input);
        assert_eq!(input.text(), first.plain_text());
        assert_eq!(cached_clone.text(), changed.plain_text());
    }

    #[test]
    fn render_text_equality_handles_alternate_segmentation_without_flattening() {
        let segmented = |parts: &[&str]| {
            VisibleEditorBlock::from_semantic(
                block(1),
                &SemanticDocument::new(
                    parts
                        .iter()
                        .enumerate()
                        .map(|(index, text)| {
                            SemanticBlock::new(
                                block(index as u8),
                                SemanticBlockKind::Paragraph,
                                None,
                                *text,
                                vec![],
                            )
                        })
                        .collect(),
                ),
                0.into(),
            )
        };
        for (left, right) in [
            (vec!["a\nb", "c"], vec!["a", "b\nc"]),
            (vec!["é\n\n🦀\n"], vec!["é", "", "🦀", ""]),
            (vec![], vec![""]),
            (vec!["", ""], vec!["\n"]),
        ] {
            let left = segmented(&left);
            let right = segmented(&right);
            assert_eq!(left.text, right.text);
            assert_ne!(left.text, VisibleText::Plain(Arc::new("different".into())));
            assert_text_not_flattened(&left);
            assert_text_not_flattened(&right);
            assert_eq!(left.text(), right.text());
        }
    }

    #[test]
    fn cached_style_signatures_invalidate_for_every_resolved_layout_property() {
        let semantic = SemanticDocument::new(vec![SemanticBlock::new(
            block(1),
            SemanticBlockKind::Paragraph,
            None,
            "words that wrap across rows\nand a soft break",
            vec![],
        )]);
        let styled = |properties| {
            let mut catalog = StyleCatalog::default();
            catalog
                .upsert(StyleDefinition {
                    id: StyleCatalog::body_id(),
                    display_name: "Body".into(),
                    role: StyleRole::Body,
                    inherits: None,
                    properties,
                })
                .unwrap();
            VisibleEditorBlock::from_semantic_with_styles(
                block(1),
                &semantic,
                0.into(),
                &StyleCatalogProjection::new(catalog),
            )
        };
        let before = styled(StyleProperties::default());
        let viewport = EditorViewport::new(320.0, 240.0).unwrap();
        let previous =
            BlockLayoutGeometry::build(&before, viewport, 0.0, regression_metrics(), None).unwrap();
        for properties in [
            StyleProperties {
                font_family: Some("Monospace".into()),
                ..StyleProperties::default()
            },
            StyleProperties {
                font_size_points: Some(18.5),
                ..StyleProperties::default()
            },
            StyleProperties {
                weight: Some(700),
                ..StyleProperties::default()
            },
            StyleProperties {
                italic: Some(true),
                ..StyleProperties::default()
            },
            StyleProperties {
                alignment: Some(TextAlignment::End),
                ..StyleProperties::default()
            },
            StyleProperties {
                first_line_indent_points: Some(9.0),
                ..StyleProperties::default()
            },
            StyleProperties {
                left_indent_points: Some(10.0),
                ..StyleProperties::default()
            },
            StyleProperties {
                right_indent_points: Some(11.0),
                ..StyleProperties::default()
            },
            StyleProperties {
                line_spacing: Some(1.5),
                ..StyleProperties::default()
            },
            StyleProperties {
                space_before_points: Some(8.0),
                ..StyleProperties::default()
            },
            StyleProperties {
                space_after_points: Some(7.0),
                ..StyleProperties::default()
            },
        ] {
            let changed = styled(properties.clone());
            assert_ne!(
                before.block_spans[0].style_signature, changed.block_spans[0].style_signature,
                "{properties:?}"
            );
            let incremental = BlockLayoutGeometry::build(
                &changed,
                viewport,
                0.0,
                regression_metrics(),
                Some(&previous),
            )
            .unwrap();
            let fresh =
                BlockLayoutGeometry::build(&changed, viewport, 0.0, regression_metrics(), None)
                    .unwrap();
            assert_eq!(incremental, fresh, "{properties:?}");
            assert!(!Arc::ptr_eq(
                &previous.height_index,
                &incremental.height_index
            ));
        }
    }

    #[test]
    fn compact_advances_match_the_original_f32_model_exactly() {
        let characters = (0..=255)
            .filter_map(char::from_u32)
            .chain("尾🦀é\u{2002}\u{2003}\u{2009}\u{200a}\u{200b}\u{301}\u{fffc}".chars())
            .collect::<Vec<_>>();
        for family in [
            None,
            Some(EditorFontFamily::Serif),
            Some(EditorFontFamily::SansSerif),
            Some(EditorFontFamily::Monospace),
        ] {
            for kind in [
                SemanticBlockKind::Paragraph,
                SemanticBlockKind::Heading1,
                SemanticBlockKind::Heading3,
            ] {
                for font_size_points in [None, Some(0.25), Some(9.375), Some(18.0), Some(72.5)] {
                    let span = VisibleBlockSpan {
                        start: 0.into(),
                        end: 0.into(),
                        kind,
                        list_depth: 0,
                        list_ordinal: 0,
                        style_signature: 0,
                        marks: 0..0,
                        font_signature: 0,
                        style: Arc::new(ResolvedBlockStyle {
                            font_family: family,
                            font_size_points,
                            ..ResolvedBlockStyle::default()
                        }),
                    };
                    for metrics in [
                        EditorLayoutMetrics::default(),
                        regression_metrics(),
                        EditorLayoutMetrics {
                            scalar_width: 1.3,
                            caret_width: 2.0,
                            ..regression_metrics()
                        },
                    ] {
                        for span in [None, Some(&span)] {
                            let compact = ScalarAdvances::new(&characters, span, metrics);
                            assert_eq!(compact.codes.len(), characters.len());
                            assert_eq!(compact.get(characters.len(), metrics), None);
                            for ((index, character), actual) in
                                characters.iter().enumerate().zip(compact.iter(metrics))
                            {
                                let expected = reference_scalar_advance(*character, span, metrics);
                                assert_eq!(
                                    actual.to_bits(),
                                    expected.to_bits(),
                                    "{character:?}, {family:?}, {kind:?}, {font_size_points:?}, {metrics:?}"
                                );
                                assert_eq!(compact.get(index, metrics), Some(expected));
                            }
                        }
                    }
                }
            }
        }
    }

    fn reference_scalar_advance(
        character: char,
        span: Option<&VisibleBlockSpan>,
        metrics: EditorLayoutMetrics,
    ) -> f32 {
        if character == '\t' {
            return metrics.scalar_width * TAB_COLUMNS;
        }

        let font_size = span
            .and_then(|span| span.style.font_size_points)
            .map(points_to_pixels)
            .unwrap_or_else(|| default_font_size(span.map(|span| span.kind)));
        let family = span
            .and_then(|span| span.style.font_family)
            .unwrap_or(EditorFontFamily::Serif);
        let base = metrics.scalar_width * (font_size / 20.0);
        if family == EditorFontFamily::Monospace {
            return base * (4.0 / 3.0);
        }

        // Source Serif 4's common Latin advances, normalized to the 9px body
        // advance at 20px. The same ratios remain a close deterministic model for
        // Source Sans 3; monospace uses a separate cell width above.
        let proportion = match character {
            ' ' => 0.52,
            '\u{2009}' | '\u{200A}' => 0.25,
            '\u{2002}' | '\u{2003}' => 0.9,
            'i' | 'j' | 'l' | 'I' | '!' | '|' => 0.57,
            'f' => 0.65,
            'r' => 0.87,
            't' | 'J' => 0.74,
            'a' => 1.13,
            'b' | 'd' | 'h' | 'n' | 'p' | 'q' | 'u' => 1.22,
            'c' => 0.97,
            'e' | 'v' => 1.07,
            'g' | 'o' => 1.18,
            'k' => 1.12,
            'm' => 1.83,
            's' => 0.88,
            'w' => 1.59,
            'x' => 1.06,
            'y' => 1.02,
            'z' => 0.9,
            'A' | 'V' | 'Y' => 1.45,
            'B' | 'E' | 'F' | 'P' | 'R' => 1.3,
            'C' | 'D' | 'G' | 'O' | 'Q' => 1.48,
            'H' | 'K' | 'N' | 'U' => 1.52,
            'L' => 1.17,
            'M' => 1.75,
            'S' | 'T' => 1.25,
            'W' => 2.05,
            'X' | 'Z' => 1.42,
            '0'..='9' => 1.11,
            '.' | ',' | ':' | ';' | '\'' | '"' | '`' => 0.52,
            '-' | '_' | '(' | ')' | '[' | ']' | '{' | '}' => 0.68,
            _ if character.is_ascii_punctuation() => 0.82,
            _ => 1.22,
        };
        (base * proportion).max(metrics.caret_width)
    }

    #[test]
    fn inline_fonts_reflow_only_changed_lines_and_keep_carets_in_mixed_size_rows() {
        let text =
            "Small words surround LARGE and continue across several wrapped rows. ".repeat(40);
        let make = |marks| {
            SemanticDocument::new(vec![
                SemanticBlock::new(
                    block(1),
                    SemanticBlockKind::Paragraph,
                    None,
                    text.clone(),
                    marks,
                ),
                SemanticBlock::new(
                    block(2),
                    SemanticBlockKind::Paragraph,
                    None,
                    "Unchanged tail",
                    vec![],
                ),
            ])
        };
        let mut input = VisibleEditorBlock::from_semantic(block(1), &make(vec![]), 0.into());
        let viewport = EditorViewport::new(480.0, 300.0).unwrap();
        let metrics = EditorLayoutMetrics::default();
        let before = BlockLayoutGeometry::build(&input, viewport, 0.0, metrics, None).unwrap();
        let marks = vec![
            SemanticMarkRange::new(
                EditorSelection::new(21.into(), 26.into()),
                SemanticInlineMark::FontSize(48),
            ),
            SemanticMarkRange::new(
                EditorSelection::new(21.into(), 26.into()),
                SemanticInlineMark::FontFamily(InlineFontFamily::Monospace),
            ),
            SemanticMarkRange::new(
                EditorSelection::new(1050.into(), 1300.into()),
                SemanticInlineMark::FontSize(9),
            ),
        ];
        input.update_semantic(&make(marks), &StyleCatalogProjection::default());
        let after =
            BlockLayoutGeometry::build(&input, viewport, 0.0, metrics, Some(&before)).unwrap();
        let full = BlockLayoutGeometry::build(&input, viewport, 0.0, metrics, None).unwrap();
        assert_eq!(after, full);
        assert!(!Arc::ptr_eq(
            &before.height_index[0].metrics,
            &after.height_index[0].metrics
        ));
        assert!(Arc::ptr_eq(
            &before.height_index[1].metrics,
            &after.height_index[1].metrics
        ));
        let large = after
            .scalars
            .iter()
            .find(|scalar| scalar.position == 21.into())
            .unwrap();
        assert_eq!(large.font_size, 64.0);
        assert_eq!(large.font_family, EditorFontFamily::Monospace);
        assert!(large.bounds.height >= large.font_size);
        assert!(
            after.height_index[0]
                .rows
                .iter()
                .any(|row| row.height == metrics.line_height)
        );
        for scalar in &*after.scalars {
            if scalar.character == '\n' {
                continue;
            }
            let caret = after.caret(scalar.position).unwrap();
            assert_eq!(caret.y, scalar.bounds.y);
            assert_eq!(caret.height, scalar.bounds.height);
            assert_eq!(
                after.hit_test(caret.x, caret.y + caret.height * 0.5),
                Some(scalar.position)
            );
        }
        let scroll = after.caret(1200.into()).unwrap().y;
        let scrolled =
            BlockLayoutGeometry::build(&input, viewport, scroll, metrics, Some(&after)).unwrap();
        assert_eq!(
            scrolled,
            BlockLayoutGeometry::build(&input, viewport, scroll, metrics, None).unwrap()
        );
        assert!(
            scrolled
                .scalars
                .iter()
                .any(|scalar| scalar.font_size == 12.0)
        );
    }

    #[test]
    fn ordered_caret_updates_match_last_write_wins_at_repeated_boundaries() {
        let mut carets = Vec::new();
        let mut expected = BTreeMap::new();
        for offset in 0..2_100_u64 {
            if offset % 11 == 0 {
                continue; // A viewport may omit ranges of positions.
            }
            let position = DocumentPosition::from(100 + offset);
            for visit in 0..1 + offset % 3 {
                let rectangle = caret_rectangle(offset as f32, visit as f32, regression_metrics());
                replace_or_push_caret(&mut carets, position, rectangle);
                expected.insert(position, rectangle);
            }
        }
        assert_eq!(carets, expected.into_iter().collect::<Vec<_>>());
    }

    #[test]
    fn caret_boundaries_follow_wrapped_scalars_and_empty_styled_paragraphs() {
        let semantic = SemanticDocument::new(vec![
            SemanticBlock::new(
                block(1),
                SemanticBlockKind::Heading1,
                None,
                "é 🦀 word\t".repeat(350),
                vec![],
            ),
            SemanticBlock::new(block(2), SemanticBlockKind::Paragraph, None, "\n", vec![]),
            SemanticBlock::new(block(3), SemanticBlockKind::SceneBreak, None, "", vec![]),
            SemanticBlock::new(
                block(4),
                SemanticBlockKind::OrderedListItem,
                None,
                "tail\nline",
                vec![],
            )
            .with_list_depth(2),
            SemanticBlock::new(
                block(5),
                SemanticBlockKind::OrderedListItem,
                None,
                "",
                vec![],
            )
            .with_list_depth(1),
        ]);
        let input = VisibleEditorBlock::from_semantic(block(1), &semantic, 100.into());
        let metrics = regression_metrics();
        for width in [96.0, 320.0, 1_200.0] {
            let viewport = EditorViewport::new(width, 240.0).unwrap();
            let initial = BlockLayoutGeometry::build(&input, viewport, 0.0, metrics, None).unwrap();
            // Include a UTF-8 chunk boundary and the final empty, indented item.
            let chunk_y = initial
                .caret((100 + LAYOUT_CHUNK_SCALARS as u64).into())
                .unwrap()
                .y;
            for scroll in [0.0, chunk_y, initial.max_scroll_y(viewport)] {
                let geometry =
                    BlockLayoutGeometry::build(&input, viewport, scroll, metrics, Some(&initial))
                        .unwrap();
                assert!(!geometry.carets.is_empty());
                assert!(geometry.carets.windows(2).all(|pair| pair[0].0 < pair[1].0));
                for scalar in geometry.draw_scalars() {
                    assert_eq!(
                        geometry.caret(scalar.position),
                        Some(caret_rectangle(
                            scalar.bounds.x,
                            scalar.bounds.y,
                            EditorLayoutMetrics {
                                line_height: scalar.bounds.height,
                                ..metrics
                            }
                        )),
                        "width {width}, scroll {scroll}, scalar {:?}",
                        scalar.position,
                    );
                }
                if scroll == initial.max_scroll_y(viewport) {
                    let last = geometry.height_index.last().unwrap();
                    assert!(last.first_x > metrics.inset_x);
                    assert_eq!(
                        geometry.caret(geometry.document_range.end()),
                        Some(caret_rectangle(
                            last.first_x,
                            last.start_y - scroll,
                            metrics
                        )),
                    );
                }
            }
        }
    }

    #[test]
    fn font_family_resolution_preserves_aliases_and_inheritance() {
        for (name, expected) in [
            ("Source Serif 4", EditorFontFamily::Serif),
            ("sErIf", EditorFontFamily::Serif),
            ("SANS-SERIF", EditorFontFamily::SansSerif),
            ("Arial", EditorFontFamily::SansSerif),
            ("CourIER New", EditorFontFamily::Monospace),
            ("Mono Serif", EditorFontFamily::Monospace),
            ("", EditorFontFamily::SansSerif),
            ("尾 SANS sérif", EditorFontFamily::SansSerif),
        ] {
            let mut style = ResolvedBlockStyle::default();
            merge_style(
                &mut style,
                &StyleProperties {
                    font_family: Some(name.into()),
                    ..StyleProperties::default()
                },
            );
            assert_eq!(style.font_family, Some(expected), "{name}");
            merge_style(&mut style, &StyleProperties::default());
            assert_eq!(style.font_family, Some(expected), "inherit {name}");
            merge_style(
                &mut style,
                &StyleProperties {
                    font_family: Some("Monospace".into()),
                    ..StyleProperties::default()
                },
            );
            assert_eq!(style.font_family, Some(EditorFontFamily::Monospace));
        }
    }

    #[test]
    fn explicit_catalog_style_controls_deterministic_geometry_and_text_style() {
        let mut catalog = StyleCatalog::default();
        catalog
            .upsert(StyleDefinition {
                id: StyleCatalog::body_id(),
                display_name: "Body".into(),
                role: StyleRole::Body,
                inherits: None,
                properties: StyleProperties {
                    font_family: Some("Monospace".into()),
                    font_size_points: Some(18.0),
                    weight: Some(700),
                    italic: Some(true),
                    alignment: Some(TextAlignment::Center),
                    left_indent_points: Some(9.0),
                    line_spacing: Some(1.5),
                    ..StyleProperties::default()
                },
            })
            .expect("replace reserved body properties");
        let semantic = SemanticDocument::new(vec![SemanticBlock::new(
            block(1),
            SemanticBlockKind::Paragraph,
            Some("body".into()),
            "abc",
            vec![
                SemanticMarkRange::new(
                    EditorSelection::new(0.into(), 1.into()),
                    SemanticInlineMark::SmallCaps,
                ),
                SemanticMarkRange::new(
                    EditorSelection::new(1.into(), 2.into()),
                    SemanticInlineMark::Superscript,
                ),
                SemanticMarkRange::new(
                    EditorSelection::new(2.into(), 3.into()),
                    SemanticInlineMark::Subscript,
                ),
            ],
        )]);
        let visible = VisibleEditorBlock::from_semantic_with_styles(
            block(1),
            &semantic,
            DocumentPosition::default(),
            &StyleCatalogProjection::new(catalog),
        );
        let geometry = BlockLayoutGeometry::build(
            &visible,
            EditorViewport::new(200.0, 100.0).expect("viewport"),
            0.0,
            regression_metrics(),
            None,
        )
        .expect("geometry");
        let first = &geometry.draw_scalars()[0];
        assert_eq!(first.font_size, 24.0);
        assert_eq!(first.font_weight, 700);
        assert!(first.block_italic);
        assert_eq!(first.font_family, EditorFontFamily::Monospace);
        assert_eq!(first.bounds.height, 30.0);
        assert!(first.bounds.x > regression_metrics().inset_x + 12.0);
        assert!(first.small_caps);
        assert!(geometry.draw_scalars()[1].superscript);
        assert!(geometry.draw_scalars()[2].subscript);
    }

    #[test]
    fn nested_lists_and_quote_styles_apply_their_indents() {
        let semantic = SemanticDocument::new(vec![
            SemanticBlock::new(
                block(1),
                SemanticBlockKind::UnorderedListItem,
                None,
                "one",
                Vec::new(),
            ),
            SemanticBlock::new(
                block(2),
                SemanticBlockKind::OrderedListItem,
                None,
                "two",
                Vec::new(),
            )
            .with_list_depth(1),
            SemanticBlock::new(
                block(3),
                SemanticBlockKind::BlockQuote,
                None,
                "quote",
                Vec::new(),
            ),
        ]);
        let visible =
            VisibleEditorBlock::from_semantic(block(1), &semantic, DocumentPosition::default());
        let geometry = BlockLayoutGeometry::build(
            &visible,
            EditorViewport::new(400.0, 200.0).expect("viewport"),
            0.0,
            regression_metrics(),
            None,
        )
        .expect("geometry");
        let starts = geometry
            .draw_scalars()
            .iter()
            .filter(|scalar| scalar.block_start)
            .collect::<Vec<_>>();
        assert_eq!(starts.len(), 3);
        assert_eq!(starts[0].list_marker, Some(0));
        assert_eq!(starts[1].list_marker, Some(1));
        assert!(starts[1].bounds.x > starts[0].bounds.x);
        assert_eq!(starts[2].block_kind, SemanticBlockKind::BlockQuote);
        assert_eq!(starts[2].bounds.x, regression_metrics().inset_x + 32.0);
    }

    #[test]
    fn manuscript_defaults_wrap_inside_symmetric_page_margins() {
        let metrics = EditorLayoutMetrics::default();
        let geometry = BlockLayoutGeometry::build(
            &VisibleEditorBlock::new(block(8), "abcdefghijk", DocumentPosition::default()),
            EditorViewport::new(200.0, 160.0).expect("viewport"),
            0.0,
            metrics,
            None,
        )
        .expect("geometry");
        let scalars = geometry.draw_scalars();
        assert_eq!(scalars[0].bounds.x, metrics.inset_x);
        assert_eq!(scalars[0].bounds.y, metrics.inset_y);
        assert!(scalars[10].bounds.y >= metrics.inset_y + metrics.line_height);
    }

    #[test]
    fn wrap_cursors_and_scrolled_chunks_match_a_sequential_reference() {
        let metrics = regression_metrics();
        let viewport = EditorViewport::new(112.0, 240.0).unwrap();
        for text in ["a".repeat(3_072), "é🦀 word,\t wide\u{2003}".repeat(250)] {
            let input = VisibleEditorBlock::new(block(12), text, 100.into());
            let initial = BlockLayoutGeometry::build(&input, viewport, 0.0, metrics, None).unwrap();
            let entry = &initial.height_index[0];
            let mut expected = Vec::new();
            let (mut row, mut x) = (0, entry.first_x);
            // An independent, non-checkpointed reference also exercises wraps
            // exactly at chunk boundaries and positions outside the viewport.
            for offset in 0..entry.scalar_len {
                assert_eq!(cursor_after_prefix(offset, entry, metrics), (row, x));
                if entry.wrap_before.binary_search(&offset).is_ok() {
                    row += 1;
                    x = metrics.inset_x;
                }
                expected.push((x, entry.start_y + row as f32 * entry.line_height));
                x += entry.scalar_advances.get(offset, metrics).unwrap();
            }
            assert_eq!(cursor_after_prefix(usize::MAX, entry, metrics), (row, x));
            for scroll in [0.0, 2_000.0, entry.end_y - viewport.height] {
                let geometry =
                    BlockLayoutGeometry::build(&input, viewport, scroll, metrics, Some(&initial))
                        .unwrap();
                assert!(!geometry.scalars.is_empty());
                for scalar in geometry.draw_scalars() {
                    let offset = (scalar.position.value() - entry.start.value()) as usize;
                    let (x, y) = expected[offset];
                    assert_eq!((scalar.bounds.x, scalar.bounds.y), (x, y - scroll));
                }
            }
        }
    }

    #[test]
    fn overlapping_unsorted_marks_preserve_styles_across_chunks_and_hard_breaks() {
        let marks = [
            SemanticInlineMark::Bold,
            SemanticInlineMark::Italic,
            SemanticInlineMark::Underline,
            SemanticInlineMark::Strikethrough,
            SemanticInlineMark::Link("https://example.com".into()),
            SemanticInlineMark::SmallCaps,
            SemanticInlineMark::Superscript,
            SemanticInlineMark::Subscript,
        ];
        let paragraph = "aé🦀bcdefghij".repeat(400);
        let mut input =
            VisibleEditorBlock::new(block(12), format!("{paragraph}\n{paragraph}"), 100.into());
        // Straddle 1,024-scalar chunk boundaries and the hard break at 4,900.
        let anchors = [100, 1_120, 2_144, 4_896, 4_900, 5_920];
        for anchor in anchors.into_iter().rev() {
            input
                .mark_ranges
                .extend(
                    marks
                        .iter()
                        .enumerate()
                        .rev()
                        .map(|(i, mark)| VisibleMarkRange {
                            range: EditorSelection::new(
                                (anchor + i as u64).into(),
                                (anchor + 4 + i as u64).into(),
                            ),
                            mark: mark.clone(),
                        }),
                );
        }
        input.mark_ranges.push(input.mark_ranges[0].clone());
        let full = BlockLayoutGeometry::build(
            &input,
            EditorViewport::new(300.0, 100_000.0).unwrap(),
            0.0,
            regression_metrics(),
            None,
        )
        .unwrap();
        for anchor in anchors {
            let scroll = full
                .draw_scalars()
                .iter()
                .find(|scalar| scalar.position.value() == anchor)
                .unwrap()
                .bounds
                .y;
            let geometry = BlockLayoutGeometry::build(
                &input,
                EditorViewport::new(300.0, 240.0).unwrap(),
                scroll,
                regression_metrics(),
                Some(&full),
            )
            .unwrap();
            assert!(!geometry.draw_scalars().is_empty());
            for scalar in geometry.draw_scalars() {
                let expected = marks.each_ref().map(|mark| {
                    input.mark_ranges.iter().any(|range| {
                        range.range.start() <= scalar.position
                            && scalar.position < range.range.end()
                            && range.mark == *mark
                    })
                });
                assert_eq!(
                    [
                        scalar.bold,
                        scalar.italic,
                        scalar.underline,
                        scalar.strikethrough,
                        scalar.link,
                        scalar.small_caps,
                        scalar.superscript,
                        scalar.subscript,
                    ],
                    expected,
                    "position {:?}",
                    scalar.position
                );
            }
        }
    }

    #[test]
    fn proportional_advances_are_monotonic_and_scale_without_glyph_collisions() {
        let input = VisibleEditorBlock::new(
            block(12),
            "Wider minds write wisely, then rest.",
            DocumentPosition::default(),
        );
        let one_x_metrics = regression_metrics();
        let one_x = BlockLayoutGeometry::build(
            &input,
            EditorViewport::new(152.0, 140.0).expect("viewport"),
            0.0,
            one_x_metrics,
            None,
        )
        .expect("one-times geometry");
        let two_x_metrics = EditorLayoutMetrics {
            inset_x: one_x_metrics.inset_x * 2.0,
            inset_y: one_x_metrics.inset_y * 2.0,
            scalar_width: one_x_metrics.scalar_width * 2.0,
            line_height: one_x_metrics.line_height * 2.0,
            caret_width: one_x_metrics.caret_width * 2.0,
        };
        let two_x = BlockLayoutGeometry::build(
            &input,
            EditorViewport::new(304.0, 280.0).expect("viewport"),
            0.0,
            two_x_metrics,
            None,
        )
        .expect("two-times geometry");

        let one_x_scalars = one_x.draw_scalars();
        let two_x_scalars = two_x.draw_scalars();
        assert_eq!(one_x_scalars.len(), two_x_scalars.len());
        assert!(
            one_x_scalars.iter().any(|scalar| scalar.character == 'W'
                && scalar.bounds.width > one_x_metrics.scalar_width)
        );
        assert!(
            one_x_scalars.iter().any(|scalar| scalar.character == 'i'
                && scalar.bounds.width < one_x_metrics.scalar_width)
        );

        for (index, (one, two)) in one_x_scalars.iter().zip(two_x_scalars).enumerate() {
            assert_eq!(two.position, one.position, "scalar {index} position");
            assert!((two.bounds.x - one.bounds.x * 2.0).abs() < 0.001);
            assert!((two.bounds.y - one.bounds.y * 2.0).abs() < 0.001);
            assert!((two.bounds.width - one.bounds.width * 2.0).abs() < 0.001);
            if let Some(next) = one_x_scalars.get(index + 1)
                && (next.bounds.y - one.bounds.y).abs() < f32::EPSILON
            {
                assert!(
                    next.bounds.x >= one.bounds.x + one.bounds.width,
                    "same-line scalar {index} overlaps its successor"
                );
            }
        }
    }

    #[test]
    fn prose_and_overlong_tokens_wrap_inside_the_manuscript_pane() {
        let metrics = regression_metrics();
        for (text, width) in [
            ("The harbor held the last of the evening light.", 152.0),
            ("wwwww", 70.0),
        ] {
            let viewport = EditorViewport::new(width, 140.0).unwrap();
            let geometry = BlockLayoutGeometry::build(
                &VisibleEditorBlock::new(block(13), text, DocumentPosition::default()),
                viewport,
                0.0,
                metrics,
                None,
            )
            .unwrap();
            let scalars = geometry.draw_scalars();
            assert!(
                scalars
                    .iter()
                    .any(|scalar| scalar.bounds.y > metrics.inset_y)
            );
            assert!(
                scalars.iter().all(|scalar| {
                    scalar.bounds.x >= metrics.inset_x
                        && scalar.bounds.x + scalar.bounds.width
                            <= width - metrics.inset_x + f32::EPSILON
                }),
                "text escaped the pane: {text}"
            );
        }
    }

    #[test]
    fn words_and_punctuation_stay_together_at_wrap_boundaries() {
        let metrics = regression_metrics();
        for (text, width, last_word) in [
            ("one two turning", 128.0, 8),
            ("word, next", 100.0, 6),
            ("ééé\u{2003}next", 90.0, 4),
        ] {
            let geometry = BlockLayoutGeometry::build(
                &VisibleEditorBlock::new(block(14), text, DocumentPosition::default()),
                EditorViewport::new(width, 120.0).unwrap(),
                0.0,
                metrics,
                None,
            )
            .unwrap();
            let scalars = geometry.draw_scalars();
            for word in scalars.split(|scalar| scalar.character.is_whitespace()) {
                assert!(
                    word.iter()
                        .all(|scalar| scalar.bounds.y == word[0].bounds.y),
                    "word split across rows: {text}"
                );
            }
            assert!(scalars[last_word].bounds.y > scalars[last_word - 1].bounds.y);
            assert_eq!(scalars[last_word].bounds.x, metrics.inset_x);
        }
    }

    #[test]
    fn paragraph_alignment_positions_every_wrapped_row_and_preserves_caret_geometry() {
        let metrics = regression_metrics();
        let viewport = EditorViewport::new(260.0, 800.0).unwrap();
        for alignment in [
            TextAlignment::Start,
            TextAlignment::Center,
            TextAlignment::End,
            TextAlignment::Justify,
        ] {
            let semantic = SemanticDocument::new(vec![
                SemanticBlock::new(
                    block(1),
                    SemanticBlockKind::Paragraph,
                    None,
                    "Several words form lines of different widths and the last line is short.",
                    vec![],
                )
                .with_paragraph_format(parchmint_editor_api::ParagraphFormat {
                    alignment: Some(alignment),
                    line_spacing_percent: Some(200),
                }),
            ]);
            let visible = VisibleEditorBlock::from_semantic(block(1), &semantic, 0.into());
            let geometry =
                BlockLayoutGeometry::build(&visible, viewport, 0.0, metrics, None).unwrap();
            let mut rows: Vec<Vec<&EditorScalarGeometry>> = Vec::new();
            for scalar in geometry.draw_scalars() {
                if rows
                    .last()
                    .is_none_or(|row| row[0].bounds.y != scalar.bounds.y)
                {
                    rows.push(Vec::new());
                }
                rows.last_mut().unwrap().push(scalar);
                let caret = geometry.caret(scalar.position).unwrap();
                assert_eq!(caret.x, scalar.bounds.x);
                assert_eq!(caret.height, 40.0);
            }
            assert!(rows.len() > 2);
            for (index, row) in rows.iter().enumerate() {
                let first = row[0].bounds;
                let last = row
                    .iter()
                    .rev()
                    .find(|s| !s.character.is_whitespace())
                    .unwrap()
                    .bounds;
                let right = last.x + last.width;
                match alignment {
                    TextAlignment::Start => assert_eq!(first.x, metrics.inset_x),
                    TextAlignment::Center => {
                        assert!((first.x + right - viewport.width).abs() < 0.001)
                    }
                    TextAlignment::End => {
                        assert!((right - (viewport.width - metrics.inset_x)).abs() < 0.001)
                    }
                    TextAlignment::Justify if index + 1 < rows.len() => {
                        assert_eq!(first.x, metrics.inset_x);
                        assert!((right - (viewport.width - metrics.inset_x)).abs() < 0.001);
                    }
                    _ => {}
                }
            }
            assert_eq!(
                geometry,
                BlockLayoutGeometry::build(&visible, viewport, 0.0, metrics, Some(&geometry))
                    .unwrap()
            );
        }
    }

    #[test]
    fn semantic_defaults_render_manuscript_serif_hierarchy() {
        let visible = VisibleEditorBlock::from_semantic(
            block(7),
            &SemanticDocument::new(vec![
                SemanticBlock::new(
                    block(7),
                    SemanticBlockKind::Heading1,
                    None,
                    "Heading",
                    Vec::new(),
                ),
                SemanticBlock::new(
                    block(8),
                    SemanticBlockKind::Paragraph,
                    None,
                    "Body",
                    Vec::new(),
                ),
            ]),
            DocumentPosition::default(),
        );
        let geometry = BlockLayoutGeometry::build(
            &visible,
            EditorViewport::new(480.0, 240.0).expect("viewport"),
            0.0,
            regression_metrics(),
            None,
        )
        .expect("geometry");
        let heading = &geometry.draw_scalars()[0];
        let body = geometry
            .draw_scalars()
            .iter()
            .find(|scalar| scalar.character == 'B')
            .expect("paragraph scalar");
        assert_eq!(heading.font_family, EditorFontFamily::Serif);
        assert_eq!(heading.font_size, 32.0);
        assert_eq!(heading.font_weight, 700);
        assert_eq!(body.font_family, EditorFontFamily::Serif);
        assert_eq!(body.font_size, 20.0);
        assert_eq!(body.font_weight, 400);
    }

    #[test]
    fn explicit_heading_weight_overrides_available_default_face_weight() {
        let mut catalog = StyleCatalog::default();
        catalog
            .upsert(StyleDefinition {
                id: StyleCatalog::heading_1_id(),
                display_name: "Heading 1".into(),
                role: StyleRole::Heading1,
                inherits: None,
                properties: StyleProperties {
                    weight: Some(700),
                    ..StyleProperties::default()
                },
            })
            .expect("replace reserved heading properties");
        let semantic = SemanticDocument::new(vec![SemanticBlock::new(
            block(9),
            SemanticBlockKind::Heading1,
            None,
            "Heading",
            Vec::new(),
        )]);
        let visible = VisibleEditorBlock::from_semantic_with_styles(
            block(9),
            &semantic,
            DocumentPosition::default(),
            &StyleCatalogProjection::new(catalog),
        );
        let geometry = BlockLayoutGeometry::build(
            &visible,
            EditorViewport::new(480.0, 120.0).expect("viewport"),
            0.0,
            regression_metrics(),
            None,
        )
        .expect("geometry");

        assert_eq!(
            geometry.draw_scalars()[0].font_family,
            EditorFontFamily::Serif
        );
        assert_eq!(geometry.draw_scalars()[0].font_weight, 700);
    }

    #[test]
    fn centered_blocks_use_proportional_advances_for_their_offset() {
        let mut catalog = StyleCatalog::default();
        catalog
            .upsert(StyleDefinition {
                id: StyleCatalog::body_id(),
                display_name: "Body".into(),
                role: StyleRole::Body,
                inherits: None,
                properties: StyleProperties {
                    alignment: Some(TextAlignment::Center),
                    ..StyleProperties::default()
                },
            })
            .expect("replace body alignment");
        let centered = |text: &str| {
            let visible = VisibleEditorBlock::from_semantic_with_styles(
                block(8),
                &SemanticDocument::new(vec![SemanticBlock::new(
                    block(8),
                    SemanticBlockKind::Paragraph,
                    Some("body".into()),
                    text,
                    Vec::new(),
                )]),
                DocumentPosition::default(),
                &StyleCatalogProjection::new(catalog.clone()),
            );
            BlockLayoutGeometry::build(
                &visible,
                EditorViewport::new(200.0, 120.0).expect("viewport"),
                0.0,
                regression_metrics(),
                None,
            )
            .expect("geometry")
        };
        let wide = centered("WW");
        let narrow = centered("ii");

        assert!(
            wide.draw_scalars()[0].bounds.x < narrow.draw_scalars()[0].bounds.x,
            "a wider centered run must begin farther left"
        );
    }

    #[test]
    fn retained_carets_support_vertical_line_edge_and_bounded_scroll_geometry() {
        let viewport = EditorViewport::new(200.0, 20.0).expect("viewport");
        let geometry = BlockLayoutGeometry::build(
            &VisibleEditorBlock::new(block(9), "abc\ndef", DocumentPosition::default()),
            viewport,
            0.0,
            regression_metrics(),
            None,
        )
        .expect("geometry");

        assert_eq!(
            geometry.document_range(),
            EditorSelection::new(0.into(), 7.into())
        );
        assert_eq!(geometry.caret_below(1.into()), Some(5.into()));
        assert_eq!(geometry.caret_above(5.into()), Some(1.into()));
        assert_eq!(geometry.line_start(2.into()), Some(0.into()));
        assert_eq!(geometry.line_end(2.into()), Some(3.into()));
        assert!(geometry.max_scroll_y(viewport) > 0.0);
    }

    #[test]
    fn word_and_paragraph_selection_use_unicode_scalar_positions() {
        let geometry = BlockLayoutGeometry::build(
            &VisibleEditorBlock::new(block(10), "héllo, world", DocumentPosition::default()),
            EditorViewport::new(240.0, 80.0).expect("viewport"),
            0.0,
            regression_metrics(),
            None,
        )
        .expect("geometry");

        assert_eq!(
            geometry.word_selection_at(2.into()),
            Some(EditorSelection::new(0.into(), 5.into()))
        );
        assert_eq!(
            geometry.word_selection_at(5.into()),
            None,
            "comma is not a word"
        );
        assert_eq!(
            geometry.paragraph_selection_at(8.into()),
            Some(EditorSelection::new(0.into(), 12.into()))
        );
    }

    #[test]
    fn layout_chunks_borrow_utf8_ranges_across_empty_lines_and_chunk_boundaries() {
        let text = format!("\n{}🦀尾\n\né\n", "é".repeat(LAYOUT_CHUNK_SCALARS - 1));
        let input = VisibleEditorBlock::new(block(1), &text, 100.into());
        let mut position = 100;
        for (line, expected) in input.layout_lines.iter().zip(text.split('\n')) {
            assert_eq!(line.start.value(), position);
            let mut reconstructed = String::new();
            let mut scalar_offset = 0;
            for chunk in &line.shape.chunks {
                let content = &input.text.line(line)[chunk.text_range.clone()];
                assert_eq!(chunk.scalar_len, content.chars().count());
                assert!(chunk.scalar_len <= LAYOUT_CHUNK_SCALARS);
                assert_eq!(chunk.scalar_offset, scalar_offset);
                assert_eq!(chunk.scalar_offset, scalar_offset);
                reconstructed.push_str(content);
                scalar_offset += chunk.scalar_len;
            }
            assert_eq!(reconstructed, expected);
            assert_eq!(line.end.value(), position + scalar_offset as u64);
            position = line.end.value() + 1;
        }
        assert_eq!(input.layout_lines.len(), 5);
        assert_eq!(input.layout_lines[1].shape.chunks.len(), 2);
        assert!(input.layout_lines.last().unwrap().hard_break.is_none());
    }

    #[test]
    fn soft_breaks_and_empty_paragraphs_keep_their_semantic_spans() {
        let semantic = SemanticDocument::new(
            ["first\n\nline", "", "é\n", "tail"]
                .into_iter()
                .enumerate()
                .map(|(index, text)| {
                    SemanticBlock::new(
                        block(index as u8),
                        SemanticBlockKind::Paragraph,
                        None,
                        text,
                        Vec::new(),
                    )
                })
                .collect(),
        );
        let input = VisibleEditorBlock::from_semantic(block(1), &semantic, 100.into());
        assert_eq!(
            input
                .layout_lines
                .iter()
                .map(|line| line.span_index)
                .collect::<Vec<_>>(),
            [
                Some(0),
                Some(0),
                Some(0),
                Some(1),
                Some(2),
                Some(2),
                Some(3)
            ]
        );
    }

    #[test]
    fn incremental_heights_match_full_layout_after_unicode_edits_and_line_splits() {
        let viewport = EditorViewport::new(200.0, 140.0).unwrap();
        let mut text = format!("head\n{}\n\nunchanged tail", "é 🦀 word ".repeat(150));
        let mut previous = None;
        let mut seed = 31_u64;
        for step in 0..60 {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
            let count = text.chars().count();
            let scalar = seed as usize % count;
            let byte = text.char_indices().nth(scalar).unwrap().0;
            if step % 3 == 0 {
                text.remove(byte);
            } else {
                text.insert_str(byte, if step % 3 == 1 { "\n" } else { "尾" });
            }
            let input = VisibleEditorBlock::new(block(1), &text, (step * 100).into());
            let scroll = (step % 5) as f32 * 150.0;
            let incremental = BlockLayoutGeometry::build(
                &input,
                viewport,
                scroll,
                regression_metrics(),
                previous.as_ref(),
            )
            .unwrap();
            let fresh =
                BlockLayoutGeometry::build(&input, viewport, scroll, regression_metrics(), None)
                    .unwrap();
            assert_eq!(incremental, fresh, "edit {step}");
            previous = Some(incremental);
        }
    }

    #[test]
    fn incremental_render_input_matches_full_rebuild_across_semantic_changes() {
        let mut paragraphs = vec![
            (SemanticBlockKind::Paragraph, "same\n\né 🦀".into(), 0),
            (SemanticBlockKind::OrderedListItem, "same".into(), 0),
            (
                SemanticBlockKind::OrderedListItem,
                "尾 word ".repeat(400),
                1,
            ),
            (SemanticBlockKind::Paragraph, String::new(), 0),
        ];
        let mut input = VisibleEditorBlock::new(block(1), "", 0.into());
        let mut geometry = None;
        let mut seed = 31_u64;
        for step in 0..120 {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
            let index = seed as usize % paragraphs.len();
            let text = &mut paragraphs[index].1;
            let scalar = seed as usize % (text.chars().count() + 1);
            let byte = text
                .char_indices()
                .nth(scalar)
                .map_or(text.len(), |(byte, _)| byte);
            match step % 6 {
                0 => text.insert_str(byte, "尾\n🦀"),
                1 if byte < text.len() => {
                    text.remove(byte);
                }
                2 => {
                    let tail = text.split_off(byte);
                    paragraphs.insert(index + 1, (SemanticBlockKind::Paragraph, tail, 0));
                }
                3 if paragraphs.len() > 1 => {
                    let removed = paragraphs.remove(index);
                    let target = index.min(paragraphs.len() - 1);
                    paragraphs[target].1.push_str(&removed.1);
                }
                4 => {
                    paragraphs[index].0 = match step / 6 % 4 {
                        0 => SemanticBlockKind::SceneBreak,
                        1 => SemanticBlockKind::OrderedListItem,
                        2 => SemanticBlockKind::PageBreak,
                        _ => SemanticBlockKind::Heading2,
                    };
                    paragraphs[index].2 = step % 3;
                }
                _ => {}
            }
            let semantic = SemanticDocument::new(
                paragraphs
                    .iter()
                    .enumerate()
                    .map(|(i, (kind, text, depth))| {
                        let marks = if text.is_empty() || step % 2 == 0 {
                            vec![]
                        } else {
                            vec![SemanticMarkRange::new(
                                EditorSelection::new(0.into(), 1.into()),
                                SemanticInlineMark::Link("https://example.com".into()),
                            )]
                        };
                        SemanticBlock::new(block(i as u8 + 1), *kind, None, text, marks)
                            .with_list_depth(*depth)
                    })
                    .collect(),
            );
            let mut catalog = StyleCatalog::default();
            catalog
                .upsert(StyleDefinition {
                    id: StyleCatalog::body_id(),
                    display_name: "Body".into(),
                    role: StyleRole::Body,
                    inherits: None,
                    properties: StyleProperties {
                        font_family: Some(if step % 2 == 0 { "Serif" } else { "Monospace" }.into()),
                        font_size_points: Some(12.0 + (step % 3) as f32),
                        space_before_points: Some((step % 4) as f32),
                        space_after_points: Some((step % 5) as f32),
                        ..StyleProperties::default()
                    },
                })
                .unwrap();
            let styles = StyleCatalogProjection::new(catalog);
            let start = ((step / 10 * 100) as u64).into();
            input = VisibleEditorBlock::build_semantic(
                block(1),
                &semantic,
                start,
                &styles,
                Some(&input),
            );
            let fresh =
                VisibleEditorBlock::from_semantic_with_styles(block(1), &semantic, start, &styles);
            assert_eq!(input, fresh, "render input at edit {step}");
            let flat = flattened_reference(&fresh, semantic.plain_text());
            let viewport = EditorViewport::new(200.0 + (step % 3) as f32 * 20.0, 140.0).unwrap();
            let scroll = (step % 5) as f32 * 70.0;
            let incremental = BlockLayoutGeometry::build(
                &input,
                viewport,
                scroll,
                regression_metrics(),
                geometry.as_ref(),
            )
            .unwrap();
            let fresh =
                BlockLayoutGeometry::build(&fresh, viewport, scroll, regression_metrics(), None)
                    .unwrap();
            assert_eq!(incremental, fresh, "geometry at edit {step}");
            let flat =
                BlockLayoutGeometry::build(&flat, viewport, scroll, regression_metrics(), None)
                    .unwrap();
            assert_eq!(incremental, flat, "flattened geometry at edit {step}");
            assert_text_not_flattened(&input);
            assert_eq!(input.text(), semantic.plain_text());
            geometry = Some(incremental);
        }
        for semantic in [
            SemanticDocument::default(),
            SemanticDocument::new(vec![SemanticBlock::new(
                block(1),
                SemanticBlockKind::Paragraph,
                None,
                "\n",
                vec![],
            )]),
        ] {
            input.update_semantic(&semantic, &StyleCatalogProjection::default());
            assert_text_not_flattened(&input);
            assert_eq!(
                input,
                VisibleEditorBlock::from_semantic(block(1), &semantic, input.document_start())
            );
            let viewport = EditorViewport::new(320.0, 240.0).unwrap();
            let flat = flattened_reference(&input, semantic.plain_text());
            assert_eq!(
                BlockLayoutGeometry::build(&input, viewport, 0.0, regression_metrics(), None)
                    .unwrap(),
                BlockLayoutGeometry::build(&flat, viewport, 0.0, regression_metrics(), None)
                    .unwrap(),
            );
            assert_eq!(input.text(), semantic.plain_text());
        }
    }

    #[test]
    fn unchanged_line_metrics_are_shared_but_style_and_width_changes_reflow() {
        let viewport = EditorViewport::new(200.0, 140.0).unwrap();
        let input = |text: &str, kind| {
            VisibleEditorBlock::from_semantic(
                block(1),
                &SemanticDocument::new(vec![
                    SemanticBlock::new(block(1), kind, None, text, Vec::new()),
                    SemanticBlock::new(
                        block(2),
                        SemanticBlockKind::Paragraph,
                        None,
                        "unchanged é tail that wraps across several rows",
                        Vec::new(),
                    ),
                ]),
                0.into(),
            )
        };
        let first_input = input("heading", SemanticBlockKind::Paragraph);
        let first =
            BlockLayoutGeometry::build(&first_input, viewport, 0.0, regression_metrics(), None)
                .unwrap();
        for (text, kind, width) in [
            ("edited\nheading", SemanticBlockKind::Paragraph, 200.0),
            ("heading", SemanticBlockKind::Heading1, 200.0),
            ("heading", SemanticBlockKind::Paragraph, 300.0),
        ] {
            let changed = input(text, kind);
            let viewport = EditorViewport::new(width, 140.0).unwrap();
            let incremental = BlockLayoutGeometry::build(
                &changed,
                viewport,
                0.0,
                regression_metrics(),
                Some(&first),
            )
            .unwrap();
            let fresh =
                BlockLayoutGeometry::build(&changed, viewport, 0.0, regression_metrics(), None)
                    .unwrap();
            assert_eq!(incremental, fresh);
            assert_eq!(
                Arc::ptr_eq(
                    &first.height_index.last().unwrap().metrics,
                    &incremental.height_index.last().unwrap().metrics,
                ),
                width == 200.0
            );
            assert!(!Arc::ptr_eq(
                &first.height_index[0].metrics,
                &incremental.height_index[0].metrics,
            ));
        }
    }

    #[test]
    fn cached_heights_follow_a_changed_document_start() {
        let text = "paragraph\n".repeat(100);
        let first_input = VisibleEditorBlock::new(block(1), &text, 0.into());
        let moved_input = VisibleEditorBlock::new(block(1), &text, 1_000.into());
        let viewport = EditorViewport::new(400.0, 200.0).unwrap();
        let first =
            BlockLayoutGeometry::build(&first_input, viewport, 0.0, regression_metrics(), None)
                .unwrap();
        let moved = BlockLayoutGeometry::build(
            &moved_input,
            viewport,
            0.0,
            regression_metrics(),
            Some(&first),
        )
        .unwrap();
        let fresh =
            BlockLayoutGeometry::build(&moved_input, viewport, 0.0, regression_metrics(), None)
                .unwrap();
        assert_eq!(moved.caret(1_900.into()), fresh.caret(1_900.into()));
        assert_eq!(moved, fresh);
    }

    #[test]
    fn thousands_of_lines_materialize_only_the_visible_window_and_reuse_heights_on_scroll() {
        let text = vec!["paragraph"; 5_000].join("\n");
        let input = VisibleEditorBlock::new(block(10), text, DocumentPosition::default());
        let viewport = EditorViewport::new(400.0, 200.0).expect("viewport");
        let first = BlockLayoutGeometry::build(&input, viewport, 0.0, regression_metrics(), None)
            .expect("first viewport");

        assert!(first.layout_work().materialized_lines <= 24);
        assert!(first.layout_work().materialized_chunks <= 24);
        assert!(first.layout_work().materialized_scalars <= 240);
        assert!(first.max_scroll_y(viewport) > 50_000.0);

        let scrolled = BlockLayoutGeometry::build(
            &input,
            viewport,
            40_000.0,
            regression_metrics(),
            Some(&first),
        )
        .expect("scrolled viewport");
        assert!(Arc::ptr_eq(&first.height_index, &scrolled.height_index));
        assert_eq!(first.content_height, scrolled.content_height);
        assert!(scrolled.layout_work().materialized_lines <= 32);
        assert!(scrolled.layout_work().materialized_scalars <= 320);
        assert!(scrolled.draw_scalars()[0].position.value() > 1_000);
    }

    #[test]
    fn huge_single_line_is_chunked_and_preserves_global_mark_geometry() {
        let scalar_count = 250_000_u64;
        let input = VisibleEditorBlock::new(
            block(11),
            "x".repeat(scalar_count as usize),
            DocumentPosition::default(),
        )
        .with_bold_ranges(vec![EditorSelection::new(0.into(), scalar_count.into())]);
        let viewport = EditorViewport::new(200.0, 100.0).expect("viewport");
        let geometry =
            BlockLayoutGeometry::build(&input, viewport, 100_000.0, regression_metrics(), None)
                .expect("deep huge-paragraph viewport");

        let work = geometry.layout_work();
        assert_eq!(work.materialized_lines, 1);
        assert!(work.materialized_chunks <= 2);
        assert!(work.materialized_scalars <= 450);
        assert_eq!(
            geometry.height_index[0].prefix_cursors.len(),
            input.layout_lines[0].shape.chunks.len() + 1,
            "cursor checkpoints stay bounded to chunk boundaries"
        );
        assert_eq!(geometry.document_range().end().value(), scalar_count);
        let first = geometry.draw_scalars().first().expect("visible scalar");
        assert!(
            first.position.value() > scalar_count / 3,
            "a deep scroll must not materialize document-head scalars"
        );
        assert!(first.bold);
        let selection = EditorSelection::new(
            first.position,
            DocumentPosition::from(first.position.value() + 5),
        );
        assert_eq!(geometry.selection_rectangles(selection).len(), 5);
    }
}
