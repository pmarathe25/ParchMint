use super::dependencies::*;
use super::native_callbacks::{IcedDesktopDriver, ProductionDesktopUi, ProductionUiState};
use super::project_session::ProductionProjectFilesystem;
use super::workflow_adapters::{ControlledExporter, spellcheck_fault};
use super::*;

pub(super) struct SharedServices {
    pub(super) editor: Arc<EditorIcedAdapter>,
    pub(super) spellcheck: Arc<EnUsSpellcheckService>,
    pub(super) dictionary_source: Arc<ProductionDictionarySource>,
    pub(super) exporter: Arc<ControlledExporter>,
    pub(super) workspace_state: Arc<FileWorkspaceStateStore>,
    pub(super) preferences: Arc<dyn PreferenceService>,
    pub(super) appearance: Arc<dyn AppearanceService>,
    pub(super) platform: UiPlatformServices,
    pub(super) controls: ProductionControls,
}

/// Reads the authoritative persisted dictionaries at reload time. Project
/// entries are resolved through their live project query; global entries come
/// from the preference store. The spellcheck worker therefore never owns a
/// second, divergent dictionary store.
pub(super) struct ProductionDictionarySource {
    pub(super) projects: Mutex<BTreeMap<ProjectId, Weak<dyn ProjectSnapshotQuery>>>,
    pub(super) preferences: Arc<dyn PreferenceService>,
}

impl ProductionDictionarySource {
    pub(super) fn new(preferences: Arc<dyn PreferenceService>) -> Self {
        Self {
            projects: Mutex::new(BTreeMap::new()),
            preferences,
        }
    }

    pub(super) fn register_project(
        &self,
        project: ProjectId,
        query: Arc<dyn ProjectSnapshotQuery>,
    ) {
        let mut projects = self
            .projects
            .lock()
            .expect("production dictionary project registry lock");
        projects.retain(|_, query| query.upgrade().is_some());
        projects.insert(project, Arc::downgrade(&query));
    }
}

impl SavedDictionarySource for ProductionDictionarySource {
    fn project_words(
        &self,
        project: SpellcheckProjectId,
        _revision: DictionaryRevision,
    ) -> Result<Vec<String>, DictionaryLoadError> {
        let query = {
            let mut projects = self.projects.lock().map_err(|_| {
                DictionaryLoadError::new("project dictionary registry lock is poisoned")
            })?;
            let query = projects.get(&project).and_then(Weak::upgrade);
            if query.is_none() {
                projects.remove(&project);
            }
            query.ok_or_else(|| {
                DictionaryLoadError::new("project dictionary source is unavailable")
            })?
        };
        let snapshot = query
            .snapshot()
            .map_err(|error| DictionaryLoadError::new(error.to_string()))?;
        Ok(snapshot
            .project
            .dictionary
            .iter()
            .map(str::to_owned)
            .collect())
    }

    fn global_words(
        &self,
        _revision: DictionaryRevision,
    ) -> Result<Vec<String>, DictionaryLoadError> {
        block_on(self.preferences.load())
            .map(|snapshot| snapshot.values.global_dictionary)
            .map_err(|error| DictionaryLoadError::new(error.to_string()))
    }
}

/// Concrete application-wide services retained by the production bootstrap.
pub struct ProductionApplicationGraph {
    shared: Arc<SharedServices>,
}

impl ProductionApplicationGraph {
    pub fn controls(&self) -> &ProductionControls {
        &self.shared.controls
    }

    pub fn editor(&self) -> Arc<EditorIcedAdapter> {
        Arc::clone(&self.shared.editor)
    }

    pub fn spellcheck(&self) -> Arc<dyn SpellcheckService> {
        self.shared.spellcheck.clone()
    }

    pub fn exporter(&self) -> Arc<dyn Exporter> {
        self.shared.exporter.clone()
    }

    pub fn check_spelling(
        &self,
        request: parchmint_spellcheck_api::SpellcheckRequest,
    ) -> SpellcheckOperation<parchmint_spellcheck_api::SpellcheckResultStream> {
        if let Some(kind) = self
            .shared
            .controls
            .take_fault(ProductionFaultPoint::Spellcheck)
        {
            self.shared.controls.service_operation(
                ProductionFaultPoint::Spellcheck,
                "check",
                false,
            );
            return Box::pin(async move { Err(spellcheck_fault(kind)) });
        }
        self.shared.controls.service_operation(
            ProductionFaultPoint::Spellcheck,
            "check scheduled",
            true,
        );
        self.shared.spellcheck.check(request)
    }

    pub fn suggest_spelling(
        &self,
        request: parchmint_spellcheck_api::SuggestionRequest,
    ) -> SpellcheckOperation<Vec<parchmint_spellcheck_api::SpellingSuggestion>> {
        if let Some(kind) = self
            .shared
            .controls
            .take_fault(ProductionFaultPoint::Spellcheck)
        {
            self.shared.controls.service_operation(
                ProductionFaultPoint::Spellcheck,
                "suggest",
                false,
            );
            return Box::pin(async move { Err(spellcheck_fault(kind)) });
        }
        self.shared.controls.service_operation(
            ProductionFaultPoint::Spellcheck,
            "suggest scheduled",
            true,
        );
        self.shared.spellcheck.suggest(request)
    }

    pub fn reload_project_dictionary(
        &self,
        project: ProjectId,
        revision: parchmint_spellcheck_api::DictionaryRevision,
    ) -> SpellcheckOperation<()> {
        if let Some(kind) = self
            .shared
            .controls
            .take_fault(ProductionFaultPoint::Spellcheck)
        {
            self.shared.controls.service_operation(
                ProductionFaultPoint::Spellcheck,
                "reload project dictionary",
                false,
            );
            return Box::pin(async move { Err(spellcheck_fault(kind)) });
        }
        self.shared.controls.service_operation(
            ProductionFaultPoint::Spellcheck,
            "reload project dictionary scheduled",
            true,
        );
        self.shared
            .spellcheck
            .reload_project_dictionary(project, revision)
    }

    pub fn reload_global_dictionary(
        &self,
        revision: parchmint_spellcheck_api::DictionaryRevision,
    ) -> SpellcheckOperation<()> {
        if let Some(kind) = self
            .shared
            .controls
            .take_fault(ProductionFaultPoint::Spellcheck)
        {
            self.shared.controls.service_operation(
                ProductionFaultPoint::Spellcheck,
                "reload global dictionary",
                false,
            );
            return Box::pin(async move { Err(spellcheck_fault(kind)) });
        }
        self.shared.controls.service_operation(
            ProductionFaultPoint::Spellcheck,
            "reload global dictionary scheduled",
            true,
        );
        self.shared.spellcheck.reload_global_dictionary(revision)
    }

    pub fn workspace_state(&self) -> Arc<FileWorkspaceStateStore> {
        Arc::clone(&self.shared.workspace_state)
    }
}

pub(crate) fn assemble() -> Result<DesktopBootstrap, StartupError> {
    let controls = ProductionControls::default();
    assemble_with_controls(controls)
}

pub(crate) fn assemble_with_controls(
    controls: ProductionControls,
) -> Result<DesktopBootstrap, StartupError> {
    let platform = NativePlatform::initialize()
        .map_err(|error| StartupError::production("native platform", error))?;
    assemble_with_platform(controls, platform, Arc::new(IcedDesktopDriver))
}

#[cfg(feature = "interaction-harness")]
pub(super) fn assemble_interaction_harness(
    controls: ProductionControls,
    platform: NativePlatform,
    driver: Arc<dyn super::native_callbacks::NativeDesktopDriver>,
) -> Result<DesktopBootstrap, StartupError> {
    assemble_with_platform(controls, platform, driver)
}

fn assemble_with_platform(
    controls: ProductionControls,
    platform: NativePlatform,
    driver: Arc<dyn super::native_callbacks::NativeDesktopDriver>,
) -> Result<DesktopBootstrap, StartupError> {
    let paths = block_on(platform.application_paths.application_paths())
        .map_err(|error| StartupError::production("application paths", error))?;
    #[cfg(feature = "diagnostics")]
    match diagnostics::configure_file(paths.data()) {
        Ok(path) => diagnostics::event(
            DiagnosticLevel::Info,
            "desktop.startup",
            "production application graph is assembling",
            &[("log_path", path.to_string_lossy().as_ref())],
        ),
        Err(error) => eprintln!("ParchMint diagnostics could not be configured: {error}"),
    }
    let preference_store = Arc::new(FilePreferenceStore::new(
        paths.configuration().join("preferences.json"),
    ));
    let preferences: Arc<dyn PreferenceService> =
        Arc::new(PreferenceCoordinator::new(preference_store));
    let appearance = Arc::new(AppearanceController::new(Arc::clone(&preferences)));
    let dictionary_source = Arc::new(ProductionDictionarySource::new(preferences.clone()));
    let editor = Arc::new(
        EditorIcedAdapter::new(EditorIcedConfig::default())
            .map_err(|error| StartupError::production("editor", error))?,
    );
    let spellcheck = Arc::new(
        EnUsSpellcheckService::new(EnUsSpellcheckConfig {
            saved_dictionaries: dictionary_source.clone(),
            ..EnUsSpellcheckConfig::default()
        })
        .map_err(|error| StartupError::production("spellcheck", error))?,
    );
    let ui_platform = UiPlatformServices::new(
        platform.menus.clone(),
        platform.dialogs.clone(),
        platform.clipboard.clone(),
        platform.external_open.clone(),
        platform.application_paths.clone(),
        platform.appearance.clone(),
    )
    .with_menu_activations(platform.menu_activations.clone());
    let shared = Arc::new(SharedServices {
        editor,
        spellcheck,
        dictionary_source,
        exporter: Arc::new(ControlledExporter {
            inner: HtmlExporter,
            controls: controls.clone(),
        }),
        workspace_state: Arc::new(FileWorkspaceStateStore::new(
            paths.data().join("workspaces"),
        )),
        preferences: preferences.clone(),
        appearance: appearance.clone(),
        platform: ui_platform,
        controls: controls.clone(),
    });
    for component in [
        "platform",
        "preferences",
        "appearance",
        "editor",
        "spellcheck",
        "export",
        "workspace-state",
        "project-service-factory",
        "iced-ui",
    ] {
        controls.observe(ProductionObservation::ComponentReady(component));
    }

    let graph = Arc::new(ProductionApplicationGraph {
        shared: Arc::clone(&shared),
    });
    let application: ApplicationServices = graph;
    let project_filesystem = Arc::new(ProductionProjectFilesystem {
        shared: Arc::clone(&shared),
    });
    let ui = Arc::new(ProductionDesktopUi {
        state: Mutex::new(ProductionUiState::default()),
        registry: platform.iced_window_registry(),
        editor: shared.editor.clone(),
        preferences: preferences.clone(),
        appearance: appearance.clone(),
        platform: shared.platform.clone(),
        controls,
        driver,
    });
    let platform_services: PlatformServices = platform.appearance.clone();

    Ok(DesktopBootstrap::new(
        application,
        project_filesystem,
        preferences,
        appearance,
        platform_services,
        ui,
    ))
}
