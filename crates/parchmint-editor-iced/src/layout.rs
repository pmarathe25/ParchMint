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
            inset_x: 16.0,
            inset_y: 16.0,
            scalar_width: 8.0,
            line_height: 20.0,
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
    tabs: Vec<usize>,
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
    let mut tabs = Vec::new();
    let mut position = document_start.value();
    let finish_line = |lines: &mut Vec<VisibleLayoutLine>,
                       line_start: u64,
                       line_text: &mut String,
                       line_scalar_len: usize,
                       tabs: &mut Vec<usize>,
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
            tabs: std::mem::take(tabs),
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
                &mut tabs,
                Some(DocumentPosition::from(position)),
            );
            position = position.saturating_add(1);
            line_start = position;
            line_scalar_len = 0;
            continue;
        }
        if character == '\t' {
            tabs.push(line_scalar_len);
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
        &mut tabs,
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
    tabs: Vec<usize>,
    start_y: f32,
    end_y: f32,
    line_height: f32,
    first_x: f32,
    right_edge: f32,
    chunk_rows: Vec<(usize, usize)>,
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
        let first_x = metrics.inset_x
            + span.map_or(0.0, |span| {
                block_indent(span, metrics) + alignment_offset(span, viewport, metrics)
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
        let mut entry = LineHeightEntry {
            line_index,
            start: line.start,
            end: line.end,
            scalar_len: line.scalar_len,
            tabs: line.tabs.clone(),
            start_y: y,
            end_y: y,
            line_height,
            first_x,
            right_edge: metrics.inset_x + viewport.width - right_indent,
            chunk_rows: Vec::with_capacity(line.chunks.len()),
        };
        for chunk in &line.chunks {
            let start = cursor_after_prefix(chunk.scalar_offset, &entry, metrics).0;
            let end = cursor_after_prefix(
                chunk.scalar_offset.saturating_add(chunk.scalar_len),
                &entry,
                metrics,
            )
            .0;
            entry.chunk_rows.push((start, end));
        }
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
    let mut row = 0_usize;
    let mut x = entry.first_x;
    let mut offset = 0_usize;
    for tab in entry
        .tabs
        .iter()
        .copied()
        .take_while(|tab| *tab < scalar_count)
    {
        advance_normal_run(
            tab.saturating_sub(offset),
            &mut row,
            &mut x,
            entry.right_edge,
            metrics,
        );
        advance_width(
            metrics.scalar_width * TAB_COLUMNS,
            &mut row,
            &mut x,
            entry.right_edge,
            metrics,
        );
        offset = tab.saturating_add(1);
    }
    advance_normal_run(
        scalar_count.saturating_sub(offset),
        &mut row,
        &mut x,
        entry.right_edge,
        metrics,
    );
    (row, x)
}

fn advance_normal_run(
    count: usize,
    row: &mut usize,
    x: &mut f32,
    right_edge: f32,
    metrics: EditorLayoutMetrics,
) {
    if count == 0 {
        return;
    }
    if *x > metrics.inset_x && *x + metrics.scalar_width > right_edge {
        *row = row.saturating_add(1);
        *x = metrics.inset_x;
    }
    let available = ((right_edge - *x) / metrics.scalar_width).floor().max(0.0) as usize;
    let fit = if *x <= metrics.inset_x {
        available.max(1)
    } else {
        available
    };
    if count <= fit {
        *x += count as f32 * metrics.scalar_width;
        return;
    }
    let remaining = count.saturating_sub(fit);
    *row = row.saturating_add(1);
    let capacity = ((right_edge - metrics.inset_x) / metrics.scalar_width)
        .floor()
        .max(1.0) as usize;
    *row = row.saturating_add((remaining - 1) / capacity);
    let final_count = (remaining - 1) % capacity + 1;
    *x = metrics.inset_x + final_count as f32 * metrics.scalar_width;
}

fn advance_width(
    width: f32,
    row: &mut usize,
    x: &mut f32,
    right_edge: f32,
    metrics: EditorLayoutMetrics,
) {
    if *x > metrics.inset_x && *x + width > right_edge {
        *row = row.saturating_add(1);
        *x = metrics.inset_x;
    }
    *x += width;
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
        let width = if character == '\t' {
            metrics.scalar_width * TAB_COLUMNS
        } else {
            metrics.scalar_width
        };
        if x > metrics.inset_x && x + width > entry.right_edge {
            row = row.saturating_add(1);
            x = metrics.inset_x;
        }
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
            .unwrap_or(16.0),
        font_weight: span.and_then(|span| span.style.weight).unwrap_or(400),
        block_italic: span.and_then(|span| span.style.italic).unwrap_or(false),
        font_family: span
            .map(|span| editor_font_family(span.style.font_family.as_deref()))
            .unwrap_or(EditorFontFamily::SansSerif),
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
        SemanticBlockKind::BlockQuote => explicit + metrics.scalar_width * 3.0,
        _ => explicit,
    }
}

fn alignment_offset(
    span: &VisibleBlockSpan,
    viewport: EditorViewport,
    metrics: EditorLayoutMetrics,
) -> f32 {
    let content_width = (span.end.value() - span.start.value()) as f32 * metrics.scalar_width;
    let indents = block_indent(span, metrics)
        + points_to_pixels(span.style.right_indent_points.unwrap_or(0.0));
    let remaining = (viewport.width - indents - content_width).max(0.0);
    match span.style.alignment.unwrap_or(TextAlignment::Start) {
        TextAlignment::Start | TextAlignment::Justify => 0.0,
        TextAlignment::Center => remaining * 0.5,
        TextAlignment::End => remaining,
    }
}

fn points_to_pixels(points: f32) -> f32 {
    points * (4.0 / 3.0)
}

fn editor_font_family(value: Option<&str>) -> EditorFontFamily {
    let value = value.unwrap_or_default().to_ascii_lowercase();
    if value.contains("mono") || value.contains("courier") {
        EditorFontFamily::Monospace
    } else if value.contains("serif") && !value.contains("sans") {
        EditorFontFamily::Serif
    } else {
        EditorFontFamily::SansSerif
    }
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
            EditorLayoutMetrics::default(),
            None,
        )
        .expect("geometry");
        let first = &geometry.draw_scalars()[0];
        assert_eq!(first.font_size, 24.0);
        assert_eq!(first.font_weight, 700);
        assert!(first.block_italic);
        assert_eq!(first.font_family, EditorFontFamily::Monospace);
        assert_eq!(first.bounds.height, 30.0);
        assert!(first.bounds.x > EditorLayoutMetrics::default().inset_x + 12.0);
        assert!(first.small_caps);
        assert!(geometry.draw_scalars()[1].superscript);
        assert!(geometry.draw_scalars()[2].subscript);
    }

    #[test]
    fn nested_list_and_quote_blocks_have_visible_deterministic_indentation() {
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
            EditorLayoutMetrics::default(),
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
        assert!(starts[2].bounds.x > EditorLayoutMetrics::default().inset_x);
    }

    #[test]
    fn retained_carets_support_vertical_line_edge_and_bounded_scroll_geometry() {
        let viewport = EditorViewport::new(200.0, 20.0).expect("viewport");
        let geometry = BlockLayoutGeometry::build(
            &VisibleEditorBlock::new(block(9), "abc\ndef", DocumentPosition::default()),
            viewport,
            0.0,
            EditorLayoutMetrics::default(),
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
    fn thousands_of_lines_materialize_only_the_visible_window_and_reuse_heights_on_scroll() {
        let text = vec!["paragraph"; 5_000].join("\n");
        let input = VisibleEditorBlock::new(block(10), text, DocumentPosition::default());
        let viewport = EditorViewport::new(400.0, 200.0).expect("viewport");
        let first =
            BlockLayoutGeometry::build(&input, viewport, 0.0, EditorLayoutMetrics::default(), None)
                .expect("first viewport");

        assert!(first.layout_work().materialized_lines <= 24);
        assert!(first.layout_work().materialized_chunks <= 24);
        assert!(first.layout_work().materialized_scalars <= 240);
        assert!(first.max_scroll_y(viewport) > 50_000.0);

        let scrolled = BlockLayoutGeometry::build(
            &input,
            viewport,
            40_000.0,
            EditorLayoutMetrics::default(),
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
        let geometry = BlockLayoutGeometry::build(
            &input,
            viewport,
            100_000.0,
            EditorLayoutMetrics::default(),
            None,
        )
        .expect("deep huge-paragraph viewport");

        let work = geometry.layout_work();
        assert_eq!(work.materialized_lines, 1);
        assert!(work.materialized_chunks <= 2);
        assert!(work.materialized_scalars <= 450);
        assert_eq!(geometry.document_range().end().value(), scalar_count);
        let first = geometry.draw_scalars().first().expect("visible scalar");
        assert!(first.position.value() > 100_000);
        assert!(first.bold);
        let selection = EditorSelection::new(
            first.position,
            DocumentPosition::from(first.position.value() + 5),
        );
        assert_eq!(geometry.selection_rectangles(selection).len(), 5);
    }
}
