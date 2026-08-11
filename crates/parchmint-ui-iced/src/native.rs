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
    Element, Length, Subscription, Task, Theme,
    futures::SinkExt,
    widget::{button, column, container, row, text, text_input},
    window,
};
use parchmint_application::{ReplacementEdit, ReplacementSelection};
use parchmint_editor_api::{
    AtomicBlockKind, BlockFormatKind, CanonicalDocumentLoad, EditorAdapter,
    EditorCommand as AdapterEditorCommand, EditorCommandKind, EditorCommandOrigin, EditorRevision,
    EditorSelection, InlineMarkKind, SharedEditorSession, ViewId,
};
use parchmint_editor_iced::{
    EditorIcedAdapter, EditorSurfaceTheme, EditorViewport, MountedEditorBinding,
    MountedEditorBindingConfig, MountedEditorClipboardIntent, MountedEditorSession,
};
use parchmint_export_api::{ExportNumbering, ExportRunOptions};
use parchmint_platform_api::{
    ClipboardContent, ClipboardFormats, PathDialog, PathDialogKind, SystemAppearance,
    SystemAppearanceEvent, SystemAppearanceEventService, UntrustedClipboardContent,
    WindowCapability,
};
use parchmint_preferences::{
    AppearanceMode, RecentProject as PreferenceRecentProject, ResolvedAppearance,
};
use parchmint_ui_api::{
    ExportArtifact, ExportArtifactAction, ExportArtifactToken, ProjectSaveKind,
    ProjectSessionCapability, ProjectUiPorts, ProjectUiProject,
};

use crate::{
    EditorEffect, EditorPane, LauncherState, NewProjectDraft, ProjectEffect, ProjectMessage,
    ProjectTask, ProjectTaskCompletion, ProjectTaskPayload, ProjectTaskTicket, ProjectWorkspace,
    RecentProject, RibbonDestination, Shell, ShellLayout,
    async_service_feeds::{
        AsyncServiceFeeds, BlockingServiceJob, HistoryListResult, RecoveryAcceptedResult,
        RecoveryReconcileResult, SearchBatchResult, SearchRequest, SearchStart,
    },
    design_tokens::ParchMintTheme,
    iced_editor_surface::{EditorCenterMessage, EditorHostSlots, editor_center_surface},
    iced_project_surface::{ProjectSurfaceMessage, project_surface as workspace_surface},
    project_runtime::{
        EditorEffectCompletion, EditorRuntimeIntent, NativeProjectEffectExecutor,
        ProjectEffectCompletion, ProjectRuntimeError, ResolvedDocumentMount,
    },
};

const LAUNCHER_CAPABILITY: WindowCapability = WindowCapability::new(u64::MAX, 1);

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
        result: Result<(), String>,
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
    SystemAppearanceStreamFailed(String),
    SaveFinished {
        window: window::Id,
        result: Result<u64, String>,
    },
    ProjectEffectFinished {
        window: window::Id,
        result: Result<ProjectEffectCompletion, ProjectRuntimeError>,
    },
    EditorEffectFinished {
        window: window::Id,
        result: Result<EditorEffectCompletion, ProjectRuntimeError>,
    },
    SearchFinished {
        window: window::Id,
        ticket: ProjectTaskTicket,
        result: Result<Vec<SearchBatchResult>, String>,
    },
    HistoryFinished {
        window: window::Id,
        ticket: ProjectTaskTicket,
        result: Result<HistoryListResult, String>,
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
    ExportFinished {
        window: window::Id,
        ticket: ProjectTaskTicket,
        result: Result<Option<ExportArtifact>, String>,
    },
    ExportArtifactActionFinished(Result<(), String>),
    RecoveryReconciled {
        window: window::Id,
        ticket: ProjectTaskTicket,
        result: Result<RecoveryReconcileResult, String>,
    },
    RecoveryAccepted {
        window: window::Id,
        ticket: ProjectTaskTicket,
        result: Result<RecoveryAcceptedResult, String>,
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

#[derive(Clone)]
struct AppearanceEventSubscription(Arc<dyn SystemAppearanceEventService>);

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
    export_artifacts: BTreeMap<String, ExportArtifactToken>,
    autosave: AutosaveState,
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
            Message::ChooseOpenProject => self.choose_directory(false),
            Message::ChooseNewProjectDestination => self.choose_directory(true),
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
                    Ok(()) => {
                        self.status = Some(format!(
                            "Recovery is durable through editor revision {revision}."
                        ));
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
                Task::none()
            }
            Message::ProjectEffectFinished { window, result } => {
                self.finish_project_effect(window, result)
            }
            Message::EditorEffectFinished { window, result } => {
                self.finish_editor_effect(window, result)
            }
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
                result,
            } => {
                let Some(NativeWindow::Project(state)) = self.windows.get_mut(&window) else {
                    return Task::none();
                };
                let Some(workspace) = state.workspace.as_mut() else {
                    return Task::none();
                };
                let payload = result
                    .map(|history| history.reducer_payload())
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
                            workspace.reconcile_snapshot(&snapshot);
                            workspace.accept_completion(ProjectTaskCompletion::for_ticket(
                                ticket,
                                ProjectTaskPayload::ReplacementApplied { revision },
                            ));
                        }
                        let Some(ports) = state.project.ports().cloned() else {
                            return Task::none();
                        };
                        Self::save_task(window, ports, ProjectSaveKind::Structural)
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
            Message::ExportFinished {
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
                let payload = match result {
                    Ok(Some(artifact)) => {
                        state
                            .export_artifacts
                            .insert(artifact.display_name.clone(), artifact.token);
                        ProjectTaskPayload::ExportSucceeded {
                            output_name: artifact.display_name,
                        }
                    }
                    Ok(None) => ProjectTaskPayload::Failed("Export canceled".into()),
                    Err(error) => ProjectTaskPayload::Failed(error),
                };
                workspace.accept_completion(ProjectTaskCompletion::for_ticket(ticket, payload));
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
                ticket,
                result,
            } => {
                let Some(NativeWindow::Project(state)) = self.windows.get_mut(&window) else {
                    return Task::none();
                };
                match result {
                    Ok(recovery) => {
                        let Some(acceptance) = recovery.acceptance else {
                            if let Some(workspace) = state.workspace.as_mut() {
                                workspace.accept_completion(ProjectTaskCompletion::for_ticket(
                                    ticket,
                                    ProjectTaskPayload::Failed(
                                        "No recoverable editor records were found.".to_owned(),
                                    ),
                                ));
                            }
                            return Task::none();
                        };
                        let Some(feeds) = state.service_feeds.as_ref() else {
                            return Task::none();
                        };
                        let job = feeds.accept_recovery(acceptance);
                        Task::perform(Self::run_service_job(job), move |result| {
                            Message::RecoveryAccepted {
                                window,
                                ticket,
                                result,
                            }
                        })
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
                    .map(|accepted| accepted.reducer_payload())
                    .unwrap_or_else(ProjectTaskPayload::Failed);
                let accepted =
                    workspace.accept_completion(ProjectTaskCompletion::for_ticket(ticket, payload));
                if accepted && let Some(binding) = state.editor_bindings.get(&EditorPane::Primary) {
                    let _ = binding.update(parchmint_editor_iced::MountedEditorMessage::Focus(
                        0_u64.into(),
                    ));
                }
                Task::none()
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
                    async move { callbacks.set_appearance(mode) },
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
        match self.windows.get(&id) {
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
                        state.shell.destination(),
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
        }
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
        ];
        if let Some(events) = &self.appearance_events {
            subscriptions.push(Subscription::run_with(
                AppearanceEventSubscription(Arc::clone(events)),
                appearance_event_subscription,
            ));
        }
        Subscription::batch(subscriptions)
    }

    fn system_appearance_event(&mut self, event: SystemAppearanceEvent) -> Task<Message> {
        if event.generation <= self.last_appearance_generation {
            return Task::none();
        }
        self.last_appearance_generation = event.generation;
        let appearance = resolved_system_appearance(event.appearance);
        match self.callbacks.system_appearance_changed(appearance) {
            Ok(Some(appearance)) => {
                self.apply_appearance(appearance);
                self.status = None;
            }
            Ok(None) => {}
            Err(error) => self.status = Some(error),
        }
        Task::none()
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

    fn typed_project_view<'a>(
        id: window::Id,
        workspace: &'a ProjectWorkspace,
        editor_hosts: &'a EditorHostSlots,
        destination: RibbonDestination,
        appearance: ResolvedAppearance,
        close_failure: Option<&str>,
        status: Option<&str>,
    ) -> Element<'a, Message> {
        let theme = ParchMintTheme::new(appearance);
        let editor = editor_center_surface(workspace.editor(), theme, editor_hosts)
            .map(ProjectSurfaceMessage::EditorCenter);
        let surface =
            workspace_surface(workspace, destination, theme, editor).map(move |message| {
                Message::ProjectSurface {
                    window: id,
                    message,
                }
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

        match message {
            ProjectSurfaceMessage::Navigate(destination) => {
                state.shell.select_destination(destination);
                if destination == RibbonDestination::History
                    && let Some(feeds) = state.service_feeds.as_ref()
                {
                    let ticket = workspace.begin_task(ProjectTask::LoadHistory);
                    let job = feeds.history_list(None, 100, None);
                    return Task::perform(Self::run_service_job(job), move |result| {
                        Message::HistoryFinished {
                            window: id,
                            ticket,
                            result,
                        }
                    });
                }
                Task::none()
            }
            ProjectSurfaceMessage::Focus(_) => Task::none(),
            ProjectSurfaceMessage::Project(message) => {
                let appearance = match &message {
                    ProjectMessage::SetAppearance(mode) => Some(*mode),
                    _ => None,
                };
                let clipboard_status = match &message {
                    ProjectMessage::CopySelection => Some("Project item copied"),
                    ProjectMessage::CutSelection => Some("Project item ready to move"),
                    ProjectMessage::CancelCut => Some("Project move cancelled"),
                    ProjectMessage::PasteSelection { .. } => Some("Pasting project item…"),
                    _ => None,
                };
                let effects = workspace.update(message);
                if let Some(status) = clipboard_status {
                    self.status = Some(status.to_owned());
                }
                if let Some(mode) = appearance {
                    let callbacks = Arc::clone(&self.callbacks);
                    return Task::perform(
                        async move { callbacks.set_appearance(mode) },
                        Message::AppearanceFinished,
                    );
                }
                let mut direct = Vec::new();
                let mut tasks = Vec::new();
                for effect in effects {
                    match effect {
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
                            }
                        }
                        ProjectEffect::FocusRecoveredEditor => {
                            if let Some(feeds) = state.service_feeds.as_ref() {
                                let ticket = workspace.begin_task(ProjectTask::AcceptRecovery);
                                let job = feeds.reconcile_recovery();
                                tasks.push(Task::perform(
                                    Self::run_service_job(job),
                                    move |result| Message::RecoveryReconciled {
                                        window: id,
                                        ticket,
                                        result,
                                    },
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
                            if project_ui.snapshot.project.revision.value()
                                != captured_project_revision
                            {
                                self.status =
                                    Some("project changed before replacement preview".into());
                                continue;
                            }
                            let ticket = workspace.begin_task(ProjectTask::ReplacementPreview);
                            let selection = replacement_selection(
                                &project_ui.snapshot,
                                workspace.global_search().results(),
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
                            if project_ui.snapshot.project.revision.value()
                                != captured_project_revision
                            {
                                self.status = Some("project changed before replacement".into());
                                continue;
                            }
                            let ticket = workspace.begin_task(ProjectTask::ApplyReplacement);
                            let selection = replacement_selection(
                                &project_ui.snapshot,
                                workspace.global_search().results(),
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
                            output_name,
                            number_documents,
                            source_revision,
                        } => {
                            let Some(ports) = state.project.ports().cloned() else {
                                self.status = Some("project export port is unavailable".into());
                                continue;
                            };
                            let capability = state.project.window;
                            let ticket =
                                workspace.begin_task(ProjectTask::Export { source_revision });
                            let options = ExportRunOptions {
                                numbering: if number_documents {
                                    ExportNumbering::Documents
                                } else {
                                    ExportNumbering::None
                                },
                            };
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
                                            "export dialog returned for a stale window".into()
                                        );
                                    }
                                    let Some(selection) = selected.into_value() else {
                                        return Ok(None);
                                    };
                                    let ports_for_export = ports.clone();
                                    Self::run_blocking_operation("export project", move || {
                                        let access = ports_for_export
                                            .access()
                                            .map_err(|error| error.to_string())?;
                                        access
                                            .export_target(|export| {
                                                export.export_to_path(selection, options)
                                            })
                                            .map_err(|error| error.to_string())?
                                            .map(Some)
                                            .map_err(|error| error.to_string())
                                    })
                                    .await
                                },
                                move |result| Message::ExportFinished {
                                    window: id,
                                    ticket,
                                    result,
                                },
                            ));
                        }
                        ProjectEffect::OpenExportResult(output) => {
                            let action = ExportArtifactAction::Open;
                            let Some(artifact) = state.export_artifacts.get(&output).copied()
                            else {
                                self.status =
                                    Some("completed export artifact is no longer available".into());
                                continue;
                            };
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
                        ProjectEffect::RevealExportResult(output) => {
                            let action = ExportArtifactAction::Reveal;
                            let Some(artifact) = state.export_artifacts.get(&output).copied()
                            else {
                                self.status =
                                    Some("completed export artifact is no longer available".into());
                                continue;
                            };
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
                Task::batch(tasks)
            }
            ProjectSurfaceMessage::EditorCenter(message) => {
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
                    message,
                } = message
                {
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
                        Ok(update) if update.document_changed() => {
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
                            let session = binding.session();
                            return Task::perform(
                                async move {
                                    let projection = adapter
                                        .project(session, revision)
                                        .await
                                        .map_err(|error| error.to_string())?;
                                    let access =
                                        ports.access().map_err(|error| error.to_string())?;
                                    access
                                        .persistence(|persistence| {
                                            persistence.persist_editor_projection(projection)
                                        })
                                        .map_err(|error| error.to_string())?
                                        .map(|_| ())
                                        .map_err(|error| error.to_string())
                                },
                                move |result| Message::EditorProjectionPersisted {
                                    window: id,
                                    revision: revision.value(),
                                    result,
                                },
                            );
                        }
                        Ok(_) => {}
                        Err(error) => self.status = Some(error.to_string()),
                    }
                } else if !effects.is_empty() {
                    return Self::editor_effect_tasks(id, state.effect_executor.clone(), effects);
                }
                Task::none()
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
            Task::perform(executor.execute_project_effect(effect), move |result| {
                Message::ProjectEffectFinished { window, result }
            })
        }))
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
            MountedEditorClipboardIntent::Paste => (
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
        if request.intent != MountedEditorClipboardIntent::Paste {
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
        self.status = mutation.presentation_error;
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
                }
                state.effect_executor = state
                    .project
                    .ports()
                    .cloned()
                    .map(|ports| NativeProjectEffectExecutor::new(ports, snapshot));
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
                Self::project_effect_tasks(window, state.effect_executor.clone(), reopen)
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
            Ok(ProjectEffectCompletion::TreePaste { snapshot, kind }) => {
                let snapshot = Arc::new(*snapshot);
                if let Some(project_ui) = state.project.project_ui.as_mut() {
                    project_ui.snapshot = Arc::clone(&snapshot);
                }
                if let Some(workspace) = state.workspace.as_mut() {
                    workspace.reconcile_snapshot(&snapshot);
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
                self.status = Some(match kind {
                    crate::TreeClipboardKind::Copy => "Project item pasted".to_owned(),
                    crate::TreeClipboardKind::Cut => "Project item moved".to_owned(),
                });
                Task::none()
            }
            Ok(ProjectEffectCompletion::OpenDocuments(documents)) => {
                for document in documents {
                    if let Err(error) =
                        Self::mount_resolved_document(state, document, self.appearance)
                    {
                        self.status = Some(error);
                        break;
                    }
                }
                Task::none()
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
            Ok(ProjectEffectCompletion::NavigateSearch { document, range }) => {
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
                self.status = Some(format!("Project action could not complete: {error}"));
                Task::none()
            }
        }
    }

    fn finish_editor_effect(
        &mut self,
        window: window::Id,
        result: Result<EditorEffectCompletion, ProjectRuntimeError>,
    ) -> Task<Message> {
        match result {
            Ok(EditorEffectCompletion::ProjectMutation(completion)) => {
                self.finish_project_effect(window, Ok(completion))
            }
            Ok(EditorEffectCompletion::GlobalDictionaryUpdated) => {
                self.status = None;
                Task::none()
            }
            Ok(EditorEffectCompletion::Intent(intent)) => {
                let Some(NativeWindow::Project(state)) = self.windows.get_mut(&window) else {
                    return Task::none();
                };
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
                        return Self::persist_projection_task(
                            window, ports, adapter, session, revision,
                        );
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

    fn mount_editor_load(
        state: &mut NativeProjectState,
        pane: EditorPane,
        view: parchmint_editor_api::ViewId,
        load: CanonicalDocumentLoad,
        appearance: ResolvedAppearance,
    ) -> Result<(), String> {
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
        state.editor_hosts.insert(
            pane,
            crate::iced_editor_surface::EditorPaneSlot::mounted(binding.host().clone()),
        );
        state.editor_bindings.insert(pane, binding);
        state.mounted_documents.insert(pane, document_id);
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
            EditorRuntimeIntent::Mount { pane, view, load } => {
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
                Ok(None)
            }
            EditorRuntimeIntent::ShowSpellingMenu(_) => {
                Err("native spelling-menu presentation is not yet available".to_owned())
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
                    .map(|_| ())
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
            async move {
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
            },
            move |result| Message::SaveFinished { window, result },
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
                async move {
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
                },
                move |result| Message::SaveFinished { window, result },
            ));
        }
        Task::batch(tasks)
    }

    fn open_launcher_window(&mut self) -> Task<Message> {
        let (id, task) = window::open(window_settings((900.0, 620.0), (720, 480)));
        self.callbacks.project_window_created(LAUNCHER_CAPABILITY);
        self.windows.insert(id, NativeWindow::Launcher);
        task.map(|_| Message::WindowOpened)
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
        let config = MountedEditorBindingConfig::new(
            MountedEditorSession::Open(CanonicalDocumentLoad::new(
                document.document_id,
                document.body.clone(),
            )),
            project.window,
            state.view(),
            viewport,
            surface_theme,
        );
        match MountedEditorBinding::mount(adapter.as_ref(), config) {
            Ok(binding) => {
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

    fn open_project_window(&mut self, project: NativeProjectWindow) -> Task<Message> {
        let (id, task) = window::open(window_settings(
            (1280.0, 720.0),
            ShellLayout::MIN_WINDOW_SIZE,
        ));
        self.project_windows.insert(project.window, id);
        self.callbacks.project_window_created(project.window);
        let workspace = project
            .project_ui
            .as_ref()
            .map(|project| Box::new(ProjectWorkspace::from_snapshot(project.snapshot.as_ref())));
        let (editor_hosts, editor_bindings, mounted_documents) = workspace.as_deref().map_or_else(
            || (EditorHostSlots::default(), BTreeMap::new(), BTreeMap::new()),
            |workspace| self.mount_initial_editor(&project, workspace),
        );
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
                export_artifacts: BTreeMap::new(),
                autosave: AutosaveState::default(),
            })),
        );
        task.map(|_| Message::WindowOpened)
    }

    fn choose_directory(&mut self, create: bool) -> Task<Message> {
        if self.opening_project {
            return Task::none();
        }
        self.opening_project = true;
        let callbacks = Arc::clone(&self.callbacks);
        Task::perform(
            async move {
                callbacks.choose_project_directory(
                    LAUNCHER_CAPABILITY,
                    if create {
                        "Choose New Project Location"
                    } else {
                        "Open ParchMint Project"
                    },
                )
            },
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
            async move {
                let result = callbacks.open_project(project.clone());
                (project, result)
            },
            |(project, result)| Message::ProjectOpenFinished { project, result },
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
            async move {
                let result = callbacks.create_project(request);
                (project, result)
            },
            |(project, result)| Message::ProjectOpenFinished { project, result },
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
        Task::perform(
            async move { callbacks.close_project(project) },
            move |result| Message::ProjectCloseFinished { window: id, result },
        )
    }

    fn finish_close(&mut self, id: window::Id) -> Task<Message> {
        self.closing_windows.remove(&id);
        let removed = self.windows.remove(&id);
        match removed {
            Some(NativeWindow::Project(state)) => {
                self.project_windows.remove(&state.project.window);
                self.close_failures.remove(&state.project.window);
                self.callbacks
                    .project_window_destroyed(state.project.window);
            }
            Some(NativeWindow::Launcher) => {
                self.callbacks.project_window_destroyed(LAUNCHER_CAPABILITY)
            }
            None => {}
        }
        if self.windows.is_empty() {
            Task::batch([window::close(id), iced::exit()])
        } else {
            window::close(id)
        }
    }
}

struct ClipboardMutation {
    session: SharedEditorSession,
    revision: EditorRevision,
    presentation_error: Option<String>,
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
        presentation_error,
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
    if let Err(error) = adapter.paste_untrusted_at(
        request.editor_session.clone(),
        request.view,
        request.selection,
        request.revision,
        source,
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
    if revision == before {
        if let Some(error) = presentation_error {
            return Err(error);
        }
        return Ok(None);
    }
    Ok(Some(ClipboardMutation {
        session: request.editor_session.clone(),
        revision,
        presentation_error,
    }))
}

fn set_link_command(selection: EditorSelection, target: Option<String>) -> EditorCommandKind {
    EditorCommandKind::SetLink {
        range: selection,
        target,
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
    fn untrusted_rich_paste_is_sanitized_and_applied_as_bounded_plain_text() {
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
        assert_eq!(projection.body(), "<p>Keep</p>");
        assert_eq!(projection.semantic().plain_text(), "Keep");
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
            documents: vec![parchmint_application::DocumentSnapshot {
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
        let _duplicate = desktop.update(Message::SystemAppearanceEvent(SystemAppearanceEvent {
            generation: 2,
            appearance: SystemAppearance::Dark,
        }));

        assert_eq!(desktop.appearance, ResolvedAppearance::Dark);
        assert_eq!(
            callbacks
                .system_appearances
                .lock()
                .expect("system appearance mutex poisoned")
                .as_slice(),
            [ResolvedAppearance::Dark, ResolvedAppearance::Light]
        );
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
