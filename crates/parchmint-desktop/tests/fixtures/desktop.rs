use std::{
    collections::BTreeSet,
    future::Future,
    path::{Path, PathBuf},
    pin::Pin,
    sync::{Arc, Mutex, mpsc},
    task::{Context, Poll, Waker},
};

use parchmint_desktop::{
    ApplicationServices, DesktopBootstrap, DesktopRuntime, DesktopStartup, DesktopUi,
    DesktopUiError, FinalSaveRequest, LaunchIntent, NewProjectRequest, PlatformServices,
    ProjectFilesystemError, ProjectFilesystemService, ProjectSession, RequestedProjectPath,
};
use parchmint_editor_api::EventStream;
use parchmint_platform_api::{
    AsyncResult, SystemAppearance, SystemAppearanceService, WindowCapability,
};
use parchmint_preferences::{
    AppearanceMode, AppearanceService, ApplicationPreferences, PreferenceChange, PreferenceCommand,
    PreferenceError, PreferenceFuture, PreferenceRevision, PreferenceService, PreferenceSnapshot,
    ResolvedAppearance, ThemeSnapshot,
};
use parchmint_ui_api::ProjectSessionCapability;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    Started {
        appearance: ThemeSnapshot,
        intent: LaunchIntent,
    },
    Opened {
        project: PathBuf,
        window: WindowCapability,
        session: ProjectSessionCapability,
    },
    Focused(WindowCapability),
    Locked(PathBuf),
    Retained(WindowCapability),
    Closed(WindowCapability),
    SaveFailed(WindowCapability),
    Ran,
}

#[derive(Default)]
pub struct RecordingUi {
    events: Mutex<Vec<Event>>,
    fail_project_open: Mutex<bool>,
    received_application_services: Mutex<bool>,
}

impl RecordingUi {
    pub fn events(&self) -> Vec<Event> {
        self.events
            .lock()
            .expect("UI events mutex poisoned")
            .clone()
    }

    pub fn fail_next_project_open(&self) {
        *self
            .fail_project_open
            .lock()
            .expect("UI failure mutex poisoned") = true;
    }

    pub fn received_application_services(&self) -> bool {
        *self
            .received_application_services
            .lock()
            .expect("application services mutex poisoned")
    }

    fn event(&self, event: Event) {
        self.events
            .lock()
            .expect("UI events mutex poisoned")
            .push(event);
    }
}

impl DesktopUi for RecordingUi {
    fn start(&self, startup: DesktopStartup) -> Result<(), DesktopUiError> {
        let _runtime: DesktopRuntime = startup.runtime;
        *self
            .received_application_services
            .lock()
            .expect("application services mutex poisoned") = startup
            .services
            .application
            .is::<FixtureApplicationServices>();
        self.event(Event::Started {
            appearance: startup.appearance,
            intent: startup.launch_intent,
        });
        Ok(())
    }

    fn project_opened(
        &self,
        project: &RequestedProjectPath,
        window: WindowCapability,
        session: ProjectSessionCapability,
    ) -> Result<(), DesktopUiError> {
        if std::mem::take(
            &mut *self
                .fail_project_open
                .lock()
                .expect("UI failure mutex poisoned"),
        ) {
            return Err(DesktopUiError::new("injected project-window failure"));
        }
        self.event(Event::Opened {
            project: project.as_path().to_path_buf(),
            window,
            session,
        });
        Ok(())
    }

    fn focus_window(&self, window: WindowCapability) -> Result<(), DesktopUiError> {
        self.event(Event::Focused(window));
        Ok(())
    }

    fn project_locked(&self, project: &RequestedProjectPath) -> Result<(), DesktopUiError> {
        self.event(Event::Locked(project.as_path().to_path_buf()));
        Ok(())
    }

    fn retain_window_for_final_save(&self, window: WindowCapability) -> Result<(), DesktopUiError> {
        self.event(Event::Retained(window));
        Ok(())
    }

    fn project_closed(&self, window: WindowCapability) -> Result<(), DesktopUiError> {
        self.event(Event::Closed(window));
        Ok(())
    }

    fn final_save_failed(
        &self,
        window: WindowCapability,
        _error: &ProjectFilesystemError,
    ) -> Result<(), DesktopUiError> {
        self.event(Event::SaveFailed(window));
        Ok(())
    }

    fn run(&self, _runtime: DesktopRuntime) -> Result<parchmint_desktop::ExitCode, DesktopUiError> {
        self.event(Event::Ran);
        Ok(parchmint_desktop::ExitCode::SUCCESS)
    }
}

#[derive(Default)]
pub struct FakeProjectFilesystem {
    locks: Arc<Mutex<BTreeSet<PathBuf>>>,
    begin_save_calls: Mutex<Vec<PathBuf>>,
}

impl FakeProjectFilesystem {
    pub fn shared() -> (Arc<Self>, Arc<Self>) {
        let locks = Arc::new(Mutex::new(BTreeSet::new()));
        (
            Arc::new(Self {
                locks: Arc::clone(&locks),
                begin_save_calls: Mutex::new(Vec::new()),
            }),
            Arc::new(Self {
                locks,
                begin_save_calls: Mutex::new(Vec::new()),
            }),
        )
    }

    pub fn is_locked(&self, project: &Path) -> bool {
        self.locks
            .lock()
            .expect("filesystem locks mutex poisoned")
            .contains(project)
    }

    pub fn final_save_requests(&self) -> Vec<PathBuf> {
        self.begin_save_calls
            .lock()
            .expect("filesystem save requests mutex poisoned")
            .clone()
    }
}

impl ProjectFilesystemService for FakeProjectFilesystem {
    fn create(
        &self,
        request: &NewProjectRequest,
    ) -> Result<Box<dyn ProjectSession>, ProjectFilesystemError> {
        self.open(&RequestedProjectPath::new(&request.destination))
    }

    fn open(
        &self,
        project: &RequestedProjectPath,
    ) -> Result<Box<dyn ProjectSession>, ProjectFilesystemError> {
        let path = project.as_path().to_path_buf();
        let mut locks = self.locks.lock().expect("filesystem locks mutex poisoned");
        if !locks.insert(path.clone()) {
            return Err(ProjectFilesystemError::Locked { path });
        }
        Ok(Box::new(ProjectLease {
            path,
            locks: Arc::clone(&self.locks),
        }))
    }

    fn begin_final_save(&self, session: &dyn ProjectSession) -> Result<(), ProjectFilesystemError> {
        let session = session
            .as_any()
            .downcast_ref::<ProjectLease>()
            .expect("fixture sessions must remain project leases");
        self.begin_save_calls
            .lock()
            .expect("filesystem save requests mutex poisoned")
            .push(session.path.clone());
        Ok(())
    }
}

struct ProjectLease {
    path: PathBuf,
    locks: Arc<Mutex<BTreeSet<PathBuf>>>,
}

impl Drop for ProjectLease {
    fn drop(&mut self) {
        self.locks
            .lock()
            .expect("filesystem locks mutex poisoned")
            .remove(&self.path);
    }
}

pub struct FakePreferences {
    snapshot: Mutex<PreferenceSnapshot>,
}

impl FakePreferences {
    fn new(mode: AppearanceMode) -> Self {
        Self {
            snapshot: Mutex::new(PreferenceSnapshot {
                revision: PreferenceRevision::default(),
                values: ApplicationPreferences {
                    appearance: mode,
                    ..ApplicationPreferences::default()
                },
            }),
        }
    }

    pub fn snapshot(&self) -> PreferenceSnapshot {
        self.snapshot
            .lock()
            .expect("preference snapshot mutex poisoned")
            .clone()
    }
}

impl PreferenceService for FakePreferences {
    fn load(&self) -> PreferenceFuture<'_, Result<PreferenceSnapshot, PreferenceError>> {
        let snapshot = self.snapshot();
        Box::pin(async move { Ok(snapshot) })
    }

    fn update(
        &self,
        expected: PreferenceRevision,
        command: PreferenceCommand,
    ) -> PreferenceFuture<'_, Result<PreferenceSnapshot, PreferenceError>> {
        let result = {
            let mut snapshot = self
                .snapshot
                .lock()
                .expect("preference snapshot mutex poisoned");
            if snapshot.revision != expected {
                Err(PreferenceError::StaleRevision {
                    expected,
                    actual: snapshot.revision,
                })
            } else if let PreferenceCommand::AddRecentProject(project) = command {
                snapshot
                    .values
                    .recent_projects
                    .retain(|existing| existing.path != project.path);
                snapshot.values.recent_projects.insert(0, project);
                snapshot.revision = PreferenceRevision::from(snapshot.revision.value() + 1);
                Ok(snapshot.clone())
            } else {
                Err(PreferenceError::NotInitialized)
            }
        };
        Box::pin(async move { result })
    }

    fn changes(&self) -> EventStream<PreferenceChange> {
        let (_sender, receiver) = mpsc::channel();
        EventStream::from_receiver(receiver)
    }
}

struct FakeAppearance {
    snapshot: Mutex<ThemeSnapshot>,
}

impl Default for FakeAppearance {
    fn default() -> Self {
        Self {
            snapshot: Mutex::new(ThemeSnapshot::new(ResolvedAppearance::Light, 0)),
        }
    }
}

impl AppearanceService for FakeAppearance {
    fn initialize(
        &self,
        preferences: &PreferenceSnapshot,
        system: ResolvedAppearance,
    ) -> Result<ThemeSnapshot, PreferenceError> {
        let appearance = match preferences.values.appearance {
            AppearanceMode::System => system,
            AppearanceMode::Light => ResolvedAppearance::Light,
            AppearanceMode::Dark => ResolvedAppearance::Dark,
        };
        let snapshot = ThemeSnapshot::new(appearance, 1);
        *self.snapshot.lock().expect("appearance mutex poisoned") = snapshot;
        Ok(snapshot)
    }

    fn set_mode(
        &self,
        _expected: PreferenceRevision,
        _mode: AppearanceMode,
    ) -> PreferenceFuture<'_, Result<ThemeSnapshot, PreferenceError>> {
        Box::pin(async { Err(PreferenceError::NotInitialized) })
    }

    fn system_appearance_changed(
        &self,
        _appearance: ResolvedAppearance,
    ) -> Result<Option<ThemeSnapshot>, PreferenceError> {
        Ok(None)
    }

    fn current(&self) -> ThemeSnapshot {
        *self.snapshot.lock().expect("appearance mutex poisoned")
    }

    fn changes(&self) -> EventStream<ThemeSnapshot> {
        let (_sender, receiver) = mpsc::channel();
        EventStream::from_receiver(receiver)
    }
}

struct FakeSystemAppearance(SystemAppearance);

struct FixtureApplicationServices;

impl SystemAppearanceService for FakeSystemAppearance {
    fn current_appearance(&self) -> AsyncResult<SystemAppearance> {
        let appearance = self.0;
        Box::pin(async move { Ok(appearance) })
    }
}

pub struct Fixture {
    pub bootstrap: DesktopBootstrap,
    pub filesystem: Arc<FakeProjectFilesystem>,
    pub preferences: Arc<FakePreferences>,
    pub ui: Arc<RecordingUi>,
}

pub fn fixture(preference_mode: AppearanceMode, system_appearance: SystemAppearance) -> Fixture {
    let filesystem = Arc::new(FakeProjectFilesystem::default());
    fixture_with_filesystem(filesystem, preference_mode, system_appearance)
}

pub fn fixture_with_filesystem(
    filesystem: Arc<FakeProjectFilesystem>,
    preference_mode: AppearanceMode,
    system_appearance: SystemAppearance,
) -> Fixture {
    let ui = Arc::new(RecordingUi::default());
    let preferences = Arc::new(FakePreferences::new(preference_mode));
    let bootstrap = DesktopBootstrap::new(
        Arc::new(FixtureApplicationServices) as ApplicationServices,
        filesystem.clone(),
        preferences.clone(),
        Arc::new(FakeAppearance::default()),
        Arc::new(FakeSystemAppearance(system_appearance)) as PlatformServices,
        ui.clone(),
    );
    Fixture {
        bootstrap,
        filesystem,
        preferences,
        ui,
    }
}

pub fn block_on<T>(future: impl Future<Output = T>) -> T {
    let waker = Waker::noop();
    let mut context = Context::from_waker(waker);
    let mut future = Box::pin(future);
    loop {
        match Pin::as_mut(&mut future).poll(&mut context) {
            Poll::Ready(value) => return value,
            Poll::Pending => std::thread::yield_now(),
        }
    }
}

pub fn final_save_request(runtime: &DesktopRuntime, project: &Path) -> FinalSaveRequest {
    runtime
        .begin_final_save(project)
        .expect("fixture project must begin a final save")
}
