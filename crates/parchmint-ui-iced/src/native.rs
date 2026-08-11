//! Native Iced event-loop integration for the desktop executable.

use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    fmt,
    hash::{Hash, Hasher},
    path::PathBuf,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use iced::{
    Element, Event, Length, Subscription, Task, Theme, event,
    futures::{SinkExt, StreamExt, channel::mpsc as futures_mpsc},
    keyboard, mouse,
    widget::{button, column, container, row, text, text_input},
    window,
};
use parchmint_application::{ReplacementEdit, ReplacementSelection};
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
    ClipboardContent, ClipboardFormats, MenuActivation, MenuActivationService, MenuCommand,
    MenuService, PathDialog, PathDialogKind, SemanticMenu, SemanticMenuEntry, SystemAppearance,
    SystemAppearanceEvent, SystemAppearanceEventService, UntrustedClipboardContent,
    WindowCapability, WindowResult,
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
    NewProjectDraft, ProjectEffect, ProjectMessage, ProjectTask, ProjectTaskCompletion,
    ProjectTaskPayload, ProjectTaskTicket, ProjectWorkspace, RecentProject, RibbonDestination,
    SelectionGesture, Shell, ShellLayout, SpellingDecoration, SpellingMenu, SpellingMenuAction,
    SpellingMenuRequest,
    async_service_feeds::{
        AsyncServiceFeeds, BlockingServiceJob, DeletedPreviewResult, HistoryListResult,
        HistoryPreviewResult, RecoveryAcceptanceTicket, RecoveryAcceptedResult,
        RecoveryDiscardedResult, RecoveryReconcileResult, SearchBatchResult, SearchRequest,
        SearchStart,
    },
    design_tokens::ParchMintTheme,
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
            | Event::Mouse(mouse::Event::CursorMoved { .. })
            | Event::Mouse(mouse::Event::ButtonReleased(_))
            | Event::Window(window::Event::Resized(_))
            | Event::Window(window::Event::Focused)
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

/// Raw window values copied only inside Iced's event-loop-owned
/// [`window::run`] callback and consumed synchronously by the native adapter.
#[doc(hidden)]
#[derive(Debug, Clone, Copy)]
pub struct NativeWindowAttachment {
    pub raw_window: window::raw_window_handle::RawWindowHandle,
    pub raw_display: window::raw_window_handle::RawDisplayHandle,
}

/// How an installed semantic menu is presented on the current target.
#[doc(hidden)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NativeMenuAttachment {
    Native,
    InWindow,
}

/// Desktop lifecycle callbacks invoked by native window interactions.
pub trait NativeDesktopCallbacks: Send + Sync {
    fn open_project(&self, project: PathBuf) -> Result<NativeProjectOpenResult, String>;
    fn close_project(&self, project: PathBuf) -> Result<(), String>;

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

    /// Accepts a platform-delivered OS appearance event after the native
    /// subscription has preserved its source ordering.
    fn system_appearance_changed(
        &self,
        _appearance: ResolvedAppearance,
    ) -> Result<Option<ResolvedAppearance>, String> {
        Ok(None)
    }

    /// Supplies the optional platform event stream to the native driver.
    fn system_appearance_events(&self) -> Option<Arc<dyn SystemAppearanceEventService>> {
        None
    }

    /// Supplies semantic menu installation without exposing a native handle.
    fn menu_service(&self) -> Option<Arc<dyn MenuService>> {
        None
    }

    /// Supplies the typed activation source fed by native menu callbacks.
    fn menu_activations(&self) -> Option<Arc<dyn MenuActivationService>> {
        None
    }

    /// Attaches an installed binding while Iced owns a live window callback.
    fn attach_menu(
        &self,
        _window: WindowCapability,
        _binding: u64,
        _attachment: NativeWindowAttachment,
    ) -> Result<NativeMenuAttachment, String> {
        Err("native menu attachment is unavailable".to_owned())
    }

    /// Removes native menu state while Iced still owns the live window.
    fn detach_menu(
        &self,
        _window: WindowCapability,
        _attachment: NativeWindowAttachment,
    ) -> Result<(), String> {
        Ok(())
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
    pub recent_projects: Vec<PreferenceRecentProject>,
    pub projects: Vec<NativeProjectWindow>,
    pub locked_project: Option<PathBuf>,
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
    .map_err(|error| NativeDesktopError::new(error.to_string()))
}

#[derive(Debug, Clone)]
enum Message {
    WindowOpened,
    RuntimeEvent {
        window: window::Id,
        event: Event,
        accelerator_fallback: bool,
    },
    MenuInstalled {
        capability: WindowCapability,
        result: Result<u64, String>,
    },
    MenuAttached {
        capability: WindowCapability,
        binding: u64,
        result: Result<NativeMenuAttachment, String>,
    },
    MenuDetached(Result<(), String>),
    ToggleInWindowMenu {
        capability: WindowCapability,
        label: String,
    },
    InWindowMenuActivation(MenuActivation),
    MenuActivation(MenuActivation),
    MenuActivationStreamFailed(String),
    WorkspaceLoaded {
        window: window::Id,
        result: Result<Option<WorkspaceSnapshot>, String>,
    },
    WorkspacePersisted {
        result: Result<(), String>,
    },
    CloseRequested(window::Id),
    ShowNewProject,
    NewProjectTitleChanged(String),
    NewProjectDestinationChanged(String),
    NewProjectAuthorChanged(String),
    ChooseOpenProject,
    ChooseNewProjectDestination,
    DirectoryChosen {
        create: bool,
        result: Result<Option<PathBuf>, String>,
    },
    OpenProject(PathBuf),
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
        revision: u64,
        result: Result<ProjectSnapshot, String>,
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
    SystemAppearanceEvent(SystemAppearanceEvent),
    SystemAppearanceChangedFinished {
        generation: u64,
        result: Result<Option<ResolvedAppearance>, String>,
    },
    SystemAppearanceStreamFailed(String),
    SaveFinished {
        window: window::Id,
        result: Result<u64, String>,
    },
    ProjectEffectFinished {
        window: window::Id,
        history_action: Option<HistoryWorkflowAction>,
        result: Result<ProjectEffectCompletion, ProjectRuntimeError>,
    },
    EditorEffectFinished {
        window: window::Id,
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
        result: Result<RecoveryAcceptedResult, String>,
    },
    RecoveryDiscarded {
        window: window::Id,
        session: ProjectSessionCapability,
        ticket: ProjectTaskTicket,
        result: Result<RecoveryDiscardedResult, String>,
    },
    SelectDestination {
        window: window::Id,
        destination: RibbonDestination,
    },
    AppearanceSelected(AppearanceMode),
    AppearanceFinished(Result<ResolvedAppearance, String>),
    RetryClose(window::Id),
    CancelClose(window::Id),
    ProjectCloseFinished {
        window: window::Id,
        result: Result<(), String>,
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

#[derive(Clone)]
struct AppearanceEventSubscription(Arc<dyn SystemAppearanceEventService>);

#[derive(Clone)]
struct MenuActivationSubscription(Arc<dyn MenuActivationService>);

impl Hash for MenuActivationSubscription {
    fn hash<H: Hasher>(&self, state: &mut H) {
        Arc::as_ptr(&self.0).cast::<()>().hash(state);
    }
}

fn menu_activation_subscription(
    subscription: &MenuActivationSubscription,
) -> iced::futures::stream::BoxStream<'static, Message> {
    let service = Arc::clone(&subscription.0);
    Box::pin(iced::stream::channel(1, async move |mut output| {
        let stream = match service.subscribe() {
            Ok(stream) => stream,
            Err(error) => {
                let _ = output
                    .send(Message::MenuActivationStreamFailed(error.to_string()))
                    .await;
                return;
            }
        };
        loop {
            match stream.next_timeout(Duration::from_secs(1)) {
                Ok(Some(activation)) => {
                    if output
                        .send(Message::MenuActivation(activation))
                        .await
                        .is_err()
                    {
                        break;
                    }
                }
                Ok(None) => {}
                Err(error) => {
                    let _ = output
                        .send(Message::MenuActivationStreamFailed(error.to_string()))
                        .await;
                    break;
                }
            }
        }
    }))
}

impl Hash for AppearanceEventSubscription {
    fn hash<H: Hasher>(&self, state: &mut H) {
        Arc::as_ptr(&self.0).cast::<()>().hash(state);
    }
}

fn appearance_event_subscription(
    subscription: &AppearanceEventSubscription,
) -> iced::futures::stream::BoxStream<'static, Message> {
    let service = Arc::clone(&subscription.0);
    Box::pin(iced::stream::channel(1, async move |mut output| {
        let stream = match service.subscribe() {
            Ok(stream) => stream,
            Err(error) => {
                let _ = output
                    .send(Message::SystemAppearanceStreamFailed(error.to_string()))
                    .await;
                return;
            }
        };
        loop {
            match stream.next_timeout(Duration::from_secs(1)) {
                Ok(Some(event)) => {
                    if output
                        .send(Message::SystemAppearanceEvent(event))
                        .await
                        .is_err()
                    {
                        break;
                    }
                }
                Ok(None) => {}
                Err(error) => {
                    let _ = output
                        .send(Message::SystemAppearanceStreamFailed(error.to_string()))
                        .await;
                    break;
                }
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

fn semantic_desktop_menu(
    project_window: bool,
    save_enabled: bool,
    edit_enabled: bool,
) -> SemanticMenu {
    let command = |id, label, enabled| {
        SemanticMenuEntry::Command(if enabled {
            MenuCommand::new(id, label)
        } else {
            MenuCommand::disabled(id, label)
        })
    };
    SemanticMenu::new(vec![
        SemanticMenuEntry::Submenu {
            label: "File".to_owned(),
            entries: vec![
                command("file.new", "New Project", true),
                command("file.open", "Open Project…", true),
                SemanticMenuEntry::Separator,
                command("file.save", "Save", project_window && save_enabled),
                command("file.close", "Close", project_window),
            ],
        },
        SemanticMenuEntry::Submenu {
            label: "Edit".to_owned(),
            entries: vec![
                command("edit.undo", "Undo", edit_enabled),
                command("edit.redo", "Redo", edit_enabled),
                SemanticMenuEntry::Separator,
                command("edit.copy", "Copy", edit_enabled),
                command("edit.cut", "Cut", edit_enabled),
                command("edit.paste", "Paste", edit_enabled),
            ],
        },
    ])
}

#[cfg(target_os = "linux")]
fn linux_menu_bar(
    capability: WindowCapability,
    binding: u64,
    menu: &SemanticMenu,
    open: Option<&str>,
) -> Element<'static, Message> {
    let mut roots = row![].spacing(2).height(28);
    let mut open_entries = None;
    for entry in menu.entries() {
        let SemanticMenuEntry::Submenu { label, entries } = entry else {
            continue;
        };
        roots = roots.push(
            button(text(label.clone()).size(13))
                .padding([4, 10])
                .height(28)
                .on_press(Message::ToggleInWindowMenu {
                    capability,
                    label: label.clone(),
                }),
        );
        if open == Some(label.as_str()) {
            open_entries = Some(entries);
        }
    }

    let mut bar = column![container(roots).padding([0, 6]).width(Length::Fill)].spacing(0);
    if let Some(entries) = open_entries {
        let mut commands = row![].spacing(2).height(30);
        for entry in entries {
            match entry {
                SemanticMenuEntry::Command(command) => {
                    let accelerator = menu_accelerator(command.id())
                        .map_or_else(String::new, |value| format!("  {value}"));
                    let item = button(text(format!("{}{}", command.label(), accelerator)).size(12))
                        .padding([4, 10])
                        .height(28);
                    commands = commands.push(if command.enabled() {
                        item.on_press(Message::InWindowMenuActivation(MenuActivation {
                            binding: WindowResult::new(capability, binding),
                            command_id: command.id().to_owned(),
                        }))
                    } else {
                        item
                    });
                }
                SemanticMenuEntry::Separator => {
                    commands = commands.push(text("│").size(16));
                }
                SemanticMenuEntry::Submenu { .. } => {}
            }
        }
        bar = bar.push(container(commands).padding([1, 8]).width(Length::Fill));
    }
    container(bar).width(Length::Fill).into()
}

#[cfg(target_os = "linux")]
fn menu_accelerator(command: &str) -> Option<&'static str> {
    match command {
        "file.open" => Some("Ctrl+O"),
        "file.save" => Some("Ctrl+S"),
        "file.new" => Some("Ctrl+N"),
        "file.close" => Some("Ctrl+W"),
        "edit.copy" => Some("Ctrl+C"),
        "edit.cut" => Some("Ctrl+X"),
        "edit.paste" => Some("Ctrl+V"),
        "edit.undo" => Some("Ctrl+Z"),
        "edit.redo" => Some("Ctrl+Y"),
        _ => None,
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
    launcher: LauncherState,
    windows: BTreeMap<window::Id, NativeWindow>,
    project_windows: BTreeMap<WindowCapability, window::Id>,
    closing_windows: BTreeSet<window::Id>,
    close_failures: BTreeMap<WindowCapability, String>,
    opening_project: bool,
    creating_project: bool,
    status: Option<String>,
    callbacks: Arc<dyn NativeDesktopCallbacks>,
    menu_service: Option<Arc<dyn MenuService>>,
    menu_activations: Option<Arc<dyn MenuActivationService>>,
    menu_bindings: BTreeMap<WindowCapability, u64>,
    in_window_menus: BTreeMap<WindowCapability, u64>,
    open_in_window_menu: Option<(WindowCapability, String)>,
    appearance_events: Option<Arc<dyn SystemAppearanceEventService>>,
    last_appearance_generation: u64,
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
    effect_executor: Option<NativeProjectEffectExecutor>,
    service_feeds: Option<AsyncServiceFeeds>,
    recovery_acceptance: Option<RecoveryAcceptanceTicket>,
    active_export: Option<ExportOperationToken>,
    export_destination: Option<parchmint_platform_api::UntrustedPathSelection>,
    autosave: AutosaveState,
    next_spellcheck_generation: u64,
    spellcheck_generation: BTreeMap<ViewId, u64>,
    spelling_issues: BTreeMap<ViewId, Vec<NativeSpellingIssue>>,
    pending_spelling_menu: Option<NativeSpellingMenuContext>,
    spelling_menu: Option<SpellingMenu>,
    refresh_spellcheck_view: Option<ViewId>,
    modifiers: keyboard::Modifiers,
    resizing: Option<SidebarPanel>,
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
    through_revision: u64,
    save_in_flight: bool,
}

impl AutosaveState {
    const IDLE_DELAY: Duration = Duration::from_millis(1_500);
    const CONTINUOUS_LIMIT: Duration = Duration::from_secs(30);

    fn mark_dirty(&mut self, revision: u64, now: Instant) {
        self.first_dirty.get_or_insert(now);
        self.last_edit = Some(now);
        self.through_revision = self.through_revision.max(revision);
    }

    fn should_save(&self, now: Instant) -> bool {
        !self.save_in_flight
            && self.first_dirty.is_some_and(|first| {
                now.saturating_duration_since(first) >= Self::CONTINUOUS_LIMIT
                    || self
                        .last_edit
                        .is_some_and(|last| now.saturating_duration_since(last) >= Self::IDLE_DELAY)
            })
    }
}

impl NativeDesktop {
    fn boot(startup: NativeDesktopStartup) -> (Self, Task<Message>) {
        let appearance_events = startup.callbacks.system_appearance_events();
        let menu_service = startup.callbacks.menu_service();
        let menu_activations = startup.callbacks.menu_activations();
        let mut desktop = Self {
            appearance: startup.appearance,
            launcher: LauncherState::default(),
            windows: BTreeMap::new(),
            project_windows: BTreeMap::new(),
            closing_windows: BTreeSet::new(),
            close_failures: BTreeMap::new(),
            opening_project: false,
            creating_project: false,
            status: startup
                .locked_project
                .map(|path| format!("Project is already open: {}", path.display())),
            callbacks: startup.callbacks,
            menu_service,
            menu_activations,
            menu_bindings: BTreeMap::new(),
            in_window_menus: BTreeMap::new(),
            open_in_window_menu: None,
            appearance_events,
            last_appearance_generation: 0,
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
            Message::WindowOpened => Task::none(),
            Message::RuntimeEvent {
                window,
                event,
                accelerator_fallback,
            } => self.runtime_event(window, event, accelerator_fallback),
            Message::MenuInstalled { capability, result } => {
                match result {
                    Ok(binding)
                        if self.project_windows.contains_key(&capability)
                            || capability == LAUNCHER_CAPABILITY =>
                    {
                        if self
                            .menu_bindings
                            .get(&capability)
                            .is_none_or(|current| *current < binding)
                        {
                            self.menu_bindings.insert(capability, binding);
                            self.in_window_menus.remove(&capability);
                            if self
                                .open_in_window_menu
                                .as_ref()
                                .is_some_and(|(window, _)| *window == capability)
                            {
                                self.open_in_window_menu = None;
                            }
                            return self.attach_menu(capability, binding);
                        }
                    }
                    Ok(_) => {}
                    Err(error) => self.status = Some(error),
                }
                Task::none()
            }
            Message::MenuAttached {
                capability,
                binding,
                result,
            } => {
                if self.menu_bindings.get(&capability) != Some(&binding) {
                    return Task::none();
                }
                match result {
                    Ok(NativeMenuAttachment::Native) => {
                        self.in_window_menus.remove(&capability);
                    }
                    Ok(NativeMenuAttachment::InWindow) => {
                        self.in_window_menus.insert(capability, binding);
                    }
                    Err(error) => self.status = Some(error),
                }
                Task::none()
            }
            Message::MenuDetached(result) => {
                if let Err(error) = result {
                    self.status = Some(error);
                }
                Task::none()
            }
            Message::ToggleInWindowMenu { capability, label } => {
                let requested = (capability, label);
                self.open_in_window_menu =
                    (self.open_in_window_menu.as_ref() != Some(&requested)).then_some(requested);
                Task::none()
            }
            Message::InWindowMenuActivation(activation) => {
                self.open_in_window_menu = None;
                self.activate_menu(activation)
            }
            Message::MenuActivation(activation) => self.activate_menu(activation),
            Message::MenuActivationStreamFailed(error) => {
                self.status = Some(error);
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
            Message::OpenProject(project) => self.route_project_open(project),
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
                revision,
                result,
            } => {
                match result {
                    Ok(snapshot) => {
                        self.status = Some(format!(
                            "Recovery is durable through editor revision {revision}."
                        ));
                        if let Some(NativeWindow::Project(state)) = self.windows.get_mut(&window) {
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
                            if let Some(workspace) = state.workspace.as_mut() {
                                workspace.reconcile_snapshot(&snapshot);
                            }
                        }
                    }
                    Err(error) => {
                        self.status = Some(error.clone());
                        if let Some(NativeWindow::Project(state)) = self.windows.get_mut(&window)
                            && let Some(workspace) = state.workspace.as_mut()
                        {
                            workspace.update(ProjectMessage::SaveFailed(error));
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
            Message::SystemAppearanceEvent(event) => self.system_appearance_event(event),
            Message::SystemAppearanceChangedFinished { generation, result } => {
                if generation != self.last_appearance_generation {
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
            Message::SaveFinished { window, result } => {
                if let Some(NativeWindow::Project(state)) = self.windows.get_mut(&window)
                    && let Some(workspace) = state.workspace.as_mut()
                {
                    state.autosave.save_in_flight = false;
                    match result {
                        Ok(revision) => {
                            workspace.update(ProjectMessage::SaveCompleted(revision));
                            if revision >= state.autosave.through_revision {
                                state.autosave.first_dirty = None;
                                state.autosave.last_edit = None;
                            } else {
                                state.autosave.first_dirty = Some(Instant::now());
                            }
                            self.status = None;
                        }
                        Err(error) => {
                            workspace.update(ProjectMessage::SaveFailed(error.clone()));
                            self.status = Some(error);
                        }
                    }
                }
                self.refresh_menu(window)
            }
            Message::ProjectEffectFinished {
                window,
                history_action,
                result,
            } => self.finish_project_effect(window, history_action, result),
            Message::EditorEffectFinished { window, result } => {
                self.finish_editor_effect(window, result)
            }
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
            Message::HistoryReinitialized { window, result } => {
                let Some(NativeWindow::Project(state)) = self.windows.get_mut(&window) else {
                    return Task::none();
                };
                let Some(workspace) = state.workspace.as_mut() else {
                    return Task::none();
                };
                match result {
                    Ok(message) => {
                        workspace.update(ProjectMessage::HistoryReinitialized(message));
                    }
                    Err(error) => workspace.fail_history_workflow(error),
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
                            workspace.reconcile_snapshot(&snapshot);
                        }
                        let Some(ports) = state.project.ports().cloned() else {
                            return Task::none();
                        };
                        Self::save_task(window, ports, ProjectSaveKind::Structural)
                    }
                    Err(error) => {
                        let accepted = state.workspace.as_mut().is_some_and(|workspace| {
                            workspace.accept_completion(ProjectTaskCompletion::for_ticket(
                                ticket,
                                ProjectTaskPayload::Failed(error.clone()),
                            ))
                        });
                        if accepted {
                            self.status = Some(error);
                        }
                        Task::none()
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
                result,
            } => {
                let mut recovered_document = None;
                let mut activate = false;
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
                if activate {
                    self.status = None;
                    self.activate_reconciled_project(window, recovered_document)
                } else {
                    Task::none()
                }
            }
            Message::RecoveryDiscarded {
                window,
                session,
                ticket,
                result,
            } => {
                let mut activate = false;
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
                if activate {
                    self.status = None;
                    self.activate_reconciled_project(window, None)
                } else {
                    Task::none()
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
                    Message::AppearanceFinished,
                )
            }
            Message::AppearanceFinished(result) => {
                match result {
                    Ok(appearance) => {
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
                if let Some(NativeWindow::Project(state)) = self.windows.get(&window) {
                    self.close_failures.remove(&state.project.window);
                }
                Task::none()
            }
            Message::ProjectCloseFinished { window, result } => match result {
                Ok(()) => self.finish_close(window),
                Err(error) => {
                    self.closing_windows.remove(&window);
                    if let Some(NativeWindow::Project(state)) = self.windows.get(&window) {
                        self.close_failures.insert(state.project.window, error);
                    }
                    Task::none()
                }
            },
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
        self.with_in_window_menu(id, content)
    }

    fn with_in_window_menu<'a>(
        &self,
        id: window::Id,
        content: Element<'a, Message>,
    ) -> Element<'a, Message> {
        #[cfg(target_os = "linux")]
        {
            let Some(capability) = self.capability_for_window(id) else {
                return content;
            };
            let Some(binding) = self.in_window_menus.get(&capability).copied() else {
                return content;
            };
            let menu = match self.windows.get(&id) {
                Some(NativeWindow::Launcher) => semantic_desktop_menu(false, false, false),
                Some(NativeWindow::Project(state)) => semantic_desktop_menu(
                    true,
                    !state.autosave.save_in_flight,
                    !state.editor_bindings.is_empty(),
                ),
                None => return content,
            };
            let open = self
                .open_in_window_menu
                .as_ref()
                .filter(|(window, _)| *window == capability)
                .map(|(_, label)| label.as_str());
            column![linux_menu_bar(capability, binding, &menu, open), content]
                .spacing(0)
                .width(Length::Fill)
                .height(Length::Fill)
                .into()
        }
        #[cfg(not(target_os = "linux"))]
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
        if let Some(events) = &self.appearance_events {
            subscriptions.push(Subscription::run_with(
                AppearanceEventSubscription(Arc::clone(events)),
                appearance_event_subscription,
            ));
        }
        if let Some(activations) = &self.menu_activations {
            subscriptions.push(Subscription::run_with(
                MenuActivationSubscription(Arc::clone(activations)),
                menu_activation_subscription,
            ));
        }
        Subscription::batch(subscriptions)
    }

    fn capability_for_window(&self, id: window::Id) -> Option<WindowCapability> {
        match self.windows.get(&id) {
            Some(NativeWindow::Launcher) => Some(LAUNCHER_CAPABILITY),
            Some(NativeWindow::Project(state)) => Some(state.project.window),
            None => None,
        }
    }

    fn refresh_menu(&self, id: window::Id) -> Task<Message> {
        let Some(service) = self.menu_service.as_ref().cloned() else {
            return Task::none();
        };
        let Some(capability) = self.capability_for_window(id) else {
            return Task::none();
        };
        let menu = match self.windows.get(&id) {
            Some(NativeWindow::Launcher) => semantic_desktop_menu(false, false, false),
            Some(NativeWindow::Project(state)) => semantic_desktop_menu(
                true,
                !state.autosave.save_in_flight,
                !state.editor_bindings.is_empty(),
            ),
            None => return Task::none(),
        };
        Task::perform(
            async move {
                let binding = service
                    .install(capability, menu)
                    .await
                    .map_err(|error| error.to_string())?;
                if binding.window() != capability {
                    return Err("menu install returned for a stale window".to_owned());
                }
                Ok(binding.into_value())
            },
            move |result| Message::MenuInstalled { capability, result },
        )
    }

    fn attach_menu(&self, capability: WindowCapability, binding: u64) -> Task<Message> {
        let id = if capability == LAUNCHER_CAPABILITY {
            self.windows
                .iter()
                .find_map(|(id, window)| matches!(window, NativeWindow::Launcher).then_some(*id))
        } else {
            self.project_windows.get(&capability).copied()
        };
        let Some(id) = id else {
            return Task::none();
        };
        let callbacks = Arc::clone(&self.callbacks);
        window::run(id, move |window| {
            let raw_window = window
                .window_handle()
                .map_err(|error| format!("native window handle is unavailable: {error}"))?
                .as_raw();
            let raw_display = window
                .display_handle()
                .map_err(|error| format!("native display handle is unavailable: {error}"))?
                .as_raw();
            callbacks.attach_menu(
                capability,
                binding,
                NativeWindowAttachment {
                    raw_window,
                    raw_display,
                },
            )
        })
        .map(move |result| Message::MenuAttached {
            capability,
            binding,
            result,
        })
    }

    fn detach_menu(&self, id: window::Id, capability: WindowCapability) -> Task<Message> {
        let callbacks = Arc::clone(&self.callbacks);
        window::run(id, move |window| {
            let raw_window = window
                .window_handle()
                .map_err(|error| format!("native window handle is unavailable: {error}"))?
                .as_raw();
            let raw_display = window
                .display_handle()
                .map_err(|error| format!("native display handle is unavailable: {error}"))?
                .as_raw();
            callbacks.detach_menu(
                capability,
                NativeWindowAttachment {
                    raw_window,
                    raw_display,
                },
            )
        })
        .map(Message::MenuDetached)
    }

    fn activate_menu(&mut self, activation: MenuActivation) -> Task<Message> {
        let capability = activation.binding.window();
        if self.menu_bindings.get(&capability) != Some(activation.binding.value()) {
            self.status = Some("Ignored a stale menu activation.".to_owned());
            return Task::none();
        }
        let id = if capability == LAUNCHER_CAPABILITY {
            self.windows
                .iter()
                .find_map(|(id, window)| matches!(window, NativeWindow::Launcher).then_some(*id))
        } else {
            self.project_windows.get(&capability).copied()
        };
        let Some(id) = id else {
            self.status = Some("Ignored a menu activation for a closed window.".to_owned());
            return Task::none();
        };
        self.activate_menu_command(id, &activation.command_id)
    }

    fn activate_menu_command(&mut self, id: window::Id, command: &str) -> Task<Message> {
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
                self.status = Some(format!("Unknown menu command: {command}"));
                Task::none()
            }
        };
        Task::batch([task, self.refresh_menu(id)])
    }

    fn runtime_event(
        &mut self,
        id: window::Id,
        event: Event,
        accelerator_fallback: bool,
    ) -> Task<Message> {
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
        if accelerator_fallback
            && let Event::Keyboard(keyboard::Event::KeyPressed {
                key: keyboard::Key::Character(key),
                modifiers,
                repeat: false,
                ..
            }) = &event
            && let Some(command) = keyboard_accelerator(key, *modifiers)
        {
            return self.activate_menu_command(id, command);
        }
        if matches!(event, Event::Window(window::Event::Focused)) {
            return self.refresh_menu(id);
        }
        let Some(NativeWindow::Project(state)) = self.windows.get_mut(&id) else {
            return Task::none();
        };
        match event {
            Event::Keyboard(keyboard::Event::ModifiersChanged(modifiers)) => {
                state.modifiers = modifiers;
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
                } else {
                    return iced::widget::operation::focus_next();
                }
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
                state.resizing = None;
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

    fn system_appearance_event(&mut self, event: SystemAppearanceEvent) -> Task<Message> {
        if event.generation <= self.last_appearance_generation {
            return Task::none();
        }
        self.last_appearance_generation = event.generation;
        let appearance = resolved_system_appearance(event.appearance);
        let callbacks = Arc::clone(&self.callbacks);
        Task::perform(
            Self::run_blocking_operation("apply system appearance", move || {
                callbacks.system_appearance_changed(appearance)
            }),
            move |result| Message::SystemAppearanceChangedFinished {
                generation: event.generation,
                result,
            },
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
            ProjectSurfaceMessage::ResizePointer(x) => {
                match state.resizing {
                    Some(SidebarPanel::Explorer) => {
                        state.shell.layout_mut().resize_explorer(x.max(0.0) as u32);
                    }
                    Some(SidebarPanel::Inspector) => {
                        let width = state
                            .shell
                            .layout()
                            .requested_width()
                            .saturating_sub(x.max(0.0) as u32);
                        state.shell.layout_mut().resize_inspector(width);
                    }
                    Some(SidebarPanel::Editor) => {
                        let center = state.shell.layout().center();
                        if center.width() > 0 {
                            let ratio = (x - center.x() as f32) / center.width() as f32;
                            workspace.editor_mut().set_split_ratio(f64::from(ratio));
                        }
                    }
                    None => {}
                }
                Task::none()
            }
            ProjectSurfaceMessage::EndResize => {
                state.resizing = None;
                Self::workspace_persist_task(id, state)
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
                if let ProjectMessage::SelectHierarchy { gesture, .. } = &mut message
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
                let modal_after = workspace.modal().is_some();
                if !modal_before && modal_after {
                    state
                        .shell
                        .open_dialog(crate::DialogKind::RestoreConfirmation);
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
                        Message::AppearanceFinished,
                    );
                }
                let mut direct = Vec::new();
                let mut tasks = Vec::new();
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
                            tasks.push(Task::perform(
                                Self::run_blocking_operation("reinitialize History", move || {
                                    let access =
                                        ports.access().map_err(|error| error.to_string())?;
                                    access
                                        .history_maintenance(|history| history.reinitialize())
                                        .map_err(|error| error.to_string())?
                                        .map_err(|error| error.to_string())
                                }),
                                move |result| Message::HistoryReinitialized { window: id, result },
                            ));
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
                                let job = feeds.accept_recovery(acceptance);
                                tasks.push(Task::perform(
                                    Self::run_service_job(job),
                                    move |result| Message::RecoveryAccepted {
                                        window: id,
                                        session,
                                        ticket,
                                        result,
                                    },
                                ));
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
                                let job = feeds.discard_recovery(acceptance);
                                tasks.push(Task::perform(
                                    Self::run_service_job(job),
                                    move |result| Message::RecoveryDiscarded {
                                        window: id,
                                        session,
                                        ticket,
                                        result,
                                    },
                                ));
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
                            let preview_results = workspace.replacement_preview().results();
                            let selection = replacement_selection(
                                &project_ui.snapshot,
                                &preview_results,
                                &included_match_ids,
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
                                    result: Box::new(result),
                                },
                            ));
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
                        effect => direct.push(effect),
                    }
                }
                tasks.push(Self::project_effect_tasks(
                    id,
                    state.effect_executor.clone(),
                    direct,
                ));
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
                if matches!(message, EditorCenterMessage::CommitHierarchyDrop) {
                    let effects = workspace.update(ProjectMessage::CommitHierarchyDrag);
                    return Task::batch([
                        Self::project_effect_tasks(id, state.effect_executor.clone(), effects),
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
                            let revision = update.revision();
                            workspace.update(ProjectMessage::MarkDirty(revision.value()));
                            state.autosave.mark_dirty(revision.value(), Instant::now());
                            let Some(binding) = state.editor_bindings.get(&pane) else {
                                self.status = Some(
                                    "The edited view is no longer mounted; recovery was not persisted."
                                        .to_owned(),
                                );
                                return Task::none();
                            };
                            let Some(ports) = state.project.ports().cloned() else {
                                self.status = Some(
                                    "This project session has no persistence port.".to_owned(),
                                );
                                return Task::none();
                            };
                            let Some(adapter) = state.project.editor_adapter().cloned() else {
                                self.status =
                                    Some("This project session has no editor adapter.".to_owned());
                                return Task::none();
                            };
                            let persistence = Self::persist_projection_task(
                                id,
                                ports,
                                adapter,
                                binding.session(),
                                revision,
                            );
                            let spellcheck =
                                Self::spellcheck_task(id, state, view).unwrap_or_else(|error| {
                                    self.status = Some(error);
                                    Task::none()
                                });
                            return Task::batch([persistence, spellcheck]);
                        }
                        Err(error) => self.status = Some(error.to_string()),
                    }
                } else if !effects.is_empty() {
                    let request_save = effects
                        .iter()
                        .any(|effect| matches!(effect, EditorEffect::RequestSave));
                    effects.retain(|effect| !matches!(effect, EditorEffect::RequestSave));
                    let editor_tasks =
                        Self::editor_effect_tasks(id, state.effect_executor.clone(), effects);
                    if request_save && !state.autosave.save_in_flight {
                        let Some(ports) = state.project.ports().cloned() else {
                            self.status =
                                Some("This project session has no persistence port.".into());
                            return editor_tasks;
                        };
                        let through_revision = workspace.project_revision();
                        state.autosave.through_revision =
                            state.autosave.through_revision.max(through_revision);
                        state.autosave.save_in_flight = true;
                        workspace.update(ProjectMessage::StartSave(through_revision));
                        return Task::batch([
                            editor_tasks,
                            Self::save_task(id, ports, ProjectSaveKind::Explicit),
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
        Task::batch(effects.into_iter().map(|effect| {
            let executor = executor.clone();
            let history_action = match &effect {
                ProjectEffect::CreateNamedSnapshot(_) => Some(HistoryWorkflowAction::NamedSnapshot),
                ProjectEffect::RestoreHistory { .. } => Some(HistoryWorkflowAction::Restore),
                _ => None,
            };
            Task::perform(executor.execute_project_effect(effect), move |result| {
                Message::ProjectEffectFinished {
                    window,
                    history_action,
                    result,
                }
            })
        }))
    }

    fn open_spelling_menu(
        window: window::Id,
        state: &mut NativeProjectState,
        pane: EditorPane,
        view: ViewId,
        comment_range: EditorSelection,
        spelling_range: Option<EditorSelection>,
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
        Self::editor_effect_tasks(window, state.effect_executor.clone(), effects)
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
            let remaining = state
                .spelling_issues
                .get_mut(&context.view)
                .map(|issues| {
                    issues.retain(|issue| issue.range != context.range);
                    issues
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
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            let Some(workspace) = state.workspace.as_mut() else {
                return Task::none();
            };
            let effects =
                workspace
                    .editor_mut()
                    .update(crate::EditorMessage::SetSpellingDecorations {
                        view: context.view,
                        decorations: remaining,
                    });
            return Self::editor_effect_tasks(window, state.effect_executor.clone(), effects);
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
        Self::editor_effect_tasks(window, state.effect_executor.clone(), effects)
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
            .mark_dirty(mutation.revision.value(), Instant::now());
        let Some(ports) = state.project.ports().cloned() else {
            self.status = Some("This project session has no persistence port.".into());
            return Task::none();
        };
        let Some(adapter) = state.project.editor_adapter().cloned() else {
            self.status = Some("This project session has no editor adapter.".into());
            return Task::none();
        };
        self.status = mutation.presentation_error.or(mutation.feedback);
        Self::persist_projection_task(window, ports, adapter, mutation.session, mutation.revision)
    }

    fn editor_effect_tasks(
        window: window::Id,
        executor: Option<NativeProjectEffectExecutor>,
        effects: Vec<EditorEffect>,
    ) -> Task<Message> {
        let Some(executor) = executor else {
            return Task::none();
        };
        Task::batch(effects.into_iter().map(|effect| {
            let executor = executor.clone();
            Task::perform(executor.execute_editor_effect(effect), move |result| {
                Message::EditorEffectFinished { window, result }
            })
        }))
    }

    fn finish_project_effect(
        &mut self,
        window: window::Id,
        history_action: Option<HistoryWorkflowAction>,
        result: Result<ProjectEffectCompletion, ProjectRuntimeError>,
    ) -> Task<Message> {
        let Some(NativeWindow::Project(state)) = self.windows.get_mut(&window) else {
            return Task::none();
        };
        match result {
            Ok(ProjectEffectCompletion::WorkflowSnapshot(snapshot)) => {
                let snapshot = Arc::new(*snapshot);
                if let Some(project_ui) = state.project.project_ui.as_mut() {
                    project_ui.snapshot = Arc::clone(&snapshot);
                }
                if let Some(workspace) = state.workspace.as_mut() {
                    workspace.reconcile_snapshot(&snapshot);
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
                Task::batch([reopen, refresh])
            }
            Ok(ProjectEffectCompletion::RefreshedSnapshot(snapshot)) => {
                let snapshot = *snapshot;
                if let Some(workspace) = state.workspace.as_mut() {
                    workspace.reconcile_snapshot(&snapshot);
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
                    return Task::none();
                };
                let through_revision = snapshot.project.revision.value();
                state.autosave.save_in_flight = true;
                if let Some(workspace) = state.workspace.as_mut() {
                    workspace.update(ProjectMessage::StartSave(through_revision));
                }
                Self::save_task(window, ports, ProjectSaveKind::Structural)
            }
            Ok(ProjectEffectCompletion::TreePaste {
                snapshot,
                kind,
                created_roots,
            }) => {
                let snapshot = Arc::new(*snapshot);
                if let Some(project_ui) = state.project.project_ui.as_mut() {
                    project_ui.snapshot = Arc::clone(&snapshot);
                }
                if let Some(workspace) = state.workspace.as_mut() {
                    workspace.reconcile_snapshot(&snapshot);
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
                Task::none()
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
                self.status = Some(error);
                Task::none()
            }
        }
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
        result: Result<EditorEffectCompletion, ProjectRuntimeError>,
    ) -> Task<Message> {
        match result {
            Ok(EditorEffectCompletion::ProjectMutation(completion)) => {
                let project = self.finish_project_effect(window, None, Ok(completion));
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
                self.windows
                    .get_mut(&window)
                    .and_then(|native| match native {
                        NativeWindow::Project(state) => state
                            .refresh_spellcheck_view
                            .take()
                            .and_then(|view| Self::spellcheck_task(window, state, view).ok()),
                        NativeWindow::Launcher => None,
                    })
                    .unwrap_or_else(Task::none)
            }
            Ok(EditorEffectCompletion::SavedThrough(revision)) => {
                if let Some(NativeWindow::Project(state)) = self.windows.get_mut(&window)
                    && let Some(workspace) = state.workspace.as_mut()
                {
                    state.autosave.save_in_flight = false;
                    workspace.update(ProjectMessage::SaveCompleted(revision));
                }
                self.status = None;
                Task::none()
            }
            Ok(EditorEffectCompletion::Intent(intent)) => {
                let Some(NativeWindow::Project(state)) = self.windows.get_mut(&window) else {
                    return Task::none();
                };
                if let EditorRuntimeIntent::Mount { snapshot, .. } = &intent {
                    Self::accept_hydrated_snapshot(state, snapshot.as_ref().clone());
                }
                let spellcheck_view = editor_intent_view(&intent);
                match Self::apply_editor_intent(state, intent, self.appearance) {
                    Ok(Some((session, revision))) => {
                        let Some(workspace) = state.workspace.as_mut() else {
                            return Task::none();
                        };
                        workspace.update(ProjectMessage::MarkDirty(revision.value()));
                        state.autosave.mark_dirty(revision.value(), Instant::now());
                        let Some(ports) = state.project.ports().cloned() else {
                            self.status =
                                Some("This project session has no persistence port.".into());
                            return Task::none();
                        };
                        let Some(adapter) = state.project.editor_adapter().cloned() else {
                            self.status =
                                Some("This project session has no editor adapter.".into());
                            return Task::none();
                        };
                        let persistence = Self::persist_projection_task(
                            window, ports, adapter, session, revision,
                        );
                        let spellcheck = spellcheck_view
                            .map(|view| Self::spellcheck_task(window, state, view))
                            .transpose()
                            .unwrap_or_else(|error| {
                                self.status = Some(error);
                                None
                            })
                            .unwrap_or_else(Task::none);
                        return Task::batch([persistence, spellcheck]);
                    }
                    Ok(None) => {}
                    Err(error) => self.status = Some(error),
                }
                Task::none()
            }
            Err(error) => {
                self.status = Some(format!("Editor action could not complete: {error}"));
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
                    binding.detach().map_err(|error| error.to_string())?;
                }
                state.mounted_documents.remove(&pane);
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
            binding.detach().map_err(|error| error.to_string())?;
        }
        let session = shared.map_or(
            MountedEditorSession::Open(load),
            MountedEditorSession::Reuse,
        );
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
                    binding.detach().map_err(|error| error.to_string())?;
                }
                state.mounted_documents.remove(&pane);
                state.editor_hosts.insert(
                    pane,
                    crate::iced_editor_surface::EditorPaneSlot::state(
                        crate::iced_editor_surface::EditorCenterPaneState::Empty,
                    ),
                );
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

    fn persist_projection_task(
        window: window::Id,
        ports: ProjectUiPorts,
        adapter: Arc<EditorIcedAdapter>,
        session: parchmint_editor_api::SharedEditorSession,
        revision: EditorRevision,
    ) -> Task<Message> {
        Task::perform(
            async move {
                let projection = adapter
                    .project(session, revision)
                    .await
                    .map_err(|error| error.to_string())?;
                let access = ports.access().map_err(|error| error.to_string())?;
                access
                    .persistence(|persistence| persistence.persist_editor_projection(projection))
                    .map_err(|error| error.to_string())?
                    .map_err(|error| error.to_string())?;
                access
                    .snapshot(|query| query.snapshot())
                    .map_err(|error| error.to_string())?
                    .map_err(|error| error.to_string())
            },
            move |result| Message::EditorProjectionPersisted {
                window,
                revision: revision.value(),
                result,
            },
        )
    }

    fn save_task(
        window: window::Id,
        ports: ProjectUiPorts,
        kind: ProjectSaveKind,
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
            move |result| Message::SaveFinished { window, result },
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
        let decorations = issues
            .iter()
            .map(|issue| {
                SpellingDecoration::new(
                    issue.word.clone(),
                    crate::FindMatch::new(issue.range.start().value(), issue.range.end().value()),
                )
            })
            .collect();
        state.spelling_issues.insert(ticket.view, issues);
        let Some(workspace) = state.workspace.as_mut() else {
            return Task::none();
        };
        let effects = workspace
            .editor_mut()
            .update(crate::EditorMessage::SetSpellingDecorations {
                view: ticket.view,
                decorations,
            });
        self.status = None;
        Self::editor_effect_tasks(window, state.effect_executor.clone(), effects)
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
            let Some(workspace) = state.workspace.as_mut() else {
                continue;
            };
            if !state.autosave.should_save(now) {
                continue;
            }
            let Some(ports) = state.project.ports().cloned() else {
                continue;
            };
            let through_revision = state.autosave.through_revision;
            state.autosave.save_in_flight = true;
            workspace.update(ProjectMessage::StartSave(through_revision));
            let window = *window;
            tasks.push(Task::perform(
                Self::run_blocking_operation("autosave project", move || {
                    let access = ports.access().map_err(|error| error.to_string())?;
                    let (handle, _) = access
                        .persistence(|persistence| {
                            persistence.request_save(ProjectSaveKind::Autosave)
                        })
                        .map_err(|error| error.to_string())?
                        .map_err(|error| error.to_string())?;
                    let saved = access
                        .persistence(|persistence| persistence.await_save(handle))
                        .map_err(|error| error.to_string())?
                        .map_err(|error| error.to_string())?;
                    Ok(saved.written.project_revision.value())
                }),
                move |result| Message::SaveFinished { window, result },
            ));
        }
        Task::batch(tasks)
    }

    fn open_launcher_window(&mut self) -> Task<Message> {
        let (id, task) = window::open(window_settings((900.0, 620.0), (720, 480)));
        self.callbacks.project_window_created(LAUNCHER_CAPABILITY);
        self.windows.insert(id, NativeWindow::Launcher);
        Task::batch([task.map(|_| Message::WindowOpened), self.refresh_menu(id)])
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
        if let Some(adapter) = state.project.editor_adapter() {
            for session in sessions {
                iced::futures::executor::block_on(adapter.close(session));
            }
        }
        state.editor_hosts = EditorHostSlots::default();
        state.mounted_documents.clear();
        state.spellcheck_generation.clear();
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
        Task::batch([Task::batch(spellcheck_tasks), self.refresh_menu(window)])
    }

    fn open_project_window(&mut self, project: NativeProjectWindow) -> Task<Message> {
        let (id, task) = window::open(window_settings(
            (1280.0, 720.0),
            ShellLayout::MIN_WINDOW_SIZE,
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
        let effect_executor = project.project_ui.as_ref().map(|project| {
            NativeProjectEffectExecutor::new(project.ports.clone(), Arc::clone(&project.snapshot))
        });
        let service_feeds = project
            .project_ui
            .as_ref()
            .map(|project| AsyncServiceFeeds::new(project.ports.clone()));
        self.windows.insert(
            id,
            NativeWindow::Project(Box::new(NativeProjectState {
                project: project.clone(),
                shell: Shell::new(project.window),
                workspace,
                editor_hosts,
                editor_bindings,
                mounted_documents,
                effect_executor,
                service_feeds,
                recovery_acceptance: None,
                active_export: None,
                export_destination: None,
                autosave: AutosaveState::default(),
                next_spellcheck_generation: 0,
                spellcheck_generation: BTreeMap::new(),
                spelling_issues: BTreeMap::new(),
                pending_spelling_menu: None,
                spelling_menu: None,
                refresh_spellcheck_view: None,
                modifiers: keyboard::Modifiers::default(),
                resizing: None,
            })),
        );
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
            task.map(|_| Message::WindowOpened),
            self.refresh_menu(id),
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
                self.open_project_window(window)
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
        let project = state.project.project.clone();
        let callbacks = Arc::clone(&self.callbacks);
        let persist = Self::workspace_persist_task(id, state);
        let close = Task::perform(
            Self::run_blocking_operation("close project", move || callbacks.close_project(project)),
            move |result| Message::ProjectCloseFinished { window: id, result },
        );
        Task::batch([persist, close])
    }

    fn finish_close(&mut self, id: window::Id) -> Task<Message> {
        self.closing_windows.remove(&id);
        let removed = self.windows.remove(&id);
        let capability = match removed {
            Some(NativeWindow::Project(state)) => {
                self.project_windows.remove(&state.project.window);
                self.menu_bindings.remove(&state.project.window);
                self.in_window_menus.remove(&state.project.window);
                self.close_failures.remove(&state.project.window);
                self.callbacks
                    .project_window_destroyed(state.project.window);
                Some(state.project.window)
            }
            Some(NativeWindow::Launcher) => {
                self.menu_bindings.remove(&LAUNCHER_CAPABILITY);
                self.in_window_menus.remove(&LAUNCHER_CAPABILITY);
                self.callbacks.project_window_destroyed(LAUNCHER_CAPABILITY);
                Some(LAUNCHER_CAPABILITY)
            }
            None => None,
        };
        if self
            .open_in_window_menu
            .as_ref()
            .is_some_and(|(open, _)| Some(*open) == capability)
        {
            self.open_in_window_menu = None;
        }
        let close = if self.windows.is_empty() {
            Task::batch([window::close(id), iced::exit()])
        } else {
            window::close(id)
        };
        match capability {
            Some(capability) => self.detach_menu(id, capability).chain(close),
            None => close,
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
    session: SharedEditorSession,
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
        session: request.editor_session.clone(),
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
        session: request.editor_session.clone(),
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
    let create_action = if creating_project {
        button("New Project").width(144).height(36)
    } else {
        button("Create Project")
            .width(144)
            .height(36)
            .on_press(Message::ShowNewProject)
    };
    let mut content = column![
        text("ParchMint").size(24),
        text("Recent projects").size(24),
        text("Open a recent project, create a new one, or choose another project folder.").size(16),
        row![
            create_action,
            if opening_project {
                button("Opening Project…").width(128).height(36)
            } else {
                button("Open Project")
                    .width(128)
                    .height(36)
                    .on_press(Message::ChooseOpenProject)
            }
        ]
        .spacing(12),
    ]
    .spacing(24)
    .max_width(720);
    if creating_project {
        content = content.push(
            column![
                text_input("Project title", new_project.title())
                    .on_input(Message::NewProjectTitleChanged)
                    .padding(10),
                text_input("Project destination", new_project.destination())
                    .on_input(Message::NewProjectDestinationChanged)
                    .padding(10),
                button("Choose Destination…").on_press(Message::ChooseNewProjectDestination),
                text_input(
                    "Author (optional)",
                    new_project.author().unwrap_or_default()
                )
                .on_input(Message::NewProjectAuthorChanged)
                .padding(10),
                button("Create and Open").on_press(Message::CreateProject),
            ]
            .spacing(10),
        );
    }
    if recent_projects.is_empty() {
        content = content.push(text("No recent projects yet.").size(14));
    }
    for project in recent_projects {
        let path = project.path().to_owned();
        content = content.push(
            button(
                row![
                    text("▱").size(24),
                    column![
                        text(project.name().to_owned()).size(18),
                        text(path.clone()).size(14),
                        text(format!("◷  Opened {}", project.last_opened())).size(13),
                    ]
                    .spacing(6),
                ]
                .spacing(14),
            )
            .padding([10, 14])
            .width(520)
            .height(96)
            .on_press(Message::OpenProject(PathBuf::from(path))),
        );
    }
    if let Some(status) = status {
        content = content.push(text(status).size(14));
    }
    container(content)
        .padding(iced::Padding {
            top: 72.0,
            right: 72.0,
            bottom: 72.0,
            left: 100.0,
        })
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
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

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use iced::futures::executor::block_on;

    use super::*;

    struct RecordingCallbacks {
        open_result: Mutex<Option<NativeProjectOpenResult>>,
        closed: Mutex<Vec<PathBuf>>,
        created: Mutex<Vec<WindowCapability>>,
        destroyed: Mutex<Vec<WindowCapability>>,
        system_appearances: Mutex<Vec<ResolvedAppearance>>,
        system_appearance_result: Mutex<Result<Option<ResolvedAppearance>, String>>,
    }

    impl RecordingCallbacks {
        fn opening(result: NativeProjectOpenResult) -> Self {
            Self {
                open_result: Mutex::new(Some(result)),
                closed: Mutex::new(Vec::new()),
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
            Ok(())
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
    fn semantic_menu_preserves_submenu_order_labels_and_enabled_state() {
        let menu = semantic_desktop_menu(true, false, true);
        let [
            SemanticMenuEntry::Submenu {
                label: file_label,
                entries: file_entries,
            },
            SemanticMenuEntry::Submenu {
                label: edit_label,
                entries: edit_entries,
            },
        ] = menu.entries()
        else {
            panic!("desktop menu must retain File and Edit submenus");
        };

        assert_eq!(file_label, "File");
        assert_eq!(edit_label, "Edit");
        let SemanticMenuEntry::Command(save) = &file_entries[3] else {
            panic!("save command order changed");
        };
        assert_eq!(save.id(), "file.save");
        assert!(!save.enabled());
        let SemanticMenuEntry::Command(copy) = &edit_entries[3] else {
            panic!("copy command order changed");
        };
        assert_eq!(copy.id(), "edit.copy");
        assert!(copy.enabled());
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
    fn autosave_waits_for_idle_but_caps_continuous_input() {
        let start = Instant::now();
        let mut autosave = AutosaveState::default();
        autosave.mark_dirty(1, start);

        assert!(!autosave.should_save(start + Duration::from_millis(1_499)));
        assert!(autosave.should_save(start + Duration::from_millis(1_500)));

        autosave.save_in_flight = false;
        autosave.mark_dirty(2, start + Duration::from_secs(29));
        assert!(autosave.should_save(start + Duration::from_secs(30)));
        assert_eq!(autosave.through_revision, 2);
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
            recent_projects: Vec::new(),
            projects: vec![project.clone()],
            locked_project: None,
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
    fn stale_menu_binding_cannot_activate_a_live_project_window() {
        let project = NativeProjectWindow {
            project: PathBuf::from("/tmp/menu.parchmint"),
            window: WindowCapability::new(44, 3),
            session: parchmint_ui_api::ProjectSessionRegistry::new().register(44),
            project_ui: None,
            editor: None,
        };
        let callbacks = Arc::new(RecordingCallbacks::opening(NativeProjectOpenResult::Locked));
        let (mut desktop, _tasks) = NativeDesktop::boot(NativeDesktopStartup {
            appearance: ResolvedAppearance::Light,
            recent_projects: Vec::new(),
            projects: vec![project.clone()],
            locked_project: None,
            callbacks,
        });
        let _ = desktop.update(Message::MenuInstalled {
            capability: project.window,
            result: Ok(9),
        });
        let windows_before = desktop.windows.len();

        let _ = desktop.update(Message::MenuActivation(MenuActivation {
            binding: parchmint_platform_api::WindowResult::new(project.window, 8),
            command_id: "file.close".to_owned(),
        }));

        assert_eq!(desktop.windows.len(), windows_before);
        assert_eq!(desktop.menu_bindings.get(&project.window), Some(&9));
        assert_eq!(
            desktop.status.as_deref(),
            Some("Ignored a stale menu activation.")
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn linux_in_window_menu_tracks_rebind_and_routes_through_menu_activation() {
        let callbacks = Arc::new(RecordingCallbacks::opening(NativeProjectOpenResult::Locked));
        let (mut desktop, _tasks) = NativeDesktop::boot(NativeDesktopStartup {
            appearance: ResolvedAppearance::Light,
            recent_projects: Vec::new(),
            projects: Vec::new(),
            locked_project: None,
            callbacks,
        });

        let _ = desktop.update(Message::MenuInstalled {
            capability: LAUNCHER_CAPABILITY,
            result: Ok(7),
        });
        let _ = desktop.update(Message::MenuAttached {
            capability: LAUNCHER_CAPABILITY,
            binding: 7,
            result: Ok(NativeMenuAttachment::InWindow),
        });
        assert_eq!(desktop.in_window_menus.get(&LAUNCHER_CAPABILITY), Some(&7));

        let _ = desktop.update(Message::ToggleInWindowMenu {
            capability: LAUNCHER_CAPABILITY,
            label: "File".to_owned(),
        });
        assert_eq!(
            desktop.open_in_window_menu.as_ref(),
            Some(&(LAUNCHER_CAPABILITY, "File".to_owned()))
        );

        let _ = desktop.update(Message::InWindowMenuActivation(MenuActivation {
            binding: WindowResult::new(LAUNCHER_CAPABILITY, 7),
            command_id: "file.new".to_owned(),
        }));
        assert!(desktop.creating_project);
        assert!(desktop.open_in_window_menu.is_none());

        let _ = desktop.update(Message::MenuInstalled {
            capability: LAUNCHER_CAPABILITY,
            result: Ok(8),
        });
        let _ = desktop.update(Message::MenuAttached {
            capability: LAUNCHER_CAPABILITY,
            binding: 7,
            result: Ok(NativeMenuAttachment::InWindow),
        });
        assert_eq!(desktop.menu_bindings.get(&LAUNCHER_CAPABILITY), Some(&8));
        assert_eq!(desktop.in_window_menus.get(&LAUNCHER_CAPABILITY), None);
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
            recent_projects: Vec::new(),
            projects: Vec::new(),
            locked_project: None,
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
        assert_eq!(desktop.windows.len(), 2);
        assert!(desktop.status.is_none());
        assert!(!desktop.opening_project);
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
            recent_projects: Vec::new(),
            projects,
            locked_project: None,
            callbacks,
        });

        let _appearance = desktop.update(Message::AppearanceFinished(Ok(ResolvedAppearance::Dark)));

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
            recent_projects: Vec::new(),
            projects,
            locked_project: None,
            callbacks: callbacks.clone(),
        });
        *callbacks
            .system_appearance_result
            .lock()
            .expect("system appearance result mutex poisoned") = Ok(Some(ResolvedAppearance::Dark));

        let _event = desktop.update(Message::SystemAppearanceEvent(SystemAppearanceEvent {
            generation: 1,
            appearance: SystemAppearance::Dark,
        }));
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
        let _system_only = desktop.update(Message::SystemAppearanceEvent(SystemAppearanceEvent {
            generation: 2,
            appearance: SystemAppearance::Light,
        }));
        let _completion = desktop.update(Message::SystemAppearanceChangedFinished {
            generation: 2,
            result: Ok(None),
        });
        let _duplicate = desktop.update(Message::SystemAppearanceEvent(SystemAppearanceEvent {
            generation: 2,
            appearance: SystemAppearance::Dark,
        }));

        assert_eq!(desktop.appearance, ResolvedAppearance::Dark);
        assert_eq!(desktop.last_appearance_generation, 2);
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
            recent_projects: Vec::new(),
            projects: vec![project.clone(), other.clone()],
            locked_project: None,
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
            result: Ok(()),
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
