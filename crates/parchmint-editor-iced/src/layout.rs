use parchmint_editor_api::{BlockId, DocumentPosition, EditorSelection, SelectionRectangle};

const TAB_COLUMNS: f32 = 4.0;

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
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VisibleEditorBlock {
    block: BlockId,
    text: String,
    document_start: DocumentPosition,
}

impl VisibleEditorBlock {
    pub fn new(block: BlockId, text: impl Into<String>, document_start: DocumentPosition) -> Self {
        Self {
            block,
            text: text.into(),
            document_start,
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
}

/// The one geometry object used for block drawing, hit testing, carets, and selections.
#[derive(Debug, Clone, PartialEq)]
pub struct BlockLayoutGeometry {
    block: BlockId,
    scalars: Vec<EditorScalarGeometry>,
    carets: Vec<(DocumentPosition, EditorRectangle)>,
}

impl BlockLayoutGeometry {
    pub(crate) fn build(
        input: &VisibleEditorBlock,
        viewport: EditorViewport,
        pixel_scroll_y: f32,
        metrics: EditorLayoutMetrics,
    ) -> Result<Self, &'static str> {
        metrics.validate()?;
        if !pixel_scroll_y.is_finite() || pixel_scroll_y < 0.0 {
            return Err("pixel scroll must be nonnegative and finite");
        }

        let start = input.document_start.value();
        let mut x = metrics.inset_x;
        let mut y = metrics.inset_y - pixel_scroll_y;
        let mut scalars = Vec::new();
        let mut carets = Vec::new();
        let first = DocumentPosition::from(start);
        if line_visible(y, viewport, metrics) {
            carets.push((first, caret_rectangle(x, y, metrics)));
        }

        for (offset, character) in input.text.chars().enumerate() {
            let position = start
                .checked_add(offset as u64)
                .ok_or("document position overflowed")?;
            let width = match character {
                '\n' => 0.0,
                '\t' => metrics.scalar_width * TAB_COLUMNS,
                _ => metrics.scalar_width,
            };

            if character != '\n'
                && x > metrics.inset_x
                && x + width > metrics.inset_x + viewport.width
            {
                x = metrics.inset_x;
                y += metrics.line_height;
                replace_or_push_caret(
                    &mut carets,
                    DocumentPosition::from(position),
                    caret_rectangle(x, y, metrics),
                    y,
                    viewport,
                    metrics,
                );
            }

            let bounds = EditorRectangle {
                x,
                y,
                width,
                height: metrics.line_height,
            };
            if line_visible(y, viewport, metrics) {
                scalars.push(EditorScalarGeometry {
                    position: DocumentPosition::from(position),
                    character,
                    bounds,
                });
            }
            if y > metrics.inset_y + viewport.height {
                break;
            }

            if character == '\n' {
                x = metrics.inset_x;
                y += metrics.line_height;
            } else {
                x += width;
            }
            let next = position
                .checked_add(1)
                .ok_or("document position overflowed")?;
            if line_visible(y, viewport, metrics) {
                carets.push((DocumentPosition::from(next), caret_rectangle(x, y, metrics)));
            }
        }

        Ok(Self {
            block: input.block,
            scalars,
            carets,
        })
    }

    pub const fn block(&self) -> BlockId {
        self.block
    }

    pub fn draw_scalars(&self) -> &[EditorScalarGeometry] {
        &self.scalars
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
}

fn replace_or_push_caret(
    carets: &mut Vec<(DocumentPosition, EditorRectangle)>,
    position: DocumentPosition,
    rectangle: EditorRectangle,
    y: f32,
    viewport: EditorViewport,
    metrics: EditorLayoutMetrics,
) {
    if let Some((_, existing)) = carets
        .iter_mut()
        .find(|(candidate, _)| *candidate == position)
    {
        *existing = rectangle;
    } else if line_visible(y, viewport, metrics) {
        carets.push((position, rectangle));
    }
}

fn line_visible(y: f32, viewport: EditorViewport, metrics: EditorLayoutMetrics) -> bool {
    y + metrics.line_height >= metrics.inset_y && y <= metrics.inset_y + viewport.height
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
