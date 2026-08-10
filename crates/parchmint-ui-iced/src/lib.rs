//! Iced desktop presentation state for the shell and editor workspace.

mod editor_workspace;
#[allow(
    dead_code,
    reason = "the private Iced surface is exercised by headless fixture tests"
)]
mod iced_editor_surface;

pub use editor_workspace::*;

use std::collections::{BTreeMap, BTreeSet};

use parchmint_platform_api::WindowCapability;
use parchmint_preferences::{AppearanceMode, ResolvedAppearance};

/// The three application appearance choices supported by the shell.
pub const SUPPORTED_APPEARANCES: &[AppearanceMode] = &[
    AppearanceMode::System,
    AppearanceMode::Light,
    AppearanceMode::Dark,
];

/// A message received by one shell window.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShellMessage {
    OpenMenu {
        window: WindowCapability,
        menu: MenuKind,
    },
    CloseMenu {
        window: WindowCapability,
    },
    SelectDestination {
        window: WindowCapability,
        destination: RibbonDestination,
    },
    OpenGlobalSearch {
        window: WindowCapability,
    },
    Focus {
        window: WindowCapability,
        target: FocusTarget,
    },
    FocusNextRegion {
        window: WindowCapability,
    },
    OpenDialog {
        window: WindowCapability,
        dialog: DialogKind,
    },
    DismissDialog {
        window: WindowCapability,
    },
    SetExplorerVisible {
        window: WindowCapability,
        visible: bool,
    },
    SetInspectorVisible {
        window: WindowCapability,
        visible: bool,
    },
}

impl ShellMessage {
    fn window(&self) -> WindowCapability {
        match self {
            Self::OpenMenu { window, .. }
            | Self::CloseMenu { window }
            | Self::SelectDestination { window, .. }
            | Self::OpenGlobalSearch { window }
            | Self::Focus { window, .. }
            | Self::FocusNextRegion { window }
            | Self::OpenDialog { window, .. }
            | Self::DismissDialog { window }
            | Self::SetExplorerVisible { window, .. }
            | Self::SetInspectorVisible { window, .. } => *window,
        }
    }
}

/// A menu surface belonging to the shell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuKind {
    Application,
    Project,
    ExplorerContext,
    EditorContext,
    InspectorContext,
}

/// Workspace destinations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RibbonDestination {
    Editor,
    Cards,
    History,
    RecentlyDeleted,
    Export,
    Settings,
    GlobalSearch,
}

impl RibbonDestination {
    pub const fn is_ribbon(self) -> bool {
        !matches!(self, Self::GlobalSearch)
    }
}

/// An Inspector disclosure section.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum InspectorSection {
    Synopsis,
    Metadata,
    Comments,
}

/// A target that can own keyboard focus in the non-editor shell.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FocusTarget {
    None,
    ModeSwitch,
    FormattingToolbar,
    Explorer,
    ActiveTab,
    EditorDocument(String),
    Inspector,
    StatusBar,
    NewProjectAction,
    ProjectName,
}

/// Major regions in the documented F6 order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum F6Region {
    None,
    ModeSwitch,
    FormattingToolbar,
    Explorer,
    ActiveTab,
    FocusedEditor,
    Inspector,
    StatusBar,
}

impl F6Region {
    const ORDER: [Self; 7] = [
        Self::ModeSwitch,
        Self::FormattingToolbar,
        Self::Explorer,
        Self::ActiveTab,
        Self::FocusedEditor,
        Self::Inspector,
        Self::StatusBar,
    ];

    fn next(self) -> Self {
        match Self::ORDER.iter().position(|region| *region == self) {
            Some(index) => Self::ORDER[(index + 1) % Self::ORDER.len()],
            None => Self::ORDER[0],
        }
    }

    fn target(self) -> FocusTarget {
        match self {
            Self::None => FocusTarget::None,
            Self::ModeSwitch => FocusTarget::ModeSwitch,
            Self::FormattingToolbar => FocusTarget::FormattingToolbar,
            Self::Explorer => FocusTarget::Explorer,
            Self::ActiveTab => FocusTarget::ActiveTab,
            Self::FocusedEditor => FocusTarget::EditorDocument(String::new()),
            Self::Inspector => FocusTarget::Inspector,
            Self::StatusBar => FocusTarget::StatusBar,
        }
    }
}

/// A modal dialog.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DialogKind {
    CreateProject,
    LockedProject,
    SaveFailure,
    RestoreConfirmation,
}

/// An unsaved New Project form.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NewProjectDraft {
    title: String,
    destination: String,
    author: Option<String>,
}

impl NewProjectDraft {
    pub const fn language(&self) -> &'static str {
        "en-US"
    }

    pub fn focus(&self) -> FocusTarget {
        FocusTarget::ProjectName
    }

    pub fn title(&self) -> &str {
        &self.title
    }

    pub fn destination(&self) -> &str {
        &self.destination
    }

    pub fn author(&self) -> Option<&str> {
        self.author.as_deref()
    }

    pub fn set_title(&mut self, title: impl Into<String>) {
        self.title = title.into();
    }

    pub fn set_destination(&mut self, destination: impl Into<String>) {
        self.destination = destination.into();
    }

    pub fn set_author(&mut self, author: Option<String>) {
        self.author = author.filter(|author| !author.is_empty());
    }
}

/// One launcher recent-project row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecentProject {
    name: String,
    path: String,
    last_opened: String,
}

impl RecentProject {
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn path(&self) -> &str {
        &self.path
    }

    pub fn last_opened(&self) -> &str {
        &self.last_opened
    }
}

/// Launcher state.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LauncherState {
    recent_projects: Vec<RecentProject>,
    new_project: NewProjectDraft,
}

impl LauncherState {
    pub const fn is_visible(&self) -> bool {
        true
    }

    pub fn recent_projects(&self) -> &[RecentProject] {
        &self.recent_projects
    }

    pub fn new_project(&self) -> &NewProjectDraft {
        &self.new_project
    }

    pub fn new_project_mut(&mut self) -> &mut NewProjectDraft {
        &mut self.new_project
    }

    pub fn add_recent_project(
        &mut self,
        name: impl Into<String>,
        path: impl Into<String>,
        last_opened: impl Into<String>,
    ) {
        let path = path.into();
        self.recent_projects.retain(|project| project.path != path);
        self.recent_projects.insert(
            0,
            RecentProject {
                name: name.into(),
                path,
                last_opened: last_opened.into(),
            },
        );
    }
}

/// An integer rectangle in physical pixels.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PaneGeometry {
    x: u32,
    y: u32,
    width: u32,
    height: u32,
}

impl PaneGeometry {
    pub const MIN_SIDEBAR_WIDTH: u32 = 240;
    pub const MAX_SIDEBAR_WIDTH: u32 = 480;

    const fn new(x: u32, y: u32, width: u32, height: u32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    pub const fn x(&self) -> u32 {
        self.x
    }

    pub const fn y(&self) -> u32 {
        self.y
    }

    pub const fn width(&self) -> u32 {
        self.width
    }

    pub const fn height(&self) -> u32 {
        self.height
    }
}

/// Desktop workspace geometry.
#[derive(Debug, Clone, PartialEq)]
pub struct ShellLayout {
    requested_width: u32,
    requested_height: u32,
    scale: f32,
    explorer_width: u32,
    inspector_width: u32,
    explorer_visible: bool,
    inspector_visible: bool,
}

impl ShellLayout {
    pub const MIN_WINDOW_SIZE: (u32, u32) = (1280, 720);
    pub const RIBBON_HEIGHT: u32 = 52;
    pub const STATUS_BAR_HEIGHT: u32 = 32;
    pub const MIN_HIT_TARGET: u32 = 32;
    const DEFAULT_EXPLORER_WIDTH: u32 = 280;
    const DEFAULT_INSPECTOR_WIDTH: u32 = 360;

    pub fn for_window(width: u32, height: u32) -> Self {
        Self::for_window_at_scale(width, height, 1.0)
    }

    pub fn for_window_at_scale(width: u32, height: u32, scale: f32) -> Self {
        let scale = scale.max(1.0);
        Self {
            requested_width: width,
            requested_height: height,
            scale,
            explorer_width: Self::scale_dimension(Self::DEFAULT_EXPLORER_WIDTH, scale),
            inspector_width: Self::scale_dimension(Self::DEFAULT_INSPECTOR_WIDTH, scale),
            explorer_visible: true,
            inspector_visible: true,
        }
    }

    pub fn is_supported(&self) -> bool {
        self.requested_width >= Self::MIN_WINDOW_SIZE.0
            && self.requested_height >= Self::MIN_WINDOW_SIZE.1
    }

    pub fn hit_target_minimum(&self) -> u32 {
        Self::scale_dimension(Self::MIN_HIT_TARGET, self.scale)
    }

    pub fn has_no_clipped_controls(&self) -> bool {
        self.display_width() >= self.explorer().width() + self.inspector().width()
            && self.center().width() > 0
    }

    pub fn ribbon(&self) -> PaneGeometry {
        PaneGeometry::new(0, 0, self.display_width(), self.scaled(Self::RIBBON_HEIGHT))
    }

    pub fn status_bar(&self) -> PaneGeometry {
        let height = self.scaled(Self::STATUS_BAR_HEIGHT);
        PaneGeometry::new(
            0,
            self.display_height().saturating_sub(height),
            self.display_width(),
            height,
        )
    }

    pub fn explorer(&self) -> PaneGeometry {
        PaneGeometry::new(
            0,
            self.ribbon().height(),
            if self.explorer_visible {
                self.explorer_width
            } else {
                0
            },
            self.workspace_height(),
        )
    }

    pub fn inspector(&self) -> PaneGeometry {
        let width = if self.inspector_visible {
            self.inspector_width
        } else {
            0
        };
        PaneGeometry::new(
            self.display_width().saturating_sub(width),
            self.ribbon().height(),
            width,
            self.workspace_height(),
        )
    }

    pub fn center(&self) -> PaneGeometry {
        let explorer = self.explorer();
        let inspector = self.inspector();
        PaneGeometry::new(
            explorer.width(),
            self.ribbon().height(),
            self.display_width()
                .saturating_sub(explorer.width())
                .saturating_sub(inspector.width()),
            self.workspace_height(),
        )
    }

    pub fn resize_explorer(&mut self, width: u32) {
        self.explorer_width = self.clamp_sidebar(width);
    }

    pub fn resize_inspector(&mut self, width: u32) {
        self.inspector_width = self.clamp_sidebar(width);
    }

    pub fn set_explorer_visible(&mut self, visible: bool) {
        self.explorer_visible = visible;
    }

    pub fn set_inspector_visible(&mut self, visible: bool) {
        self.inspector_visible = visible;
    }

    fn display_width(&self) -> u32 {
        self.scaled(self.requested_width.max(Self::MIN_WINDOW_SIZE.0))
    }

    fn display_height(&self) -> u32 {
        self.scaled(self.requested_height.max(Self::MIN_WINDOW_SIZE.1))
    }

    fn workspace_height(&self) -> u32 {
        self.display_height()
            .saturating_sub(self.ribbon().height())
            .saturating_sub(self.status_bar().height())
    }

    fn clamp_sidebar(&self, width: u32) -> u32 {
        let minimum = self.scaled(PaneGeometry::MIN_SIDEBAR_WIDTH);
        let maximum = self.scaled(PaneGeometry::MAX_SIDEBAR_WIDTH);
        self.scaled(width).clamp(minimum, maximum)
    }

    fn scaled(&self, logical: u32) -> u32 {
        Self::scale_dimension(logical, self.scale)
    }

    fn scale_dimension(logical: u32, scale: f32) -> u32 {
        (logical as f32 * scale).round() as u32
    }
}

/// A shell task that may complete after its requesting view has changed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ShellTask {
    LoadRecentProjects,
    CreateProject,
    OpenProject,
    PersistWorkspace,
    InstallMenu,
}

/// A non-sensitive shell task failure suitable for presentation state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShellTaskError {
    message: String,
}

impl ShellTaskError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }

    pub fn message(&self) -> &str {
        &self.message
    }
}

/// A ticket identifying one specific async request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TaskTicket {
    window: WindowCapability,
    task: ShellTask,
    request: u64,
}

impl TaskTicket {
    pub fn window(&self) -> WindowCapability {
        self.window
    }

    pub fn task(&self) -> ShellTask {
        self.task
    }

    pub fn request(&self) -> u64 {
        self.request
    }
}

/// The delayed result of a single shell task.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskCompletion {
    ticket: TaskTicket,
    result: Result<(), ShellTaskError>,
}

impl TaskCompletion {
    /// Builds the first completion for a task, useful at shell startup.
    pub fn new(
        window: WindowCapability,
        task: ShellTask,
        result: Result<(), ShellTaskError>,
    ) -> Self {
        Self::for_ticket(
            TaskTicket {
                window,
                task,
                request: 1,
            },
            result,
        )
    }

    pub fn for_ticket(ticket: TaskTicket, result: Result<(), ShellTaskError>) -> Self {
        Self { ticket, result }
    }

    pub fn ticket(&self) -> TaskTicket {
        self.ticket
    }

    pub fn result(&self) -> Result<(), &ShellTaskError> {
        self.result.as_ref().copied()
    }
}

/// The framework-private non-editor shell model for a single native window.
#[derive(Debug, Clone)]
pub struct Shell {
    window: WindowCapability,
    launcher: LauncherState,
    destination: RibbonDestination,
    global_search_open: bool,
    expanded_inspector_sections: BTreeSet<InspectorSection>,
    inspector_has_document_context: bool,
    layout: ShellLayout,
    focus: FocusTarget,
    focus_region: F6Region,
    dialog: Option<DialogKind>,
    dialog_invoker: FocusTarget,
    menu: Option<MenuKind>,
    appearance: ResolvedAppearance,
    pending: BTreeMap<ShellTask, u64>,
}

impl Shell {
    pub fn new(window: WindowCapability) -> Self {
        Self {
            window,
            launcher: LauncherState::default(),
            destination: RibbonDestination::Editor,
            global_search_open: false,
            expanded_inspector_sections: BTreeSet::new(),
            inspector_has_document_context: false,
            layout: ShellLayout::for_window(
                ShellLayout::MIN_WINDOW_SIZE.0,
                ShellLayout::MIN_WINDOW_SIZE.1,
            ),
            focus: FocusTarget::None,
            focus_region: F6Region::None,
            dialog: None,
            dialog_invoker: FocusTarget::None,
            menu: None,
            appearance: ResolvedAppearance::Light,
            pending: BTreeMap::from([(ShellTask::LoadRecentProjects, 1)]),
        }
    }

    pub fn windows() -> ShellWindows {
        ShellWindows::default()
    }

    pub fn window(&self) -> WindowCapability {
        self.window
    }

    pub fn launcher(&self) -> &LauncherState {
        &self.launcher
    }

    pub fn launcher_mut(&mut self) -> &mut LauncherState {
        &mut self.launcher
    }

    pub fn recent_projects(&self) -> &[RecentProject] {
        self.launcher.recent_projects()
    }

    pub fn destination(&self) -> RibbonDestination {
        self.destination
    }

    pub const fn global_search_is_open(&self) -> bool {
        self.global_search_open
    }

    pub fn inspector_section_is_expanded(&self, section: InspectorSection) -> bool {
        self.expanded_inspector_sections.contains(&section)
    }

    pub const fn comments_are_available(&self) -> bool {
        self.inspector_has_document_context
    }

    pub fn layout(&self) -> &ShellLayout {
        &self.layout
    }

    pub fn set_layout(&mut self, layout: ShellLayout) {
        self.layout = layout;
    }

    pub fn focus_target(&self) -> FocusTarget {
        self.focus.clone()
    }

    pub fn focus_region(&self) -> F6Region {
        self.focus_region
    }

    pub const fn has_visible_focus(&self) -> bool {
        !matches!(self.focus_region, F6Region::None)
    }

    pub const fn dialog_kind(&self) -> Option<DialogKind> {
        self.dialog
    }

    pub const fn focus_is_trapped(&self) -> bool {
        self.dialog.is_some()
    }

    pub fn open_menu(&self) -> Option<MenuKind> {
        self.menu
    }

    pub fn resolved_appearance(&self) -> ResolvedAppearance {
        self.appearance
    }

    /// Applies one window-scoped message only when its exact capability is live.
    pub fn accept(&mut self, message: ShellMessage) -> bool {
        if message.window() != self.window {
            return false;
        }
        match message {
            ShellMessage::OpenMenu { menu, .. } => self.menu = Some(menu),
            ShellMessage::CloseMenu { .. } => self.menu = None,
            ShellMessage::SelectDestination { destination, .. } => {
                self.select_destination(destination)
            }
            ShellMessage::OpenGlobalSearch { .. } => self.open_global_search(),
            ShellMessage::Focus { target, .. } => self.focus(target),
            ShellMessage::FocusNextRegion { .. } => self.focus_next_region(),
            ShellMessage::OpenDialog { dialog, .. } => self.open_dialog(dialog),
            ShellMessage::DismissDialog { .. } => self.dismiss_dialog(),
            ShellMessage::SetExplorerVisible { visible, .. } => {
                self.layout.set_explorer_visible(visible);
            }
            ShellMessage::SetInspectorVisible { visible, .. } => {
                self.layout.set_inspector_visible(visible);
            }
        }
        true
    }

    /// Starts a request without blocking the update loop.
    pub fn begin_task(&mut self, task: ShellTask) -> TaskTicket {
        let request = self
            .pending
            .get(&task)
            .copied()
            .unwrap_or_default()
            .saturating_add(1);
        self.pending.insert(task, request);
        TaskTicket {
            window: self.window,
            task,
            request,
        }
    }

    /// Accepts only the completion belonging to the exact pending request.
    pub fn accept_completion(&mut self, completion: TaskCompletion) -> bool {
        let ticket = completion.ticket();
        if ticket.window != self.window || self.pending.get(&ticket.task) != Some(&ticket.request) {
            return false;
        }
        self.pending.remove(&ticket.task);
        completion.result().is_ok()
    }

    pub fn select_destination(&mut self, destination: RibbonDestination) {
        if destination.is_ribbon() {
            self.destination = destination;
        }
    }

    pub fn open_global_search(&mut self) {
        self.global_search_open = true;
    }

    pub fn close_global_search(&mut self) {
        self.global_search_open = false;
    }

    pub fn toggle_inspector_section(&mut self, section: InspectorSection) {
        if !self.expanded_inspector_sections.insert(section) {
            self.expanded_inspector_sections.remove(&section);
        }
    }

    pub fn focus(&mut self, target: FocusTarget) {
        self.inspector_has_document_context = matches!(target, FocusTarget::EditorDocument(_));
        self.focus_region = match &target {
            FocusTarget::ModeSwitch => F6Region::ModeSwitch,
            FocusTarget::FormattingToolbar => F6Region::FormattingToolbar,
            FocusTarget::Explorer => F6Region::Explorer,
            FocusTarget::ActiveTab => F6Region::ActiveTab,
            FocusTarget::EditorDocument(_) => F6Region::FocusedEditor,
            FocusTarget::Inspector => F6Region::Inspector,
            FocusTarget::StatusBar => F6Region::StatusBar,
            FocusTarget::None | FocusTarget::NewProjectAction | FocusTarget::ProjectName => {
                F6Region::None
            }
        };
        self.focus = target;
    }

    pub fn focus_next_region(&mut self) {
        self.focus_region = self.focus_region.next();
        self.focus = self.focus_region.target();
        self.inspector_has_document_context = matches!(self.focus, FocusTarget::EditorDocument(_));
    }

    pub fn open_dialog(&mut self, dialog: DialogKind) {
        self.dialog = Some(dialog);
        self.dialog_invoker = self.focus.clone();
        self.focus = match dialog {
            DialogKind::CreateProject => FocusTarget::ProjectName,
            DialogKind::SaveFailure => FocusTarget::StatusBar,
            DialogKind::LockedProject | DialogKind::RestoreConfirmation => FocusTarget::ActiveTab,
        };
        self.focus_region = F6Region::None;
    }

    pub fn dismiss_dialog(&mut self) {
        if self.dialog.take().is_some() {
            self.focus(self.dialog_invoker.clone());
        }
    }

    fn set_resolved_appearance(&mut self, appearance: ResolvedAppearance) {
        self.appearance = appearance;
    }
}

/// All open shell windows.
#[derive(Debug)]
pub struct ShellWindows {
    windows: BTreeMap<WindowCapability, Shell>,
    appearance: AppearanceMode,
    system_appearance: ResolvedAppearance,
}

impl Default for ShellWindows {
    fn default() -> Self {
        Self {
            windows: BTreeMap::new(),
            appearance: AppearanceMode::System,
            system_appearance: ResolvedAppearance::Light,
        }
    }
}

impl ShellWindows {
    pub fn insert(&mut self, window: WindowCapability, shell: Shell) -> Option<Shell> {
        if shell.window() != window {
            return None;
        }
        let mut shell = shell;
        shell.set_resolved_appearance(self.resolved_appearance());
        self.windows.insert(window, shell)
    }

    pub fn remove(&mut self, window: WindowCapability) -> Option<Shell> {
        self.windows.remove(&window)
    }

    pub fn values(&self) -> impl Iterator<Item = &Shell> {
        self.windows.values()
    }

    pub fn appearance(&self) -> AppearanceMode {
        self.appearance
    }

    pub fn set_appearance(&mut self, appearance: AppearanceMode) {
        self.appearance = appearance;
        self.apply_resolved_appearance();
    }

    pub fn set_system_appearance(&mut self, appearance: ResolvedAppearance) {
        self.system_appearance = appearance;
        if self.appearance == AppearanceMode::System {
            self.apply_resolved_appearance();
        }
    }

    fn apply_resolved_appearance(&mut self) {
        let appearance = match self.appearance {
            AppearanceMode::System => self.system_appearance,
            AppearanceMode::Light => ResolvedAppearance::Light,
            AppearanceMode::Dark => ResolvedAppearance::Dark,
        };
        for shell in self.windows.values_mut() {
            shell.set_resolved_appearance(appearance);
        }
    }

    fn resolved_appearance(&self) -> ResolvedAppearance {
        match self.appearance {
            AppearanceMode::System => self.system_appearance,
            AppearanceMode::Light => ResolvedAppearance::Light,
            AppearanceMode::Dark => ResolvedAppearance::Dark,
        }
    }
}

/// A reproducible shell state mapped to the maintained visual catalog.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FixtureSize {
    pub width: u32,
    pub height: u32,
    pub scale: u16,
}

/// One testable Light/Dark fixture descriptor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VisualFixture {
    pub id: &'static str,
    pub light_reference: &'static str,
    pub dark_reference: &'static str,
    pub size: FixtureSize,
    pub destinations: &'static [&'static str],
}

impl VisualFixture {
    pub const fn light_reference(&self) -> &'static str {
        self.light_reference
    }

    pub const fn dark_reference(&self) -> &'static str {
        self.dark_reference
    }

    pub fn from_id(id: &str) -> Option<Self> {
        ALL_VISUAL_FIXTURES
            .iter()
            .copied()
            .find(|fixture| fixture.id == id)
    }
}

pub const LAUNCHER_DEFAULT: VisualFixture = VisualFixture {
    id: "launcher-default",
    light_reference: "launcher-light",
    dark_reference: "launcher-dark",
    size: FixtureSize {
        width: 1440,
        height: 900,
        scale: 1,
    },
    destinations: &["launcher", "new-project", "open-project"],
};

pub const EDITOR_SINGLE_DEFAULT: VisualFixture = VisualFixture {
    id: "editor-single-default",
    light_reference: "editor-single-light",
    dark_reference: "editor-single-dark",
    size: FixtureSize {
        width: 1440,
        height: 900,
        scale: 1,
    },
    destinations: &[
        "editor",
        "cards",
        "history",
        "recently-deleted",
        "export",
        "settings",
    ],
};

pub const EDITOR_DUAL_DEFAULT: VisualFixture = VisualFixture {
    id: "editor-dual-default",
    light_reference: "editor-dual-light",
    dark_reference: "editor-dual-dark",
    size: FixtureSize {
        width: 1440,
        height: 900,
        scale: 1,
    },
    destinations: &["editor"],
};

pub const ALL_VISUAL_FIXTURES: &[VisualFixture] =
    &[LAUNCHER_DEFAULT, EDITOR_SINGLE_DEFAULT, EDITOR_DUAL_DEFAULT];
