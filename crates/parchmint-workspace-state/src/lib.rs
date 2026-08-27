//! Versioned, application-only workspace state for one ParchMint project.
//!
//! Workspace files contain arrangement data only. They are intentionally
//! separate from project files, project saves, undo, and History.

use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    fmt,
    fs::{self, OpenOptions},
    future::Future,
    io::Write,
    path::{Path, PathBuf},
    pin::Pin,
    sync::{
        Mutex,
        atomic::{AtomicU64, Ordering},
    },
};

#[cfg(unix)]
use std::fs::File;

use parchmint_domain::{NodeId, ProjectId, ViewId};
use serde::{Deserialize, Serialize};

const WORKSPACE_FILE_VERSION: u32 = 1;
static TEMPORARY_SEQUENCE: AtomicU64 = AtomicU64::new(1);

/// A `Send` future returned by a workspace-state operation.
pub type WorkspaceFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// One revision of a saved workspace file.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct WorkspaceRevision(u64);

impl WorkspaceRevision {
    pub const fn value(self) -> u64 {
        self.0
    }

    pub const fn next(self) -> Self {
        Self(self.0.saturating_add(1))
    }
}

impl From<u64> for WorkspaceRevision {
    fn from(value: u64) -> Self {
        Self(value)
    }
}

/// Stable application-data key for one project.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ProjectIdentity(ProjectId);

impl ProjectIdentity {
    pub const fn new(project: ProjectId) -> Self {
        Self(project)
    }

    pub const fn project_id(self) -> ProjectId {
        self.0
    }
}

/// Persisted dimensions and visibility of the workspace panes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PaneLayout {
    pub explorer_width: u32,
    pub inspector_width: u32,
    pub split_ratio: f64,
    pub explorer_collapsed: bool,
    pub inspector_collapsed: bool,
    pub companion_open: bool,
}

impl Default for PaneLayout {
    fn default() -> Self {
        Self {
            explorer_width: 280,
            inspector_width: 360,
            split_ratio: 0.5,
            explorer_collapsed: false,
            inspector_collapsed: false,
            companion_open: false,
        }
    }
}

/// Expanded Explorer sections, keyed by project nodes.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ExplorerWorkspaceState {
    pub expanded_sections: BTreeSet<NodeId>,
    /// The hierarchy selection that supplies the Inspector context on reopen.
    pub selected_nodes: BTreeSet<NodeId>,
}

/// One open tab and the view it displays.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OpenTabState {
    pub view: ViewId,
    pub node: NodeId,
}

/// Non-authored state belonging to one rendered view.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SavedViewState {
    pub node: NodeId,
    pub scroll_offset: u64,
}

/// The mutually exclusive top-level workspace mode.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum WorkspaceMode {
    #[default]
    Editor,
    Cards,
}

/// The application-only snapshot saved for a project workspace.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct WorkspaceSnapshot {
    pub layout: PaneLayout,
    pub explorer: ExplorerWorkspaceState,
    pub tabs: Vec<OpenTabState>,
    pub active_view: Option<ViewId>,
    pub views: BTreeMap<ViewId, SavedViewState>,
    pub mode: WorkspaceMode,
    /// The root section currently projected in Cards, when it still exists.
    pub cards_section: Option<NodeId>,
}

impl WorkspaceSnapshot {
    /// Drops workspace references to nodes that are no longer in the project.
    pub fn remove_missing_nodes(&mut self, nodes: &BTreeSet<NodeId>) {
        self.explorer
            .expanded_sections
            .retain(|node| nodes.contains(node));
        self.explorer
            .selected_nodes
            .retain(|node| nodes.contains(node));
        self.tabs.retain(|tab| nodes.contains(&tab.node));
        self.views.retain(|_, view| nodes.contains(&view.node));
        if self
            .active_view
            .is_some_and(|active| !self.tabs.iter().any(|tab| tab.view == active))
        {
            self.active_view = None;
        }
        if self
            .cards_section
            .is_some_and(|section| !nodes.contains(&section))
        {
            self.cards_section = None;
        }
    }
}

/// A load result that distinguishes a missing file from an invalid one.
#[derive(Debug, Clone, PartialEq)]
pub struct RestoredWorkspace {
    pub snapshot: WorkspaceSnapshot,
    pub warning: Option<WorkspaceWarning>,
}

/// A non-fatal problem encountered while restoring workspace data.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkspaceWarning {
    InvalidFile { path: PathBuf, reason: String },
}

/// A storage or decoding failure for application-only workspace data.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkspaceError {
    InvalidFile {
        path: PathBuf,
        reason: String,
    },
    Storage {
        operation: &'static str,
        path: PathBuf,
        reason: String,
    },
}

impl fmt::Display for WorkspaceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidFile { path, reason } => {
                write!(
                    formatter,
                    "invalid workspace file {}: {reason}",
                    path.display()
                )
            }
            Self::Storage {
                operation,
                path,
                reason,
            } => write!(
                formatter,
                "could not {operation} workspace file {}: {reason}",
                path.display()
            ),
        }
    }
}

impl Error for WorkspaceError {}

/// Durable application-data storage for project workspace snapshots.
pub trait WorkspaceStateStore: Send + Sync {
    fn load(
        &self,
        project: ProjectIdentity,
    ) -> WorkspaceFuture<'_, Result<Option<WorkspaceSnapshot>, WorkspaceError>>;

    fn save(
        &self,
        project: ProjectIdentity,
        snapshot: &WorkspaceSnapshot,
    ) -> WorkspaceFuture<'_, Result<WorkspaceRevision, WorkspaceError>>;

    fn remove(&self, project: ProjectIdentity) -> WorkspaceFuture<'_, Result<(), WorkspaceError>>;
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredWorkspace {
    version: u32,
    revision: u64,
    layout: PaneLayout,
    explorer: StoredExplorerState,
    tabs: Vec<StoredTabState>,
    active_view: Option<String>,
    views: Vec<StoredViewState>,
    mode: WorkspaceMode,
    #[serde(default)]
    cards_section: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredExplorerState {
    expanded_sections: Vec<String>,
    #[serde(default)]
    selected_nodes: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredTabState {
    view: String,
    node: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredViewState {
    view: String,
    node: String,
    scroll_offset: u64,
}

/// Native application-data store using one versioned JSON file per project.
#[derive(Debug)]
pub struct FileWorkspaceStateStore {
    directory: PathBuf,
    operations: Mutex<()>,
}

impl FileWorkspaceStateStore {
    pub fn new(directory: impl Into<PathBuf>) -> Self {
        Self {
            directory: directory.into(),
            operations: Mutex::new(()),
        }
    }

    pub fn directory(&self) -> &Path {
        &self.directory
    }

    /// Returns the application-data path for a project's workspace file.
    pub fn path_for(&self, project: ProjectIdentity) -> PathBuf {
        self.directory.join(format!(
            "{}.workspace.json",
            encode_id(project.project_id().as_bytes())
        ))
    }

    /// Loads and prunes a saved workspace, using a default snapshot when the
    /// file is missing or invalid. Invalid data is preserved for diagnosis.
    pub fn load_or_default(
        &self,
        project: ProjectIdentity,
        nodes: &BTreeSet<NodeId>,
    ) -> WorkspaceFuture<'_, Result<RestoredWorkspace, WorkspaceError>> {
        let result = match self.load_now(project) {
            Ok(Some(mut snapshot)) => {
                snapshot.remove_missing_nodes(nodes);
                Ok(RestoredWorkspace {
                    snapshot,
                    warning: None,
                })
            }
            Ok(None) => Ok(RestoredWorkspace {
                snapshot: WorkspaceSnapshot::default(),
                warning: None,
            }),
            Err(WorkspaceError::InvalidFile { path, reason }) => Ok(RestoredWorkspace {
                snapshot: WorkspaceSnapshot::default(),
                warning: Some(WorkspaceWarning::InvalidFile { path, reason }),
            }),
            Err(error) => Err(error),
        };
        Box::pin(async move { result })
    }

    fn load_now(
        &self,
        project: ProjectIdentity,
    ) -> Result<Option<WorkspaceSnapshot>, WorkspaceError> {
        self.read_stored(project)?
            .map(decode_snapshot)
            .transpose()
            .map_err(|reason| self.invalid_file(&self.path_for(project), reason))
    }

    fn revision_now(&self, project: ProjectIdentity) -> Result<WorkspaceRevision, WorkspaceError> {
        Ok(self
            .read_stored(project)?
            .map(|stored| WorkspaceRevision::from(stored.revision))
            .unwrap_or_default())
    }

    fn read_stored(
        &self,
        project: ProjectIdentity,
    ) -> Result<Option<StoredWorkspace>, WorkspaceError> {
        let path = self.path_for(project);
        let bytes = match fs::read(&path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(self.storage("read", &path, error.to_string())),
        };
        let stored: StoredWorkspace = serde_json::from_slice(&bytes)
            .map_err(|error| self.invalid_file(&path, error.to_string()))?;
        if stored.version != WORKSPACE_FILE_VERSION {
            return Err(self.invalid_file(
                &path,
                format!("unsupported workspace version {}", stored.version),
            ));
        }
        Ok(Some(stored))
    }

    fn save_now(
        &self,
        project: ProjectIdentity,
        snapshot: &WorkspaceSnapshot,
    ) -> Result<WorkspaceRevision, WorkspaceError> {
        let _operations = self
            .operations
            .lock()
            .expect("workspace store mutex poisoned");
        fs::create_dir_all(&self.directory).map_err(|error| {
            self.storage(
                "create application-data directory",
                &self.directory,
                error.to_string(),
            )
        })?;
        let revision = self.revision_now(project)?.next();
        let encoded = serde_json::to_vec(&encode_snapshot(snapshot, revision))
            .map_err(|error| self.storage("encode", &self.path_for(project), error.to_string()))?;
        self.replace_durably(&self.path_for(project), &encoded)?;
        Ok(revision)
    }

    fn remove_now(&self, project: ProjectIdentity) -> Result<(), WorkspaceError> {
        let _operations = self
            .operations
            .lock()
            .expect("workspace store mutex poisoned");
        let path = self.path_for(project);
        match fs::remove_file(&path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(self.storage("remove", &path, error.to_string())),
        }
    }

    fn replace_durably(&self, path: &Path, bytes: &[u8]) -> Result<(), WorkspaceError> {
        let temporary = self.temporary_path(path)?;
        let result = (|| {
            let mut file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&temporary)
                .map_err(|error| self.storage("create temporary", path, error.to_string()))?;
            file.write_all(bytes)
                .map_err(|error| self.storage("write temporary", path, error.to_string()))?;
            file.sync_all()
                .map_err(|error| self.storage("flush temporary", path, error.to_string()))?;
            drop(file);
            fs::rename(&temporary, path)
                .map_err(|error| self.storage("replace", path, error.to_string()))?;
            sync_directory(&self.directory)
                .map_err(|error| self.storage("flush directory", path, error.to_string()))
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temporary);
        }
        result
    }

    fn temporary_path(&self, path: &Path) -> Result<PathBuf, WorkspaceError> {
        let file_name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("workspace");
        for _ in 0..32 {
            let sequence = TEMPORARY_SEQUENCE.fetch_add(1, Ordering::Relaxed);
            let temporary = self.directory.join(format!(
                ".{file_name}.{}.{}.tmp",
                std::process::id(),
                sequence
            ));
            if !temporary.exists() {
                return Ok(temporary);
            }
        }
        Err(self.storage(
            "allocate temporary",
            path,
            "could not allocate a unique temporary file",
        ))
    }

    fn invalid_file(&self, path: &Path, reason: impl Into<String>) -> WorkspaceError {
        WorkspaceError::InvalidFile {
            path: path.to_path_buf(),
            reason: reason.into(),
        }
    }

    fn storage(
        &self,
        operation: &'static str,
        path: &Path,
        reason: impl Into<String>,
    ) -> WorkspaceError {
        WorkspaceError::Storage {
            operation,
            path: path.to_path_buf(),
            reason: reason.into(),
        }
    }
}

impl WorkspaceStateStore for FileWorkspaceStateStore {
    fn load(
        &self,
        project: ProjectIdentity,
    ) -> WorkspaceFuture<'_, Result<Option<WorkspaceSnapshot>, WorkspaceError>> {
        let result = self.load_now(project);
        Box::pin(async move { result })
    }

    fn save(
        &self,
        project: ProjectIdentity,
        snapshot: &WorkspaceSnapshot,
    ) -> WorkspaceFuture<'_, Result<WorkspaceRevision, WorkspaceError>> {
        let result = self.save_now(project, snapshot);
        Box::pin(async move { result })
    }

    fn remove(&self, project: ProjectIdentity) -> WorkspaceFuture<'_, Result<(), WorkspaceError>> {
        let result = self.remove_now(project);
        Box::pin(async move { result })
    }
}

fn encode_snapshot(snapshot: &WorkspaceSnapshot, revision: WorkspaceRevision) -> StoredWorkspace {
    StoredWorkspace {
        version: WORKSPACE_FILE_VERSION,
        revision: revision.value(),
        layout: snapshot.layout.clone(),
        explorer: StoredExplorerState {
            expanded_sections: snapshot
                .explorer
                .expanded_sections
                .iter()
                .map(|node| encode_id(node.as_bytes()))
                .collect(),
            selected_nodes: snapshot
                .explorer
                .selected_nodes
                .iter()
                .map(|node| encode_id(node.as_bytes()))
                .collect(),
        },
        tabs: snapshot
            .tabs
            .iter()
            .map(|tab| StoredTabState {
                view: encode_id(tab.view.as_bytes()),
                node: encode_id(tab.node.as_bytes()),
            })
            .collect(),
        active_view: snapshot.active_view.map(|view| encode_id(view.as_bytes())),
        views: snapshot
            .views
            .iter()
            .map(|(view, state)| StoredViewState {
                view: encode_id(view.as_bytes()),
                node: encode_id(state.node.as_bytes()),
                scroll_offset: state.scroll_offset,
            })
            .collect(),
        mode: snapshot.mode,
        cards_section: snapshot
            .cards_section
            .map(|section| encode_id(section.as_bytes())),
    }
}

fn decode_snapshot(stored: StoredWorkspace) -> Result<WorkspaceSnapshot, String> {
    let expanded_sections = stored
        .explorer
        .expanded_sections
        .iter()
        .map(|node| decode_node_id(node))
        .collect::<Result<BTreeSet<_>, _>>()?;
    let selected_nodes = stored
        .explorer
        .selected_nodes
        .iter()
        .map(|node| decode_node_id(node))
        .collect::<Result<BTreeSet<_>, _>>()?;
    let tabs = stored
        .tabs
        .iter()
        .map(|tab| {
            Ok(OpenTabState {
                view: decode_view_id(&tab.view)?,
                node: decode_node_id(&tab.node)?,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    let active_view = stored
        .active_view
        .as_deref()
        .map(decode_view_id)
        .transpose()?;
    let views = stored
        .views
        .iter()
        .map(|state| {
            Ok((
                decode_view_id(&state.view)?,
                SavedViewState {
                    node: decode_node_id(&state.node)?,
                    scroll_offset: state.scroll_offset,
                },
            ))
        })
        .collect::<Result<BTreeMap<_, _>, String>>()?;
    let cards_section = stored
        .cards_section
        .as_deref()
        .map(decode_node_id)
        .transpose()?;
    Ok(WorkspaceSnapshot {
        layout: stored.layout,
        explorer: ExplorerWorkspaceState {
            expanded_sections,
            selected_nodes,
        },
        tabs,
        active_view,
        views,
        mode: stored.mode,
        cards_section,
    })
}

fn encode_id(bytes: &[u8; 16]) -> String {
    let mut encoded = String::with_capacity(32);
    for byte in bytes {
        use std::fmt::Write as _;
        let _ = write!(encoded, "{byte:02x}");
    }
    encoded
}

fn decode_node_id(encoded: &str) -> Result<NodeId, String> {
    decode_id(encoded).map(NodeId::from_bytes)
}

fn decode_view_id(encoded: &str) -> Result<ViewId, String> {
    decode_id(encoded).map(ViewId::from_bytes)
}

fn decode_id(encoded: &str) -> Result<[u8; 16], String> {
    if encoded.len() != 32 {
        return Err(format!(
            "invalid ID length {}; expected 32 hexadecimal characters",
            encoded.len()
        ));
    }
    let mut bytes = [0; 16];
    for (index, byte) in bytes.iter_mut().enumerate() {
        let start = index * 2;
        *byte = u8::from_str_radix(&encoded[start..start + 2], 16)
            .map_err(|_| format!("invalid hexadecimal ID {encoded:?}"))?;
    }
    Ok(bytes)
}

#[cfg(unix)]
fn sync_directory(path: &Path) -> std::io::Result<()> {
    File::open(path)?.sync_all()
}

#[cfg(not(unix))]
fn sync_directory(path: &Path) -> std::io::Result<()> {
    fs::metadata(path).map(|_| ())
}
