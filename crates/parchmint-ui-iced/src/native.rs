//! Native Iced event-loop integration for the desktop executable.

use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    error::Error,
    fmt,
    fs::File,
    io::BufWriter,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use iced::widget::svg::Handle;
use iced::{
    Element, Event, Font, Length, Subscription, Task, Theme, event, font,
    futures::{SinkExt, StreamExt, channel::mpsc as futures_mpsc},
    keyboard, mouse,
    widget::{Space, button, column, container, row, svg, text, text_input},
    window,
};
use parchmint_application::{ReplacementEdit, ReplacementSelection};
use parchmint_design_system::production_icon_svg;
use parchmint_editor_api::{
    AtomicBlockKind, BlockFormatKind, BlockId, CanonicalComment, CanonicalCommentAnchor,
    CanonicalCommentMessage, CanonicalDocumentLoad, CommentId, EditorAdapter,
    EditorCommand as AdapterEditorCommand, EditorCommandKind, EditorCommandOrigin, EditorRevision,
    EditorSelection, InlineMarkKind, SharedEditorSession, StyleCatalogProjection, ViewId,
};
use parchmint_editor_core::EditorCoreSession;
use parchmint_editor_iced::{
    EditorIcedAdapter, EditorSurfaceTheme, EditorViewport, MountedEditorBinding,
    MountedEditorBindingConfig, MountedEditorClipboardIntent, MountedEditorSession,
};
use parchmint_export_api::{ExportNumbering, ExportProgress, ExportProgressSink, ExportRunOptions};
use parchmint_history_api::HistoryCursor;
use parchmint_platform_api::{
    ClipboardContent, ClipboardFormats, PathDialog, PathDialogKind, SystemAppearance,
    UntrustedClipboardContent, WindowCapability,
};
use parchmint_preferences::{
    AppearanceMode, RecentProject as PreferenceRecentProject, ResolvedAppearance,
};
use parchmint_ui_api::{
    DictionaryRevision, ExportArtifactAction, ExportOperationToken, ExportOutcome, LanguageId,
    ProjectSaveKind, ProjectSessionCapability, ProjectSnapshot, ProjectUiPorts, ProjectUiProject,
    RevisionedTextRange, SpellcheckGeneration, SpellcheckPriority, SpellcheckRequest,
    SpellcheckResult,
};
use parchmint_workspace_state::{ProjectIdentity, WorkspaceSnapshot};

use crate::{
    DragDestination, EditorEffect, EditorPane, HistoryCurrentDocument, LauncherState,
    NewProjectDraft, Point, ProjectEffect, ProjectMessage, ProjectTask, ProjectTaskCompletion,
    ProjectTaskPayload, ProjectTaskTicket, ProjectWorkspace, RecentProject, RibbonDestination,
    SelectionGesture, Shell, ShellLayout, SpellingDecoration, SpellingMenu, SpellingMenuAction,
    SpellingMenuRequest,
    async_service_feeds::{
        AsyncServiceFeeds, BlockingServiceJob, DeletedPreviewResult, HistoryListResult,
        HistoryPreviewResult, RecoveryAcceptanceTicket, RecoveryAcceptedResult,
        RecoveryDiscardedResult, RecoveryReconcileResult, SearchBatchResult, SearchRequest,
        SearchStart,
    },
    components::{self, ButtonKind, Interaction},
    design_tokens::{
        LAUNCHER_ACTION_ROW_HEIGHT, LAUNCHER_INSET, LAUNCHER_LAST_OPENED_ICON_SIZE,
        LAUNCHER_PROJECT_CARD_GAP, LAUNCHER_PROJECT_CARD_HEIGHT,
        LAUNCHER_PROJECT_CARD_HORIZONTAL_PADDING, LAUNCHER_PROJECT_CARD_VERTICAL_PADDING,
        LAUNCHER_PROJECT_CARD_WIDTH, LAUNCHER_PROJECT_HEADER_GAP, LAUNCHER_PROJECT_ICON_SIZE,
        LAUNCHER_PROJECT_LAST_OPENED_SIZE, LAUNCHER_PROJECT_METADATA_GAP,
        LAUNCHER_PROJECT_NAME_MAX_CHARS, LAUNCHER_PROJECT_NAME_SIZE,
        LAUNCHER_PROJECT_PATH_MAX_CHARS, LAUNCHER_PROJECT_PATH_SIZE, LAUNCHER_PROJECT_TITLE_WIDTH,
        LAUNCHER_RHYTHM, LAUNCHER_SUBTITLE_SIZE, LAUNCHER_TITLE_SIZE, LAUNCHER_WORDMARK_SIZE,
        ParchMintTheme,
    },
    iced_editor_surface::{EditorCenterMessage, EditorHostSlots, editor_center_surface},
    iced_project_surface::{
        ProjectSurfaceMessage, SidebarPanel, native_project_surface as workspace_surface,
    },
    project_runtime::{
        EditorEffectCompletion, EditorRuntimeIntent, NativeProjectEffectExecutor,
        ProjectEffectCompletion, ProjectRuntimeError, ResolvedDocumentMount, canonical_load,
    },
};

const LAUNCHER_CAPABILITY: WindowCapability = WindowCapability::new(u64::MAX, 1);

fn runtime_event(event: Event, status: event::Status, window: window::Id) -> Option<Message> {
    matches!(
        event,
        Event::Keyboard(_)
            | Event::Mouse(mouse::Event::ButtonPressed(_))
            | Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left))
            | Event::Window(window::Event::Resized(_))
    )
    .then_some(Message::RuntimeEvent {
        window,
        event,
        accelerator_fallback: status == event::Status::Ignored,
    })
}

fn resize_event(event: Event, status: event::Status, window: window::Id) -> Option<Message> {
    matches!(
        event,
        Event::Mouse(mouse::Event::CursorMoved { .. })
            | Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left))
    )
    .then_some(Message::RuntimeEvent {
        window,
        event,
        accelerator_fallback: status == event::Status::Ignored,
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeNewProjectRequest {
    pub title: String,
    pub destination: PathBuf,
    pub author: Option<String>,
}

/// A verification-only capture target owned by the native Iced driver.
///
/// This deliberately names production navigation state rather than any test
/// fixture. A project target therefore captures the window built from the
/// opened project's real services and snapshot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NativeCaptureTarget {
    Launcher,
    Project(RibbonDestination),
}

/// One explicitly authorized native render-target capture.
///
/// The driver requests a 1440 x 900 logical viewport and an Iced program
/// scale factor of two by default. The actual render target is always retained
/// unless a caller explicitly enables strict dimensions.
#[derive(Debug, Clone)]
pub struct NativeCaptureRequest {
    pub target: NativeCaptureTarget,
    pub appearance: ResolvedAppearance,
    pub output_path: PathBuf,
    pub exit_after_capture: bool,
    pub settled_frames: u8,
    logical_size: (u32, u32),
    scale: u32,
    required_size: Option<(u32, u32)>,
    completion: Arc<Mutex<Option<Result<NativeCapturePng, String>>>>,
}

impl NativeCaptureRequest {
    pub const LOGICAL_SIZE: (u32, u32) = (1440, 900);
    pub const SCALE: u32 = 2;
    pub const DEFAULT_PHYSICAL_SIZE: (u32, u32) = (
        Self::LOGICAL_SIZE.0 * Self::SCALE,
        Self::LOGICAL_SIZE.1 * Self::SCALE,
    );
    pub const DEFAULT_SETTLED_FRAMES: u8 = 3;

    pub fn new(
        target: NativeCaptureTarget,
        appearance: ResolvedAppearance,
        output_path: PathBuf,
    ) -> Result<Self, NativeDesktopError> {
        if !output_path.is_absolute() {
            return Err(NativeDesktopError::new(
                "native capture output must be an absolute, explicitly authorized path",
            ));
        }
        if output_path.exists() {
            return Err(NativeDesktopError::new(format!(
                "native capture output already exists: {}",
                output_path.display()
            )));
        }
        if output_path
            .extension()
            .is_none_or(|extension| extension != "png")
        {
            return Err(NativeDesktopError::new(
                "native capture output must have a .png extension",
            ));
        }
        Ok(Self {
            target,
            appearance,
            output_path,
            exit_after_capture: true,
            settled_frames: Self::DEFAULT_SETTLED_FRAMES,
            logical_size: Self::LOGICAL_SIZE,
            scale: Self::SCALE,
            required_size: None,
            completion: Arc::new(Mutex::new(None)),
        })
    }

    /// Configures the requested logical viewport and Iced program scale.
    pub fn configure_viewport(
        &mut self,
        logical_size: (u32, u32),
        scale: u32,
    ) -> Result<(), NativeDesktopError> {
        if logical_size.0 == 0 || logical_size.1 == 0 {
            return Err(NativeDesktopError::new(
                "native capture logical dimensions must be positive",
            ));
        }
        if !matches!(scale, 1 | 2) {
            return Err(NativeDesktopError::new(
                "native capture scale must be 1 or 2",
            ));
        }
        self.logical_size = logical_size;
        self.scale = scale;
        Ok(())
    }

    /// Makes the capture fail after writing when the actual render target does
    /// not match `size`. The default retains every successful screenshot.
    pub fn require_size(&mut self, size: Option<(u32, u32)>) -> Result<(), NativeDesktopError> {
        if size.is_some_and(|(width, height)| width == 0 || height == 0) {
            return Err(NativeDesktopError::new(
                "required native capture dimensions must be positive",
            ));
        }
        self.required_size = size;
        Ok(())
    }

    pub const fn logical_size(&self) -> (u32, u32) {
        self.logical_size
    }

    pub const fn scale(&self) -> u32 {
        self.scale
    }

    pub const fn requested_physical_size(&self) -> (u32, u32) {
        (
            self.logical_size.0 * self.scale,
            self.logical_size.1 * self.scale,
        )
    }

    fn validates_output(&self) -> Result<(), String> {
        if self.settled_frames == 0 {
            return Err("native capture requires at least one settled frame".to_owned());
        }
        if !self.output_path.is_absolute() {
            return Err(
                "native capture output must be an absolute, explicitly authorized path".to_owned(),
            );
        }
        if self.output_path.exists() {
            return Err(format!(
                "native capture output already exists: {}",
                self.output_path.display()
            ));
        }
        if self.logical_size.0 == 0 || self.logical_size.1 == 0 {
            return Err("native capture logical dimensions must be positive".to_owned());
        }
        if !matches!(self.scale, 1 | 2) {
            return Err("native capture scale must be 1 or 2".to_owned());
        }
        Ok(())
    }
}

/// One completed native PNG capture, including the physical render-target
/// dimensions reported by Iced.
#[derive(Debug, Clone, PartialEq, Eq)]
struct NativeCapturePng {
    output_path: PathBuf,
    physical_size: (u32, u32),
}

/// One project window that was registered before the native loop started.
#[derive(Clone)]
pub struct NativeProjectWindow {
    pub project: PathBuf,
    pub window: WindowCapability,
    pub session: ProjectSessionCapability,
    pub project_ui: Option<ProjectUiProject>,
    pub editor: Option<Arc<EditorIcedAdapter>>,
}

impl NativeProjectWindow {
    /// Creates the production handoff with matching neutral and native editor
    /// capabilities for the same exact project session.
    pub fn typed(
        project: PathBuf,
        window: WindowCapability,
        project_ui: ProjectUiProject,
        editor: Arc<EditorIcedAdapter>,
    ) -> Self {
        Self {
            project,
            window,
            session: project_ui.session(),
            project_ui: Some(project_ui),
            editor: Some(editor),
        }
    }

    pub fn ports(&self) -> Option<&ProjectUiPorts> {
        self.project_ui.as_ref().map(|project| &project.ports)
    }

    pub fn editor_adapter(&self) -> Option<&Arc<EditorIcedAdapter>> {
        self.editor.as_ref()
    }
}

impl fmt::Debug for NativeProjectWindow {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("NativeProjectWindow")
            .field("project", &self.project)
            .field("window", &self.window)
            .field("session", &self.session)
            .field("has_project_ui", &self.project_ui.is_some())
            .field("has_native_editor", &self.editor.is_some())
            .finish()
    }
}

impl PartialEq for NativeProjectWindow {
    fn eq(&self, other: &Self) -> bool {
        self.project == other.project
            && self.window == other.window
            && self.session == other.session
    }
}

impl Eq for NativeProjectWindow {}

/// The result of routing a launcher project-open request through the desktop.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NativeProjectOpenResult {
    Opened(NativeProjectWindow),
    Focused(WindowCapability),
    Locked,
}

/// Desktop lifecycle callbacks invoked by native window interactions.
pub trait NativeDesktopCallbacks: Send + Sync {
    fn open_project(&self, project: PathBuf) -> Result<NativeProjectOpenResult, String>;
    fn close_project(&self, project: PathBuf) -> Result<(), String>;

    /// Releases a project that has no unsaved editor revision without creating
    /// a needless final checkpoint. Implementations may fall back to their
    /// full close path when they cannot distinguish a clean project.
    fn close_clean_project(&self, project: PathBuf) -> Result<(), String> {
        self.close_project(project)
    }

    fn create_project(
        &self,
        _request: NativeNewProjectRequest,
    ) -> Result<NativeProjectOpenResult, String> {
        Err("project creation is unavailable".to_owned())
    }

    fn choose_project_directory(
        &self,
        _window: WindowCapability,
        _title: &'static str,
    ) -> Result<Option<PathBuf>, String> {
        Ok(None)
    }

    fn set_appearance(&self, _mode: AppearanceMode) -> Result<ResolvedAppearance, String> {
        Err("appearance settings are unavailable".to_owned())
    }

    /// Accepts an operating-system appearance event after the native driver
    /// has preserved its source ordering.
    fn system_appearance_changed(
        &self,
        _appearance: ResolvedAppearance,
    ) -> Result<Option<ResolvedAppearance>, String> {
        Ok(None)
    }

    /// Records the platform capability when this driver creates its native
    /// project window.
    fn project_window_created(&self, _window: WindowCapability) {}

    /// Retires the platform capability after this driver removes its native
    /// project window.
    fn project_window_destroyed(&self, _window: WindowCapability) {}
}

/// ParchMint-owned values supplied to the native Iced driver.
pub struct NativeDesktopStartup {
    pub appearance: ResolvedAppearance,
    pub appearance_mode: AppearanceMode,
    pub recent_projects: Vec<PreferenceRecentProject>,
    pub projects: Vec<NativeProjectWindow>,
    pub locked_project: Option<PathBuf>,
    /// Optional verification-only flow. This state is handled by the native
    /// driver and is never routed through product reducers.
    pub capture: Option<NativeCaptureRequest>,
    pub callbacks: Arc<dyn NativeDesktopCallbacks>,
}

/// A failure while creating or running the native desktop event loop.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeDesktopError {
    message: String,
}

impl NativeDesktopError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for NativeDesktopError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl Error for NativeDesktopError {}

/// Runs the native launcher and project windows until the user closes them.
pub fn run_native_desktop(startup: NativeDesktopStartup) -> Result<(), NativeDesktopError> {
    let capture_completion = startup
        .capture
        .as_ref()
        .map(|capture| Arc::clone(&capture.completion));
    let startup = Mutex::new(Some(startup));
    iced::daemon(
        move || {
            let startup = startup
                .lock()
                .expect("native desktop startup mutex poisoned")
                .take()
                .expect("native desktop may only boot once");
            NativeDesktop::boot(startup)
        },
        NativeDesktop::update,
        NativeDesktop::view,
    )
    .title(NativeDesktop::title)
    .theme(NativeDesktop::theme)
    .subscription(NativeDesktop::subscription)
    .scale_factor(|desktop, _| {
        desktop
            .capture
            .as_ref()
            .map_or(1.0, |capture| capture.request.scale() as f32)
    })
    .default_font(iced::Font::with_name("Source Sans 3"))
    .font(include_bytes!(
        "../assets/fonts/source-sans-3/SourceSans3-Regular.ttf"
    ))
    .font(include_bytes!(
        "../assets/fonts/source-sans-3/SourceSans3-Medium.ttf"
    ))
    .font(include_bytes!(
        "../assets/fonts/source-sans-3/SourceSans3-Semibold.ttf"
    ))
    .font(include_bytes!(
        "../assets/fonts/source-sans-3/SourceSans3-Bold.ttf"
    ))
    .font(include_bytes!(
        "../assets/fonts/source-serif-4/SourceSerif4-Regular.ttf"
    ))
    .run()
    .map_err(|error| NativeDesktopError::new(error.to_string()))?;
    if let Some(completion) = capture_completion {
        match completion
            .lock()
            .map_err(|_| NativeDesktopError::new("native capture completion mutex poisoned"))?
            .take()
        {
            Some(Ok(_)) => Ok(()),
            Some(Err(error)) => Err(NativeDesktopError::new(error)),
            None => Err(NativeDesktopError::new(
                "native desktop exited before the requested capture completed",
            )),
        }
    } else {
        Ok(())
    }
}

#[derive(Debug, Clone)]
enum Message {
    WindowOpened(window::Id),
    CaptureFrameTick,
    CaptureScreenshot(window::Screenshot),
    CaptureEncoded(Result<NativeCapturePng, String>),
    RuntimeEvent {
        window: window::Id,
        event: Event,
        accelerator_fallback: bool,
    },
    DismissContextMenus {
        window: window::Id,
    },
    CancelUncommittedHierarchyDrag {
        window: window::Id,
    },
    WorkspaceLoaded {
        window: window::Id,
        result: Result<Option<WorkspaceSnapshot>, String>,
    },
    WorkspacePersisted {
        result: Result<(), String>,
    },
    CloseRequested(window::Id),
    ShowNewProject,
    CancelNewProject,
    NewProjectTitleChanged(String),
    NewProjectDestinationChanged(String),
    NewProjectAuthorChanged(String),
    ChooseOpenProject,
    ChooseNewProjectDestination,
    DirectoryChosen {
        create: bool,
        result: Result<Option<PathBuf>, String>,
    },
    OpenRecentProject(PathBuf),
    CreateProject,
    ProjectOpenFinished {
        project: PathBuf,
        result: Result<NativeProjectOpenResult, String>,
    },
    ProjectSurface {
        window: window::Id,
        message: ProjectSurfaceMessage,
    },
    EditorProjectionPersisted {
        window: window::Id,
        ticket: AutosaveTicket,
        result: Result<ProjectionRun<AutosaveCompletion>, String>,
    },
    ClipboardWriteFinished {
        window: window::Id,
        request: NativeClipboardRequest,
        result: Result<(), String>,
    },
    ClipboardReadFinished {
        window: window::Id,
        request: NativeClipboardRequest,
        result: Result<UntrustedClipboardContent, String>,
    },
    AutosaveTick(Instant),
    SystemAppearanceObserved(SystemAppearance),
    #[cfg(not(target_os = "linux"))]
    SystemThemeMode(iced::theme::Mode),
    SystemAppearanceChangedFinished {
        generation: u64,
        result: Result<Option<ResolvedAppearance>, String>,
    },
    SystemAppearanceStreamFailed(String),
    SaveFinished {
        window: window::Id,
        purpose: SavePurpose,
        result: Result<u64, String>,
    },
    ProjectEffectFinished {
        window: window::Id,
        mutation: Option<ProjectMutationTicket>,
        result: Result<ProjectEffectCompletion, ProjectRuntimeError>,
    },
    EditorEffectFinished {
        window: window::Id,
        mutation: Option<OpaqueMutationToken>,
        result: Result<EditorEffectCompletion, ProjectRuntimeError>,
    },
    SpellcheckFinished {
        window: window::Id,
        ticket: NativeSpellcheckTicket,
        result: Result<SpellcheckResult, String>,
    },
    SearchFinished {
        window: window::Id,
        ticket: ProjectTaskTicket,
        result: Result<Vec<SearchBatchResult>, String>,
    },
    HistoryFinished {
        window: window::Id,
        ticket: ProjectTaskTicket,
        append: bool,
        result: Result<HistoryListResult, String>,
    },
    HistoryPreviewFinished {
        window: window::Id,
        ticket: ProjectTaskTicket,
        result: Result<HistoryPreviewResult, String>,
    },
    HistoryMaintenanceFinished {
        window: window::Id,
        result: Result<parchmint_ui_api::HistoryMaintenanceStatus, String>,
    },
    HistoryReinitialized {
        window: window::Id,
        mutation: Option<OpaqueMutationToken>,
        result: Result<String, String>,
    },
    ExportDestinationChosen {
        window: window::Id,
        result: Result<Option<parchmint_platform_api::UntrustedPathSelection>, String>,
    },
    DeletedPreviewFinished {
        window: window::Id,
        ticket: ProjectTaskTicket,
        result: Result<DeletedPreviewResult, String>,
    },
    ReplacementPreviewFinished {
        window: window::Id,
        ticket: ProjectTaskTicket,
        result: Result<(), String>,
    },
    ReplacementApplyFinished {
        window: window::Id,
        ticket: ProjectTaskTicket,
        mutation: OpaqueMutationToken,
        result: Box<Result<parchmint_ui_api::ProjectSnapshot, String>>,
    },
    ExportOperationStarted {
        window: window::Id,
        ticket: ProjectTaskTicket,
        operation: ExportOperationToken,
    },
    ExportProgressed {
        window: window::Id,
        ticket: ProjectTaskTicket,
        operation: ExportOperationToken,
        progress: ExportProgress,
    },
    ExportFinished {
        window: window::Id,
        ticket: ProjectTaskTicket,
        operation: Option<ExportOperationToken>,
        result: Result<ExportOutcome, String>,
    },
    ExportCancelFinished {
        window: window::Id,
        operation: ExportOperationToken,
        result: Result<parchmint_export_api::CancelOutcome, String>,
    },
    ExportArtifactActionFinished(Result<(), String>),
    RecoveryReconciled {
        window: window::Id,
        session: ProjectSessionCapability,
        ticket: ProjectTaskTicket,
        result: Result<RecoveryReconcileResult, String>,
    },
    RecoveryAccepted {
        window: window::Id,
        session: ProjectSessionCapability,
        ticket: ProjectTaskTicket,
        mutation: Option<OpaqueMutationToken>,
        result: Result<RecoveryAcceptedResult, String>,
    },
    RecoveryDiscarded {
        window: window::Id,
        session: ProjectSessionCapability,
        ticket: ProjectTaskTicket,
        mutation: Option<OpaqueMutationToken>,
        result: Result<RecoveryDiscardedResult, String>,
    },
    SelectDestination {
        window: window::Id,
        destination: RibbonDestination,
    },
    AppearanceSelected(AppearanceMode),
    AppearanceFinished {
        mode: AppearanceMode,
        result: Result<ResolvedAppearance, String>,
    },
    RetryClose(window::Id),
    CancelClose(window::Id),
    ProjectCloseFinished {
        window: window::Id,
        result: Result<ProjectionRun<CloseCompletion>, String>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HistoryWorkflowAction {
    NamedSnapshot,
    Restore,
}

enum ExportWorkerEvent {
    Progress(ExportProgress),
    Finished(Result<ExportOutcome, String>),
}

struct NativeExportProgressSink {
    sender: futures_mpsc::UnboundedSender<ExportWorkerEvent>,
}

impl ExportProgressSink for NativeExportProgressSink {
    fn report(&self, progress: ExportProgress) {
        let _ = self
            .sender
            .unbounded_send(ExportWorkerEvent::Progress(progress));
    }
}

#[cfg(any(test, target_os = "linux"))]
fn streamed_appearance_subscription(
    mode: AppearanceMode,
    stream: fn() -> iced::futures::stream::BoxStream<'static, Message>,
) -> Subscription<Message> {
    if mode == AppearanceMode::System {
        Subscription::run(stream)
    } else {
        Subscription::none()
    }
}

#[cfg(target_os = "linux")]
fn linux_portal_appearance_events() -> iced::futures::stream::BoxStream<'static, Message> {
    use ashpd::desktop::settings::{ColorScheme, Settings};

    Box::pin(iced::stream::channel(1, async move |mut output| {
        let settings = match Settings::new().await {
            Ok(settings) => settings,
            Err(error) => {
                let _ = output
                    .send(Message::SystemAppearanceStreamFailed(format!(
                        "System appearance updates are unavailable: {error}"
                    )))
                    .await;
                return;
            }
        };
        let mut changes = match settings.receive_color_scheme_changed().await {
            Ok(changes) => changes,
            Err(error) => {
                let _ = output
                    .send(Message::SystemAppearanceStreamFailed(format!(
                        "System appearance updates are unavailable: {error}"
                    )))
                    .await;
                return;
            }
        };
        while let Some(scheme) = changes.next().await {
            let appearance = match scheme {
                ColorScheme::PreferDark => Some(SystemAppearance::Dark),
                ColorScheme::PreferLight => Some(SystemAppearance::Light),
                ColorScheme::NoPreference => None,
            };
            if let Some(appearance) = appearance
                && output
                    .send(Message::SystemAppearanceObserved(appearance))
                    .await
                    .is_err()
            {
                break;
            }
        }
    }))
}

fn resolved_system_appearance(appearance: SystemAppearance) -> ResolvedAppearance {
    match appearance {
        SystemAppearance::Light => ResolvedAppearance::Light,
        SystemAppearance::Dark => ResolvedAppearance::Dark,
    }
}

fn keyboard_accelerator(key: &str, modifiers: keyboard::Modifiers) -> Option<&'static str> {
    let key = key.to_ascii_lowercase();
    if modifiers == keyboard::Modifiers::COMMAND {
        return match key.as_str() {
            "n" => Some("file.new"),
            "o" => Some("file.open"),
            "s" => Some("file.save"),
            "w" => Some("file.close"),
            "c" => Some("edit.copy"),
            "x" => Some("edit.cut"),
            "v" => Some("edit.paste"),
            "z" => Some("edit.undo"),
            #[cfg(not(target_os = "macos"))]
            "y" => Some("edit.redo"),
            _ => None,
        };
    }
    #[cfg(target_os = "macos")]
    if modifiers == (keyboard::Modifiers::COMMAND | keyboard::Modifiers::SHIFT) && key == "z" {
        return Some("edit.redo");
    }
    None
}

fn should_activate_shortcut(command: &str, accelerator_fallback: bool) -> bool {
    command.starts_with("file.") || accelerator_fallback
}

#[derive(Debug, Clone)]
struct NativeClipboardRequest {
    capability: WindowCapability,
    project_session: ProjectSessionCapability,
    pane: EditorPane,
    view: ViewId,
    editor_session: SharedEditorSession,
    revision: EditorRevision,
    selection: EditorSelection,
    intent: MountedEditorClipboardIntent,
}

pub(crate) struct NativeDesktop {
    appearance: ResolvedAppearance,
    appearance_mode: AppearanceMode,
    launcher: LauncherState,
    windows: BTreeMap<window::Id, NativeWindow>,
    project_windows: BTreeMap<WindowCapability, window::Id>,
    closing_windows: BTreeSet<window::Id>,
    close_failures: BTreeMap<WindowCapability, String>,
    opening_project: bool,
    creating_project: bool,
    status: Option<String>,
    callbacks: Arc<dyn NativeDesktopCallbacks>,
    last_appearance_generation: u64,
    capture: Option<NativeCaptureState>,
}

/// Ephemeral driver state for one native render capture. It intentionally has
/// no product message or reducer representation.
#[derive(Debug, Clone)]
struct NativeCaptureState {
    request: NativeCaptureRequest,
    window: Option<window::Id>,
    window_opened: bool,
    settled_frames: u8,
    screenshot_requested: bool,
}

enum NativeWindow {
    Launcher,
    Project(Box<NativeProjectState>),
}

struct NativeProjectState {
    project: NativeProjectWindow,
    shell: Shell,
    workspace: Option<Box<ProjectWorkspace>>,
    editor_hosts: EditorHostSlots,
    editor_bindings: BTreeMap<EditorPane, MountedEditorBinding>,
    mounted_documents: BTreeMap<EditorPane, parchmint_domain::DocumentId>,
    /// Detached tabs retain their process-local adapter session. Reopening a
    /// tab must never replace edits which have not reached project persistence
    /// with the stale document snapshot carried by a mount effect.
    retained_editor_sessions: BTreeMap<parchmint_domain::DocumentId, SharedEditorSession>,
    effect_executor: Option<NativeProjectEffectExecutor>,
    synopsis_commits: SynopsisCommitQueue,
    project_mutations: ProjectMutationState,
    opaque_mutations: OpaqueMutationState,
    service_feeds: Option<AsyncServiceFeeds>,
    recovery_acceptance: Option<RecoveryAcceptanceTicket>,
    active_export: Option<ExportOperationToken>,
    export_destination: Option<parchmint_platform_api::UntrustedPathSelection>,
    autosave: AutosaveState,
    next_spellcheck_generation: u64,
    spellcheck_generation: BTreeMap<ViewId, u64>,
    pending_spellchecks: BTreeMap<ViewId, Instant>,
    spelling_issues: BTreeMap<ViewId, Vec<NativeSpellingIssue>>,
    pending_spelling_menu: Option<NativeSpellingMenuContext>,
    spelling_menu: Option<SpellingMenu>,
    /// The native event subscription receives the same button press that a
    /// widget uses to open a context menu. Consume precisely that follow-up
    /// dismissal, then let every later press close the menu normally.
    suppress_next_context_menu_dismissal: bool,
    refresh_spellcheck_view: Option<ViewId>,
    modifiers: keyboard::Modifiers,
    resizing: Option<SidebarPanel>,
    modal_focus: ModalFocus,
}

#[derive(Debug, Clone, PartialEq)]
struct ProjectMutationTicket {
    effect: ProjectEffect,
    history_action: Option<HistoryWorkflowAction>,
    synopsis_commit: Option<SynopsisCommit>,
}

#[derive(Debug, Default)]
struct ProjectMutationState {
    active: Option<ProjectMutationTicket>,
    queued: VecDeque<ProjectMutationTicket>,
    failed_effect: Option<ProjectMutationTicket>,
    failed_save: Option<ProjectMutationTicket>,
}

impl ProjectMutationState {
    fn enqueue(&mut self, ticket: ProjectMutationTicket) -> Option<ProjectMutationTicket> {
        self.queued.push_back(ticket);
        None
    }

    fn succeed(&mut self, ticket: &ProjectMutationTicket) -> Option<ProjectMutationTicket> {
        if self.active.as_ref() != Some(ticket) {
            return None;
        }
        self.active = None;
        None
    }

    fn fail_effect(&mut self, ticket: ProjectMutationTicket) {
        if self.active.as_ref() == Some(&ticket) {
            self.active = None;
        }
        self.failed_effect = Some(ticket);
    }

    fn fail_save(&mut self, ticket: ProjectMutationTicket) {
        if self.active.as_ref() == Some(&ticket) {
            self.active = None;
        }
        self.failed_save = Some(ticket);
    }

    fn retry_failed_effect(&mut self) -> Option<ProjectMutationTicket> {
        if self.active.is_some() {
            return None;
        }
        self.active = self.failed_effect.take();
        self.active.clone()
    }

    fn retry_failed_save(&mut self) -> Option<ProjectMutationTicket> {
        if self.active.is_some() {
            return None;
        }
        self.active = self.failed_save.take();
        self.active.clone()
    }

    fn start_next(&mut self) -> Option<ProjectMutationTicket> {
        if self.active.is_some() {
            return None;
        }
        self.active = self.queued.pop_front();
        self.active.clone()
    }

    fn blocks_close(&self) -> bool {
        self.active.is_some()
            || self.failed_effect.is_some()
            || self.failed_save.is_some()
            || !self.queued.is_empty()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum OpaqueMutationKind {
    Replacement,
    ProjectDictionary,
    HistoryReinitialize,
    RecoveryAccept,
    RecoveryDiscard,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct OpaqueMutationToken {
    sequence: u64,
    kind: OpaqueMutationKind,
}

struct PendingOpaqueMutation {
    token: OpaqueMutationToken,
    prior_projects: usize,
    launch: Arc<dyn Fn(&NativeProjectState) -> Task<Message>>,
}

#[derive(Default)]
struct OpaqueMutationState {
    next: u64,
    active: Option<PendingOpaqueMutation>,
    queued: VecDeque<PendingOpaqueMutation>,
    failed_effect: Option<PendingOpaqueMutation>,
    failed_save: Option<OpaqueMutationToken>,
}

impl OpaqueMutationState {
    fn begin(&mut self, kind: OpaqueMutationKind) -> OpaqueMutationToken {
        self.next = self.next.saturating_add(1);
        OpaqueMutationToken {
            sequence: self.next,
            kind,
        }
    }

    fn enqueue(
        &mut self,
        token: OpaqueMutationToken,
        prior_projects: usize,
        launch: impl Fn(&NativeProjectState) -> Task<Message> + 'static,
    ) {
        self.queued.push_back(PendingOpaqueMutation {
            token,
            prior_projects,
            launch: Arc::new(launch),
        });
    }

    fn finish(&mut self, token: OpaqueMutationToken) {
        if self
            .active
            .as_ref()
            .is_some_and(|active| active.token == token)
        {
            self.active.take();
        }
        if self
            .failed_effect
            .as_ref()
            .is_some_and(|failed| failed.token == token)
        {
            self.failed_effect.take();
        }
        if self.failed_save == Some(token) {
            self.failed_save = None;
        }
    }

    fn fail_save(&mut self, token: OpaqueMutationToken) {
        if self
            .active
            .as_ref()
            .is_some_and(|active| active.token == token)
        {
            self.active.take();
            self.failed_save = Some(token);
        }
    }

    fn fail_effect(&mut self, token: OpaqueMutationToken) {
        if self
            .active
            .as_ref()
            .is_some_and(|active| active.token == token)
        {
            self.failed_effect = self.active.take();
        }
    }

    fn blocks_close(&self) -> bool {
        self.active.is_some()
            || !self.queued.is_empty()
            || self.failed_effect.is_some()
            || self.failed_save.is_some()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PersistentMutationLane {
    Project,
    Opaque(OpaqueMutationKind),
}

fn next_persistent_mutation_lane(
    projects: &ProjectMutationState,
    opaque: &OpaqueMutationState,
) -> Option<PersistentMutationLane> {
    if projects.active.is_some()
        || projects.failed_effect.is_some()
        || projects.failed_save.is_some()
        || opaque.active.is_some()
        || opaque.failed_effect.is_some()
        || opaque.failed_save.is_some()
    {
        return None;
    }
    if opaque
        .queued
        .front()
        .is_some_and(|pending| pending.prior_projects == 0)
    {
        return opaque
            .queued
            .front()
            .map(|pending| PersistentMutationLane::Opaque(pending.token.kind));
    }
    if !projects.queued.is_empty() {
        return Some(PersistentMutationLane::Project);
    }
    opaque
        .queued
        .front()
        .map(|pending| PersistentMutationLane::Opaque(pending.token.kind))
}

#[derive(Debug, Clone)]
enum PersistentMutationTerminal {
    ProjectSucceeded(ProjectMutationTicket),
    ProjectEffectFailed(ProjectMutationTicket),
    ProjectSaveFailed(ProjectMutationTicket),
    OpaqueSucceeded(OpaqueMutationToken),
    OpaqueSaveFailed(OpaqueMutationToken),
    OpaqueFailed(OpaqueMutationToken),
}

/// One user-visible Synopsis revision waiting for the native persistence lane.
///
/// Project commands are revisioned, so launching these independently lets two
/// keypresses race on the same captured snapshot. Keep one in flight and
/// coalesce a later draft for the same node until the snapshot is refreshed.
#[derive(Debug, Clone, PartialEq, Eq)]
struct SynopsisCommit {
    node_id: String,
    synopsis: String,
}

#[derive(Debug, Default)]
struct SynopsisCommitQueue {
    in_flight: Option<SynopsisCommit>,
    queued: VecDeque<SynopsisCommit>,
}

impl SynopsisCommitQueue {
    fn enqueue(&mut self, commit: SynopsisCommit) -> Option<SynopsisCommit> {
        if let Some(queued) = self
            .queued
            .iter_mut()
            .find(|queued| queued.node_id == commit.node_id)
        {
            queued.synopsis = commit.synopsis;
        } else {
            self.queued.push_back(commit);
        }
        (self.in_flight.is_none())
            .then(|| self.start_next())
            .flatten()
    }

    fn finish(&mut self, completed: &SynopsisCommit, succeeded: bool) -> Option<SynopsisCommit> {
        if self.in_flight.as_ref() != Some(completed) {
            return None;
        }
        self.in_flight = None;
        if !succeeded {
            return None;
        }
        self.start_next()
    }

    fn retain_latest_failed(&mut self, failed: &SynopsisCommit) -> SynopsisCommit {
        let latest = self
            .queued
            .iter()
            .rev()
            .find(|queued| queued.node_id == failed.node_id)
            .cloned()
            .unwrap_or_else(|| failed.clone());
        self.queued
            .retain(|queued| queued.node_id != failed.node_id);
        latest
    }

    fn start_next(&mut self) -> Option<SynopsisCommit> {
        debug_assert!(self.in_flight.is_none());
        self.in_flight = self.queued.pop_front();
        self.in_flight.clone()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ModalFocus {
    Cancel,
    Confirm,
}

impl ModalFocus {
    fn next(self) -> Self {
        match self {
            Self::Cancel => Self::Confirm,
            Self::Confirm => Self::Cancel,
        }
    }

    fn id(self) -> iced::widget::Id {
        match self {
            Self::Cancel => crate::focus::modal_cancel_id(),
            Self::Confirm => crate::focus::modal_confirm_id(),
        }
    }
}

#[derive(Debug, Clone)]
struct NativeSpellcheckTicket {
    view: ViewId,
    editor_session: SharedEditorSession,
    document_id: parchmint_domain::DocumentId,
    revision: EditorRevision,
    generation: u64,
    request: SpellcheckRequest,
}

#[derive(Debug, Clone)]
struct NativeSpellingIssue {
    word: String,
    range: EditorSelection,
    suggestions: Vec<String>,
}

#[derive(Debug, Clone)]
struct NativeSpellingMenuContext {
    pane: EditorPane,
    view: ViewId,
    editor_session: SharedEditorSession,
    revision: EditorRevision,
    word: String,
    range: EditorSelection,
    comment_range: EditorSelection,
}

#[derive(Debug, Default)]
struct AutosaveState {
    first_dirty: Option<Instant>,
    last_edit: Option<Instant>,
    dirty_sessions: BTreeMap<SharedEditorSession, EditorRevision>,
    projected_sessions: BTreeMap<SharedEditorSession, EditorRevision>,
    save_in_flight: bool,
    close_after_save: bool,
    explicit_save_waiting: bool,
}

impl AutosaveState {
    const IDLE_DELAY: Duration = Duration::from_secs(60);
    const CONTINUOUS_LIMIT: Duration = Duration::from_secs(300);

    fn mark_dirty(&mut self, session: SharedEditorSession, revision: EditorRevision, now: Instant) {
        self.first_dirty.get_or_insert(now);
        self.last_edit = Some(now);
        self.dirty_sessions
            .entry(session)
            .and_modify(|current| *current = (*current).max(revision))
            .or_insert(revision);
    }

    fn should_save(&self, now: Instant) -> bool {
        !self.dirty_sessions.is_empty()
            && !self.save_in_flight
            && self.first_dirty.is_some_and(|first| {
                now.saturating_duration_since(first) >= Self::CONTINUOUS_LIMIT
                    || self
                        .last_edit
                        .is_some_and(|last| now.saturating_duration_since(last) >= Self::IDLE_DELAY)
            })
    }

    fn record_projected(
        &mut self,
        projected: impl IntoIterator<Item = (SharedEditorSession, EditorRevision)>,
    ) {
        for (session, revision) in projected {
            self.projected_sessions
                .entry(session)
                .and_modify(|current| *current = (*current).max(revision))
                .or_insert(revision);
        }
    }

    fn finish_save(&mut self, ticket: &AutosaveTicket) {
        self.save_in_flight = false;
        self.dirty_sessions.retain(|session, current| {
            ticket
                .dirty_sessions
                .get(session)
                .is_none_or(|saved| *current > *saved)
        });
        if self.dirty_sessions.is_empty() {
            self.first_dirty = None;
            self.last_edit = None;
        } else {
            self.first_dirty = Some(Instant::now());
        }
    }

    fn is_clean(&self) -> bool {
        !self.save_in_flight && self.dirty_sessions.is_empty()
    }

    fn requires_projection_save(&self, has_projection_plans: bool) -> bool {
        has_projection_plans || !self.is_clean()
    }

    fn reset_sessions(&mut self) {
        self.dirty_sessions.clear();
        self.projected_sessions.clear();
        self.first_dirty = None;
        self.last_edit = None;
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct EditorProjectionPlan {
    session: SharedEditorSession,
    revision: EditorRevision,
}

#[derive(Debug, Clone)]
struct AutosaveTicket {
    dirty_sessions: BTreeMap<SharedEditorSession, EditorRevision>,
}

#[derive(Debug, Clone)]
enum SavePurpose {
    Untracked,
    ProjectMutation(ProjectMutationTicket),
    OpaqueMutation(OpaqueMutationToken),
}

#[derive(Debug, Clone)]
struct AutosaveCompletion {
    saved_revision: u64,
    snapshot: ProjectSnapshot,
}

#[derive(Debug, Clone)]
struct CloseCompletion {
    snapshot: Option<ProjectSnapshot>,
    result: Result<(), String>,
}

#[derive(Debug, Clone)]
struct ProjectionRun<T> {
    projected: BTreeMap<SharedEditorSession, EditorRevision>,
    result: Result<T, String>,
}

fn deduplicated_editor_sessions(
    mounted: impl IntoIterator<Item = SharedEditorSession>,
    retained: impl IntoIterator<Item = SharedEditorSession>,
) -> Vec<SharedEditorSession> {
    mounted
        .into_iter()
        .chain(retained)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn editor_projection_plans(
    adapter: &EditorIcedAdapter,
    sessions: impl IntoIterator<Item = SharedEditorSession>,
    projected: &BTreeMap<SharedEditorSession, EditorRevision>,
) -> Result<Vec<EditorProjectionPlan>, String> {
    sessions
        .into_iter()
        .map(|session| {
            let revision = adapter
                .revision(session.clone())
                .map_err(|error| error.to_string())?;
            Ok((projected.get(&session).copied() < Some(revision))
                .then_some(EditorProjectionPlan { session, revision }))
        })
        .filter_map(Result::transpose)
        .collect()
}

fn run_projection_sequence<T>(
    plans: &[EditorProjectionPlan],
    mut project: impl FnMut(&EditorProjectionPlan) -> Result<(), String>,
    finish: impl FnOnce() -> Result<T, String>,
) -> ProjectionRun<T> {
    let mut projected = BTreeMap::new();
    for plan in plans {
        if let Err(error) = project(plan) {
            return ProjectionRun {
                projected,
                result: Err(error),
            };
        }
        projected.insert(plan.session.clone(), plan.revision);
    }
    ProjectionRun {
        projected,
        result: finish(),
    }
}

fn snapshot_covers(candidate: &ProjectSnapshot, current: &ProjectSnapshot) -> bool {
    candidate.project.revision >= current.project.revision
        && current.document_summaries.iter().all(|document| {
            candidate
                .document_summaries
                .iter()
                .find(|candidate| candidate.document_id == document.document_id)
                .is_some_and(|candidate| candidate.revision >= document.revision)
        })
}

fn project_effect_requires_durability(effect: &ProjectEffect) -> bool {
    matches!(
        effect,
        ProjectEffect::CreateHierarchy { .. }
            | ProjectEffect::DeleteHierarchy(_)
            | ProjectEffect::MoveHierarchy { .. }
            | ProjectEffect::PasteCopiedSubtrees { .. }
            | ProjectEffect::PasteCutSubtrees { .. }
            | ProjectEffect::CommitNodeTitle { .. }
            | ProjectEffect::CommitSynopsis { .. }
            | ProjectEffect::CommitMetadataValue { .. }
            | ProjectEffect::UpsertMetadataField(_)
            | ProjectEffect::ReorderMetadataField { .. }
            | ProjectEffect::DeleteMetadataField(_)
            | ProjectEffect::UpsertStyle(_)
            | ProjectEffect::DeleteStyle(_)
            | ProjectEffect::ApplyGlobalReplacement { .. }
            | ProjectEffect::CreateNamedSnapshot(_)
            | ProjectEffect::RestoreHistory { .. }
            | ProjectEffect::RestoreDeletedSubtree { .. }
            | ProjectEffect::SetProjectExportSettings(_)
    )
}

fn should_reconcile_refreshed_presentation(
    mutation: Option<&ProjectMutationTicket>,
    opaque_mutation: Option<OpaqueMutationToken>,
    has_later_mutations: bool,
) -> bool {
    (mutation.is_none() && opaque_mutation.is_none()) || !has_later_mutations
}

fn editor_effect_requires_durability(effect: &EditorEffect) -> bool {
    matches!(
        effect,
        EditorEffect::SpellingDictionaryAction {
            scope: crate::SpellingDictionaryScope::Project,
            ..
        }
    )
}

/// A spellcheck result is only useful once a word has a stable boundary.
/// Checking while the user is still extending the same word repeatedly walks
/// and decorates the entire visible block, which makes normal typing lag.
fn completes_spellcheck_word(message: &parchmint_editor_iced::MountedEditorMessage) -> bool {
    match message {
        parchmint_editor_iced::MountedEditorMessage::InsertText(text) => {
            text.chars().last().is_some_and(|character| {
                !character.is_alphanumeric() && character != '\'' && character != '’'
            })
        }
        parchmint_editor_iced::MountedEditorMessage::KeyCommand(
            parchmint_editor_iced::MountedEditorKeyCommand::SplitBlock
            | parchmint_editor_iced::MountedEditorKeyCommand::InsertSoftBreak,
        ) => true,
        _ => false,
    }
}

impl NativeDesktop {
    fn boot(startup: NativeDesktopStartup) -> (Self, Task<Message>) {
        let capture_error = startup
            .capture
            .as_ref()
            .and_then(|request| request.validates_output().err());
        let capture_valid = capture_error.is_none();
        let mut desktop = Self {
            appearance: startup.appearance,
            appearance_mode: startup.appearance_mode,
            launcher: LauncherState::default(),
            windows: BTreeMap::new(),
            project_windows: BTreeMap::new(),
            closing_windows: BTreeSet::new(),
            close_failures: BTreeMap::new(),
            opening_project: false,
            creating_project: false,
            status: capture_error.clone().or_else(|| {
                startup
                    .locked_project
                    .map(|path| format!("Project is already open: {}", path.display()))
            }),
            callbacks: startup.callbacks,
            last_appearance_generation: 0,
            capture: capture_valid
                .then(|| {
                    startup.capture.map(|request| NativeCaptureState {
                        request,
                        window: None,
                        window_opened: false,
                        settled_frames: 0,
                        screenshot_requested: false,
                    })
                })
                .flatten(),
        };
        for project in startup.recent_projects.into_iter().rev() {
            desktop.launcher.add_recent_project(
                project.name,
                project.path,
                format_last_opened(project.last_opened_unix_seconds),
            );
        }
        let mut tasks = vec![desktop.open_launcher_window()];
        tasks.extend(
            startup
                .projects
                .into_iter()
                .map(|project| desktop.open_project_window(project)),
        );
        (desktop, Task::batch(tasks))
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::WindowOpened(window) => {
                if let Some(capture) = self
                    .capture
                    .as_mut()
                    .filter(|capture| capture.window == Some(window))
                {
                    capture.window_opened = true;
                    // The subsequent subscription ticks are deliberately used
                    // instead of sleeping in this update call: the event loop
                    // remains free to create, lay out, and render the window.
                }
                Task::none()
            }
            Message::CaptureFrameTick => self.capture_after_settled_frame(),
            Message::CaptureScreenshot(screenshot) => self.encode_capture(screenshot),
            Message::CaptureEncoded(result) => self.finish_capture(result),
            Message::RuntimeEvent {
                window,
                event,
                accelerator_fallback,
            } => self.runtime_event(window, event, accelerator_fallback),
            Message::DismissContextMenus { window } => self.dismiss_context_menus(window),
            Message::CancelUncommittedHierarchyDrag { window } => {
                if let Some(NativeWindow::Project(state)) = self.windows.get_mut(&window)
                    && let Some(workspace) = state.workspace.as_mut()
                    && workspace.hierarchy_drag_source().is_some()
                {
                    workspace.update(ProjectMessage::CancelHierarchyDrag);
                }
                Task::none()
            }
            Message::WorkspaceLoaded { window, result } => {
                let Some(NativeWindow::Project(state)) = self.windows.get_mut(&window) else {
                    return Task::none();
                };
                let effects = match result {
                    Ok(Some(snapshot)) => {
                        state.shell.layout_mut().restore_panes(
                            snapshot.layout.explorer_width,
                            snapshot.layout.inspector_width,
                            !snapshot.layout.explorer_collapsed,
                            !snapshot.layout.inspector_collapsed,
                        );
                        if let Some(workspace) = state.workspace.as_mut() {
                            let destination = workspace.apply_workspace_snapshot(&snapshot);
                            state.shell.select_destination(destination);
                        }
                        match Self::restored_workspace_effects(state) {
                            Ok(effects) => effects,
                            Err(error) => {
                                self.status = Some(format!(
                                    "Restored editor views could not be remounted: {error}"
                                ));
                                Vec::new()
                            }
                        }
                    }
                    Ok(None) => Vec::new(),
                    Err(error) => {
                        self.status =
                            Some(format!("Workspace layout could not be restored: {error}"));
                        Vec::new()
                    }
                };
                Self::project_effect_tasks(window, state.effect_executor.clone(), effects)
            }
            Message::WorkspacePersisted { result } => {
                if let Err(error) = result {
                    self.status = Some(format!("Workspace layout could not be saved: {error}"));
                }
                Task::none()
            }
            Message::CloseRequested(id) => self.close_window(id),
            Message::ShowNewProject => {
                self.creating_project = true;
                Task::none()
            }
            Message::CancelNewProject => {
                self.creating_project = false;
                self.status = None;
                Task::none()
            }
            Message::NewProjectTitleChanged(title) => {
                self.launcher.new_project_mut().set_title(title);
                Task::none()
            }
            Message::NewProjectDestinationChanged(destination) => {
                self.launcher.new_project_mut().set_destination(destination);
                Task::none()
            }
            Message::NewProjectAuthorChanged(author) => {
                self.launcher.new_project_mut().set_author(Some(author));
                Task::none()
            }
            Message::ChooseOpenProject => self.choose_directory(false, LAUNCHER_CAPABILITY),
            Message::ChooseNewProjectDestination => {
                self.choose_directory(true, LAUNCHER_CAPABILITY)
            }
            Message::DirectoryChosen { create, result } => {
                self.finish_directory_choice(create, result)
            }
            Message::OpenRecentProject(project) => self.route_recent_project_open(project),
            Message::CreateProject => self.route_project_create(),
            Message::ProjectOpenFinished { project, result } => {
                self.opening_project = false;
                self.finish_project_open(project, result)
            }
            Message::ProjectSurface { window, message } => {
                self.update_project_surface(window, message)
            }
            Message::EditorProjectionPersisted {
                window,
                ticket,
                result,
            } => {
                let Some(NativeWindow::Project(state)) = self.windows.get_mut(&window) else {
                    return Task::none();
                };
                let run = match result {
                    Ok(run) => run,
                    Err(error) => ProjectionRun {
                        projected: BTreeMap::new(),
                        result: Err(error),
                    },
                };
                state.autosave.record_projected(run.projected);
                state.autosave.save_in_flight = false;
                let close_after_save = state.autosave.close_after_save;
                state.autosave.close_after_save = false;
                match run.result {
                    Ok(completion) => {
                        state.autosave.finish_save(&ticket);
                        Self::accept_persistence_snapshot(state, completion.snapshot);
                        Self::prune_durable_unmounted_sessions(state);
                        if let Some(workspace) = state.workspace.as_mut() {
                            workspace
                                .update(ProjectMessage::SaveCompleted(completion.saved_revision));
                        }
                        self.status = None;
                        if close_after_save
                            && !state.project_mutations.blocks_close()
                            && !state.opaque_mutations.blocks_close()
                        {
                            return self.continue_close_window(window);
                        } else if close_after_save {
                            state.autosave.close_after_save = true;
                        } else if state.autosave.explicit_save_waiting
                            && !state.project_mutations.blocks_close()
                            && !state.opaque_mutations.blocks_close()
                        {
                            return self.start_projection_save(window, ProjectSaveKind::Explicit);
                        }
                    }
                    Err(error) => {
                        self.status = Some(error.clone());
                        if let Some(workspace) = state.workspace.as_mut() {
                            workspace.update(ProjectMessage::SaveFailed(error.clone()));
                        }
                        if close_after_save {
                            self.status = None;
                            self.closing_windows.remove(&window);
                            self.close_failures.insert(state.project.window, error);
                        } else if state.autosave.explicit_save_waiting
                            && !state.project_mutations.blocks_close()
                            && !state.opaque_mutations.blocks_close()
                        {
                            return self.start_projection_save(window, ProjectSaveKind::Explicit);
                        }
                    }
                }
                Task::none()
            }
            Message::ClipboardWriteFinished {
                window,
                request,
                result,
            } => self.finish_clipboard_write(window, request, result),
            Message::ClipboardReadFinished {
                window,
                request,
                result,
            } => self.finish_clipboard_read(window, request, result),
            Message::AutosaveTick(now) => self.autosave_tick(now),
            Message::SystemAppearanceObserved(appearance) => {
                self.system_appearance_event(appearance)
            }
            #[cfg(not(target_os = "linux"))]
            Message::SystemThemeMode(mode) => match mode {
                iced::theme::Mode::Light => self.system_appearance_event(SystemAppearance::Light),
                iced::theme::Mode::Dark => self.system_appearance_event(SystemAppearance::Dark),
                iced::theme::Mode::None => Task::none(),
            },
            Message::SystemAppearanceChangedFinished { generation, result } => {
                if self.appearance_mode != AppearanceMode::System
                    || generation != self.last_appearance_generation
                {
                    return Task::none();
                }
                match result {
                    Ok(Some(appearance)) => {
                        self.apply_appearance(appearance);
                        self.status = None;
                    }
                    Ok(None) => {}
                    Err(error) => self.status = Some(error),
                }
                Task::none()
            }
            Message::SystemAppearanceStreamFailed(error) => {
                self.status = Some(error);
                Task::none()
            }
            Message::SaveFinished {
                window,
                purpose,
                result,
            } => {
                let mut terminal = None;
                let mut terminal_error = None;
                if let Some(NativeWindow::Project(state)) = self.windows.get_mut(&window)
                    && let Some(workspace) = state.workspace.as_mut()
                {
                    state.autosave.save_in_flight = false;
                    match result {
                        Ok(revision) => {
                            workspace.update(ProjectMessage::SaveCompleted(revision));
                            self.status = None;
                            terminal = match purpose {
                                SavePurpose::ProjectMutation(ticket) => {
                                    Some(PersistentMutationTerminal::ProjectSucceeded(ticket))
                                }
                                SavePurpose::OpaqueMutation(token) => {
                                    Some(PersistentMutationTerminal::OpaqueSucceeded(token))
                                }
                                SavePurpose::Untracked => None,
                            };
                        }
                        Err(error) => {
                            workspace.update(ProjectMessage::SaveFailed(error.clone()));
                            self.status = Some(error.clone());
                            terminal = match purpose {
                                SavePurpose::ProjectMutation(ticket) => {
                                    Some(PersistentMutationTerminal::ProjectSaveFailed(ticket))
                                }
                                SavePurpose::OpaqueMutation(token) => {
                                    Some(PersistentMutationTerminal::OpaqueSaveFailed(token))
                                }
                                SavePurpose::Untracked => None,
                            };
                            terminal_error = Some(error);
                        }
                    }
                }
                if let Some(terminal) = terminal {
                    self.after_persistent_mutation_terminal(window, terminal, terminal_error)
                } else if terminal_error.is_some() {
                    Task::none()
                } else if self
                    .windows
                    .get(&window)
                    .is_some_and(|native| match native {
                        NativeWindow::Project(state) => state.autosave.close_after_save,
                        NativeWindow::Launcher => false,
                    })
                {
                    self.continue_close_window(window)
                } else {
                    Task::none()
                }
            }
            Message::ProjectEffectFinished {
                window,
                mutation,
                result,
            } => self.finish_project_effect(window, mutation, None, result),
            Message::EditorEffectFinished {
                window,
                mutation,
                result,
            } => self.finish_editor_effect(window, mutation, result),
            Message::SpellcheckFinished {
                window,
                ticket,
                result,
            } => self.finish_spellcheck(window, ticket, result),
            Message::SearchFinished {
                window,
                ticket,
                result,
            } => {
                let Some(NativeWindow::Project(state)) = self.windows.get_mut(&window) else {
                    return Task::none();
                };
                let Some(workspace) = state.workspace.as_mut() else {
                    return Task::none();
                };
                match result {
                    Ok(batches) => {
                        for batch in batches {
                            let accepted = state
                                .service_feeds
                                .as_ref()
                                .is_some_and(|feeds| feeds.search().accept_batch(&batch).is_ok());
                            if accepted {
                                workspace.accept_completion(ProjectTaskCompletion::for_ticket(
                                    ticket.clone(),
                                    batch.reducer_payload(),
                                ));
                            }
                        }
                    }
                    Err(error) => {
                        workspace.accept_completion(ProjectTaskCompletion::for_ticket(
                            ticket,
                            ProjectTaskPayload::Failed(error.clone()),
                        ));
                        self.status = Some(error);
                    }
                }
                Task::none()
            }
            Message::HistoryFinished {
                window,
                ticket,
                append,
                result,
            } => {
                let Some(NativeWindow::Project(state)) = self.windows.get_mut(&window) else {
                    return Task::none();
                };
                let Some(workspace) = state.workspace.as_mut() else {
                    return Task::none();
                };
                let next_cursor = result
                    .as_ref()
                    .ok()
                    .and_then(|history| history.next_cursor.as_ref())
                    .map(|cursor| cursor.as_str().to_owned());
                let payload = result
                    .map(|mut history| {
                        if append {
                            let mut checkpoints = workspace.history().checkpoints().to_vec();
                            checkpoints.append(&mut history.checkpoints);
                            history.checkpoints = checkpoints;
                        }
                        history.reducer_payload()
                    })
                    .unwrap_or_else(ProjectTaskPayload::Failed);
                if workspace.accept_completion(ProjectTaskCompletion::for_ticket(ticket, payload)) {
                    workspace.finish_history_page(next_cursor);
                }
                Task::none()
            }
            Message::HistoryPreviewFinished {
                window,
                ticket,
                result,
            } => {
                let Some(NativeWindow::Project(state)) = self.windows.get_mut(&window) else {
                    return Task::none();
                };
                let Some(workspace) = state.workspace.as_mut() else {
                    return Task::none();
                };
                let payload = result
                    .map(|preview| preview.reducer_payload())
                    .unwrap_or_else(ProjectTaskPayload::Failed);
                workspace.accept_completion(ProjectTaskCompletion::for_ticket(ticket, payload));
                Task::none()
            }
            Message::HistoryMaintenanceFinished { window, result } => {
                let Some(NativeWindow::Project(state)) = self.windows.get_mut(&window) else {
                    return Task::none();
                };
                let Some(workspace) = state.workspace.as_mut() else {
                    return Task::none();
                };
                match result {
                    Ok(status) => {
                        workspace.update(ProjectMessage::HistoryMaintenanceLoaded(status));
                    }
                    Err(error) => workspace.fail_history_workflow(error),
                }
                Task::none()
            }
            Message::HistoryReinitialized {
                window,
                mutation,
                result,
            } => {
                let Some(NativeWindow::Project(state)) = self.windows.get_mut(&window) else {
                    return Task::none();
                };
                let Some(workspace) = state.workspace.as_mut() else {
                    return Task::none();
                };
                let terminal_error = result.as_ref().err().cloned();
                match result {
                    Ok(message) => {
                        workspace.update(ProjectMessage::HistoryReinitialized(message));
                    }
                    Err(error) => workspace.fail_history_workflow(error),
                }
                if let Some(token) = mutation {
                    let terminal = if terminal_error.is_some() {
                        PersistentMutationTerminal::OpaqueFailed(token)
                    } else {
                        PersistentMutationTerminal::OpaqueSucceeded(token)
                    };
                    return self.after_persistent_mutation_terminal(
                        window,
                        terminal,
                        terminal_error,
                    );
                }
                Task::none()
            }
            Message::ExportDestinationChosen { window, result } => {
                let Some(NativeWindow::Project(state)) = self.windows.get_mut(&window) else {
                    return Task::none();
                };
                let Some(workspace) = state.workspace.as_mut() else {
                    return Task::none();
                };
                match result {
                    Ok(selection) => {
                        let display = selection
                            .as_ref()
                            .map(|selection| selection.as_path().display().to_string());
                        state.export_destination = selection;
                        workspace.update(ProjectMessage::SetExportDestination(display));
                    }
                    Err(error) => {
                        workspace.update(ProjectMessage::ExportFailed(error));
                    }
                };
                Task::none()
            }
            Message::DeletedPreviewFinished {
                window,
                ticket,
                result,
            } => {
                let Some(NativeWindow::Project(state)) = self.windows.get_mut(&window) else {
                    return Task::none();
                };
                let Some(workspace) = state.workspace.as_mut() else {
                    return Task::none();
                };
                let payload = result
                    .map(|preview| preview.reducer_payload())
                    .unwrap_or_else(ProjectTaskPayload::Failed);
                workspace.accept_completion(ProjectTaskCompletion::for_ticket(ticket, payload));
                Task::none()
            }
            Message::ReplacementPreviewFinished {
                window,
                ticket,
                result,
            } => {
                let Some(NativeWindow::Project(state)) = self.windows.get_mut(&window) else {
                    return Task::none();
                };
                let Some(workspace) = state.workspace.as_mut() else {
                    return Task::none();
                };
                let payload = result
                    .map(|()| ProjectTaskPayload::ReplacementPreviewReady)
                    .unwrap_or_else(ProjectTaskPayload::Failed);
                workspace.accept_completion(ProjectTaskCompletion::for_ticket(ticket, payload));
                Task::none()
            }
            Message::ReplacementApplyFinished {
                window,
                ticket,
                mutation,
                result,
            } => {
                let Some(NativeWindow::Project(state)) = self.windows.get_mut(&window) else {
                    return Task::none();
                };
                match *result {
                    Ok(snapshot) => {
                        let revision = snapshot.project.revision.value();
                        let snapshot = Arc::new(snapshot);
                        if let Some(project_ui) = state.project.project_ui.as_mut() {
                            project_ui.snapshot = Arc::clone(&snapshot);
                            state.effect_executor = Some(NativeProjectEffectExecutor::new(
                                project_ui.ports.clone(),
                                Arc::clone(&snapshot),
                            ));
                        }
                        if let Some(workspace) = state.workspace.as_mut() {
                            workspace.accept_completion(ProjectTaskCompletion::for_ticket(
                                ticket,
                                ProjectTaskPayload::ReplacementApplied { revision },
                            ));
                        }
                        let Some(ports) = state.project.ports().cloned() else {
                            return self.after_persistent_mutation_terminal(
                                window,
                                PersistentMutationTerminal::OpaqueFailed(mutation),
                                Some(
                                    "Project replacement completed without a persistence port."
                                        .to_owned(),
                                ),
                            );
                        };
                        state.autosave.save_in_flight = true;
                        if let Some(workspace) = state.workspace.as_mut() {
                            workspace.update(ProjectMessage::StartSave(revision));
                        }
                        Self::save_task(
                            window,
                            ports,
                            ProjectSaveKind::Structural,
                            SavePurpose::OpaqueMutation(mutation),
                        )
                    }
                    Err(error) => {
                        let accepted = state.workspace.as_mut().is_some_and(|workspace| {
                            workspace.accept_completion(ProjectTaskCompletion::for_ticket(
                                ticket,
                                ProjectTaskPayload::Failed(error.clone()),
                            ))
                        });
                        if accepted {
                            self.status = Some(error.clone());
                        }
                        self.after_persistent_mutation_terminal(
                            window,
                            PersistentMutationTerminal::OpaqueFailed(mutation),
                            Some(error),
                        )
                    }
                }
            }
            Message::ExportOperationStarted {
                window,
                ticket,
                operation,
            } => {
                let Some(NativeWindow::Project(state)) = self.windows.get_mut(&window) else {
                    return Task::none();
                };
                let accepted = state.workspace.as_mut().is_some_and(|workspace| {
                    workspace.accept_completion(ProjectTaskCompletion::for_ticket(
                        ticket,
                        ProjectTaskPayload::ExportPlanning,
                    ))
                });
                if accepted {
                    state.active_export = Some(operation);
                }
                Task::none()
            }
            Message::ExportProgressed {
                window,
                ticket,
                operation,
                progress,
            } => {
                let Some(NativeWindow::Project(state)) = self.windows.get_mut(&window) else {
                    return Task::none();
                };
                if state.active_export != Some(operation) {
                    return Task::none();
                }
                let payload = match progress {
                    ExportProgress::Planning => ProjectTaskPayload::ExportPlanning,
                    ExportProgress::Rendering { completed, total } => {
                        ProjectTaskPayload::ExportProgress { completed, total }
                    }
                    ExportProgress::Committing => ProjectTaskPayload::ExportCommitting,
                };
                if let Some(workspace) = state.workspace.as_mut() {
                    workspace.accept_completion(ProjectTaskCompletion::for_ticket(ticket, payload));
                }
                Task::none()
            }
            Message::ExportFinished {
                window,
                ticket,
                operation,
                result,
            } => {
                let Some(NativeWindow::Project(state)) = self.windows.get_mut(&window) else {
                    return Task::none();
                };
                if operation.is_some() && state.active_export != operation {
                    return Task::none();
                }
                let Some(workspace) = state.workspace.as_mut() else {
                    return Task::none();
                };
                let payload = match result {
                    Ok(ExportOutcome::Completed(artifact)) => {
                        ProjectTaskPayload::ExportSucceeded { artifact }
                    }
                    Ok(ExportOutcome::Cancelled) => ProjectTaskPayload::ExportCancelled,
                    Err(error) => ProjectTaskPayload::Failed(error),
                };
                workspace.accept_completion(ProjectTaskCompletion::for_ticket(ticket, payload));
                if state.active_export == operation {
                    state.active_export = None;
                }
                Task::none()
            }
            Message::ExportCancelFinished {
                window,
                operation,
                result,
            } => {
                let Some(NativeWindow::Project(state)) = self.windows.get_mut(&window) else {
                    return Task::none();
                };
                if state.active_export == Some(operation)
                    && let Err(error) = result
                {
                    self.status = Some(error);
                }
                Task::none()
            }
            Message::ExportArtifactActionFinished(result) => {
                match result {
                    Ok(()) => self.status = None,
                    Err(error) => self.status = Some(error),
                }
                Task::none()
            }
            Message::RecoveryReconciled {
                window,
                session,
                ticket,
                result,
            } => {
                let Some(NativeWindow::Project(state)) = self.windows.get_mut(&window) else {
                    return Task::none();
                };
                if state.project.session != session {
                    return Task::none();
                }
                match result {
                    Ok(recovery) => {
                        let acceptance = recovery.acceptance;
                        let payload = if recovery.acceptance.is_some() {
                            ProjectTaskPayload::RecoveryAvailable {
                                accepted_records: recovery.accepted_records,
                                affected_documents: recovery
                                    .affected_documents
                                    .into_iter()
                                    .map(|summary| (summary.document_id, summary.revision))
                                    .collect(),
                                isolation: recovery.isolation,
                            }
                        } else if let Some(isolation) = recovery.isolation {
                            ProjectTaskPayload::Failed(format!(
                                "Recovery records were isolated and cannot be applied: {isolation}"
                            ))
                        } else {
                            ProjectTaskPayload::RecoveryUnavailable
                        };
                        let resolved = matches!(payload, ProjectTaskPayload::RecoveryUnavailable);
                        let accepted = state.workspace.as_mut().is_some_and(|workspace| {
                            workspace.accept_completion(ProjectTaskCompletion::for_ticket(
                                ticket, payload,
                            ))
                        });
                        if accepted {
                            state.recovery_acceptance = acceptance;
                        }
                        if resolved && accepted {
                            self.status = None;
                            return self.activate_reconciled_project(window, None);
                        }
                        Task::none()
                    }
                    Err(error) => {
                        if let Some(workspace) = state.workspace.as_mut() {
                            workspace.accept_completion(ProjectTaskCompletion::for_ticket(
                                ticket,
                                ProjectTaskPayload::Failed(error.clone()),
                            ));
                        }
                        self.status = Some(error);
                        Task::none()
                    }
                }
            }
            Message::RecoveryAccepted {
                window,
                session,
                ticket,
                mutation,
                result,
            } => {
                let mut recovered_document = None;
                let mut activate = false;
                let mut failed = None;
                {
                    let Some(NativeWindow::Project(state)) = self.windows.get_mut(&window) else {
                        return Task::none();
                    };
                    if state.project.session != session {
                        return Task::none();
                    }
                    let Some(workspace) = state.workspace.as_mut() else {
                        return Task::none();
                    };
                    match result {
                        Ok(accepted) => {
                            let recovered_revision = accepted.project_revision;
                            let snapshot = Arc::new(accepted.snapshot);
                            let fully_resolved = accepted.isolation.is_none();
                            let payload = accepted.isolation.as_ref().map_or_else(
                                || ProjectTaskPayload::RecoveryAccepted {
                                    revision: recovered_revision,
                                },
                                |isolation| {
                                    ProjectTaskPayload::Failed(format!(
                                        "Recovered edits were saved, but isolated records remain: {isolation}"
                                    ))
                                },
                            );
                            let completion_accepted = workspace.accept_completion(
                                ProjectTaskCompletion::for_ticket(ticket, payload),
                            );
                            if completion_accepted {
                                recovered_document = accepted.recovered_document;
                                workspace.reconcile_snapshot(&snapshot);
                                if let Some(project_ui) = state.project.project_ui.as_mut() {
                                    project_ui.snapshot = Arc::clone(&snapshot);
                                    state.effect_executor = Some(NativeProjectEffectExecutor::new(
                                        project_ui.ports.clone(),
                                        Arc::clone(&snapshot),
                                    ));
                                }
                            }
                            activate = completion_accepted && fully_resolved;
                        }
                        Err(error) => {
                            failed = Some(error.clone());
                            let completion_accepted =
                                workspace.accept_completion(ProjectTaskCompletion::for_ticket(
                                    ticket,
                                    ProjectTaskPayload::Failed(error.clone()),
                                ));
                            if completion_accepted {
                                self.status = Some(error);
                            }
                        }
                    }
                }
                let continuation = if let Some(token) = mutation {
                    let terminal = if failed.is_some() {
                        PersistentMutationTerminal::OpaqueFailed(token)
                    } else {
                        PersistentMutationTerminal::OpaqueSucceeded(token)
                    };
                    self.after_persistent_mutation_terminal(window, terminal, failed)
                } else {
                    Task::none()
                };
                if activate {
                    self.status = None;
                    Task::batch([
                        continuation,
                        self.activate_reconciled_project(window, recovered_document),
                    ])
                } else {
                    continuation
                }
            }
            Message::RecoveryDiscarded {
                window,
                session,
                ticket,
                mutation,
                result,
            } => {
                let mut activate = false;
                let mut failed = None;
                {
                    let Some(NativeWindow::Project(state)) = self.windows.get_mut(&window) else {
                        return Task::none();
                    };
                    if state.project.session != session {
                        return Task::none();
                    }
                    let Some(workspace) = state.workspace.as_mut() else {
                        return Task::none();
                    };
                    match result {
                        Ok(discarded) => {
                            let snapshot = Arc::new(discarded.snapshot);
                            let fully_resolved = discarded.isolation.is_none();
                            let payload = discarded.isolation.as_ref().map_or(
                                ProjectTaskPayload::RecoveryDiscarded {
                                    revision: discarded.project_revision,
                                },
                                |isolation| {
                                    ProjectTaskPayload::Failed(format!(
                                        "Current state was kept, but isolated records remain: {isolation}"
                                    ))
                                },
                            );
                            let completion_accepted = workspace.accept_completion(
                                ProjectTaskCompletion::for_ticket(ticket, payload),
                            );
                            if completion_accepted {
                                workspace.reconcile_snapshot(&snapshot);
                                if let Some(project_ui) = state.project.project_ui.as_mut() {
                                    project_ui.snapshot = Arc::clone(&snapshot);
                                    state.effect_executor = Some(NativeProjectEffectExecutor::new(
                                        project_ui.ports.clone(),
                                        Arc::clone(&snapshot),
                                    ));
                                }
                            }
                            activate = completion_accepted && fully_resolved;
                        }
                        Err(error) => {
                            failed = Some(error.clone());
                            let completion_accepted =
                                workspace.accept_completion(ProjectTaskCompletion::for_ticket(
                                    ticket,
                                    ProjectTaskPayload::Failed(error.clone()),
                                ));
                            if completion_accepted {
                                self.status = Some(error);
                            }
                        }
                    }
                }
                let continuation = if let Some(token) = mutation {
                    let terminal = if failed.is_some() {
                        PersistentMutationTerminal::OpaqueFailed(token)
                    } else {
                        PersistentMutationTerminal::OpaqueSucceeded(token)
                    };
                    self.after_persistent_mutation_terminal(window, terminal, failed)
                } else {
                    Task::none()
                };
                if activate {
                    self.status = None;
                    Task::batch([continuation, self.activate_reconciled_project(window, None)])
                } else {
                    continuation
                }
            }
            Message::SelectDestination {
                window,
                destination,
            } => {
                if let Some(NativeWindow::Project(state)) = self.windows.get_mut(&window) {
                    state.shell.select_destination(destination);
                }
                Task::none()
            }
            Message::AppearanceSelected(mode) => {
                let callbacks = Arc::clone(&self.callbacks);
                Task::perform(
                    Self::run_blocking_operation("set appearance", move || {
                        callbacks.set_appearance(mode)
                    }),
                    move |result| Message::AppearanceFinished { mode, result },
                )
            }
            Message::AppearanceFinished { mode, result } => {
                match result {
                    Ok(appearance) => {
                        self.appearance_mode = mode;
                        self.last_appearance_generation =
                            self.last_appearance_generation.saturating_add(1);
                        self.apply_appearance(appearance);
                        self.status = None;
                    }
                    Err(error) => self.status = Some(error),
                }
                Task::none()
            }
            Message::RetryClose(window) => self.close_window(window),
            Message::CancelClose(window) => {
                self.closing_windows.remove(&window);
                self.status = None;
                if let Some(NativeWindow::Project(state)) = self.windows.get_mut(&window) {
                    state.autosave.close_after_save = false;
                    self.close_failures.remove(&state.project.window);
                }
                Task::none()
            }
            Message::ProjectCloseFinished { window, result } => {
                let run = match result {
                    Ok(run) => run,
                    Err(error) => ProjectionRun {
                        projected: BTreeMap::new(),
                        result: Err(error),
                    },
                };
                if let Some(NativeWindow::Project(state)) = self.windows.get_mut(&window) {
                    state.autosave.record_projected(run.projected);
                }
                match run.result {
                    Ok(completion) => {
                        if let Some(snapshot) = completion.snapshot
                            && let Some(NativeWindow::Project(state)) =
                                self.windows.get_mut(&window)
                        {
                            Self::accept_persistence_snapshot(state, snapshot);
                        }
                        match completion.result {
                            Ok(()) => {
                                self.status = None;
                                self.finish_close(window)
                            }
                            Err(error) => {
                                self.status = None;
                                self.closing_windows.remove(&window);
                                if let Some(NativeWindow::Project(state)) =
                                    self.windows.get(&window)
                                {
                                    self.close_failures.insert(state.project.window, error);
                                }
                                Task::none()
                            }
                        }
                    }
                    Err(error) => {
                        self.status = None;
                        self.closing_windows.remove(&window);
                        if let Some(NativeWindow::Project(state)) = self.windows.get(&window) {
                            self.close_failures.insert(state.project.window, error);
                        }
                        Task::none()
                    }
                }
            }
        }
    }

    fn view(&self, id: window::Id) -> Element<'_, Message> {
        let content = match self.windows.get(&id) {
            Some(NativeWindow::Launcher) => self.launcher_view(),
            Some(NativeWindow::Project(state)) => state.workspace.as_deref().map_or_else(
                || {
                    legacy_project_surface(
                        id,
                        state.project.project.display().to_string(),
                        state.shell.destination(),
                        self.close_failures.get(&state.project.window).cloned(),
                        self.status.clone(),
                    )
                },
                |workspace| {
                    Self::typed_project_view(
                        id,
                        workspace,
                        &state.editor_hosts,
                        state.spelling_menu.as_ref(),
                        state.shell.destination(),
                        state.shell.layout(),
                        [
                            state
                                .shell
                                .inspector_section_is_expanded(crate::InspectorSection::Synopsis),
                            state
                                .shell
                                .inspector_section_is_expanded(crate::InspectorSection::Metadata),
                            state
                                .shell
                                .inspector_section_is_expanded(crate::InspectorSection::Comments),
                        ],
                        self.appearance,
                        self.close_failures
                            .get(&state.project.window)
                            .map(String::as_str),
                        self.status.as_deref(),
                    )
                },
            ),
            None => container(text("Opening ParchMint…"))
                .center(Length::Fill)
                .into(),
        };
        content
    }

    fn title(&self, id: window::Id) -> String {
        match self.windows.get(&id) {
            Some(NativeWindow::Launcher) | None => "ParchMint".to_owned(),
            Some(NativeWindow::Project(state)) => state
                .project
                .project
                .file_name()
                .and_then(|name| name.to_str())
                .map_or_else(
                    || "ParchMint".to_owned(),
                    |name| format!("{name} — ParchMint"),
                ),
        }
    }

    fn theme(&self, _id: window::Id) -> Theme {
        ParchMintTheme::new(self.appearance).iced_theme()
    }

    fn subscription(&self) -> Subscription<Message> {
        let mut subscriptions = vec![
            window::close_requests().map(Message::CloseRequested),
            iced::time::every(Duration::from_millis(250)).map(Message::AutosaveTick),
            event::listen_with(runtime_event),
        ];
        // A global cursor subscription rebuilds the complete editor surface for
        // every pointer move. Subscribe only while a splitter drag needs it.
        if self.windows.values().any(
            |window| matches!(window, NativeWindow::Project(state) if state.resizing.is_some()),
        ) {
            subscriptions.push(event::listen_with(resize_event));
        }
        if self.capture.is_some() {
            subscriptions.push(
                iced::time::every(Duration::from_millis(16)).map(|_| Message::CaptureFrameTick),
            );
        }
        if self.appearance_mode == AppearanceMode::System {
            #[cfg(target_os = "linux")]
            subscriptions.push(streamed_appearance_subscription(
                self.appearance_mode,
                linux_portal_appearance_events,
            ));
            #[cfg(not(target_os = "linux"))]
            subscriptions.push(iced::system::theme_changes().map(Message::SystemThemeMode));
        }
        Subscription::batch(subscriptions)
    }

    fn capture_after_settled_frame(&mut self) -> Task<Message> {
        let Some(capture) = self.capture.as_mut() else {
            return Task::none();
        };
        let Some(window) = capture.window else {
            return Task::none();
        };
        if !capture.window_opened || capture.screenshot_requested {
            return Task::none();
        }
        capture.settled_frames = capture.settled_frames.saturating_add(1);
        if capture.settled_frames < capture.request.settled_frames {
            return Task::none();
        }
        capture.screenshot_requested = true;
        window::screenshot(window).map(Message::CaptureScreenshot)
    }

    fn encode_capture(&mut self, screenshot: window::Screenshot) -> Task<Message> {
        let Some(capture) = self.capture.as_ref() else {
            return Task::none();
        };
        let output_path = capture.request.output_path.clone();
        let actual = (screenshot.size.width, screenshot.size.height);
        let bytes = screenshot.rgba.to_vec();
        Task::perform(
            Self::run_blocking_operation("encode native capture", move || {
                encode_capture_png(&output_path, actual, bytes)?;
                Ok(NativeCapturePng {
                    output_path,
                    physical_size: actual,
                })
            }),
            Message::CaptureEncoded,
        )
    }

    fn finish_capture(&mut self, result: Result<NativeCapturePng, String>) -> Task<Message> {
        let Some(capture) = self.capture.take() else {
            return Task::none();
        };
        match result {
            Ok(png) => {
                let requested = capture.request.requested_physical_size();
                let strict_error = strict_size_error(&capture.request, &png);
                println!(
                    "native capture written: {} ({}x{} RGBA; requested {}x{} at {}x)",
                    png.output_path.display(),
                    png.physical_size.0,
                    png.physical_size.1,
                    requested.0,
                    requested.1,
                    capture.request.scale(),
                );
                *capture
                    .request
                    .completion
                    .lock()
                    .expect("native capture completion mutex poisoned") =
                    Some(strict_error.map_or_else(|| Ok(png), Err));
                if capture.request.exit_after_capture {
                    iced::exit()
                } else {
                    Task::none()
                }
            }
            Err(error) => {
                eprintln!("native capture failed: {error}");
                *capture
                    .request
                    .completion
                    .lock()
                    .expect("native capture completion mutex poisoned") = Some(Err(error.clone()));
                self.status = Some(format!("Native capture failed: {error}"));
                iced::exit()
            }
        }
    }

    fn capability_for_window(&self, id: window::Id) -> Option<WindowCapability> {
        match self.windows.get(&id) {
            Some(NativeWindow::Launcher) => Some(LAUNCHER_CAPABILITY),
            Some(NativeWindow::Project(state)) => Some(state.project.window),
            None => None,
        }
    }

    fn activate_shortcut(&mut self, id: window::Id, command: &str) -> Task<Message> {
        let capability = self.capability_for_window(id);
        let task = match command {
            "file.new" => {
                self.creating_project = true;
                self.windows
                    .iter()
                    .find_map(|(window, native)| {
                        matches!(native, NativeWindow::Launcher).then_some(*window)
                    })
                    .map_or_else(Task::none, window::gain_focus)
            }
            "file.open" => capability.map_or_else(Task::none, |capability| {
                self.choose_directory(false, capability)
            }),
            "file.save" => self.update_project_surface(
                id,
                ProjectSurfaceMessage::EditorCenter(EditorCenterMessage::Workspace(
                    crate::EditorMessage::Save,
                )),
            ),
            "file.close" => self.close_window(id),
            "edit.copy" | "edit.cut" | "edit.paste" => {
                let intent = match command {
                    "edit.copy" => MountedEditorClipboardIntent::Copy,
                    "edit.cut" => MountedEditorClipboardIntent::Cut,
                    _ => MountedEditorClipboardIntent::Paste,
                };
                let Some(NativeWindow::Project(state)) = self.windows.get_mut(&id) else {
                    return Task::none();
                };
                let Some(workspace) = state.workspace.as_ref() else {
                    return Task::none();
                };
                let pane = workspace.editor().focused_pane();
                let Some(view) = state
                    .editor_bindings
                    .get(&pane)
                    .map(MountedEditorBinding::view)
                else {
                    return Task::none();
                };
                Self::clipboard_task(id, state, pane, view, intent).unwrap_or_else(|error| {
                    self.status = Some(error);
                    Task::none()
                })
            }
            "edit.undo" | "edit.redo" => self.update_project_surface(
                id,
                ProjectSurfaceMessage::EditorCenter(EditorCenterMessage::Workspace(
                    if command == "edit.undo" {
                        crate::EditorMessage::Undo
                    } else {
                        crate::EditorMessage::Redo
                    },
                )),
            ),
            _ => {
                self.status = Some(format!("Unknown keyboard shortcut command: {command}"));
                Task::none()
            }
        };
        task
    }

    fn runtime_event(
        &mut self,
        id: window::Id,
        event: Event,
        accelerator_fallback: bool,
    ) -> Task<Message> {
        if matches!(
            event,
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left))
        ) && self.windows.get(&id).is_some_and(|window| {
            matches!(window, NativeWindow::Project(state) if state
                    .workspace
                    .as_ref()
                    .is_some_and(|workspace| workspace.hierarchy_context_menu().is_some()))
        }) {
            // The Explorer menu is composed over the entire project surface.
            // Its backdrop receives outside left clicks, while its buttons
            // receive action clicks. Scheduling a second window-wide dismissal
            // here races those buttons and can discard their action.
            return Task::none();
        }
        if matches!(event, Event::Mouse(mouse::Event::ButtonPressed(_))) {
            // Let the widget tree consume this press first: a context-menu
            // action or a new secondary-click must not be preempted by the
            // window-wide dismissal. The follow-up message runs afterwards.
            return Task::perform(async {}, move |_| Message::DismissContextMenus {
                window: id,
            });
        }
        if matches!(
            event,
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left))
        ) {
            return Task::perform(async {}, move |_| Message::CancelUncommittedHierarchyDrag {
                window: id,
            });
        }
        if let Event::Keyboard(keyboard::Event::KeyPressed {
            key,
            modifiers,
            repeat: false,
            ..
        }) = &event
        {
            let cancel_pointer_interaction =
                matches!(key, keyboard::Key::Named(keyboard::key::Named::Escape))
                    && self.windows.get(&id).is_some_and(|window| {
                        let NativeWindow::Project(state) = window else {
                            return false;
                        };
                        state.workspace.as_ref().is_some_and(|workspace| {
                            workspace.hierarchy_drag_source().is_some()
                                || workspace.hierarchy_context_menu().is_some()
                                || [EditorPane::Primary, EditorPane::Companion]
                                    .into_iter()
                                    .any(|pane| workspace.editor().tab_drag_source(pane).is_some())
                        })
                    });
            if cancel_pointer_interaction {
                if let Some(NativeWindow::Project(state)) = self.windows.get_mut(&id)
                    && let Some(workspace) = state.workspace.as_mut()
                {
                    workspace.update(ProjectMessage::CancelHierarchyDrag);
                    workspace.update(ProjectMessage::CloseHierarchyContextMenu);
                    workspace
                        .editor_mut()
                        .update(crate::EditorMessage::CancelTabDrag);
                }
                return Task::none();
            }
            let enter_node = matches!(key, keyboard::Key::Named(keyboard::key::Named::Enter))
                .then(|| {
                    let NativeWindow::Project(state) = self.windows.get(&id)? else {
                        return None;
                    };
                    if state.shell.focus_target() != crate::FocusTarget::Explorer {
                        return None;
                    }
                    let workspace = state.workspace.as_ref()?;
                    workspace
                        .explorer()
                        .selected_ids()
                        .first()
                        .filter(|node| {
                            workspace
                                .explorer()
                                .row(node)
                                .is_some_and(|row| row.kind == crate::HierarchyRowKind::Document)
                        })
                        .map(|node| (*node).to_owned())
                })
                .flatten();
            if let Some(node_id) = enter_node {
                return self.update_project_surface(
                    id,
                    ProjectSurfaceMessage::Project(ProjectMessage::OpenHierarchyNode(node_id)),
                );
            }
            let local_find_open = self.windows.get(&id).is_some_and(|window| {
                let NativeWindow::Project(state) = window else {
                    return false;
                };
                state.workspace.as_ref().is_some_and(|workspace| {
                    let view = workspace
                        .editor()
                        .pane(workspace.editor().focused_pane())
                        .view();
                    workspace.editor().local_search(view).is_open()
                })
            });
            let find_message = match key {
                keyboard::Key::Character(key)
                    if key.eq_ignore_ascii_case("f") && modifiers.command() =>
                {
                    Some(crate::EditorMessage::OpenLocalFind)
                }
                keyboard::Key::Named(keyboard::key::Named::Enter) if local_find_open => {
                    Some(crate::EditorMessage::NavigateFind(if modifiers.shift() {
                        crate::FindDirection::Previous
                    } else {
                        crate::FindDirection::Next
                    }))
                }
                keyboard::Key::Named(keyboard::key::Named::Escape) if local_find_open => {
                    Some(crate::EditorMessage::CloseLocalFind)
                }
                _ => None,
            };
            if let Some(message) = find_message {
                return self.update_project_surface(
                    id,
                    ProjectSurfaceMessage::EditorCenter(EditorCenterMessage::Workspace(message)),
                );
            }
        }
        if let Event::Keyboard(keyboard::Event::KeyPressed {
            key: keyboard::Key::Character(key),
            modifiers,
            repeat: false,
            ..
        }) = &event
            && let Some(command) = keyboard_accelerator(key, *modifiers)
            && should_activate_shortcut(command, accelerator_fallback)
        {
            return self.activate_shortcut(id, command);
        }
        let Some(NativeWindow::Project(state)) = self.windows.get_mut(&id) else {
            return Task::none();
        };
        match event {
            Event::Keyboard(keyboard::Event::ModifiersChanged(modifiers)) => {
                state.modifiers = modifiers;
            }
            Event::Keyboard(keyboard::Event::KeyPressed {
                key: keyboard::Key::Named(keyboard::key::Named::F6 | keyboard::key::Named::Tab),
                repeat: false,
                ..
            }) if state.shell.focus_is_trapped() => {
                state.modal_focus = state.modal_focus.next();
                return iced::widget::operation::focus(state.modal_focus.id());
            }
            Event::Keyboard(keyboard::Event::KeyPressed {
                key: keyboard::Key::Named(keyboard::key::Named::F6),
                repeat: false,
                ..
            }) if !state.shell.focus_is_trapped() => {
                state.shell.focus_next_region();
                if matches!(
                    state.shell.focus_target(),
                    crate::FocusTarget::EditorDocument(_)
                ) && let Some(workspace) = state.workspace.as_ref()
                    && let Some(binding) = state
                        .editor_bindings
                        .get(&workspace.editor().focused_pane())
                {
                    let _ = binding.restore_focus();
                }
                return crate::focus::region_id(state.shell.focus_region())
                    .map_or_else(Task::none, iced::widget::operation::focus);
            }
            Event::Keyboard(keyboard::Event::KeyPressed {
                key: keyboard::Key::Named(keyboard::key::Named::Escape),
                repeat: false,
                ..
            }) => {
                if state
                    .workspace
                    .as_ref()
                    .is_some_and(|workspace| workspace.modal().is_some())
                {
                    if let Some(workspace) = state.workspace.as_mut() {
                        workspace.update(ProjectMessage::DismissModal);
                    }
                    state.shell.dismiss_dialog();
                } else if let Some(workspace) = state.workspace.as_mut() {
                    workspace.update(ProjectMessage::CancelCut);
                }
            }
            Event::Mouse(mouse::Event::CursorMoved { position }) => match state.resizing {
                Some(SidebarPanel::Explorer) => {
                    state
                        .shell
                        .layout_mut()
                        .resize_explorer(position.x.max(0.0) as u32);
                }
                Some(SidebarPanel::Inspector) => {
                    let width = state
                        .shell
                        .layout
                        .requested_width()
                        .saturating_sub(position.x.max(0.0) as u32);
                    state.shell.layout_mut().resize_inspector(width);
                }
                Some(SidebarPanel::Editor) => {
                    let center = state.shell.layout().center();
                    if center.width() > 0 {
                        let ratio = (position.x - center.x() as f32) / center.width() as f32;
                        if let Some(workspace) = state.workspace.as_mut() {
                            workspace.editor_mut().set_split_ratio(f64::from(ratio));
                        }
                    }
                }
                None => {}
            },
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                if state.resizing.take().is_some() {
                    return Self::workspace_persist_task(id, state);
                }
            }
            Event::Window(window::Event::Resized(size)) => {
                state
                    .shell
                    .layout_mut()
                    .resize_window(size.width as u32, size.height as u32);
            }
            _ => {}
        }
        Task::none()
    }

    fn dismiss_context_menus(&mut self, window: window::Id) -> Task<Message> {
        let Some(NativeWindow::Project(state)) = self.windows.get_mut(&window) else {
            return Task::none();
        };
        // Do not compare timestamps here: Iced may dispatch the native event
        // subscription before or after the widget message for one click. A
        // one-shot flag has no ordering dependency and therefore preserves a
        // newly opened Explorer or editor menu until the next button press.
        if state.suppress_next_context_menu_dismissal {
            state.suppress_next_context_menu_dismissal = false;
            return Task::none();
        }
        state.pending_spelling_menu = None;
        state.spelling_menu = None;
        if let Some(workspace) = state.workspace.as_mut() {
            workspace.update(ProjectMessage::CloseHierarchyContextMenu);
        }
        Task::none()
    }

    fn system_appearance_event(&mut self, appearance: SystemAppearance) -> Task<Message> {
        if self.appearance_mode != AppearanceMode::System {
            return Task::none();
        }
        self.last_appearance_generation = self.last_appearance_generation.saturating_add(1);
        let generation = self.last_appearance_generation;
        let appearance = resolved_system_appearance(appearance);
        let callbacks = Arc::clone(&self.callbacks);
        Task::perform(
            Self::run_blocking_operation("apply system appearance", move || {
                callbacks.system_appearance_changed(appearance)
            }),
            move |result| Message::SystemAppearanceChangedFinished { generation, result },
        )
    }

    fn apply_appearance(&mut self, appearance: ResolvedAppearance) {
        self.appearance = appearance;
        let theme = match appearance {
            ResolvedAppearance::Light => EditorSurfaceTheme::light(),
            ResolvedAppearance::Dark => EditorSurfaceTheme::dark(),
        };
        for native in self.windows.values_mut() {
            if let NativeWindow::Project(state) = native {
                for binding in state.editor_bindings.values_mut() {
                    binding.set_theme(theme);
                }
            }
        }
    }

    /// Builds the launcher surface with owned display values. Keeping this
    /// static makes it usable by the headless visual-verification boundary.
    fn launcher_view(&self) -> Element<'static, Message> {
        launcher_surface(
            self.launcher.recent_projects(),
            self.launcher.new_project(),
            self.creating_project,
            self.opening_project,
            self.status.clone(),
        )
    }

    #[cfg(feature = "visual-verification")]
    pub(crate) fn verification_launcher_element() -> Element<'static, ()> {
        let mut launcher = LauncherState::default();
        launcher.add_recent_project("Northbound", "/Projects/Northbound", "4 days ago · 9:07 AM");
        launcher.add_recent_project(
            "The Glass Harbor",
            "/Projects/Glass-Harbor",
            "yesterday · 4:18 PM",
        );
        launcher_surface(
            launcher.recent_projects(),
            launcher.new_project(),
            false,
            false,
            None,
        )
        .map(|_| ())
    }

    #[allow(
        clippy::too_many_arguments,
        reason = "the native window owns the independently scoped view inputs"
    )]
    fn typed_project_view<'a>(
        id: window::Id,
        workspace: &'a ProjectWorkspace,
        editor_hosts: &'a EditorHostSlots,
        spelling_menu: Option<&'a SpellingMenu>,
        destination: RibbonDestination,
        layout: &'a ShellLayout,
        inspector_expansion: [bool; 3],
        appearance: ResolvedAppearance,
        close_failure: Option<&str>,
        status: Option<&str>,
    ) -> Element<'a, Message> {
        let theme = ParchMintTheme::new(appearance);
        let editor = editor_center_surface(workspace.editor(), theme, editor_hosts, spelling_menu)
            .map(ProjectSurfaceMessage::EditorCenter);
        let surface = workspace_surface(
            workspace,
            destination,
            theme,
            editor,
            layout,
            inspector_expansion,
        )
        .map(move |message| Message::ProjectSurface {
            window: id,
            message,
        });
        let mut content = column![surface]
            .spacing(0)
            .width(Length::Fill)
            .height(Length::Fill);
        if let Some(error) = close_failure {
            content = content.push(
                container(
                    row![
                        text(format!("Final save failed: {error}")).size(13),
                        button("Retry").on_press(Message::RetryClose(id)),
                        button("Cancel Close").on_press(Message::CancelClose(id)),
                    ]
                    .spacing(8),
                )
                .padding([6, 12])
                .width(Length::Fill),
            );
        }
        if let Some(status) = status {
            content = content.push(
                container(text(status.to_owned()).size(12))
                    .padding([4, 12])
                    .width(Length::Fill),
            );
        }
        content.into()
    }

    fn update_project_surface(
        &mut self,
        id: window::Id,
        message: ProjectSurfaceMessage,
    ) -> Task<Message> {
        let Some(NativeWindow::Project(state)) = self.windows.get_mut(&id) else {
            return Task::none();
        };
        let Some(workspace) = state.workspace.as_mut() else {
            return Task::none();
        };

        if matches!(
            workspace.content_state(),
            crate::ContentState::Loading | crate::ContentState::Recovery
        ) && !matches!(
            &message,
            ProjectSurfaceMessage::Project(
                ProjectMessage::AcceptRecovery
                    | ProjectMessage::DiscardRecovery
                    | ProjectMessage::RetryRecovery
            )
        ) {
            return Task::none();
        }

        match message {
            ProjectSurfaceMessage::Navigate(destination) => {
                state.shell.select_destination(destination);
                if destination == RibbonDestination::History {
                    let ticket = workspace.begin_task(ProjectTask::LoadHistory);
                    if let Some(feeds) = state.service_feeds.as_ref() {
                        let job = feeds.history_list(None, 100, None);
                        let load = Task::perform(Self::run_service_job(job), move |result| {
                            Message::HistoryFinished {
                                window: id,
                                ticket,
                                append: false,
                                result,
                            }
                        });
                        let maintenance =
                            state
                                .project
                                .ports()
                                .cloned()
                                .map_or_else(Task::none, |ports| {
                                    Task::perform(
                                        Self::run_blocking_operation(
                                            "inspect History maintenance",
                                            move || {
                                                let access = ports
                                                    .access()
                                                    .map_err(|error| error.to_string())?;
                                                access
                                                    .history_maintenance(|history| history.status())
                                                    .map_err(|error| error.to_string())?
                                                    .map_err(|error| error.to_string())
                                            },
                                        ),
                                        move |result| Message::HistoryMaintenanceFinished {
                                            window: id,
                                            result,
                                        },
                                    )
                                });
                        let persist = Self::workspace_persist_task(id, state);
                        return Task::batch([load, maintenance, persist]);
                    }
                    workspace.accept_completion(ProjectTaskCompletion::for_ticket(
                        ticket,
                        ProjectTaskPayload::Failed(
                            "History is unavailable for this project session.".to_owned(),
                        ),
                    ));
                }
                if destination == RibbonDestination::RecentlyDeleted
                    && let Some(ProjectEffect::PreviewDeleted {
                        node_id,
                        checkpoint_id,
                        document_id,
                    }) = workspace.selected_deleted_preview_effect()
                {
                    let ticket = workspace.begin_task(ProjectTask::PreviewDeleted {
                        node_id: node_id.clone(),
                        checkpoint_id: checkpoint_id.clone(),
                        document_id: document_id.clone(),
                    });
                    if let Some(feeds) = state.service_feeds.as_ref() {
                        let job = feeds.deleted_preview(node_id, checkpoint_id, document_id);
                        let preview = Task::perform(Self::run_service_job(job), move |result| {
                            Message::DeletedPreviewFinished {
                                window: id,
                                ticket,
                                result,
                            }
                        });
                        let persist = Self::workspace_persist_task(id, state);
                        return Task::batch([preview, persist]);
                    }
                    workspace.accept_completion(ProjectTaskCompletion::for_ticket(
                        ticket,
                        ProjectTaskPayload::Failed(
                            "Recently Deleted preview is unavailable for this project session."
                                .to_owned(),
                        ),
                    ));
                }
                Self::workspace_persist_task(id, state)
            }
            ProjectSurfaceMessage::Focus(target) => {
                state.shell.focus(target.clone());
                if matches!(target, crate::FocusTarget::EditorDocument(_))
                    && let Some(binding) = state
                        .editor_bindings
                        .get(&workspace.editor().focused_pane())
                {
                    let _ = binding.restore_focus();
                }
                Task::none()
            }
            ProjectSurfaceMessage::ToggleExplorer => {
                let visible = !state.shell.layout().explorer_is_visible();
                state.shell.layout_mut().set_explorer_visible(visible);
                Self::workspace_persist_task(id, state)
            }
            ProjectSurfaceMessage::ToggleInspector => {
                let visible = !state.shell.layout().inspector_is_visible();
                state.shell.layout_mut().set_inspector_visible(visible);
                Self::workspace_persist_task(id, state)
            }
            ProjectSurfaceMessage::ToggleInspectorSection(section) => {
                state.shell.toggle_inspector_section(section);
                Task::none()
            }
            ProjectSurfaceMessage::OpenContextualHistory => {
                let Some(document) = workspace.focused_history_document().map(str::to_owned) else {
                    return Task::none();
                };
                workspace.update(ProjectMessage::SetHistoryDocumentFilter(Some(
                    document.clone(),
                )));
                state.shell.select_destination(RibbonDestination::History);
                let ticket = workspace.begin_task(ProjectTask::LoadHistory);
                let Some(feeds) = state.service_feeds.as_ref() else {
                    workspace.accept_completion(ProjectTaskCompletion::for_ticket(
                        ticket,
                        ProjectTaskPayload::Failed(
                            "History is unavailable for this project session.".to_owned(),
                        ),
                    ));
                    return Task::none();
                };
                let affected = stable_id_bytes(&document)
                    .ok()
                    .map(parchmint_domain::DocumentId::from_bytes);
                let job = feeds.history_list(None, 100, affected);
                let load = Task::perform(Self::run_service_job(job), move |result| {
                    Message::HistoryFinished {
                        window: id,
                        ticket,
                        append: false,
                        result,
                    }
                });
                let persist = Self::workspace_persist_task(id, state);
                Task::batch([load, persist])
            }
            ProjectSurfaceMessage::BeginResize(panel) => {
                state.resizing = Some(panel);
                Task::none()
            }
            ProjectSurfaceMessage::LoadMoreHistory => {
                let Some(cursor) = workspace.history().next_cursor().map(str::to_owned) else {
                    return Task::none();
                };
                workspace.begin_history_load_more();
                let ticket = workspace.begin_task(ProjectTask::LoadHistory);
                let Some(feeds) = state.service_feeds.as_ref() else {
                    workspace.accept_completion(ProjectTaskCompletion::for_ticket(
                        ticket,
                        ProjectTaskPayload::Failed(
                            "History is unavailable for this project session.".to_owned(),
                        ),
                    ));
                    return Task::none();
                };
                let affected = workspace
                    .history()
                    .active_document_filter()
                    .and_then(|document| stable_id_bytes(document).ok())
                    .map(parchmint_domain::DocumentId::from_bytes);
                let job = feeds.history_list(Some(HistoryCursor::new(cursor)), 100, affected);
                Task::perform(Self::run_service_job(job), move |result| {
                    Message::HistoryFinished {
                        window: id,
                        ticket,
                        append: true,
                        result,
                    }
                })
            }
            ProjectSurfaceMessage::Project(mut message) => {
                if let ProjectMessage::SelectHierarchy { gesture, .. }
                | ProjectMessage::BeginHierarchyDrag { gesture, .. } = &mut message
                    && *gesture == SelectionGesture::Replace
                {
                    *gesture = if state.modifiers.shift() {
                        SelectionGesture::ContiguousRange
                    } else if state.modifiers.command() {
                        SelectionGesture::Additive
                    } else {
                        SelectionGesture::Replace
                    };
                }
                let modal_before = workspace.modal().is_some();
                let opens_hierarchy_context =
                    matches!(&message, ProjectMessage::OpenHierarchyContextMenu { .. });
                let hierarchy_rename_target = match &message {
                    ProjectMessage::BeginHierarchyRename(node_id) => Some(node_id.clone()),
                    _ => None,
                };
                let appearance = match &message {
                    ProjectMessage::SetAppearance(mode) => Some(*mode),
                    _ => None,
                };
                let history_filter = match &message {
                    ProjectMessage::SetHistoryDocumentFilter(document) => Some(document.clone()),
                    _ => None,
                };
                let clipboard_status = match &message {
                    ProjectMessage::CopySelection => Some("Project item copied"),
                    ProjectMessage::CutSelection => Some("Project item ready to move"),
                    ProjectMessage::CancelCut => Some("Project move cancelled"),
                    ProjectMessage::PasteSelection { .. } => Some("Pasting project item…"),
                    _ => None,
                };
                if matches!(message, ProjectMessage::ShowGlobalSearch) {
                    state.shell.open_global_search();
                } else if matches!(message, ProjectMessage::ShowExplorer) {
                    state.shell.close_global_search();
                }
                let effects = workspace.update(message);
                if opens_hierarchy_context {
                    state.suppress_next_context_menu_dismissal = true;
                }
                let modal_after = workspace.modal().is_some();
                let focus_modal_initial = !modal_before && modal_after;
                if !modal_before && modal_after {
                    state
                        .shell
                        .open_dialog(crate::DialogKind::RestoreConfirmation);
                    state.modal_focus = ModalFocus::Cancel;
                } else if modal_before && !modal_after {
                    state.shell.dismiss_dialog();
                }
                if let Some(status) = clipboard_status {
                    self.status = Some(status.to_owned());
                }
                if let Some(mode) = appearance {
                    let callbacks = Arc::clone(&self.callbacks);
                    return Task::perform(
                        Self::run_blocking_operation("set appearance", move || {
                            callbacks.set_appearance(mode)
                        }),
                        move |result| Message::AppearanceFinished { mode, result },
                    );
                }
                let mut direct = Vec::new();
                let mut tasks = Vec::new();
                if focus_modal_initial {
                    tasks.push(iced::widget::operation::focus(
                        crate::focus::modal_cancel_id(),
                    ));
                }
                if let Some(node_id) = hierarchy_rename_target {
                    tasks.push(iced::widget::operation::focus(
                        crate::iced_project_surface::hierarchy_rename_input_id(&node_id),
                    ));
                }
                if let Some(document) = history_filter {
                    let ticket = workspace.begin_task(ProjectTask::LoadHistory);
                    if let Some(feeds) = state.service_feeds.as_ref() {
                        let affected = document
                            .as_deref()
                            .and_then(|document| stable_id_bytes(document).ok())
                            .map(parchmint_domain::DocumentId::from_bytes);
                        let job = feeds.history_list(None, 100, affected);
                        tasks.push(Task::perform(Self::run_service_job(job), move |result| {
                            Message::HistoryFinished {
                                window: id,
                                ticket,
                                append: false,
                                result,
                            }
                        }));
                    } else {
                        workspace.accept_completion(ProjectTaskCompletion::for_ticket(
                            ticket,
                            ProjectTaskPayload::Failed(
                                "History is unavailable for this project session.".to_owned(),
                            ),
                        ));
                    }
                }
                for effect in effects {
                    match effect {
                        ProjectEffect::CommitSynopsis { node_id, synopsis } => {
                            tasks.push(Self::queue_synopsis_commit_task(
                                &mut state.synopsis_commits,
                                &mut state.project_mutations,
                                node_id,
                                synopsis,
                            ));
                        }
                        ProjectEffect::ChooseExportDestination { output_name } => {
                            let Some(ports) = state.project.ports().cloned() else {
                                workspace.update(ProjectMessage::ExportFailed(
                                    "Export destination chooser is unavailable.".to_owned(),
                                ));
                                continue;
                            };
                            let capability = state.project.window;
                            tasks.push(Task::perform(
                                async move {
                                    let access =
                                        ports.access().map_err(|error| error.to_string())?;
                                    let selected = access
                                        .platform_services()
                                        .map_err(|error| error.to_string())?
                                        .dialogs
                                        .choose_path(
                                            capability,
                                            PathDialog {
                                                kind: PathDialogKind::SaveFile,
                                                title: Some(format!("Export {output_name}")),
                                            },
                                        )
                                        .await
                                        .map_err(|error| error.to_string())?;
                                    if selected.window() != capability {
                                        return Err(
                                            "export dialog returned for a stale window".to_owned()
                                        );
                                    }
                                    Ok(selected.into_value())
                                },
                                move |result| Message::ExportDestinationChosen {
                                    window: id,
                                    result,
                                },
                            ));
                        }
                        ProjectEffect::ReinitializeHistory => {
                            let Some(ports) = state.project.ports().cloned() else {
                                workspace.fail_history_workflow(
                                    "History maintenance is unavailable for this session."
                                        .to_owned(),
                                );
                                continue;
                            };
                            let mutation = state
                                .opaque_mutations
                                .begin(OpaqueMutationKind::HistoryReinitialize);
                            let launch = move |_: &NativeProjectState| {
                                let ports = ports.clone();
                                Task::perform(
                                    Self::run_blocking_operation(
                                        "reinitialize History",
                                        move || {
                                            let access = ports
                                                .access()
                                                .map_err(|error| error.to_string())?;
                                            access
                                                .history_maintenance(|history| {
                                                    history.reinitialize()
                                                })
                                                .map_err(|error| error.to_string())?
                                                .map_err(|error| error.to_string())
                                        },
                                    ),
                                    move |result| Message::HistoryReinitialized {
                                        window: id,
                                        mutation: Some(mutation),
                                        result,
                                    },
                                )
                            };
                            let prior_projects = state.project_mutations.queued.len();
                            state
                                .opaque_mutations
                                .enqueue(mutation, prior_projects, launch);
                        }
                        ProjectEffect::SearchProject {
                            query,
                            case_sensitive,
                            whole_word,
                            generation,
                        } => {
                            if let Some(feeds) = state.service_feeds.as_ref() {
                                let ticket =
                                    workspace.begin_task(ProjectTask::GlobalSearch { generation });
                                let start = feeds.search().start(SearchRequest {
                                    text: query,
                                    case_sensitive,
                                    whole_word,
                                    generation,
                                    metadata_fields: state
                                        .project
                                        .project_ui
                                        .as_ref()
                                        .map(|project| {
                                            project
                                                .snapshot
                                                .project
                                                .metadata
                                                .iter()
                                                .map(|field| field.id)
                                                .collect()
                                        })
                                        .unwrap_or_default(),
                                });
                                tasks.push(Task::perform(Self::run_search(start), move |result| {
                                    Message::SearchFinished {
                                        window: id,
                                        ticket,
                                        result,
                                    }
                                }));
                            } else {
                                let ticket =
                                    workspace.begin_task(ProjectTask::GlobalSearch { generation });
                                workspace.accept_completion(ProjectTaskCompletion::for_ticket(
                                    ticket,
                                    ProjectTaskPayload::Failed(
                                        "Project search is unavailable for this session."
                                            .to_owned(),
                                    ),
                                ));
                            }
                        }
                        ProjectEffect::PreviewHistory(checkpoint_id) => {
                            let current = state.project.project_ui.as_ref().and_then(|project| {
                                history_current_document(&project.snapshot, workspace)
                            });
                            let document_id = current
                                .as_ref()
                                .map(|document| document.document_id.clone());
                            workspace.set_history_current_document(current);
                            if let Some(feeds) = state.service_feeds.as_ref() {
                                let ticket = workspace.begin_task(ProjectTask::PreviewHistory {
                                    checkpoint_id: checkpoint_id.clone(),
                                });
                                let job = feeds.history_preview(checkpoint_id, document_id);
                                tasks.push(Task::perform(
                                    Self::run_service_job(job),
                                    move |result| Message::HistoryPreviewFinished {
                                        window: id,
                                        ticket,
                                        result,
                                    },
                                ));
                            } else {
                                let ticket = workspace
                                    .begin_task(ProjectTask::PreviewHistory { checkpoint_id });
                                workspace.accept_completion(ProjectTaskCompletion::for_ticket(
                                    ticket,
                                    ProjectTaskPayload::Failed(
                                        "History preview is unavailable for this project session."
                                            .to_owned(),
                                    ),
                                ));
                            }
                        }
                        ProjectEffect::PreviewDeleted {
                            node_id,
                            checkpoint_id,
                            document_id,
                        } => {
                            if let Some(feeds) = state.service_feeds.as_ref() {
                                let ticket = workspace.begin_task(ProjectTask::PreviewDeleted {
                                    node_id: node_id.clone(),
                                    checkpoint_id: checkpoint_id.clone(),
                                    document_id: document_id.clone(),
                                });
                                let job =
                                    feeds.deleted_preview(node_id, checkpoint_id, document_id);
                                tasks.push(Task::perform(
                                    Self::run_service_job(job),
                                    move |result| Message::DeletedPreviewFinished {
                                        window: id,
                                        ticket,
                                        result,
                                    },
                                ));
                            } else {
                                let ticket = workspace.begin_task(ProjectTask::PreviewDeleted {
                                    node_id,
                                    checkpoint_id,
                                    document_id,
                                });
                                workspace.accept_completion(ProjectTaskCompletion::for_ticket(
                                    ticket,
                                    ProjectTaskPayload::Failed(
                                        "Recently Deleted preview is unavailable for this project session."
                                            .to_owned(),
                                    ),
                                ));
                            }
                        }
                        ProjectEffect::FocusRecoveredEditor => {
                            if let (Some(feeds), Some(acceptance)) = (
                                state.service_feeds.as_ref(),
                                state.recovery_acceptance.take(),
                            ) {
                                let session = state.project.session;
                                let ticket = workspace.begin_task(ProjectTask::AcceptRecovery);
                                let feeds = feeds.clone();
                                let mutation = state
                                    .opaque_mutations
                                    .begin(OpaqueMutationKind::RecoveryAccept);
                                let launch = move |_: &NativeProjectState| {
                                    let job = feeds.accept_recovery(acceptance);
                                    let ticket = ticket.clone();
                                    Task::perform(Self::run_service_job(job), move |result| {
                                        Message::RecoveryAccepted {
                                            window: id,
                                            session,
                                            ticket,
                                            mutation: Some(mutation),
                                            result,
                                        }
                                    })
                                };
                                let prior_projects = state.project_mutations.queued.len();
                                state
                                    .opaque_mutations
                                    .enqueue(mutation, prior_projects, launch);
                            } else {
                                let ticket = workspace.begin_task(ProjectTask::AcceptRecovery);
                                workspace.accept_completion(ProjectTaskCompletion::for_ticket(
                                    ticket,
                                    ProjectTaskPayload::Failed(
                                        "Recovery must be reconciled again before it can be accepted."
                                            .to_owned(),
                                    ),
                                ));
                            }
                        }
                        ProjectEffect::DiscardRecovery => {
                            if let (Some(feeds), Some(acceptance)) = (
                                state.service_feeds.as_ref(),
                                state.recovery_acceptance.take(),
                            ) {
                                let session = state.project.session;
                                let ticket = workspace.begin_task(ProjectTask::DiscardRecovery);
                                let feeds = feeds.clone();
                                let mutation = state
                                    .opaque_mutations
                                    .begin(OpaqueMutationKind::RecoveryDiscard);
                                let launch = move |_: &NativeProjectState| {
                                    let job = feeds.discard_recovery(acceptance);
                                    let ticket = ticket.clone();
                                    Task::perform(Self::run_service_job(job), move |result| {
                                        Message::RecoveryDiscarded {
                                            window: id,
                                            session,
                                            ticket,
                                            mutation: Some(mutation),
                                            result,
                                        }
                                    })
                                };
                                let prior_projects = state.project_mutations.queued.len();
                                state
                                    .opaque_mutations
                                    .enqueue(mutation, prior_projects, launch);
                            } else {
                                let ticket = workspace.begin_task(ProjectTask::DiscardRecovery);
                                workspace.accept_completion(ProjectTaskCompletion::for_ticket(
                                    ticket,
                                    ProjectTaskPayload::Failed(
                                        "Recovery discard is unavailable for this project session."
                                            .to_owned(),
                                    ),
                                ));
                            }
                        }
                        ProjectEffect::ReconcileRecovery => {
                            if let Some(feeds) = state.service_feeds.as_ref() {
                                let session = state.project.session;
                                let ticket = workspace.begin_task(ProjectTask::ReconcileRecovery);
                                let job = feeds.reconcile_recovery();
                                tasks.push(Task::perform(
                                    Self::run_service_job(job),
                                    move |result| Message::RecoveryReconciled {
                                        window: id,
                                        session,
                                        ticket,
                                        result,
                                    },
                                ));
                            } else {
                                let ticket = workspace.begin_task(ProjectTask::ReconcileRecovery);
                                workspace.accept_completion(ProjectTaskCompletion::for_ticket(
                                    ticket,
                                    ProjectTaskPayload::Failed(
                                        "Recovery reconciliation is unavailable for this project session."
                                            .to_owned(),
                                    ),
                                ));
                            }
                        }
                        ProjectEffect::BuildReplacementPreview {
                            captured_project_revision,
                            replacement,
                            ..
                        } => {
                            let Some(project_ui) = state.project.project_ui.as_ref() else {
                                self.status = Some("project snapshot is unavailable".into());
                                continue;
                            };
                            let ticket = workspace.begin_task(ProjectTask::ReplacementPreview);
                            if project_ui.snapshot.project.revision.value()
                                != captured_project_revision
                            {
                                let error =
                                    "project changed before replacement preview revalidation"
                                        .to_owned();
                                workspace.accept_completion(ProjectTaskCompletion::for_ticket(
                                    ticket,
                                    ProjectTaskPayload::Failed(error.clone()),
                                ));
                                self.status = Some(error);
                                continue;
                            }
                            let preview_results = workspace.replacement_preview().results();
                            let selection = replacement_selection(
                                &project_ui.snapshot,
                                &preview_results,
                                &workspace
                                    .replacement_preview()
                                    .included_match_ids()
                                    .into_iter()
                                    .map(str::to_owned)
                                    .collect::<Vec<_>>(),
                                &replacement,
                            );
                            let ports = project_ui.ports.clone();
                            tasks.push(Task::perform(
                                async move {
                                    let selection = selection?;
                                    let access =
                                        ports.access().map_err(|error| error.to_string())?;
                                    access
                                        .replacements_service()
                                        .map_err(|error| error.to_string())?
                                        .preview(selection)
                                        .await
                                        .map(|_| ())
                                        .map_err(|error| error.to_string())
                                },
                                move |result| Message::ReplacementPreviewFinished {
                                    window: id,
                                    ticket,
                                    result,
                                },
                            ));
                        }
                        ProjectEffect::ApplyGlobalReplacement {
                            captured_project_revision,
                            included_match_ids,
                            replacement,
                        } => {
                            let Some(project_ui) = state.project.project_ui.as_ref() else {
                                self.status = Some("project snapshot is unavailable".into());
                                continue;
                            };
                            let ticket = workspace.begin_task(ProjectTask::ApplyReplacement);
                            if project_ui.snapshot.project.revision.value()
                                != captured_project_revision
                            {
                                let error =
                                    "project changed before replacement revalidation".to_owned();
                                workspace.accept_completion(ProjectTaskCompletion::for_ticket(
                                    ticket,
                                    ProjectTaskPayload::Failed(error.clone()),
                                ));
                                self.status = Some(error);
                                continue;
                            }
                            let mutation = state
                                .opaque_mutations
                                .begin(OpaqueMutationKind::Replacement);
                            let launch = move |state: &NativeProjectState| {
                                let ticket = ticket.clone();
                                let setup = (|| {
                                    let project_ui =
                                        state.project.project_ui.as_ref().ok_or_else(|| {
                                            "project snapshot is unavailable".to_owned()
                                        })?;
                                    let workspace = state.workspace.as_ref().ok_or_else(|| {
                                        "project workspace is unavailable".to_owned()
                                    })?;
                                    let selection = replacement_selection(
                                        &project_ui.snapshot,
                                        &workspace.replacement_preview().results(),
                                        &included_match_ids,
                                        &replacement,
                                    )?;
                                    Ok::<_, String>((project_ui.ports.clone(), selection))
                                })();
                                Task::perform(
                                    async move {
                                        let (ports, selection) = setup?;
                                        let access =
                                            ports.access().map_err(|error| error.to_string())?;
                                        access
                                            .replacements_service()
                                            .map_err(|error| error.to_string())?
                                            .apply(selection)
                                            .await
                                            .map_err(|error| error.to_string())?;
                                        access
                                            .snapshot(|query| query.snapshot())
                                            .map_err(|error| error.to_string())?
                                            .map_err(|error| error.to_string())
                                    },
                                    move |result| Message::ReplacementApplyFinished {
                                        window: id,
                                        ticket,
                                        mutation,
                                        result: Box::new(result),
                                    },
                                )
                            };
                            let prior_projects = state.project_mutations.queued.len();
                            state
                                .opaque_mutations
                                .enqueue(mutation, prior_projects, launch);
                        }
                        ProjectEffect::ExportEntireManuscript {
                            output_name: _,
                            number_documents,
                            source_revision,
                        } => {
                            let Some(ports) = state.project.ports().cloned() else {
                                let error =
                                    "Project export is unavailable for this session.".to_owned();
                                workspace.update(ProjectMessage::ExportFailed(error.clone()));
                                self.status = Some(error);
                                continue;
                            };
                            let ticket =
                                workspace.begin_task(ProjectTask::Export { source_revision });
                            let options = ExportRunOptions {
                                numbering: if number_documents {
                                    ExportNumbering::Documents
                                } else {
                                    ExportNumbering::None
                                },
                            };
                            let Some(selection) = state.export_destination.clone() else {
                                workspace.update(ProjectMessage::ExportFailed(
                                    "Select an export destination before running Export."
                                        .to_owned(),
                                ));
                                continue;
                            };
                            tasks.push(Self::export_task(id, ticket, ports, selection, options));
                        }
                        ProjectEffect::CancelExport => {
                            let Some(operation) = state.active_export else {
                                self.status = Some("export operation is no longer active".into());
                                continue;
                            };
                            let Some(ports) = state.project.ports().cloned() else {
                                self.status = Some("project export port is unavailable".into());
                                continue;
                            };
                            tasks.push(Task::perform(
                                Self::run_blocking_operation("cancel export", move || {
                                    let access =
                                        ports.access().map_err(|error| error.to_string())?;
                                    access
                                        .export_target(|export| export.cancel_export(operation))
                                        .map_err(|error| error.to_string())?
                                        .map_err(|error| error.to_string())
                                }),
                                move |result| Message::ExportCancelFinished {
                                    window: id,
                                    operation,
                                    result,
                                },
                            ));
                        }
                        ProjectEffect::OpenExportResult(artifact) => {
                            let action = ExportArtifactAction::Open;
                            let Some(ports) = state.project.ports().cloned() else {
                                self.status = Some("project export port is unavailable".into());
                                continue;
                            };
                            tasks.push(Task::perform(
                                Self::run_blocking_operation("open export artifact", move || {
                                    let access =
                                        ports.access().map_err(|error| error.to_string())?;
                                    access
                                        .export_target(|export| {
                                            export.act_on_artifact(artifact, action)
                                        })
                                        .map_err(|error| error.to_string())?
                                        .map_err(|error| error.to_string())
                                }),
                                Message::ExportArtifactActionFinished,
                            ));
                        }
                        ProjectEffect::RevealExportResult(artifact) => {
                            let action = ExportArtifactAction::Reveal;
                            let Some(ports) = state.project.ports().cloned() else {
                                self.status = Some("project export port is unavailable".into());
                                continue;
                            };
                            tasks.push(Task::perform(
                                Self::run_blocking_operation("open export artifact", move || {
                                    let access =
                                        ports.access().map_err(|error| error.to_string())?;
                                    access
                                        .export_target(|export| {
                                            export.act_on_artifact(artifact, action)
                                        })
                                        .map_err(|error| error.to_string())?
                                        .map_err(|error| error.to_string())
                                }),
                                Message::ExportArtifactActionFinished,
                            ));
                        }
                        effect if project_effect_requires_durability(&effect) => {
                            let history_action = match &effect {
                                ProjectEffect::CreateNamedSnapshot(_) => {
                                    Some(HistoryWorkflowAction::NamedSnapshot)
                                }
                                ProjectEffect::RestoreHistory { .. } => {
                                    Some(HistoryWorkflowAction::Restore)
                                }
                                _ => None,
                            };
                            state.project_mutations.enqueue(ProjectMutationTicket {
                                effect,
                                history_action,
                                synopsis_commit: None,
                            });
                        }
                        effect => direct.push(effect),
                    }
                }
                tasks.push(Self::project_effect_tasks(
                    id,
                    state.effect_executor.clone(),
                    direct,
                ));
                tasks.push(Self::launch_next_persistent_mutation(id, state));
                tasks.push(Self::workspace_persist_task(id, state));
                Task::batch(tasks)
            }
            ProjectSurfaceMessage::EditorCenter(message) => {
                if matches!(message, EditorCenterMessage::BeginSplitResize) {
                    state.resizing = Some(SidebarPanel::Editor);
                    return Task::none();
                }
                if let EditorCenterMessage::HierarchyDropTarget(pane) = message {
                    workspace.update(ProjectMessage::SetDragDestination(Some(
                        DragDestination::EditorPane(pane),
                    )));
                    return Task::none();
                }
                if let EditorCenterMessage::ClearHierarchyDropTarget(pane) = message {
                    workspace.update(ProjectMessage::ClearDragDestination(
                        DragDestination::EditorPane(pane),
                    ));
                    return Task::none();
                }
                if matches!(message, EditorCenterMessage::CommitHierarchyDrop) {
                    let effects = workspace.update(ProjectMessage::CommitHierarchyDrag);
                    return Task::batch([
                        Self::tracked_project_effect_tasks(id, state, effects),
                        Self::workspace_persist_task(id, state),
                    ]);
                }
                let refresh_local_search = message.workspace_messages().iter().any(|message| {
                    matches!(
                        message,
                        crate::EditorMessage::SetFindQuery(_)
                            | crate::EditorMessage::SetFindOptions { .. }
                    )
                });
                if let EditorCenterMessage::SetReplaceDraft { pane, value } = &message {
                    state.editor_hosts.set_replace_draft(*pane, value.clone());
                }

                let mut effects = Vec::new();
                for workspace_message in message.workspace_messages() {
                    effects.extend(workspace.editor_mut().update(workspace_message));
                }
                // An empty pane must stop rendering its mounted host in the
                // same update as the final tab close. Waiting for the
                // asynchronous effect executor leaves stale manuscript text
                // on screen for at least one frame (and indefinitely if that
                // task is superseded during shutdown).
                let mut immediate_effects = Vec::new();
                effects.retain(|effect| {
                    if let EditorEffect::UnmountView { pane, view } = effect {
                        immediate_effects.push((*pane, *view));
                        false
                    } else {
                        true
                    }
                });
                for (pane, view) in immediate_effects {
                    if let Some(binding) = state.editor_bindings.remove(&pane) {
                        if binding.view() == view {
                            if let Some(previous_document) = state.mounted_documents.remove(&pane) {
                                state
                                    .retained_editor_sessions
                                    .insert(previous_document, binding.session());
                            }
                            if let Err(error) = binding.detach() {
                                self.status = Some(error.to_string());
                            }
                        } else {
                            self.status =
                                Some("editor unmount view does not match the mounted pane".into());
                        }
                    }
                    state.mounted_documents.remove(&pane);
                    if pane == EditorPane::Primary {
                        state.editor_hosts.insert(
                            pane,
                            crate::iced_editor_surface::EditorPaneSlot::state(
                                crate::iced_editor_surface::EditorCenterPaneState::Empty,
                            ),
                        );
                    } else {
                        state.editor_hosts.remove(pane);
                    }
                }
                if refresh_local_search {
                    let pane = workspace.editor().focused_pane();
                    let view = workspace.editor().pane(pane).view();
                    if let Some(binding) = state.editor_bindings.get(&pane)
                        && let Some(adapter) = state.project.editor_adapter()
                    {
                        let search = workspace.editor().local_search(view);
                        match adapter.primary_visible_block(binding.session()) {
                            Ok(block) => {
                                let matches = local_find_matches(
                                    block.text(),
                                    search.query(),
                                    search.case_sensitive(),
                                    search.whole_word(),
                                );
                                effects.extend(
                                    workspace
                                        .editor_mut()
                                        .update(crate::EditorMessage::SetFindMatches(matches)),
                                );
                            }
                            Err(error) => self.status = Some(error.to_string()),
                        }
                    }
                }

                if let EditorCenterMessage::Mounted {
                    pane,
                    view,
                    message: parchmint_editor_iced::MountedEditorMessage::Clipboard(intent),
                } = &message
                {
                    return match Self::clipboard_task(id, state, *pane, *view, *intent) {
                        Ok(task) => task,
                        Err(error) => {
                            self.status = Some(error);
                            Task::none()
                        }
                    };
                }

                if let EditorCenterMessage::Mounted {
                    pane,
                    view,
                    message:
                        parchmint_editor_iced::MountedEditorMessage::OpenSpellingMenu {
                            comment_range,
                            spelling_range,
                            invocation_point,
                        },
                } = message
                {
                    return Self::open_spelling_menu(
                        id,
                        state,
                        pane,
                        view,
                        comment_range,
                        spelling_range,
                        invocation_point,
                    );
                }

                match message {
                    EditorCenterMessage::DismissSpellingMenu => {
                        state.pending_spelling_menu = None;
                        state.spelling_menu = None;
                        return Task::none();
                    }
                    EditorCenterMessage::ChooseSpellingAction(action) => {
                        return Self::choose_spelling_action(id, state, action);
                    }
                    _ => {}
                }

                if let EditorCenterMessage::Mounted {
                    pane,
                    view,
                    message,
                } = message
                {
                    let presentation_changed = matches!(
                        &message,
                        parchmint_editor_iced::MountedEditorMessage::Scroll { .. }
                            | parchmint_editor_iced::MountedEditorMessage::ViewportChanged(_)
                    );
                    let completed_word = completes_spellcheck_word(&message);
                    let update = if let Some(binding) = state.editor_bindings.get(&pane) {
                        if binding.view() != view {
                            Err(parchmint_editor_api::EditorError::InvalidCommand {
                                reason: "mounted editor message view does not match binding",
                            })
                        } else {
                            binding.update(message)
                        }
                    } else {
                        state.editor_hosts.update_mounted(pane, view, message)
                    };
                    match update {
                        Ok(update) => {
                            if presentation_changed
                                && let Some(binding) = state.editor_bindings.get(&pane)
                                && let Some(adapter) = state.project.editor_adapter()
                            {
                                match adapter.view_snapshot(binding.session(), view) {
                                    Ok(snapshot) => workspace.editor_mut().set_scroll_offset(
                                        pane,
                                        snapshot.presentation.pixel_scroll_y,
                                    ),
                                    Err(error) => self.status = Some(error.to_string()),
                                }
                            }
                            if let Some(style_name) = state
                                .project
                                .project_ui
                                .as_ref()
                                .and_then(|project| {
                                    project.snapshot.project.styles.get(update.active_style())
                                })
                                .map(|style| style.display_name.clone())
                            {
                                workspace.editor_mut().update(
                                    crate::EditorMessage::SetActiveParagraphStyle(style_name),
                                );
                            }
                            if !update.document_changed() {
                                return if presentation_changed {
                                    Self::workspace_persist_task(id, state)
                                } else {
                                    Task::none()
                                };
                            }
                            if let Some(session) = state
                                .editor_bindings
                                .get(&pane)
                                .map(MountedEditorBinding::session)
                                && let Err(error) = Self::refresh_shared_editor_hosts(
                                    &state.editor_bindings,
                                    &session,
                                )
                            {
                                self.status = Some(error);
                            }
                            let revision = update.revision();
                            let Some(session) = state
                                .editor_bindings
                                .get(&pane)
                                .map(MountedEditorBinding::session)
                            else {
                                self.status = Some(
                                    "edited document has no mounted editor session".to_owned(),
                                );
                                return Task::none();
                            };
                            workspace.update(ProjectMessage::MarkDirty(revision.value()));
                            state.autosave.mark_dirty(session, revision, Instant::now());
                            let delay = if completed_word { 150 } else { 400 };
                            state
                                .pending_spellchecks
                                .insert(view, Instant::now() + Duration::from_millis(delay));
                            return Task::none();
                        }
                        Err(parchmint_editor_api::EditorError::InvalidCommand { reason })
                            if reason == "mounted editor message has no host slot"
                                || reason
                                    == "mounted editor message view does not match host slot" =>
                        {
                            // The host was intentionally removed while Iced
                            // was dispatching its final event. A stale Canvas
                            // message is harmless and must not surface as an
                            // application error.
                            return Task::none();
                        }
                        Err(error) => self.status = Some(error.to_string()),
                    }
                } else if !effects.is_empty() {
                    let request_save = effects
                        .iter()
                        .any(|effect| matches!(effect, EditorEffect::RequestSave));
                    effects.retain(|effect| !matches!(effect, EditorEffect::RequestSave));
                    let editor_tasks = Self::editor_effect_tasks(
                        id,
                        state.effect_executor.clone(),
                        &state.project_mutations,
                        &mut state.opaque_mutations,
                        effects,
                    );
                    if request_save {
                        if state.project_mutations.blocks_close()
                            || state.opaque_mutations.blocks_close()
                        {
                            state.autosave.explicit_save_waiting = true;
                            let retry = Self::retry_pending_mutations(id, state);
                            self.status =
                                Some("Finishing project changes before saving…".to_owned());
                            return Task::batch([editor_tasks, retry]);
                        }
                        if state.autosave.save_in_flight {
                            // The active save captured an earlier immutable
                            // frontier. Preserve this explicit request so its
                            // projection/save runs after that frontier reaches
                            // a terminal result.
                            state.autosave.explicit_save_waiting = true;
                            return Task::batch([
                                editor_tasks,
                                Self::workspace_persist_task(id, state),
                            ]);
                        }
                        let Some(ports) = state.project.ports().cloned() else {
                            self.status =
                                Some("This project session has no persistence port.".into());
                            return editor_tasks;
                        };
                        let Some(adapter) = state.project.editor_adapter().cloned() else {
                            self.status = Some("project editor adapter is unavailable".into());
                            return editor_tasks;
                        };
                        let sessions = deduplicated_editor_sessions(
                            state
                                .editor_bindings
                                .values()
                                .map(MountedEditorBinding::session),
                            state.retained_editor_sessions.values().cloned(),
                        );
                        let plans = match editor_projection_plans(
                            adapter.as_ref(),
                            sessions,
                            &state.autosave.projected_sessions,
                        ) {
                            Ok(plans) => plans,
                            Err(error) => {
                                self.status = Some(error);
                                return editor_tasks;
                            }
                        };
                        if !state.autosave.requires_projection_save(!plans.is_empty()) {
                            state.autosave.explicit_save_waiting = false;
                            return Task::batch([
                                editor_tasks,
                                Self::workspace_persist_task(id, state),
                            ]);
                        }
                        let ticket = AutosaveTicket {
                            dirty_sessions: state.autosave.dirty_sessions.clone(),
                        };
                        let through_revision = workspace.project_revision();
                        state.autosave.save_in_flight = true;
                        workspace.update(ProjectMessage::StartSave(through_revision));
                        return Task::batch([
                            editor_tasks,
                            Self::autosave_task(
                                id,
                                ports,
                                adapter,
                                plans,
                                ticket,
                                ProjectSaveKind::Explicit,
                            ),
                            Self::workspace_persist_task(id, state),
                        ]);
                    }
                    return Task::batch([editor_tasks, Self::workspace_persist_task(id, state)]);
                }
                Self::workspace_persist_task(id, state)
            }
        }
    }

    fn project_effect_tasks(
        window: window::Id,
        executor: Option<NativeProjectEffectExecutor>,
        effects: Vec<ProjectEffect>,
    ) -> Task<Message> {
        let Some(executor) = executor else {
            return Task::none();
        };
        Task::batch(
            effects
                .into_iter()
                .map(|effect| Self::project_effect_task(window, executor.clone(), None, effect)),
        )
    }

    fn tracked_project_effect_tasks(
        window: window::Id,
        state: &mut NativeProjectState,
        effects: Vec<ProjectEffect>,
    ) -> Task<Message> {
        let mut tasks = Vec::new();
        let mut untracked = Vec::new();
        for effect in effects {
            let history_action = match &effect {
                ProjectEffect::CreateNamedSnapshot(_) => Some(HistoryWorkflowAction::NamedSnapshot),
                ProjectEffect::RestoreHistory { .. } => Some(HistoryWorkflowAction::Restore),
                _ => None,
            };
            if project_effect_requires_durability(&effect) {
                let ticket = ProjectMutationTicket {
                    effect: effect.clone(),
                    history_action,
                    synopsis_commit: None,
                };
                state.project_mutations.enqueue(ticket);
            } else {
                untracked.push(effect);
            }
        }
        tasks.push(Self::project_effect_tasks(
            window,
            state.effect_executor.clone(),
            untracked,
        ));
        tasks.push(Self::launch_next_persistent_mutation(window, state));
        Task::batch(tasks)
    }

    fn launch_project_mutation(
        window: window::Id,
        state: &NativeProjectState,
        ticket: ProjectMutationTicket,
    ) -> Task<Message> {
        let Some(executor) = state.effect_executor.clone() else {
            return Task::perform(async {}, move |_| Message::ProjectEffectFinished {
                window,
                mutation: Some(ticket),
                result: Err(ProjectRuntimeError::InvalidEffect(
                    "project mutation executor is unavailable",
                )),
            });
        };
        Self::project_effect_task(
            window,
            executor,
            Some(ticket.clone()),
            ticket.effect.clone(),
        )
    }

    fn launch_next_persistent_mutation(
        window: window::Id,
        state: &mut NativeProjectState,
    ) -> Task<Message> {
        match next_persistent_mutation_lane(&state.project_mutations, &state.opaque_mutations) {
            Some(PersistentMutationLane::Opaque(_)) => {
                let pending = state
                    .opaque_mutations
                    .queued
                    .pop_front()
                    .expect("front was checked");
                let launch = Arc::clone(&pending.launch);
                state.opaque_mutations.active = Some(pending);
                launch(state)
            }
            Some(PersistentMutationLane::Project) => {
                let ticket = state
                    .project_mutations
                    .start_next()
                    .expect("project lane requires a queued ticket");
                for pending in &mut state.opaque_mutations.queued {
                    pending.prior_projects = pending.prior_projects.saturating_sub(1);
                }
                Self::launch_project_mutation(window, state, ticket)
            }
            None => Task::none(),
        }
    }

    fn after_persistent_mutation_terminal(
        &mut self,
        window: window::Id,
        terminal: PersistentMutationTerminal,
        error: Option<String>,
    ) -> Task<Message> {
        let mut resume_close = false;
        let mut resume_explicit = false;
        let had_error = error.is_some();
        let next = {
            let Some(NativeWindow::Project(state)) = self.windows.get_mut(&window) else {
                return Task::none();
            };
            match terminal {
                PersistentMutationTerminal::ProjectSucceeded(ticket) => {
                    let next_synopsis = ticket
                        .synopsis_commit
                        .as_ref()
                        .and_then(|commit| state.synopsis_commits.finish(commit, true));
                    state.project_mutations.succeed(&ticket);
                    if let Some(commit) = next_synopsis {
                        state.project_mutations.enqueue(ProjectMutationTicket {
                            effect: ProjectEffect::CommitSynopsis {
                                node_id: commit.node_id.clone(),
                                synopsis: commit.synopsis.clone(),
                            },
                            history_action: None,
                            synopsis_commit: Some(commit),
                        });
                    }
                }
                PersistentMutationTerminal::ProjectEffectFailed(ticket) => {
                    Self::record_failed_project_mutation(state, ticket);
                }
                PersistentMutationTerminal::ProjectSaveFailed(ticket) => {
                    state.project_mutations.fail_save(ticket);
                }
                PersistentMutationTerminal::OpaqueSucceeded(token) => {
                    state.opaque_mutations.finish(token);
                }
                PersistentMutationTerminal::OpaqueFailed(token) => {
                    state.opaque_mutations.fail_effect(token);
                }
                PersistentMutationTerminal::OpaqueSaveFailed(token) => {
                    state.opaque_mutations.fail_save(token);
                }
            }
            if let Some(error) = error {
                self.status = Some(error.clone());
                if state.autosave.close_after_save {
                    state.autosave.close_after_save = false;
                    self.closing_windows.remove(&window);
                    self.close_failures.insert(state.project.window, error);
                }
            }
            let next = if had_error {
                Task::none()
            } else {
                Self::launch_next_persistent_mutation(window, state)
            };
            let drained = state.project_mutations.queued.is_empty()
                && state.project_mutations.active.is_none()
                && state.opaque_mutations.queued.is_empty()
                && state.opaque_mutations.active.is_none();
            if drained && !had_error {
                resume_close = state.autosave.close_after_save
                    && !state.project_mutations.blocks_close()
                    && !state.opaque_mutations.blocks_close();
                resume_explicit = state.autosave.explicit_save_waiting
                    && !state.project_mutations.blocks_close()
                    && !state.opaque_mutations.blocks_close();
            }
            next
        };
        if resume_close {
            Task::batch([next, self.continue_close_window(window)])
        } else if resume_explicit {
            Task::batch([
                next,
                self.start_projection_save(window, ProjectSaveKind::Explicit),
            ])
        } else {
            next
        }
    }

    fn retry_project_mutation(window: window::Id, state: &mut NativeProjectState) -> Task<Message> {
        if let Some(ticket) = state.project_mutations.retry_failed_save() {
            let Some(ports) = state.project.ports().cloned() else {
                state.project_mutations.fail_save(ticket);
                return Task::none();
            };
            state.autosave.save_in_flight = true;
            if let Some(workspace) = state.workspace.as_mut() {
                workspace.update(ProjectMessage::StartSave(workspace.project_revision()));
            }
            return Self::save_task(
                window,
                ports,
                ProjectSaveKind::Structural,
                SavePurpose::ProjectMutation(ticket),
            );
        }
        let Some(ticket) = state.project_mutations.retry_failed_effect() else {
            return Task::none();
        };
        if let Some(commit) = ticket.synopsis_commit.as_ref() {
            state.synopsis_commits.in_flight = Some(commit.clone());
        }
        Self::launch_project_mutation(window, state, ticket)
    }

    fn retry_pending_mutations(
        window: window::Id,
        state: &mut NativeProjectState,
    ) -> Task<Message> {
        if state.project_mutations.failed_save.is_some()
            || state.project_mutations.failed_effect.is_some()
        {
            return Self::retry_project_mutation(window, state);
        }
        if let Some(failed) = state.opaque_mutations.failed_effect.take() {
            let launch = Arc::clone(&failed.launch);
            state.opaque_mutations.active = Some(failed);
            return launch(state);
        }
        if let Some(token) = state.opaque_mutations.failed_save.take() {
            let Some(ports) = state.project.ports().cloned() else {
                state.opaque_mutations.failed_save = Some(token);
                return Task::none();
            };
            state.opaque_mutations.active = Some(PendingOpaqueMutation {
                token,
                prior_projects: 0,
                launch: Arc::new(|_| Task::none()),
            });
            state.autosave.save_in_flight = true;
            let revision = state
                .workspace
                .as_ref()
                .map_or(0, |workspace| workspace.project_revision());
            if let Some(workspace) = state.workspace.as_mut() {
                workspace.update(ProjectMessage::StartSave(revision));
            }
            return Self::save_task(
                window,
                ports,
                ProjectSaveKind::Structural,
                SavePurpose::OpaqueMutation(token),
            );
        }
        Task::none()
    }

    fn project_effect_task(
        window: window::Id,
        executor: NativeProjectEffectExecutor,
        mutation: Option<ProjectMutationTicket>,
        effect: ProjectEffect,
    ) -> Task<Message> {
        Task::perform(executor.execute_project_effect(effect), move |result| {
            Message::ProjectEffectFinished {
                window,
                mutation,
                result,
            }
        })
    }

    fn queue_synopsis_commit_task(
        queue: &mut SynopsisCommitQueue,
        mutations: &mut ProjectMutationState,
        node_id: String,
        synopsis: String,
    ) -> Task<Message> {
        let Some(commit) = queue.enqueue(SynopsisCommit { node_id, synopsis }) else {
            return Task::none();
        };
        let effect = ProjectEffect::CommitSynopsis {
            node_id: commit.node_id.clone(),
            synopsis: commit.synopsis.clone(),
        };
        mutations.enqueue(ProjectMutationTicket {
            effect,
            history_action: None,
            synopsis_commit: Some(commit),
        });
        Task::none()
    }

    fn open_spelling_menu(
        window: window::Id,
        state: &mut NativeProjectState,
        pane: EditorPane,
        view: ViewId,
        comment_range: EditorSelection,
        spelling_range: Option<EditorSelection>,
        invocation_point: (f32, f32),
    ) -> Task<Message> {
        let issue = spelling_range.and_then(|range| {
            state
                .spelling_issues
                .get(&view)
                .and_then(|issues| issues.iter().find(|issue| issue.range == range))
                .cloned()
        });
        let Some(binding) = state.editor_bindings.get(&pane) else {
            return Task::none();
        };
        if binding.view() != view {
            return Task::none();
        }
        let Some(adapter) = state.project.editor_adapter() else {
            return Task::none();
        };
        let session = binding.session();
        let revision = match adapter.revision(session.clone()) {
            Ok(revision) => revision,
            Err(_error) => {
                return Task::none();
            }
        };
        let block = match adapter.primary_visible_block(session.clone()) {
            Ok(block) => block,
            Err(_error) => {
                return Task::none();
            }
        };
        let anchor_range = issue
            .as_ref()
            .map(|issue| issue.range)
            .unwrap_or(comment_range);
        let rectangles = match adapter.geometry(session.clone(), view, block.block()) {
            Ok(geometry) if anchor_range.is_collapsed() => {
                geometry.caret(anchor_range.head()).into_iter().collect()
            }
            Ok(geometry) => geometry.selection_rectangles(anchor_range),
            Err(_error) => {
                return Task::none();
            }
        };
        let Some(first) = rectangles.first().copied() else {
            return Task::none();
        };
        let word_bounds = crate::Rect::new(first.x, first.y, first.width, first.height);
        let viewport = match adapter.view_snapshot(session.clone(), view) {
            Ok(snapshot) => snapshot.presentation.viewport,
            Err(_) => return Task::none(),
        };
        let in_project_dictionary = state.project.project_ui.as_ref().is_some_and(|project| {
            issue
                .as_ref()
                .is_some_and(|issue| project.snapshot.project.dictionary.contains(&issue.word))
        });
        state.pending_spelling_menu = Some(NativeSpellingMenuContext {
            pane,
            view,
            editor_session: session,
            revision,
            word: issue
                .as_ref()
                .map(|issue| issue.word.clone())
                .unwrap_or_default(),
            range: anchor_range,
            comment_range,
        });
        let request = SpellingMenuRequest::new(
            pane,
            issue
                .as_ref()
                .map(|issue| issue.word.clone())
                .unwrap_or_else(|| "Comment".into()),
            word_bounds,
            crate::Rect::new(0.0, 0.0, viewport.width, viewport.height),
        )
        .with_invocation_point(Point::new(invocation_point.0, invocation_point.1))
        .with_suggestions(
            issue
                .as_ref()
                .map(|issue| issue.suggestions.clone())
                .unwrap_or_default(),
        )
        .with_dictionary_membership(in_project_dictionary, false)
        .with_spelling_actions(issue.is_some());
        let Some(workspace) = state.workspace.as_mut() else {
            return Task::none();
        };
        let effects = workspace
            .editor_mut()
            .update(crate::EditorMessage::OpenSpellingMenu(request));
        Self::editor_effect_tasks(
            window,
            state.effect_executor.clone(),
            &state.project_mutations,
            &mut state.opaque_mutations,
            effects,
        )
    }

    fn choose_spelling_action(
        window: window::Id,
        state: &mut NativeProjectState,
        action: SpellingMenuAction,
    ) -> Task<Message> {
        let Some(context) = state.pending_spelling_menu.take() else {
            return Task::none();
        };
        state.spelling_menu = None;
        let Some(binding) = state.editor_bindings.get(&context.pane) else {
            return Task::none();
        };
        let Some(adapter) = state.project.editor_adapter() else {
            return Task::none();
        };
        if binding.view() != context.view
            || binding.session() != context.editor_session
            || adapter.revision(binding.session()).ok() != Some(context.revision)
        {
            return Task::none();
        }
        if action == SpellingMenuAction::AddComment {
            if adapter
                .execute(
                    binding.session(),
                    EditorCommandOrigin::new(context.view),
                    AdapterEditorCommand::new(
                        context.revision,
                        EditorCommandKind::SetSelection {
                            selection: context.comment_range,
                        },
                    ),
                )
                .is_err()
            {
                return Task::none();
            }
            let Some(workspace) = state.workspace.as_mut() else {
                return Task::none();
            };
            workspace
                .editor_mut()
                .update(crate::EditorMessage::BeginCommentAtSelection);
            return Task::none();
        }
        if action == SpellingMenuAction::Ignore {
            let mut remaining_issues = state
                .spelling_issues
                .get(&context.view)
                .cloned()
                .unwrap_or_default();
            remaining_issues.retain(|issue| issue.range != context.range);
            let remaining = remaining_issues
                .iter()
                .map(|issue| {
                    SpellingDecoration::new(
                        issue.word.clone(),
                        crate::FindMatch::new(
                            issue.range.start().value(),
                            issue.range.end().value(),
                        ),
                    )
                })
                .collect::<Vec<_>>();
            let shared_views = state
                .editor_bindings
                .values()
                .filter(|binding| binding.session() == context.editor_session)
                .map(MountedEditorBinding::view)
                .collect::<Vec<_>>();
            let Some(workspace) = state.workspace.as_mut() else {
                return Task::none();
            };
            let mut effects = Vec::new();
            for view in shared_views {
                state.spelling_issues.insert(view, remaining_issues.clone());
                effects.extend(workspace.editor_mut().update(
                    crate::EditorMessage::SetSpellingDecorations {
                        view,
                        decorations: remaining.clone(),
                    },
                ));
            }
            return Self::editor_effect_tasks(
                window,
                state.effect_executor.clone(),
                &state.project_mutations,
                &mut state.opaque_mutations,
                effects,
            );
        }
        if let Err(_error) = adapter.execute(
            binding.session(),
            EditorCommandOrigin::new(context.view),
            AdapterEditorCommand::new(
                context.revision,
                EditorCommandKind::SetSelection {
                    selection: context.range,
                },
            ),
        ) {
            return Task::none();
        }
        if !matches!(action, SpellingMenuAction::Replace(_)) {
            state.refresh_spellcheck_view = Some(context.view);
        }
        let Some(workspace) = state.workspace.as_mut() else {
            return Task::none();
        };
        let effects = workspace
            .editor_mut()
            .update(crate::EditorMessage::ChooseSpellingAction {
                pane: context.pane,
                word: context.word,
                action,
            });
        Self::editor_effect_tasks(
            window,
            state.effect_executor.clone(),
            &state.project_mutations,
            &mut state.opaque_mutations,
            effects,
        )
    }

    fn clipboard_task(
        window: window::Id,
        state: &mut NativeProjectState,
        pane: EditorPane,
        view: ViewId,
        intent: MountedEditorClipboardIntent,
    ) -> Result<Task<Message>, String> {
        let binding = state
            .editor_bindings
            .get(&pane)
            .ok_or_else(|| "clipboard action targets an unmounted editor".to_owned())?;
        if binding.view() != view {
            return Err("clipboard action view does not match the mounted editor".to_owned());
        }
        let adapter = state
            .project
            .editor_adapter()
            .ok_or_else(|| "project editor adapter is unavailable".to_owned())?;
        let editor_session = binding.session();
        let (revision, selection, plain_text) = match intent {
            MountedEditorClipboardIntent::Copy | MountedEditorClipboardIntent::Cut => {
                let Some(content) = adapter
                    .selection_clipboard(editor_session.clone(), view)
                    .map_err(|error| error.to_string())?
                else {
                    binding.restore_focus().map_err(|error| error.to_string())?;
                    return Ok(Task::none());
                };
                (
                    content.revision(),
                    content.selection(),
                    Some(content.plain_text().to_owned()),
                )
            }
            MountedEditorClipboardIntent::Paste
            | MountedEditorClipboardIntent::PasteWithoutFormatting => (
                adapter
                    .revision(editor_session.clone())
                    .map_err(|error| error.to_string())?,
                adapter
                    .selection(editor_session.clone(), view)
                    .map_err(|error| error.to_string())?,
                None,
            ),
        };
        let request = NativeClipboardRequest {
            capability: state.project.window,
            project_session: state.project.session,
            pane,
            view,
            editor_session,
            revision,
            selection,
            intent,
        };
        let ports = state
            .project
            .ports()
            .cloned()
            .ok_or_else(|| "project clipboard port is unavailable".to_owned())?;

        match plain_text {
            Some(plain_text) => {
                let capability = request.capability;
                let completion = request.clone();
                Ok(Task::perform(
                    async move {
                        let access = ports.access().map_err(|error| error.to_string())?;
                        let result = access
                            .platform_services()
                            .map_err(|error| error.to_string())?
                            .clipboard
                            .write(capability, ClipboardContent::plain_text(plain_text))
                            .await
                            .map_err(|error| error.to_string())?;
                        if result.window() != capability {
                            return Err("clipboard write returned for a stale window".to_owned());
                        }
                        Ok(())
                    },
                    move |result| Message::ClipboardWriteFinished {
                        window,
                        request: completion,
                        result,
                    },
                ))
            }
            None => {
                let capability = request.capability;
                let completion = request.clone();
                Ok(Task::perform(
                    async move {
                        let access = ports.access().map_err(|error| error.to_string())?;
                        let result = access
                            .platform_services()
                            .map_err(|error| error.to_string())?
                            .clipboard
                            .read(capability, ClipboardFormats::plain_text_and_html())
                            .await
                            .map_err(|error| error.to_string())?;
                        if result.window() != capability {
                            return Err("clipboard read returned for a stale window".to_owned());
                        }
                        Ok(result.into_value())
                    },
                    move |result| Message::ClipboardReadFinished {
                        window,
                        request: completion,
                        result,
                    },
                ))
            }
        }
    }

    fn finish_clipboard_write(
        &mut self,
        window: window::Id,
        request: NativeClipboardRequest,
        result: Result<(), String>,
    ) -> Task<Message> {
        if self.project_windows.get(&request.capability) != Some(&window) {
            self.status = Some("clipboard completion targets a stale window".to_owned());
            return Task::none();
        }
        let Some(NativeWindow::Project(state)) = self.windows.get_mut(&window) else {
            self.status = Some("clipboard completion targets a closed window".to_owned());
            return Task::none();
        };
        let binding = match clipboard_target(state, &request) {
            Ok(binding) => binding,
            Err(error) => {
                self.status = Some(error);
                return Task::none();
            }
        };
        if request.intent == MountedEditorClipboardIntent::Copy {
            match result {
                Ok(()) => {
                    self.status = binding.restore_focus().err().map(|error| error.to_string());
                }
                Err(error) => {
                    let _ = binding.restore_focus();
                    self.status = Some(error);
                }
            }
            return Task::none();
        }
        if request.intent != MountedEditorClipboardIntent::Cut {
            self.status = Some("clipboard write completion has the wrong intent".to_owned());
            return Task::none();
        }
        let Some(adapter) = state.project.editor_adapter().cloned() else {
            self.status = Some("project editor adapter is unavailable".to_owned());
            return Task::none();
        };
        let mutation = apply_completed_cut(adapter.as_ref(), binding, &request, result);
        self.finish_clipboard_mutation(window, mutation)
    }

    fn finish_clipboard_read(
        &mut self,
        window: window::Id,
        request: NativeClipboardRequest,
        result: Result<UntrustedClipboardContent, String>,
    ) -> Task<Message> {
        if self.project_windows.get(&request.capability) != Some(&window) {
            self.status = Some("clipboard completion targets a stale window".to_owned());
            return Task::none();
        }
        let Some(NativeWindow::Project(state)) = self.windows.get_mut(&window) else {
            self.status = Some("clipboard completion targets a closed window".to_owned());
            return Task::none();
        };
        let binding = match clipboard_target(state, &request) {
            Ok(binding) => binding,
            Err(error) => {
                self.status = Some(error);
                return Task::none();
            }
        };
        if !matches!(
            request.intent,
            MountedEditorClipboardIntent::Paste
                | MountedEditorClipboardIntent::PasteWithoutFormatting
        ) {
            self.status = Some("clipboard read completion has the wrong intent".to_owned());
            return Task::none();
        }
        let source = match result {
            Ok(source) => source,
            Err(error) => {
                let _ = binding.restore_focus();
                self.status = Some(error);
                return Task::none();
            }
        };
        let Some(adapter) = state.project.editor_adapter().cloned() else {
            self.status = Some("project editor adapter is unavailable".to_owned());
            return Task::none();
        };
        let mutation = apply_completed_paste(adapter.as_ref(), binding, &request, &source);
        self.finish_clipboard_mutation(window, mutation)
    }

    fn finish_clipboard_mutation(
        &mut self,
        window: window::Id,
        mutation: Result<Option<ClipboardMutation>, String>,
    ) -> Task<Message> {
        let mutation = match mutation {
            Ok(Some(mutation)) => mutation,
            Ok(None) => {
                self.status = None;
                return Task::none();
            }
            Err(error) => {
                self.status = Some(error);
                return Task::none();
            }
        };
        let Some(NativeWindow::Project(state)) = self.windows.get_mut(&window) else {
            self.status = Some("clipboard mutation completed for a closed window".to_owned());
            return Task::none();
        };
        let Some(workspace) = state.workspace.as_mut() else {
            self.status = Some("project workspace is unavailable".to_owned());
            return Task::none();
        };
        if let Some(style_name) = state
            .project
            .project_ui
            .as_ref()
            .and_then(|project| project.snapshot.project.styles.get(mutation.active_style))
            .map(|style| style.display_name.clone())
        {
            workspace
                .editor_mut()
                .update(crate::EditorMessage::SetActiveParagraphStyle(style_name));
        }
        workspace.update(ProjectMessage::MarkDirty(mutation.revision.value()));
        state
            .autosave
            .mark_dirty(mutation.editor_session, mutation.revision, Instant::now());
        self.status = mutation.presentation_error.or(mutation.feedback);
        Task::none()
    }

    fn editor_effect_tasks(
        window: window::Id,
        executor: Option<NativeProjectEffectExecutor>,
        project_mutations: &ProjectMutationState,
        opaque_mutations: &mut OpaqueMutationState,
        effects: Vec<EditorEffect>,
    ) -> Task<Message> {
        let Some(executor) = executor else {
            return Task::none();
        };
        let mut tasks = Vec::new();
        for effect in effects {
            let mutation = editor_effect_requires_durability(&effect)
                .then(|| opaque_mutations.begin(OpaqueMutationKind::ProjectDictionary));
            if let Some(token) = mutation {
                let lane_is_idle =
                    !project_mutations.blocks_close() && !opaque_mutations.blocks_close();
                if lane_is_idle {
                    let retry_effect = effect.clone();
                    opaque_mutations.active = Some(PendingOpaqueMutation {
                        token,
                        prior_projects: 0,
                        launch: Arc::new(move |state| {
                            let Some(executor) = state.effect_executor.clone() else {
                                return Task::perform(async {}, move |_| {
                                    Message::EditorEffectFinished {
                                        window,
                                        mutation: Some(token),
                                        result: Err(ProjectRuntimeError::InvalidEffect(
                                            "project dictionary executor is unavailable",
                                        )),
                                    }
                                });
                            };
                            Task::perform(
                                executor.execute_editor_effect(retry_effect.clone()),
                                move |result| Message::EditorEffectFinished {
                                    window,
                                    mutation: Some(token),
                                    result,
                                },
                            )
                        }),
                    });
                    let executor = executor.clone();
                    tasks.push(Task::perform(
                        executor.execute_editor_effect(effect),
                        move |result| Message::EditorEffectFinished {
                            window,
                            mutation: Some(token),
                            result,
                        },
                    ));
                } else {
                    let retry_effect = effect.clone();
                    opaque_mutations.enqueue(token, project_mutations.queued.len(), move |state| {
                        let Some(executor) = state.effect_executor.clone() else {
                            return Task::perform(async {}, move |_| {
                                Message::EditorEffectFinished {
                                    window,
                                    mutation: Some(token),
                                    result: Err(ProjectRuntimeError::InvalidEffect(
                                        "project dictionary executor is unavailable",
                                    )),
                                }
                            });
                        };
                        Task::perform(
                            executor.execute_editor_effect(retry_effect.clone()),
                            move |result| Message::EditorEffectFinished {
                                window,
                                mutation: Some(token),
                                result,
                            },
                        )
                    });
                }
            } else {
                let executor = executor.clone();
                tasks.push(Task::perform(
                    executor.execute_editor_effect(effect),
                    move |result| Message::EditorEffectFinished {
                        window,
                        mutation: None,
                        result,
                    },
                ));
            }
        }
        Task::batch(tasks)
    }

    fn finish_project_effect(
        &mut self,
        window: window::Id,
        mutation: Option<ProjectMutationTicket>,
        opaque_mutation: Option<OpaqueMutationToken>,
        result: Result<ProjectEffectCompletion, ProjectRuntimeError>,
    ) -> Task<Message> {
        let Some(NativeWindow::Project(state)) = self.windows.get_mut(&window) else {
            return Task::none();
        };
        let history_action = mutation.as_ref().and_then(|ticket| ticket.history_action);
        match result {
            Ok(ProjectEffectCompletion::WorkflowSnapshot(snapshot)) => {
                Self::prune_deleted_document_sessions(state, snapshot.as_ref());
                let snapshot = Arc::new(*snapshot);
                if let Some(project_ui) = state.project.project_ui.as_mut() {
                    project_ui.snapshot = Arc::clone(&snapshot);
                }
                if let Some(workspace) = state.workspace.as_mut() {
                    let has_later_mutations = !state.project_mutations.queued.is_empty()
                        || !state.opaque_mutations.queued.is_empty();
                    if should_reconcile_refreshed_presentation(
                        mutation.as_ref(),
                        opaque_mutation,
                        has_later_mutations,
                    ) {
                        workspace.reconcile_snapshot(&snapshot);
                    }
                    workspace.update(ProjectMessage::SaveCompleted(
                        snapshot.project.revision.value(),
                    ));
                    if history_action.is_some() {
                        workspace.complete_history_workflow();
                    }
                }
                state.effect_executor = state
                    .project
                    .ports()
                    .cloned()
                    .map(|ports| NativeProjectEffectExecutor::new(ports, snapshot));
                if let Err(error) = Self::refresh_mounted_style_catalogs(state) {
                    self.status = Some(format!("Could not refresh mounted styles: {error}"));
                }
                let mut reopen = Vec::new();
                if let Some(workspace) = state.workspace.as_ref() {
                    if let Some(document) = workspace
                        .editor()
                        .pane(EditorPane::Primary)
                        .active_document()
                    {
                        reopen.push(ProjectEffect::OpenDocumentInPrimary(document.to_owned()));
                    }
                    if let Some(document) = workspace
                        .editor()
                        .pane(EditorPane::Companion)
                        .active_document()
                    {
                        reopen.push(ProjectEffect::OpenDocumentInCompanion(document.to_owned()));
                    }
                }
                self.status = None;
                let reopen =
                    Self::project_effect_tasks(window, state.effect_executor.clone(), reopen);
                let refresh = if history_action.is_some() {
                    match (state.service_feeds.as_ref(), state.workspace.as_mut()) {
                        (Some(feeds), Some(workspace)) => {
                            let ticket = workspace.begin_task(ProjectTask::LoadHistory);
                            let job = feeds.history_list(None, 100, None);
                            Task::perform(Self::run_service_job(job), move |result| {
                                Message::HistoryFinished {
                                    window,
                                    ticket,
                                    append: false,
                                    result,
                                }
                            })
                        }
                        (None, Some(workspace)) => {
                            let ticket = workspace.begin_task(ProjectTask::LoadHistory);
                            workspace.accept_completion(ProjectTaskCompletion::for_ticket(
                                ticket,
                                ProjectTaskPayload::Failed(
                                    "History could not be refreshed because its service feed is unavailable."
                                        .to_owned(),
                                ),
                            ));
                            Task::none()
                        }
                        _ => Task::none(),
                    }
                } else {
                    Task::none()
                };
                let terminal = mutation.map_or_else(Task::none, |ticket| {
                    self.after_persistent_mutation_terminal(
                        window,
                        PersistentMutationTerminal::ProjectSucceeded(ticket),
                        None,
                    )
                });
                Task::batch([reopen, refresh, terminal])
            }
            Ok(ProjectEffectCompletion::RefreshedSnapshot(snapshot)) => {
                let snapshot = *snapshot;
                Self::prune_deleted_document_sessions(state, &snapshot);
                if let Some(workspace) = state.workspace.as_mut() {
                    // Project mutations are optimistically reduced before
                    // their asynchronous command executes. Replacing the
                    // whole presentation here would erase later queued edits
                    // which have not reached the command lane yet.
                    let has_later_mutations = !state.project_mutations.queued.is_empty()
                        || !state.opaque_mutations.queued.is_empty();
                    if should_reconcile_refreshed_presentation(
                        mutation.as_ref(),
                        opaque_mutation,
                        has_later_mutations,
                    ) {
                        workspace.reconcile_snapshot(&snapshot);
                    }
                    workspace.update(ProjectMessage::MarkDirty(snapshot.project.revision.value()));
                }
                if let Some(project_ui) = state.project.project_ui.as_mut() {
                    project_ui.snapshot = Arc::new(snapshot.clone());
                    state.effect_executor = Some(NativeProjectEffectExecutor::new(
                        project_ui.ports.clone(),
                        Arc::clone(&project_ui.snapshot),
                    ));
                }
                if let Err(error) = Self::refresh_mounted_style_catalogs(state) {
                    self.status = Some(format!("Could not refresh mounted styles: {error}"));
                }
                let Some(ports) = state.project.ports().cloned() else {
                    let error = "Project mutation completed without a persistence port.".to_owned();
                    let terminal = mutation
                        .map(PersistentMutationTerminal::ProjectSaveFailed)
                        .or_else(|| {
                            opaque_mutation.map(PersistentMutationTerminal::OpaqueSaveFailed)
                        });
                    if let Some(terminal) = terminal {
                        return self.after_persistent_mutation_terminal(
                            window,
                            terminal,
                            Some(error),
                        );
                    }
                    self.status = Some(error);
                    return Task::none();
                };
                let through_revision = snapshot.project.revision.value();
                state.autosave.save_in_flight = true;
                if let Some(workspace) = state.workspace.as_mut() {
                    workspace.update(ProjectMessage::StartSave(through_revision));
                }
                let purpose = mutation.map_or_else(
                    || opaque_mutation.map_or(SavePurpose::Untracked, SavePurpose::OpaqueMutation),
                    SavePurpose::ProjectMutation,
                );
                Self::save_task(window, ports, ProjectSaveKind::Structural, purpose)
            }
            Ok(ProjectEffectCompletion::TreePaste {
                snapshot,
                kind,
                created_roots,
            }) => {
                Self::prune_deleted_document_sessions(state, snapshot.as_ref());
                let snapshot = Arc::new(*snapshot);
                if let Some(project_ui) = state.project.project_ui.as_mut() {
                    project_ui.snapshot = Arc::clone(&snapshot);
                }
                if let Some(workspace) = state.workspace.as_mut() {
                    let has_later_mutations = !state.project_mutations.queued.is_empty()
                        || !state.opaque_mutations.queued.is_empty();
                    if should_reconcile_refreshed_presentation(
                        mutation.as_ref(),
                        opaque_mutation,
                        has_later_mutations,
                    ) {
                        workspace.reconcile_snapshot(&snapshot);
                    }
                    if !created_roots.is_empty() {
                        workspace.select_tree_roots(&created_roots);
                    }
                    workspace.complete_tree_paste(kind);
                    workspace.update(ProjectMessage::SaveCompleted(
                        snapshot.project.revision.value(),
                    ));
                }
                state.effect_executor = state
                    .project
                    .ports()
                    .cloned()
                    .map(|ports| NativeProjectEffectExecutor::new(ports, snapshot));
                if let Err(error) = Self::refresh_mounted_style_catalogs(state) {
                    self.status = Some(format!("Could not refresh mounted styles: {error}"));
                }
                self.status = Some(match kind {
                    crate::TreeClipboardKind::Copy => "Project item pasted".to_owned(),
                    crate::TreeClipboardKind::Cut => "Project item moved".to_owned(),
                });
                let terminal = mutation.map_or_else(Task::none, |ticket| {
                    self.after_persistent_mutation_terminal(
                        window,
                        PersistentMutationTerminal::ProjectSucceeded(ticket),
                        None,
                    )
                });
                if let Some(token) = opaque_mutation {
                    return self.after_persistent_mutation_terminal(
                        window,
                        PersistentMutationTerminal::OpaqueSucceeded(token),
                        None,
                    );
                }
                terminal
            }
            Ok(ProjectEffectCompletion::OpenDocuments {
                snapshot,
                documents,
            }) => {
                Self::accept_hydrated_snapshot(state, *snapshot);
                let mut spellcheck_tasks = Vec::new();
                for document in documents {
                    let pane = document.pane;
                    if let Err(error) =
                        Self::mount_resolved_document(state, document, self.appearance)
                    {
                        self.status = Some(error);
                        break;
                    }
                    if let Some(view) = state
                        .editor_bindings
                        .get(&pane)
                        .map(MountedEditorBinding::view)
                        && let Ok(task) = Self::spellcheck_task(window, state, view)
                    {
                        spellcheck_tasks.push(task);
                    }
                }
                Task::batch(spellcheck_tasks)
            }
            Ok(ProjectEffectCompletion::ApplyAppearance(snapshot)) => {
                self.appearance = snapshot.appearance;
                self.status = None;
                Task::none()
            }
            Ok(ProjectEffectCompletion::SavedThrough(revision)) => {
                if let Some(workspace) = state.workspace.as_mut() {
                    workspace.update(ProjectMessage::SaveCompleted(revision));
                }
                Task::none()
            }
            Ok(ProjectEffectCompletion::FocusRecoveredEditor) => {
                if let Some(workspace) = state.workspace.as_mut() {
                    workspace
                        .editor_mut()
                        .update(crate::EditorMessage::FocusPane(EditorPane::Primary));
                }
                if let Some(binding) = state.editor_bindings.get(&EditorPane::Primary)
                    && let Err(error) = binding.update(
                        parchmint_editor_iced::MountedEditorMessage::Focus(0_u64.into()),
                    )
                {
                    self.status = Some(error.to_string());
                }
                Task::none()
            }
            Ok(ProjectEffectCompletion::NavigateSearch {
                snapshot,
                document,
                range,
            }) => {
                Self::accept_hydrated_snapshot(state, *snapshot);
                if let Err(error) = Self::mount_resolved_document(state, document, self.appearance)
                {
                    self.status = Some(error);
                    return Task::none();
                }
                let Some(binding) = state.editor_bindings.get(&EditorPane::Primary) else {
                    self.status = Some("search navigation did not mount an editor".into());
                    return Task::none();
                };
                let Some(adapter) = state.project.editor_adapter() else {
                    self.status = Some("project editor adapter is unavailable".into());
                    return Task::none();
                };
                let revision = match adapter.revision(binding.session()) {
                    Ok(revision) => revision,
                    Err(error) => {
                        self.status = Some(error.to_string());
                        return Task::none();
                    }
                };
                let result = adapter.execute(
                    binding.session(),
                    EditorCommandOrigin::new(binding.view()),
                    AdapterEditorCommand::new(
                        revision,
                        EditorCommandKind::SetSelection {
                            selection: EditorSelection::new(
                                range.start().into(),
                                range.end().into(),
                            ),
                        },
                    ),
                );
                if let Err(error) = result {
                    self.status = Some(error.to_string());
                } else if let Err(error) = binding.refresh() {
                    self.status = Some(error.to_string());
                } else {
                    self.status = None;
                }
                Task::none()
            }
            Err(error) => {
                let error = format!("Project action could not complete: {error}");
                if history_action.is_some()
                    && let Some(workspace) = state.workspace.as_mut()
                {
                    workspace.fail_history_workflow(error.clone());
                }
                let terminal = mutation
                    .map(PersistentMutationTerminal::ProjectEffectFailed)
                    .or_else(|| opaque_mutation.map(PersistentMutationTerminal::OpaqueFailed));
                if let Some(terminal) = terminal {
                    self.after_persistent_mutation_terminal(window, terminal, Some(error))
                } else {
                    self.status = Some(error);
                    Task::none()
                }
            }
        }
    }

    fn record_failed_project_mutation(
        state: &mut NativeProjectState,
        mut ticket: ProjectMutationTicket,
    ) {
        if let Some(commit) = ticket.synopsis_commit.as_ref() {
            state.synopsis_commits.finish(commit, false);
            let latest = state.synopsis_commits.retain_latest_failed(commit);
            ticket.effect = ProjectEffect::CommitSynopsis {
                node_id: latest.node_id.clone(),
                synopsis: latest.synopsis.clone(),
            };
            ticket.synopsis_commit = Some(latest);
        }
        state.project_mutations.fail_effect(ticket);
    }

    fn refresh_mounted_style_catalogs(state: &NativeProjectState) -> Result<(), String> {
        let Some(project_ui) = state.project.project_ui.as_ref() else {
            return Ok(());
        };
        let Some(adapter) = state.project.editor_adapter() else {
            return Ok(());
        };
        let styles = StyleCatalogProjection::new(project_ui.snapshot.project.styles.clone());
        let mut refreshed = Vec::new();
        for binding in state.editor_bindings.values() {
            let session = binding.session();
            if refreshed.contains(&session) {
                continue;
            }
            adapter
                .set_style_catalog(session.clone(), styles.clone())
                .map_err(|error| error.to_string())?;
            refreshed.push(session);
        }
        Ok(())
    }

    fn accept_hydrated_snapshot(state: &mut NativeProjectState, mut snapshot: ProjectSnapshot) {
        if let Some(current) = state.project.project_ui.as_ref().map(|ui| &ui.snapshot) {
            for loaded in &current.documents {
                let same_frontier = snapshot
                    .document_summaries
                    .iter()
                    .find(|summary| summary.document_id == loaded.document_id)
                    .is_some_and(|summary| summary.revision == loaded.revision);
                if same_frontier
                    && !snapshot
                        .documents
                        .iter()
                        .any(|candidate| candidate.document_id == loaded.document_id)
                {
                    snapshot.documents.push(loaded.clone());
                }
            }
        }
        let snapshot = Arc::new(snapshot);
        if let Some(project_ui) = state.project.project_ui.as_mut() {
            project_ui.snapshot = Arc::clone(&snapshot);
        }
        if let Some(workspace) = state.workspace.as_mut() {
            workspace.reconcile_snapshot(&snapshot);
        }
        state.effect_executor = state
            .effect_executor
            .as_ref()
            .map(|executor| executor.refreshed(Arc::clone(&snapshot)))
            .or_else(|| {
                state
                    .project
                    .ports()
                    .cloned()
                    .map(|ports| NativeProjectEffectExecutor::new(ports, snapshot))
            });
    }

    fn finish_editor_effect(
        &mut self,
        window: window::Id,
        mutation: Option<OpaqueMutationToken>,
        result: Result<EditorEffectCompletion, ProjectRuntimeError>,
    ) -> Task<Message> {
        match result {
            Ok(EditorEffectCompletion::ProjectMutation(completion)) => {
                let project = self.finish_project_effect(window, None, mutation, Ok(completion));
                let refresh = self
                    .windows
                    .get_mut(&window)
                    .and_then(|native| match native {
                        NativeWindow::Project(state) => state
                            .refresh_spellcheck_view
                            .take()
                            .and_then(|view| Self::spellcheck_task(window, state, view).ok()),
                        NativeWindow::Launcher => None,
                    })
                    .unwrap_or_else(Task::none);
                Task::batch([project, refresh])
            }
            Ok(EditorEffectCompletion::GlobalDictionaryUpdated) => {
                self.status = None;
                let refresh = self
                    .windows
                    .get_mut(&window)
                    .and_then(|native| match native {
                        NativeWindow::Project(state) => state
                            .refresh_spellcheck_view
                            .take()
                            .and_then(|view| Self::spellcheck_task(window, state, view).ok()),
                        NativeWindow::Launcher => None,
                    })
                    .unwrap_or_else(Task::none);
                let terminal = mutation.map_or_else(Task::none, |token| {
                    self.after_persistent_mutation_terminal(
                        window,
                        PersistentMutationTerminal::OpaqueSucceeded(token),
                        None,
                    )
                });
                Task::batch([refresh, terminal])
            }
            Ok(EditorEffectCompletion::SavedThrough(revision)) => {
                let mut continue_close = false;
                if let Some(NativeWindow::Project(state)) = self.windows.get_mut(&window)
                    && let Some(workspace) = state.workspace.as_mut()
                {
                    state.autosave.save_in_flight = false;
                    continue_close = state.autosave.close_after_save;
                    state.autosave.close_after_save = false;
                    workspace.update(ProjectMessage::SaveCompleted(revision));
                }
                self.status = None;
                if continue_close {
                    self.continue_close_window(window)
                } else {
                    Task::none()
                }
            }
            Ok(EditorEffectCompletion::Intent(intent)) => {
                let Some(NativeWindow::Project(state)) = self.windows.get_mut(&window) else {
                    return Task::none();
                };
                if let EditorRuntimeIntent::Mount {
                    pane,
                    load,
                    snapshot,
                    ..
                } = &intent
                {
                    let active_document = state
                        .workspace
                        .as_ref()
                        .and_then(|workspace| workspace.editor().pane(*pane).active_document());
                    if !mount_matches_active_document(active_document, load.document_id) {
                        // Mount work runs asynchronously. A later tab activation
                        // may have superseded this completion, so it must not
                        // hydrate its snapshot or replace the retained host.
                        return Task::none();
                    }
                    Self::accept_hydrated_snapshot(state, snapshot.as_ref().clone());
                }
                let spellcheck_view = editor_intent_view(&intent);
                match Self::apply_editor_intent(state, intent, self.appearance) {
                    Ok(Some((session, revision))) => {
                        let Some(workspace) = state.workspace.as_mut() else {
                            return Task::none();
                        };
                        workspace.update(ProjectMessage::MarkDirty(revision.value()));
                        state.autosave.mark_dirty(session, revision, Instant::now());
                        let spellcheck = spellcheck_view
                            .map(|view| Self::spellcheck_task(window, state, view))
                            .transpose()
                            .unwrap_or_else(|error| {
                                self.status = Some(error);
                                None
                            })
                            .unwrap_or_else(Task::none);
                        return spellcheck;
                    }
                    Ok(None) => {}
                    Err(error) => self.status = Some(error),
                }
                Task::none()
            }
            Err(error) => {
                let error = format!("Editor action could not complete: {error}");
                if let Some(token) = mutation {
                    return self.after_persistent_mutation_terminal(
                        window,
                        PersistentMutationTerminal::OpaqueFailed(token),
                        Some(error),
                    );
                }
                self.status = Some(error);
                Task::none()
            }
        }
    }

    fn mount_resolved_document(
        state: &mut NativeProjectState,
        document: ResolvedDocumentMount,
        appearance: ResolvedAppearance,
    ) -> Result<(), String> {
        let view = state
            .workspace
            .as_ref()
            .ok_or_else(|| "project workspace is unavailable".to_owned())?
            .editor()
            .pane(document.pane)
            .view();
        Self::mount_editor_load(state, document.pane, view, document.load, appearance)
    }

    /// Refreshes every retained Canvas sharing `session` after its adapter
    /// frame has already been advanced. A document shown in split panes now
    /// redraws in both places without running the expensive layout step twice.
    fn refresh_shared_editor_hosts(
        bindings: &BTreeMap<EditorPane, MountedEditorBinding>,
        session: &parchmint_editor_api::SharedEditorSession,
    ) -> Result<(), String> {
        for binding in bindings.values() {
            if binding.session() == *session {
                binding
                    .refresh_after_shared_frame()
                    .map_err(|error| error.to_string())?;
            }
        }
        Ok(())
    }

    fn restored_workspace_effects(
        state: &mut NativeProjectState,
    ) -> Result<Vec<ProjectEffect>, String> {
        let targets = [EditorPane::Primary, EditorPane::Companion].map(|pane| {
            let document = state
                .workspace
                .as_ref()
                .and_then(|workspace| workspace.editor().pane(pane).active_document())
                .map(str::to_owned);
            (pane, document)
        });
        let mut effects = Vec::new();
        for (pane, document) in targets {
            let Some(document) = document else {
                if let Some(binding) = state.editor_bindings.remove(&pane) {
                    if let Some(previous_document) = state.mounted_documents.remove(&pane) {
                        state
                            .retained_editor_sessions
                            .insert(previous_document, binding.session());
                    }
                    binding.detach().map_err(|error| error.to_string())?;
                }
                state.editor_hosts.remove(pane);
                continue;
            };
            let document_id = stable_id_bytes(&document)
                .map(parchmint_domain::DocumentId::from_bytes)
                .map_err(|error| error.to_string())?;
            if state.mounted_documents.get(&pane) == Some(&document_id) {
                continue;
            }
            effects.push(restored_document_effect(pane, document));
        }
        Ok(effects)
    }

    fn mount_editor_load(
        state: &mut NativeProjectState,
        pane: EditorPane,
        view: parchmint_editor_api::ViewId,
        load: CanonicalDocumentLoad,
        appearance: ResolvedAppearance,
    ) -> Result<(), String> {
        let scroll_offset = state
            .workspace
            .as_ref()
            .map(|workspace| workspace.editor().pane(pane).scroll_offset())
            .unwrap_or_default();
        let adapter = state
            .project
            .editor_adapter()
            .cloned()
            .ok_or_else(|| "project editor adapter is unavailable".to_owned())?;
        let document_id = load.document_id;
        let shared = state
            .mounted_documents
            .iter()
            .find(|(mounted_pane, mounted)| **mounted_pane != pane && **mounted == document_id)
            .and_then(|(mounted_pane, _)| state.editor_bindings.get(mounted_pane))
            .map(MountedEditorBinding::session);
        if let Some(binding) = state.editor_bindings.remove(&pane) {
            if let Some(previous_document) = state.mounted_documents.remove(&pane) {
                state
                    .retained_editor_sessions
                    .insert(previous_document, binding.session());
            }
            binding.detach().map_err(|error| error.to_string())?;
        }
        let session =
            mount_session_for_document(shared, &state.retained_editor_sessions, document_id, load);
        let viewport = EditorViewport::new(720.0, 520.0)
            .expect("native editor viewport constants must be positive");
        let theme = match appearance {
            ResolvedAppearance::Light => EditorSurfaceTheme::light(),
            ResolvedAppearance::Dark => EditorSurfaceTheme::dark(),
        };
        let binding = MountedEditorBinding::mount(
            adapter.as_ref(),
            MountedEditorBindingConfig::new(session, state.project.window, view, viewport, theme),
        )
        .map_err(|error| error.to_string())?;
        binding
            .restore_scroll(viewport, scroll_offset)
            .map_err(|error| error.to_string())?;
        state.editor_hosts.insert(
            pane,
            crate::iced_editor_surface::EditorPaneSlot::mounted(binding.host().clone()),
        );
        let active_style = binding.active_style().map_err(|error| error.to_string())?;
        let mounted_session = binding.session();
        let mounted_revision = adapter
            .revision(mounted_session.clone())
            .map_err(|error| error.to_string())?;
        state
            .autosave
            .projected_sessions
            .entry(mounted_session)
            .or_insert(mounted_revision);
        state.editor_bindings.insert(pane, binding);
        state.mounted_documents.insert(pane, document_id);
        if let Some(style_name) = state
            .project
            .project_ui
            .as_ref()
            .and_then(|project| project.snapshot.project.styles.get(active_style))
            .map(|style| style.display_name.clone())
            && let Some(workspace) = state.workspace.as_mut()
        {
            workspace
                .editor_mut()
                .update(crate::EditorMessage::SetActiveParagraphStyle(style_name));
        }
        Self::prune_durable_unmounted_sessions(state);
        Ok(())
    }

    fn apply_editor_intent(
        state: &mut NativeProjectState,
        intent: EditorRuntimeIntent,
        appearance: ResolvedAppearance,
    ) -> Result<Option<(parchmint_editor_api::SharedEditorSession, EditorRevision)>, String> {
        match intent {
            EditorRuntimeIntent::Command { view, command } => {
                Self::apply_editor_command(state, view, command)
            }
            EditorRuntimeIntent::Mount {
                pane,
                view,
                load,
                snapshot: _,
            } => {
                Self::mount_editor_load(state, pane, view, load, appearance)?;
                Ok(None)
            }
            EditorRuntimeIntent::Unmount { pane, view } => {
                if let Some(binding) = state.editor_bindings.remove(&pane) {
                    if binding.view() != view {
                        return Err("editor unmount view does not match the mounted pane".into());
                    }
                    if let Some(previous_document) = state.mounted_documents.remove(&pane) {
                        state
                            .retained_editor_sessions
                            .insert(previous_document, binding.session());
                    }
                    binding.detach().map_err(|error| error.to_string())?;
                }
                if pane == EditorPane::Primary {
                    state.editor_hosts.insert(
                        pane,
                        crate::iced_editor_surface::EditorPaneSlot::state(
                            crate::iced_editor_surface::EditorCenterPaneState::Empty,
                        ),
                    );
                } else {
                    state.editor_hosts.remove(pane);
                }
                Ok(None)
            }
            EditorRuntimeIntent::SetSearchDecorations {
                view,
                mut decorations,
                active,
            } => {
                if let Some(active) = active {
                    decorations.push(active);
                }
                let binding = state
                    .editor_bindings
                    .values()
                    .find(|binding| binding.view() == view)
                    .ok_or_else(|| "search decorations target an unmounted view".to_owned())?;
                state
                    .project
                    .editor_adapter()
                    .ok_or_else(|| "project editor adapter is unavailable".to_owned())?
                    .set_search_decorations(binding.session(), view, decorations)
                    .map_err(|error| error.to_string())?;
                Ok(None)
            }
            EditorRuntimeIntent::SetSpellcheckDecorations { view, decorations } => {
                let binding = state
                    .editor_bindings
                    .values()
                    .find(|binding| binding.view() == view)
                    .ok_or_else(|| "spellcheck decorations target an unmounted view".to_owned())?;
                state
                    .project
                    .editor_adapter()
                    .ok_or_else(|| "project editor adapter is unavailable".to_owned())?
                    .set_spellcheck_decorations(binding.session(), view, decorations)
                    .map_err(|error| error.to_string())?;
                binding.refresh().map_err(|error| error.to_string())?;
                Ok(None)
            }
            EditorRuntimeIntent::NavigateCommentAnchor {
                view,
                comment,
                range,
            } => {
                let binding = state
                    .editor_bindings
                    .values()
                    .find(|binding| binding.view() == view)
                    .ok_or_else(|| "comment navigation targets an unmounted view".to_owned())?;
                let adapter = state
                    .project
                    .editor_adapter()
                    .ok_or_else(|| "project editor adapter is unavailable".to_owned())?;
                let revision = adapter
                    .revision(binding.session())
                    .map_err(|error| error.to_string())?;
                adapter
                    .execute(
                        binding.session(),
                        EditorCommandOrigin::new(view),
                        AdapterEditorCommand::new(
                            revision,
                            EditorCommandKind::SetSelection { selection: range },
                        ),
                    )
                    .map_err(|error| error.to_string())?;
                adapter
                    .set_active_comment_decoration(binding.session(), view, comment)
                    .map_err(|error| error.to_string())?;
                binding.refresh().map_err(|error| error.to_string())?;
                Ok(None)
            }
            EditorRuntimeIntent::ShowSpellingMenu(menu) => {
                if state.pending_spelling_menu.is_none() {
                    return Err("spelling menu has no live word anchor".to_owned());
                }
                state.spelling_menu = Some(menu);
                state.suppress_next_context_menu_dismissal = true;
                Ok(None)
            }
            EditorRuntimeIntent::RestoreFocus { view } => {
                let binding = state
                    .editor_bindings
                    .values()
                    .find(|binding| binding.view() == view)
                    .ok_or_else(|| "focus restoration targets an unmounted view".to_owned())?;
                binding
                    .update(parchmint_editor_iced::MountedEditorMessage::Focus(
                        0_u64.into(),
                    ))
                    .map_err(|error| error.to_string())?;
                Ok(None)
            }
        }
    }

    fn apply_editor_command(
        state: &mut NativeProjectState,
        view: parchmint_editor_api::ViewId,
        command: crate::EditorCommand,
    ) -> Result<Option<(parchmint_editor_api::SharedEditorSession, EditorRevision)>, String> {
        let binding = state
            .editor_bindings
            .values()
            .find(|binding| binding.view() == view)
            .ok_or_else(|| "editor command targets an unmounted view".to_owned())?;
        let session = binding.session();
        let adapter = state
            .project
            .editor_adapter()
            .ok_or_else(|| "project editor adapter is unavailable".to_owned())?;
        let before = adapter
            .revision(session.clone())
            .map_err(|error| error.to_string())?;
        let selection = adapter
            .selection(session.clone(), view)
            .map_err(|error| error.to_string())?;

        let execute = |kind| {
            let revision = adapter
                .revision(session.clone())
                .map_err(|error| error.to_string())?;
            adapter
                .execute(
                    session.clone(),
                    EditorCommandOrigin::new(view),
                    AdapterEditorCommand::new(revision, kind),
                )
                .map_err(|error| error.to_string())
        };

        match command {
            crate::EditorCommand::ApplyParagraphStyle(style) => {
                let style = state
                    .project
                    .project_ui
                    .as_ref()
                    .and_then(|project| {
                        project
                            .snapshot
                            .project
                            .styles
                            .iter()
                            .find(|definition| definition.display_name == style)
                    })
                    .map(|definition| definition.id)
                    .ok_or_else(|| format!("unknown paragraph style {style}"))?;
                execute(EditorCommandKind::ApplyParagraphStyle {
                    range: selection,
                    style,
                })?;
            }
            crate::EditorCommand::ToggleBold => {
                execute(EditorCommandKind::ToggleInlineMark {
                    range: selection,
                    mark: InlineMarkKind::Bold,
                })?;
            }
            crate::EditorCommand::ToggleItalic => {
                execute(EditorCommandKind::ToggleInlineMark {
                    range: selection,
                    mark: InlineMarkKind::Italic,
                })?;
            }
            crate::EditorCommand::ToggleUnderline => {
                execute(EditorCommandKind::ToggleInlineMark {
                    range: selection,
                    mark: InlineMarkKind::Underline,
                })?;
            }
            crate::EditorCommand::ToggleStrikethrough => {
                execute(EditorCommandKind::ToggleInlineMark {
                    range: selection,
                    mark: InlineMarkKind::Strikethrough,
                })?;
            }
            crate::EditorCommand::ToggleSmallCaps => {
                execute(EditorCommandKind::ToggleInlineMark {
                    range: selection,
                    mark: InlineMarkKind::SmallCaps,
                })?;
            }
            crate::EditorCommand::ToggleSuperscript => {
                execute(EditorCommandKind::ToggleInlineMark {
                    range: selection,
                    mark: InlineMarkKind::Superscript,
                })?;
            }
            crate::EditorCommand::ToggleSubscript => {
                execute(EditorCommandKind::ToggleInlineMark {
                    range: selection,
                    mark: InlineMarkKind::Subscript,
                })?;
            }
            crate::EditorCommand::SetLink { target } => {
                execute(set_link_command(selection, target))?;
            }
            crate::EditorCommand::ToggleBulletedList => {
                execute(EditorCommandKind::ToggleBlockFormat {
                    range: selection,
                    format: BlockFormatKind::BulletedList,
                })?;
            }
            crate::EditorCommand::ToggleNumberedList => {
                execute(EditorCommandKind::ToggleBlockFormat {
                    range: selection,
                    format: BlockFormatKind::NumberedList,
                })?;
            }
            crate::EditorCommand::ToggleBlockQuote => {
                execute(EditorCommandKind::ToggleBlockFormat {
                    range: selection,
                    format: BlockFormatKind::BlockQuote,
                })?;
            }
            crate::EditorCommand::InsertSceneBreak => {
                execute(EditorCommandKind::InsertAtomicBlock {
                    selection,
                    kind: AtomicBlockKind::SceneBreak,
                })?;
            }
            crate::EditorCommand::InsertPageBreak => {
                execute(EditorCommandKind::InsertAtomicBlock {
                    selection,
                    kind: AtomicBlockKind::PageBreak,
                })?;
            }
            crate::EditorCommand::CreateComment {
                body,
                document_level,
            } => {
                let document = state
                    .mounted_documents
                    .iter()
                    .find_map(|(pane, document)| {
                        state
                            .editor_bindings
                            .get(pane)
                            .is_some_and(|candidate| candidate.view() == view)
                            .then_some(*document)
                    })
                    .ok_or_else(|| "comment target document is unavailable".to_owned())?;
                let thread = unique_comment_id(state, document, before, 0, &[]);
                let message = unique_comment_id(state, document, before, 1, &[thread]);
                let anchor = if document_level {
                    CanonicalCommentAnchor::Document {
                        unknown_fields: BTreeMap::new(),
                    }
                } else {
                    CanonicalCommentAnchor::Text {
                        block: BlockId::from_bytes(*document.as_bytes()),
                        range: selection,
                        quote: String::new(),
                        context_before: String::new(),
                        context_after: String::new(),
                        orphaned: false,
                        unknown_fields: BTreeMap::new(),
                    }
                };
                execute(EditorCommandKind::CreateComment {
                    comment: CanonicalComment {
                        id: thread,
                        messages: vec![CanonicalCommentMessage {
                            id: message,
                            body,
                            unknown_fields: BTreeMap::new(),
                        }],
                        resolved: false,
                        anchor,
                        unknown_fields: BTreeMap::new(),
                    },
                })?;
            }
            crate::EditorCommand::ReplyToComment { thread_id, body } => {
                let thread = parse_comment_id(&thread_id)?;
                let document = state
                    .mounted_documents
                    .iter()
                    .find_map(|(pane, document)| {
                        state
                            .editor_bindings
                            .get(pane)
                            .is_some_and(|candidate| candidate.view() == view)
                            .then_some(*document)
                    })
                    .ok_or_else(|| "comment target document is unavailable".to_owned())?;
                execute(EditorCommandKind::ReplyToComment {
                    thread,
                    message: CanonicalCommentMessage {
                        id: unique_comment_id(state, document, before, 2, &[thread]),
                        body,
                        unknown_fields: BTreeMap::new(),
                    },
                })?;
            }
            crate::EditorCommand::SetCommentResolved {
                thread_id,
                resolved,
            } => execute(EditorCommandKind::SetCommentResolved {
                thread: parse_comment_id(&thread_id)?,
                resolved,
            })?,
            crate::EditorCommand::DeleteCommentThread { thread_id } => {
                execute(EditorCommandKind::DeleteCommentThread {
                    thread: parse_comment_id(&thread_id)?,
                })?
            }
            crate::EditorCommand::DeleteCommentMessage {
                thread_id,
                message_id,
            } => execute(EditorCommandKind::DeleteCommentMessage {
                thread: parse_comment_id(&thread_id)?,
                message: parse_comment_id(&message_id)?,
            })?,
            crate::EditorCommand::EditCommentMessage {
                thread_id,
                message_id,
                body,
            } => execute(EditorCommandKind::EditCommentMessage {
                thread: parse_comment_id(&thread_id)?,
                message: parse_comment_id(&message_id)?,
                body,
            })?,
            crate::EditorCommand::ReattachComment { thread_id } => {
                execute(EditorCommandKind::ReattachComment {
                    thread: parse_comment_id(&thread_id)?,
                    range: selection,
                })?
            }
            crate::EditorCommand::ConvertCommentToDocument { thread_id } => {
                execute(EditorCommandKind::ConvertCommentToDocument {
                    thread: parse_comment_id(&thread_id)?,
                })?
            }
            crate::EditorCommand::Undo => execute(EditorCommandKind::Undo)?,
            crate::EditorCommand::Redo => execute(EditorCommandKind::Redo)?,
            crate::EditorCommand::NavigateFindMatch { range } => {
                execute(EditorCommandKind::SetSelection {
                    selection: EditorSelection::new(range.start().into(), range.end().into()),
                })?;
            }
            crate::EditorCommand::ReplaceActiveFindMatch { replacement }
            | crate::EditorCommand::ReplaceSpelling { replacement, .. } => {
                execute(EditorCommandKind::ReplaceRange {
                    range: selection,
                    text: replacement,
                })?
            }
            crate::EditorCommand::ReplaceAllFindMatches { replacement } => {
                let mut matches = state
                    .workspace
                    .as_ref()
                    .map(|workspace| workspace.editor().local_search(view).matches().to_vec())
                    .unwrap_or_default();
                matches.reverse();
                for range in matches {
                    execute(EditorCommandKind::ReplaceRange {
                        range: EditorSelection::new(range.start().into(), range.end().into()),
                        text: replacement.clone(),
                    })?;
                }
            }
        }
        binding.refresh().map_err(|error| error.to_string())?;
        let revision = adapter
            .revision(session.clone())
            .map_err(|error| error.to_string())?;
        Ok((revision != before).then_some((session, revision)))
    }

    fn project_state_sessions(
        state: &NativeProjectState,
    ) -> Result<Vec<SharedEditorSession>, String> {
        Ok(deduplicated_editor_sessions(
            state
                .editor_bindings
                .values()
                .map(MountedEditorBinding::session),
            state.retained_editor_sessions.values().cloned(),
        ))
    }

    fn projection_plans(state: &NativeProjectState) -> Result<Vec<EditorProjectionPlan>, String> {
        let sessions = Self::project_state_sessions(state)?;
        if sessions.is_empty() {
            return Ok(Vec::new());
        }
        let adapter = state
            .project
            .editor_adapter()
            .ok_or_else(|| "project editor adapter is unavailable".to_owned())?;
        editor_projection_plans(adapter, sessions, &state.autosave.projected_sessions)
    }

    fn persist_projection(
        ports: &ProjectUiPorts,
        adapter: &EditorIcedAdapter,
        plan: &EditorProjectionPlan,
    ) -> Result<(), String> {
        let projection =
            iced::futures::executor::block_on(adapter.project(plan.session.clone(), plan.revision))
                .map_err(|error| error.to_string())?;
        let access = ports.access().map_err(|error| error.to_string())?;
        access
            .persistence(|persistence| persistence.persist_editor_projection(projection))
            .map_err(|error| error.to_string())?
            .map(|_| ())
            .map_err(|error| error.to_string())
    }

    fn autosave_task(
        window: window::Id,
        ports: ProjectUiPorts,
        adapter: Arc<EditorIcedAdapter>,
        plans: Vec<EditorProjectionPlan>,
        ticket: AutosaveTicket,
        kind: ProjectSaveKind,
    ) -> Task<Message> {
        Task::perform(
            Self::run_blocking_operation("autosave project", move || {
                Ok(run_projection_sequence(
                    &plans,
                    |plan| Self::persist_projection(&ports, adapter.as_ref(), plan),
                    || {
                        let access = ports.access().map_err(|error| error.to_string())?;
                        let (handle, _) = access
                            .persistence(|persistence| persistence.request_save(kind))
                            .map_err(|error| error.to_string())?
                            .map_err(|error| error.to_string())?;
                        let saved = access
                            .persistence(|persistence| persistence.await_save(handle))
                            .map_err(|error| error.to_string())?
                            .map_err(|error| error.to_string())?;
                        let snapshot = access
                            .snapshot(|query| query.snapshot())
                            .map_err(|error| error.to_string())?
                            .map_err(|error| error.to_string())?;
                        Ok(AutosaveCompletion {
                            saved_revision: saved.written.project_revision.value(),
                            snapshot,
                        })
                    },
                ))
            }),
            move |result| Message::EditorProjectionPersisted {
                window,
                ticket: ticket.clone(),
                result,
            },
        )
    }

    fn accept_persistence_snapshot(state: &mut NativeProjectState, snapshot: ProjectSnapshot) {
        let is_current = state
            .project
            .project_ui
            .as_ref()
            .is_none_or(|project| snapshot_covers(&snapshot, project.snapshot.as_ref()));
        if !is_current {
            return;
        }
        let snapshot = Arc::new(snapshot);
        if let Some(project_ui) = state.project.project_ui.as_mut() {
            project_ui.snapshot = Arc::clone(&snapshot);
            state.effect_executor = state
                .effect_executor
                .as_ref()
                .map(|executor| executor.refreshed(Arc::clone(&snapshot)))
                .or_else(|| {
                    Some(NativeProjectEffectExecutor::new(
                        project_ui.ports.clone(),
                        Arc::clone(&snapshot),
                    ))
                });
        }
        // The workspace owns live editor and Synopsis drafts. Persistence
        // refreshes the authoritative service snapshot only; reconciling the
        // asynchronous result here could replace newer user presentation.
    }

    fn save_task(
        window: window::Id,
        ports: ProjectUiPorts,
        kind: ProjectSaveKind,
        purpose: SavePurpose,
    ) -> Task<Message> {
        Task::perform(
            Self::run_blocking_operation("save project", move || {
                let access = ports.access().map_err(|error| error.to_string())?;
                let (handle, _) = access
                    .persistence(|persistence| persistence.request_save(kind))
                    .map_err(|error| error.to_string())?
                    .map_err(|error| error.to_string())?;
                let saved = access
                    .persistence(|persistence| persistence.await_save(handle))
                    .map_err(|error| error.to_string())?
                    .map_err(|error| error.to_string())?;
                Ok(saved.written.project_revision.value())
            }),
            move |result| Message::SaveFinished {
                window,
                purpose: purpose.clone(),
                result,
            },
        )
    }

    fn workspace_load_task(
        window: window::Id,
        ports: ProjectUiPorts,
        project: parchmint_domain::ProjectId,
    ) -> Task<Message> {
        Task::perform(
            async move {
                let access = ports.access().map_err(|error| error.to_string())?;
                access
                    .workspace_state_service()
                    .map_err(|error| error.to_string())?
                    .load(ProjectIdentity::new(project))
                    .await
                    .map_err(|error| error.to_string())
            },
            move |result| Message::WorkspaceLoaded { window, result },
        )
    }

    fn workspace_persist_task(_window: window::Id, state: &NativeProjectState) -> Task<Message> {
        let Some(project_ui) = state.project.project_ui.as_ref() else {
            return Task::none();
        };
        let Some(workspace) = state.workspace.as_ref() else {
            return Task::none();
        };
        let ports = project_ui.ports.clone();
        let project = project_ui.snapshot.project.id;
        let snapshot =
            workspace.workspace_snapshot(state.shell.layout(), state.shell.destination());
        Task::perform(
            async move {
                let access = ports.access().map_err(|error| error.to_string())?;
                access
                    .workspace_state_service()
                    .map_err(|error| error.to_string())?
                    .save(ProjectIdentity::new(project), &snapshot)
                    .await
                    .map(|_| ())
                    .map_err(|error| error.to_string())
            },
            |result| Message::WorkspacePersisted { result },
        )
    }

    /// Captures the visible mounted block and runs the offline service on a
    /// worker. The completion carries the exact editor revision and a locally
    /// monotonic generation so an old result cannot repaint a newer document.
    fn spellcheck_task(
        window: window::Id,
        state: &mut NativeProjectState,
        view: ViewId,
    ) -> Result<Task<Message>, String> {
        let binding = state
            .editor_bindings
            .values()
            .find(|binding| binding.view() == view)
            .ok_or_else(|| "spellcheck targets an unmounted editor".to_owned())?;
        let adapter = state
            .project
            .editor_adapter()
            .ok_or_else(|| "project editor adapter is unavailable".to_owned())?;
        let session = binding.session();
        let revision = adapter
            .revision(session.clone())
            .map_err(|error| error.to_string())?;
        let block = adapter
            .primary_visible_block(session.clone())
            .map_err(|error| error.to_string())?;
        let document_id = *state
            .mounted_documents
            .iter()
            .find_map(|(pane, document)| {
                state
                    .editor_bindings
                    .get(pane)
                    .is_some_and(|mounted| mounted.view() == view)
                    .then_some(document)
            })
            .ok_or_else(|| "spellcheck mounted document is unavailable".to_owned())?;
        state.next_spellcheck_generation = state.next_spellcheck_generation.saturating_add(1);
        let generation = state.next_spellcheck_generation;
        state.spellcheck_generation.insert(view, generation);
        let project_dictionary = state
            .project
            .project_ui
            .as_ref()
            .map(|project| DictionaryRevision::from(project.snapshot.project.revision.value()))
            .unwrap_or_default();
        let project_id = state
            .project
            .project_ui
            .as_ref()
            .map(|project| project.snapshot.project.id)
            .ok_or_else(|| "project spellcheck identity is unavailable".to_owned())?;
        let request = SpellcheckRequest {
            language: LanguageId::EnUs,
            document_id,
            document_revision: revision,
            blocks: vec![RevisionedTextRange {
                block_id: block.block(),
                range: EditorSelection::new(
                    block.document_start(),
                    parchmint_editor_api::DocumentPosition::from(
                        block.document_start().value() + block.text().chars().count() as u64,
                    ),
                ),
                text: block.text().to_owned(),
            }],
            project_dictionary,
            global_dictionary: DictionaryRevision::default(),
            generation: SpellcheckGeneration::from(generation),
            priority: SpellcheckPriority::Visible,
        };
        let ticket = NativeSpellcheckTicket {
            view,
            editor_session: session,
            document_id,
            revision,
            generation,
            request: request.clone(),
        };
        let ports = state
            .project
            .ports()
            .cloned()
            .ok_or_else(|| "project spellcheck port is unavailable".to_owned())?;
        Ok(Task::perform(
            Self::run_blocking_operation("spellcheck visible text", move || {
                let access = ports.access().map_err(|error| error.to_string())?;
                let project_reload = access
                    .spellcheck(|spellcheck| {
                        spellcheck.reload_project_dictionary(project_id, request.project_dictionary)
                    })
                    .map_err(|error| error.to_string())?;
                iced::futures::executor::block_on(project_reload);
                let global_reload = access
                    .spellcheck(|spellcheck| {
                        spellcheck.reload_global_dictionary(request.global_dictionary)
                    })
                    .map_err(|error| error.to_string())?;
                iced::futures::executor::block_on(global_reload);
                let operation = access
                    .spellcheck(|spellcheck| spellcheck.check(request))
                    .map_err(|error| error.to_string())?;
                let mut stream = iced::futures::executor::block_on(operation);
                stream
                    .next()
                    .ok_or_else(|| "spellcheck stopped without a result".to_owned())
            }),
            move |result| Message::SpellcheckFinished {
                window,
                ticket,
                result,
            },
        ))
    }

    fn finish_spellcheck(
        &mut self,
        window: window::Id,
        ticket: NativeSpellcheckTicket,
        result: Result<SpellcheckResult, String>,
    ) -> Task<Message> {
        let Some(NativeWindow::Project(state)) = self.windows.get_mut(&window) else {
            return Task::none();
        };
        if state.spellcheck_generation.get(&ticket.view) != Some(&ticket.generation) {
            return Task::none();
        }
        let Some(binding) = state
            .editor_bindings
            .values()
            .find(|binding| binding.view() == ticket.view)
        else {
            return Task::none();
        };
        if binding.session() != ticket.editor_session
            || state
                .project
                .editor_adapter()
                .and_then(|adapter| adapter.revision(binding.session()).ok())
                != Some(ticket.revision)
        {
            return Task::none();
        }
        if state.mounted_documents.iter().find_map(|(pane, document)| {
            state
                .editor_bindings
                .get(pane)
                .is_some_and(|mounted| mounted.view() == ticket.view)
                .then_some(*document)
        }) != Some(ticket.document_id)
        {
            return Task::none();
        }
        let result = match result {
            Ok(result) if ticket.request.accepts(&result) => result,
            Ok(_) => return Task::none(),
            Err(error) => {
                self.status = Some(error);
                return Task::none();
            }
        };
        let mut issues = result
            .issues
            .into_iter()
            .map(|issue| {
                let mut suggestions = issue.suggestions;
                suggestions.sort_by_key(|suggestion| suggestion.rank);
                NativeSpellingIssue {
                    word: issue.word,
                    range: issue.range,
                    suggestions: suggestions
                        .into_iter()
                        .map(|suggestion| suggestion.word)
                        .collect(),
                }
            })
            .collect::<Vec<_>>();
        issues.sort_by_key(|issue| issue.range.start());
        let decorations: Vec<SpellingDecoration> = issues
            .iter()
            .map(|issue| {
                SpellingDecoration::new(
                    issue.word.clone(),
                    crate::FindMatch::new(issue.range.start().value(), issue.range.end().value()),
                )
            })
            .collect();
        // Spellcheck ranges belong to the shared document session, while the
        // adapter retains decorations per visible view. Mirror one accepted
        // result into every pane showing that document so the companion never
        // displays stale or unrelated underlines.
        let shared_views = state
            .editor_bindings
            .values()
            .filter(|binding| binding.session() == ticket.editor_session)
            .map(MountedEditorBinding::view)
            .collect::<Vec<_>>();
        let Some(workspace) = state.workspace.as_mut() else {
            return Task::none();
        };
        let mut effects = Vec::new();
        for view in shared_views {
            state.spelling_issues.insert(view, issues.clone());
            effects.extend(workspace.editor_mut().update(
                crate::EditorMessage::SetSpellingDecorations {
                    view,
                    decorations: decorations.clone(),
                },
            ));
        }
        self.status = None;
        Self::editor_effect_tasks(
            window,
            state.effect_executor.clone(),
            &state.project_mutations,
            &mut state.opaque_mutations,
            effects,
        )
    }

    async fn run_service_job<T: Send + 'static>(job: BlockingServiceJob<T>) -> Result<T, String> {
        let (sender, receiver) = iced::futures::channel::oneshot::channel();
        std::thread::Builder::new()
            .name(format!("parchmint-{}", job.operation().replace(' ', "-")))
            .spawn(move || {
                let _ = sender.send(job.run().map_err(|error| error.to_string()));
            })
            .map_err(|error| error.to_string())?;
        receiver
            .await
            .map_err(|_| "project service worker stopped without a result".to_owned())?
    }

    async fn run_blocking_operation<T, F>(
        operation_name: &'static str,
        operation: F,
    ) -> Result<T, String>
    where
        T: Send + 'static,
        F: FnOnce() -> Result<T, String> + Send + 'static,
    {
        let (sender, receiver) = iced::futures::channel::oneshot::channel();
        std::thread::Builder::new()
            .name(format!("parchmint-{}", operation_name.replace(' ', "-")))
            .spawn(move || {
                let _ = sender.send(operation());
            })
            .map_err(|error| error.to_string())?;
        receiver
            .await
            .map_err(|_| format!("{operation_name} worker stopped without a result"))?
    }

    fn export_task(
        window: window::Id,
        ticket: ProjectTaskTicket,
        ports: ProjectUiPorts,
        selection: parchmint_platform_api::UntrustedPathSelection,
        options: ExportRunOptions,
    ) -> Task<Message> {
        let stream = iced::stream::channel(16, async move |mut output| {
            let (worker_sender, mut worker_events) = futures_mpsc::unbounded();
            let progress: Arc<dyn ExportProgressSink> = Arc::new(NativeExportProgressSink {
                sender: worker_sender.clone(),
            });
            let operation =
                match ports
                    .access()
                    .map_err(|error| error.to_string())
                    .and_then(|access| {
                        access
                            .export_target(|export| export.begin_export(progress))
                            .map_err(|error| error.to_string())?
                            .map_err(|error| error.to_string())
                    }) {
                    Ok(operation) => operation,
                    Err(error) => {
                        let _ = output
                            .send(Message::ExportFinished {
                                window,
                                ticket,
                                operation: None,
                                result: Err(error),
                            })
                            .await;
                        return;
                    }
                };
            if output
                .send(Message::ExportOperationStarted {
                    window,
                    ticket: ticket.clone(),
                    operation,
                })
                .await
                .is_err()
            {
                return;
            }

            let worker_ports = ports.clone();
            let terminal_sender = worker_sender.clone();
            let spawn = std::thread::Builder::new()
                .name("parchmint-export-project".into())
                .spawn(move || {
                    let result = worker_ports
                        .access()
                        .map_err(|error| error.to_string())
                        .and_then(|access| {
                            access
                                .export_target(|export| {
                                    export.export_to_path(operation, selection, options)
                                })
                                .map_err(|error| error.to_string())?
                                .map_err(|error| error.to_string())
                        });
                    let _ = terminal_sender.unbounded_send(ExportWorkerEvent::Finished(result));
                });
            if let Err(error) = spawn {
                let _ = worker_sender
                    .unbounded_send(ExportWorkerEvent::Finished(Err(error.to_string())));
            }
            drop(worker_sender);

            while let Some(event) = worker_events.next().await {
                let terminal = matches!(event, ExportWorkerEvent::Finished(_));
                let message = match event {
                    ExportWorkerEvent::Progress(progress) => Message::ExportProgressed {
                        window,
                        ticket: ticket.clone(),
                        operation,
                        progress,
                    },
                    ExportWorkerEvent::Finished(result) => Message::ExportFinished {
                        window,
                        ticket: ticket.clone(),
                        operation: Some(operation),
                        result,
                    },
                };
                if output.send(message).await.is_err() || terminal {
                    break;
                }
            }
        });
        Task::run(stream, |message| message)
    }

    async fn run_search(start: SearchStart) -> Result<Vec<SearchBatchResult>, String> {
        let (sender, receiver) = iced::futures::channel::oneshot::channel();
        std::thread::Builder::new()
            .name(format!("parchmint-search-{}", start.generation))
            .spawn(move || {
                let result = start
                    .job
                    .run()
                    .and_then(|_| start.batches.into_iter().collect())
                    .map_err(|error| error.to_string());
                let _ = sender.send(result);
            })
            .map_err(|error| error.to_string())?;
        receiver
            .await
            .map_err(|_| "search worker stopped without a result".to_owned())?
    }

    fn autosave_tick(&mut self, now: Instant) -> Task<Message> {
        let mut tasks = Vec::new();
        for (window, native) in &mut self.windows {
            let NativeWindow::Project(state) = native else {
                continue;
            };
            let due_spellchecks = state
                .pending_spellchecks
                .iter()
                .filter_map(|(view, due)| (*due <= now).then_some(*view))
                .collect::<Vec<_>>();
            for view in due_spellchecks {
                state.pending_spellchecks.remove(&view);
                if let Ok(task) = Self::spellcheck_task(*window, state, view) {
                    tasks.push(task);
                }
            }
            if !state.autosave.should_save(now)
                || state.project_mutations.blocks_close()
                || state.opaque_mutations.blocks_close()
            {
                continue;
            }
            if state.workspace.is_none() {
                continue;
            }
            let Some(ports) = state.project.ports().cloned() else {
                continue;
            };
            let Some(adapter) = state.project.editor_adapter().cloned() else {
                continue;
            };
            let plans = match Self::projection_plans(state) {
                Ok(plans) => plans,
                Err(error) => {
                    self.status = Some(error);
                    continue;
                }
            };
            let through_revision = state
                .workspace
                .as_ref()
                .expect("workspace presence was checked above")
                .project_revision();
            let ticket = AutosaveTicket {
                dirty_sessions: state.autosave.dirty_sessions.clone(),
            };
            state.autosave.save_in_flight = true;
            state
                .workspace
                .as_mut()
                .expect("workspace presence was checked above")
                .update(ProjectMessage::StartSave(through_revision));
            let window = *window;
            tasks.push(Self::autosave_task(
                window,
                ports,
                adapter,
                plans,
                ticket,
                ProjectSaveKind::Autosave,
            ));
        }
        Task::batch(tasks)
    }

    fn start_projection_save(
        &mut self,
        window: window::Id,
        kind: ProjectSaveKind,
    ) -> Task<Message> {
        let Some(NativeWindow::Project(state)) = self.windows.get_mut(&window) else {
            return Task::none();
        };
        if state.autosave.save_in_flight
            || state.project_mutations.blocks_close()
            || state.opaque_mutations.blocks_close()
        {
            if kind == ProjectSaveKind::Explicit {
                state.autosave.explicit_save_waiting = true;
            }
            return Task::none();
        }
        let Some(ports) = state.project.ports().cloned() else {
            self.status = Some("This project session has no persistence port.".into());
            return Task::none();
        };
        let Some(adapter) = state.project.editor_adapter().cloned() else {
            self.status = Some("project editor adapter is unavailable".into());
            return Task::none();
        };
        let plans = match Self::projection_plans(state) {
            Ok(plans) => plans,
            Err(error) => {
                self.status = Some(error);
                return Task::none();
            }
        };
        if !state.autosave.requires_projection_save(!plans.is_empty()) {
            state.autosave.explicit_save_waiting = false;
            return Task::none();
        }
        let ticket = AutosaveTicket {
            dirty_sessions: state.autosave.dirty_sessions.clone(),
        };
        state.autosave.save_in_flight = true;
        state.autosave.explicit_save_waiting = false;
        let through_revision = state
            .workspace
            .as_ref()
            .map_or(0, |workspace| workspace.project_revision());
        if let Some(workspace) = state.workspace.as_mut() {
            workspace.update(ProjectMessage::StartSave(through_revision));
        }
        Self::autosave_task(window, ports, adapter, plans, ticket, kind)
    }

    fn open_launcher_window(&mut self) -> Task<Message> {
        let capture_request = self.capture.as_ref().and_then(|capture| {
            matches!(capture.request.target, NativeCaptureTarget::Launcher)
                .then(|| capture.request.clone())
        });
        let (id, task) = window::open(capture_request.as_ref().map_or_else(
            || window_settings((900.0, 620.0), (720, 480)),
            capture_window_settings,
        ));
        self.callbacks.project_window_created(LAUNCHER_CAPABILITY);
        self.windows.insert(id, NativeWindow::Launcher);
        if capture_request.is_some() {
            self.capture
                .as_mut()
                .expect("capture state was checked above")
                .window = Some(id);
        }
        task.map(Message::WindowOpened)
    }

    fn mount_initial_editor(
        &mut self,
        project: &NativeProjectWindow,
        workspace: &ProjectWorkspace,
    ) -> (
        EditorHostSlots,
        BTreeMap<EditorPane, MountedEditorBinding>,
        BTreeMap<EditorPane, parchmint_domain::DocumentId>,
    ) {
        let mut slots = EditorHostSlots::default();
        let mut bindings = BTreeMap::new();
        let mut mounted_documents = BTreeMap::new();
        let Some(project_ui) = project.project_ui.as_ref() else {
            return (slots, bindings, mounted_documents);
        };
        let Some(adapter) = project.editor_adapter() else {
            return (slots, bindings, mounted_documents);
        };
        let pane = EditorPane::Primary;
        let state = workspace.editor().pane(pane);
        let Some(active_document) = state.active_document() else {
            return (slots, bindings, mounted_documents);
        };
        let Some(document) =
            project_ui.snapshot.documents.iter().find(|document| {
                stable_id_string(document.document_id.as_bytes()) == active_document
            })
        else {
            self.status = Some("The active document is missing from the project snapshot.".into());
            return (slots, bindings, mounted_documents);
        };
        let viewport = EditorViewport::new(720.0, 520.0)
            .expect("native editor viewport constants must be positive");
        let surface_theme = match self.appearance {
            ResolvedAppearance::Light => EditorSurfaceTheme::light(),
            ResolvedAppearance::Dark => EditorSurfaceTheme::dark(),
        };
        let load = match canonical_load(&project_ui.snapshot, document.document_id) {
            Ok(load) => load,
            Err(error) => {
                self.status = Some(error.to_string());
                return (slots, bindings, mounted_documents);
            }
        };
        let config = MountedEditorBindingConfig::new(
            MountedEditorSession::Open(load),
            project.window,
            state.view(),
            viewport,
            surface_theme,
        );
        match MountedEditorBinding::mount(adapter.as_ref(), config) {
            Ok(binding) => {
                if let Err(error) = binding.restore_scroll(viewport, state.scroll_offset()) {
                    self.status = Some(format!("Could not restore editor scroll: {error}"));
                }
                slots.insert(
                    pane,
                    crate::iced_editor_surface::EditorPaneSlot::mounted(binding.host().clone()),
                );
                mounted_documents.insert(pane, document.document_id);
                bindings.insert(pane, binding);
            }
            Err(error) => {
                self.status = Some(format!("Could not mount the editor: {error}"));
                slots.insert(
                    pane,
                    crate::iced_editor_surface::EditorPaneSlot::state(
                        crate::iced_editor_surface::EditorCenterPaneState::Error(error.to_string()),
                    ),
                );
            }
        }
        (slots, bindings, mounted_documents)
    }

    fn activate_reconciled_project(
        &mut self,
        window: window::Id,
        recovered_document: Option<parchmint_domain::DocumentId>,
    ) -> Task<Message> {
        let Some(NativeWindow::Project(mut state)) = self.windows.remove(&window) else {
            return Task::none();
        };
        let mut sessions = Vec::new();
        for (_, binding) in std::mem::take(&mut state.editor_bindings) {
            let session = binding.session();
            let _ = binding.detach();
            if !sessions.contains(&session) {
                sessions.push(session);
            }
        }
        for session in state.retained_editor_sessions.values() {
            if !sessions.contains(session) {
                sessions.push(session.clone());
            }
        }
        if let Some(adapter) = state.project.editor_adapter() {
            for session in sessions {
                iced::futures::executor::block_on(adapter.close(session));
            }
        }
        state.editor_hosts = EditorHostSlots::default();
        state.mounted_documents.clear();
        state.retained_editor_sessions.clear();
        state.autosave.reset_sessions();
        state.spellcheck_generation.clear();
        state.pending_spellchecks.clear();
        state.spelling_issues.clear();
        state.pending_spelling_menu = None;
        state.spelling_menu = None;
        state.recovery_acceptance = None;

        if let (Some(document), Some(project_ui), Some(workspace)) = (
            recovered_document,
            state.project.project_ui.as_ref(),
            state.workspace.as_mut(),
        ) && project_ui
            .snapshot
            .documents
            .iter()
            .any(|candidate| candidate.document_id == document)
        {
            let title = project_ui
                .snapshot
                .project
                .nodes
                .iter()
                .find_map(|(_, node)| match node.kind {
                    parchmint_domain::NodeKind::Document(candidate) if candidate == document => {
                        Some(node.title.clone())
                    }
                    _ => None,
                })
                .unwrap_or_else(|| "Recovered document".to_owned());
            workspace
                .editor_mut()
                .update(crate::EditorMessage::OpenTab {
                    pane: EditorPane::Primary,
                    tab: crate::TabSpec::new(stable_id_string(document.as_bytes()), title),
                });
        }

        if let Some(workspace) = state.workspace.as_deref() {
            let (hosts, bindings, documents) = self.mount_initial_editor(&state.project, workspace);
            state.editor_hosts = hosts;
            state.editor_bindings = bindings;
            state.mounted_documents = documents;
            if let Some(adapter) = state.project.editor_adapter() {
                let mounted = state
                    .editor_bindings
                    .values()
                    .filter_map(|binding| {
                        let session = binding.session();
                        adapter
                            .revision(session.clone())
                            .ok()
                            .map(|revision| (session, revision))
                    })
                    .collect::<Vec<_>>();
                state.autosave.record_projected(mounted);
            }
        }
        if let Some(binding) = state.editor_bindings.get(&EditorPane::Primary) {
            let _ = binding.restore_focus();
            if let Ok(active_style) = binding.active_style()
                && let Some(style_name) = state
                    .project
                    .project_ui
                    .as_ref()
                    .and_then(|project| project.snapshot.project.styles.get(active_style))
                    .map(|style| style.display_name.clone())
                && let Some(workspace) = state.workspace.as_mut()
            {
                workspace
                    .editor_mut()
                    .update(crate::EditorMessage::SetActiveParagraphStyle(style_name));
            }
        }
        self.windows.insert(window, NativeWindow::Project(state));
        let spellcheck_tasks = match self.windows.get_mut(&window) {
            Some(NativeWindow::Project(state)) => state
                .editor_bindings
                .values()
                .map(MountedEditorBinding::view)
                .collect::<Vec<_>>()
                .into_iter()
                .filter_map(|view| Self::spellcheck_task(window, state, view).ok())
                .collect::<Vec<_>>(),
            _ => Vec::new(),
        };
        Task::batch(spellcheck_tasks)
    }

    fn open_project_window(&mut self, project: NativeProjectWindow) -> Task<Message> {
        let capture_request = self.capture.as_ref().and_then(|capture| {
            matches!(capture.request.target, NativeCaptureTarget::Project(_))
                .then(|| capture.request.clone())
        });
        let capture_destination =
            capture_request
                .as_ref()
                .and_then(|capture| match capture.target {
                    NativeCaptureTarget::Project(destination) => Some(destination),
                    NativeCaptureTarget::Launcher => None,
                });
        let (id, task) = window::open(capture_destination.map_or_else(
            || window_settings((1280.0, 720.0), ShellLayout::MIN_WINDOW_SIZE),
            |_| capture_window_settings(capture_request.as_ref().expect("capture request exists")),
        ));
        self.project_windows.insert(project.window, id);
        self.callbacks.project_window_created(project.window);
        let workspace_load = project
            .project_ui
            .as_ref()
            .map_or_else(Task::none, |project| {
                Self::workspace_load_task(id, project.ports.clone(), project.snapshot.project.id)
            });
        let mut workspace = project
            .project_ui
            .as_ref()
            .map(|project| Box::new(ProjectWorkspace::from_snapshot(project.snapshot.as_ref())));
        if let Some(workspace) = workspace.as_mut() {
            workspace.begin_session(
                project.session.generation(),
                project
                    .project_ui
                    .as_ref()
                    .map_or(0, |project| project.snapshot.project.revision.value()),
            );
        }
        let editor_hosts = EditorHostSlots::default();
        let editor_bindings = BTreeMap::new();
        let mounted_documents = BTreeMap::new();
        let retained_editor_sessions = BTreeMap::new();
        let effect_executor = project.project_ui.as_ref().map(|project| {
            NativeProjectEffectExecutor::new(project.ports.clone(), Arc::clone(&project.snapshot))
        });
        let service_feeds = project
            .project_ui
            .as_ref()
            .map(|project| AsyncServiceFeeds::new(project.ports.clone()));
        let mut shell = Shell::new(project.window);
        if let Some(destination) = capture_destination {
            shell.select_destination(destination);
        }
        self.windows.insert(
            id,
            NativeWindow::Project(Box::new(NativeProjectState {
                project: project.clone(),
                shell,
                workspace,
                editor_hosts,
                editor_bindings,
                mounted_documents,
                retained_editor_sessions,
                effect_executor,
                synopsis_commits: SynopsisCommitQueue::default(),
                project_mutations: ProjectMutationState::default(),
                opaque_mutations: OpaqueMutationState::default(),
                service_feeds,
                recovery_acceptance: None,
                active_export: None,
                export_destination: None,
                autosave: AutosaveState::default(),
                next_spellcheck_generation: 0,
                spellcheck_generation: BTreeMap::new(),
                pending_spellchecks: BTreeMap::new(),
                spelling_issues: BTreeMap::new(),
                pending_spelling_menu: None,
                spelling_menu: None,
                suppress_next_context_menu_dismissal: false,
                refresh_spellcheck_view: None,
                modifiers: keyboard::Modifiers::default(),
                resizing: None,
                modal_focus: ModalFocus::Cancel,
            })),
        );
        if capture_destination.is_some() {
            self.capture
                .as_mut()
                .expect("capture state was checked above")
                .window = Some(id);
        }
        let recovery_task = match self.windows.get_mut(&id) {
            Some(NativeWindow::Project(state)) => {
                let session = state.project.session;
                match (state.workspace.as_mut(), state.service_feeds.as_ref()) {
                    (Some(workspace), Some(feeds)) => {
                        let ticket = workspace.begin_recovery_reconciliation();
                        let job = feeds.reconcile_recovery();
                        Task::perform(Self::run_service_job(job), move |result| {
                            Message::RecoveryReconciled {
                                window: id,
                                session,
                                ticket,
                                result,
                            }
                        })
                    }
                    (Some(workspace), None) => {
                        let ticket = workspace.begin_recovery_reconciliation();
                        workspace.accept_completion(ProjectTaskCompletion::for_ticket(
                            ticket,
                            ProjectTaskPayload::Failed(
                                "Recovery reconciliation is unavailable for this project session."
                                    .to_owned(),
                            ),
                        ));
                        Task::none()
                    }
                    _ => Task::none(),
                }
            }
            _ => Task::none(),
        };
        Task::batch([
            task.map(Message::WindowOpened),
            workspace_load,
            recovery_task,
        ])
    }

    fn choose_directory(&mut self, create: bool, capability: WindowCapability) -> Task<Message> {
        if self.opening_project {
            return Task::none();
        }
        self.opening_project = true;
        let callbacks = Arc::clone(&self.callbacks);
        Task::perform(
            Self::run_blocking_operation("choose project directory", move || {
                callbacks.choose_project_directory(
                    capability,
                    if create {
                        "Choose New Project Location"
                    } else {
                        "Open ParchMint Project"
                    },
                )
            }),
            move |result| Message::DirectoryChosen { create, result },
        )
    }

    fn finish_directory_choice(
        &mut self,
        create: bool,
        result: Result<Option<PathBuf>, String>,
    ) -> Task<Message> {
        self.opening_project = false;
        match result {
            Ok(Some(directory)) if create => {
                let title = self.launcher.new_project().title();
                let destination = directory.join(suggested_directory_name(title));
                self.launcher
                    .new_project_mut()
                    .set_destination(destination.display().to_string());
                Task::none()
            }
            Ok(Some(project)) => self.route_project_open(project),
            Ok(None) => Task::none(),
            Err(error) => {
                self.status = Some(error);
                Task::none()
            }
        }
    }

    fn route_recent_project_open(&mut self, project: PathBuf) -> Task<Message> {
        if !project.exists() {
            self.status = Some(format!(
                "The project at {} is no longer available. It may have been moved or deleted.",
                project.display()
            ));
            return Task::none();
        }
        self.route_project_open(project)
    }

    fn route_project_open(&mut self, project: PathBuf) -> Task<Message> {
        if self.opening_project {
            return Task::none();
        }
        self.opening_project = true;
        let callbacks = Arc::clone(&self.callbacks);
        Task::perform(
            Self::run_blocking_operation("open project", move || {
                let result = callbacks.open_project(project.clone());
                Ok((project, result))
            }),
            |result| match result {
                Ok((project, result)) => Message::ProjectOpenFinished { project, result },
                Err(error) => Message::ProjectOpenFinished {
                    project: PathBuf::new(),
                    result: Err(error),
                },
            },
        )
    }

    fn route_project_create(&mut self) -> Task<Message> {
        if self.opening_project {
            return Task::none();
        }
        let draft = self.launcher.new_project();
        let request = NativeNewProjectRequest {
            title: draft.title().trim().to_owned(),
            destination: PathBuf::from(draft.destination().trim()),
            author: draft
                .author()
                .map(str::trim)
                .filter(|author| !author.is_empty())
                .map(str::to_owned),
        };
        if request.title.is_empty() || request.destination.as_os_str().is_empty() {
            self.status = Some("Enter a project title and choose a destination.".to_owned());
            return Task::none();
        }
        let project = request.destination.clone();
        self.opening_project = true;
        let callbacks = Arc::clone(&self.callbacks);
        Task::perform(
            Self::run_blocking_operation("create project", move || {
                Ok((project, callbacks.create_project(request)))
            }),
            |result| match result {
                Ok((project, result)) => Message::ProjectOpenFinished { project, result },
                Err(error) => Message::ProjectOpenFinished {
                    project: PathBuf::new(),
                    result: Err(error),
                },
            },
        )
    }

    fn finish_project_open(
        &mut self,
        project: PathBuf,
        result: Result<NativeProjectOpenResult, String>,
    ) -> Task<Message> {
        match result {
            Ok(NativeProjectOpenResult::Opened(window)) => {
                self.status = None;
                self.creating_project = false;
                self.launcher.add_recent_project(
                    window
                        .project_ui
                        .as_ref()
                        .map(|project| project.snapshot.project.display_title.as_str())
                        .filter(|name| !name.is_empty())
                        .unwrap_or_else(|| {
                            window
                                .project
                                .file_name()
                                .and_then(|name| name.to_str())
                                .unwrap_or("Untitled Project")
                        }),
                    window.project.display().to_string(),
                    "Opened just now",
                );
                let open_project = self.open_project_window(window);
                let close_launcher = self
                    .windows
                    .iter()
                    .find_map(|(id, window)| {
                        matches!(window, NativeWindow::Launcher).then_some(*id)
                    })
                    .map_or_else(Task::none, |id| self.finish_close(id));
                Task::batch([open_project, close_launcher])
            }
            Ok(NativeProjectOpenResult::Focused(capability)) => {
                self.status = None;
                self.project_windows
                    .get(&capability)
                    .copied()
                    .map_or_else(Task::none, window::gain_focus)
            }
            Ok(NativeProjectOpenResult::Locked) => {
                self.status = Some(format!("Project is already open: {}", project.display()));
                Task::none()
            }
            Err(error) => {
                self.status = Some(error);
                Task::none()
            }
        }
    }

    fn close_window(&mut self, id: window::Id) -> Task<Message> {
        let Some(window) = self.windows.get(&id) else {
            return Task::none();
        };
        let NativeWindow::Project(state) = window else {
            return self.finish_close(id);
        };
        if !self.closing_windows.insert(id) {
            return Task::none();
        }
        if state.project_mutations.blocks_close() || state.opaque_mutations.blocks_close() {
            let NativeWindow::Project(state) = self
                .windows
                .get_mut(&id)
                .expect("project window remains live while close waits")
            else {
                unreachable!("project window kind was checked above")
            };
            state.autosave.close_after_save = true;
            let retry = Self::retry_pending_mutations(id, state);
            self.status = Some("Finishing project changes before closing…".to_owned());
            return retry;
        }
        if state.autosave.save_in_flight {
            let NativeWindow::Project(state) = self
                .windows
                .get_mut(&id)
                .expect("project window remains live while close waits")
            else {
                unreachable!("project window kind was checked above")
            };
            state.autosave.close_after_save = true;
            self.status = Some("Finishing the current save before closing…".to_owned());
            return Task::none();
        }
        self.continue_close_window(id)
    }

    fn continue_close_window(&mut self, id: window::Id) -> Task<Message> {
        let Some(NativeWindow::Project(state)) = self.windows.get(&id) else {
            return Task::none();
        };
        if state.project_mutations.blocks_close()
            || state.opaque_mutations.blocks_close()
            || state.autosave.save_in_flight
        {
            let NativeWindow::Project(state) = self
                .windows
                .get_mut(&id)
                .expect("project window remains live while queued close waits")
            else {
                unreachable!("project window kind was checked above")
            };
            state.autosave.close_after_save = true;
            return Task::none();
        }
        let project = state.project.project.clone();
        let is_clean = state.autosave.is_clean();
        let callbacks = Arc::clone(&self.callbacks);
        let persist = Self::workspace_persist_task(id, state);
        let plans = match Self::projection_plans(state) {
            Ok(plans) => plans,
            Err(error) => {
                self.status = None;
                self.closing_windows.remove(&id);
                self.close_failures.insert(state.project.window, error);
                return Task::none();
            }
        };
        let ports = state.project.ports().cloned();
        let adapter = state.project.editor_adapter().cloned();
        let has_plans = !plans.is_empty();
        self.status = Some(if is_clean {
            "Closing project…".to_owned()
        } else {
            "Saving project before closing…".to_owned()
        });
        let close = Task::perform(
            Self::run_blocking_operation("close project", move || {
                Ok(run_projection_sequence(
                    &plans,
                    |plan| {
                        let ports = ports
                            .as_ref()
                            .ok_or_else(|| "project persistence port is unavailable".to_owned())?;
                        let adapter = adapter
                            .as_ref()
                            .ok_or_else(|| "project editor adapter is unavailable".to_owned())?;
                        Self::persist_projection(ports, adapter.as_ref(), plan)
                    },
                    || {
                        let snapshot = ports
                            .as_ref()
                            .map(|ports| {
                                let access = ports.access().map_err(|error| error.to_string())?;
                                access
                                    .snapshot(|query| query.snapshot())
                                    .map_err(|error| error.to_string())?
                                    .map_err(|error| error.to_string())
                            })
                            .transpose()?;
                        let result = if is_clean && !has_plans {
                            callbacks.close_clean_project(project)
                        } else {
                            callbacks.close_project(project)
                        };
                        Ok(CloseCompletion { snapshot, result })
                    },
                ))
            }),
            move |result| Message::ProjectCloseFinished { window: id, result },
        );
        Task::batch([persist, close])
    }

    fn finish_close(&mut self, id: window::Id) -> Task<Message> {
        self.closing_windows.remove(&id);
        let removed = self.windows.remove(&id);
        match removed {
            Some(NativeWindow::Project(mut state)) => {
                Self::teardown_editor_sessions(&mut state);
                self.project_windows.remove(&state.project.window);
                self.close_failures.remove(&state.project.window);
                self.callbacks
                    .project_window_destroyed(state.project.window);
            }
            Some(NativeWindow::Launcher) => {
                self.callbacks.project_window_destroyed(LAUNCHER_CAPABILITY);
            }
            None => {}
        };
        let close = if self.windows.is_empty() {
            Task::batch([window::close(id), iced::exit()])
        } else {
            window::close(id)
        };
        close
    }

    fn teardown_editor_sessions(state: &mut NativeProjectState) {
        let mut sessions = state
            .retained_editor_sessions
            .values()
            .cloned()
            .collect::<BTreeSet<_>>();
        for (_, binding) in std::mem::take(&mut state.editor_bindings) {
            sessions.insert(binding.session());
            let _ = binding.detach();
        }
        if let Some(adapter) = state.project.editor_adapter() {
            for session in sessions {
                iced::futures::executor::block_on(adapter.close(session));
            }
        }
        state.retained_editor_sessions.clear();
        state.mounted_documents.clear();
        state.autosave.reset_sessions();
    }

    fn prune_durable_unmounted_sessions(state: &mut NativeProjectState) {
        let mounted = state
            .editor_bindings
            .values()
            .map(MountedEditorBinding::session)
            .collect::<BTreeSet<_>>();
        let removable = state
            .retained_editor_sessions
            .iter()
            .filter(|(_, session)| {
                !mounted.contains(*session) && !state.autosave.dirty_sessions.contains_key(*session)
            })
            .map(|(document, session)| (*document, session.clone()))
            .collect::<Vec<_>>();
        for (document, session) in removable {
            state.retained_editor_sessions.remove(&document);
            state.autosave.projected_sessions.remove(&session);
            if let Some(adapter) = state.project.editor_adapter() {
                iced::futures::executor::block_on(adapter.close(session));
            }
        }
    }

    fn prune_deleted_document_sessions(state: &mut NativeProjectState, snapshot: &ProjectSnapshot) {
        let live = snapshot
            .project
            .nodes
            .iter()
            .filter_map(|(_, node)| match node.kind {
                parchmint_domain::NodeKind::Document(document) => Some(document),
                _ => None,
            })
            .collect::<BTreeSet<_>>();
        let deleted = state
            .retained_editor_sessions
            .keys()
            .chain(state.mounted_documents.values())
            .filter(|document| !live.contains(document))
            .copied()
            .collect::<BTreeSet<_>>();
        for document in deleted {
            let panes = state
                .mounted_documents
                .iter()
                .filter_map(|(pane, mounted)| (*mounted == document).then_some(*pane))
                .collect::<Vec<_>>();
            let mut sessions = BTreeSet::new();
            for pane in panes {
                state.mounted_documents.remove(&pane);
                if let Some(binding) = state.editor_bindings.remove(&pane) {
                    sessions.insert(binding.session());
                    let _ = binding.detach();
                }
                state.editor_hosts.remove(pane);
            }
            if let Some(session) = state.retained_editor_sessions.remove(&document) {
                sessions.insert(session);
            }
            for session in sessions {
                state.autosave.dirty_sessions.remove(&session);
                state.autosave.projected_sessions.remove(&session);
                if let Some(adapter) = state.project.editor_adapter() {
                    iced::futures::executor::block_on(adapter.close(session));
                }
            }
        }
    }
}

fn restored_document_effect(pane: EditorPane, document: String) -> ProjectEffect {
    match pane {
        EditorPane::Primary => ProjectEffect::OpenDocumentInPrimary(document),
        EditorPane::Companion => ProjectEffect::OpenDocumentInCompanion(document),
    }
}

struct ClipboardMutation {
    editor_session: SharedEditorSession,
    revision: EditorRevision,
    active_style: parchmint_domain::StyleId,
    presentation_error: Option<String>,
    feedback: Option<String>,
}

fn clipboard_target<'a>(
    state: &'a NativeProjectState,
    request: &NativeClipboardRequest,
) -> Result<&'a MountedEditorBinding, String> {
    let binding = state
        .editor_bindings
        .get(&request.pane)
        .ok_or_else(|| "clipboard completion targets an unmounted editor".to_owned())?;
    validate_clipboard_identity(
        state.project.window,
        state.project.session,
        binding,
        request,
    )?;
    Ok(binding)
}

fn validate_clipboard_identity(
    live_window: WindowCapability,
    live_project_session: ProjectSessionCapability,
    binding: &MountedEditorBinding,
    request: &NativeClipboardRequest,
) -> Result<(), String> {
    if live_window != request.capability {
        return Err("clipboard completion targets a stale window capability".to_owned());
    }
    if live_project_session != request.project_session {
        return Err("clipboard completion targets a stale project session".to_owned());
    }
    if binding.view() != request.view || binding.session() != request.editor_session {
        return Err("clipboard completion targets a stale editor session".to_owned());
    }
    Ok(())
}

fn apply_completed_cut(
    adapter: &EditorIcedAdapter,
    binding: &MountedEditorBinding,
    request: &NativeClipboardRequest,
    write_result: Result<(), String>,
) -> Result<Option<ClipboardMutation>, String> {
    if let Err(error) = write_result {
        let _ = binding.restore_focus();
        return Err(error);
    }
    if let Err(error) = adapter.execute(
        request.editor_session.clone(),
        EditorCommandOrigin::new(request.view),
        AdapterEditorCommand::new(
            request.revision,
            EditorCommandKind::DeleteRange {
                range: request.selection,
            },
        ),
    ) {
        let _ = binding.restore_focus();
        return Err(error.to_string());
    }
    let revision = adapter
        .revision(request.editor_session.clone())
        .map_err(|error| error.to_string())?;
    let presentation_error = binding
        .refresh()
        .and_then(|_| binding.restore_focus())
        .err()
        .map(|error| error.to_string());
    Ok(Some(ClipboardMutation {
        editor_session: request.editor_session.clone(),
        revision,
        active_style: adapter
            .active_style(request.editor_session.clone(), request.view)
            .map_err(|error| error.to_string())?,
        presentation_error,
        feedback: None,
    }))
}

fn apply_completed_paste(
    adapter: &EditorIcedAdapter,
    binding: &MountedEditorBinding,
    request: &NativeClipboardRequest,
    source: &UntrustedClipboardContent,
) -> Result<Option<ClipboardMutation>, String> {
    let before = adapter
        .revision(request.editor_session.clone())
        .map_err(|error| error.to_string())?;
    let paste = match request.intent {
        MountedEditorClipboardIntent::Paste => adapter.paste_untrusted_at(
            request.editor_session.clone(),
            request.view,
            request.selection,
            request.revision,
            source,
        ),
        MountedEditorClipboardIntent::PasteWithoutFormatting => adapter.paste_untrusted_plain_at(
            request.editor_session.clone(),
            request.view,
            request.selection,
            request.revision,
            source,
        ),
        _ => return Err("clipboard read completion has the wrong intent".to_owned()),
    }
    .map_err(|error| {
        let _ = binding.restore_focus();
        error.to_string()
    })?;
    let revision = adapter
        .revision(request.editor_session.clone())
        .map_err(|error| error.to_string())?;
    let presentation_error = binding
        .refresh()
        .and_then(|_| binding.restore_focus())
        .err()
        .map(|error| error.to_string());
    if revision == before {
        if let Some(error) = presentation_error {
            return Err(error);
        }
        return Ok(None);
    }
    Ok(Some(ClipboardMutation {
        editor_session: request.editor_session.clone(),
        revision,
        active_style: adapter
            .active_style(request.editor_session.clone(), request.view)
            .map_err(|error| error.to_string())?,
        presentation_error,
        feedback: paste_feedback(paste.omitted_images(), paste.unsafe_content_removed()),
    }))
}

fn paste_feedback(omitted_images: usize, unsafe_content_removed: bool) -> Option<String> {
    match (omitted_images, unsafe_content_removed) {
        (0, false) => None,
        (images, false) => Some(format!(
            "Pasted text; omitted {images} unsupported image{}.",
            if images == 1 { "" } else { "s" }
        )),
        (0, true) => Some("Pasted supported content; unsafe content was removed.".to_owned()),
        (images, true) => Some(format!(
            "Pasted supported content; removed unsafe content and omitted {images} image{}.",
            if images == 1 { "" } else { "s" }
        )),
    }
}

fn set_link_command(selection: EditorSelection, target: Option<String>) -> EditorCommandKind {
    EditorCommandKind::SetLink {
        range: selection,
        target,
    }
}

fn derived_comment_id(
    document: parchmint_domain::DocumentId,
    revision: EditorRevision,
    ordinal: u64,
) -> CommentId {
    let mut bytes = *document.as_bytes();
    for (slot, byte) in bytes[8..]
        .iter_mut()
        .zip(revision.value().saturating_add(1).to_be_bytes())
    {
        *slot ^= byte;
    }
    for (slot, byte) in bytes[..8].iter_mut().zip(ordinal.to_be_bytes()) {
        *slot ^= byte;
    }
    CommentId::from_bytes(bytes)
}

fn unique_comment_id(
    state: &NativeProjectState,
    document: parchmint_domain::DocumentId,
    revision: EditorRevision,
    first_ordinal: u64,
    reserved: &[CommentId],
) -> CommentId {
    let used = state
        .project
        .project_ui
        .as_ref()
        .into_iter()
        .flat_map(|project| &project.snapshot.documents)
        .flat_map(|document| &document.comments)
        .flat_map(|thread| {
            std::iter::once(thread.id).chain(thread.messages.iter().map(|message| message.id))
        })
        .collect::<BTreeSet<_>>();
    let mut ordinal = first_ordinal;
    loop {
        let candidate = derived_comment_id(document, revision, ordinal);
        if !used.contains(&candidate) && !reserved.contains(&candidate) {
            return candidate;
        }
        ordinal = ordinal.saturating_add(1);
    }
}

fn parse_comment_id(value: &str) -> Result<CommentId, String> {
    if value.len() != 32 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("comment ID is invalid".into());
    }
    let mut bytes = [0; 16];
    for (index, byte) in bytes.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&value[index * 2..index * 2 + 2], 16)
            .map_err(|_| "comment ID is invalid".to_owned())?;
    }
    Ok(CommentId::from_bytes(bytes))
}

fn editor_intent_view(intent: &EditorRuntimeIntent) -> Option<ViewId> {
    match intent {
        EditorRuntimeIntent::Command { view, .. } => Some(*view),
        _ => None,
    }
}

fn local_find_matches(
    text: &str,
    query: &str,
    case_sensitive: bool,
    whole_word: bool,
) -> Vec<crate::FindMatch> {
    let haystack = text.chars().collect::<Vec<_>>();
    let needle = query.chars().collect::<Vec<_>>();
    if needle.is_empty() || needle.len() > haystack.len() {
        return Vec::new();
    }
    (0..=haystack.len() - needle.len())
        .filter(|start| {
            haystack[*start..*start + needle.len()]
                .iter()
                .zip(&needle)
                .all(|(left, right)| {
                    if case_sensitive {
                        left == right
                    } else {
                        left.to_lowercase().eq(right.to_lowercase())
                    }
                })
        })
        .filter(|start| {
            if !whole_word {
                return true;
            }
            let before = start.checked_sub(1).and_then(|index| haystack.get(index));
            let after = haystack.get(start + needle.len());
            before.is_none_or(|character| !is_word_character(*character))
                && after.is_none_or(|character| !is_word_character(*character))
        })
        .map(|start| crate::FindMatch::new(start as u64, (start + needle.len()) as u64))
        .collect()
}

fn is_word_character(character: char) -> bool {
    character.is_alphanumeric() || character == '_'
}

fn replacement_selection(
    snapshot: &parchmint_ui_api::ProjectSnapshot,
    results: &[crate::GlobalSearchResult],
    included_match_ids: &[String],
    replacement: &str,
) -> Result<ReplacementSelection, String> {
    let included = included_match_ids.iter().collect::<BTreeSet<_>>();
    if included.is_empty() {
        return Err("replacement requires at least one included match".into());
    }
    let mut ranges = BTreeMap::<String, Vec<(usize, usize)>>::new();
    for match_id in included {
        let result = results
            .iter()
            .find(|result| &result.match_id == match_id)
            .ok_or_else(|| "replacement contains a stale or unknown match".to_owned())?;
        let parts = match_id.split(':').collect::<Vec<_>>();
        if parts.len() != 6 || parts[0] != result.document_id || parts[2] != "Body" {
            return Err(
                "global replacement currently accepts only validated document-body matches".into(),
            );
        }
        let start = parts[3]
            .parse::<usize>()
            .map_err(|_| "replacement match start is malformed".to_owned())?;
        let end = parts[4]
            .parse::<usize>()
            .map_err(|_| "replacement match end is malformed".to_owned())?;
        let revision = parts[5]
            .parse::<u64>()
            .map_err(|_| "replacement match revision is malformed".to_owned())?;
        if revision != result.indexed_revision || start > end {
            return Err("replacement match identity is inconsistent".into());
        }
        ranges
            .entry(result.document_id.clone())
            .or_default()
            .push((start, end));
    }

    let mut edits = Vec::new();
    for (document_id, mut document_ranges) in ranges {
        let source = snapshot
            .documents
            .iter()
            .find(|document| stable_id_string(document.document_id.as_bytes()) == document_id)
            .ok_or_else(|| "replacement document is no longer available".to_owned())?;
        if document_ranges.iter().any(|(start, end)| {
            source.body.get(*start..*end).is_none()
                || source.revision.value()
                    != results
                        .iter()
                        .find(|result| result.document_id == document_id)
                        .map_or(u64::MAX, |result| result.indexed_revision)
        }) {
            return Err("replacement source changed after the search completed".into());
        }
        document_ranges.sort_unstable_by(|left, right| right.cmp(left));
        for pair in document_ranges.windows(2) {
            if pair[1].1 > pair[0].0 {
                return Err("replacement matches overlap".into());
            }
        }
        let mut body = source.body.clone();
        for (start, end) in document_ranges {
            body.replace_range(start..end, replacement);
        }
        edits.push(ReplacementEdit {
            document_id: source.document_id,
            observed_revision: source.revision,
            expected_body: source.body.clone(),
            replacement_body: body,
        });
    }
    Ok(ReplacementSelection {
        label: "Global Replace".to_owned(),
        edits,
    })
}

fn launcher_surface(
    recent_projects: &[RecentProject],
    new_project: &NewProjectDraft,
    creating_project: bool,
    opening_project: bool,
    status: Option<String>,
) -> Element<'static, Message> {
    let mut content = column![launcher_text(
        "ParchMint",
        LAUNCHER_WORDMARK_SIZE,
        LauncherTextKind::Wordmark
    )]
    .spacing(f32::from(LAUNCHER_RHYTHM));
    if let Some(status) = status {
        content = content.push(launcher_text(
            status,
            LAUNCHER_SUBTITLE_SIZE,
            LauncherTextKind::Secondary,
        ));
    }
    if creating_project {
        let choose_destination = if opening_project {
            launcher_button("Choosing Location…", ButtonKind::Secondary)
                .height(36)
                .width(184)
        } else {
            launcher_button("Choose Destination…", ButtonKind::Secondary)
                .height(36)
                .width(184)
                .on_press(Message::ChooseNewProjectDestination)
        };
        let create = if opening_project {
            launcher_button("Creating Project…", ButtonKind::Primary)
                .height(36)
                .width(160)
        } else {
            launcher_button("Create and Open", ButtonKind::Primary)
                .height(36)
                .width(160)
                .on_press(Message::CreateProject)
        };
        content = content.push(column![
            launcher_text(
                "Create project",
                LAUNCHER_TITLE_SIZE,
                LauncherTextKind::Primary
            ),
            launcher_text(
                "Set a project name, location, and optional author.",
                LAUNCHER_SUBTITLE_SIZE,
                LauncherTextKind::Secondary,
            ),
            column![
                text_input("Project title", new_project.title())
                    .on_input(Message::NewProjectTitleChanged)
                    .padding(10)
                    .width(520),
                text_input("Project destination", new_project.destination())
                    .on_input(Message::NewProjectDestinationChanged)
                    .padding(10)
                    .width(520),
                choose_destination,
                text_input(
                    "Author (optional)",
                    new_project.author().unwrap_or_default()
                )
                .on_input(Message::NewProjectAuthorChanged)
                .padding(10)
                .width(520),
                row![
                    launcher_button("Cancel", ButtonKind::Secondary)
                        .height(36)
                        .width(100)
                        .on_press_maybe((!opening_project).then_some(Message::CancelNewProject)),
                    create,
                ]
                .spacing(12),
            ]
            .spacing(10),
        ]);
    } else {
        content = content.push(launcher_text(
            "Recent projects",
            LAUNCHER_TITLE_SIZE,
            LauncherTextKind::Primary,
        ));
        content = content.push(launcher_text(
            "Open a recent project, create a new one, or choose another project folder.",
            LAUNCHER_SUBTITLE_SIZE,
            LauncherTextKind::Secondary,
        ));
        content = content.push(
            row![
                launcher_button("Create Project", ButtonKind::Primary)
                    .width(144)
                    .height(36)
                    .on_press(Message::ShowNewProject),
                if opening_project {
                    launcher_button("Opening Project…", ButtonKind::Secondary)
                        .width(128)
                        .height(36)
                } else {
                    launcher_button("Open Project", ButtonKind::Secondary)
                        .width(128)
                        .height(36)
                        .on_press(Message::ChooseOpenProject)
                }
            ]
            .spacing(12)
            .height(f32::from(LAUNCHER_ACTION_ROW_HEIGHT)),
        );
        if recent_projects.is_empty() {
            content = content.push(launcher_text(
                "No recent projects yet.",
                LAUNCHER_SUBTITLE_SIZE,
                LauncherTextKind::Secondary,
            ));
        } else {
            let cards = recent_projects.iter().fold(
                column!().spacing(f32::from(LAUNCHER_PROJECT_CARD_GAP)),
                |cards, project| cards.push(launcher_project_card(project)),
            );
            content = content.push(cards);
        }
    }
    container(content)
        .padding(iced::Padding {
            top: f32::from(LAUNCHER_INSET),
            right: f32::from(LAUNCHER_INSET),
            bottom: f32::from(LAUNCHER_INSET),
            left: f32::from(LAUNCHER_INSET),
        })
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

#[derive(Clone, Copy)]
enum LauncherTextKind {
    Wordmark,
    Primary,
    Secondary,
    Muted,
}

fn launcher_text(
    value: impl text::IntoFragment<'static>,
    size: u16,
    kind: LauncherTextKind,
) -> iced::widget::Text<'static> {
    text(value)
        .size(f32::from(size))
        .line_height(match kind {
            LauncherTextKind::Wordmark | LauncherTextKind::Primary => 1.2,
            LauncherTextKind::Secondary => 1.4,
            LauncherTextKind::Muted if size == LAUNCHER_PROJECT_PATH_SIZE => 1.35,
            LauncherTextKind::Muted => 1.2,
        })
        .font(
            if matches!(kind, LauncherTextKind::Wordmark | LauncherTextKind::Primary) {
                Font {
                    weight: font::Weight::Semibold,
                    ..Font::DEFAULT
                }
            } else {
                Font::DEFAULT
            },
        )
        .style(move |theme| {
            let palette = launcher_theme(theme).palette();
            iced::widget::text::Style {
                color: Some(match kind {
                    LauncherTextKind::Wordmark => palette.accent,
                    LauncherTextKind::Primary => palette.primary_text,
                    LauncherTextKind::Secondary => palette.secondary_text,
                    LauncherTextKind::Muted => palette.muted_text,
                }),
            }
        })
}

fn launcher_button(
    label: &'static str,
    kind: ButtonKind,
) -> iced::widget::Button<'static, Message> {
    button(
        launcher_text(label, 12, LauncherTextKind::Primary).style(move |theme| {
            let palette = launcher_theme(theme).palette();
            iced::widget::text::Style {
                color: Some(match kind {
                    ButtonKind::Primary => palette.on_accent_text,
                    _ => palette.primary_text,
                }),
            }
        }),
    )
    .style(move |theme, status| {
        components::button_style(
            launcher_theme(theme),
            kind,
            launcher_button_interaction(status),
        )
    })
}

fn launcher_project_card(project: &RecentProject) -> iced::widget::Button<'static, Message> {
    let path = project.path().to_owned();
    let folder = launcher_icon("launcher-project", LAUNCHER_PROJECT_ICON_SIZE);
    let clock = launcher_icon("launcher-last-opened", LAUNCHER_LAST_OPENED_ICON_SIZE);
    let details = column![
        row![
            launcher_text(
                truncate_launcher_label(project.name(), LAUNCHER_PROJECT_NAME_MAX_CHARS),
                LAUNCHER_PROJECT_NAME_SIZE,
                LauncherTextKind::Primary,
            )
            .width(f32::from(LAUNCHER_PROJECT_TITLE_WIDTH))
            .wrapping(text::Wrapping::None),
            launcher_text(
                truncate_launcher_label(&path, LAUNCHER_PROJECT_PATH_MAX_CHARS),
                LAUNCHER_PROJECT_PATH_SIZE,
                LauncherTextKind::Muted,
            )
            .width(Length::Fill)
            .wrapping(text::Wrapping::None),
        ]
        .spacing(f32::from(LAUNCHER_PROJECT_HEADER_GAP))
        .height(24)
        .align_y(iced::alignment::Vertical::Center),
        Space::new().height(f32::from(LAUNCHER_PROJECT_METADATA_GAP)),
        row![
            clock,
            launcher_text(
                format!("Opened {}", project.last_opened()),
                LAUNCHER_PROJECT_LAST_OPENED_SIZE,
                LauncherTextKind::Muted,
            ),
        ]
        .spacing(6)
        .align_y(iced::alignment::Vertical::Center),
    ];
    button(
        row![column![Space::new().height(4), folder], details]
            .spacing(12)
            .align_y(iced::alignment::Vertical::Top),
    )
    .padding([
        LAUNCHER_PROJECT_CARD_VERTICAL_PADDING,
        LAUNCHER_PROJECT_CARD_HORIZONTAL_PADDING,
    ])
    .width(f32::from(LAUNCHER_PROJECT_CARD_WIDTH))
    .height(f32::from(LAUNCHER_PROJECT_CARD_HEIGHT))
    .on_press(Message::OpenRecentProject(PathBuf::from(path)))
    .style(move |theme, status| {
        components::button_style(
            launcher_theme(theme),
            ButtonKind::Secondary,
            launcher_button_interaction(status),
        )
    })
}

fn truncate_launcher_label(value: &str, maximum_chars: usize) -> String {
    let mut characters = value.chars();
    let visible = characters.by_ref().take(maximum_chars).collect::<String>();

    if characters.next().is_some() {
        format!("{visible}…")
    } else {
        visible
    }
}

fn launcher_icon(name: &'static str, size: u16) -> iced::widget::Svg<'static> {
    let source =
        production_icon_svg(name).expect("launcher icon is checked into the design system");
    svg(Handle::from_memory(source.as_bytes()))
        .width(f32::from(size))
        .height(f32::from(size))
        .style(move |theme, _| iced::widget::svg::Style {
            color: Some(launcher_theme(theme).palette().muted_text),
        })
}

fn launcher_theme(theme: &Theme) -> ParchMintTheme {
    ParchMintTheme::from_iced_theme(theme).unwrap_or_else(|| {
        if theme.palette().background.r
            + theme.palette().background.g
            + theme.palette().background.b
            < 1.5
        {
            ParchMintTheme::new(ResolvedAppearance::Dark)
        } else {
            ParchMintTheme::new(ResolvedAppearance::Light)
        }
    })
}

fn launcher_button_interaction(status: iced::widget::button::Status) -> Interaction {
    match status {
        iced::widget::button::Status::Active => Interaction::Rest,
        iced::widget::button::Status::Hovered => Interaction::Hovered,
        iced::widget::button::Status::Pressed => Interaction::Pressed,
        iced::widget::button::Status::Disabled => Interaction::Disabled,
    }
}

fn legacy_project_surface(
    id: window::Id,
    project: String,
    destination: RibbonDestination,
    close_failure: Option<String>,
    status: Option<String>,
) -> Element<'static, Message> {
    let navigation = row![
        destination_button(id, "Editor", RibbonDestination::Editor),
        destination_button(id, "Cards", RibbonDestination::Cards),
        destination_button(id, "History", RibbonDestination::History),
        destination_button(id, "Recently Deleted", RibbonDestination::RecentlyDeleted),
        destination_button(id, "Export", RibbonDestination::Export),
        destination_button(id, "Settings", RibbonDestination::Settings),
    ]
    .spacing(8);
    let mut content = column![
        navigation,
        text(format!("{:?}", destination)).size(24),
        text(project).size(14),
        text("The project session is open and connected to the production service graph.").size(16),
    ]
    .spacing(20);
    if destination == RibbonDestination::Settings {
        content = content.push(
            column![
                text("System appearance").size(28),
                text("Appearance").size(20),
                text("Choose the application appearance. Project styles and export do not change."),
                button("System — Follow the operating system")
                    .on_press(Message::AppearanceSelected(AppearanceMode::System)),
                button("Light — Keep this appearance when the system changes")
                    .on_press(Message::AppearanceSelected(AppearanceMode::Light)),
                button("Dark — Keep this appearance when the system changes")
                    .on_press(Message::AppearanceSelected(AppearanceMode::Dark)),
            ]
            .spacing(10),
        );
    }
    if let Some(error) = close_failure {
        content = content.push(
            column![
                text(format!("Final save failed: {error}")).size(14),
                row![
                    button("Retry").on_press(Message::RetryClose(id)),
                    button("Cancel Close").on_press(Message::CancelClose(id)),
                ]
                .spacing(8),
            ]
            .spacing(8),
        );
    }
    if let Some(status) = status {
        content = content.push(text(status).size(14));
    }
    container(content)
        .padding(24)
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

fn suggested_directory_name(title: &str) -> String {
    let slug = title
        .trim()
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>();
    let slug = slug
        .split('-')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    if slug.is_empty() {
        "untitled-project".to_owned()
    } else {
        slug
    }
}

fn stable_id_string(bytes: &[u8; 16]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn mount_matches_active_document(
    active_document: Option<&str>,
    document: parchmint_domain::DocumentId,
) -> bool {
    let document = stable_id_string(document.as_bytes());
    active_document == Some(document.as_str())
}

fn mount_session_for_document(
    shared: Option<SharedEditorSession>,
    retained: &BTreeMap<parchmint_domain::DocumentId, SharedEditorSession>,
    document: parchmint_domain::DocumentId,
    load: CanonicalDocumentLoad,
) -> MountedEditorSession {
    shared
        .or_else(|| retained.get(&document).cloned())
        .map_or_else(
            || MountedEditorSession::Open(load),
            MountedEditorSession::Reuse,
        )
}

fn stable_id_bytes(value: &str) -> Result<[u8; 16], String> {
    if value.len() != 32 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("stable identifier must contain 32 hexadecimal characters".to_owned());
    }
    let mut bytes = [0_u8; 16];
    for (index, slot) in bytes.iter_mut().enumerate() {
        *slot = u8::from_str_radix(&value[index * 2..index * 2 + 2], 16)
            .map_err(|error| format!("invalid stable identifier: {error}"))?;
    }
    Ok(bytes)
}

fn history_current_document(
    snapshot: &ProjectSnapshot,
    workspace: &ProjectWorkspace,
) -> Option<HistoryCurrentDocument> {
    let document_id = workspace.focused_history_document()?;
    let source = snapshot
        .documents
        .iter()
        .find(|document| stable_id_string(document.document_id.as_bytes()) == document_id)?;
    let title = snapshot
        .project
        .nodes
        .iter()
        .find_map(|(_, node)| match node.kind {
            parchmint_domain::NodeKind::Document(candidate) if candidate == source.document_id => {
                Some(node.title.clone())
            }
            _ => None,
        })
        .unwrap_or_else(|| "Active document".to_owned());
    let semantic = EditorCoreSession::open(CanonicalDocumentLoad::new(
        source.document_id,
        source.body.clone(),
    ))
    .ok()?
    .canonical_projection()
    .semantic()
    .clone();
    Some(HistoryCurrentDocument {
        document_id: document_id.to_owned(),
        title,
        body: source.body.clone(),
        semantic,
    })
}

fn format_last_opened(seconds: u64) -> String {
    if seconds == 0 {
        "at an unknown time".to_owned()
    } else {
        let days = (seconds / 86_400) as i64;
        let seconds_in_day = seconds % 86_400;
        let hour = seconds_in_day / 3_600;
        let minute = seconds_in_day % 3_600 / 60;
        let (year, month, day) = civil_date_from_unix_days(days);
        format!("on {year:04}-{month:02}-{day:02} at {hour:02}:{minute:02} UTC")
    }
}

fn civil_date_from_unix_days(days: i64) -> (i64, u64, u64) {
    // Howard Hinnant's civil-from-days transform, with Unix epoch offset.
    let shifted = days + 719_468;
    let era = if shifted >= 0 {
        shifted
    } else {
        shifted - 146_096
    } / 146_097;
    let day_of_era = shifted - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let mut year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_prime + 2) / 5 + 1;
    let month = month_prime + if month_prime < 10 { 3 } else { -9 };
    year += i64::from(month <= 2);
    (year, month as u64, day as u64)
}

fn destination_button(
    window: window::Id,
    label: &'static str,
    destination: RibbonDestination,
) -> iced::widget::Button<'static, Message> {
    button(label).on_press(Message::SelectDestination {
        window,
        destination,
    })
}

fn window_settings(size: (f32, f32), minimum: (u32, u32)) -> window::Settings {
    window::Settings {
        size: iced::Size::new(size.0, size.1),
        min_size: Some(iced::Size::new(minimum.0 as f32, minimum.1 as f32)),
        position: window::Position::Centered,
        exit_on_close_request: false,
        ..window::Settings::default()
    }
}

fn capture_window_settings(capture: &NativeCaptureRequest) -> window::Settings {
    let (width, height) = capture.logical_size();
    window::Settings {
        size: iced::Size::new(width as f32, height as f32),
        min_size: Some(iced::Size::new(width as f32, height as f32)),
        max_size: Some(iced::Size::new(width as f32, height as f32)),
        position: window::Position::Centered,
        resizable: false,
        exit_on_close_request: false,
        ..window::Settings::default()
    }
}

fn encode_capture_png(
    output_path: &std::path::Path,
    size: (u32, u32),
    rgba: Vec<u8>,
) -> Result<(), String> {
    let expected_len = usize::try_from(u64::from(size.0) * u64::from(size.1) * 4)
        .map_err(|_| "native capture dimensions exceed supported memory size".to_owned())?;
    if rgba.len() != expected_len {
        return Err(format!(
            "native capture contains {} RGBA bytes; expected {expected_len}",
            rgba.len()
        ));
    }
    let file = File::options()
        .write(true)
        .create_new(true)
        .open(output_path)
        .map_err(|error| format!("could not create {}: {error}", output_path.display()))?;
    let writer = BufWriter::new(file);
    let mut encoder = png::Encoder::new(writer, size.0, size.1);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder
        .write_header()
        .map_err(|error| format!("could not encode {}: {error}", output_path.display()))?;
    writer
        .write_image_data(&rgba)
        .map_err(|error| format!("could not encode {}: {error}", output_path.display()))
}

fn strict_size_error(request: &NativeCaptureRequest, png: &NativeCapturePng) -> Option<String> {
    request
        .required_size
        .filter(|required| *required != png.physical_size)
        .map(|required| {
            format!(
                "native capture wrote {} at {}x{} pixels, but --require-size requested {}x{}",
                png.output_path.display(),
                png.physical_size.0,
                png.physical_size.1,
                required.0,
                required.1,
            )
        })
}

#[cfg(test)]
mod tests {
    use std::{
        cell::RefCell,
        pin::Pin,
        sync::{
            Mutex,
            atomic::{AtomicU64, Ordering},
        },
        task::{Context, Poll},
    };

    use iced::futures::StreamExt;
    use iced::futures::executor::block_on;
    use iced::{Settings, Size};
    use iced_test::Simulator;

    use super::*;

    static APPEARANCE_STREAM_DROPS: AtomicU64 = AtomicU64::new(0);

    struct DropCountingAppearanceStream;

    impl iced::futures::Stream for DropCountingAppearanceStream {
        type Item = Message;

        fn poll_next(self: Pin<&mut Self>, _context: &mut Context<'_>) -> Poll<Option<Message>> {
            Poll::Pending
        }
    }

    impl Drop for DropCountingAppearanceStream {
        fn drop(&mut self) {
            APPEARANCE_STREAM_DROPS.fetch_add(1, Ordering::Relaxed);
        }
    }

    fn drop_counting_appearance_stream() -> iced::futures::stream::BoxStream<'static, Message> {
        Box::pin(DropCountingAppearanceStream)
    }

    #[test]
    fn native_capture_request_requires_a_new_absolute_png_path() {
        let relative = NativeCaptureRequest::new(
            NativeCaptureTarget::Launcher,
            ResolvedAppearance::Light,
            PathBuf::from("capture.png"),
        );
        assert!(relative.is_err());

        let path = std::env::temp_dir().join(format!(
            "parchmint-native-capture-request-{}.png",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);
        let request = NativeCaptureRequest::new(
            NativeCaptureTarget::Launcher,
            ResolvedAppearance::Dark,
            path.clone(),
        )
        .expect("fresh absolute PNG path is authorized");
        assert_eq!(request.settled_frames, 3);
        assert!(std::fs::File::create(&path).is_ok());
        assert!(
            NativeCaptureRequest::new(
                NativeCaptureTarget::Launcher,
                ResolvedAppearance::Dark,
                path.clone(),
            )
            .is_err()
        );
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn launcher_foundation_preserves_penpot_geometry_and_semantic_variants() {
        assert_eq!(LAUNCHER_INSET, 72);
        assert_eq!(LAUNCHER_RHYTHM, 28);
        assert_eq!(LAUNCHER_ACTION_ROW_HEIGHT, 52);
        assert_eq!(LAUNCHER_PROJECT_CARD_WIDTH, 520);
        assert_eq!(LAUNCHER_PROJECT_CARD_HEIGHT, 96);
        assert_eq!(LAUNCHER_PROJECT_CARD_GAP, 22);
        assert_eq!(LAUNCHER_PROJECT_CARD_HORIZONTAL_PADDING, 16);
        assert_eq!(LAUNCHER_PROJECT_CARD_VERTICAL_PADDING, 10);
        assert_eq!(LAUNCHER_PROJECT_TITLE_WIDTH, 124);
        assert_eq!(LAUNCHER_PROJECT_HEADER_GAP, 14);
        assert_eq!(LAUNCHER_PROJECT_METADATA_GAP, 12);

        for appearance in [ResolvedAppearance::Light, ResolvedAppearance::Dark] {
            let theme = ParchMintTheme::new(appearance);
            assert_eq!(
                components::button_style(theme, ButtonKind::Primary, Interaction::Rest).background,
                Some(iced::Background::Color(theme.palette().accent))
            );
            assert_eq!(
                components::button_style(theme, ButtonKind::Secondary, Interaction::Rest)
                    .background,
                Some(iced::Background::Color(theme.palette().panel))
            );
            assert_eq!(
                ParchMintTheme::from_iced_theme(&theme.iced_theme()),
                Some(theme)
            );
        }
    }

    #[test]
    fn launcher_card_labels_truncate_on_character_boundaries_without_wrapping() {
        assert_eq!(truncate_launcher_label("Northbound", 24), "Northbound");
        assert_eq!(truncate_launcher_label("aé日b", 3), "aé日…");
        assert_eq!(
            truncate_launcher_label(
                "/Projects/a-recent-project-with-a-deliberately-long-name",
                LAUNCHER_PROJECT_PATH_MAX_CHARS,
            ),
            "/Projects/a-recent-project-with-a-deli…"
        );
    }

    #[test]
    fn project_creation_view_keeps_submit_and_cancel_actions_available() {
        let mut draft = NewProjectDraft::default();
        draft.set_title("Northbound");
        draft.set_destination("/Projects/Northbound");
        draft.set_author(Some("A. Writer".to_owned()));
        let mut simulator = Simulator::<Message>::with_size(
            Settings::default(),
            Size::new(900.0, 620.0),
            launcher_surface(&[], &draft, true, false, None),
        );

        assert!(simulator.find("Choose Destination…").is_ok());
        assert!(simulator.find("Cancel").is_ok());
        assert!(simulator.find("Create and Open").is_ok());
    }

    #[test]
    fn native_capture_preserves_clamped_dimensions_unless_strict_size_is_requested() {
        let path = std::env::temp_dir().join(format!(
            "parchmint-native-capture-size-{}.png",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);
        let mut request = NativeCaptureRequest::new(
            NativeCaptureTarget::Launcher,
            ResolvedAppearance::Light,
            path.clone(),
        )
        .expect("fresh output path");
        request
            .configure_viewport((960, 540), 1)
            .expect("scale one viewport");
        assert_eq!(request.requested_physical_size(), (960, 540));
        let png = NativeCapturePng {
            output_path: path,
            physical_size: (1920, 1013),
        };
        assert!(strict_size_error(&request, &png).is_none());
        request
            .require_size(Some((2880, 1800)))
            .expect("strict dimensions");
        assert!(
            strict_size_error(&request, &png)
                .expect("strict mismatch")
                .contains("1920x1013")
        );
    }

    #[test]
    fn native_capture_encoder_writes_rgba_png_without_replacing_a_file() {
        let path = std::env::temp_dir().join(format!(
            "parchmint-native-capture-encoder-{}.png",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);
        encode_capture_png(&path, (1, 1), vec![12, 34, 56, 255]).expect("encode one RGBA pixel");
        let file = std::io::BufReader::new(File::open(&path).expect("open encoded PNG"));
        let mut decoder = png::Decoder::new(file);
        decoder.set_transformations(png::Transformations::EXPAND);
        let mut reader = decoder.read_info().expect("read PNG info");
        let mut bytes = vec![0; reader.output_buffer_size().expect("PNG output size")];
        let frame = reader.next_frame(&mut bytes).expect("read PNG frame");
        assert_eq!((frame.width, frame.height), (1, 1));
        assert_eq!(&bytes[..frame.buffer_size()], &[12, 34, 56, 255]);
        assert!(encode_capture_png(&path, (1, 1), vec![0, 0, 0, 255]).is_err());
        let _ = std::fs::remove_file(path);
    }

    struct RecordingCallbacks {
        open_result: Mutex<Option<NativeProjectOpenResult>>,
        created_projects: Mutex<Vec<NativeNewProjectRequest>>,
        closed: Mutex<Vec<PathBuf>>,
        close_results: Mutex<Vec<Result<(), String>>>,
        created: Mutex<Vec<WindowCapability>>,
        destroyed: Mutex<Vec<WindowCapability>>,
        system_appearances: Mutex<Vec<ResolvedAppearance>>,
        system_appearance_result: Mutex<Result<Option<ResolvedAppearance>, String>>,
    }

    impl RecordingCallbacks {
        fn opening(result: NativeProjectOpenResult) -> Self {
            Self {
                open_result: Mutex::new(Some(result)),
                created_projects: Mutex::new(Vec::new()),
                closed: Mutex::new(Vec::new()),
                close_results: Mutex::new(vec![Ok(())]),
                created: Mutex::new(Vec::new()),
                destroyed: Mutex::new(Vec::new()),
                system_appearances: Mutex::new(Vec::new()),
                system_appearance_result: Mutex::new(Ok(None)),
            }
        }
    }

    impl NativeDesktopCallbacks for RecordingCallbacks {
        fn open_project(&self, _project: PathBuf) -> Result<NativeProjectOpenResult, String> {
            self.open_result
                .lock()
                .expect("open result mutex poisoned")
                .take()
                .ok_or_else(|| "no open result configured".to_owned())
        }

        fn close_project(&self, project: PathBuf) -> Result<(), String> {
            self.closed
                .lock()
                .expect("closed projects mutex poisoned")
                .push(project);
            self.close_results
                .lock()
                .expect("close results mutex poisoned")
                .pop()
                .unwrap_or(Ok(()))
        }

        fn create_project(
            &self,
            request: NativeNewProjectRequest,
        ) -> Result<NativeProjectOpenResult, String> {
            self.created_projects
                .lock()
                .expect("created projects mutex poisoned")
                .push(request);
            self.open_result
                .lock()
                .expect("open result mutex poisoned")
                .take()
                .ok_or_else(|| "no create result configured".to_owned())
        }

        fn project_window_created(&self, window: WindowCapability) {
            self.created
                .lock()
                .expect("created windows mutex poisoned")
                .push(window);
        }

        fn project_window_destroyed(&self, window: WindowCapability) {
            self.destroyed
                .lock()
                .expect("destroyed windows mutex poisoned")
                .push(window);
        }

        fn system_appearance_changed(
            &self,
            appearance: ResolvedAppearance,
        ) -> Result<Option<ResolvedAppearance>, String> {
            self.system_appearances
                .lock()
                .expect("system appearance mutex poisoned")
                .push(appearance);
            self.system_appearance_result
                .lock()
                .expect("system appearance result mutex poisoned")
                .clone()
        }
    }

    fn launcher_window(desktop: &NativeDesktop) -> window::Id {
        desktop
            .windows
            .iter()
            .find_map(|(id, native)| matches!(native, NativeWindow::Launcher).then_some(*id))
            .expect("launcher window")
    }

    fn click_and_update(desktop: &mut NativeDesktop, window: window::Id, label: &str) {
        let mut simulator = Simulator::<Message>::with_size(
            Settings::default(),
            Size::new(900.0, 620.0),
            desktop.view(window),
        );
        simulator.click(label).expect("rendered action");
        for message in simulator.into_messages() {
            let _ = desktop.update(message);
        }
    }

    fn type_into_and_update(
        desktop: &mut NativeDesktop,
        window: window::Id,
        placeholder: &str,
        value: &str,
    ) {
        let mut simulator = Simulator::<Message>::with_size(
            Settings::default(),
            Size::new(900.0, 620.0),
            desktop.view(window),
        );
        simulator.click(placeholder).expect("rendered text input");
        assert_ne!(simulator.typewrite(value), event::Status::Ignored);
        for message in simulator.into_messages() {
            let _ = desktop.update(message);
        }
    }

    fn complete_task(desktop: &mut NativeDesktop, task: Task<Message>) {
        let Some(stream) = iced_test::runtime::task::into_stream(task) else {
            return;
        };
        let actions = block_on(stream.collect::<Vec<_>>());
        for action in actions {
            if let iced_test::runtime::Action::Output(message) = action {
                // Window-open actions wait for a platform acknowledgement that
                // headless Iced deliberately does not synthesize. The output
                // itself is the callback completion under test; route it into
                // the real desktop and leave native window actions to the
                // production event loop.
                let _ = desktop.update(message);
            }
        }
    }

    fn native_close_actions(task: Task<Message>) -> (Vec<window::Id>, usize) {
        let Some(stream) = iced_test::runtime::task::into_stream(task) else {
            return (Vec::new(), 0);
        };
        let mut closed = Vec::new();
        let mut exits = 0;
        for action in block_on(stream.collect::<Vec<_>>()) {
            match action {
                iced_test::runtime::Action::Window(iced_test::runtime::window::Action::Close(
                    window,
                )) => closed.push(window),
                iced_test::runtime::Action::Exit => exits += 1,
                _ => {}
            }
        }
        (closed, exits)
    }

    fn legacy_project(path: PathBuf, window_id: u64) -> NativeProjectWindow {
        NativeProjectWindow {
            project: path,
            window: WindowCapability::new(window_id, 1),
            session: parchmint_ui_api::ProjectSessionRegistry::new().register(window_id),
            project_ui: None,
            editor: None,
        }
    }

    #[test]
    fn rendered_launcher_creation_flow_submits_callback_request_and_opens_project() {
        let destination = std::env::temp_dir().join(format!(
            "parchmint-native-flow-create-{}",
            std::process::id()
        ));
        let project = legacy_project(destination.clone(), 41);
        let callbacks = Arc::new(RecordingCallbacks::opening(
            NativeProjectOpenResult::Opened(project.clone()),
        ));
        let (mut desktop, _boot) = NativeDesktop::boot(NativeDesktopStartup {
            appearance: ResolvedAppearance::Light,
            appearance_mode: AppearanceMode::System,
            recent_projects: Vec::new(),
            projects: Vec::new(),
            locked_project: None,
            capture: None,
            callbacks: callbacks.clone(),
        });
        let launcher = launcher_window(&desktop);

        click_and_update(&mut desktop, launcher, "Create Project");
        type_into_and_update(&mut desktop, launcher, "Project title", "Flow Novel");
        type_into_and_update(
            &mut desktop,
            launcher,
            "Project destination",
            destination.to_str().expect("temporary path is UTF-8"),
        );
        type_into_and_update(&mut desktop, launcher, "Author (optional)", "Flow Author");

        let mut simulator = Simulator::<Message>::with_size(
            Settings::default(),
            Size::new(900.0, 620.0),
            desktop.view(launcher),
        );
        simulator
            .click("Create and Open")
            .expect("rendered creation submit");
        let messages = simulator.into_messages().collect::<Vec<_>>();
        assert_eq!(messages.len(), 1);
        let create_task = desktop.update(messages.into_iter().next().expect("submit message"));
        complete_task(&mut desktop, create_task);

        assert_eq!(
            callbacks
                .created_projects
                .lock()
                .expect("created projects mutex poisoned")
                .as_slice(),
            [NativeNewProjectRequest {
                title: "Flow Novel".to_owned(),
                destination: destination.clone(),
                author: Some("Flow Author".to_owned()),
            }]
        );
        assert!(!desktop.creating_project);
        assert!(desktop.project_windows.contains_key(&project.window));
        let project_window = desktop.project_windows[&project.window];
        let mut project_surface = Simulator::<Message>::with_size(
            Settings::default(),
            Size::new(900.0, 620.0),
            desktop.view(project_window),
        );
        assert!(
            project_surface
                .find(destination.to_str().expect("UTF-8 path"))
                .is_ok()
        );
    }

    #[test]
    fn rendered_recent_project_flow_shows_pending_then_locked_and_missing_feedback() {
        let existing = std::env::temp_dir().join(format!(
            "parchmint-native-flow-recent-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&existing).expect("temporary recent project directory");
        let callbacks = Arc::new(RecordingCallbacks::opening(NativeProjectOpenResult::Locked));
        let (mut desktop, _boot) = NativeDesktop::boot(NativeDesktopStartup {
            appearance: ResolvedAppearance::Light,
            appearance_mode: AppearanceMode::System,
            recent_projects: vec![PreferenceRecentProject::new(
                "Existing recent",
                existing.to_string_lossy(),
                0,
            )],
            projects: Vec::new(),
            locked_project: None,
            capture: None,
            callbacks,
        });
        let launcher = launcher_window(&desktop);

        let mut simulator = Simulator::<Message>::with_size(
            Settings::default(),
            Size::new(900.0, 620.0),
            desktop.view(launcher),
        );
        simulator
            .click("Existing recent")
            .expect("recent card action");
        let open_task = desktop.update(
            simulator
                .into_messages()
                .next()
                .expect("recent card message"),
        );
        assert!(desktop.opening_project);
        {
            let mut pending_surface = Simulator::<Message>::with_size(
                Settings::default(),
                Size::new(900.0, 620.0),
                desktop.view(launcher),
            );
            assert!(pending_surface.find("Opening Project…").is_ok());
        }
        complete_task(&mut desktop, open_task);
        let expected_locked = format!("Project is already open: {}", existing.display());
        {
            let mut locked_surface = Simulator::<Message>::with_size(
                Settings::default(),
                Size::new(900.0, 620.0),
                desktop.view(launcher),
            );
            assert!(locked_surface.find(expected_locked.as_str()).is_ok());
        }

        let missing = existing.with_file_name(format!(
            "parchmint-native-flow-missing-{}",
            std::process::id()
        ));
        desktop.launcher.add_recent_project(
            "Missing recent",
            missing.to_string_lossy(),
            "at an unknown time",
        );
        click_and_update(&mut desktop, launcher, "Missing recent");
        let expected_missing = format!(
            "The project at {} is no longer available. It may have been moved or deleted.",
            missing.display()
        );
        let mut missing_surface = Simulator::<Message>::with_size(
            Settings::default(),
            Size::new(900.0, 620.0),
            desktop.view(launcher),
        );
        assert!(missing_surface.find(expected_missing.as_str()).is_ok());
        std::fs::remove_dir(&existing).expect("remove temporary recent project directory");
    }

    #[test]
    fn rendered_final_save_failure_can_cancel_then_retry_to_successful_close() {
        let project = legacy_project(PathBuf::from("/tmp/native-flow-close.parchmint"), 42);
        let callbacks = Arc::new(RecordingCallbacks::opening(NativeProjectOpenResult::Locked));
        let (mut desktop, _boot) = NativeDesktop::boot(NativeDesktopStartup {
            appearance: ResolvedAppearance::Light,
            appearance_mode: AppearanceMode::System,
            recent_projects: Vec::new(),
            projects: vec![project.clone()],
            locked_project: None,
            capture: None,
            callbacks: callbacks.clone(),
        });
        let native_window = desktop.project_windows[&project.window];
        *callbacks
            .close_results
            .lock()
            .expect("close results mutex poisoned") = vec![
            Ok(()),
            Err("disk unavailable again".to_owned()),
            Err("disk unavailable".to_owned()),
        ];
        let initial_close = desktop.update(Message::CloseRequested(native_window));
        complete_task(&mut desktop, initial_close);
        {
            let mut failed_surface = Simulator::<Message>::with_size(
                Settings::default(),
                Size::new(900.0, 620.0),
                desktop.view(native_window),
            );
            assert!(
                failed_surface
                    .find("Final save failed: disk unavailable")
                    .is_ok()
            );
            assert!(failed_surface.find("Retry").is_ok());
            assert!(failed_surface.find("Cancel Close").is_ok());
        }

        click_and_update(&mut desktop, native_window, "Cancel Close");
        {
            let mut cancelled_surface = Simulator::<Message>::with_size(
                Settings::default(),
                Size::new(900.0, 620.0),
                desktop.view(native_window),
            );
            assert!(
                cancelled_surface
                    .find("Final save failed: disk unavailable")
                    .is_err()
            );
        }

        let failed_again = desktop.update(Message::CloseRequested(native_window));
        complete_task(&mut desktop, failed_again);
        let mut retry_simulator = Simulator::<Message>::with_size(
            Settings::default(),
            Size::new(900.0, 620.0),
            desktop.view(native_window),
        );
        retry_simulator
            .click("Retry")
            .expect("rendered retry action");
        let retry_task = desktop.update(
            retry_simulator
                .into_messages()
                .next()
                .expect("retry message"),
        );
        assert!(desktop.closing_windows.contains(&native_window));
        complete_task(&mut desktop, retry_task);

        assert!(!desktop.windows.contains_key(&native_window));
        assert!(!desktop.project_windows.contains_key(&project.window));
        assert_eq!(
            callbacks
                .destroyed
                .lock()
                .expect("destroyed windows mutex poisoned")
                .as_slice(),
            [project.window]
        );
    }

    #[test]
    fn project_window_minimum_uses_the_shell_contract() {
        let settings = window_settings((1280.0, 720.0), ShellLayout::MIN_WINDOW_SIZE);

        assert_eq!(
            settings.min_size,
            Some(iced::Size::new(
                ShellLayout::MIN_WINDOW_SIZE.0 as f32,
                ShellLayout::MIN_WINDOW_SIZE.1 as f32,
            ))
        );
    }

    #[test]
    fn restored_tabs_route_through_session_authorized_lazy_open_effects() {
        let primary = "primary-unloaded".to_owned();
        let companion = "companion-unloaded".to_owned();

        assert_eq!(
            restored_document_effect(EditorPane::Primary, primary.clone()),
            ProjectEffect::OpenDocumentInPrimary(primary)
        );
        assert_eq!(
            restored_document_effect(EditorPane::Companion, companion.clone()),
            ProjectEffect::OpenDocumentInCompanion(companion)
        );
    }

    #[test]
    fn rapid_synopsis_edits_use_one_native_persistence_lane_and_ignore_reversed_completion() {
        let mut workspace = ProjectWorkspace::from_fixture(crate::ProjectFixture::Explorer);
        let node_id = "chapter-one".to_owned();
        assert!(
            workspace
                .update(ProjectMessage::EditSynopsis {
                    node_id: node_id.clone(),
                    action: iced::widget::text_editor::Action::Move(
                        iced::widget::text_editor::Motion::End,
                    ),
                })
                .is_empty()
        );

        let first = workspace.update(ProjectMessage::EditSynopsis {
            node_id: node_id.clone(),
            action: iced::widget::text_editor::Action::Edit(
                iced::widget::text_editor::Edit::Insert('a'),
            ),
        });
        let second = workspace.update(ProjectMessage::EditSynopsis {
            node_id,
            action: iced::widget::text_editor::Action::Edit(
                iced::widget::text_editor::Edit::Insert('b'),
            ),
        });
        let commit = |effects: Vec<ProjectEffect>| match effects.as_slice() {
            [ProjectEffect::CommitSynopsis { node_id, synopsis }] => SynopsisCommit {
                node_id: node_id.clone(),
                synopsis: synopsis.clone(),
            },
            _ => panic!("every Synopsis keystroke emits one persistence intent"),
        };
        let first = commit(first);
        let second = commit(second);
        assert_ne!(first.synopsis, second.synopsis);

        let mut queue = SynopsisCommitQueue::default();
        assert_eq!(queue.enqueue(first.clone()), Some(first.clone()));
        assert_eq!(queue.enqueue(second.clone()), None);

        // The old Task::batch path could complete this newer request before
        // the older one. It was never started here, so that completion cannot
        // advance or corrupt the persistence lane.
        assert_eq!(queue.finish(&second, true), None);
        assert_eq!(queue.finish(&first, true), Some(second));
    }

    #[test]
    fn keyboard_accelerators_use_the_platform_command_modifier() {
        assert_eq!(
            keyboard_accelerator("n", keyboard::Modifiers::COMMAND),
            Some("file.new")
        );
        assert_eq!(
            keyboard_accelerator("v", keyboard::Modifiers::COMMAND),
            Some("edit.paste")
        );
        assert_eq!(keyboard_accelerator("s", keyboard::Modifiers::NONE), None);
        #[cfg(target_os = "macos")]
        assert_eq!(
            keyboard_accelerator(
                "z",
                keyboard::Modifiers::COMMAND | keyboard::Modifiers::SHIFT
            ),
            Some("edit.redo")
        );
        #[cfg(not(target_os = "macos"))]
        assert_eq!(
            keyboard_accelerator("y", keyboard::Modifiers::COMMAND),
            Some("edit.redo")
        );
    }

    #[test]
    fn late_mount_completion_cannot_replace_the_newly_active_tab() {
        let first = parchmint_domain::DocumentId::from_bytes([71; 16]);
        let second = parchmint_domain::DocumentId::from_bytes([72; 16]);
        let active = stable_id_string(second.as_bytes());

        assert!(!mount_matches_active_document(Some(&active), first));
        assert!(mount_matches_active_document(Some(&active), second));
        assert!(!mount_matches_active_document(None, second));
    }

    #[test]
    fn captured_editor_shortcuts_do_not_duplicate_clipboard_or_text_input_actions() {
        assert!(should_activate_shortcut("file.save", false));
        assert!(should_activate_shortcut("file.close", false));
        assert!(should_activate_shortcut("edit.undo", true));
        assert!(should_activate_shortcut("edit.redo", true));
        assert!(!should_activate_shortcut("edit.undo", false));
        assert!(!should_activate_shortcut("edit.copy", false));
        assert!(!should_activate_shortcut("edit.paste", false));
    }

    #[test]
    fn runtime_event_only_marks_ignored_keyboard_input_as_accelerator_fallback() {
        let event = Event::Keyboard(keyboard::Event::KeyPressed {
            key: keyboard::Key::Character("s".into()),
            modified_key: keyboard::Key::Character("s".into()),
            physical_key: keyboard::key::Physical::Code(keyboard::key::Code::KeyS),
            location: keyboard::Location::Standard,
            modifiers: keyboard::Modifiers::COMMAND,
            text: Some("s".into()),
            repeat: false,
        });
        let window = window::Id::unique();
        let Some(Message::RuntimeEvent {
            accelerator_fallback,
            ..
        }) = runtime_event(event.clone(), event::Status::Captured, window)
        else {
            panic!("captured keyboard event reaches the runtime router");
        };
        assert!(!accelerator_fallback);
        let Some(Message::RuntimeEvent {
            accelerator_fallback,
            ..
        }) = runtime_event(event, event::Status::Ignored, window)
        else {
            panic!("ignored keyboard event reaches the runtime router");
        };
        assert!(accelerator_fallback);
    }

    #[test]
    fn window_wide_left_release_schedules_uncommitted_hierarchy_drag_cleanup() {
        let window = window::Id::unique();
        let Some(Message::RuntimeEvent { event, .. }) = runtime_event(
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
            event::Status::Ignored,
            window,
        ) else {
            panic!("left release reaches the native fallback router");
        };
        let project = legacy_project(PathBuf::from("/tmp/release-drag.parchmint"), 66);
        let callbacks = Arc::new(RecordingCallbacks::opening(NativeProjectOpenResult::Locked));
        let (mut desktop, _) = NativeDesktop::boot(NativeDesktopStartup {
            appearance: ResolvedAppearance::Light,
            appearance_mode: AppearanceMode::System,
            recent_projects: Vec::new(),
            projects: vec![project.clone()],
            locked_project: None,
            capture: None,
            callbacks,
        });
        let native_window = desktop.project_windows[&project.window];
        let task = desktop.runtime_event(native_window, event, true);
        let Some(stream) = iced_test::runtime::task::into_stream(task) else {
            panic!("release fallback schedules a cleanup message");
        };
        let actions = block_on(stream.collect::<Vec<_>>());
        assert!(actions.into_iter().any(|action| matches!(
            action,
            iced_test::runtime::Action::Output(
                Message::CancelUncommittedHierarchyDrag { window }
            ) if window == native_window
        )));
    }

    #[test]
    fn captured_file_close_still_routes_through_runtime_event() {
        let project = legacy_project(PathBuf::from("/tmp/runtime-close.parchmint"), 65);
        let callbacks = Arc::new(RecordingCallbacks::opening(NativeProjectOpenResult::Locked));
        let (mut desktop, _boot) = NativeDesktop::boot(NativeDesktopStartup {
            appearance: ResolvedAppearance::Light,
            appearance_mode: AppearanceMode::System,
            recent_projects: Vec::new(),
            projects: vec![project.clone()],
            locked_project: None,
            capture: None,
            callbacks,
        });
        let window = desktop.project_windows[&project.window];
        let event = Event::Keyboard(keyboard::Event::KeyPressed {
            key: keyboard::Key::Character("w".into()),
            modified_key: keyboard::Key::Character("w".into()),
            physical_key: keyboard::key::Physical::Code(keyboard::key::Code::KeyW),
            location: keyboard::Location::Standard,
            modifiers: keyboard::Modifiers::COMMAND,
            text: Some("w".into()),
            repeat: false,
        });
        let message = runtime_event(event, event::Status::Captured, window)
            .expect("captured key reaches native runtime routing");
        let _close = desktop.update(message);

        assert!(desktop.closing_windows.contains(&window));
    }

    #[test]
    fn spellcheck_waits_for_a_completed_word_boundary() {
        use parchmint_editor_iced::{MountedEditorKeyCommand, MountedEditorMessage};

        assert!(!completes_spellcheck_word(
            &MountedEditorMessage::InsertText("unfinished".to_owned(),)
        ));
        assert!(completes_spellcheck_word(
            &MountedEditorMessage::InsertText("finished ".to_owned(),)
        ));
        assert!(completes_spellcheck_word(
            &MountedEditorMessage::InsertText("finished,".to_owned(),)
        ));
        assert!(completes_spellcheck_word(
            &MountedEditorMessage::KeyCommand(MountedEditorKeyCommand::SplitBlock,)
        ));
        assert!(!completes_spellcheck_word(
            &MountedEditorMessage::KeyCommand(MountedEditorKeyCommand::Backspace,)
        ));
    }

    #[test]
    fn blocking_callback_helper_leaves_the_event_loop_thread() {
        let event_loop_thread = std::thread::current().id();
        let worker_thread = block_on(NativeDesktop::run_blocking_operation(
            "test callback",
            || Ok(std::thread::current().id()),
        ))
        .expect("blocking callback worker");

        assert_ne!(worker_thread, event_loop_thread);
    }

    #[test]
    fn remounting_a_detached_tab_reuses_its_live_editor_session() {
        let adapter = EditorIcedAdapter::new(parchmint_editor_iced::EditorIcedConfig::default())
            .expect("editor adapter");
        let document = parchmint_editor_api::DocumentId::from_bytes([61; 16]);
        let first_view = ViewId::from_bytes([62; 16]);
        let viewport = EditorViewport::new(320.0, 240.0).expect("editor viewport");
        let first = MountedEditorBinding::mount(
            &adapter,
            MountedEditorBindingConfig::new(
                MountedEditorSession::Open(CanonicalDocumentLoad::new(document, "draft")),
                WindowCapability::new(61, 1),
                first_view,
                viewport,
                EditorSurfaceTheme::light(),
            ),
        )
        .expect("first tab mount");
        let session = first.session();
        adapter
            .input_en_us(session.clone(), first_view, " retained")
            .expect("mounted edit");
        first.detach().expect("tab switch detaches its view");

        let retained = BTreeMap::from([(document, session.clone())]);
        let remounted = MountedEditorBinding::mount(
            &adapter,
            MountedEditorBindingConfig::new(
                mount_session_for_document(
                    None,
                    &retained,
                    document,
                    CanonicalDocumentLoad::new(document, "stale persisted snapshot"),
                ),
                WindowCapability::new(61, 1),
                ViewId::from_bytes([63; 16]),
                viewport,
                EditorSurfaceTheme::light(),
            ),
        )
        .expect("returning tab remounts retained session");

        assert_eq!(
            adapter
                .primary_visible_block(session)
                .expect("live retained text")
                .text(),
            " retaineddraft"
        );
        remounted.detach().expect("remounted tab detaches");
    }

    #[test]
    fn autosave_waits_a_minute_for_idle_and_caps_continuous_input() {
        let start = Instant::now();
        let mut autosave = AutosaveState::default();
        let session = SharedEditorSession::new(1);
        autosave.mark_dirty(session.clone(), 1.into(), start);

        assert!(!autosave.should_save(start + Duration::from_secs(59)));
        assert!(autosave.should_save(start + Duration::from_secs(60)));

        autosave.save_in_flight = true;
        autosave.finish_save(&AutosaveTicket {
            dirty_sessions: BTreeMap::from([(session.clone(), 1.into())]),
        });
        assert!(!autosave.should_save(start + Duration::from_secs(3_600)));

        autosave.mark_dirty(session, 2.into(), start + Duration::from_secs(299));
        assert!(!autosave.should_save(start + Duration::from_secs(358)));
        assert!(autosave.should_save(start + Duration::from_secs(359)));
    }

    #[test]
    fn autosave_plans_an_unfocused_retained_dirty_session() {
        let adapter = EditorIcedAdapter::new(parchmint_editor_iced::EditorIcedConfig::default())
            .expect("editor adapter");
        let viewport = EditorViewport::new(320.0, 240.0).expect("editor viewport");
        let retained_binding = MountedEditorBinding::mount(
            &adapter,
            MountedEditorBindingConfig::new(
                MountedEditorSession::Open(CanonicalDocumentLoad::new(
                    parchmint_editor_api::DocumentId::from_bytes([81; 16]),
                    "retained",
                )),
                WindowCapability::new(81, 1),
                ViewId::from_bytes([82; 16]),
                viewport,
                EditorSurfaceTheme::light(),
            ),
        )
        .expect("retained editor mount");
        let retained_session = retained_binding.session();
        adapter
            .input_en_us(retained_session.clone(), retained_binding.view(), "dirty ")
            .expect("retained edit");
        retained_binding.detach().expect("retained view detaches");
        let focused_binding = MountedEditorBinding::mount(
            &adapter,
            MountedEditorBindingConfig::new(
                MountedEditorSession::Open(CanonicalDocumentLoad::new(
                    parchmint_editor_api::DocumentId::from_bytes([83; 16]),
                    "focused",
                )),
                WindowCapability::new(81, 1),
                ViewId::from_bytes([84; 16]),
                viewport,
                EditorSurfaceTheme::light(),
            ),
        )
        .expect("focused editor mount");
        let focused_session = focused_binding.session();
        let sessions =
            deduplicated_editor_sessions([focused_session.clone()], [retained_session.clone()]);
        let projected = BTreeMap::from([
            (retained_session.clone(), EditorRevision::default()),
            (focused_session.clone(), EditorRevision::default()),
        ]);

        let plans = editor_projection_plans(&adapter, sessions, &projected)
            .expect("autosave projection plans");

        assert_eq!(
            plans,
            vec![EditorProjectionPlan {
                session: retained_session,
                revision: 1.into(),
            }]
        );
    }

    #[test]
    fn final_close_continuation_runs_only_after_every_projection() {
        let first = SharedEditorSession::new(91);
        let second = SharedEditorSession::new(92);
        let plans = vec![
            EditorProjectionPlan {
                session: first,
                revision: 3.into(),
            },
            EditorProjectionPlan {
                session: second,
                revision: 7.into(),
            },
        ];
        let events = RefCell::new(Vec::new());

        let run = run_projection_sequence(
            &plans,
            |plan| {
                events
                    .borrow_mut()
                    .push(format!("project-{}", plan.revision.value()));
                Ok(())
            },
            || {
                events.borrow_mut().push("close".to_owned());
                Ok(())
            },
        );

        assert!(run.result.is_ok());
        assert_eq!(events.into_inner(), ["project-3", "project-7", "close"]);
    }

    #[test]
    fn projection_failure_blocks_close_and_retry_keeps_completed_frontier() {
        let first = SharedEditorSession::new(101);
        let second = SharedEditorSession::new(102);
        let plans = vec![
            EditorProjectionPlan {
                session: first.clone(),
                revision: 4.into(),
            },
            EditorProjectionPlan {
                session: second.clone(),
                revision: 6.into(),
            },
        ];
        let close_calls = RefCell::new(0_u8);
        let failed = run_projection_sequence(
            &plans,
            |plan| {
                if plan.session == second {
                    Err("injected projection failure".to_owned())
                } else {
                    Ok(())
                }
            },
            || {
                *close_calls.borrow_mut() += 1;
                Ok(())
            },
        );
        assert_eq!(failed.result, Err("injected projection failure".to_owned()));
        assert_eq!(*close_calls.borrow(), 0);
        assert_eq!(
            failed.projected,
            BTreeMap::from([(first.clone(), 4.into())])
        );

        let retry = plans
            .iter()
            .filter(|plan| failed.projected.get(&plan.session) < Some(&plan.revision))
            .cloned()
            .collect::<Vec<_>>();
        assert_eq!(retry, vec![plans[1].clone()]);
        let succeeded = run_projection_sequence(
            &retry,
            |_| Ok(()),
            || {
                *close_calls.borrow_mut() += 1;
                Ok(())
            },
        );
        assert!(succeeded.result.is_ok());
        assert_eq!(*close_calls.borrow(), 1);
        assert_eq!(failed.projected.get(&first), Some(&4.into()));
    }

    #[test]
    fn one_shared_session_mounted_twice_is_projected_once() {
        let adapter = EditorIcedAdapter::new(parchmint_editor_iced::EditorIcedConfig::default())
            .expect("editor adapter");
        let viewport = EditorViewport::new(320.0, 240.0).expect("editor viewport");
        let first = MountedEditorBinding::mount(
            &adapter,
            MountedEditorBindingConfig::new(
                MountedEditorSession::Open(CanonicalDocumentLoad::new(
                    parchmint_editor_api::DocumentId::from_bytes([111; 16]),
                    "shared",
                )),
                WindowCapability::new(111, 1),
                ViewId::from_bytes([112; 16]),
                viewport,
                EditorSurfaceTheme::light(),
            ),
        )
        .expect("first shared mount");
        let session = first.session();
        let second = MountedEditorBinding::mount(
            &adapter,
            MountedEditorBindingConfig::new(
                MountedEditorSession::Reuse(session.clone()),
                WindowCapability::new(111, 1),
                ViewId::from_bytes([113; 16]),
                viewport,
                EditorSurfaceTheme::light(),
            ),
        )
        .expect("second shared mount");
        adapter
            .input_en_us(session.clone(), first.view(), "dirty ")
            .expect("shared edit");
        let sessions =
            deduplicated_editor_sessions([first.session(), second.session()], std::iter::empty());

        let plans = editor_projection_plans(
            &adapter,
            sessions,
            &BTreeMap::from([(session.clone(), EditorRevision::default())]),
        )
        .expect("shared projection plan");

        assert_eq!(
            plans,
            vec![EditorProjectionPlan {
                session,
                revision: 1.into(),
            }]
        );
    }

    fn synopsis_ticket(node: &str, synopsis: &str) -> ProjectMutationTicket {
        let commit = SynopsisCommit {
            node_id: node.to_owned(),
            synopsis: synopsis.to_owned(),
        };
        ProjectMutationTicket {
            effect: ProjectEffect::CommitSynopsis {
                node_id: commit.node_id.clone(),
                synopsis: commit.synopsis.clone(),
            },
            history_action: None,
            synopsis_commit: Some(commit),
        }
    }

    #[test]
    fn project_mutation_barrier_serializes_saves_and_blocks_close_through_retry() {
        let first = synopsis_ticket("chapter", "first");
        let second = synopsis_ticket("chapter", "second");
        let mut mutations = ProjectMutationState::default();

        assert_eq!(mutations.enqueue(first.clone()), None);
        assert_eq!(mutations.start_next(), Some(first.clone()));
        assert_eq!(mutations.enqueue(second.clone()), None);
        assert!(mutations.blocks_close());
        assert_eq!(mutations.succeed(&first), None);
        assert_eq!(mutations.start_next(), Some(second.clone()));
        assert!(mutations.blocks_close(), "save one cannot clear save two");

        mutations.fail_effect(second.clone());
        assert!(
            mutations.blocks_close(),
            "failure remains a durability barrier"
        );
        assert_eq!(mutations.retry_failed_effect(), Some(second.clone()));
        assert!(mutations.blocks_close());
        assert_eq!(mutations.succeed(&second), None);
        assert!(!mutations.blocks_close());
    }

    #[test]
    fn rapid_structural_effects_launch_only_after_the_prior_effect_is_durable() {
        let rename = ProjectMutationTicket {
            effect: ProjectEffect::CommitNodeTitle {
                node_id: "chapter".to_owned(),
                title: "Renamed".to_owned(),
            },
            history_action: None,
            synopsis_commit: None,
        };
        let settings = ProjectMutationTicket {
            effect: ProjectEffect::SetProjectExportSettings(
                parchmint_domain::ProjectExportSettings::default(),
            ),
            history_action: None,
            synopsis_commit: None,
        };
        let mut mutations = ProjectMutationState::default();

        assert_eq!(mutations.enqueue(rename.clone()), None);
        assert_eq!(mutations.start_next(), Some(rename.clone()));
        assert_eq!(mutations.enqueue(settings.clone()), None);
        assert_eq!(mutations.active, Some(rename.clone()));
        assert_eq!(mutations.succeed(&rename), None);
        assert_eq!(mutations.start_next(), Some(settings.clone()));
        assert_eq!(mutations.active, Some(settings.clone()));
        assert_eq!(mutations.succeed(&settings), None);
        assert!(!mutations.blocks_close());
    }

    #[test]
    fn applied_non_idempotent_effect_retries_only_its_failed_save() {
        let create = ProjectMutationTicket {
            effect: ProjectEffect::CreateHierarchy {
                parent_id: "manuscript".to_owned(),
                kind: crate::HierarchyItemKind::Document,
            },
            history_action: None,
            synopsis_commit: None,
        };
        let mut mutations = ProjectMutationState::default();
        let mut effect_executions = 0;

        mutations.enqueue(create.clone());
        let launched = mutations.start_next().expect("effect launches once");
        effect_executions += 1;
        mutations.fail_save(launched);

        assert_eq!(mutations.retry_failed_save(), Some(create.clone()));
        assert_eq!(effect_executions, 1, "save retry must not replay create");
        assert_eq!(mutations.succeed(&create), None);
        assert!(!mutations.blocks_close());
    }

    #[test]
    fn opaque_replacement_and_project_dictionary_mutations_block_close_through_save() {
        let mut mutations = OpaqueMutationState::default();
        let replacement = mutations.begin(OpaqueMutationKind::Replacement);
        let dictionary = mutations.begin(OpaqueMutationKind::ProjectDictionary);
        mutations.enqueue(replacement, 0, |_| Task::none());
        mutations.enqueue(dictionary, 0, |_| Task::none());
        let first = mutations.queued.pop_front().expect("replacement queued");
        mutations.active = Some(first);
        assert!(mutations.blocks_close());

        mutations.finish(replacement);
        let second = mutations.queued.pop_front().expect("dictionary queued");
        mutations.active = Some(second);
        assert!(
            mutations.blocks_close(),
            "dictionary save is still outstanding"
        );
        mutations.fail_save(dictionary);
        assert!(
            mutations.blocks_close(),
            "failed save remains a close barrier"
        );
        assert_eq!(mutations.failed_save.take(), Some(dictionary));
        mutations.active = Some(PendingOpaqueMutation {
            token: dictionary,
            prior_projects: 0,
            launch: Arc::new(|_| Task::none()),
        });
        mutations.fail_save(dictionary);
        assert_eq!(mutations.failed_save, Some(dictionary));
        assert!(mutations.active.is_none());
        mutations.finish(dictionary);
        assert!(!mutations.blocks_close());

        assert!(editor_effect_requires_durability(
            &EditorEffect::SpellingDictionaryAction {
                view: ViewId::from_bytes([131; 16]),
                word: "parchmint".to_owned(),
                scope: crate::SpellingDictionaryScope::Project,
                add: true,
            }
        ));
    }

    #[test]
    fn first_mutation_completion_preserves_the_next_optimistic_edit() {
        let first = ProjectMutationTicket {
            effect: ProjectEffect::CommitNodeTitle {
                node_id: "chapter-one".to_owned(),
                title: "First rename".to_owned(),
            },
            history_action: None,
            synopsis_commit: None,
        };
        let second = ProjectMutationTicket {
            effect: ProjectEffect::CommitMetadataValue {
                node_id: "chapter-two".to_owned(),
                field_id: "status".to_owned(),
                value: "Revised locally".to_owned(),
            },
            history_action: None,
            synopsis_commit: None,
        };
        let mut mutations = ProjectMutationState::default();
        assert_eq!(mutations.enqueue(first.clone()), None);
        assert_eq!(mutations.start_next(), Some(first.clone()));
        assert_eq!(mutations.enqueue(second.clone()), None);

        assert!(!should_reconcile_refreshed_presentation(
            Some(&first),
            None,
            true
        ));
        assert_eq!(mutations.succeed(&first), None);
        assert_eq!(mutations.start_next(), Some(second.clone()));
        assert_eq!(mutations.active, Some(second));
    }

    #[test]
    fn queued_explicit_save_intent_survives_an_active_save_failure() {
        let mut autosave = AutosaveState::default();
        autosave.save_in_flight = true;
        autosave.explicit_save_waiting = true;

        autosave.save_in_flight = false;

        assert!(
            autosave.explicit_save_waiting,
            "failure must leave the queued explicit request retryable"
        );
    }

    #[test]
    fn clean_explicit_save_does_not_request_an_empty_persistence_write() {
        let now = Instant::now();
        let session = SharedEditorSession::new(123);
        let mut autosave = AutosaveState::default();

        assert!(!autosave.requires_projection_save(false));
        assert!(autosave.requires_projection_save(true));

        autosave.mark_dirty(session, EditorRevision::from(1), now);
        assert!(autosave.requires_projection_save(false));
    }

    #[test]
    fn persistent_scheduler_orders_project_replacement_dictionary_and_later_project_work() {
        let rename = ProjectMutationTicket {
            effect: ProjectEffect::CommitNodeTitle {
                node_id: "chapter".to_owned(),
                title: "Renamed".to_owned(),
            },
            history_action: None,
            synopsis_commit: None,
        };
        let metadata = ProjectMutationTicket {
            effect: ProjectEffect::CommitMetadataValue {
                node_id: "chapter".to_owned(),
                field_id: "status".to_owned(),
                value: "Done".to_owned(),
            },
            history_action: None,
            synopsis_commit: None,
        };
        let mut projects = ProjectMutationState::default();
        let mut opaque = OpaqueMutationState::default();
        projects.enqueue(rename.clone());
        let replacement = opaque.begin(OpaqueMutationKind::Replacement);
        opaque.enqueue(replacement, projects.queued.len(), |_| Task::none());
        let dictionary = opaque.begin(OpaqueMutationKind::ProjectDictionary);
        opaque.enqueue(dictionary, projects.queued.len(), |_| Task::none());
        projects.enqueue(metadata.clone());

        assert_eq!(
            next_persistent_mutation_lane(&projects, &opaque),
            Some(PersistentMutationLane::Project)
        );
        assert_eq!(projects.start_next(), Some(rename.clone()));
        for pending in &mut opaque.queued {
            pending.prior_projects -= 1;
        }
        projects.succeed(&rename);
        assert_eq!(
            next_persistent_mutation_lane(&projects, &opaque),
            Some(PersistentMutationLane::Opaque(
                OpaqueMutationKind::Replacement
            ))
        );
        let first = opaque.queued.pop_front().expect("replacement queued");
        opaque.active = Some(first);
        assert!(next_persistent_mutation_lane(&projects, &opaque).is_none());
        opaque.finish(replacement);
        assert_eq!(
            next_persistent_mutation_lane(&projects, &opaque),
            Some(PersistentMutationLane::Opaque(
                OpaqueMutationKind::ProjectDictionary
            ))
        );
        let second = opaque.queued.pop_front().expect("dictionary queued");
        opaque.active = Some(second);
        opaque.finish(dictionary);
        assert_eq!(
            next_persistent_mutation_lane(&projects, &opaque),
            Some(PersistentMutationLane::Project)
        );
        assert_eq!(projects.start_next(), Some(metadata));
        assert!(projects.blocks_close());
    }

    #[test]
    fn recovery_and_history_workflows_are_typed_persistent_close_barriers() {
        let mut mutations = OpaqueMutationState::default();
        for kind in [
            OpaqueMutationKind::RecoveryAccept,
            OpaqueMutationKind::RecoveryDiscard,
            OpaqueMutationKind::HistoryReinitialize,
        ] {
            let token = mutations.begin(kind);
            mutations.enqueue(token, 0, |_| Task::none());
        }
        assert!(mutations.blocks_close());
        assert_eq!(
            next_persistent_mutation_lane(&ProjectMutationState::default(), &mutations),
            Some(PersistentMutationLane::Opaque(
                OpaqueMutationKind::RecoveryAccept
            ))
        );
    }

    #[test]
    fn first_persistent_failure_stops_queue_until_exact_retry_succeeds() {
        let project = legacy_project(PathBuf::from("/tmp/failure-lane.parchmint"), 141);
        let callbacks = Arc::new(RecordingCallbacks::opening(NativeProjectOpenResult::Locked));
        let (mut desktop, _) = NativeDesktop::boot(NativeDesktopStartup {
            appearance: ResolvedAppearance::Light,
            appearance_mode: AppearanceMode::System,
            recent_projects: Vec::new(),
            projects: vec![project.clone()],
            locked_project: None,
            capture: None,
            callbacks,
        });
        let window = desktop.project_windows[&project.window];
        let first = ProjectMutationTicket {
            effect: ProjectEffect::CommitNodeTitle {
                node_id: "one".to_owned(),
                title: "First".to_owned(),
            },
            history_action: None,
            synopsis_commit: None,
        };
        let second = ProjectMutationTicket {
            effect: ProjectEffect::CommitNodeTitle {
                node_id: "two".to_owned(),
                title: "Second".to_owned(),
            },
            history_action: None,
            synopsis_commit: None,
        };
        let NativeWindow::Project(state) = desktop.windows.get_mut(&window).expect("project")
        else {
            panic!("project window")
        };
        state.project_mutations.active = Some(first.clone());
        state.project_mutations.queued.push_back(second.clone());
        state.autosave.close_after_save = true;

        let _failed = desktop.after_persistent_mutation_terminal(
            window,
            PersistentMutationTerminal::ProjectEffectFailed(first.clone()),
            Some("injected failure".to_owned()),
        );
        let NativeWindow::Project(state) = desktop.windows.get(&window).expect("project") else {
            panic!("project window")
        };
        assert_eq!(state.project_mutations.failed_effect, Some(first.clone()));
        assert_eq!(state.project_mutations.queued.front(), Some(&second));
        assert!(state.project_mutations.active.is_none());
        assert!(desktop.close_failures.contains_key(&project.window));

        let NativeWindow::Project(state) = desktop.windows.get_mut(&window).expect("project")
        else {
            panic!("project window")
        };
        assert_eq!(
            state.project_mutations.retry_failed_effect(),
            Some(first.clone())
        );
        let _succeeded = desktop.after_persistent_mutation_terminal(
            window,
            PersistentMutationTerminal::ProjectSucceeded(first),
            None,
        );
        let NativeWindow::Project(state) = desktop.windows.get(&window).expect("project") else {
            panic!("project window")
        };
        assert_eq!(state.project_mutations.active, Some(second));
    }

    #[test]
    fn explicit_save_projects_every_dirty_session_before_becoming_clean() {
        let first = SharedEditorSession::new(121);
        let second = SharedEditorSession::new(122);
        let plans = vec![
            EditorProjectionPlan {
                session: first.clone(),
                revision: 2.into(),
            },
            EditorProjectionPlan {
                session: second.clone(),
                revision: 5.into(),
            },
        ];
        let events = RefCell::new(Vec::new());
        let run = run_projection_sequence(
            &plans,
            |plan| {
                events
                    .borrow_mut()
                    .push(format!("project-{}", plan.revision.value()));
                Ok(())
            },
            || {
                events.borrow_mut().push("explicit-save".to_owned());
                Ok(9_u64)
            },
        );
        let mut autosave = AutosaveState::default();
        autosave.mark_dirty(first.clone(), 2.into(), Instant::now());
        autosave.mark_dirty(second.clone(), 5.into(), Instant::now());
        let ticket = AutosaveTicket {
            dirty_sessions: autosave.dirty_sessions.clone(),
        };
        autosave.record_projected(run.projected);
        assert!(!autosave.is_clean());
        assert_eq!(run.result, Ok(9));
        autosave.finish_save(&ticket);

        assert_eq!(
            events.into_inner(),
            ["project-2", "project-5", "explicit-save"]
        );
        assert!(autosave.is_clean());
    }

    #[test]
    fn failed_synopsis_keeps_the_latest_visible_draft_for_retry() {
        let first = SynopsisCommit {
            node_id: "chapter".to_owned(),
            synopsis: "first".to_owned(),
        };
        let latest = SynopsisCommit {
            node_id: "chapter".to_owned(),
            synopsis: "latest local draft".to_owned(),
        };
        let mut queue = SynopsisCommitQueue::default();
        assert_eq!(queue.enqueue(first.clone()), Some(first.clone()));
        assert_eq!(queue.enqueue(latest.clone()), None);
        assert_eq!(queue.finish(&first, false), None);

        assert_eq!(queue.retain_latest_failed(&first), latest);
    }

    #[test]
    fn successful_project_teardown_closes_unique_mounted_and_retained_sessions() {
        let project = legacy_project(PathBuf::from("/tmp/session-teardown.parchmint"), 123);
        let callbacks = Arc::new(RecordingCallbacks::opening(NativeProjectOpenResult::Locked));
        let (mut desktop, _) = NativeDesktop::boot(NativeDesktopStartup {
            appearance: ResolvedAppearance::Light,
            appearance_mode: AppearanceMode::System,
            recent_projects: Vec::new(),
            projects: vec![project.clone()],
            locked_project: None,
            capture: None,
            callbacks,
        });
        let window = desktop.project_windows[&project.window];
        let adapter = Arc::new(
            EditorIcedAdapter::new(parchmint_editor_iced::EditorIcedConfig::default())
                .expect("editor adapter"),
        );
        let viewport = EditorViewport::new(320.0, 240.0).expect("editor viewport");
        let mounted_document = parchmint_domain::DocumentId::from_bytes([123; 16]);
        let retained_document = parchmint_domain::DocumentId::from_bytes([124; 16]);
        let first = MountedEditorBinding::mount(
            adapter.as_ref(),
            MountedEditorBindingConfig::new(
                MountedEditorSession::Open(CanonicalDocumentLoad::new(mounted_document, "one")),
                project.window,
                ViewId::from_bytes([125; 16]),
                viewport,
                EditorSurfaceTheme::light(),
            ),
        )
        .expect("first mount");
        let mounted_session = first.session();
        let second = MountedEditorBinding::mount(
            adapter.as_ref(),
            MountedEditorBindingConfig::new(
                MountedEditorSession::Reuse(mounted_session.clone()),
                project.window,
                ViewId::from_bytes([126; 16]),
                viewport,
                EditorSurfaceTheme::light(),
            ),
        )
        .expect("shared second mount");
        let retained = block_on(adapter.open(CanonicalDocumentLoad::new(retained_document, "two")));
        let NativeWindow::Project(state) = desktop.windows.get_mut(&window).expect("project")
        else {
            panic!("project window");
        };
        state.project.editor = Some(adapter.clone());
        state.editor_bindings.insert(EditorPane::Primary, first);
        state.editor_bindings.insert(EditorPane::Companion, second);
        state
            .mounted_documents
            .insert(EditorPane::Primary, mounted_document);
        state
            .mounted_documents
            .insert(EditorPane::Companion, mounted_document);
        state
            .retained_editor_sessions
            .insert(retained_document, retained.clone());

        NativeDesktop::teardown_editor_sessions(state);

        assert!(adapter.revision(mounted_session).is_err());
        assert!(adapter.revision(retained).is_err());
        assert!(state.editor_bindings.is_empty());
        assert!(state.retained_editor_sessions.is_empty());
    }

    #[test]
    fn local_find_uses_scalar_ranges_case_options_and_word_boundaries() {
        assert_eq!(
            local_find_matches("Harbor harbor harbors", "harbor", false, true),
            vec![crate::FindMatch::new(0, 6), crate::FindMatch::new(7, 13)]
        );
        assert_eq!(
            local_find_matches("Ωmega harbor", "harbor", true, false),
            vec![crate::FindMatch::new(6, 12)]
        );
        assert!(local_find_matches("Harbor", "harbor", true, false).is_empty());
    }

    #[test]
    fn link_editor_command_routes_the_current_selection_and_target() {
        let selection = EditorSelection::new(4.into(), 11.into());
        assert_eq!(
            set_link_command(selection, Some("https://example.com".to_owned())),
            EditorCommandKind::SetLink {
                range: selection,
                target: Some("https://example.com".to_owned()),
            }
        );
        assert_eq!(
            set_link_command(selection, None),
            EditorCommandKind::SetLink {
                range: selection,
                target: None,
            }
        );
    }

    fn clipboard_fixture(
        body: &str,
        selection: EditorSelection,
        intent: MountedEditorClipboardIntent,
    ) -> (
        Arc<EditorIcedAdapter>,
        MountedEditorBinding,
        NativeClipboardRequest,
    ) {
        let adapter = Arc::new(
            EditorIcedAdapter::new(parchmint_editor_iced::EditorIcedConfig::default())
                .expect("clipboard adapter"),
        );
        let window = WindowCapability::new(51, 3);
        let view = ViewId::from_bytes([52; 16]);
        let binding = MountedEditorBinding::mount(
            adapter.as_ref(),
            MountedEditorBindingConfig::new(
                MountedEditorSession::Open(CanonicalDocumentLoad::new(
                    parchmint_editor_api::DocumentId::from_bytes([53; 16]),
                    body,
                )),
                window,
                view,
                EditorViewport::new(320.0, 240.0).expect("clipboard viewport"),
                EditorSurfaceTheme::light(),
            ),
        )
        .expect("mounted clipboard binding");
        let session = binding.session();
        adapter
            .execute(
                session.clone(),
                EditorCommandOrigin::new(view),
                AdapterEditorCommand::new(
                    EditorRevision::default(),
                    EditorCommandKind::SetSelection { selection },
                ),
            )
            .expect("clipboard selection");
        let project_session = parchmint_ui_api::ProjectSessionRegistry::new().register(51);
        let request = NativeClipboardRequest {
            capability: window,
            project_session,
            pane: EditorPane::Primary,
            view,
            editor_session: session,
            revision: EditorRevision::default(),
            selection,
            intent,
        };
        (adapter, binding, request)
    }

    #[test]
    fn successful_cut_deletes_only_after_write_and_returns_persistence_revision() {
        let (adapter, binding, request) = clipboard_fixture(
            "<p>Hello <strong>world</strong></p>",
            EditorSelection::new(6.into(), 11.into()),
            MountedEditorClipboardIntent::Cut,
        );
        assert_eq!(
            block_on(adapter.project(request.editor_session.clone(), 0.into()))
                .expect("pre-write projection")
                .body(),
            "<p>Hello <strong>world</strong></p>"
        );

        let mutation = apply_completed_cut(adapter.as_ref(), &binding, &request, Ok(()))
            .expect("successful clipboard write permits cut")
            .expect("cut mutation signal");

        assert_eq!(mutation.revision, EditorRevision::from(1));
        assert_eq!(
            block_on(adapter.project(request.editor_session.clone(), mutation.revision))
                .expect("cut projection")
                .body(),
            "<p>Hello </p>"
        );
    }

    #[test]
    fn failed_cut_write_leaves_document_and_revision_unchanged() {
        let (adapter, binding, request) = clipboard_fixture(
            "<p>Hello world</p>",
            EditorSelection::new(6.into(), 11.into()),
            MountedEditorClipboardIntent::Cut,
        );

        assert!(
            apply_completed_cut(
                adapter.as_ref(),
                &binding,
                &request,
                Err("clipboard unavailable".into()),
            )
            .is_err()
        );
        assert_eq!(
            adapter
                .revision(request.editor_session.clone())
                .expect("unchanged revision"),
            EditorRevision::default()
        );
        assert_eq!(
            block_on(adapter.project(request.editor_session.clone(), 0.into()))
                .expect("unchanged projection")
                .body(),
            "<p>Hello world</p>"
        );
    }

    #[test]
    fn untrusted_rich_paste_retains_supported_marks_and_reports_omissions() {
        let (adapter, binding, request) = clipboard_fixture(
            "<p></p>",
            EditorSelection::default(),
            MountedEditorClipboardIntent::Paste,
        );
        let source = UntrustedClipboardContent::empty()
            .with_html("<p><strong>Keep</strong><script>drop()</script><img src=x></p>");

        let mutation = apply_completed_paste(adapter.as_ref(), &binding, &request, &source)
            .expect("sanitized paste")
            .expect("paste mutation signal");
        let projection =
            block_on(adapter.project(request.editor_session.clone(), mutation.revision))
                .expect("pasted projection");
        assert_eq!(projection.body(), "<p><strong>Keep</strong></p>");
        assert_eq!(projection.semantic().plain_text(), "Keep");
        assert_eq!(projection.semantic().blocks()[0].marks().len(), 1);
        assert_eq!(
            mutation.feedback.as_deref(),
            Some("Pasted supported content; removed unsafe content and omitted 1 image.")
        );
    }

    #[test]
    fn paste_without_formatting_uses_plain_text_when_html_is_available() {
        let (adapter, binding, request) = clipboard_fixture(
            "<p></p>",
            EditorSelection::default(),
            MountedEditorClipboardIntent::PasteWithoutFormatting,
        );
        let source = UntrustedClipboardContent::empty()
            .with_plain_text("Keep")
            .with_html("<strong>Keep</strong>");
        let mutation = apply_completed_paste(adapter.as_ref(), &binding, &request, &source)
            .expect("plain paste")
            .expect("paste mutation signal");
        let projection =
            block_on(adapter.project(request.editor_session.clone(), mutation.revision))
                .expect("pasted projection");
        assert_eq!(projection.body(), "<p>Keep</p>");
        assert!(projection.semantic().blocks()[0].marks().is_empty());
    }

    #[test]
    fn clipboard_completion_rejects_stale_window_session_and_editor_revision() {
        let (adapter, binding, request) = clipboard_fixture(
            "<p>base</p>",
            EditorSelection::default(),
            MountedEditorClipboardIntent::Paste,
        );
        assert!(
            validate_clipboard_identity(
                request.capability,
                request.project_session,
                &binding,
                &request,
            )
            .is_ok()
        );
        assert!(
            validate_clipboard_identity(
                WindowCapability::new(request.capability.window_id(), 99),
                request.project_session,
                &binding,
                &request,
            )
            .is_err()
        );
        let stale_project = parchmint_ui_api::ProjectSessionRegistry::new().register(99);
        assert!(
            validate_clipboard_identity(request.capability, stale_project, &binding, &request,)
                .is_err()
        );

        adapter
            .input_en_us(request.editor_session.clone(), request.view, "new ")
            .expect("intervening editor mutation");
        let before = block_on(adapter.project(request.editor_session.clone(), 1.into()))
            .expect("intervening projection")
            .body()
            .to_owned();
        assert!(
            apply_completed_paste(
                adapter.as_ref(),
                &binding,
                &request,
                &UntrustedClipboardContent::empty().with_plain_text("stale"),
            )
            .is_err()
        );
        assert_eq!(
            block_on(adapter.project(request.editor_session.clone(), 1.into()))
                .expect("unchanged stale projection")
                .body(),
            before
        );
    }

    #[test]
    fn global_replacement_revalidates_body_matches_and_builds_one_atomic_edit() {
        let document = parchmint_domain::DocumentId::from_bytes([4; 16]);
        let node = parchmint_domain::NodeId::from_bytes([3; 16]);
        let mut project =
            parchmint_domain::Project::new(parchmint_domain::ProjectId::from_bytes([1; 16]));
        project
            .nodes
            .try_insert_document(
                node,
                document,
                parchmint_domain::NodeId::manuscript_root(),
                0,
                "Chapter",
            )
            .expect("replacement fixture document inserts");
        let snapshot = parchmint_ui_api::ProjectSnapshot {
            project,
            document_summaries: Vec::new(),
            documents: vec![parchmint_application::DocumentSnapshot {
                comments: Vec::new(),
                document_id: document,
                body: "harbor harbor".to_owned(),
                revision: EditorRevision::from(3),
                visibility: parchmint_application::DocumentVisibility::Open,
            }],
            styles_css: String::new(),
        };
        let document_id = stable_id_string(document.as_bytes());
        let block_id = stable_id_string(&[9; 16]);
        let first = format!("{document_id}:{block_id}:Body:0:6:3");
        let second = format!("{document_id}:{block_id}:Body:7:13:3");
        let results = vec![
            crate::GlobalSearchResult {
                document_id: document_id.clone(),
                match_id: first.clone(),
                prefix: String::new(),
                matching_text: "harbor".to_owned(),
                suffix: " harbor".to_owned(),
                indexed_revision: 3,
            },
            crate::GlobalSearchResult {
                document_id,
                match_id: second.clone(),
                prefix: "harbor ".to_owned(),
                matching_text: "harbor".to_owned(),
                suffix: String::new(),
                indexed_revision: 3,
            },
        ];

        let selection = replacement_selection(&snapshot, &results, &[first, second], "port")
            .expect("current body matches build a replacement selection");

        assert_eq!(selection.edits.len(), 1);
        assert_eq!(selection.edits[0].expected_body, "harbor harbor");
        assert_eq!(selection.edits[0].replacement_body, "port port");
    }

    #[test]
    fn boot_plans_a_real_launcher_and_each_registered_project_window() {
        let project = NativeProjectWindow {
            project: PathBuf::from("/tmp/novel.parchmint"),
            window: WindowCapability::new(4, 1),
            session: parchmint_ui_api::ProjectSessionRegistry::new().register(4),
            project_ui: None,
            editor: None,
        };
        let callbacks = Arc::new(RecordingCallbacks::opening(NativeProjectOpenResult::Locked));

        let (desktop, _open_tasks) = NativeDesktop::boot(NativeDesktopStartup {
            appearance: ResolvedAppearance::Dark,
            appearance_mode: AppearanceMode::Dark,
            recent_projects: Vec::new(),
            projects: vec![project.clone()],
            locked_project: None,
            capture: None,
            callbacks: callbacks.clone(),
        });

        assert_eq!(desktop.windows.len(), 2);
        assert_eq!(desktop.project_windows.len(), 1);
        assert!(desktop.project_windows.contains_key(&project.window));
        assert_eq!(desktop.appearance, ResolvedAppearance::Dark);
        assert_eq!(
            callbacks
                .created
                .lock()
                .expect("created windows mutex poisoned")
                .as_slice(),
            [LAUNCHER_CAPABILITY, project.window]
        );
    }

    #[test]
    fn launcher_project_open_uses_the_desktop_callback_and_adds_the_native_window() {
        let project = NativeProjectWindow {
            project: PathBuf::from("/tmp/routed.parchmint"),
            window: WindowCapability::new(7, 2),
            session: parchmint_ui_api::ProjectSessionRegistry::new().register(7),
            project_ui: None,
            editor: None,
        };
        let callbacks = Arc::new(RecordingCallbacks::opening(
            NativeProjectOpenResult::Opened(project.clone()),
        ));
        let (mut desktop, _open_tasks) = NativeDesktop::boot(NativeDesktopStartup {
            appearance: ResolvedAppearance::Light,
            appearance_mode: AppearanceMode::System,
            recent_projects: Vec::new(),
            projects: Vec::new(),
            locked_project: None,
            capture: None,
            callbacks: callbacks.clone(),
        });
        let _pending_open = desktop.route_project_open(project.project.clone());
        assert!(desktop.opening_project);
        assert_eq!(desktop.windows.len(), 1);
        let _open_task = desktop.update(Message::ProjectOpenFinished {
            project: project.project.clone(),
            result: Ok(NativeProjectOpenResult::Opened(project.clone())),
        });

        assert!(desktop.project_windows.contains_key(&project.window));
        assert_eq!(desktop.windows.len(), 1);
        assert!(
            desktop
                .windows
                .values()
                .all(|window| matches!(window, NativeWindow::Project(_)))
        );
        assert!(desktop.status.is_none());
        assert!(!desktop.opening_project);
    }

    #[test]
    fn missing_recent_project_shows_an_error_without_starting_an_open_operation() {
        let callbacks = Arc::new(RecordingCallbacks::opening(NativeProjectOpenResult::Locked));
        let (mut desktop, _open_tasks) = NativeDesktop::boot(NativeDesktopStartup {
            appearance: ResolvedAppearance::Light,
            appearance_mode: AppearanceMode::System,
            recent_projects: Vec::new(),
            projects: Vec::new(),
            locked_project: None,
            capture: None,
            callbacks,
        });
        let missing = std::env::temp_dir().join(format!(
            "parchmint-missing-recent-project-{}",
            std::process::id()
        ));
        assert!(!missing.exists());
        let expected = format!(
            "The project at {} is no longer available. It may have been moved or deleted.",
            missing.display()
        );

        let _task = desktop.update(Message::OpenRecentProject(missing.clone()));

        assert!(!desktop.opening_project);
        assert_eq!(desktop.status.as_deref(), Some(expected.as_str()));
        let mut simulator = Simulator::<Message>::with_size(
            Settings::default(),
            Size::new(900.0, 620.0),
            launcher_surface(
                &[],
                desktop.launcher.new_project(),
                false,
                false,
                Some(expected.clone()),
            ),
        );
        assert!(simulator.find(expected.as_str()).is_ok());
    }

    #[test]
    fn system_appearance_stream_is_conditional_and_dropped_when_mode_becomes_fixed() {
        let system = streamed_appearance_subscription(
            AppearanceMode::System,
            drop_counting_appearance_stream,
        );
        let fixed =
            streamed_appearance_subscription(AppearanceMode::Dark, drop_counting_appearance_stream);
        assert_eq!(
            iced_test::runtime::futures::subscription::into_recipes(system).len(),
            1
        );
        assert!(iced_test::runtime::futures::subscription::into_recipes(fixed).is_empty());

        APPEARANCE_STREAM_DROPS.store(0, Ordering::Relaxed);
        let mut tracker = iced_test::runtime::futures::subscription::Tracker::new();
        let (sender, _receiver) = iced::futures::channel::mpsc::channel(1);
        let running = tracker.update(
            iced_test::runtime::futures::subscription::into_recipes(
                streamed_appearance_subscription(
                    AppearanceMode::System,
                    drop_counting_appearance_stream,
                ),
            )
            .into_iter(),
            sender.clone(),
        );
        assert_eq!(running.len(), 1);
        assert_eq!(APPEARANCE_STREAM_DROPS.load(Ordering::Relaxed), 0);

        let canceled = tracker.update(
            iced_test::runtime::futures::subscription::into_recipes(
                streamed_appearance_subscription(
                    AppearanceMode::Light,
                    drop_counting_appearance_stream,
                ),
            )
            .into_iter(),
            sender,
        );
        assert!(canceled.is_empty());
        block_on(running.into_iter().next().expect("appearance execution"));
        assert_eq!(APPEARANCE_STREAM_DROPS.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn fixed_mode_ignores_system_events_without_starting_callback_work() {
        let callbacks = Arc::new(RecordingCallbacks::opening(NativeProjectOpenResult::Locked));
        let (mut desktop, _boot) = NativeDesktop::boot(NativeDesktopStartup {
            appearance: ResolvedAppearance::Dark,
            appearance_mode: AppearanceMode::Dark,
            recent_projects: Vec::new(),
            projects: Vec::new(),
            locked_project: None,
            capture: None,
            callbacks: callbacks.clone(),
        });

        let task = desktop.update(Message::SystemAppearanceObserved(SystemAppearance::Light));

        assert!(iced_test::runtime::task::into_stream(task).is_none());
        assert_eq!(desktop.appearance, ResolvedAppearance::Dark);
        assert!(
            callbacks
                .system_appearances
                .lock()
                .expect("system appearances mutex poisoned")
                .is_empty()
        );
    }

    #[test]
    fn one_appearance_result_rethemes_launcher_and_every_project_window() {
        let mut registry = parchmint_ui_api::ProjectSessionRegistry::new();
        let projects = vec![
            NativeProjectWindow {
                project: PathBuf::from("/tmp/first.parchmint"),
                window: WindowCapability::new(11, 1),
                session: registry.register(11),
                project_ui: None,
                editor: None,
            },
            NativeProjectWindow {
                project: PathBuf::from("/tmp/second.parchmint"),
                window: WindowCapability::new(12, 1),
                session: registry.register(12),
                project_ui: None,
                editor: None,
            },
        ];
        let callbacks = Arc::new(RecordingCallbacks::opening(NativeProjectOpenResult::Locked));
        let (mut desktop, _open_tasks) = NativeDesktop::boot(NativeDesktopStartup {
            appearance: ResolvedAppearance::Light,
            appearance_mode: AppearanceMode::System,
            recent_projects: Vec::new(),
            projects,
            locked_project: None,
            capture: None,
            callbacks,
        });

        let _appearance = desktop.update(Message::AppearanceFinished {
            mode: AppearanceMode::Dark,
            result: Ok(ResolvedAppearance::Dark),
        });

        assert_eq!(desktop.appearance, ResolvedAppearance::Dark);
        let expected = ParchMintTheme::new(ResolvedAppearance::Dark).iced_theme();
        for id in desktop.windows.keys().copied() {
            assert_eq!(desktop.theme(id), expected);
        }
    }

    #[test]
    fn ordered_system_events_retheme_all_windows_only_when_the_controller_accepts_them() {
        let mut registry = parchmint_ui_api::ProjectSessionRegistry::new();
        let projects = vec![
            NativeProjectWindow {
                project: PathBuf::from("/tmp/system-first.parchmint"),
                window: WindowCapability::new(21, 1),
                session: registry.register(21),
                project_ui: None,
                editor: None,
            },
            NativeProjectWindow {
                project: PathBuf::from("/tmp/system-second.parchmint"),
                window: WindowCapability::new(22, 1),
                session: registry.register(22),
                project_ui: None,
                editor: None,
            },
        ];
        let callbacks = Arc::new(RecordingCallbacks::opening(NativeProjectOpenResult::Locked));
        let (mut desktop, _open_tasks) = NativeDesktop::boot(NativeDesktopStartup {
            appearance: ResolvedAppearance::Light,
            appearance_mode: AppearanceMode::System,
            recent_projects: Vec::new(),
            projects,
            locked_project: None,
            capture: None,
            callbacks: callbacks.clone(),
        });
        *callbacks
            .system_appearance_result
            .lock()
            .expect("system appearance result mutex poisoned") = Ok(Some(ResolvedAppearance::Dark));

        let _event = desktop.update(Message::SystemAppearanceObserved(SystemAppearance::Dark));
        let _completion = desktop.update(Message::SystemAppearanceChangedFinished {
            generation: 1,
            result: Ok(Some(ResolvedAppearance::Dark)),
        });
        assert_eq!(desktop.appearance, ResolvedAppearance::Dark);
        let expected = ParchMintTheme::new(ResolvedAppearance::Dark).iced_theme();
        for id in desktop.windows.keys().copied() {
            assert_eq!(desktop.theme(id), expected);
        }

        *callbacks
            .system_appearance_result
            .lock()
            .expect("system appearance result mutex poisoned") = Ok(None);
        let _system_only =
            desktop.update(Message::SystemAppearanceObserved(SystemAppearance::Light));
        let _completion = desktop.update(Message::SystemAppearanceChangedFinished {
            generation: 2,
            result: Ok(None),
        });
        let _duplicate = desktop.update(Message::SystemAppearanceObserved(SystemAppearance::Dark));

        assert_eq!(desktop.appearance, ResolvedAppearance::Dark);
        assert_eq!(desktop.last_appearance_generation, 3);
    }

    #[test]
    fn closing_last_project_then_launcher_emits_native_close_and_final_exit_actions() {
        let project = NativeProjectWindow {
            project: PathBuf::from("/tmp/shutdown-actions.parchmint"),
            window: WindowCapability::new(71, 1),
            session: parchmint_ui_api::ProjectSessionRegistry::new().register(71),
            project_ui: None,
            editor: None,
        };
        let callbacks = Arc::new(RecordingCallbacks::opening(NativeProjectOpenResult::Locked));
        let (mut desktop, _boot) = NativeDesktop::boot(NativeDesktopStartup {
            appearance: ResolvedAppearance::Light,
            appearance_mode: AppearanceMode::System,
            recent_projects: Vec::new(),
            projects: vec![project.clone()],
            locked_project: None,
            capture: None,
            callbacks,
        });
        let project_window = desktop.project_windows[&project.window];
        let launcher_window = desktop
            .windows
            .iter()
            .find_map(|(id, window)| matches!(window, NativeWindow::Launcher).then_some(*id))
            .expect("launcher window");

        let (closed, exits) = native_close_actions(desktop.finish_close(project_window));
        assert_eq!(closed, vec![project_window]);
        assert_eq!(exits, 0);
        assert_eq!(desktop.windows.len(), 1);

        let (closed, exits) =
            native_close_actions(desktop.update(Message::CloseRequested(launcher_window)));
        assert_eq!(closed, vec![launcher_window]);
        assert_eq!(exits, 1);
        assert!(desktop.windows.is_empty());
    }

    #[test]
    fn final_save_completion_controls_when_a_project_window_is_removed() {
        let project = NativeProjectWindow {
            project: PathBuf::from("/tmp/closing.parchmint"),
            window: WindowCapability::new(8, 3),
            session: parchmint_ui_api::ProjectSessionRegistry::new().register(8),
            project_ui: None,
            editor: None,
        };
        let other = NativeProjectWindow {
            project: PathBuf::from("/tmp/stays-open.parchmint"),
            window: WindowCapability::new(9, 1),
            session: parchmint_ui_api::ProjectSessionRegistry::new().register(9),
            project_ui: None,
            editor: None,
        };
        let callbacks = Arc::new(RecordingCallbacks::opening(NativeProjectOpenResult::Locked));
        let (mut desktop, _open_tasks) = NativeDesktop::boot(NativeDesktopStartup {
            appearance: ResolvedAppearance::Light,
            appearance_mode: AppearanceMode::System,
            recent_projects: Vec::new(),
            projects: vec![project.clone(), other.clone()],
            locked_project: None,
            capture: None,
            callbacks: callbacks.clone(),
        });
        let native_window = desktop.project_windows[&project.window];
        desktop.closing_windows.insert(native_window);

        let _failed_close = desktop.update(Message::ProjectCloseFinished {
            window: native_window,
            result: Err("save failed".to_owned()),
        });
        assert!(desktop.windows.contains_key(&native_window));
        assert!(!desktop.closing_windows.contains(&native_window));
        assert_eq!(
            desktop
                .close_failures
                .get(&project.window)
                .map(String::as_str),
            Some("save failed")
        );
        assert!(!desktop.close_failures.contains_key(&other.window));

        let _cancel = desktop.update(Message::CancelClose(native_window));
        assert!(!desktop.close_failures.contains_key(&project.window));

        let _failed_again = desktop.update(Message::ProjectCloseFinished {
            window: native_window,
            result: Err("save failed again".to_owned()),
        });
        assert_eq!(
            desktop
                .close_failures
                .get(&project.window)
                .map(String::as_str),
            Some("save failed again")
        );

        desktop.closing_windows.insert(native_window);
        let _successful_close = desktop.update(Message::ProjectCloseFinished {
            window: native_window,
            result: Ok(ProjectionRun {
                projected: BTreeMap::new(),
                result: Ok(CloseCompletion {
                    snapshot: None,
                    result: Ok(()),
                }),
            }),
        });
        assert!(!desktop.windows.contains_key(&native_window));
        assert!(!desktop.project_windows.contains_key(&project.window));
        assert_eq!(
            callbacks
                .destroyed
                .lock()
                .expect("destroyed windows mutex poisoned")
                .as_slice(),
            [project.window]
        );
        assert!(desktop.project_windows.contains_key(&other.window));
    }

    #[test]
    fn recent_project_timestamp_is_rendered_as_a_date_and_time() {
        assert_eq!(
            format_last_opened(1_704_067_140),
            "on 2023-12-31 at 23:59 UTC"
        );
        assert_eq!(format_last_opened(0), "at an unknown time");
    }
}
