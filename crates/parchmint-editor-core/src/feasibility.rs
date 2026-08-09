//! Framework-neutral Stage 32 editor feasibility prototype.
//!
//! This module deliberately describes widget inputs and outputs without
//! depending on a GUI runtime. It lets a later `iced` host use one layout for
//! drawing, hit-testing, carets, and selections while the editor session keeps
//! all document authority.

use std::{collections::BTreeMap, error::Error, fmt, mem, time::Duration};

use crate::{
    AppliedEditorChange, BlockId, CanonicalDocumentLoad, CanonicalProjection, DocumentPosition,
    EditorCommand, EditorCommandKind, EditorCommandOrigin, EditorCoreSession, EditorError,
    EditorRevision, EditorSelection, TransactionId, ViewId,
};

const TAB_COLUMNS: f32 = 4.0;

/// The Stage 32 gate outcome supported by current evidence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GateStatus {
    /// The ParchMint core or deterministic prototype exercises this contract.
    Passed,
    /// The prototype exercises the contract, but a real host must still be measured.
    PrototypePassedRuntimeRequired,
    /// No GUI/runtime evidence exists, so this mandatory gate remains open.
    RuntimeMeasurementRequired,
    /// Optional work is intentionally postponed until evidence requires it.
    Deferred,
    /// The product requirements explicitly exclude this gate from v1.
    Excluded,
}

/// One durable Stage 32 gate assessment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GateAssessment {
    id: &'static str,
    status: GateStatus,
    evidence: &'static str,
}

impl GateAssessment {
    pub const fn id(self) -> &'static str {
        self.id
    }

    pub const fn status(self) -> GateStatus {
        self.status
    }

    pub const fn evidence(self) -> &'static str {
        self.evidence
    }
}

/// The only safe delivery decision while mandatory runtime gates are open.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FeasibilityDecision {
    /// Keep the prototype, but do not begin broad editor delivery.
    HoldBroadEditorDelivery,
}

/// Evaluation of an optional document-engine candidate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EngineCandidateAssessment {
    pub package: &'static str,
    pub version: &'static str,
    pub evaluated: bool,
    pub adopted: bool,
    pub rationale: &'static str,
}

/// No current evidence requires evaluating or adopting `text-document`.
pub const TEXT_DOCUMENT_1_8_ASSESSMENT: EngineCandidateAssessment = EngineCandidateAssessment {
    package: "text-document",
    version: "1.8.0",
    evaluated: false,
    adopted: false,
    rationale: "await a measured iced deficiency before considering another document engine",
};

/// Durable Stage 32 result. Runtime-dependent gates are intentionally not
/// promoted to passes by this framework-neutral prototype.
pub const STAGE32_FEASIBILITY_DECISION: FeasibilityDecision =
    FeasibilityDecision::HoldBroadEditorDelivery;

/// Complete current-scope gate inventory.
pub const STAGE32_GATE_ASSESSMENTS: &[GateAssessment] = &[
    GateAssessment {
        id: "views.shared_document_undo",
        status: GateStatus::Passed,
        evidence: "EditorCoreSession owns one body and undo stack for all attached views",
    },
    GateAssessment {
        id: "views.independent_presentation",
        status: GateStatus::PrototypePassedRuntimeRequired,
        evidence: "view descriptors isolate selection, scroll, viewport, search, and focus",
    },
    GateAssessment {
        id: "input.en_us_and_paste",
        status: GateStatus::PrototypePassedRuntimeRequired,
        evidence: "deterministic en-US input and sanitizing paste descriptors feed core commands",
    },
    GateAssessment {
        id: "layout.geometry_consistency",
        status: GateStatus::PrototypePassedRuntimeRequired,
        evidence: "draw, hit-test, caret, and selection geometry share one scalar layout",
    },
    GateAssessment {
        id: "projection.affected_block_invalidation",
        status: GateStatus::Passed,
        evidence: "widget updates expose only core-reported changed BlockIds",
    },
    GateAssessment {
        id: "canonical.fidelity",
        status: GateStatus::Passed,
        evidence: "all mutations project through EditorCoreSession",
    },
    GateAssessment {
        id: "recovery.replay",
        status: GateStatus::Passed,
        evidence: "successful core commands replay from the canonical load and preserve revision output",
    },
    GateAssessment {
        id: "budgets.latency_memory",
        status: GateStatus::RuntimeMeasurementRequired,
        evidence: "key-to-paint and process memory remain unmeasured by real iced runners on Windows, macOS, and Linux",
    },
    GateAssessment {
        id: "lifecycle.failure_behavior",
        status: GateStatus::PrototypePassedRuntimeRequired,
        evidence: "bounded attach, detach, stale-command, invalid-input, and resource descriptors exist",
    },
    GateAssessment {
        id: "engine.private_document_engine",
        status: GateStatus::Passed,
        evidence: "the prototype reaches text storage only through EditorCoreSession",
    },
    GateAssessment {
        id: "engine.optional_text_typeset",
        status: GateStatus::Deferred,
        evidence: "there is no measured iced layout deficiency yet",
    },
    GateAssessment {
        id: "scope.stop_on_mandatory_failure",
        status: GateStatus::Passed,
        evidence: "the decision holds broad delivery while runtime gates remain open",
    },
    GateAssessment {
        id: "scope.ime_multilingual_bidi_screen_reader",
        status: GateStatus::Excluded,
        evidence: "explicitly outside the v1 Stage 32 gate",
    },
];

/// Host-independent prototype failure.
#[derive(Debug, Clone, PartialEq)]
pub enum PrototypeError {
    Editor(EditorError),
    InvalidGeometry(&'static str),
    InvalidInput(&'static str),
    RecoveryMismatch,
}

impl fmt::Display for PrototypeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Editor(error) => write!(formatter, "{error}"),
            Self::InvalidGeometry(reason) => write!(formatter, "invalid widget geometry: {reason}"),
            Self::InvalidInput(reason) => write!(formatter, "invalid editor input: {reason}"),
            Self::RecoveryMismatch => {
                formatter.write_str("recovery replay did not match the canonical projection")
            }
        }
    }
}

impl Error for PrototypeError {}

impl From<EditorError> for PrototypeError {
    fn from(value: EditorError) -> Self {
        Self::Editor(value)
    }
}

/// Rectangle in logical host coordinates.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LogicalRectangle {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl LogicalRectangle {
    fn is_valid(self) -> bool {
        self.x.is_finite()
            && self.y.is_finite()
            && self.width.is_finite()
            && self.height.is_finite()
            && self.width >= 0.0
            && self.height >= 0.0
    }
}

/// Independent viewport state supplied by one mounted host view.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ViewportDescriptor {
    pub width: f32,
    pub height: f32,
    pub scroll_y: f32,
}

impl ViewportDescriptor {
    pub fn new(width: f32, height: f32, scroll_y: f32) -> Result<Self, PrototypeError> {
        if !width.is_finite() || width <= 0.0 {
            return Err(PrototypeError::InvalidGeometry(
                "viewport width must be positive and finite",
            ));
        }
        if !height.is_finite() || height <= 0.0 {
            return Err(PrototypeError::InvalidGeometry(
                "viewport height must be positive and finite",
            ));
        }
        if !scroll_y.is_finite() || scroll_y < 0.0 {
            return Err(PrototypeError::InvalidGeometry(
                "scroll offset must be nonnegative and finite",
            ));
        }
        Ok(Self {
            width,
            height,
            scroll_y,
        })
    }
}

/// View-local search state and deterministic scalar ranges.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchDescriptor {
    query: String,
    matches: Vec<EditorSelection>,
}

impl SearchDescriptor {
    pub fn query(&self) -> &str {
        &self.query
    }

    pub fn matches(&self) -> &[EditorSelection] {
        &self.matches
    }
}

/// Complete framework-neutral state for one mounted editor widget.
#[derive(Debug, Clone, PartialEq)]
pub struct EditorViewDescriptor {
    view: ViewId,
    selection: EditorSelection,
    viewport: ViewportDescriptor,
    focused: bool,
    search: SearchDescriptor,
}

impl EditorViewDescriptor {
    pub const fn view(&self) -> ViewId {
        self.view
    }

    pub const fn selection(&self) -> EditorSelection {
        self.selection
    }

    pub const fn viewport(&self) -> ViewportDescriptor {
        self.viewport
    }

    pub const fn focused(&self) -> bool {
        self.focused
    }

    pub const fn search(&self) -> &SearchDescriptor {
        &self.search
    }
}

/// Supported formatting retained as a paste descriptor.
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

/// Sanitized clipboard content. ParchMint can apply the text now and retain
/// the mark descriptors for the formatting implementation stage.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SanitizedPaste {
    text: String,
    marks: Vec<PasteMark>,
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

    pub const fn unsafe_content_removed(&self) -> bool {
        self.unsafe_content_removed
    }

    pub const fn omitted_images(&self) -> usize {
        self.omitted_images
    }
}

/// Clipboard source used by the prototype.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PasteSource<'a> {
    PlainText(&'a str),
    RichHtml(&'a str),
}

/// Host redraw information produced by one document mutation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WidgetUpdate {
    revision: EditorRevision,
    transaction: Option<TransactionId>,
    invalidated_blocks: Vec<BlockId>,
    redraw_views: Vec<ViewId>,
}

impl WidgetUpdate {
    pub const fn revision(&self) -> EditorRevision {
        self.revision
    }

    pub const fn transaction(&self) -> Option<TransactionId> {
        self.transaction
    }

    pub fn invalidated_blocks(&self) -> &[BlockId] {
        &self.invalidated_blocks
    }

    pub fn redraw_views(&self) -> &[ViewId] {
        &self.redraw_views
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct JournalCommand {
    origin: ViewId,
    kind: EditorCommandKind,
}

/// Replayable recovery evidence built only from successful core commands.
#[derive(Debug, Clone, PartialEq)]
pub struct RecoveryRecord {
    initial: CanonicalDocumentLoad,
    commands: Vec<JournalCommand>,
    expected: CanonicalProjection,
}

impl RecoveryRecord {
    pub const fn expected_revision(&self) -> EditorRevision {
        self.expected.revision()
    }

    pub fn replay(&self) -> Result<CanonicalProjection, PrototypeError> {
        let mut session = EditorCoreSession::open(self.initial.clone())?;
        let mut attached = Vec::new();
        for entry in &self.commands {
            if !attached.contains(&entry.origin) {
                session.attach_view(entry.origin)?;
                attached.push(entry.origin);
            }
            session.execute(
                EditorCommandOrigin::new(entry.origin),
                EditorCommand::new(session.revision(), entry.kind.clone()),
            )?;
        }
        let recovered = session.canonical_projection();
        if recovered != self.expected {
            return Err(PrototypeError::RecoveryMismatch);
        }
        Ok(recovered)
    }
}

/// Shared custom-widget prototype over the authoritative editor session.
pub struct EditorWidgetPrototype {
    session: EditorCoreSession,
    initial: CanonicalDocumentLoad,
    views: BTreeMap<ViewId, EditorViewDescriptor>,
    journal: Vec<JournalCommand>,
}

impl EditorWidgetPrototype {
    pub fn open(load: CanonicalDocumentLoad) -> Result<Self, PrototypeError> {
        Ok(Self {
            session: EditorCoreSession::open(load.clone())?,
            initial: load,
            views: BTreeMap::new(),
            journal: Vec::new(),
        })
    }

    pub fn mount_view(
        &mut self,
        view: ViewId,
        viewport: ViewportDescriptor,
    ) -> Result<(), PrototypeError> {
        self.session.attach_view(view)?;
        self.views.insert(
            view,
            EditorViewDescriptor {
                view,
                selection: EditorSelection::default(),
                viewport,
                focused: false,
                search: SearchDescriptor {
                    query: String::new(),
                    matches: Vec::new(),
                },
            },
        );
        Ok(())
    }

    pub fn unmount_view(&mut self, view: ViewId) -> Result<EditorViewDescriptor, PrototypeError> {
        self.session.detach_view(view)?;
        self.views
            .remove(&view)
            .ok_or_else(|| EditorError::UnknownView { view }.into())
    }

    pub fn view(&self, view: ViewId) -> Result<&EditorViewDescriptor, PrototypeError> {
        self.views
            .get(&view)
            .ok_or_else(|| EditorError::UnknownView { view }.into())
    }

    pub fn set_viewport(
        &mut self,
        view: ViewId,
        viewport: ViewportDescriptor,
    ) -> Result<(), PrototypeError> {
        self.view_mut(view)?.viewport = viewport;
        Ok(())
    }

    pub fn focus(&mut self, view: ViewId) -> Result<(), PrototypeError> {
        if !self.views.contains_key(&view) {
            return Err(EditorError::UnknownView { view }.into());
        }
        for descriptor in self.views.values_mut() {
            descriptor.focused = descriptor.view == view;
        }
        Ok(())
    }

    pub fn set_search(
        &mut self,
        view: ViewId,
        query: impl Into<String>,
    ) -> Result<(), PrototypeError> {
        let query = query.into();
        let matches = search_ranges(self.session.text_for_feasibility(), &query);
        self.view_mut(view)?.search = SearchDescriptor { query, matches };
        Ok(())
    }

    pub fn set_selection(
        &mut self,
        view: ViewId,
        selection: EditorSelection,
    ) -> Result<(), PrototypeError> {
        self.session.execute(
            EditorCommandOrigin::new(view),
            EditorCommand::new(
                self.session.revision(),
                EditorCommandKind::SetSelection { selection },
            ),
        )?;
        self.refresh_view_state()?;
        Ok(())
    }

    /// Applies printable ASCII, newline, or literal tab input as one core command.
    pub fn input_en_us(
        &mut self,
        view: ViewId,
        text: &str,
    ) -> Result<WidgetUpdate, PrototypeError> {
        if text.is_empty() {
            return Err(PrototypeError::InvalidInput("text input must not be empty"));
        }
        if !text
            .chars()
            .all(|character| character.is_ascii_graphic() || matches!(character, ' ' | '\n' | '\t'))
        {
            return Err(PrototypeError::InvalidInput(
                "the Stage 32 input gate accepts en-US text, newline, and tab",
            ));
        }
        self.replace_selection(view, text)
    }

    /// Sanitizes and inserts clipboard content. Images are omitted and reported.
    pub fn paste(
        &mut self,
        view: ViewId,
        source: PasteSource<'_>,
    ) -> Result<(SanitizedPaste, WidgetUpdate), PrototypeError> {
        let sanitized = sanitize_paste(source);
        let update = if sanitized.text().is_empty() {
            self.view(view)?;
            WidgetUpdate {
                revision: self.session.revision(),
                transaction: None,
                invalidated_blocks: Vec::new(),
                redraw_views: Vec::new(),
            }
        } else {
            self.replace_selection(view, sanitized.text())?
        };
        Ok((sanitized, update))
    }

    pub fn undo(&mut self, view: ViewId) -> Result<WidgetUpdate, PrototypeError> {
        self.execute_document(view, EditorCommandKind::Undo)
    }

    pub fn redo(&mut self, view: ViewId) -> Result<WidgetUpdate, PrototypeError> {
        self.execute_document(view, EditorCommandKind::Redo)
    }

    pub fn projection(&self) -> CanonicalProjection {
        self.session.canonical_projection()
    }

    pub fn layout(
        &self,
        view: ViewId,
        metrics: LayoutMetrics,
    ) -> Result<LayoutDescriptor, PrototypeError> {
        let descriptor = self.view(view)?;
        LayoutDescriptor::build(
            self.session.primary_block(),
            self.session.text_for_feasibility(),
            descriptor.viewport,
            metrics,
        )
    }

    pub fn recovery_record(&self) -> RecoveryRecord {
        RecoveryRecord {
            initial: self.initial.clone(),
            commands: self.journal.clone(),
            expected: self.session.canonical_projection(),
        }
    }

    /// A deterministic lower-bound estimate for resources owned by prototype
    /// descriptors. A platform runner must use process/allocator measurements
    /// for the actual memory gate.
    pub fn estimated_descriptor_bytes(&self) -> usize {
        let view_bytes = self
            .views
            .values()
            .map(|view| {
                mem::size_of::<EditorViewDescriptor>()
                    + view.search.query.capacity()
                    + view.search.matches.capacity() * mem::size_of::<EditorSelection>()
            })
            .sum::<usize>();
        self.session.text_for_feasibility().len()
            + view_bytes
            + self.journal.capacity() * mem::size_of::<JournalCommand>()
    }

    /// Consumes the prototype and reports the deterministic descriptor release.
    pub fn close(self) -> LifecycleMeasurement {
        LifecycleMeasurement {
            before_close_bytes: self.estimated_descriptor_bytes(),
            after_close_bytes: 0,
            released_views: self.views.len(),
        }
    }

    fn replace_selection(
        &mut self,
        view: ViewId,
        text: &str,
    ) -> Result<WidgetUpdate, PrototypeError> {
        let selection = self.view(view)?.selection;
        let start = selection.start();
        let kind = if selection.is_collapsed() {
            EditorCommandKind::InsertText {
                at: start,
                text: text.to_owned(),
            }
        } else {
            EditorCommandKind::ReplaceRange {
                range: selection,
                text: text.to_owned(),
            }
        };
        let update = self.execute_document(view, kind)?;
        let inserted = u64::try_from(text.chars().count())
            .map_err(|_| PrototypeError::InvalidInput("inserted text is too large"))?;
        let caret = start
            .value()
            .checked_add(inserted)
            .ok_or(PrototypeError::InvalidInput(
                "inserted text position overflowed",
            ))?;
        self.set_selection(
            view,
            EditorSelection::new(DocumentPosition::from(caret), DocumentPosition::from(caret)),
        )?;
        Ok(update)
    }

    fn execute_document(
        &mut self,
        view: ViewId,
        kind: EditorCommandKind,
    ) -> Result<WidgetUpdate, PrototypeError> {
        let applied = self.session.execute(
            EditorCommandOrigin::new(view),
            EditorCommand::new(self.session.revision(), kind.clone()),
        )?;
        self.journal.push(JournalCommand { origin: view, kind });
        self.refresh_view_state()?;
        Ok(self.widget_update(applied))
    }

    fn widget_update(&self, applied: AppliedEditorChange) -> WidgetUpdate {
        WidgetUpdate {
            revision: applied.revision(),
            transaction: applied.transaction(),
            invalidated_blocks: applied.changed_blocks().to_vec(),
            redraw_views: self.views.keys().copied().collect(),
        }
    }

    fn refresh_view_state(&mut self) -> Result<(), PrototypeError> {
        let body = self.session.text_for_feasibility();
        for (view, descriptor) in &mut self.views {
            descriptor.selection = self.session.selection(*view)?;
            descriptor.search.matches = search_ranges(body, &descriptor.search.query);
        }
        Ok(())
    }

    fn view_mut(&mut self, view: ViewId) -> Result<&mut EditorViewDescriptor, PrototypeError> {
        self.views
            .get_mut(&view)
            .ok_or_else(|| EditorError::UnknownView { view }.into())
    }
}

/// Deterministic text metrics supplied by a host's single layout pass.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LayoutMetrics {
    pub origin_x: f32,
    pub origin_y: f32,
    pub scalar_width: f32,
    pub line_height: f32,
    pub caret_width: f32,
}

/// One drawable scalar. Newline descriptors have zero width.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DrawScalar {
    pub position: DocumentPosition,
    pub character: char,
    pub bounds: LogicalRectangle,
}

/// A single-source layout used for draw, hit-test, caret, and selection.
#[derive(Debug, Clone, PartialEq)]
pub struct LayoutDescriptor {
    block: BlockId,
    scalars: Vec<DrawScalar>,
    carets: Vec<(DocumentPosition, LogicalRectangle)>,
}

impl LayoutDescriptor {
    fn build(
        block: BlockId,
        text: &str,
        viewport: ViewportDescriptor,
        metrics: LayoutMetrics,
    ) -> Result<Self, PrototypeError> {
        validate_metrics(metrics)?;
        let mut x = metrics.origin_x;
        let mut y = metrics.origin_y - viewport.scroll_y;
        let estimated_visible_scalars = ((viewport.width / metrics.scalar_width).ceil() as usize)
            .saturating_mul((viewport.height / metrics.line_height).ceil() as usize + 2);
        let mut scalars = Vec::with_capacity(estimated_visible_scalars);
        let mut carets = Vec::with_capacity(estimated_visible_scalars.saturating_add(1));
        if line_is_visible(y, viewport, metrics) {
            carets.push((DocumentPosition::default(), caret_rectangle(x, y, metrics)));
        }

        for (index, character) in text.chars().enumerate() {
            let width = if character == '\n' {
                0.0
            } else if character == '\t' {
                metrics.scalar_width * TAB_COLUMNS
            } else {
                metrics.scalar_width
            };
            if character != '\n'
                && x > metrics.origin_x
                && x + width > metrics.origin_x + viewport.width
            {
                x = metrics.origin_x;
                y += metrics.line_height;
                if let Some((position, caret)) = carets.last_mut()
                    && position.value() == index as u64
                {
                    *caret = caret_rectangle(x, y, metrics);
                } else if line_is_visible(y, viewport, metrics) {
                    carets.push((
                        DocumentPosition::from(index as u64),
                        caret_rectangle(x, y, metrics),
                    ));
                }
            }
            let bounds = LogicalRectangle {
                x,
                y,
                width,
                height: metrics.line_height,
            };
            if line_is_visible(y, viewport, metrics) {
                scalars.push(DrawScalar {
                    position: DocumentPosition::from(index as u64),
                    character,
                    bounds,
                });
            }
            if y >= metrics.origin_y + viewport.height {
                break;
            }
            if character == '\n' {
                x = metrics.origin_x;
                y += metrics.line_height;
            } else {
                x += width;
            }
            if line_is_visible(y, viewport, metrics) {
                carets.push((
                    DocumentPosition::from(index as u64 + 1),
                    caret_rectangle(x, y, metrics),
                ));
            }
        }

        debug_assert!(scalars.iter().all(|scalar| scalar.bounds.is_valid()));
        debug_assert!(carets.iter().all(|(_, caret)| caret.is_valid()));
        Ok(Self {
            block,
            scalars,
            carets,
        })
    }

    pub const fn block(&self) -> BlockId {
        self.block
    }

    pub fn draw_scalars(&self) -> &[DrawScalar] {
        &self.scalars
    }

    pub fn caret(&self, position: DocumentPosition) -> Option<LogicalRectangle> {
        self.carets
            .iter()
            .find(|(candidate, _)| *candidate == position)
            .map(|(_, caret)| *caret)
    }

    pub fn hit_test(&self, x: f32, y: f32) -> Option<DocumentPosition> {
        if !x.is_finite() || !y.is_finite() {
            return None;
        }
        self.carets
            .iter()
            .min_by(|(_, left), (_, right)| {
                point_distance_squared(*left, x, y).total_cmp(&point_distance_squared(*right, x, y))
            })
            .map(|(position, _)| *position)
    }

    pub fn selection_rectangles(&self, selection: EditorSelection) -> Vec<LogicalRectangle> {
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

/// Current supported desktop targets for host measurement input.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum DesktopPlatform {
    Windows,
    MacOs,
    Linux,
}

/// One host-observed key-to-paint sample.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LatencySample {
    pub key_to_paint: Duration,
    pub longest_ui_turn: Duration,
}

/// One repeated lifecycle memory sample from a platform runner.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MemoryCycleSample {
    pub before_open_bytes: usize,
    pub after_open_bytes: usize,
    pub after_close_bytes: usize,
}

/// Measurements reported by one real iced platform runner.
///
/// Constructing this descriptor does not itself provide runtime evidence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlatformMeasurements {
    platform: DesktopPlatform,
    latency: Vec<LatencySample>,
    warm_viewport: Duration,
    memory_cycles: Vec<MemoryCycleSample>,
}

impl PlatformMeasurements {
    pub fn new(
        platform: DesktopPlatform,
        latency: Vec<LatencySample>,
        warm_viewport: Duration,
        memory_cycles: Vec<MemoryCycleSample>,
    ) -> Self {
        Self {
            platform,
            latency,
            warm_viewport,
            memory_cycles,
        }
    }

    pub const fn platform(&self) -> DesktopPlatform {
        self.platform
    }

    pub fn key_to_paint_p95(&self) -> Option<Duration> {
        percentile(self.latency.iter().map(|sample| sample.key_to_paint), 95)
    }

    pub fn key_to_paint_p99(&self) -> Option<Duration> {
        percentile(self.latency.iter().map(|sample| sample.key_to_paint), 99)
    }

    pub fn passes_latency_gate(&self) -> bool {
        self.key_to_paint_p95()
            .is_some_and(|value| value <= Duration::from_millis(16))
            && self
                .key_to_paint_p99()
                .is_some_and(|value| value <= Duration::from_millis(33))
            && self
                .latency
                .iter()
                .all(|sample| sample.longest_ui_turn <= Duration::from_millis(2))
            && self.warm_viewport <= Duration::from_millis(250)
    }

    pub fn memory_stabilizes(&self) -> bool {
        let Some(last) = self.memory_cycles.last() else {
            return false;
        };
        self.memory_cycles.len() >= 3
            && last.after_close_bytes <= last.before_open_bytes
            && self
                .memory_cycles
                .windows(2)
                .all(|pair| pair[1].after_close_bytes <= pair[0].after_close_bytes)
    }
}

/// Deterministic lifecycle accounting for descriptor ownership.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LifecycleMeasurement {
    pub before_close_bytes: usize,
    pub after_close_bytes: usize,
    pub released_views: usize,
}

/// True only after all three desktop hosts pass mandatory latency and memory gates.
pub fn all_platform_budgets_pass(measurements: &[PlatformMeasurements]) -> bool {
    [
        DesktopPlatform::Windows,
        DesktopPlatform::MacOs,
        DesktopPlatform::Linux,
    ]
    .into_iter()
    .all(|platform| {
        measurements.iter().any(|measurement| {
            measurement.platform == platform
                && measurement.passes_latency_gate()
                && measurement.memory_stabilizes()
        })
    })
}

/// Sanitizes clipboard input without letting markup reach the editor core.
pub fn sanitize_paste(source: PasteSource<'_>) -> SanitizedPaste {
    match source {
        PasteSource::PlainText(text) => SanitizedPaste {
            text: normalize_newlines(text),
            marks: Vec::new(),
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
        match (closing, name) {
            (false, "img") => omitted_images += 1,
            (_, "br") => push_line_break(&mut text),
            (false, "p" | "div" | "h1" | "h2" | "h3" | "blockquote") => {
                push_paragraph_break(&mut text)
            }
            (true, "p" | "div" | "h1" | "h2" | "h3" | "blockquote") => {
                push_paragraph_break(&mut text)
            }
            (false, "strong" | "b") => {
                open_marks.push((name.into(), text.chars().count(), PasteMarkKind::Bold))
            }
            (false, "em" | "i") => {
                open_marks.push((name.into(), text.chars().count(), PasteMarkKind::Italic))
            }
            (false, "u") => {
                open_marks.push((name.into(), text.chars().count(), PasteMarkKind::Underline))
            }
            (false, "s" | "strike" | "del") => open_marks.push((
                name.into(),
                text.chars().count(),
                PasteMarkKind::Strikethrough,
            )),
            (false, "a") => {
                if let Some(link) = safe_href(raw_tag) {
                    open_marks.push((name.into(), text.chars().count(), PasteMarkKind::Link(link)));
                } else {
                    unsafe_content_removed = true;
                }
            }
            (true, "strong" | "b" | "em" | "i" | "u" | "s" | "strike" | "del" | "a") => {
                close_mark(name, text.chars().count(), &mut open_marks, &mut marks)
            }
            _ => {
                if raw_tag.contains('=') {
                    unsafe_content_removed = true;
                }
            }
        }
        cursor = end + 1;
    }

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

    SanitizedPaste {
        text: trimmed,
        marks,
        unsafe_content_removed,
        omitted_images,
    }
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

fn search_ranges(body: &str, query: &str) -> Vec<EditorSelection> {
    if query.is_empty() {
        return Vec::new();
    }
    body.match_indices(query)
        .map(|(byte_start, matched)| {
            let start = body[..byte_start].chars().count() as u64;
            let end = start + matched.chars().count() as u64;
            EditorSelection::new(DocumentPosition::from(start), DocumentPosition::from(end))
        })
        .collect()
}

fn validate_metrics(metrics: LayoutMetrics) -> Result<(), PrototypeError> {
    if !metrics.origin_x.is_finite() || !metrics.origin_y.is_finite() {
        return Err(PrototypeError::InvalidGeometry(
            "layout origin must be finite",
        ));
    }
    if !metrics.scalar_width.is_finite() || metrics.scalar_width <= 0.0 {
        return Err(PrototypeError::InvalidGeometry(
            "scalar width must be positive and finite",
        ));
    }
    if !metrics.line_height.is_finite() || metrics.line_height <= 0.0 {
        return Err(PrototypeError::InvalidGeometry(
            "line height must be positive and finite",
        ));
    }
    if !metrics.caret_width.is_finite() || metrics.caret_width <= 0.0 {
        return Err(PrototypeError::InvalidGeometry(
            "caret width must be positive and finite",
        ));
    }
    Ok(())
}

fn caret_rectangle(x: f32, y: f32, metrics: LayoutMetrics) -> LogicalRectangle {
    LogicalRectangle {
        x,
        y,
        width: metrics.caret_width,
        height: metrics.line_height,
    }
}

fn line_is_visible(y: f32, viewport: ViewportDescriptor, metrics: LayoutMetrics) -> bool {
    y + metrics.line_height >= metrics.origin_y && y <= metrics.origin_y + viewport.height
}

fn point_distance_squared(rectangle: LogicalRectangle, x: f32, y: f32) -> f32 {
    let center_x = rectangle.x + rectangle.width / 2.0;
    let center_y = rectangle.y + rectangle.height / 2.0;
    (center_x - x).powi(2) + (center_y - y).powi(2)
}

fn percentile(values: impl Iterator<Item = Duration>, percentile: usize) -> Option<Duration> {
    let mut values = values.collect::<Vec<_>>();
    if values.is_empty() {
        return None;
    }
    values.sort_unstable();
    let rank = (percentile * values.len()).div_ceil(100).saturating_sub(1);
    values.get(rank).copied()
}

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use super::*;
    use crate::{CanonicalAnchor, CanonicalComment, CommentId, DocumentId};

    fn document(value: u8) -> DocumentId {
        DocumentId::from_bytes([value; 16])
    }

    fn view(value: u8) -> ViewId {
        ViewId::from_bytes([value; 16])
    }

    fn selection(anchor: u64, head: u64) -> EditorSelection {
        EditorSelection::new(DocumentPosition::from(anchor), DocumentPosition::from(head))
    }

    fn viewport(scroll_y: f32) -> ViewportDescriptor {
        ViewportDescriptor::new(240.0, 160.0, scroll_y).expect("valid viewport")
    }

    fn metrics() -> LayoutMetrics {
        LayoutMetrics {
            origin_x: 10.0,
            origin_y: 20.0,
            scalar_width: 8.0,
            line_height: 16.0,
            caret_width: 1.0,
        }
    }

    #[test]
    fn two_views_share_edits_and_undo_while_presentation_stays_independent() {
        let left = view(1);
        let right = view(2);
        let mut prototype =
            EditorWidgetPrototype::open(CanonicalDocumentLoad::new(document(32), "alpha alpha"))
                .expect("open prototype");
        prototype
            .mount_view(left, viewport(0.0))
            .expect("mount left");
        prototype
            .mount_view(right, viewport(80.0))
            .expect("mount right");
        prototype.focus(left).expect("focus left");
        prototype.set_search(left, "alpha").expect("search left");
        prototype
            .set_search(right, "missing")
            .expect("search right");
        prototype
            .set_selection(left, selection(5, 5))
            .expect("left caret");
        prototype
            .set_selection(right, selection(0, 5))
            .expect("right selection");

        let update = prototype.input_en_us(left, "!").expect("type");
        assert_eq!(prototype.projection().body(), "alpha! alpha");
        assert_eq!(
            update.invalidated_blocks(),
            &[prototype.session.primary_block()]
        );
        assert_eq!(update.redraw_views(), &[left, right]);
        assert_ne!(
            prototype.view(left).expect("left").selection(),
            prototype.view(right).expect("right").selection()
        );
        assert_ne!(
            prototype.view(left).expect("left").viewport(),
            prototype.view(right).expect("right").viewport()
        );
        assert_ne!(
            prototype.view(left).expect("left").search(),
            prototype.view(right).expect("right").search()
        );
        assert!(prototype.view(left).expect("left").focused());
        assert!(!prototype.view(right).expect("right").focused());

        prototype.undo(right).expect("shared undo");
        assert_eq!(prototype.projection().body(), "alpha alpha");
    }

    #[test]
    fn paste_sanitizes_unsafe_content_retains_marks_and_reports_images() {
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
            sanitize_paste(PasteSource::PlainText("one\r\n\r\ntwo")).text(),
            "one\n\ntwo"
        );

        let mounted = view(1);
        let mut prototype =
            EditorWidgetPrototype::open(CanonicalDocumentLoad::new(document(32), "alpha"))
                .expect("open prototype");
        prototype.mount_view(mounted, viewport(0.0)).expect("mount");
        let (image_only, update) = prototype
            .paste(mounted, PasteSource::RichHtml("<img src=x>"))
            .expect("omit image");
        assert_eq!(image_only.omitted_images(), 1);
        assert_eq!(update.transaction(), None);
        assert_eq!(prototype.projection().body(), "alpha");
    }

    #[test]
    fn one_layout_is_authoritative_for_draw_hit_test_caret_and_selection() {
        let mounted = view(1);
        let mut prototype =
            EditorWidgetPrototype::open(CanonicalDocumentLoad::new(document(32), "abcd\nef"))
                .expect("open prototype");
        prototype.mount_view(mounted, viewport(0.0)).expect("mount");
        let layout = prototype.layout(mounted, metrics()).expect("layout");

        let draw = layout.draw_scalars()[2];
        assert_eq!(
            layout.hit_test(draw.bounds.x, draw.bounds.y + 8.0),
            Some(draw.position)
        );
        assert_eq!(layout.caret(draw.position).expect("caret").x, draw.bounds.x);
        assert_eq!(
            layout.selection_rectangles(selection(2, 3)),
            vec![draw.bounds]
        );
        assert_eq!(layout.block(), prototype.session.primary_block());
    }

    #[test]
    fn layout_work_is_viewport_bounded_and_wrap_carets_use_the_draw_layout() {
        let mounted = view(1);
        let body = "word ".repeat(250_000);
        let mut prototype =
            EditorWidgetPrototype::open(CanonicalDocumentLoad::new(document(32), body))
                .expect("open prototype");
        prototype.mount_view(mounted, viewport(0.0)).expect("mount");
        let layout = prototype.layout(mounted, metrics()).expect("layout");

        assert!(layout.draw_scalars().len() < 1_000);
        let wrapped = layout
            .caret(DocumentPosition::from(30))
            .expect("wrap caret");
        assert_eq!((wrapped.x, wrapped.y), (10.0, 36.0));
        assert_eq!(
            layout.hit_test(wrapped.x, wrapped.y + 8.0),
            Some(DocumentPosition::from(30))
        );
    }

    #[test]
    fn recovery_replays_core_commands_with_ids_comments_anchors_and_revision_fidelity() {
        let mounted = view(1);
        let comment_id = CommentId::from_bytes([8; 16]);
        let mut load = CanonicalDocumentLoad::new(document(32), "alpha");
        load.comments.push(CanonicalComment {
            id: comment_id,
            range: selection(1, 3),
            body: "note".into(),
        });
        load.anchors.push(CanonicalAnchor {
            block: BlockId::from_bytes([9; 16]),
            position: DocumentPosition::from(4),
        });
        let mut prototype = EditorWidgetPrototype::open(load).expect("open prototype");
        prototype.mount_view(mounted, viewport(0.0)).expect("mount");
        prototype
            .set_selection(mounted, selection(5, 5))
            .expect("caret");
        prototype.input_en_us(mounted, " beta").expect("edit");
        let expected = prototype.projection();

        let recovered = prototype.recovery_record().replay().expect("replay");
        assert_eq!(recovered, expected);
        assert_eq!(recovered.document_id(), document(32));
        assert_eq!(recovered.comments()[0].id, comment_id);
        assert_eq!(recovered.anchors()[0].block, BlockId::from_bytes([9; 16]));
    }

    #[test]
    fn platform_budget_evaluator_requires_each_desktop_target() {
        let passing = |platform| {
            PlatformMeasurements::new(
                platform,
                vec![
                    LatencySample {
                        key_to_paint: Duration::from_millis(8),
                        longest_ui_turn: Duration::from_millis(1),
                    };
                    100
                ],
                Duration::from_millis(100),
                vec![
                    MemoryCycleSample {
                        before_open_bytes: 10,
                        after_open_bytes: 20,
                        after_close_bytes: 10,
                    },
                    MemoryCycleSample {
                        before_open_bytes: 10,
                        after_open_bytes: 20,
                        after_close_bytes: 10,
                    },
                    MemoryCycleSample {
                        before_open_bytes: 10,
                        after_open_bytes: 20,
                        after_close_bytes: 10,
                    },
                ],
            )
        };
        let mut measurements = vec![
            passing(DesktopPlatform::Windows),
            passing(DesktopPlatform::MacOs),
        ];
        assert!(!all_platform_budgets_pass(&measurements));
        measurements.push(passing(DesktopPlatform::Linux));
        assert!(all_platform_budgets_pass(&measurements));
    }

    #[test]
    fn durable_decision_does_not_promote_missing_runtime_evidence() {
        assert_eq!(
            STAGE32_FEASIBILITY_DECISION,
            FeasibilityDecision::HoldBroadEditorDelivery
        );
        assert!(STAGE32_GATE_ASSESSMENTS.iter().any(|gate| {
            gate.id() == "budgets.latency_memory"
                && gate.status() == GateStatus::RuntimeMeasurementRequired
        }));
        assert_unevaluated_and_unadopted(TEXT_DOCUMENT_1_8_ASSESSMENT);
    }

    fn assert_unevaluated_and_unadopted(assessment: EngineCandidateAssessment) {
        assert!(!assessment.evaluated);
        assert!(!assessment.adopted);
    }

    #[test]
    fn lifecycle_and_input_failures_are_bounded() {
        let mounted = view(1);
        let mut prototype =
            EditorWidgetPrototype::open(CanonicalDocumentLoad::new(document(32), "alpha"))
                .expect("open prototype");
        prototype.mount_view(mounted, viewport(0.0)).expect("mount");

        assert!(matches!(
            prototype.mount_view(mounted, viewport(0.0)),
            Err(PrototypeError::Editor(
                EditorError::ViewAlreadyAttached { .. }
            ))
        ));
        assert!(matches!(
            prototype.input_en_us(mounted, "\u{03bb}"),
            Err(PrototypeError::InvalidInput(_))
        ));
        prototype.unmount_view(mounted).expect("unmount");
        assert!(matches!(
            prototype.view(mounted),
            Err(PrototypeError::Editor(EditorError::UnknownView { .. }))
        ));

        let lifecycle = prototype.close();
        assert_eq!(lifecycle.released_views, 0);
        assert_eq!(lifecycle.after_close_bytes, 0);
    }

    #[test]
    #[ignore = "Stage 32 local diagnostic; host paint and process memory still require platform runners"]
    fn local_core_descriptor_diagnostic() {
        let left = view(1);
        let right = view(2);
        let body = "word ".repeat(250_000);
        let opened = Instant::now();
        let mut prototype =
            EditorWidgetPrototype::open(CanonicalDocumentLoad::new(document(32), body))
                .expect("open large prototype");
        prototype
            .mount_view(left, viewport(0.0))
            .expect("mount left");
        prototype
            .mount_view(right, viewport(160.0))
            .expect("mount right");
        let open_elapsed = opened.elapsed();

        let mut samples = Vec::new();
        for _ in 0..100 {
            prototype
                .set_selection(left, selection(0, 0))
                .expect("caret");
            let started = Instant::now();
            prototype.input_en_us(left, "x").expect("edit");
            prototype.layout(left, metrics()).expect("left layout");
            prototype.layout(right, metrics()).expect("right layout");
            samples.push(started.elapsed());
            prototype.undo(right).expect("shared undo");
        }
        let p95 = percentile(samples.iter().copied(), 95).expect("p95");
        let p99 = percentile(samples.iter().copied(), 99).expect("p99");
        eprintln!(
            "stage32 local-only: open_250k_words={open_elapsed:?} model_plus_descriptor_p95={p95:?} model_plus_descriptor_p99={p99:?} estimated_descriptor_bytes={}",
            prototype.estimated_descriptor_bytes()
        );
        assert_eq!(
            prototype.projection().body().split_whitespace().count(),
            250_000
        );
        assert_eq!(prototype.close().after_close_bytes, 0);
    }
}
