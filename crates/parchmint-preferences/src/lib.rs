//! Application preferences and framework-neutral appearance updates.
//!
//! Preferences stay outside ParchMint projects and never affect project save,
//! undo, History, or export.

use std::{
    error::Error,
    fmt,
    fs::{self, File, OpenOptions},
    future::Future,
    io::Write,
    path::{Path, PathBuf},
    pin::Pin,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
        mpsc,
    },
};

use parchmint_editor_api::EventStream;
use serde::{Deserialize, Serialize};

const PREFERENCE_FILE_VERSION: u32 = 2;
static TEMPORARY_SEQUENCE: AtomicU64 = AtomicU64::new(1);

/// A `Send` future returned by a preference operation.
pub type PreferenceFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// One monotonically increasing revision of the application preference file.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
pub struct PreferenceRevision(u64);

impl PreferenceRevision {
    pub const fn value(self) -> u64 {
        self.0
    }

    pub const fn next(self) -> Self {
        Self(self.0.saturating_add(1))
    }
}

impl From<u64> for PreferenceRevision {
    fn from(value: u64) -> Self {
        Self(value)
    }
}

/// The user's persisted application appearance choice.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum AppearanceMode {
    #[default]
    System,
    Light,
    Dark,
}

/// The concrete appearance resolved before a UI consumes a theme snapshot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResolvedAppearance {
    Light,
    Dark,
}

/// Settings stored outside a ParchMint project.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApplicationPreferences {
    pub appearance: AppearanceMode,
    pub recent_projects: Vec<RecentProject>,
    pub global_dictionary: Vec<String>,
}

/// One typed recent-project entry stored outside project data.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecentProject {
    pub name: String,
    pub path: String,
    pub last_opened_unix_seconds: u64,
}

impl RecentProject {
    pub fn new(
        name: impl Into<String>,
        path: impl Into<String>,
        last_opened_unix_seconds: u64,
    ) -> Self {
        Self {
            name: name.into(),
            path: path.into(),
            last_opened_unix_seconds,
        }
    }

    fn migrated(path: String) -> Self {
        let name = Path::new(&path)
            .file_name()
            .and_then(|name| name.to_str())
            .filter(|name| !name.is_empty())
            .unwrap_or(&path)
            .to_owned();
        Self::new(name, path, 0)
    }
}

/// The complete preference model returned by loads and updates.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreferenceSnapshot {
    pub revision: PreferenceRevision,
    pub values: ApplicationPreferences,
}

/// A single application-only preference change.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PreferenceCommand {
    SetAppearance(AppearanceMode),
    AddRecentProject(RecentProject),
    RemoveRecentProject(String),
    ClearRecentProjects,
    AddGlobalDictionaryWord(String),
    RemoveGlobalDictionaryWord(String),
}

/// A published successful preference update.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreferenceChange {
    pub snapshot: PreferenceSnapshot,
}

/// A failure to load or durably update application preferences.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PreferenceError {
    StaleRevision {
        expected: PreferenceRevision,
        actual: PreferenceRevision,
    },
    UnreadableFile {
        path: PathBuf,
        reason: String,
    },
    Storage {
        operation: &'static str,
        path: PathBuf,
        reason: String,
    },
    NotInitialized,
}

impl fmt::Display for PreferenceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::StaleRevision { expected, actual } => write!(
                formatter,
                "stale preference revision: expected {}, actual {}",
                expected.value(),
                actual.value()
            ),
            Self::UnreadableFile { path, reason } => {
                write!(
                    formatter,
                    "unreadable preference file {}: {reason}",
                    path.display()
                )
            }
            Self::Storage {
                operation,
                path,
                reason,
            } => write!(
                formatter,
                "could not {operation} preference file {}: {reason}",
                path.display()
            ),
            Self::NotInitialized => formatter.write_str("appearance controller is not initialized"),
        }
    }
}

impl Error for PreferenceError {}

/// Durable storage for one application preference file.
pub trait PreferenceStore: Send + Sync {
    fn load(&self) -> PreferenceFuture<'_, Result<PreferenceSnapshot, PreferenceError>>;

    fn compare_and_save(
        &self,
        expected: PreferenceRevision,
        preferences: &ApplicationPreferences,
    ) -> PreferenceFuture<'_, Result<PreferenceSnapshot, PreferenceError>>;
}

/// Application-level coordinator for revision-checked preference commands.
pub trait PreferenceService: Send + Sync {
    fn load(&self) -> PreferenceFuture<'_, Result<PreferenceSnapshot, PreferenceError>>;

    fn update(
        &self,
        expected: PreferenceRevision,
        command: PreferenceCommand,
    ) -> PreferenceFuture<'_, Result<PreferenceSnapshot, PreferenceError>>;

    fn changes(&self) -> EventStream<PreferenceChange>;
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredPreferences {
    version: u32,
    revision: PreferenceRevision,
    preferences: ApplicationPreferences,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredPreferencesV1 {
    version: u32,
    revision: PreferenceRevision,
    preferences: ApplicationPreferencesV1,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ApplicationPreferencesV1 {
    #[serde(default)]
    appearance: AppearanceMode,
    #[serde(default)]
    recent_projects: Vec<String>,
    #[serde(default)]
    global_dictionary: Vec<String>,
}

/// Native, versioned application preference storage.
#[derive(Debug)]
pub struct FilePreferenceStore {
    path: PathBuf,
    operations: Mutex<()>,
}

impl FilePreferenceStore {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            operations: Mutex::new(()),
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    fn load_now(&self) -> Result<PreferenceSnapshot, PreferenceError> {
        let bytes = match fs::read(&self.path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(PreferenceSnapshot {
                    revision: PreferenceRevision::default(),
                    values: ApplicationPreferences::default(),
                });
            }
            Err(error) => return Err(self.unreadable(error.to_string())),
        };
        let version = serde_json::from_slice::<serde_json::Value>(&bytes)
            .ok()
            .and_then(|value| value.get("version").and_then(serde_json::Value::as_u64))
            .ok_or_else(|| self.unreadable("preference file has no valid version"))?;
        match version {
            1 => {
                let stored: StoredPreferencesV1 = serde_json::from_slice(&bytes)
                    .map_err(|error| self.unreadable(error.to_string()))?;
                debug_assert_eq!(stored.version, 1);
                Ok(PreferenceSnapshot {
                    revision: stored.revision,
                    values: ApplicationPreferences {
                        appearance: stored.preferences.appearance,
                        recent_projects: stored
                            .preferences
                            .recent_projects
                            .into_iter()
                            .map(RecentProject::migrated)
                            .collect(),
                        global_dictionary: stored.preferences.global_dictionary,
                    },
                })
            }
            2 => {
                let stored: StoredPreferences = serde_json::from_slice(&bytes)
                    .map_err(|error| self.unreadable(error.to_string()))?;
                Ok(PreferenceSnapshot {
                    revision: stored.revision,
                    values: stored.preferences,
                })
            }
            version => Err(self.unreadable(format!("unsupported preference version {version}"))),
        }
    }

    fn save_now(
        &self,
        expected: PreferenceRevision,
        preferences: &ApplicationPreferences,
    ) -> Result<PreferenceSnapshot, PreferenceError> {
        let _operations = self
            .operations
            .lock()
            .expect("preference store mutex poisoned");
        let current = self.load_now()?;
        if current.revision != expected {
            return Err(PreferenceError::StaleRevision {
                expected,
                actual: current.revision,
            });
        }

        let snapshot = PreferenceSnapshot {
            revision: current.revision.next(),
            values: preferences.clone(),
        };
        let encoded = serde_json::to_vec(&StoredPreferences {
            version: PREFERENCE_FILE_VERSION,
            revision: snapshot.revision,
            preferences: snapshot.values.clone(),
        })
        .map_err(|error| self.storage("encode", error.to_string()))?;
        self.replace_durably(&encoded)?;
        Ok(snapshot)
    }

    fn replace_durably(&self, bytes: &[u8]) -> Result<(), PreferenceError> {
        let parent = self.path.parent().unwrap_or_else(|| Path::new("."));
        let temporary = self.temporary_path(parent)?;
        let result = (|| {
            let mut file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&temporary)
                .map_err(|error| self.storage("create temporary", error.to_string()))?;
            file.write_all(bytes)
                .map_err(|error| self.storage("write temporary", error.to_string()))?;
            file.sync_all()
                .map_err(|error| self.storage("flush temporary", error.to_string()))?;
            drop(file);
            fs::rename(&temporary, &self.path)
                .map_err(|error| self.storage("replace", error.to_string()))?;
            sync_directory(parent)
                .map_err(|error| self.storage("flush directory", error.to_string()))
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temporary);
        }
        result
    }

    fn temporary_path(&self, parent: &Path) -> Result<PathBuf, PreferenceError> {
        for _ in 0..32 {
            let sequence = TEMPORARY_SEQUENCE.fetch_add(1, Ordering::Relaxed);
            let temporary = parent.join(format!(
                ".{}.{}.{}.tmp",
                self.path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or("preferences"),
                std::process::id(),
                sequence
            ));
            if !temporary.exists() {
                return Ok(temporary);
            }
        }
        Err(self.storage(
            "allocate temporary",
            "could not allocate a unique temporary file",
        ))
    }

    fn unreadable(&self, reason: impl Into<String>) -> PreferenceError {
        PreferenceError::UnreadableFile {
            path: self.path.clone(),
            reason: reason.into(),
        }
    }

    fn storage(&self, operation: &'static str, reason: impl Into<String>) -> PreferenceError {
        PreferenceError::Storage {
            operation,
            path: self.path.clone(),
            reason: reason.into(),
        }
    }
}

impl PreferenceStore for FilePreferenceStore {
    fn load(&self) -> PreferenceFuture<'_, Result<PreferenceSnapshot, PreferenceError>> {
        let result = self.load_now();
        Box::pin(async move { result })
    }

    fn compare_and_save(
        &self,
        expected: PreferenceRevision,
        preferences: &ApplicationPreferences,
    ) -> PreferenceFuture<'_, Result<PreferenceSnapshot, PreferenceError>> {
        let result = self.save_now(expected, preferences);
        Box::pin(async move { result })
    }
}

/// Coordinates revision-checked application preference commands.
pub struct PreferenceCoordinator {
    store: Arc<dyn PreferenceStore>,
    state: Mutex<Option<PreferenceSnapshot>>,
    subscribers: Mutex<Vec<mpsc::Sender<PreferenceChange>>>,
}

impl PreferenceCoordinator {
    pub fn new(store: Arc<dyn PreferenceStore>) -> Self {
        Self {
            store,
            state: Mutex::new(None),
            subscribers: Mutex::new(Vec::new()),
        }
    }

    fn publish(&self, change: PreferenceChange) {
        let mut subscribers = self
            .subscribers
            .lock()
            .expect("preference subscribers mutex poisoned");
        subscribers.retain(|subscriber| subscriber.send(change.clone()).is_ok());
    }
}

impl PreferenceService for PreferenceCoordinator {
    fn load(&self) -> PreferenceFuture<'_, Result<PreferenceSnapshot, PreferenceError>> {
        let store = Arc::clone(&self.store);
        let state = &self.state;
        Box::pin(async move {
            let snapshot = store.load().await?;
            *state.lock().expect("preference coordinator mutex poisoned") = Some(snapshot.clone());
            Ok(snapshot)
        })
    }

    fn update(
        &self,
        expected: PreferenceRevision,
        command: PreferenceCommand,
    ) -> PreferenceFuture<'_, Result<PreferenceSnapshot, PreferenceError>> {
        let store = Arc::clone(&self.store);
        let state = &self.state;
        let coordinator = self;
        Box::pin(async move {
            let cached = {
                state
                    .lock()
                    .expect("preference coordinator mutex poisoned")
                    .clone()
            };
            let current = match cached {
                Some(snapshot) => snapshot,
                None => store.load().await?,
            };
            if current.revision != expected {
                return Err(PreferenceError::StaleRevision {
                    expected,
                    actual: current.revision,
                });
            }

            let mut values = current.values;
            apply_command(&mut values, command);
            let snapshot = store.compare_and_save(expected, &values).await?;
            *state.lock().expect("preference coordinator mutex poisoned") = Some(snapshot.clone());
            coordinator.publish(PreferenceChange {
                snapshot: snapshot.clone(),
            });
            Ok(snapshot)
        })
    }

    fn changes(&self) -> EventStream<PreferenceChange> {
        let (sender, receiver) = mpsc::channel();
        self.subscribers
            .lock()
            .expect("preference subscribers mutex poisoned")
            .push(sender);
        EventStream::from_receiver(receiver)
    }
}

fn apply_command(values: &mut ApplicationPreferences, command: PreferenceCommand) {
    match command {
        PreferenceCommand::SetAppearance(mode) => values.appearance = mode,
        PreferenceCommand::AddRecentProject(project) => {
            values
                .recent_projects
                .retain(|existing| existing.path != project.path);
            values.recent_projects.insert(0, project);
            values.recent_projects.sort_by(|left, right| {
                right
                    .last_opened_unix_seconds
                    .cmp(&left.last_opened_unix_seconds)
            });
        }
        PreferenceCommand::RemoveRecentProject(project) => {
            values
                .recent_projects
                .retain(|existing| existing.path != project);
        }
        PreferenceCommand::ClearRecentProjects => values.recent_projects.clear(),
        PreferenceCommand::AddGlobalDictionaryWord(word) => {
            if !values.global_dictionary.contains(&word) {
                values.global_dictionary.push(word);
                values.global_dictionary.sort();
            }
        }
        PreferenceCommand::RemoveGlobalDictionaryWord(word) => {
            values
                .global_dictionary
                .retain(|existing| existing != &word);
        }
    }
}

/// An immutable, numbered appearance update for every window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ThemeSnapshot {
    pub appearance: ResolvedAppearance,
    pub generation: u64,
}

impl ThemeSnapshot {
    pub const fn new(appearance: ResolvedAppearance, generation: u64) -> Self {
        Self {
            appearance,
            generation,
        }
    }

    /// Applies snapshots to windows in ascending ID order.
    pub fn apply_to_windows(snapshots: &[Self], windows: &[u64], mut apply: impl FnMut(u64, Self)) {
        let mut ordered = windows.to_vec();
        ordered.sort_unstable();
        for snapshot in snapshots {
            for &window in &ordered {
                apply(window, *snapshot);
            }
        }
    }
}

/// Framework-neutral appearance events and state transitions.
pub trait AppearanceService: Send + Sync {
    fn initialize(
        &self,
        preferences: &PreferenceSnapshot,
        system: ResolvedAppearance,
    ) -> Result<ThemeSnapshot, PreferenceError>;

    fn set_mode(
        &self,
        expected: PreferenceRevision,
        mode: AppearanceMode,
    ) -> PreferenceFuture<'_, Result<ThemeSnapshot, PreferenceError>>;

    fn system_appearance_changed(
        &self,
        appearance: ResolvedAppearance,
    ) -> Result<Option<ThemeSnapshot>, PreferenceError>;

    fn current(&self) -> ThemeSnapshot;

    fn changes(&self) -> EventStream<ThemeSnapshot>;
}

#[derive(Debug, Clone, Copy)]
struct AppearanceState {
    initialized: bool,
    mode: AppearanceMode,
    system: ResolvedAppearance,
    current: ThemeSnapshot,
}

impl Default for AppearanceState {
    fn default() -> Self {
        Self {
            initialized: false,
            mode: AppearanceMode::System,
            system: ResolvedAppearance::Light,
            current: ThemeSnapshot::new(ResolvedAppearance::Light, 0),
        }
    }
}

/// Resolves persisted appearance preferences and publishes snapshots.
pub struct AppearanceController {
    state: Mutex<AppearanceState>,
    preferences: Arc<dyn PreferenceService>,
    subscribers: Mutex<Vec<mpsc::Sender<ThemeSnapshot>>>,
}

impl AppearanceController {
    pub fn new(preferences: Arc<dyn PreferenceService>) -> Self {
        Self {
            state: Mutex::new(AppearanceState::default()),
            preferences,
            subscribers: Mutex::new(Vec::new()),
        }
    }

    fn publish(&self, snapshot: ThemeSnapshot) {
        let mut subscribers = self
            .subscribers
            .lock()
            .expect("theme subscribers mutex poisoned");
        subscribers.retain(|subscriber| subscriber.send(snapshot).is_ok());
    }

    pub fn initialize(
        &self,
        preferences: &PreferenceSnapshot,
        system: ResolvedAppearance,
    ) -> Result<ThemeSnapshot, PreferenceError> {
        <Self as AppearanceService>::initialize(self, preferences, system)
    }

    pub fn set_mode(
        &self,
        expected: PreferenceRevision,
        mode: AppearanceMode,
    ) -> PreferenceFuture<'_, Result<ThemeSnapshot, PreferenceError>> {
        <Self as AppearanceService>::set_mode(self, expected, mode)
    }

    pub fn system_appearance_changed(
        &self,
        appearance: ResolvedAppearance,
    ) -> Result<Option<ThemeSnapshot>, PreferenceError> {
        <Self as AppearanceService>::system_appearance_changed(self, appearance)
    }

    pub fn current(&self) -> ThemeSnapshot {
        <Self as AppearanceService>::current(self)
    }

    pub fn changes(&self) -> EventStream<ThemeSnapshot> {
        <Self as AppearanceService>::changes(self)
    }
}

impl AppearanceService for AppearanceController {
    fn initialize(
        &self,
        preferences: &PreferenceSnapshot,
        system: ResolvedAppearance,
    ) -> Result<ThemeSnapshot, PreferenceError> {
        let snapshot = {
            let mut state = self.state.lock().expect("appearance state mutex poisoned");
            state.initialized = true;
            state.mode = preferences.values.appearance;
            state.system = system;
            state.current = ThemeSnapshot::new(resolve(state.mode, system), 1);
            state.current
        };
        self.publish(snapshot);
        Ok(snapshot)
    }

    fn set_mode(
        &self,
        expected: PreferenceRevision,
        mode: AppearanceMode,
    ) -> PreferenceFuture<'_, Result<ThemeSnapshot, PreferenceError>> {
        let preferences = Arc::clone(&self.preferences);
        let controller = self;
        Box::pin(async move {
            if !controller
                .state
                .lock()
                .expect("appearance state mutex poisoned")
                .initialized
            {
                return Err(PreferenceError::NotInitialized);
            }
            preferences
                .update(expected, PreferenceCommand::SetAppearance(mode))
                .await?;
            let snapshot = {
                let mut state = controller
                    .state
                    .lock()
                    .expect("appearance state mutex poisoned");
                state.mode = mode;
                state.current = ThemeSnapshot::new(
                    resolve(mode, state.system),
                    state.current.generation.saturating_add(1),
                );
                state.current
            };
            controller.publish(snapshot);
            Ok(snapshot)
        })
    }

    fn system_appearance_changed(
        &self,
        appearance: ResolvedAppearance,
    ) -> Result<Option<ThemeSnapshot>, PreferenceError> {
        let snapshot = {
            let mut state = self.state.lock().expect("appearance state mutex poisoned");
            if !state.initialized {
                return Err(PreferenceError::NotInitialized);
            }
            state.system = appearance;
            if state.mode != AppearanceMode::System || state.current.appearance == appearance {
                None
            } else {
                state.current =
                    ThemeSnapshot::new(appearance, state.current.generation.saturating_add(1));
                Some(state.current)
            }
        };
        if let Some(snapshot) = snapshot {
            self.publish(snapshot);
        }
        Ok(snapshot)
    }

    fn current(&self) -> ThemeSnapshot {
        self.state
            .lock()
            .expect("appearance state mutex poisoned")
            .current
    }

    fn changes(&self) -> EventStream<ThemeSnapshot> {
        let (sender, receiver) = mpsc::channel();
        self.subscribers
            .lock()
            .expect("theme subscribers mutex poisoned")
            .push(sender);
        EventStream::from_receiver(receiver)
    }
}

const fn resolve(mode: AppearanceMode, system: ResolvedAppearance) -> ResolvedAppearance {
    match mode {
        AppearanceMode::System => system,
        AppearanceMode::Light => ResolvedAppearance::Light,
        AppearanceMode::Dark => ResolvedAppearance::Dark,
    }
}

fn sync_directory(path: &Path) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        File::open(path)?.sync_all()
    }
    #[cfg(not(unix))]
    {
        let _ = path;
        Ok(())
    }
}
