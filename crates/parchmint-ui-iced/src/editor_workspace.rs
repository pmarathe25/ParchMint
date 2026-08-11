use std::collections::BTreeMap;

use parchmint_application::DocumentVisibility;
use parchmint_domain::{NodeKind, ProjectSection};
use parchmint_editor_api::ViewId;
use parchmint_preferences::ResolvedAppearance;
use parchmint_ui_api::ProjectSnapshot;

const TAB_HEIGHT: f32 = 32.0;
const TAB_MAX_WIDTH: f32 = 200.0;
const TAB_MIN_WIDTH: f32 = 64.0;
const TAB_CLOSE_WIDTH: f32 = 24.0;
const TAB_TITLE_INSET: f32 = 16.0;
const APPROXIMATE_TITLE_SCALAR_WIDTH: f32 = 8.0;
const SPELLING_MENU_WIDTH: f32 = 224.0;
const SPELLING_MENU_MIN_HEIGHT: f32 = 128.0;

/// The two editor hosts available in the workspace.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum EditorPane {
    Primary,
    Companion,
}

/// A deterministic editor state with a maintained visual reference.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorFixture {
    DualPane,
    SameDocumentTwoViews,
}

/// One document tab requested by the UI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TabSpec {
    id: String,
    title: String,
    dirty: bool,
}

impl TabSpec {
    pub fn new(id: impl Into<String>, title: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            dirty: false,
        }
    }

    pub fn dirty(mut self, dirty: bool) -> Self {
        self.dirty = dirty;
        self
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn title(&self) -> &str {
        &self.title
    }

    pub const fn is_dirty(&self) -> bool {
        self.dirty
    }
}

/// Per-host state. Document data is deliberately represented by adapter effects.
#[derive(Debug, Clone, PartialEq)]
pub struct EditorPaneState {
    pane: EditorPane,
    view: ViewId,
    tabs: Vec<TabSpec>,
    active_tab: Option<usize>,
    scroll_offset: f32,
    mount_generation: u64,
}

impl EditorPaneState {
    fn empty(pane: EditorPane, view: ViewId) -> Self {
        Self {
            pane,
            view,
            tabs: Vec::new(),
            active_tab: None,
            scroll_offset: 0.0,
            mount_generation: 1,
        }
    }

    fn populated(
        pane: EditorPane,
        view: ViewId,
        tabs: Vec<TabSpec>,
        active_tab: usize,
        scroll_offset: f32,
    ) -> Self {
        Self {
            pane,
            view,
            tabs,
            active_tab: Some(active_tab),
            scroll_offset,
            mount_generation: 1,
        }
    }

    pub const fn pane(&self) -> EditorPane {
        self.pane
    }

    pub const fn view(&self) -> ViewId {
        self.view
    }

    pub fn tabs(&self) -> &[TabSpec] {
        &self.tabs
    }

    pub fn active_document(&self) -> Option<&str> {
        self.active_tab
            .and_then(|index| self.tabs.get(index))
            .map(TabSpec::id)
    }

    pub const fn scroll_offset(&self) -> f32 {
        self.scroll_offset
    }

    pub const fn is_populated(&self) -> bool {
        self.active_tab.is_some()
    }

    fn activate(&mut self, id: &str) -> Option<bool> {
        let index = self.tabs.iter().position(|tab| tab.id == id)?;
        let changed = self.active_tab != Some(index);
        if changed {
            self.active_tab = Some(index);
            self.mount_generation = self.mount_generation.saturating_add(1);
        }
        Some(changed)
    }

    fn open(&mut self, tab: TabSpec) -> bool {
        if let Some(changed) = self.activate(&tab.id) {
            return changed;
        }
        self.tabs.push(tab);
        self.active_tab = Some(self.tabs.len() - 1);
        self.mount_generation = self.mount_generation.saturating_add(1);
        true
    }

    fn close(&mut self, id: &str) -> bool {
        let Some(index) = self.tabs.iter().position(|tab| tab.id == id) else {
            return false;
        };
        let active = self.active_tab;
        self.tabs.remove(index);
        self.active_tab = match (self.tabs.is_empty(), active) {
            (true, _) => None,
            (false, Some(active)) if active > index => Some(active - 1),
            (false, Some(active)) if active == index => Some(index.min(self.tabs.len() - 1)),
            (false, active) => active,
        };
        self.mount_generation = self.mount_generation.saturating_add(1);
        true
    }

    fn move_tab(&mut self, id: &str, target_index: usize) -> bool {
        let Some(source_index) = self.tabs.iter().position(|tab| tab.id == id) else {
            return false;
        };
        let active_id = self.active_document().map(str::to_owned);
        let tab = self.tabs.remove(source_index);
        let target_index = target_index.min(self.tabs.len());
        self.tabs.insert(target_index, tab);
        self.active_tab = active_id
            .as_deref()
            .and_then(|active| self.tabs.iter().position(|tab| tab.id == active));
        true
    }

    fn reconcile_tabs(&mut self, titles: &BTreeMap<String, String>) -> bool {
        let previous_active = self.active_document().map(str::to_owned);
        let previous_tabs = self.tabs.len();
        self.tabs.retain_mut(|tab| {
            let Some(title) = titles.get(&tab.id) else {
                return false;
            };
            tab.title.clone_from(title);
            true
        });
        self.active_tab = previous_active
            .as_deref()
            .and_then(|active| self.tabs.iter().position(|tab| tab.id == active))
            .or_else(|| (!self.tabs.is_empty()).then_some(0));
        let active_survived = previous_active.as_deref() == self.active_document();
        if previous_tabs != self.tabs.len() || !active_survived {
            self.mount_generation = self.mount_generation.saturating_add(1);
        }
        active_survived
    }
}

/// A scalar range used for local search and editor decorations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FindMatch {
    start: u64,
    end: u64,
}

impl FindMatch {
    pub const fn new(start: u64, end: u64) -> Self {
        Self { start, end }
    }

    pub const fn start(self) -> u64 {
        self.start
    }

    pub const fn end(self) -> u64 {
        self.end
    }
}

/// Local Find/Replace state belonging to exactly one mounted view.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LocalSearchState {
    open: bool,
    query: String,
    matches: Vec<FindMatch>,
    active_match: Option<usize>,
    case_sensitive: bool,
    whole_word: bool,
    replace_visible: bool,
}

impl LocalSearchState {
    pub fn open(query: impl Into<String>, matches: Vec<FindMatch>) -> Self {
        let active_match = (!matches.is_empty()).then_some(0);
        Self {
            open: true,
            query: query.into(),
            matches,
            active_match,
            ..Self::default()
        }
    }

    pub const fn is_open(&self) -> bool {
        self.open
    }

    pub fn query(&self) -> &str {
        &self.query
    }

    pub fn matches(&self) -> &[FindMatch] {
        &self.matches
    }

    pub fn active_match(&self) -> Option<FindMatch> {
        self.active_match
            .and_then(|index| self.matches.get(index))
            .copied()
    }

    pub const fn case_sensitive(&self) -> bool {
        self.case_sensitive
    }

    pub const fn whole_word(&self) -> bool {
        self.whole_word
    }

    pub const fn replace_visible(&self) -> bool {
        self.replace_visible
    }

    fn close(&mut self) {
        *self = Self::default();
    }

    fn navigate(&mut self, direction: FindDirection) -> Option<FindMatch> {
        if self.matches.is_empty() {
            self.active_match = None;
            return None;
        }
        let current = self.active_match.unwrap_or_default();
        self.active_match = Some(match direction {
            FindDirection::Next => (current + 1) % self.matches.len(),
            FindDirection::Previous => current.checked_sub(1).unwrap_or(self.matches.len() - 1),
        });
        self.active_match()
    }
}

/// Direction used by Enter and Shift+Enter in Local Find.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FindDirection {
    Next,
    Previous,
}

/// Commands exposed by the one always-visible formatting toolbar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FormattingCommand {
    ParagraphStyle(String),
    Bold,
    Italic,
    Underline,
    Strikethrough,
    BulletedList,
    NumberedList,
    BlockQuote,
    Link,
    SceneBreak,
    PageBreak,
}

/// Adapter-facing commands resolved against the focused editor view.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditorCommand {
    ApplyParagraphStyle(String),
    ToggleBold,
    ToggleItalic,
    ToggleUnderline,
    ToggleStrikethrough,
    ToggleBulletedList,
    ToggleNumberedList,
    ToggleBlockQuote,
    SetLink {
        target: Option<String>,
    },
    InsertSceneBreak,
    InsertPageBreak,
    Undo,
    Redo,
    NavigateFindMatch {
        range: FindMatch,
    },
    ReplaceActiveFindMatch {
        replacement: String,
    },
    ReplaceAllFindMatches {
        replacement: String,
    },
    ReplaceSpelling {
        misspelling: String,
        replacement: String,
    },
}

impl FormattingCommand {
    fn editor_command(self) -> Option<EditorCommand> {
        match self {
            FormattingCommand::ParagraphStyle(style) => {
                Some(EditorCommand::ApplyParagraphStyle(style))
            }
            FormattingCommand::Bold => Some(EditorCommand::ToggleBold),
            FormattingCommand::Italic => Some(EditorCommand::ToggleItalic),
            FormattingCommand::Underline => Some(EditorCommand::ToggleUnderline),
            FormattingCommand::Strikethrough => Some(EditorCommand::ToggleStrikethrough),
            FormattingCommand::BulletedList => Some(EditorCommand::ToggleBulletedList),
            FormattingCommand::NumberedList => Some(EditorCommand::ToggleNumberedList),
            FormattingCommand::BlockQuote => Some(EditorCommand::ToggleBlockQuote),
            FormattingCommand::Link => None,
            FormattingCommand::SceneBreak => Some(EditorCommand::InsertSceneBreak),
            FormattingCommand::PageBreak => Some(EditorCommand::InsertPageBreak),
        }
    }
}

/// Draft state for the editor-owned link popover.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LinkEditorState {
    open: bool,
    target: String,
    validation_error: Option<String>,
}

impl LinkEditorState {
    pub const fn is_open(&self) -> bool {
        self.open
    }

    pub fn target(&self) -> &str {
        &self.target
    }

    pub fn validation_error(&self) -> Option<&str> {
        self.validation_error.as_deref()
    }

    fn open(&mut self) {
        *self = Self {
            open: true,
            ..Self::default()
        };
    }

    fn set_target(&mut self, target: String) {
        self.target = target;
        self.validation_error = None;
    }

    fn reject_empty_target(&mut self) {
        self.validation_error = Some("Enter a URL before applying a link.".to_owned());
    }

    fn close(&mut self) {
        *self = Self::default();
    }
}

/// Logical point used by deterministic editor menus.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Point {
    x: f32,
    y: f32,
}

impl Point {
    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }

    pub const fn x(self) -> f32 {
        self.x
    }

    pub const fn y(self) -> f32 {
        self.y
    }
}

/// Logical rectangle used by headless layout fixtures.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Rect {
    x: f32,
    y: f32,
    width: f32,
    height: f32,
}

impl Rect {
    pub const fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    pub const fn left(self) -> f32 {
        self.x
    }

    pub const fn right(self) -> f32 {
        self.x + self.width
    }

    pub const fn top(self) -> f32 {
        self.y
    }

    pub const fn bottom(self) -> f32 {
        self.y + self.height
    }

    pub const fn width(self) -> f32 {
        self.width
    }

    pub const fn height(self) -> f32 {
        self.height
    }

    pub const fn center(self) -> Point {
        Point::new(self.x + self.width / 2.0, self.y + self.height / 2.0)
    }
}

/// A request to anchor a spelling menu to one decorated word.
#[derive(Debug, Clone, PartialEq)]
pub struct SpellingMenuRequest {
    pane: EditorPane,
    word: String,
    word_bounds: Rect,
    pane_bounds: Rect,
    suggestions: Vec<String>,
    in_project_dictionary: bool,
    in_global_dictionary: bool,
}

impl SpellingMenuRequest {
    pub fn new(
        pane: EditorPane,
        word: impl Into<String>,
        word_bounds: Rect,
        pane_bounds: Rect,
    ) -> Self {
        Self {
            pane,
            word: word.into(),
            word_bounds,
            pane_bounds,
            suggestions: Vec::new(),
            in_project_dictionary: false,
            in_global_dictionary: false,
        }
    }

    pub fn with_suggestions(mut self, suggestions: Vec<String>) -> Self {
        self.suggestions = suggestions;
        self
    }

    pub const fn with_dictionary_membership(
        mut self,
        in_project_dictionary: bool,
        in_global_dictionary: bool,
    ) -> Self {
        self.in_project_dictionary = in_project_dictionary;
        self.in_global_dictionary = in_global_dictionary;
        self
    }
}

/// A service-facing dictionary scope available from the spelling menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpellingDictionaryScope {
    Project,
    Global,
}

/// One applicable action in the word-anchored spelling menu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SpellingMenuAction {
    Replace(String),
    AddToDictionary(SpellingDictionaryScope),
    RemoveFromDictionary(SpellingDictionaryScope),
    Ignore,
}

/// The fully placed spelling menu, independent of a native compositor.
#[derive(Debug, Clone, PartialEq)]
pub struct SpellingMenu {
    pane: EditorPane,
    word: String,
    suggestions: Vec<String>,
    actions: Vec<SpellingMenuAction>,
    anchor_bounds: Rect,
    bounds: Rect,
    invocation_point: Point,
}

impl SpellingMenu {
    fn layout(request: SpellingMenuRequest) -> Self {
        let invocation_point = request.word_bounds.center();
        let width = SPELLING_MENU_WIDTH.min(request.pane_bounds.width.max(0.0));
        let requested_height = SPELLING_MENU_MIN_HEIGHT
            + request.suggestions.len().saturating_sub(1) as f32 * TAB_HEIGHT;
        let height = requested_height.min(request.pane_bounds.height.max(0.0));
        let preferred_x = invocation_point.x - width / 2.0;
        let maximum_x = (request.pane_bounds.right() - width).max(request.pane_bounds.left());
        let x = preferred_x.clamp(request.pane_bounds.left(), maximum_x);
        let below = request.word_bounds.bottom();
        let above = request.word_bounds.top() - height;
        let preferred_y = if below + height <= request.pane_bounds.bottom() {
            below
        } else {
            above
        };
        let maximum_y = (request.pane_bounds.bottom() - height).max(request.pane_bounds.top());
        let y = preferred_y.clamp(request.pane_bounds.top(), maximum_y);
        let mut actions = request
            .suggestions
            .iter()
            .cloned()
            .map(SpellingMenuAction::Replace)
            .collect::<Vec<_>>();
        actions.push(if request.in_project_dictionary {
            SpellingMenuAction::RemoveFromDictionary(SpellingDictionaryScope::Project)
        } else {
            SpellingMenuAction::AddToDictionary(SpellingDictionaryScope::Project)
        });
        actions.push(if request.in_global_dictionary {
            SpellingMenuAction::RemoveFromDictionary(SpellingDictionaryScope::Global)
        } else {
            SpellingMenuAction::AddToDictionary(SpellingDictionaryScope::Global)
        });
        actions.push(SpellingMenuAction::Ignore);
        Self {
            pane: request.pane,
            word: request.word,
            suggestions: request.suggestions,
            actions,
            anchor_bounds: request.word_bounds,
            bounds: Rect::new(x, y, width, height),
            invocation_point,
        }
    }

    pub const fn pane(&self) -> EditorPane {
        self.pane
    }

    pub fn word(&self) -> &str {
        &self.word
    }

    pub fn suggestions(&self) -> &[String] {
        &self.suggestions
    }

    pub fn actions(&self) -> &[SpellingMenuAction] {
        &self.actions
    }

    pub const fn anchor_bounds(&self) -> Rect {
        self.anchor_bounds
    }

    pub const fn bounds(&self) -> Rect {
        self.bounds
    }

    pub const fn invocation_point(&self) -> Point {
        self.invocation_point
    }
}

/// One laid-out tab with separate title, tooltip, and close geometry.
#[derive(Debug, Clone, PartialEq)]
pub struct TabLayout {
    id: String,
    full_title: String,
    display_title: String,
    tooltip: Option<String>,
    bounds: Rect,
    close_bounds: Rect,
    active: bool,
    dirty: bool,
}

impl TabLayout {
    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn full_title(&self) -> &str {
        &self.full_title
    }

    pub fn display_title(&self) -> &str {
        &self.display_title
    }

    pub fn tooltip(&self) -> Option<&str> {
        self.tooltip.as_deref()
    }

    pub const fn bounds(&self) -> Rect {
        self.bounds
    }

    pub const fn close_bounds(&self) -> Rect {
        self.close_bounds
    }

    pub const fn is_active(&self) -> bool {
        self.active
    }

    pub const fn is_dirty(&self) -> bool {
        self.dirty
    }
}

/// Deterministic geometry for one populated pane's 32 px tab strip.
#[derive(Debug, Clone, PartialEq)]
pub struct TabStripLayout {
    tabs: Vec<TabLayout>,
    active_index: Option<usize>,
}

impl TabStripLayout {
    pub fn tabs(&self) -> &[TabLayout] {
        &self.tabs
    }

    pub fn active_tab(&self) -> &TabLayout {
        self.active_index
            .and_then(|index| self.tabs.get(index))
            .or_else(|| self.tabs.first())
            .expect("a tab-strip layout requires at least one tab")
    }
}

/// Theme-independent color used for verifiable focus contrast.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FocusColor {
    red: f32,
    green: f32,
    blue: f32,
}

impl FocusColor {
    const fn new(red: f32, green: f32, blue: f32) -> Self {
        Self { red, green, blue }
    }

    fn luminance(self) -> f32 {
        0.2126 * linear(self.red) + 0.7152 * linear(self.green) + 0.0722 * linear(self.blue)
    }
}

/// Focus treatment carrying a non-color outline plus contrast-testable color.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PaneFocusStyle {
    focused: bool,
    outline: FocusColor,
    outline_width: f32,
}

impl PaneFocusStyle {
    pub const fn is_focused(self) -> bool {
        self.focused
    }

    pub const fn outline_width(self) -> f32 {
        self.outline_width
    }

    pub fn is_visible_against(self, appearance: ResolvedAppearance) -> bool {
        if !self.focused || self.outline_width < 2.0 {
            return false;
        }
        let background = match appearance {
            ResolvedAppearance::Light => FocusColor::new(1.0, 1.0, 1.0),
            ResolvedAppearance::Dark => FocusColor::new(0.055, 0.071, 0.067),
        };
        contrast_ratio(self.outline, background) >= 3.0
    }
}

fn linear(component: f32) -> f32 {
    if component <= 0.04045 {
        component / 12.92
    } else {
        ((component + 0.055) / 1.055).powf(2.4)
    }
}

fn contrast_ratio(left: FocusColor, right: FocusColor) -> f32 {
    let (lighter, darker) = if left.luminance() >= right.luminance() {
        (left.luminance(), right.luminance())
    } else {
        (right.luminance(), left.luminance())
    };
    (lighter + 0.05) / (darker + 0.05)
}

/// Which count is currently shown at the left side of the status bar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusCount {
    Selection(usize),
    ActiveDocument(usize),
}

/// Status data computed from the focused view without loading every document body.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EditorStatusBar {
    current_count: StatusCount,
    manuscript_total: usize,
}

impl EditorStatusBar {
    pub const fn current_count(self) -> StatusCount {
        self.current_count
    }

    pub const fn manuscript_total(self) -> usize {
        self.manuscript_total
    }
}

/// Where the Inspector derives its visible sections and comments list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InspectorContext {
    None,
    Group { group_id: String },
    Document { document_id: String },
}

impl InspectorContext {
    pub const fn comments_available(&self) -> bool {
        matches!(self, Self::Document { .. })
    }
}

/// The type and reattachment state of a comment anchor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommentAnchor {
    Range {
        document_id: String,
        range: FindMatch,
    },
    Position {
        document_id: String,
        position: u64,
    },
    Document {
        document_id: String,
    },
    Orphaned {
        document_id: String,
    },
}

impl CommentAnchor {
    fn document_id(&self) -> &str {
        match self {
            Self::Range { document_id, .. }
            | Self::Position { document_id, .. }
            | Self::Document { document_id }
            | Self::Orphaned { document_id } => document_id,
        }
    }

    fn is_navigable(&self) -> bool {
        matches!(self, Self::Range { .. } | Self::Position { .. })
    }
}

/// Adapter decorations owned independently by each mounted view.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EditorDecorations {
    search: Vec<FindMatch>,
    spelling: Vec<SpellingDecoration>,
    comment_highlight: Option<String>,
}

impl EditorDecorations {
    pub fn search(&self) -> &[FindMatch] {
        &self.search
    }

    pub fn spelling(&self) -> &[SpellingDecoration] {
        &self.spelling
    }

    pub fn comment_highlight(&self) -> Option<&str> {
        self.comment_highlight.as_deref()
    }
}

/// One in-place spelling underline.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpellingDecoration {
    word: String,
    range: FindMatch,
}

impl SpellingDecoration {
    pub fn new(word: impl Into<String>, range: FindMatch) -> Self {
        Self {
            word: word.into(),
            range,
        }
    }

    pub fn word(&self) -> &str {
        &self.word
    }

    pub const fn range(&self) -> FindMatch {
        self.range
    }
}

/// Editor work that may finish after focus, tabs, or document revisions change.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum EditorTask {
    RefreshSpellcheck { view: ViewId },
    LoadComments { view: ViewId },
    RefreshWordCount { view: ViewId },
}

impl EditorTask {
    pub const fn view(&self) -> ViewId {
        match self {
            Self::RefreshSpellcheck { view }
            | Self::LoadComments { view }
            | Self::RefreshWordCount { view } => *view,
        }
    }
}

/// An exact live-request ticket including the mounted document generation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AsyncEditorTicket {
    task: EditorTask,
    request: u64,
    document_id: Option<String>,
    document_revision: u64,
    mount_generation: u64,
}

impl AsyncEditorTicket {
    pub fn task(&self) -> &EditorTask {
        &self.task
    }

    pub const fn request(&self) -> u64 {
        self.request
    }

    pub fn document_id(&self) -> Option<&str> {
        self.document_id.as_deref()
    }

    pub const fn document_revision(&self) -> u64 {
        self.document_revision
    }

    pub const fn mount_generation(&self) -> u64 {
        self.mount_generation
    }
}

/// Typed payloads accepted only through their exact editor ticket.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AsyncEditorPayload {
    SpellcheckApplied,
    SpellcheckDecorations(Vec<SpellingDecoration>),
    SpellcheckFailed(String),
    CommentsLoaded(Vec<(String, CommentAnchor)>),
    WordCount(usize),
}

/// A delayed editor result carrying its complete request identity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AsyncEditorCompletion {
    ticket: AsyncEditorTicket,
    payload: AsyncEditorPayload,
}

impl AsyncEditorCompletion {
    pub fn new(view: ViewId, task: EditorTask, payload: AsyncEditorPayload) -> Self {
        Self {
            ticket: AsyncEditorTicket {
                task,
                request: 1,
                document_id: None,
                document_revision: 0,
                mount_generation: 0,
            },
            payload,
        }
        .with_claimed_view(view)
    }

    fn with_claimed_view(mut self, view: ViewId) -> Self {
        if self.ticket.task.view() != view {
            self.ticket.request = 0;
        }
        self
    }

    pub fn for_ticket(ticket: AsyncEditorTicket, payload: AsyncEditorPayload) -> Self {
        Self { ticket, payload }
    }

    pub fn ticket(&self) -> &AsyncEditorTicket {
        &self.ticket
    }

    pub fn payload(&self) -> &AsyncEditorPayload {
        &self.payload
    }
}

/// Messages produced by Iced widgets or mounted editor callbacks.
#[derive(Debug, Clone, PartialEq)]
pub enum EditorMessage {
    FocusPane(EditorPane),
    FocusFormattingToolbar,
    OpenTab {
        pane: EditorPane,
        tab: TabSpec,
    },
    ActivateTab {
        pane: EditorPane,
        document_id: String,
    },
    CloseTab {
        pane: EditorPane,
        document_id: String,
    },
    MoveTab {
        pane: EditorPane,
        document_id: String,
        target_index: usize,
    },
    Format(FormattingCommand),
    OpenLinkEditor,
    SetLinkTarget(String),
    ApplyLink,
    RemoveLink,
    CancelLinkEditor,
    Undo,
    Redo,
    OpenLocalFind,
    CloseLocalFind,
    SetFindQuery(String),
    SetFindMatches(Vec<FindMatch>),
    SetFindOptions {
        case_sensitive: bool,
        whole_word: bool,
    },
    NavigateFind(FindDirection),
    SetReplaceVisible(bool),
    ReplaceActiveMatch(String),
    ReplaceAllMatches(String),
    SetCommentAnchor {
        comment_id: String,
        anchor: CommentAnchor,
    },
    SelectComment(String),
    SetInspectorContext(InspectorContext),
    SetSpellingDecorations {
        view: ViewId,
        decorations: Vec<SpellingDecoration>,
    },
    OpenSpellingMenu(SpellingMenuRequest),
    ChooseSpellingAction {
        pane: EditorPane,
        word: String,
        action: SpellingMenuAction,
    },
    SetSelectionWordCount {
        pane: EditorPane,
        words: Option<usize>,
    },
    SetDocumentWordCount {
        document_id: String,
        words: usize,
    },
    SetDocumentRevision {
        document_id: String,
        revision: u64,
    },
    SetManuscriptTotal(usize),
}

/// Side effects translated by the integration layer into editor-adapter calls.
#[derive(Debug, Clone, PartialEq)]
pub enum EditorEffect {
    Command {
        view: ViewId,
        command: EditorCommand,
    },
    MountDocument {
        pane: EditorPane,
        view: ViewId,
        document_id: String,
    },
    UnmountView {
        pane: EditorPane,
        view: ViewId,
    },
    SetSearchDecorations {
        view: ViewId,
        matches: Vec<FindMatch>,
        active: Option<FindMatch>,
    },
    SetSpellcheckDecorations {
        view: ViewId,
        decorations: Vec<SpellingDecoration>,
    },
    NavigateCommentAnchor {
        view: ViewId,
        comment_id: String,
        highlight: bool,
    },
    ShowOrphanedComment {
        comment_id: String,
    },
    ShowSpellingMenu(SpellingMenu),
    SpellingDictionaryAction {
        view: ViewId,
        word: String,
        scope: SpellingDictionaryScope,
        add: bool,
    },
    RestoreEditorFocus {
        view: ViewId,
    },
}

/// Deterministic editor workspace presentation state.
#[derive(Debug, Clone)]
pub struct EditorWorkspace {
    source: EditorWorkspaceSource,
    primary: EditorPaneState,
    companion: EditorPaneState,
    focused_pane: EditorPane,
    toolbar_focused: bool,
    link_editor: LinkEditorState,
    local_search: BTreeMap<ViewId, LocalSearchState>,
    decorations: BTreeMap<ViewId, EditorDecorations>,
    selection_word_counts: BTreeMap<ViewId, Option<usize>>,
    document_word_counts: BTreeMap<String, usize>,
    document_revisions: BTreeMap<String, u64>,
    manuscript_total: usize,
    last_focused_view: BTreeMap<String, ViewId>,
    comments: BTreeMap<String, CommentAnchor>,
    spellcheck_errors: BTreeMap<ViewId, String>,
    inspector: InspectorContext,
    pending: BTreeMap<EditorTask, AsyncEditorTicket>,
    next_request: u64,
}

#[derive(Debug, Clone, Copy)]
enum EditorWorkspaceSource {
    Fixture(EditorFixture),
    Production,
}

impl EditorWorkspace {
    pub fn from_fixture(fixture: EditorFixture) -> Self {
        let primary_view = ViewId::from_bytes([36; 16]);
        let companion_view = ViewId::from_bytes([37; 16]);
        let (primary_tabs, companion_tabs, companion_scroll) = match fixture {
            EditorFixture::DualPane => (
                vec![TabSpec::new("chapter-one", "Chapter One")],
                vec![TabSpec::new("chapter-two", "Chapter Two")],
                96.0,
            ),
            EditorFixture::SameDocumentTwoViews => (
                vec![TabSpec::new("chapter-one", "Chapter One")],
                vec![TabSpec::new("chapter-one", "Chapter One")],
                144.0,
            ),
        };
        let primary =
            EditorPaneState::populated(EditorPane::Primary, primary_view, primary_tabs, 0, 0.0);
        let companion = EditorPaneState::populated(
            EditorPane::Companion,
            companion_view,
            companion_tabs,
            0,
            companion_scroll,
        );
        let mut local_search = BTreeMap::new();
        local_search.insert(primary_view, LocalSearchState::default());
        local_search.insert(companion_view, LocalSearchState::default());
        let mut decorations = BTreeMap::new();
        decorations.insert(primary_view, EditorDecorations::default());
        decorations.insert(companion_view, EditorDecorations::default());
        let mut selection_word_counts = BTreeMap::new();
        selection_word_counts.insert(primary_view, None);
        selection_word_counts.insert(companion_view, None);
        let document_word_counts = BTreeMap::from([
            ("chapter-one".to_owned(), 412),
            ("chapter-two".to_owned(), 367),
        ]);
        let document_revisions =
            BTreeMap::from([("chapter-one".to_owned(), 0), ("chapter-two".to_owned(), 0)]);
        let mut last_focused_view = BTreeMap::new();
        last_focused_view.insert("chapter-one".to_owned(), primary_view);
        let inspector = InspectorContext::Document {
            document_id: "chapter-one".to_owned(),
        };
        Self {
            source: EditorWorkspaceSource::Fixture(fixture),
            primary,
            companion,
            focused_pane: EditorPane::Primary,
            toolbar_focused: false,
            link_editor: LinkEditorState::default(),
            local_search,
            decorations,
            selection_word_counts,
            document_word_counts,
            document_revisions,
            manuscript_total: 1_204,
            last_focused_view,
            comments: BTreeMap::new(),
            spellcheck_errors: BTreeMap::new(),
            inspector,
            pending: BTreeMap::new(),
            next_request: 0,
        }
    }

    /// Hydrates editor tabs, document revisions, and word counts from production state.
    pub fn from_snapshot(snapshot: &ProjectSnapshot) -> Self {
        let hydrated = HydratedDocuments::from_snapshot(snapshot);
        let primary_view = production_view_id(snapshot, EditorPane::Primary);
        let companion_view = production_view_id(snapshot, EditorPane::Companion);
        let primary_tabs = hydrated
            .ordered
            .iter()
            .filter(|document| {
                hydrated.visibility.get(&document.id) == Some(&DocumentVisibility::Open)
                    || hydrated.initial_document.as_deref() == Some(document.id.as_str())
            })
            .map(|document| TabSpec::new(document.id.clone(), document.title.clone()))
            .collect::<Vec<_>>();
        let primary = if primary_tabs.is_empty() {
            EditorPaneState::empty(EditorPane::Primary, primary_view)
        } else {
            let active = hydrated
                .initial_document
                .as_deref()
                .and_then(|id| primary_tabs.iter().position(|tab| tab.id() == id))
                .unwrap_or_default();
            EditorPaneState::populated(EditorPane::Primary, primary_view, primary_tabs, active, 0.0)
        };
        let companion = EditorPaneState::empty(EditorPane::Companion, companion_view);
        let local_search = BTreeMap::from([
            (primary_view, LocalSearchState::default()),
            (companion_view, LocalSearchState::default()),
        ]);
        let decorations = BTreeMap::from([
            (primary_view, EditorDecorations::default()),
            (companion_view, EditorDecorations::default()),
        ]);
        let selection_word_counts = BTreeMap::from([(primary_view, None), (companion_view, None)]);
        let mut last_focused_view = BTreeMap::new();
        let inspector = if let Some(document_id) = primary.active_document() {
            last_focused_view.insert(document_id.to_owned(), primary_view);
            InspectorContext::Document {
                document_id: document_id.to_owned(),
            }
        } else {
            InspectorContext::None
        };
        Self {
            source: EditorWorkspaceSource::Production,
            primary,
            companion,
            focused_pane: EditorPane::Primary,
            toolbar_focused: false,
            link_editor: LinkEditorState::default(),
            local_search,
            decorations,
            selection_word_counts,
            document_word_counts: hydrated.word_counts,
            document_revisions: hydrated.revisions,
            manuscript_total: hydrated.manuscript_total,
            last_focused_view,
            comments: BTreeMap::new(),
            spellcheck_errors: BTreeMap::new(),
            inspector,
            pending: BTreeMap::new(),
            next_request: 0,
        }
    }

    /// Reconciles authoritative documents without resetting surviving pane/view state.
    pub fn reconcile_snapshot(&mut self, snapshot: &ProjectSnapshot) {
        let hydrated = HydratedDocuments::from_snapshot(snapshot);
        let titles = hydrated
            .ordered
            .iter()
            .map(|document| (document.id.clone(), document.title.clone()))
            .collect::<BTreeMap<_, _>>();
        let primary_active_survived = self.primary.reconcile_tabs(&titles);
        let companion_active_survived = self.companion.reconcile_tabs(&titles);

        for document in &hydrated.ordered {
            if hydrated.visibility.get(&document.id) == Some(&DocumentVisibility::Open)
                && !self.primary.tabs.iter().any(|tab| tab.id == document.id)
                && !self.companion.tabs.iter().any(|tab| tab.id == document.id)
            {
                self.primary
                    .tabs
                    .push(TabSpec::new(document.id.clone(), document.title.clone()));
                if self.primary.active_tab.is_none() {
                    self.primary.active_tab = Some(self.primary.tabs.len() - 1);
                    self.primary.mount_generation = self.primary.mount_generation.saturating_add(1);
                }
            }
        }
        if self.primary.tabs.is_empty()
            && self.companion.tabs.is_empty()
            && let Some(initial) = hydrated.initial_document.as_deref()
            && let Some(title) = titles.get(initial)
        {
            self.primary.tabs.push(TabSpec::new(initial, title));
            self.primary.active_tab = Some(0);
            self.primary.mount_generation = self.primary.mount_generation.saturating_add(1);
        }

        if !primary_active_survived {
            self.reset_view_transients(self.primary.view);
        }
        if !companion_active_survived {
            self.reset_view_transients(self.companion.view);
        }
        self.document_word_counts = hydrated.word_counts;
        self.document_revisions = hydrated.revisions;
        self.manuscript_total = hydrated.manuscript_total;
        self.last_focused_view.retain(|document_id, view| {
            [&self.primary, &self.companion].into_iter().any(|pane| {
                pane.view == *view && pane.tabs.iter().any(|tab| tab.id == *document_id)
            })
        });
        self.comments
            .retain(|_, anchor| titles.contains_key(anchor.document_id()));
        if matches!(
            &self.inspector,
            InspectorContext::Document { document_id } if !titles.contains_key(document_id)
        ) || matches!(
            &self.inspector,
            InspectorContext::Group { group_id } if !snapshot.project.nodes.iter().any(|(id, node)| {
                stable_id_string(id.as_bytes()) == *group_id && node.kind.can_have_children()
            })
        ) {
            self.inspector = self
                .pane(self.focused_pane)
                .active_document()
                .map(|document_id| InspectorContext::Document {
                    document_id: document_id.to_owned(),
                })
                .unwrap_or(InspectorContext::None);
        }
        if !self.pane(self.focused_pane).is_populated() {
            self.focused_pane = if self.primary.is_populated() {
                EditorPane::Primary
            } else if self.companion.is_populated() {
                EditorPane::Companion
            } else {
                EditorPane::Primary
            };
            self.toolbar_focused = false;
        }
        self.pending.retain(|task, ticket| {
            let pane = [&self.primary, &self.companion]
                .into_iter()
                .find(|pane| pane.view == task.view() && pane.is_populated());
            pane.is_some_and(|pane| {
                pane.mount_generation == ticket.mount_generation
                    && pane.active_document() == ticket.document_id.as_deref()
                    && ticket.document_id.as_deref().is_some_and(|document_id| {
                        self.document_revisions
                            .get(document_id)
                            .copied()
                            .unwrap_or_default()
                            == ticket.document_revision
                    })
            })
        });
    }

    pub fn fixture_reference(&self, appearance: ResolvedAppearance) -> &'static str {
        match (self.source, appearance) {
            (EditorWorkspaceSource::Production, _) => {
                panic!("production workspaces do not have fixture references")
            }
            (
                EditorWorkspaceSource::Fixture(EditorFixture::DualPane),
                ResolvedAppearance::Light,
            ) => "editor-dual-light",
            (EditorWorkspaceSource::Fixture(EditorFixture::DualPane), ResolvedAppearance::Dark) => {
                "editor-dual-dark"
            }
            (EditorWorkspaceSource::Fixture(EditorFixture::SameDocumentTwoViews), _) => {
                "editor-same-document-two-views-light"
            }
        }
    }

    pub fn pane(&self, pane: EditorPane) -> &EditorPaneState {
        match pane {
            EditorPane::Primary => &self.primary,
            EditorPane::Companion => &self.companion,
        }
    }

    fn pane_mut(&mut self, pane: EditorPane) -> &mut EditorPaneState {
        match pane {
            EditorPane::Primary => &mut self.primary,
            EditorPane::Companion => &mut self.companion,
        }
    }

    pub const fn focused_pane(&self) -> EditorPane {
        self.focused_pane
    }

    pub fn local_search(&self, view: ViewId) -> &LocalSearchState {
        self.local_search
            .get(&view)
            .expect("local search exists for every fixture view")
    }

    pub fn decorations(&self, view: ViewId) -> &EditorDecorations {
        self.decorations
            .get(&view)
            .expect("decorations exist for every fixture view")
    }

    pub fn inspector_context(&self) -> &InspectorContext {
        &self.inspector
    }

    pub fn spellcheck_error(&self, view: ViewId) -> Option<&str> {
        self.spellcheck_errors.get(&view).map(String::as_str)
    }

    pub fn document_word_count(&self, document_id: &str) -> Option<usize> {
        self.document_word_counts.get(document_id).copied()
    }

    pub fn document_revision(&self, document_id: &str) -> Option<u64> {
        self.document_revisions.get(document_id).copied()
    }

    pub const fn toolbar_is_focused(&self) -> bool {
        self.toolbar_focused
    }

    pub fn link_editor(&self) -> &LinkEditorState {
        &self.link_editor
    }

    pub fn status_bar(&self) -> EditorStatusBar {
        let pane = self.pane(self.focused_pane);
        let selection = self
            .selection_word_counts
            .get(&pane.view)
            .copied()
            .flatten();
        let document_words = pane
            .active_document()
            .and_then(|document| self.document_word_counts.get(document))
            .copied()
            .unwrap_or_default();
        EditorStatusBar {
            current_count: selection
                .map(StatusCount::Selection)
                .unwrap_or(StatusCount::ActiveDocument(document_words)),
            manuscript_total: self.manuscript_total,
        }
    }

    pub fn pane_focus_style(
        &self,
        pane: EditorPane,
        appearance: ResolvedAppearance,
    ) -> PaneFocusStyle {
        let focused = self.focused_pane == pane;
        let outline = match appearance {
            ResolvedAppearance::Light => FocusColor::new(0.0, 0.36, 0.27),
            ResolvedAppearance::Dark => FocusColor::new(0.45, 1.0, 0.78),
        };
        PaneFocusStyle {
            focused,
            outline,
            outline_width: if focused { 2.0 } else { 0.0 },
        }
    }

    pub fn tab_strip_layout(width: f32, tabs: &[TabSpec], active_id: &str) -> TabStripLayout {
        if tabs.is_empty() {
            return TabStripLayout {
                tabs: Vec::new(),
                active_index: None,
            };
        }
        let available = if width.is_finite() {
            width.max(0.0)
        } else {
            0.0
        };
        let width_per_tab = (available / tabs.len() as f32).clamp(TAB_MIN_WIDTH, TAB_MAX_WIDTH);
        let mut x = 0.0;
        let layouts = tabs
            .iter()
            .map(|tab| {
                let bounds = Rect::new(x, 0.0, width_per_tab, TAB_HEIGHT);
                let close_bounds = Rect::new(
                    bounds.right() - TAB_CLOSE_WIDTH,
                    0.0,
                    TAB_CLOSE_WIDTH,
                    TAB_HEIGHT,
                );
                let (display_title, tooltip) = fit_tab_title(&tab.title, width_per_tab);
                x += width_per_tab;
                TabLayout {
                    id: tab.id.clone(),
                    full_title: tab.title.clone(),
                    display_title,
                    tooltip,
                    bounds,
                    close_bounds,
                    active: tab.id == active_id,
                    dirty: tab.dirty,
                }
            })
            .collect::<Vec<_>>();
        let active_index = layouts.iter().position(|tab| tab.active).or(Some(0));
        TabStripLayout {
            tabs: layouts,
            active_index,
        }
    }

    /// Starts an editor task immediately and invalidates any older task of the same kind/view.
    pub fn begin_task(&mut self, task: EditorTask) -> AsyncEditorTicket {
        self.next_request = self.next_request.saturating_add(1);
        let view = task.view();
        let pane = self.pane_for_view(view);
        let (document_id, mount_generation) = pane
            .map(|pane| {
                (
                    pane.active_document().map(str::to_owned),
                    pane.mount_generation,
                )
            })
            .unwrap_or((None, 0));
        let document_revision = document_id
            .as_deref()
            .and_then(|document_id| self.document_revisions.get(document_id))
            .copied()
            .unwrap_or_default();
        let ticket = AsyncEditorTicket {
            task: task.clone(),
            request: self.next_request,
            document_id,
            document_revision,
            mount_generation,
        };
        self.pending.insert(task, ticket.clone());
        ticket
    }

    /// Applies a completion only when task, request, view, document, and mount generation match.
    pub fn accept_completion(&mut self, completion: AsyncEditorCompletion) -> bool {
        let ticket = completion.ticket();
        if self.pending.get(ticket.task()) != Some(ticket)
            || !self.ticket_is_live(ticket)
            || !payload_matches_task(ticket.task(), completion.payload())
        {
            return false;
        }
        let ticket = completion.ticket.clone();
        self.pending.remove(&ticket.task);
        self.apply_async_payload(
            ticket.task.view(),
            ticket.document_id.as_deref(),
            completion.payload,
        )
    }

    pub fn update(&mut self, message: EditorMessage) -> Vec<EditorEffect> {
        match message {
            EditorMessage::FocusPane(pane) => {
                self.focus_pane(pane);
                Vec::new()
            }
            EditorMessage::FocusFormattingToolbar => {
                self.toolbar_focused = true;
                Vec::new()
            }
            EditorMessage::OpenTab { pane, tab } => self.open_tab(pane, tab),
            EditorMessage::ActivateTab { pane, document_id } => {
                self.activate_tab(pane, &document_id)
            }
            EditorMessage::CloseTab { pane, document_id } => self.close_tab(pane, &document_id),
            EditorMessage::MoveTab {
                pane,
                document_id,
                target_index,
            } => {
                self.pane_mut(pane).move_tab(&document_id, target_index);
                Vec::new()
            }
            EditorMessage::OpenLinkEditor => {
                self.link_editor.open();
                Vec::new()
            }
            EditorMessage::Format(command) => {
                if let Some(command) = command.editor_command() {
                    self.command(command)
                } else {
                    self.link_editor.open();
                    Vec::new()
                }
            }
            EditorMessage::SetLinkTarget(target) => {
                if self.link_editor.is_open() {
                    self.link_editor.set_target(target);
                }
                Vec::new()
            }
            EditorMessage::ApplyLink => self.apply_link(),
            EditorMessage::RemoveLink => self.remove_link(),
            EditorMessage::CancelLinkEditor => {
                self.link_editor.close();
                Vec::new()
            }
            EditorMessage::Undo => self.command(EditorCommand::Undo),
            EditorMessage::Redo => self.command(EditorCommand::Redo),
            EditorMessage::OpenLocalFind => {
                self.focused_search_mut().open = true;
                Vec::new()
            }
            EditorMessage::CloseLocalFind => self.close_local_find(),
            EditorMessage::SetFindQuery(query) => {
                let search = self.focused_search_mut();
                search.query = query;
                search.matches.clear();
                search.active_match = None;
                Vec::new()
            }
            EditorMessage::SetFindMatches(matches) => self.set_find_matches(matches),
            EditorMessage::SetFindOptions {
                case_sensitive,
                whole_word,
            } => {
                let search = self.focused_search_mut();
                search.case_sensitive = case_sensitive;
                search.whole_word = whole_word;
                Vec::new()
            }
            EditorMessage::NavigateFind(direction) => self.navigate_find(direction),
            EditorMessage::SetReplaceVisible(visible) => {
                self.focused_search_mut().replace_visible = visible;
                Vec::new()
            }
            EditorMessage::ReplaceActiveMatch(replacement) => {
                self.command(EditorCommand::ReplaceActiveFindMatch { replacement })
            }
            EditorMessage::ReplaceAllMatches(replacement) => {
                self.command(EditorCommand::ReplaceAllFindMatches { replacement })
            }
            EditorMessage::SetCommentAnchor { comment_id, anchor } => {
                self.comments.insert(comment_id, anchor);
                Vec::new()
            }
            EditorMessage::SelectComment(comment_id) => self.select_comment(comment_id),
            EditorMessage::SetInspectorContext(context) => {
                self.inspector = context;
                Vec::new()
            }
            EditorMessage::SetSpellingDecorations { view, decorations } => {
                self.set_spelling_decorations(view, decorations)
            }
            EditorMessage::OpenSpellingMenu(request) => {
                vec![EditorEffect::ShowSpellingMenu(SpellingMenu::layout(
                    request,
                ))]
            }
            EditorMessage::ChooseSpellingAction { pane, word, action } => {
                self.choose_spelling_action(pane, word, action)
            }
            EditorMessage::SetSelectionWordCount { pane, words } => {
                let view = self.pane(pane).view;
                self.focus_pane(pane);
                self.selection_word_counts.insert(view, words);
                Vec::new()
            }
            EditorMessage::SetDocumentWordCount { document_id, words } => {
                self.document_word_counts.insert(document_id, words);
                Vec::new()
            }
            EditorMessage::SetDocumentRevision {
                document_id,
                revision,
            } => {
                self.set_document_revision(document_id, revision);
                Vec::new()
            }
            EditorMessage::SetManuscriptTotal(total) => {
                self.manuscript_total = total;
                Vec::new()
            }
        }
    }

    fn focus_pane(&mut self, pane: EditorPane) {
        if !self.pane(pane).is_populated() {
            return;
        }
        self.focused_pane = pane;
        self.toolbar_focused = false;
        let state = self.pane(pane);
        let view = state.view;
        let document = state.active_document().map(str::to_owned);
        if let Some(document) = document {
            self.last_focused_view.insert(document.clone(), view);
            self.inspector = InspectorContext::Document {
                document_id: document,
            };
        }
    }

    fn command(&self, command: EditorCommand) -> Vec<EditorEffect> {
        let view = self.pane(self.focused_pane).view;
        vec![EditorEffect::Command { view, command }]
    }

    fn apply_link(&mut self) -> Vec<EditorEffect> {
        if !self.link_editor.is_open() {
            return Vec::new();
        }
        let target = self.link_editor.target().trim();
        if target.is_empty() {
            self.link_editor.reject_empty_target();
            return Vec::new();
        }
        let target = target.to_owned();
        self.link_editor.close();
        self.command(EditorCommand::SetLink {
            target: Some(target),
        })
    }

    fn remove_link(&mut self) -> Vec<EditorEffect> {
        if !self.link_editor.is_open() {
            return Vec::new();
        }
        self.link_editor.close();
        self.command(EditorCommand::SetLink { target: None })
    }

    fn open_tab(&mut self, pane: EditorPane, tab: TabSpec) -> Vec<EditorEffect> {
        let document_id = tab.id.clone();
        let view = self.pane(pane).view;
        let changed = self.pane_mut(pane).open(tab);
        self.document_revisions
            .entry(document_id.clone())
            .or_default();
        self.focus_pane(pane);
        if !changed {
            return Vec::new();
        }
        self.invalidate_view_tasks(view);
        vec![EditorEffect::MountDocument {
            pane,
            view,
            document_id,
        }]
    }

    fn activate_tab(&mut self, pane: EditorPane, document_id: &str) -> Vec<EditorEffect> {
        let view = self.pane(pane).view;
        let Some(changed) = self.pane_mut(pane).activate(document_id) else {
            return Vec::new();
        };
        self.focus_pane(pane);
        if !changed {
            return Vec::new();
        }
        self.invalidate_view_tasks(view);
        vec![EditorEffect::MountDocument {
            pane,
            view,
            document_id: document_id.to_owned(),
        }]
    }

    fn close_tab(&mut self, pane: EditorPane, document_id: &str) -> Vec<EditorEffect> {
        let view = self.pane(pane).view;
        if !self.pane_mut(pane).close(document_id) {
            return Vec::new();
        }
        self.invalidate_view_tasks(view);
        let mut effects = vec![EditorEffect::UnmountView { pane, view }];
        if self.pane(pane).is_populated() {
            let next = self
                .pane(pane)
                .active_document()
                .expect("populated pane has an active tab")
                .to_owned();
            effects.push(EditorEffect::MountDocument {
                pane,
                view,
                document_id: next,
            });
            self.focus_pane(pane);
        } else if pane == EditorPane::Companion {
            self.focus_pane(EditorPane::Primary);
        }
        effects
    }

    fn close_local_find(&mut self) -> Vec<EditorEffect> {
        let view = self.pane(self.focused_pane).view;
        self.focused_search_mut().close();
        self.decorations.entry(view).or_default().search.clear();
        vec![
            EditorEffect::SetSearchDecorations {
                view,
                matches: Vec::new(),
                active: None,
            },
            EditorEffect::RestoreEditorFocus { view },
        ]
    }

    fn set_find_matches(&mut self, matches: Vec<FindMatch>) -> Vec<EditorEffect> {
        let view = self.pane(self.focused_pane).view;
        let search = self.focused_search_mut();
        search.active_match = (!matches.is_empty()).then_some(0);
        search.matches = matches.clone();
        self.decorations.entry(view).or_default().search = matches.clone();
        vec![EditorEffect::SetSearchDecorations {
            view,
            matches,
            active: search_active(self.local_search.get(&view)),
        }]
    }

    fn navigate_find(&mut self, direction: FindDirection) -> Vec<EditorEffect> {
        let view = self.pane(self.focused_pane).view;
        let Some(range) = self.focused_search_mut().navigate(direction) else {
            return Vec::new();
        };
        vec![EditorEffect::Command {
            view,
            command: EditorCommand::NavigateFindMatch { range },
        }]
    }

    fn select_comment(&mut self, comment_id: String) -> Vec<EditorEffect> {
        let current_document = self
            .pane(self.focused_pane)
            .active_document()
            .map(str::to_owned);
        let anchor = self.comments.get(&comment_id);
        if anchor.is_some_and(|anchor| matches!(anchor, CommentAnchor::Orphaned { .. })) {
            return vec![EditorEffect::ShowOrphanedComment { comment_id }];
        }
        let document = anchor
            .map(CommentAnchor::document_id)
            .map(str::to_owned)
            .or(current_document);
        let view = document
            .as_deref()
            .and_then(|document| self.last_focused_view.get(document))
            .copied()
            .unwrap_or_else(|| self.pane(self.focused_pane).view);
        let highlight = anchor.is_none_or(CommentAnchor::is_navigable);
        self.decorations.entry(view).or_default().comment_highlight = Some(comment_id.clone());
        vec![EditorEffect::NavigateCommentAnchor {
            view,
            comment_id,
            highlight,
        }]
    }

    fn set_spelling_decorations(
        &mut self,
        view: ViewId,
        decorations: Vec<SpellingDecoration>,
    ) -> Vec<EditorEffect> {
        let Some(state) = self.decorations.get_mut(&view) else {
            return Vec::new();
        };
        state.spelling = decorations.clone();
        self.spellcheck_errors.remove(&view);
        vec![EditorEffect::SetSpellcheckDecorations { view, decorations }]
    }

    fn choose_spelling_action(
        &self,
        pane: EditorPane,
        word: String,
        action: SpellingMenuAction,
    ) -> Vec<EditorEffect> {
        let view = self.pane(pane).view;
        match action {
            SpellingMenuAction::Replace(replacement) => vec![EditorEffect::Command {
                view,
                command: EditorCommand::ReplaceSpelling {
                    misspelling: word,
                    replacement,
                },
            }],
            SpellingMenuAction::AddToDictionary(scope) => {
                vec![EditorEffect::SpellingDictionaryAction {
                    view,
                    word,
                    scope,
                    add: true,
                }]
            }
            SpellingMenuAction::RemoveFromDictionary(scope) => {
                vec![EditorEffect::SpellingDictionaryAction {
                    view,
                    word,
                    scope,
                    add: false,
                }]
            }
            SpellingMenuAction::Ignore => Vec::new(),
        }
    }

    fn focused_search_mut(&mut self) -> &mut LocalSearchState {
        let view = self.pane(self.focused_pane).view;
        self.local_search.entry(view).or_default()
    }

    fn reset_view_transients(&mut self, view: ViewId) {
        self.local_search.insert(view, LocalSearchState::default());
        self.decorations.insert(view, EditorDecorations::default());
        self.selection_word_counts.insert(view, None);
        self.spellcheck_errors.remove(&view);
        self.invalidate_view_tasks(view);
    }

    fn pane_for_view(&self, view: ViewId) -> Option<&EditorPaneState> {
        [&self.primary, &self.companion]
            .into_iter()
            .find(|pane| pane.view == view && pane.is_populated())
    }

    fn ticket_is_live(&self, ticket: &AsyncEditorTicket) -> bool {
        self.pane_for_view(ticket.task.view()).is_some_and(|pane| {
            pane.mount_generation == ticket.mount_generation
                && pane.active_document() == ticket.document_id.as_deref()
                && ticket.document_id.as_deref().is_some_and(|document_id| {
                    self.document_revisions
                        .get(document_id)
                        .copied()
                        .unwrap_or_default()
                        == ticket.document_revision
                })
        })
    }

    fn set_document_revision(&mut self, document_id: String, revision: u64) {
        let previous = self
            .document_revisions
            .insert(document_id.clone(), revision);
        if previous == Some(revision) {
            return;
        }
        let views = [&self.primary, &self.companion]
            .into_iter()
            .filter(|pane| pane.active_document() == Some(document_id.as_str()))
            .map(|pane| pane.view)
            .collect::<Vec<_>>();
        for view in views {
            self.invalidate_view_tasks(view);
        }
    }

    fn invalidate_view_tasks(&mut self, view: ViewId) {
        self.pending.retain(|task, _| task.view() != view);
    }

    fn apply_async_payload(
        &mut self,
        view: ViewId,
        document_id: Option<&str>,
        payload: AsyncEditorPayload,
    ) -> bool {
        match payload {
            AsyncEditorPayload::SpellcheckApplied => {
                self.spellcheck_errors.remove(&view);
                true
            }
            AsyncEditorPayload::SpellcheckDecorations(decorations) => {
                !self.set_spelling_decorations(view, decorations).is_empty()
            }
            AsyncEditorPayload::SpellcheckFailed(error) => {
                self.spellcheck_errors.insert(view, error);
                true
            }
            AsyncEditorPayload::CommentsLoaded(comments) => {
                if document_id.is_none_or(|document_id| {
                    comments
                        .iter()
                        .any(|(_, anchor)| anchor.document_id() != document_id)
                }) {
                    return false;
                }
                self.comments.extend(comments);
                true
            }
            AsyncEditorPayload::WordCount(words) => {
                let Some(document_id) = document_id else {
                    return false;
                };
                self.document_word_counts
                    .insert(document_id.to_owned(), words);
                true
            }
        }
    }
}

struct HydratedDocument {
    id: String,
    title: String,
}

struct HydratedDocuments {
    ordered: Vec<HydratedDocument>,
    initial_document: Option<String>,
    visibility: BTreeMap<String, DocumentVisibility>,
    word_counts: BTreeMap<String, usize>,
    revisions: BTreeMap<String, u64>,
    manuscript_total: usize,
}

impl HydratedDocuments {
    fn from_snapshot(snapshot: &ProjectSnapshot) -> Self {
        let snapshots = snapshot
            .documents
            .iter()
            .map(|document| (stable_id_string(document.document_id.as_bytes()), document))
            .collect::<BTreeMap<_, _>>();
        let mut ordered = Vec::new();
        let mut manuscript_documents = Vec::new();
        let mut research_documents = Vec::new();
        for section in [ProjectSection::Manuscript, ProjectSection::Research] {
            append_section_documents(
                &snapshot.project,
                section.root_id(),
                section,
                &snapshots,
                &mut ordered,
                &mut manuscript_documents,
                &mut research_documents,
            );
        }
        let initial_document = manuscript_documents
            .first()
            .or_else(|| research_documents.first())
            .cloned();
        let visibility = snapshots
            .iter()
            .map(|(id, document)| (id.clone(), document.visibility))
            .collect();
        let word_counts = snapshots
            .iter()
            .map(|(id, document)| (id.clone(), count_words(&document.body)))
            .collect();
        let revisions = snapshots
            .iter()
            .map(|(id, document)| (id.clone(), document.revision.value()))
            .collect();
        let manuscript_total = manuscript_documents
            .iter()
            .filter_map(|id| snapshots.get(id))
            .map(|document| count_words(&document.body))
            .sum();
        Self {
            ordered,
            initial_document,
            visibility,
            word_counts,
            revisions,
            manuscript_total,
        }
    }
}

fn append_section_documents(
    project: &parchmint_domain::Project,
    node_id: parchmint_domain::NodeId,
    section: ProjectSection,
    snapshots: &BTreeMap<String, &parchmint_application::DocumentSnapshot>,
    ordered: &mut Vec<HydratedDocument>,
    manuscript_documents: &mut Vec<String>,
    research_documents: &mut Vec<String>,
) {
    if let Some(node) = project.nodes.get(node_id)
        && let NodeKind::Document(document_id) = node.kind
    {
        let id = stable_id_string(document_id.as_bytes());
        if snapshots.contains_key(&id) {
            ordered.push(HydratedDocument {
                id: id.clone(),
                title: node.title.clone(),
            });
            match section {
                ProjectSection::Manuscript => manuscript_documents.push(id),
                ProjectSection::Research => research_documents.push(id),
            }
        }
    }
    for child in project.nodes.children(node_id) {
        append_section_documents(
            project,
            *child,
            section,
            snapshots,
            ordered,
            manuscript_documents,
            research_documents,
        );
    }
}

fn production_view_id(snapshot: &ProjectSnapshot, pane: EditorPane) -> ViewId {
    let mut bytes = *snapshot.project.id.as_bytes();
    bytes[15] ^= match pane {
        EditorPane::Primary => 0xa1,
        EditorPane::Companion => 0xa2,
    };
    ViewId::from_bytes(bytes)
}

fn count_words(body: &str) -> usize {
    body.split_whitespace().count()
}

fn stable_id_string(bytes: &[u8; 16]) -> String {
    use std::fmt::Write as _;

    let mut serialized = String::with_capacity(32);
    for byte in bytes {
        write!(&mut serialized, "{byte:02x}").expect("writing to a String cannot fail");
    }
    serialized
}

fn search_active(search: Option<&LocalSearchState>) -> Option<FindMatch> {
    search.and_then(LocalSearchState::active_match)
}

fn payload_matches_task(task: &EditorTask, payload: &AsyncEditorPayload) -> bool {
    matches!(
        (task, payload),
        (
            EditorTask::RefreshSpellcheck { .. },
            AsyncEditorPayload::SpellcheckApplied
                | AsyncEditorPayload::SpellcheckDecorations(_)
                | AsyncEditorPayload::SpellcheckFailed(_)
        ) | (
            EditorTask::LoadComments { .. },
            AsyncEditorPayload::CommentsLoaded(_)
        ) | (
            EditorTask::RefreshWordCount { .. },
            AsyncEditorPayload::WordCount(_)
        )
    )
}

fn fit_tab_title(title: &str, width: f32) -> (String, Option<String>) {
    let capacity = ((width - TAB_CLOSE_WIDTH - TAB_TITLE_INSET) / APPROXIMATE_TITLE_SCALAR_WIDTH)
        .floor()
        .max(2.0) as usize;
    if title.chars().count() <= capacity {
        return (title.to_owned(), None);
    }
    let mut display = title
        .chars()
        .take(capacity.saturating_sub(1).max(1))
        .collect::<String>();
    display.push('…');
    (display, Some(title.to_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn activating_a_different_tab_invalidates_the_old_document_ticket() {
        let mut workspace = EditorWorkspace::from_fixture(EditorFixture::DualPane);
        let view = workspace.pane(EditorPane::Primary).view();
        let ticket = workspace.begin_task(EditorTask::RefreshWordCount { view });
        workspace.update(EditorMessage::OpenTab {
            pane: EditorPane::Primary,
            tab: TabSpec::new("chapter-three", "Chapter Three"),
        });

        assert!(
            !workspace.accept_completion(AsyncEditorCompletion::for_ticket(
                ticket,
                AsyncEditorPayload::WordCount(999),
            ))
        );
    }

    #[test]
    fn advancing_the_same_document_revision_invalidates_pending_analysis() {
        let mut workspace = EditorWorkspace::from_fixture(EditorFixture::SameDocumentTwoViews);
        let primary = workspace.pane(EditorPane::Primary).view();
        let companion = workspace.pane(EditorPane::Companion).view();
        let primary_ticket = workspace.begin_task(EditorTask::RefreshSpellcheck { view: primary });
        let companion_ticket =
            workspace.begin_task(EditorTask::RefreshSpellcheck { view: companion });

        workspace.update(EditorMessage::SetDocumentRevision {
            document_id: "chapter-one".to_owned(),
            revision: 1,
        });

        for ticket in [primary_ticket, companion_ticket] {
            assert!(
                !workspace.accept_completion(AsyncEditorCompletion::for_ticket(
                    ticket,
                    AsyncEditorPayload::SpellcheckApplied,
                ))
            );
        }
    }

    #[test]
    fn a_payload_for_a_different_task_cannot_consume_the_live_ticket() {
        let mut workspace = EditorWorkspace::from_fixture(EditorFixture::DualPane);
        let view = workspace.pane(EditorPane::Primary).view();
        let ticket = workspace.begin_task(EditorTask::RefreshWordCount { view });

        assert!(
            !workspace.accept_completion(AsyncEditorCompletion::for_ticket(
                ticket.clone(),
                AsyncEditorPayload::SpellcheckApplied,
            ))
        );
        assert!(
            workspace.accept_completion(AsyncEditorCompletion::for_ticket(
                ticket,
                AsyncEditorPayload::WordCount(450),
            ))
        );
        assert_eq!(
            workspace.status_bar().current_count(),
            StatusCount::ActiveDocument(450)
        );
    }

    #[test]
    fn reopening_the_active_document_focuses_without_remounting_or_invalidating_work() {
        let mut workspace = EditorWorkspace::from_fixture(EditorFixture::DualPane);
        let view = workspace.pane(EditorPane::Primary).view();
        let ticket = workspace.begin_task(EditorTask::RefreshWordCount { view });

        assert!(
            workspace
                .update(EditorMessage::OpenTab {
                    pane: EditorPane::Primary,
                    tab: TabSpec::new("chapter-one", "Replacement title is ignored"),
                })
                .is_empty()
        );
        assert!(
            workspace.accept_completion(AsyncEditorCompletion::for_ticket(
                ticket,
                AsyncEditorPayload::WordCount(413),
            ))
        );
    }

    #[test]
    fn spellcheck_failure_is_visible_and_a_later_exact_success_recovers() {
        let mut workspace = EditorWorkspace::from_fixture(EditorFixture::DualPane);
        let view = workspace.pane(EditorPane::Primary).view();
        let failed = workspace.begin_task(EditorTask::RefreshSpellcheck { view });
        assert!(
            workspace.accept_completion(AsyncEditorCompletion::for_ticket(
                failed,
                AsyncEditorPayload::SpellcheckFailed("dictionary unavailable".to_owned()),
            ))
        );
        assert_eq!(
            workspace.spellcheck_error(view),
            Some("dictionary unavailable")
        );

        let recovered = workspace.begin_task(EditorTask::RefreshSpellcheck { view });
        assert!(
            workspace.accept_completion(AsyncEditorCompletion::for_ticket(
                recovered,
                AsyncEditorPayload::SpellcheckApplied,
            ))
        );
        assert_eq!(workspace.spellcheck_error(view), None);
    }

    #[test]
    fn spelling_menu_exposes_ranked_replacements_and_applicable_dictionary_actions() {
        let mut workspace = EditorWorkspace::from_fixture(EditorFixture::DualPane);
        let request = SpellingMenuRequest::new(
            EditorPane::Primary,
            "teh",
            Rect::new(100.0, 100.0, 24.0, 18.0),
            Rect::new(0.0, 0.0, 500.0, 400.0),
        )
        .with_suggestions(vec!["the".to_owned(), "tech".to_owned()])
        .with_dictionary_membership(false, true);
        let effects = workspace.update(EditorMessage::OpenSpellingMenu(request));
        let [EditorEffect::ShowSpellingMenu(menu)] = effects.as_slice() else {
            panic!("expected the spelling menu")
        };

        assert_eq!(
            menu.actions(),
            [
                SpellingMenuAction::Replace("the".to_owned()),
                SpellingMenuAction::Replace("tech".to_owned()),
                SpellingMenuAction::AddToDictionary(SpellingDictionaryScope::Project),
                SpellingMenuAction::RemoveFromDictionary(SpellingDictionaryScope::Global),
                SpellingMenuAction::Ignore,
            ]
        );
    }

    #[test]
    fn closing_the_last_companion_tab_collapses_that_pane_without_deleting_a_document() {
        let mut workspace = EditorWorkspace::from_fixture(EditorFixture::DualPane);
        let effects = workspace.update(EditorMessage::CloseTab {
            pane: EditorPane::Companion,
            document_id: "chapter-two".to_owned(),
        });

        assert!(!workspace.pane(EditorPane::Companion).is_populated());
        assert_eq!(workspace.focused_pane(), EditorPane::Primary);
        assert!(matches!(
            effects.as_slice(),
            [EditorEffect::UnmountView { .. }]
        ));
    }

    #[test]
    fn document_level_and_orphaned_comments_do_not_claim_a_text_highlight() {
        let mut workspace = EditorWorkspace::from_fixture(EditorFixture::DualPane);
        workspace.update(EditorMessage::SetCommentAnchor {
            comment_id: "document".to_owned(),
            anchor: CommentAnchor::Document {
                document_id: "chapter-one".to_owned(),
            },
        });
        workspace.update(EditorMessage::SetCommentAnchor {
            comment_id: "orphaned".to_owned(),
            anchor: CommentAnchor::Orphaned {
                document_id: "chapter-one".to_owned(),
            },
        });

        assert!(matches!(
            workspace
                .update(EditorMessage::SelectComment("document".to_owned()))
                .as_slice(),
            [EditorEffect::NavigateCommentAnchor {
                highlight: false,
                ..
            }]
        ));
        assert_eq!(
            workspace.update(EditorMessage::SelectComment("orphaned".to_owned())),
            [EditorEffect::ShowOrphanedComment {
                comment_id: "orphaned".to_owned(),
            }]
        );
    }
}
