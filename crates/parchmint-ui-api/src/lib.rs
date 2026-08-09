//! Framework-neutral desktop UI boundary.
//!
//! This crate transfers ParchMint values and services into a desktop UI
//! implementation. Concrete widgets, event loops, and native handles belong
//! to implementation crates.

use std::{
    collections::BTreeMap,
    error::Error,
    fmt,
    path::{Path, PathBuf},
    sync::Arc,
};

use parchmint_application::{GlobalReplacement, ProjectCommandDispatcher};
use parchmint_editor_api::EditorAdapter;
use parchmint_platform_api::{
    ApplicationPathService, ClipboardService, DialogService, ExternalOpenService, MenuService,
    SystemAppearanceService, WindowCapability,
};
use parchmint_preferences::{AppearanceService, PreferenceService, ThemeSnapshot};
use parchmint_spellcheck_api::SpellcheckService;
use parchmint_workspace_state::WorkspaceStateStore;

/// The exit status returned when a desktop UI finishes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExitCode(i32);

impl ExitCode {
    pub const SUCCESS: Self = Self(0);

    pub const fn new(value: i32) -> Self {
        Self(value)
    }

    pub const fn value(self) -> i32 {
        self.0
    }
}

/// A UI startup or runtime failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UiError {
    message: String,
}

impl UiError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for UiError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl Error for UiError {}

/// A project path requested at process startup before project services validate it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequestedProjectPath(PathBuf);

impl RequestedProjectPath {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self(path.into())
    }

    pub fn as_path(&self) -> &Path {
        &self.0
    }

    pub fn into_path(self) -> PathBuf {
        self.0
    }
}

/// A ParchMint-owned capability for one live project session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ProjectSessionCapability {
    session_id: u64,
    generation: u64,
}

impl ProjectSessionCapability {
    pub const fn session_id(self) -> u64 {
        self.session_id
    }

    pub const fn generation(self) -> u64 {
        self.generation
    }
}

/// Tracks the live project sessions a UI may scope work to.
///
/// Retiring and recreating a session keeps its logical ID while incrementing
/// its generation, so delayed work cannot target the replacement session.
#[derive(Debug, Default)]
pub struct ProjectSessionRegistry {
    sessions: BTreeMap<u64, SessionState>,
}

#[derive(Debug, Clone, Copy, Default)]
struct SessionState {
    generation: u64,
    live: bool,
}

impl ProjectSessionRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, session_id: u64) -> ProjectSessionCapability {
        let state = self.sessions.entry(session_id).or_default();
        state.generation = state.generation.saturating_add(1);
        state.live = true;
        ProjectSessionCapability {
            session_id,
            generation: state.generation,
        }
    }

    pub fn is_current(&self, capability: ProjectSessionCapability) -> bool {
        self.sessions
            .get(&capability.session_id)
            .is_some_and(|state| state.live && state.generation == capability.generation)
    }

    pub fn retire(&mut self, capability: ProjectSessionCapability) -> bool {
        if !self.is_current(capability) {
            return false;
        }
        self.sessions
            .get_mut(&capability.session_id)
            .expect("current session must have registry state")
            .live = false;
        true
    }
}

/// Application command services used by a desktop UI.
#[derive(Clone)]
pub struct ApplicationServices {
    pub commands: Arc<dyn ProjectCommandDispatcher>,
    pub replacements: Arc<dyn GlobalReplacement>,
}

impl ApplicationServices {
    pub fn new(
        commands: Arc<dyn ProjectCommandDispatcher>,
        replacements: Arc<dyn GlobalReplacement>,
    ) -> Self {
        Self {
            commands,
            replacements,
        }
    }
}

/// Platform services used by a desktop UI.
#[derive(Clone)]
pub struct PlatformServices {
    pub menus: Arc<dyn MenuService>,
    pub dialogs: Arc<dyn DialogService>,
    pub clipboard: Arc<dyn ClipboardService>,
    pub external_open: Arc<dyn ExternalOpenService>,
    pub application_paths: Arc<dyn ApplicationPathService>,
    pub system_appearance: Arc<dyn SystemAppearanceService>,
}

impl PlatformServices {
    pub fn new(
        menus: Arc<dyn MenuService>,
        dialogs: Arc<dyn DialogService>,
        clipboard: Arc<dyn ClipboardService>,
        external_open: Arc<dyn ExternalOpenService>,
        application_paths: Arc<dyn ApplicationPathService>,
        system_appearance: Arc<dyn SystemAppearanceService>,
    ) -> Self {
        Self {
            menus,
            dialogs,
            clipboard,
            external_open,
            application_paths,
            system_appearance,
        }
    }
}

/// Services available to a running desktop UI.
#[derive(Clone)]
pub struct UiPorts {
    pub application: ApplicationServices,
    pub editor: Arc<dyn EditorAdapter>,
    pub spellcheck: Arc<dyn SpellcheckService>,
    pub platform: PlatformServices,
    pub preferences: Arc<dyn PreferenceService>,
    pub appearance: Arc<dyn AppearanceService>,
    pub workspace_state: Arc<dyn WorkspaceStateStore>,
}

impl UiPorts {
    pub fn new(
        application: ApplicationServices,
        editor: Arc<dyn EditorAdapter>,
        spellcheck: Arc<dyn SpellcheckService>,
        platform: PlatformServices,
        preferences: Arc<dyn PreferenceService>,
        appearance: Arc<dyn AppearanceService>,
        workspace_state: Arc<dyn WorkspaceStateStore>,
    ) -> Self {
        Self {
            application,
            editor,
            spellcheck,
            platform,
            preferences,
            appearance,
            workspace_state,
        }
    }
}

/// Values resolved before the UI runtime starts.
pub struct UiStartup {
    pub appearance: ThemeSnapshot,
    pub sessions: ProjectSessionRegistry,
    pub initial_project: Option<RequestedProjectPath>,
}

/// A desktop UI implementation selected by the executable.
pub trait DesktopUi: Send {
    fn run(self: Box<Self>, startup: UiStartup, ports: UiPorts) -> Result<ExitCode, UiError>;
}

/// Applies every appearance event to live windows in ascending logical-ID order.
///
/// Callers retain the complete capability, including its exact generation, in
/// each callback so a native service can reject a window that changed while an
/// event was pending.
pub fn apply_appearance_events(
    snapshots: &[ThemeSnapshot],
    windows: &[WindowCapability],
    mut apply: impl FnMut(WindowCapability, ThemeSnapshot),
) {
    let mut ordered = windows.to_vec();
    ordered.sort_by_key(|window| window.window_id());
    for snapshot in snapshots {
        for window in &ordered {
            apply(*window, *snapshot);
        }
    }
}
