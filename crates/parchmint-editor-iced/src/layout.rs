use std::sync::Arc;

use parchmint_editor_api::{
    AtomicBlockKind, BlockId, DocumentPosition, EditorSelection, SelectionRectangle,
    SemanticBlockKind, SemanticInlineMark, StyleCatalog, StyleCatalogProjection, StyleId,
    StyleProperties, TextAlignment,
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
            inset_y: 62.0,
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
    text: String,
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
    start: DocumentPosition,
    scalar_offset: usize,
    scalar_len: usize,
    text: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct VisibleLayoutLine {
    start: DocumentPosition,
    end: DocumentPosition,
    chunks: Vec<VisibleLayoutChunk>,
    scalar_len: usize,
    hard_break: Option<DocumentPosition>,
    span_index: Option<usize>,
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
    style: ResolvedBlockStyle,
}

#[derive(Debug, Clone, Default, PartialEq)]
struct ResolvedBlockStyle {
    font_family: Option<String>,
    font_size_points: Option<f32>,
    weight: Option<u16>,
    italic: Option<bool>,
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
        let layout_lines = build_layout_lines(&text, document_start, &[]);
        Self {
            block,
            layout_signature: layout_signature(&text, &[]),
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
        let mut offset = document_start.value();
        let mut mark_ranges = Vec::new();
        let mut atomic_nodes = Vec::new();
        let mut block_spans = Vec::new();
        let mut ordered_ordinals: Vec<usize> = Vec::new();
        for semantic_block in semantic.blocks() {
            let block_start = offset;
            for mark in semantic_block.marks() {
                mark_ranges.push(VisibleMarkRange {
                    range: EditorSelection::new(
                        DocumentPosition::from(offset + mark.range().start().value()),
                        DocumentPosition::from(offset + mark.range().end().value()),
                    ),
                    mark: mark.mark().clone(),
                });
            }
            let scalar_len = match semantic_block.kind() {
                SemanticBlockKind::SceneBreak => {
                    atomic_nodes
                        .push((DocumentPosition::from(offset), AtomicBlockKind::SceneBreak));
                    1
                }
                SemanticBlockKind::PageBreak => {
                    atomic_nodes.push((DocumentPosition::from(offset), AtomicBlockKind::PageBreak));
                    1
                }
                _ => semantic_block.text().chars().count() as u64,
            };
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
            block_spans.push(VisibleBlockSpan {
                start: DocumentPosition::from(block_start),
                end: DocumentPosition::from(block_start + scalar_len),
                kind: semantic_block.kind(),
                list_depth: semantic_block.list_depth(),
                list_ordinal,
                style: resolve_block_style(semantic_block, styles.catalog()),
            });
            offset += scalar_len + 1;
        }
        let text = semantic.plain_text();
        let scalar_len = text.chars().count() as u64;
        let layout_lines = build_layout_lines(&text, document_start, &block_spans);
        Self {
            block,
            layout_signature: layout_signature(&text, &block_spans),
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
        &self.text
    }

    pub const fn document_start(&self) -> DocumentPosition {
        self.document_start
    }
}

fn build_layout_lines(
    text: &str,
    document_start: DocumentPosition,
    spans: &[VisibleBlockSpan],
) -> Vec<VisibleLayoutLine> {
    let mut lines = Vec::new();
    let mut line_start = document_start.value();
    let mut line_text = String::new();
    let mut line_scalar_len = 0_usize;
    let mut position = document_start.value();
    let finish_line = |lines: &mut Vec<VisibleLayoutLine>,
                       line_start: u64,
                       line_text: &mut String,
                       line_scalar_len: usize,
                       hard_break: Option<DocumentPosition>| {
        let span_index = spans
            .iter()
            .position(|span| span.start.value() <= line_start && line_start <= span.end.value());
        let mut chunks = Vec::new();
        let mut chunk = String::new();
        let mut chunk_start = line_start;
        let mut chunk_offset = 0_usize;
        let mut chunk_len = 0_usize;
        for (offset, character) in line_text.chars().enumerate() {
            if chunk.is_empty() {
                chunk_start = line_start.saturating_add(offset as u64);
                chunk_offset = offset;
            }
            chunk.push(character);
            chunk_len += 1;
            if chunk_len == LAYOUT_CHUNK_SCALARS {
                chunks.push(VisibleLayoutChunk {
                    start: DocumentPosition::from(chunk_start),
                    scalar_offset: chunk_offset,
                    scalar_len: chunk_len,
                    text: std::mem::take(&mut chunk),
                });
                chunk_len = 0;
            }
        }
        if !chunk.is_empty() {
            chunks.push(VisibleLayoutChunk {
                start: DocumentPosition::from(chunk_start),
                scalar_offset: chunk_offset,
                scalar_len: chunk_len,
                text: std::mem::take(&mut chunk),
            });
        }
        lines.push(VisibleLayoutLine {
            start: DocumentPosition::from(line_start),
            end: DocumentPosition::from(line_start.saturating_add(line_scalar_len as u64)),
            chunks,
            scalar_len: line_scalar_len,
            hard_break,
            span_index,
        });
        line_text.clear();
    };
    for character in text.chars() {
        if character == '\n' {
            finish_line(
                &mut lines,
                line_start,
                &mut line_text,
                line_scalar_len,
                Some(DocumentPosition::from(position)),
            );
            position = position.saturating_add(1);
            line_start = position;
            line_scalar_len = 0;
            continue;
        }
        line_text.push(character);
        line_scalar_len += 1;
        position = position.saturating_add(1);
    }
    finish_line(
        &mut lines,
        line_start,
        &mut line_text,
        line_scalar_len,
        None,
    );
    lines
}

fn layout_signature(text: &str, spans: &[VisibleBlockSpan]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for byte in text.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x100_0000_01b3);
    }
    for span in spans {
        hash ^= span.start.value();
        hash = hash.wrapping_mul(0x100_0000_01b3);
        hash ^= span.end.value();
        hash = hash.wrapping_mul(0x100_0000_01b3);
        for byte in format!("{span:?}").bytes() {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(0x100_0000_01b3);
        }
    }
    hash
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
    pub font_weight: u16,
    pub block_italic: bool,
    pub font_family: EditorFontFamily,
    pub atomic: Option<AtomicBlockKind>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorFontFamily {
    SansSerif,
    Serif,
    Monospace,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct LayoutWork {
    pub materialized_lines: usize,
    pub materialized_chunks: usize,
    pub materialized_scalars: usize,
}

#[derive(Debug, Clone, PartialEq)]
struct LineHeightEntry {
    line_index: usize,
    start: DocumentPosition,
    end: DocumentPosition,
    scalar_len: usize,
    /// Logical advances used by both the canvas positions and the caret map.
    ///
    /// Canvas shapes a proportional font, so a fixed cell for every scalar
    /// causes wide glyphs to paint into their neighbours while spaces become
    /// visibly too wide. Keeping the deterministic advances here makes the
    /// viewport cache, rendering, hit testing, and wrapping use one model.
    scalar_advances: Vec<f32>,
    /// Scalar offsets that begin a visual row. These are computed once from
    /// word boundaries and consumed by every geometry path.
    wrap_before: Vec<usize>,
    /// A cursor state at every chunk boundary. Lookup may scan at most one
    /// chunk, keeping deep single-line documents linear to index and bounded
    /// to materialize.
    prefix_cursors: Vec<PrefixCursor>,
    start_y: f32,
    end_y: f32,
    line_height: f32,
    first_x: f32,
    right_edge: f32,
    chunk_rows: Vec<(usize, usize)>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct PrefixCursor {
    scalar_offset: usize,
    row: usize,
    x: f32,
}

/// The one geometry object used for block drawing, hit testing, carets, and selections.
#[derive(Debug, Clone, PartialEq)]
pub struct BlockLayoutGeometry {
    block: BlockId,
    scalars: Vec<EditorScalarGeometry>,
    carets: Vec<(DocumentPosition, EditorRectangle)>,
    block_kinds: Vec<(DocumentPosition, DocumentPosition, SemanticBlockKind)>,
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
        let height_index = previous
            .filter(|geometry| {
                geometry.layout_signature == input.layout_signature
                    && geometry.viewport_width == viewport.width
                    && geometry.metrics == metrics
            })
            .map_or_else(
                || build_height_index(input, viewport, metrics).map(Arc::new),
                |geometry| Ok(Arc::clone(&geometry.height_index)),
            )?;
        let content_height = height_index
            .last()
            .map_or(metrics.inset_y * 2.0 + metrics.line_height, |line| {
                line.end_y + metrics.inset_y
            });
        let overscan_top = (pixel_scroll_y - viewport.height).max(0.0);
        let overscan_bottom = pixel_scroll_y + viewport.height * 2.0;
        let first_line = height_index.partition_point(|line| line.end_y < overscan_top);
        let mut scalars = Vec::new();
        let mut carets = Vec::new();
        let mut work = LayoutWork::default();
        for entry in height_index.iter().skip(first_line) {
            if entry.start_y > overscan_bottom {
                break;
            }
            let line = &input.layout_lines[entry.line_index];
            work.materialized_lines += 1;
            let span = line.span_index.map(|index| &input.block_spans[index]);
            for (chunk_index, chunk) in line.chunks.iter().enumerate() {
                let (start_row, end_row) = entry.chunk_rows[chunk_index];
                let chunk_top = entry.start_y + start_row as f32 * entry.line_height;
                let chunk_bottom = entry.start_y + (end_row + 1) as f32 * entry.line_height;
                if chunk_bottom < overscan_top || chunk_top > overscan_bottom {
                    continue;
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
            if line.chunks.is_empty() && line_intersects(entry, overscan_top, overscan_bottom) {
                replace_or_push_caret(
                    &mut carets,
                    line.start,
                    caret_rectangle(entry.first_x, entry.start_y - pixel_scroll_y, metrics),
                );
            }
            if let Some(position) = line.hard_break {
                let (row, x) = cursor_after_prefix(line.scalar_len, entry, metrics);
                let global_y = entry.start_y + row as f32 * entry.line_height;
                if global_y + entry.line_height >= overscan_top && global_y <= overscan_bottom {
                    let y = global_y - pixel_scroll_y;
                    scalars.push(scalar_geometry(
                        input, None, position, '\n', x, y, 0.0, metrics,
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
        carets.sort_by_key(|(position, _)| *position);
        carets.dedup_by_key(|(position, _)| *position);
        let first = input.document_start;
        let document_end = input
            .document_start
            .value()
            .checked_add(input.scalar_len)
            .ok_or("document position overflowed")?;

        Ok(Self {
            block: input.block,
            scalars,
            carets,
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

    pub const fn layout_work(&self) -> LayoutWork {
        self.work
    }

    pub fn hit_test(&self, x: f32, y: f32) -> Option<DocumentPosition> {
        if !x.is_finite() || !y.is_finite() {
            return None;
        }
        self.carets
            .iter()
            .min_by(|(_, left), (_, right)| {
                distance_squared(*left, x, y).total_cmp(&distance_squared(*right, x, y))
            })
            .map(|(position, _)| *position)
    }

    pub fn caret(&self, position: DocumentPosition) -> Option<EditorRectangle> {
        self.carets
            .iter()
            .find(|(candidate, _)| *candidate == position)
            .map(|(_, rectangle)| *rectangle)
            .or_else(|| {
                let line = self
                    .height_index
                    .iter()
                    .filter(|line| line.start <= position && position <= line.end)
                    .max_by_key(|line| line.start)?;
                let offset = position.value().saturating_sub(line.start.value()) as usize;
                let (row, x) = cursor_after_prefix(offset, line, self.metrics);
                Some(caret_rectangle(
                    x,
                    line.start_y + row as f32 * line.line_height - self.pixel_scroll_y,
                    self.metrics,
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
) -> Result<Vec<LineHeightEntry>, &'static str> {
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
        let characters = line
            .chunks
            .iter()
            .flat_map(|chunk| chunk.text.chars())
            .collect::<Vec<_>>();
        let scalar_advances = characters
            .iter()
            .copied()
            .map(|character| scalar_advance(character, span, metrics))
            .collect::<Vec<_>>();
        let first_x = metrics.inset_x
            + span.map_or(0.0, |span| {
                block_indent(span, metrics)
                    + alignment_offset(
                        span,
                        viewport,
                        metrics,
                        scalar_advances.iter().copied().sum(),
                    )
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
        let wrap_before =
            word_wrap_offsets(&characters, &scalar_advances, first_x, right_edge, metrics);
        let mut entry = LineHeightEntry {
            line_index,
            start: line.start,
            end: line.end,
            scalar_len: line.scalar_len,
            scalar_advances,
            wrap_before,
            prefix_cursors: Vec::with_capacity(line.chunks.len().saturating_add(1)),
            start_y: y,
            end_y: y,
            line_height,
            first_x,
            // The manuscript needs symmetric page margins. The old expression
            // added the left inset to the viewport width, which placed the
            // wrapping edge outside the canvas and let prose run under the
            // inspector in narrow panes.
            right_edge,
            chunk_rows: Vec::with_capacity(line.chunks.len()),
        };
        let mut cursor = PrefixCursor {
            scalar_offset: 0,
            row: 0,
            x: entry.first_x,
        };
        let mut wrap_index = 0_usize;
        for chunk in &line.chunks {
            entry.prefix_cursors.push(cursor);
            let start = cursor.row;
            let end_offset = chunk.scalar_offset.saturating_add(chunk.scalar_len);
            for scalar_offset in chunk.scalar_offset..end_offset {
                if entry.wrap_before.get(wrap_index) == Some(&scalar_offset) {
                    cursor.row = cursor.row.saturating_add(1);
                    cursor.x = metrics.inset_x;
                    wrap_index += 1;
                }
                cursor.x += entry.scalar_advances[scalar_offset];
            }
            cursor.scalar_offset = end_offset;
            entry.chunk_rows.push((start, cursor.row));
        }
        entry.prefix_cursors.push(cursor);
        let rows = cursor_after_prefix(line.scalar_len, &entry, metrics)
            .0
            .saturating_add(1);
        y += rows as f32 * line_height;
        if line.hard_break.is_some() && span.is_some_and(|span| span.end == line.end) {
            y += points_to_pixels(
                span.and_then(|span| span.style.space_after_points)
                    .unwrap_or(0.0),
            );
        }
        entry.end_y = y;
        height_index.push(entry);
    }
    Ok(height_index)
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
    for scalar_offset in checkpoint.scalar_offset..scalar_count {
        apply_wrap_before(scalar_offset, entry, &mut row, &mut x, metrics);
        x += entry.scalar_advances[scalar_offset];
    }
    (row, x)
}

fn apply_wrap_before(
    scalar_offset: usize,
    entry: &LineHeightEntry,
    row: &mut usize,
    x: &mut f32,
    metrics: EditorLayoutMetrics,
) {
    if entry.wrap_before.binary_search(&scalar_offset).is_ok() {
        *row = row.saturating_add(1);
        *x = metrics.inset_x;
    }
}

fn word_wrap_offsets(
    characters: &[char],
    advances: &[f32],
    first_x: f32,
    right_edge: f32,
    metrics: EditorLayoutMetrics,
) -> Vec<usize> {
    let mut wraps = Vec::new();
    let mut offset = 0_usize;
    let mut x = first_x;
    while offset < characters.len() {
        if characters[offset].is_whitespace() {
            if x > metrics.inset_x && x + advances[offset] > right_edge {
                wraps.push(offset);
                x = metrics.inset_x;
            }
            x += advances[offset];
            offset += 1;
            continue;
        }

        let word_start = offset;
        while offset < characters.len() && !characters[offset].is_whitespace() {
            offset += 1;
        }
        let word_width = advances[word_start..offset].iter().copied().sum::<f32>();
        if x > metrics.inset_x && x + word_width > right_edge {
            wraps.push(word_start);
            x = metrics.inset_x;
        }
        for (scalar_offset, advance) in advances.iter().enumerate().take(offset).skip(word_start) {
            // Only an individual token wider than a row falls back to scalar
            // breaking. Ordinary words always move as a complete run.
            if x > metrics.inset_x && x + *advance > right_edge {
                wraps.push(scalar_offset);
                x = metrics.inset_x;
            }
            x += *advance;
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
    let mut materialized = 0_usize;
    for (offset, character) in chunk.text.chars().enumerate() {
        let position = chunk
            .start
            .value()
            .checked_add(offset as u64)
            .ok_or("document position overflowed")?;
        let width = entry
            .scalar_advances
            .get(chunk.scalar_offset.saturating_add(offset))
            .copied()
            .ok_or("layout chunk advance is missing")?;
        let scalar_offset = chunk.scalar_offset.saturating_add(offset);
        apply_wrap_before(scalar_offset, entry, &mut row, &mut x, metrics);
        let global_y = entry.start_y + row as f32 * entry.line_height;
        let y = global_y - pixel_scroll_y;
        let visible = global_y + entry.line_height >= overscan_top && global_y <= overscan_bottom;
        if visible {
            materialized += 1;
            replace_or_push_caret(
                carets,
                DocumentPosition::from(position),
                caret_rectangle(x, y, metrics),
            );
            scalars.push(scalar_geometry(
                input,
                span,
                DocumentPosition::from(position),
                character,
                x,
                y,
                width,
                metrics,
            ));
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
                caret_rectangle(x, y, metrics),
            );
        }
    }
    Ok(materialized)
}

#[allow(clippy::too_many_arguments)]
fn scalar_geometry(
    input: &VisibleEditorBlock,
    span: Option<&VisibleBlockSpan>,
    position: DocumentPosition,
    character: char,
    x: f32,
    y: f32,
    width: f32,
    metrics: EditorLayoutMetrics,
) -> EditorScalarGeometry {
    let offset = position.value();
    EditorScalarGeometry {
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
        bold: has_mark(&input.mark_ranges, offset, |mark| {
            matches!(mark, SemanticInlineMark::Bold)
        }),
        italic: has_mark(&input.mark_ranges, offset, |mark| {
            matches!(mark, SemanticInlineMark::Italic)
        }),
        underline: has_mark(&input.mark_ranges, offset, |mark| {
            matches!(mark, SemanticInlineMark::Underline)
        }),
        strikethrough: has_mark(&input.mark_ranges, offset, |mark| {
            matches!(mark, SemanticInlineMark::Strikethrough)
        }),
        link: has_mark(&input.mark_ranges, offset, |mark| {
            matches!(mark, SemanticInlineMark::Link(_))
        }),
        small_caps: has_mark(&input.mark_ranges, offset, |mark| {
            matches!(mark, SemanticInlineMark::SmallCaps)
        }),
        superscript: has_mark(&input.mark_ranges, offset, |mark| {
            matches!(mark, SemanticInlineMark::Superscript)
        }),
        subscript: has_mark(&input.mark_ranges, offset, |mark| {
            matches!(mark, SemanticInlineMark::Subscript)
        }),
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
        block_italic: span.and_then(|span| span.style.italic).unwrap_or(false),
        font_family: span
            .and_then(|span| span.style.font_family.as_deref())
            .map(editor_font_family)
            .unwrap_or(EditorFontFamily::Serif),
        atomic: input
            .atomic_nodes
            .iter()
            .find_map(|(candidate, kind)| (*candidate == position).then_some(*kind)),
    }
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

fn alignment_offset(
    span: &VisibleBlockSpan,
    viewport: EditorViewport,
    metrics: EditorLayoutMetrics,
    content_width: f32,
) -> f32 {
    let indents = block_indent(span, metrics)
        + points_to_pixels(span.style.right_indent_points.unwrap_or(0.0));
    let remaining = (viewport.width - metrics.inset_x * 2.0 - indents - content_width).max(0.0);
    match span.style.alignment.unwrap_or(TextAlignment::Start) {
        TextAlignment::Start | TextAlignment::Justify => 0.0,
        TextAlignment::Center => remaining * 0.5,
        TextAlignment::End => remaining,
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

fn default_font_weight(kind: Option<SemanticBlockKind>) -> u16 {
    match kind.unwrap_or(SemanticBlockKind::Paragraph) {
        // The bundled Source Serif face is regular-only. Keep reserved
        // headings on that available face; an explicit catalog weight still
        // takes precedence in `scalar_geometry`.
        SemanticBlockKind::Heading1 | SemanticBlockKind::Heading2 | SemanticBlockKind::Heading3 => {
            400
        }
        _ => 400,
    }
}

/// Returns a deterministic, font-size-aware advance for the bundled serif and
/// sans families. This is deliberately part of semantic layout rather than a
/// paint-only adjustment: selection rectangles, carets, wrapping, scrolling,
/// and Canvas now agree on proportional text geometry at every scale factor.
fn scalar_advance(
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
        .and_then(|span| span.style.font_family.as_deref())
        .map(editor_font_family)
        .unwrap_or(EditorFontFamily::Serif);
    let base = metrics.scalar_width * (font_size / 20.0);
    if family == EditorFontFamily::Monospace {
        return base;
    }

    // Source Serif 4's common Latin advances, normalized to the 9px body
    // advance at 20px. The same ratios remain a close deterministic model for
    // Source Sans 3; the monospace path above remains exact.
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

fn resolve_block_style(
    block: &parchmint_editor_api::SemanticBlock,
    catalog: &StyleCatalog,
) -> ResolvedBlockStyle {
    let style_id = block
        .paragraph_style()
        .and_then(parse_style_id)
        .unwrap_or_else(|| match block.kind() {
            SemanticBlockKind::Heading1 => StyleCatalog::heading_1_id(),
            SemanticBlockKind::Heading2 => StyleCatalog::heading_2_id(),
            SemanticBlockKind::Heading3 => StyleCatalog::heading_3_id(),
            SemanticBlockKind::BlockQuote => StyleCatalog::block_quote_id(),
            _ => StyleCatalog::body_id(),
        });
    let mut chain = Vec::new();
    let mut current = Some(style_id);
    while let Some(id) = current
        && chain.len() <= catalog.iter().count()
    {
        let Some(definition) = catalog.get(id) else {
            break;
        };
        chain.push(definition);
        current = definition.inherits;
    }
    let mut resolved = ResolvedBlockStyle::default();
    for definition in chain.into_iter().rev() {
        merge_style(&mut resolved, &definition.properties);
    }
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
    replace!(font_family);
    replace!(font_size_points);
    replace!(weight);
    replace!(italic);
    replace!(alignment);
    replace!(first_line_indent_points);
    replace!(left_indent_points);
    replace!(right_indent_points);
    replace!(line_spacing);
    replace!(space_before_points);
    replace!(space_after_points);
}

fn parse_style_id(value: &str) -> Option<StyleId> {
    let reserved = match value {
        "body" => Some(StyleCatalog::body_id()),
        "document-title" => Some(StyleCatalog::document_title_id()),
        "heading-1" => Some(StyleCatalog::heading_1_id()),
        "heading-2" => Some(StyleCatalog::heading_2_id()),
        "heading-3" => Some(StyleCatalog::heading_3_id()),
        "block-quote" => Some(StyleCatalog::block_quote_id()),
        "verse" => Some(StyleCatalog::verse_id()),
        _ => None,
    };
    if reserved.is_some() {
        return reserved;
    }
    if value.len() != 32 {
        return None;
    }
    let mut bytes = [0u8; 16];
    for (index, slot) in bytes.iter_mut().enumerate() {
        *slot = u8::from_str_radix(&value[index * 2..index * 2 + 2], 16).ok()?;
    }
    Some(StyleId::from_bytes(bytes))
}

fn has_mark(
    ranges: &[VisibleMarkRange],
    position: u64,
    predicate: impl Fn(&SemanticInlineMark) -> bool,
) -> bool {
    ranges.iter().any(|range| {
        range.range.start().value() <= position
            && position < range.range.end().value()
            && predicate(&range.mark)
    })
}

fn replace_or_push_caret(
    carets: &mut Vec<(DocumentPosition, EditorRectangle)>,
    position: DocumentPosition,
    rectangle: EditorRectangle,
) {
    if let Some((_, existing)) = carets
        .iter_mut()
        .find(|(candidate, _)| *candidate == position)
    {
        *existing = rectangle;
    } else {
        carets.push((position, rectangle));
    }
}

fn caret_rectangle(x: f32, y: f32, metrics: EditorLayoutMetrics) -> EditorRectangle {
    EditorRectangle {
        x,
        y,
        width: metrics.caret_width,
        height: metrics.line_height,
    }
}

fn distance_squared(rectangle: EditorRectangle, x: f32, y: f32) -> f32 {
    let center_x = rectangle.x + rectangle.width / 2.0;
    let center_y = rectangle.y + rectangle.height / 2.0;
    (center_x - x).powi(2) + (center_y - y).powi(2)
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
    fn nested_lists_indent_while_quotes_remain_on_the_manuscript_margin() {
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
        assert_eq!(starts[2].bounds.x, regression_metrics().inset_x);
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
    fn proportional_wrapping_stays_inside_the_manuscript_pane() {
        let metrics = regression_metrics();
        let viewport = EditorViewport::new(152.0, 140.0).expect("viewport");
        let geometry = BlockLayoutGeometry::build(
            &VisibleEditorBlock::new(
                block(13),
                "The harbor held the last of the evening light.",
                DocumentPosition::default(),
            ),
            viewport,
            0.0,
            metrics,
            None,
        )
        .expect("geometry");
        let right_edge = viewport.width - metrics.inset_x;
        let scalars = geometry.draw_scalars();
        assert!(
            scalars
                .iter()
                .any(|scalar| scalar.bounds.y > metrics.inset_y)
        );
        assert!(scalars.iter().all(|scalar| {
            scalar.bounds.x >= metrics.inset_x
                && scalar.bounds.x + scalar.bounds.width <= right_edge + f32::EPSILON
        }));
    }

    #[test]
    fn ordinary_words_move_as_a_complete_run_at_the_wrap_boundary() {
        let metrics = regression_metrics();
        let geometry = BlockLayoutGeometry::build(
            &VisibleEditorBlock::new(block(14), "one two turning", DocumentPosition::default()),
            EditorViewport::new(128.0, 120.0).expect("viewport"),
            0.0,
            metrics,
            None,
        )
        .expect("geometry");
        let scalars = geometry.draw_scalars();
        let turning = &scalars[8..];

        assert!(
            turning
                .iter()
                .all(|scalar| scalar.bounds.y == turning[0].bounds.y)
        );
        assert!(turning[0].bounds.y > scalars[7].bounds.y);
        assert_eq!(turning[0].bounds.x, metrics.inset_x);
    }

    #[test]
    fn overlong_tokens_fall_back_to_scalar_breaking_inside_the_pane() {
        let metrics = regression_metrics();
        let viewport = EditorViewport::new(70.0, 140.0).expect("viewport");
        let geometry = BlockLayoutGeometry::build(
            &VisibleEditorBlock::new(block(15), "wwwww", DocumentPosition::default()),
            viewport,
            0.0,
            metrics,
            None,
        )
        .expect("geometry");
        let scalars = geometry.draw_scalars();

        assert!(
            scalars
                .iter()
                .any(|scalar| scalar.bounds.y > metrics.inset_y)
        );
        assert!(scalars.iter().all(|scalar| {
            scalar.bounds.x + scalar.bounds.width <= viewport.width - metrics.inset_x
        }));
    }

    #[test]
    fn punctuation_stays_with_its_word_while_spaces_remain_break_opportunities() {
        let metrics = regression_metrics();
        let geometry = BlockLayoutGeometry::build(
            &VisibleEditorBlock::new(block(16), "word, next", DocumentPosition::default()),
            EditorViewport::new(100.0, 120.0).expect("viewport"),
            0.0,
            metrics,
            None,
        )
        .expect("geometry");
        let scalars = geometry.draw_scalars();

        assert_eq!(scalars[3].character, 'd');
        assert_eq!(scalars[4].character, ',');
        assert_eq!(scalars[3].bounds.y, scalars[4].bounds.y);
        assert_eq!(scalars[5].character, ' ');
        assert!(scalars[6].bounds.y > scalars[5].bounds.y);
        assert_eq!(scalars[6].bounds.x, metrics.inset_x);
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
        assert_eq!(heading.font_size, 24.0);
        assert_eq!(heading.font_weight, 400);
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
            input.layout_lines[0].chunks.len() + 1,
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
