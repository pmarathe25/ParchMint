use super::dependencies::*;
use super::*;

#[derive(Default)]
pub(super) struct ProductionUiState {
    pub(super) appearance: Option<ResolvedAppearance>,
    projects: BTreeMap<WindowCapability, NativeProjectWindow>,
    locked_project: Option<PathBuf>,
}

pub(super) trait NativeDesktopDriver: Send + Sync {
    fn run(&self, startup: NativeDesktopStartup) -> Result<(), NativeDesktopError>;
}

pub(super) struct IcedDesktopDriver;

impl NativeDesktopDriver for IcedDesktopDriver {
    fn run(&self, startup: NativeDesktopStartup) -> Result<(), NativeDesktopError> {
        run_native_desktop(startup)
    }
}

pub(super) struct ProductionDesktopUi {
    pub(super) state: Mutex<ProductionUiState>,
    pub(super) registry: IcedWindowRegistry,
    pub(super) editor: Arc<EditorIcedAdapter>,
    pub(super) preferences: Arc<dyn PreferenceService>,
    pub(super) appearance: Arc<dyn AppearanceService>,
    pub(super) platform: UiPlatformServices,
    pub(super) controls: ProductionControls,
    pub(super) driver: Arc<dyn NativeDesktopDriver>,
}

impl DesktopUi for ProductionDesktopUi {
    fn start(&self, startup: DesktopStartup) -> Result<(), DesktopUiError> {
        self.state
            .lock()
            .map_err(|_| DesktopUiError::new("Iced desktop state is unavailable"))?
            .appearance = Some(startup.appearance.appearance);
        Ok(())
    }

    fn project_opened(
        &self,
        project: &RequestedProjectPath,
        window: WindowCapability,
        session: parchmint_ui_api::ProjectSessionCapability,
    ) -> Result<(), DesktopUiError> {
        self.state
            .lock()
            .map_err(|_| DesktopUiError::new("Iced desktop state is unavailable"))?
            .projects
            .insert(
                window,
                NativeProjectWindow {
                    project: project.as_path().to_path_buf(),
                    window,
                    session,
                    project_ui: None,
                    editor: Some(self.editor.clone()),
                },
            );
        self.controls.observe(ProductionObservation::WindowOpened {
            window,
            session_id: session.session_id(),
            session_generation: session.generation(),
            typed_ports: false,
            native_editor: true,
        });
        Ok(())
    }

    fn project_opened_with_ui(
        &self,
        project: &RequestedProjectPath,
        window: WindowCapability,
        project_ui: ProjectUiProject,
    ) -> Result<(), DesktopUiError> {
        let session = project_ui.session();
        self.state
            .lock()
            .map_err(|_| DesktopUiError::new("Iced desktop state is unavailable"))?
            .projects
            .insert(
                window,
                NativeProjectWindow::typed(
                    project.as_path().to_path_buf(),
                    window,
                    project_ui,
                    self.editor.clone(),
                ),
            );
        self.controls.observe(ProductionObservation::WindowOpened {
            window,
            session_id: session.session_id(),
            session_generation: session.generation(),
            typed_ports: true,
            native_editor: true,
        });
        Ok(())
    }

    fn focus_window(&self, window: WindowCapability) -> Result<(), DesktopUiError> {
        let known = self
            .state
            .lock()
            .map_err(|_| DesktopUiError::new("Iced desktop state is unavailable"))?
            .projects
            .contains_key(&window);
        if !known {
            return Err(DesktopUiError::new("cannot focus a stale project window"));
        }
        self.controls
            .observe(ProductionObservation::WindowFocused(window));
        Ok(())
    }

    fn project_locked(&self, project: &RequestedProjectPath) -> Result<(), DesktopUiError> {
        self.state
            .lock()
            .map_err(|_| DesktopUiError::new("Iced desktop state is unavailable"))?
            .locked_project = Some(project.as_path().to_path_buf());
        self.controls.observe(ProductionObservation::ProjectLocked {
            path: project.as_path().to_path_buf(),
        });
        Ok(())
    }

    fn retain_window_for_final_save(&self, window: WindowCapability) -> Result<(), DesktopUiError> {
        self.controls
            .observe(ProductionObservation::WindowRetained(window));
        Ok(())
    }

    fn project_closed(&self, window: WindowCapability) -> Result<(), DesktopUiError> {
        self.state
            .lock()
            .map_err(|_| DesktopUiError::new("Iced desktop state is unavailable"))?
            .projects
            .remove(&window);
        self.controls
            .observe(ProductionObservation::WindowClosed(window));
        Ok(())
    }

    fn final_save_failed(
        &self,
        window: WindowCapability,
        error: &ProjectFilesystemError,
    ) -> Result<(), DesktopUiError> {
        self.controls
            .observe(ProductionObservation::FinalSaveFailed {
                window,
                reason: error.to_string(),
            });
        Ok(())
    }

    fn run(&self, runtime: DesktopRuntime) -> Result<parchmint_ui_api::ExitCode, DesktopUiError> {
        self.run_native(runtime, None)
    }

    fn run_with_native_capture(
        &self,
        runtime: DesktopRuntime,
        capture: NativeCaptureRequest,
    ) -> Result<parchmint_ui_api::ExitCode, DesktopUiError> {
        self.run_native(runtime, Some(capture))
    }
}

impl ProductionDesktopUi {
    fn run_native(
        &self,
        runtime: DesktopRuntime,
        capture: Option<NativeCaptureRequest>,
    ) -> Result<parchmint_ui_api::ExitCode, DesktopUiError> {
        let (appearance, projects, locked_project) = {
            let state = self
                .state
                .lock()
                .map_err(|_| DesktopUiError::new("Iced desktop state is unavailable"))?;
            let appearance = state.appearance.ok_or_else(|| {
                DesktopUiError::new("Iced desktop was run before startup completed")
            })?;
            (
                appearance,
                state.projects.values().cloned().collect(),
                state.locked_project.clone(),
            )
        };
        let preferences = block_on(self.preferences.load())
            .map_err(|error| DesktopUiError::new(error.to_string()))?;
        let recent_projects = preferences.values.recent_projects;
        let appearance_mode = capture
            .as_ref()
            .map_or(preferences.values.appearance, |capture| {
                match capture.appearance {
                    ResolvedAppearance::Light => AppearanceMode::Light,
                    ResolvedAppearance::Dark => AppearanceMode::Dark,
                }
            });
        self.driver
            .run(NativeDesktopStartup {
                appearance: capture
                    .as_ref()
                    .map_or(appearance, |capture| capture.appearance),
                appearance_mode,
                recent_projects,
                projects,
                locked_project,
                capture,
                callbacks: Arc::new(ProductionUiCallbacks {
                    runtime,
                    registry: self.registry.clone(),
                    editor: self.editor.clone(),
                    preferences: self.preferences.clone(),
                    appearance: self.appearance.clone(),
                    platform: self.platform.clone(),
                }),
            })
            .map_err(|error| DesktopUiError::new(error.to_string()))?;
        Ok(parchmint_ui_api::ExitCode::SUCCESS)
    }
}

struct ProductionUiCallbacks {
    runtime: DesktopRuntime,
    pub(super) registry: IcedWindowRegistry,
    pub(super) editor: Arc<EditorIcedAdapter>,
    pub(super) preferences: Arc<dyn PreferenceService>,
    pub(super) appearance: Arc<dyn AppearanceService>,
    pub(super) platform: UiPlatformServices,
}

impl NativeDesktopCallbacks for ProductionUiCallbacks {
    fn preference_changes(&self) -> Option<parchmint_editor_api::EventStream<PreferenceChange>> {
        Some(self.preferences.changes())
    }

    fn open_project(&self, project: PathBuf) -> Result<NativeProjectOpenResult, String> {
        let result = self
            .runtime
            .open_project(project.clone())
            .map_err(|error| error.to_string())?;
        Ok(match result {
            crate::OpenProjectResult::Opened { window, session } => {
                let project_ui = self
                    .runtime
                    .project_ui(session)
                    .map_err(|error| error.to_string())?
                    .ok_or_else(|| "production project UI ports are unavailable".to_owned())?;
                NativeProjectOpenResult::Opened(NativeProjectWindow::typed(
                    project,
                    window,
                    project_ui,
                    self.editor.clone(),
                ))
            }
            crate::OpenProjectResult::Focused(window) => NativeProjectOpenResult::Focused(window),
            crate::OpenProjectResult::Locked => NativeProjectOpenResult::Locked,
        })
    }

    fn close_project(&self, project: PathBuf) -> Result<(), String> {
        let request = self
            .runtime
            .begin_final_save(&project)
            .map_err(|error| error.to_string())?;
        match self
            .runtime
            .resolve_final_save(request, Ok(()))
            .map_err(|error| error.to_string())?
        {
            crate::FinalSaveResolution::Closed(_) => Ok(()),
            crate::FinalSaveResolution::SaveFailed => {
                Err(format!("final save failed for {}", project.display()))
            }
            crate::FinalSaveResolution::IgnoredStale => {
                Err(format!("project window is stale: {}", project.display()))
            }
        }
    }

    fn close_clean_project(&self, project: PathBuf) -> Result<(), String> {
        self.runtime
            .close_clean_project(&project)
            .map_err(|error| error.to_string())
    }

    fn create_project(
        &self,
        request: NativeNewProjectRequest,
    ) -> Result<NativeProjectOpenResult, String> {
        let project = request.destination.clone();
        let result = self
            .runtime
            .create_project(NewProjectRequest::new(
                request.title,
                request.destination,
                request.author,
            ))
            .map_err(|error| error.to_string())?;
        Ok(match result {
            crate::OpenProjectResult::Opened { window, session } => {
                let project_ui = self
                    .runtime
                    .project_ui(session)
                    .map_err(|error| error.to_string())?
                    .ok_or_else(|| "production project UI ports are unavailable".to_owned())?;
                NativeProjectOpenResult::Opened(NativeProjectWindow::typed(
                    project,
                    window,
                    project_ui,
                    self.editor.clone(),
                ))
            }
            crate::OpenProjectResult::Focused(window) => NativeProjectOpenResult::Focused(window),
            crate::OpenProjectResult::Locked => NativeProjectOpenResult::Locked,
        })
    }

    fn choose_project_directory(
        &self,
        window: WindowCapability,
        title: &'static str,
    ) -> Result<Option<PathBuf>, String> {
        block_on(self.platform.dialogs.choose_path(
            window,
            PathDialog {
                kind: PathDialogKind::OpenDirectory,
                title: Some(title.to_owned()),
            },
        ))
        .map_err(|error| error.to_string())
        .map(|result| {
            result
                .into_value()
                .map(|selection| selection.as_path().to_path_buf())
        })
    }

    fn set_appearance(&self, mode: AppearanceMode) -> Result<ResolvedAppearance, String> {
        if mode == AppearanceMode::System {
            let system = block_on(self.platform.system_appearance.current_appearance())
                .map(resolved_appearance)
                .map_err(|error| error.to_string())?;
            self.appearance
                .system_appearance_changed(system)
                .map_err(|error| error.to_string())?;
        }
        let current = block_on(self.preferences.load()).map_err(|error| error.to_string())?;
        block_on(self.appearance.set_mode(current.revision, mode))
            .map(|snapshot| snapshot.appearance)
            .map_err(|error| error.to_string())
    }

    fn system_appearance_changed(
        &self,
        appearance: ResolvedAppearance,
    ) -> Result<Option<ResolvedAppearance>, String> {
        self.appearance
            .system_appearance_changed(appearance)
            .map(|snapshot| snapshot.map(|snapshot| snapshot.appearance))
            .map_err(|error| error.to_string())
    }

    fn project_window_created(&self, window: WindowCapability) {
        self.registry.register_window(window);
    }

    fn project_window_destroyed(&self, window: WindowCapability) {
        self.registry.close_window(window);
    }
}
