//! Native-desktop startup and project-window lifecycle coordination.
//!
//! Production assembles concrete services through [`DesktopBootstrap`], while
//! tests may still inject ready-to-use services at the same boundary.

mod production;

pub use production::{
    ProductionApplicationGraph, ProductionControls, ProductionFaultKind, ProductionFaultPoint,
    ProductionMeasurement, ProductionObservation, ProductionProjectSession,
};

use std::{
    any::Any,
    collections::BTreeMap,
    error::Error,
    ffi::OsString,
    fmt,
    path::{Component, Path, PathBuf},
    sync::{Arc, Mutex, Weak},
    time::{SystemTime, UNIX_EPOCH},
};

use parchmint_platform_api::{SystemAppearance, SystemAppearanceService, WindowCapability};
use parchmint_preferences::{
    AppearanceService, PreferenceCommand, PreferenceError, PreferenceService, RecentProject,
    ResolvedAppearance, ThemeSnapshot,
};
pub use parchmint_ui_api::{ExitCode, RequestedProjectPath, UiError as DesktopUiError};
use parchmint_ui_api::{
    ProjectQueryError, ProjectSessionAuthority, ProjectSessionCapability, ProjectSessionRegistry,
    ProjectUiPorts, ProjectUiProject, ProjectUiServices,
};

/// The intent parsed from a process launch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LaunchIntent {
    Launcher,
    Open(RequestedProjectPath),
}

/// The request supplied to desktop startup.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaunchRequest {
    pub project: Option<RequestedProjectPath>,
}

impl LaunchRequest {
    pub const fn launcher() -> Self {
        Self { project: None }
    }

    pub fn open(path: impl Into<PathBuf>) -> Self {
        Self {
            project: Some(RequestedProjectPath::new(path)),
        }
    }

    pub fn intent(&self) -> LaunchIntent {
        self.project
            .clone()
            .map(LaunchIntent::Open)
            .unwrap_or(LaunchIntent::Launcher)
    }

    /// Parses the optional first path supplied by the operating system.
    pub fn from_environment() -> Self {
        Self::from_args(std::env::args_os())
    }

    /// Parses a process argument sequence whose first value is the executable.
    pub fn from_args(args: impl IntoIterator<Item = impl Into<OsString>>) -> Self {
        args.into_iter()
            .nth(1)
            .map(Into::into)
            .map(PathBuf::from)
            .map(Self::open)
            .unwrap_or_else(Self::launcher)
    }
}

/// Opaque application-wide graph retained by the desktop bootstrap.
pub type ApplicationServices = Arc<dyn Any + Send + Sync>;

/// The platform service needed before the UI starts.
pub type PlatformServices = Arc<dyn SystemAppearanceService>;

/// An acquired writable project session. Dropping it releases its filesystem
/// ownership, including an operating-system project lock where applicable.
pub trait ProjectSession: Send {
    fn as_any(&self) -> &dyn Any;
}

impl<T: Any + Send> ProjectSession for T {
    fn as_any(&self) -> &dyn Any {
        self
    }
}

/// A project filesystem failure relevant to desktop lifecycle decisions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProjectFilesystemError {
    Locked {
        path: PathBuf,
    },
    Failed {
        operation: &'static str,
        reason: String,
    },
}

impl ProjectFilesystemError {
    pub fn failed(operation: &'static str, reason: impl Into<String>) -> Self {
        Self::Failed {
            operation,
            reason: reason.into(),
        }
    }
}

impl fmt::Display for ProjectFilesystemError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Locked { path } => {
                write!(formatter, "project is already locked: {}", path.display())
            }
            Self::Failed { operation, reason } => {
                write!(formatter, "project filesystem {operation} failed: {reason}")
            }
        }
    }
}

impl Error for ProjectFilesystemError {}

/// Filesystem operations the desktop lifecycle needs from an injected or
/// production project graph.
pub trait ProjectFilesystemService: Send + Sync {
    fn create(
        &self,
        _request: &NewProjectRequest,
    ) -> Result<Box<dyn ProjectSession>, ProjectFilesystemError> {
        Err(ProjectFilesystemError::failed(
            "create",
            "project creation is not available from this injected filesystem",
        ))
    }

    fn open(
        &self,
        path: &RequestedProjectPath,
    ) -> Result<Box<dyn ProjectSession>, ProjectFilesystemError>;

    /// Starts the last save for `session`; completion is reported through
    /// [`DesktopRuntime::resolve_final_save`].
    fn begin_final_save(&self, session: &dyn ProjectSession) -> Result<(), ProjectFilesystemError>;

    /// Returns typed UI services backed by this exact writable lease.
    /// Injected filesystems may omit them to preserve the legacy callback seam.
    fn ui_services(
        &self,
        _session: &dyn ProjectSession,
    ) -> Result<Option<ProjectUiServices>, ProjectFilesystemError> {
        Ok(None)
    }
}

/// Ready-to-use services handed to an injected UI.
#[derive(Clone)]
pub struct DesktopServices {
    pub application: ApplicationServices,
    pub project_filesystem: Arc<dyn ProjectFilesystemService>,
    pub preferences: Arc<dyn PreferenceService>,
    pub appearance: Arc<dyn AppearanceService>,
    pub platform: PlatformServices,
}

/// Values available when the UI begins running.
#[derive(Clone)]
pub struct DesktopStartup {
    pub appearance: ThemeSnapshot,
    pub preferences: parchmint_preferences::PreferenceSnapshot,
    pub launch_intent: LaunchIntent,
    pub services: DesktopServices,
    pub runtime: DesktopRuntime,
}

/// UI callbacks driven by the desktop lifecycle.
pub trait DesktopUi: Send + Sync {
    fn start(&self, startup: DesktopStartup) -> Result<(), DesktopUiError>;

    fn project_opened(
        &self,
        project: &RequestedProjectPath,
        window: WindowCapability,
        session: ProjectSessionCapability,
    ) -> Result<(), DesktopUiError>;

    /// Receives authoritative hydration state and typed session-scoped ports.
    ///
    /// The default preserves injected UIs that only need the historical
    /// path/window/session callback.
    fn project_opened_with_ui(
        &self,
        project: &RequestedProjectPath,
        window: WindowCapability,
        project_ui: ProjectUiProject,
    ) -> Result<(), DesktopUiError> {
        self.project_opened(project, window, project_ui.session())
    }

    fn focus_window(&self, window: WindowCapability) -> Result<(), DesktopUiError>;

    fn project_locked(&self, project: &RequestedProjectPath) -> Result<(), DesktopUiError>;

    fn retain_window_for_final_save(&self, window: WindowCapability) -> Result<(), DesktopUiError>;

    fn project_closed(&self, window: WindowCapability) -> Result<(), DesktopUiError>;

    fn final_save_failed(
        &self,
        window: WindowCapability,
        error: &ProjectFilesystemError,
    ) -> Result<(), DesktopUiError>;

    fn run(&self, _runtime: DesktopRuntime) -> Result<ExitCode, DesktopUiError> {
        Ok(ExitCode::SUCCESS)
    }
}

/// A startup failure reported by the injected bootstrap seam.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StartupError {
    Preferences(PreferenceError),
    SystemAppearance(parchmint_platform_api::PlatformError),
    Appearance(PreferenceError),
    Project(ProjectFilesystemError),
    Ui(DesktopUiError),
    Production {
        component: &'static str,
        reason: String,
    },
}

impl fmt::Display for StartupError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Preferences(error) => write!(formatter, "could not load preferences: {error}"),
            Self::SystemAppearance(error) => {
                write!(formatter, "could not read system appearance: {error}")
            }
            Self::Appearance(error) => {
                write!(formatter, "could not initialize appearance: {error}")
            }
            Self::Project(error) => write!(formatter, "could not open project: {error}"),
            Self::Ui(error) => write!(formatter, "could not start desktop UI: {error}"),
            Self::Production { component, reason } => {
                write!(formatter, "could not initialize {component}: {reason}")
            }
        }
    }
}

impl Error for StartupError {}

impl StartupError {
    fn production(component: &'static str, error: impl fmt::Display) -> Self {
        Self::Production {
            component,
            reason: error.to_string(),
        }
    }

    pub fn report_and_exit(self) -> ExitCode {
        eprintln!("ParchMint could not start: {self}");
        ExitCode::new(1)
    }
}

impl From<DesktopUiError> for StartupError {
    fn from(error: DesktopUiError) -> Self {
        Self::Ui(error)
    }
}

impl From<ProjectFilesystemError> for StartupError {
    fn from(error: ProjectFilesystemError) -> Self {
        Self::Project(error)
    }
}

/// Injected desktop startup with a concrete production constructor.
pub struct DesktopBootstrap {
    pub application: ApplicationServices,
    pub project_filesystem: Arc<dyn ProjectFilesystemService>,
    pub preferences: Arc<dyn PreferenceService>,
    pub appearance: Arc<dyn AppearanceService>,
    pub platform: PlatformServices,
    pub ui: Arc<dyn DesktopUi>,
}

impl DesktopBootstrap {
    pub fn new(
        application: ApplicationServices,
        project_filesystem: Arc<dyn ProjectFilesystemService>,
        preferences: Arc<dyn PreferenceService>,
        appearance: Arc<dyn AppearanceService>,
        platform: PlatformServices,
        ui: Arc<dyn DesktopUi>,
    ) -> Self {
        Self {
            application,
            project_filesystem,
            preferences,
            appearance,
            platform,
            ui,
        }
    }

    pub fn production() -> Result<Self, StartupError> {
        production::assemble()
    }

    /// Builds the production graph with explicit integration-test controls.
    pub fn production_with_controls(controls: ProductionControls) -> Result<Self, StartupError> {
        production::assemble_with_controls(controls)
    }

    /// Returns the concrete graph when this bootstrap came from production.
    pub fn production_graph(&self) -> Option<&ProductionApplicationGraph> {
        self.application
            .downcast_ref::<ProductionApplicationGraph>()
    }

    pub async fn start(&self, request: LaunchRequest) -> Result<DesktopRuntime, StartupError> {
        let preferences = self
            .preferences
            .load()
            .await
            .map_err(StartupError::Preferences)?;
        let system = self
            .platform
            .current_appearance()
            .await
            .map_err(StartupError::SystemAppearance)?;
        let appearance = self
            .appearance
            .initialize(&preferences, resolved_appearance(system))
            .map_err(StartupError::Appearance)?;
        let services = DesktopServices {
            application: self.application.clone(),
            project_filesystem: self.project_filesystem.clone(),
            preferences: Arc::clone(&self.preferences),
            appearance: Arc::clone(&self.appearance),
            platform: self.platform.clone(),
        };
        let runtime = DesktopRuntime::new(services.clone(), Arc::clone(&self.ui));
        self.ui
            .start(DesktopStartup {
                appearance,
                preferences,
                launch_intent: request.intent(),
                services,
                runtime: runtime.clone(),
            })
            .map_err(StartupError::Ui)?;

        if let Some(project) = request.project
            && let Err(error) = runtime.open_project(project.into_path())
        {
            return Err(error.into());
        }
        Ok(runtime)
    }

    pub fn run(self, request: LaunchRequest) -> Result<ExitCode, StartupError> {
        let runtime = production::block_on(self.start(request))?;
        self.ui.run(runtime).map_err(StartupError::Ui)
    }
}

fn resolved_appearance(system: SystemAppearance) -> ResolvedAppearance {
    match system {
        SystemAppearance::Light => ResolvedAppearance::Light,
        SystemAppearance::Dark => ResolvedAppearance::Dark,
    }
}

/// The current outcome of a project-open request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpenProjectResult {
    Opened {
        window: WindowCapability,
        session: ProjectSessionCapability,
    },
    Focused(WindowCapability),
    Locked,
}

/// Validated input for creating one new project directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewProjectRequest {
    pub title: String,
    pub destination: PathBuf,
    pub author: Option<String>,
}

impl NewProjectRequest {
    pub fn new(
        title: impl Into<String>,
        destination: impl Into<PathBuf>,
        author: Option<String>,
    ) -> Self {
        Self {
            title: title.into(),
            destination: destination.into(),
            author,
        }
    }

    fn validated(&self) -> Result<Self, DesktopError> {
        let title = self.title.trim();
        if title.is_empty() || title.chars().any(char::is_control) {
            return Err(DesktopError::InvalidNewProject(
                "project title must contain visible text".to_owned(),
            ));
        }
        if self.destination.as_os_str().is_empty()
            || !self.destination.is_absolute()
            || self.destination.file_name().is_none()
            || self
                .destination
                .components()
                .any(|component| matches!(component, Component::ParentDir | Component::CurDir))
        {
            return Err(DesktopError::InvalidNewProject(
                "project destination must be an absolute, non-root directory".to_owned(),
            ));
        }
        let author = self
            .author
            .as_deref()
            .map(str::trim)
            .filter(|author| !author.is_empty())
            .map(str::to_owned);
        if author
            .as_deref()
            .is_some_and(|author| author.chars().any(char::is_control))
        {
            return Err(DesktopError::InvalidNewProject(
                "project author cannot contain control characters".to_owned(),
            ));
        }
        Ok(Self {
            title: title.to_owned(),
            destination: self.destination.clone(),
            author,
        })
    }
}

/// Exact generations retained while the final save runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FinalSaveRequest {
    pub window: WindowCapability,
    pub session: ProjectSessionCapability,
}

/// The effect of a final-save completion.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FinalSaveResolution {
    Closed(WindowCapability),
    SaveFailed,
    IgnoredStale,
}

/// A shared coordinator for project windows and sessions.
///
/// Clones point to the same generation state so a UI callback can reject a
/// delayed result after a project window has closed or reopened.
#[derive(Clone)]
pub struct DesktopRuntime {
    services: DesktopServices,
    ui: Arc<dyn DesktopUi>,
    state: Arc<Mutex<DesktopState>>,
}

impl DesktopRuntime {
    fn new(services: DesktopServices, ui: Arc<dyn DesktopUi>) -> Self {
        Self {
            services,
            ui,
            state: Arc::new(Mutex::new(DesktopState::default())),
        }
    }

    pub fn route_launch(
        &self,
        request: LaunchRequest,
    ) -> Result<Option<OpenProjectResult>, DesktopError> {
        request
            .project
            .map(|project| self.open_project(project.into_path()))
            .transpose()
    }

    pub fn open_project(
        &self,
        project: impl Into<PathBuf>,
    ) -> Result<OpenProjectResult, DesktopError> {
        let project = RequestedProjectPath::new(project);
        let existing = {
            let state = self.state.lock().expect("desktop state mutex poisoned");
            state
                .projects
                .get(project.as_path())
                .map(|live| (live.window, live.session))
        };
        if let Some((window, session)) = existing {
            self.ui.focus_window(window).map_err(DesktopError::Ui)?;
            self.record_recent_project(&project, session, None);
            return Ok(OpenProjectResult::Focused(window));
        }

        let session = match self.services.project_filesystem.open(&project) {
            Ok(session) => session,
            Err(ProjectFilesystemError::Locked { .. }) => {
                self.ui.project_locked(&project).map_err(DesktopError::Ui)?;
                return Ok(OpenProjectResult::Locked);
            }
            Err(error) => return Err(DesktopError::Project(error)),
        };
        let result = self.register_project(project.clone(), session)?;
        if let OpenProjectResult::Opened { session, .. } = result {
            self.record_recent_project(&project, session, None);
        }
        Ok(result)
    }

    /// Creates, opens, and focuses a new project only after repository
    /// validation and durable creation succeed.
    pub fn create_project(
        &self,
        request: NewProjectRequest,
    ) -> Result<OpenProjectResult, DesktopError> {
        let request = request.validated()?;
        let project = RequestedProjectPath::new(request.destination.clone());
        let session = self
            .services
            .project_filesystem
            .create(&request)
            .map_err(DesktopError::Project)?;
        let result = self.register_project(project.clone(), session)?;
        if let OpenProjectResult::Opened { session, .. } = result {
            self.record_recent_project(&project, session, Some(&request.title));
        }
        Ok(result)
    }

    fn register_project(
        &self,
        project: RequestedProjectPath,
        session: Box<dyn ProjectSession>,
    ) -> Result<OpenProjectResult, DesktopError> {
        let ui_services = self
            .services
            .project_filesystem
            .ui_services(session.as_ref())
            .map_err(DesktopError::Project)?;

        let registration = {
            let mut state = self.state.lock().expect("desktop state mutex poisoned");
            if let Some(live) = state.projects.get(project.as_path()) {
                Err(live.window)
            } else {
                Ok(state.register(project.clone(), session, ui_services))
            }
        };

        match registration {
            Err(window) => {
                self.ui.focus_window(window).map_err(DesktopError::Ui)?;
                Ok(OpenProjectResult::Focused(window))
            }
            Ok((window, session)) => {
                let project_ui = match self.project_ui(session) {
                    Ok(project_ui) => project_ui,
                    Err(error) => {
                        self.unregister(project.as_path(), session);
                        return Err(error);
                    }
                };
                let opened = match project_ui {
                    Some(project_ui) => {
                        self.ui.project_opened_with_ui(&project, window, project_ui)
                    }
                    None => self.ui.project_opened(&project, window, session),
                };
                if let Err(error) = opened {
                    self.unregister(project.as_path(), session);
                    return Err(DesktopError::Ui(error));
                }
                Ok(OpenProjectResult::Opened { window, session })
            }
        }
    }

    fn record_recent_project(
        &self,
        project: &RequestedProjectPath,
        session: ProjectSessionCapability,
        preferred_name: Option<&str>,
    ) {
        let snapshot_name = self
            .project_ui(session)
            .ok()
            .flatten()
            .map(|project| project.snapshot.project.display_title.clone())
            .filter(|name| !name.trim().is_empty());
        let fallback = project
            .as_path()
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("Untitled Project");
        let recent = RecentProject::new(
            preferred_name
                .filter(|name| !name.trim().is_empty())
                .map_or_else(
                    || snapshot_name.unwrap_or_else(|| fallback.to_owned()),
                    str::to_owned,
                ),
            project.as_path().display().to_string(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
        );
        if let Ok(current) = production::block_on(self.services.preferences.load()) {
            let _ = production::block_on(self.services.preferences.update(
                current.revision,
                PreferenceCommand::AddRecentProject(recent),
            ));
        }
    }

    /// Starts a final save while keeping the exact live window registered.
    pub fn begin_final_save(&self, project: &Path) -> Result<FinalSaveRequest, DesktopError> {
        let request = {
            let mut state = self.state.lock().expect("desktop state mutex poisoned");
            let live = state
                .projects
                .get_mut(project)
                .ok_or_else(|| DesktopError::MissingProject(project.to_path_buf()))?;
            if live.final_save_pending {
                return Err(DesktopError::FinalSaveAlreadyPending(project.to_path_buf()));
            }
            live.final_save_pending = true;
            FinalSaveRequest {
                window: live.window,
                session: live.session,
            }
        };
        if let Err(error) = self.ui.retain_window_for_final_save(request.window) {
            self.clear_final_save_pending(project, request.session);
            return Err(DesktopError::Ui(error));
        }
        let save_started = {
            let state = self.state.lock().expect("desktop state mutex poisoned");
            let live = state
                .projects
                .get(project)
                .expect("project remains registered while beginning final save");
            self.services
                .project_filesystem
                .begin_final_save(live.session_handle.as_ref())
        };
        if let Err(error) = save_started {
            self.clear_final_save_pending(project, request.session);
            let _ = self.ui.final_save_failed(request.window, &error);
            return Err(DesktopError::Project(error));
        }
        Ok(request)
    }

    /// Applies a background final-save result only if both generations are
    /// still current. A stale result is intentionally ignored.
    pub fn resolve_final_save(
        &self,
        request: FinalSaveRequest,
        result: Result<(), ProjectFilesystemError>,
    ) -> Result<FinalSaveResolution, DesktopError> {
        let project = {
            let state = self.state.lock().expect("desktop state mutex poisoned");
            state.project_for(request)
        };
        let Some(project) = project else {
            return Ok(FinalSaveResolution::IgnoredStale);
        };

        match result {
            Ok(()) => {
                self.unregister(&project, request.session);
                self.ui
                    .project_closed(request.window)
                    .map_err(DesktopError::Ui)?;
                Ok(FinalSaveResolution::Closed(request.window))
            }
            Err(error) => {
                self.clear_final_save_pending(&project, request.session);
                self.ui
                    .final_save_failed(request.window, &error)
                    .map_err(DesktopError::Ui)?;
                Ok(FinalSaveResolution::SaveFailed)
            }
        }
    }

    pub fn is_current_window(&self, window: WindowCapability) -> bool {
        self.state
            .lock()
            .expect("desktop state mutex poisoned")
            .projects
            .values()
            .any(|live| live.window == window)
    }

    pub fn is_current_session(&self, session: ProjectSessionCapability) -> bool {
        self.state
            .lock()
            .expect("desktop state mutex poisoned")
            .session_registry
            .is_current(session)
    }

    pub fn accepts(&self, window: WindowCapability, session: ProjectSessionCapability) -> bool {
        let state = self.state.lock().expect("desktop state mutex poisoned");
        state
            .projects
            .values()
            .any(|live| live.window == window && live.session == session)
            && state.session_registry.is_current(session)
    }

    /// Loads authoritative UI state and ports for the exact current session.
    pub fn project_ui(
        &self,
        session: ProjectSessionCapability,
    ) -> Result<Option<ProjectUiProject>, DesktopError> {
        let services = {
            let state = self.state.lock().expect("desktop state mutex poisoned");
            if !state.session_registry.is_current(session) {
                return Ok(None);
            }
            state
                .projects
                .values()
                .find(|live| live.session == session)
                .and_then(|live| live.ui_services.clone())
        };
        let Some(services) = services else {
            return Ok(None);
        };
        let ports = ProjectUiPorts::new(
            session,
            services,
            Arc::new(DesktopSessionAuthority {
                state: Arc::downgrade(&self.state),
            }),
        );
        let snapshot = ports
            .access()
            .map_err(|error| DesktopError::ProjectQuery(ProjectQueryError::new(error.to_string())))?
            .snapshot(|query| query.snapshot())
            .map_err(|error| DesktopError::ProjectQuery(ProjectQueryError::new(error.to_string())))?
            .map_err(DesktopError::ProjectQuery)?;
        Ok(Some(ProjectUiProject {
            snapshot: Arc::new(snapshot),
            ports,
        }))
    }

    fn clear_final_save_pending(&self, project: &Path, session: ProjectSessionCapability) {
        let mut state = self.state.lock().expect("desktop state mutex poisoned");
        if let Some(live) = state.projects.get_mut(project)
            && live.session == session
        {
            live.final_save_pending = false;
        }
    }

    fn unregister(&self, project: &Path, session: ProjectSessionCapability) {
        let removed = {
            let mut state = self.state.lock().expect("desktop state mutex poisoned");
            let matching = state
                .projects
                .get(project)
                .is_some_and(|live| live.session == session);
            if !matching {
                None
            } else {
                state.session_registry.retire(session);
                state.projects.remove(project)
            }
        };
        drop(removed);
    }
}

/// A lifecycle error after startup has created its injected runtime.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DesktopError {
    Project(ProjectFilesystemError),
    ProjectQuery(ProjectQueryError),
    Ui(DesktopUiError),
    MissingProject(PathBuf),
    FinalSaveAlreadyPending(PathBuf),
    InvalidNewProject(String),
}

impl fmt::Display for DesktopError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Project(error) => error.fmt(formatter),
            Self::ProjectQuery(error) => error.fmt(formatter),
            Self::Ui(error) => error.fmt(formatter),
            Self::MissingProject(path) => {
                write!(formatter, "project is not open: {}", path.display())
            }
            Self::FinalSaveAlreadyPending(path) => {
                write!(
                    formatter,
                    "final save is already pending: {}",
                    path.display()
                )
            }
            Self::InvalidNewProject(reason) => write!(formatter, "invalid new project: {reason}"),
        }
    }
}

impl Error for DesktopError {}

impl From<DesktopError> for StartupError {
    fn from(error: DesktopError) -> Self {
        match error {
            DesktopError::Project(error) => Self::Project(error),
            DesktopError::ProjectQuery(error) => Self::Ui(DesktopUiError::new(error.to_string())),
            DesktopError::Ui(error) => Self::Ui(error),
            other => Self::Ui(DesktopUiError::new(other.to_string())),
        }
    }
}

#[derive(Default)]
struct DesktopState {
    next_project_id: u64,
    identities: BTreeMap<PathBuf, ProjectIdentity>,
    session_registry: ProjectSessionRegistry,
    projects: BTreeMap<PathBuf, LiveProject>,
}

impl DesktopState {
    fn register(
        &mut self,
        project: RequestedProjectPath,
        session_handle: Box<dyn ProjectSession>,
        ui_services: Option<ProjectUiServices>,
    ) -> (WindowCapability, ProjectSessionCapability) {
        let project = project.into_path();
        let identity = self.identities.entry(project.clone()).or_insert_with(|| {
            self.next_project_id = self.next_project_id.saturating_add(1);
            ProjectIdentity {
                id: self.next_project_id,
                window_generation: 0,
            }
        });
        identity.window_generation = identity.window_generation.saturating_add(1);
        let window = WindowCapability::new(identity.id, identity.window_generation);
        let session = self.session_registry.register(identity.id);
        self.projects.insert(
            project,
            LiveProject {
                window,
                session,
                session_handle,
                ui_services,
                final_save_pending: false,
            },
        );
        (window, session)
    }

    fn project_for(&self, request: FinalSaveRequest) -> Option<PathBuf> {
        self.projects.iter().find_map(|(project, live)| {
            (live.window == request.window
                && live.session == request.session
                && live.final_save_pending)
                .then(|| project.clone())
        })
    }
}

struct ProjectIdentity {
    id: u64,
    window_generation: u64,
}

struct LiveProject {
    window: WindowCapability,
    session: ProjectSessionCapability,
    session_handle: Box<dyn ProjectSession>,
    ui_services: Option<ProjectUiServices>,
    final_save_pending: bool,
}

struct DesktopSessionAuthority {
    state: Weak<Mutex<DesktopState>>,
}

impl ProjectSessionAuthority for DesktopSessionAuthority {
    fn is_current(&self, session: ProjectSessionCapability) -> bool {
        self.state
            .upgrade()
            .and_then(|state| {
                state
                    .lock()
                    .ok()
                    .map(|state| state.session_registry.is_current(session))
            })
            .unwrap_or(false)
    }
}
