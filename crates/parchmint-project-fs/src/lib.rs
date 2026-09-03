//! Native directory-backed project storage and recoverable canonical writes.

use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    fmt,
    fs::{self, File, OpenOptions, TryLockError},
    io::{self, Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex, OnceLock, Weak,
        atomic::{AtomicU64, Ordering},
    },
    time::{SystemTime, UNIX_EPOCH},
};

use parchmint_project_format::{CanonicalCodec, CanonicalRelativePath, ProjectFormatCodec};
use parchmint_project_repository::{
    Abandonment, AtomicWritePlan, AtomicWriter, CommitReceipt, CreateProject, DocumentId,
    OpenProject, ProjectPath, ProjectRepository, ProjectRootCapability as ContractRoot,
    ProjectSnapshot, Reconciliation, RepositoryError, SaveTransactionRecord, StagedWrite,
    ValidationReport, WriteError,
};
use sha2::{Digest, Sha256};

const CONTROL_PATH: &str = ".parchmint/format-version";
const MANIFEST_PATH: &str = "project.toml";
const ROOT_ID_PATH: &str = ".parchmint/root-id";
const LOCK_PATH: &str = ".parchmint/write.lock";
const TRANSACTIONS_PATH: &str = ".parchmint/transactions";
const RECORD_NAME: &str = "record.bin";
const RECORD_MAGIC_V1: &[u8; 8] = b"PMTXN001";
const RECORD_MAGIC_V2: &[u8; 8] = b"PMTXN002";

static UNIQUE_ID: OnceLock<AtomicU64> = OnceLock::new();

fn next_unique_id() -> u64 {
    let seed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos() as u64
        ^ u64::from(std::process::id()).rotate_left(32);
    UNIQUE_ID
        .get_or_init(|| AtomicU64::new(seed.max(1)))
        .fetch_add(1, Ordering::Relaxed)
}

/// A filesystem operation failure with enough context for a durable error state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FsError {
    Missing {
        path: PathBuf,
    },
    AlreadyExists {
        path: PathBuf,
    },
    Locked {
        path: PathBuf,
    },
    UnsafePath {
        path: String,
    },
    Corrupt {
        path: PathBuf,
        reason: String,
    },
    NotLockOwner {
        path: PathBuf,
    },
    Io {
        operation: &'static str,
        path: PathBuf,
        reason: String,
    },
    Injected {
        operation: String,
    },
}

impl FsError {
    fn io(operation: &'static str, path: impl Into<PathBuf>, error: io::Error) -> Self {
        Self::Io {
            operation,
            path: path.into(),
            reason: error.to_string(),
        }
    }

    /// Creates a deterministic failure used by disk-operation fault injectors.
    pub fn injected(operation: impl Into<String>) -> Self {
        Self::Injected {
            operation: operation.into(),
        }
    }
}

impl fmt::Display for FsError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl Error for FsError {}

/// A path supplied before the project root has been validated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UntrustedProjectPath(PathBuf);

impl UntrustedProjectPath {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self(path.into())
    }

    pub fn as_path(&self) -> &Path {
        &self.0
    }
}

/// A validated project root tied to the lock owner that created it.
#[derive(Debug, Clone)]
pub struct ProjectRootCapability {
    path: PathBuf,
    root_id: u64,
    lock_token: String,
    lock_file: Weak<Mutex<File>>,
    identity: FileIdentity,
}

impl ProjectRootCapability {
    fn contract_root(&self) -> ContractRoot {
        ContractRoot::new(self.root_id)
    }

    /// Returns the validated root while this capability owns its write lock.
    pub fn checked_path(&self) -> Result<&Path, FsError> {
        verify_lock_owner(self)?;
        Ok(&self.path)
    }
}

/// The operating-system lock held for one writable project session.
#[derive(Debug)]
pub struct ProjectLockLease {
    file: Arc<Mutex<File>>,
    path: PathBuf,
    token: String,
}

impl Drop for ProjectLockLease {
    fn drop(&mut self) {
        let Ok(mut file) = self.file.lock() else {
            return;
        };
        if read_lock_token(&mut file, &self.path).as_deref() == Ok(self.token.as_str()) {
            let _ = file.set_len(0);
            let _ = file.sync_all();
        }
    }
}

/// Checked filesystem access used by the repository and recovery layers.
pub trait ProjectFileSystem: Send + Sync {
    fn create_root(
        &self,
        path: UntrustedProjectPath,
    ) -> Result<(ProjectRootCapability, ProjectLockLease), FsError>;
    fn acquire(
        &self,
        path: UntrustedProjectPath,
    ) -> Result<(ProjectRootCapability, ProjectLockLease), FsError>;
    fn read(
        &self,
        root: &ProjectRootCapability,
        path: &CanonicalRelativePath,
    ) -> Result<Vec<u8>, FsError>;
    fn transaction_records(
        &self,
        root: &ProjectRootCapability,
    ) -> Result<Vec<SaveTransactionRecord>, FsError>;
}

/// Native implementation backed by an ordinary directory.
#[derive(Debug, Clone, Copy, Default)]
pub struct NativeProjectFileSystem;

impl NativeProjectFileSystem {
    pub const fn new() -> Self {
        Self
    }
}

impl ProjectFileSystem for NativeProjectFileSystem {
    fn create_root(
        &self,
        path: UntrustedProjectPath,
    ) -> Result<(ProjectRootCapability, ProjectLockLease), FsError> {
        let requested = absolute_path(path.as_path())?;
        if requested.exists() {
            return Err(FsError::AlreadyExists { path: requested });
        }
        let parent = requested.parent().ok_or_else(|| FsError::UnsafePath {
            path: requested.display().to_string(),
        })?;
        fs::create_dir_all(parent).map_err(|error| FsError::io("create parent", parent, error))?;
        reject_symlink_chain(parent)?;
        fs::create_dir(&requested)
            .map_err(|error| FsError::io("create project root", &requested, error))?;

        let result = (|| {
            let canonical = fs::canonicalize(&requested)
                .map_err(|error| FsError::io("canonicalize project root", &requested, error))?;
            let control = canonical.join(".parchmint");
            fs::create_dir(&control)
                .map_err(|error| FsError::io("create control directory", &control, error))?;
            sync_directory(&canonical)?;
            let root_id = next_unique_id();
            write_new_synced(
                &canonical.join(ROOT_ID_PATH),
                format!("{root_id:016x}\n").as_bytes(),
            )?;
            let identity = file_identity(&canonical)?;
            acquire_lock(canonical, root_id, identity)
        })();

        if result.is_err() {
            let _ = fs::remove_dir_all(&requested);
        }
        result
    }

    fn acquire(
        &self,
        path: UntrustedProjectPath,
    ) -> Result<(ProjectRootCapability, ProjectLockLease), FsError> {
        let requested = absolute_path(path.as_path())?;
        let metadata = fs::symlink_metadata(&requested).map_err(|error| {
            if error.kind() == io::ErrorKind::NotFound {
                FsError::Missing {
                    path: requested.clone(),
                }
            } else {
                FsError::io("inspect project root", &requested, error)
            }
        })?;
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            return Err(FsError::UnsafePath {
                path: requested.display().to_string(),
            });
        }
        let canonical = fs::canonicalize(&requested)
            .map_err(|error| FsError::io("canonicalize project root", &requested, error))?;
        let root_id_path = canonical.join(ROOT_ID_PATH);
        let root_id_text = fs::read_to_string(&root_id_path).map_err(|error| {
            if error.kind() == io::ErrorKind::NotFound {
                FsError::Missing {
                    path: root_id_path.clone(),
                }
            } else {
                FsError::io("read root identity", &root_id_path, error)
            }
        })?;
        let root_id =
            u64::from_str_radix(root_id_text.trim(), 16).map_err(|error| FsError::Corrupt {
                path: root_id_path,
                reason: error.to_string(),
            })?;
        let identity = file_identity(&canonical)?;
        acquire_lock(canonical, root_id, identity)
    }

    fn read(
        &self,
        root: &ProjectRootCapability,
        path: &CanonicalRelativePath,
    ) -> Result<Vec<u8>, FsError> {
        verify_lock_owner(root)?;
        let checked = checked_target(root, path, false)?;
        if checked.identity == FileIdentity::MISSING {
            return Err(FsError::Missing { path: checked.path });
        }
        fs::read(&checked.path)
            .map_err(|error| FsError::io("read canonical resource", checked.path, error))
    }

    fn transaction_records(
        &self,
        root: &ProjectRootCapability,
    ) -> Result<Vec<SaveTransactionRecord>, FsError> {
        verify_lock_owner(root)?;
        let directory = root.path.join(TRANSACTIONS_PATH);
        let entries = match fs::read_dir(&directory) {
            Ok(entries) => entries,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(FsError::io("read transaction directory", directory, error)),
        };
        let mut records = Vec::new();
        for entry in entries {
            let entry =
                entry.map_err(|error| FsError::io("read transaction entry", &directory, error))?;
            if !entry
                .file_type()
                .map_err(|error| FsError::io("inspect transaction entry", entry.path(), error))?
                .is_dir()
            {
                return Err(FsError::Corrupt {
                    path: entry.path(),
                    reason: "unexpected file in transaction directory".into(),
                });
            }
            let record = read_transaction_file(root, &entry.path().join(RECORD_NAME))?;
            records.push(SaveTransactionRecord::new(
                ContractRoot::new(record.root_id),
                record.generation,
            ));
        }
        records.sort_by_key(|record| record.generation);
        Ok(records)
    }
}

fn absolute_path(path: &Path) -> Result<PathBuf, FsError> {
    if path.as_os_str().is_empty() {
        return Err(FsError::UnsafePath {
            path: String::new(),
        });
    }
    if path.is_absolute() {
        Ok(path.to_path_buf())
    } else {
        std::env::current_dir()
            .map(|directory| directory.join(path))
            .map_err(|error| FsError::io("resolve current directory", path, error))
    }
}

fn acquire_lock(
    path: PathBuf,
    root_id: u64,
    identity: FileIdentity,
) -> Result<(ProjectRootCapability, ProjectLockLease), FsError> {
    let lock_path = path.join(LOCK_PATH);
    let mut file = OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .truncate(false)
        .open(&lock_path)
        .map_err(|error| FsError::io("open project lock", &lock_path, error))?;
    file.try_lock().map_err(|error| match error {
        TryLockError::WouldBlock => FsError::Locked { path: path.clone() },
        TryLockError::Error(error) => FsError::io("acquire project lock", &lock_path, error),
    })?;
    let token = format!("{}-{:016x}", std::process::id(), next_unique_id());
    file.set_len(0)
        .map_err(|error| FsError::io("clear project lock owner", &lock_path, error))?;
    file.write_all(token.as_bytes())
        .map_err(|error| FsError::io("record project lock owner", &lock_path, error))?;
    file.sync_all()
        .map_err(|error| FsError::io("flush project lock owner", &lock_path, error))?;
    let file = Arc::new(Mutex::new(file));
    let root = ProjectRootCapability {
        path,
        root_id,
        lock_token: token.clone(),
        lock_file: Arc::downgrade(&file),
        identity,
    };
    let lease = ProjectLockLease {
        file,
        path: lock_path,
        token,
    };
    Ok((root, lease))
}

fn read_lock_token(file: &mut File, path: &Path) -> Result<String, FsError> {
    file.seek(SeekFrom::Start(0))
        .map_err(|error| FsError::io("seek project lock owner", path, error))?;
    let mut token = String::new();
    file.read_to_string(&mut token)
        .map_err(|error| FsError::io("read project lock owner", path, error))?;
    Ok(token)
}

fn verify_lock_owner(root: &ProjectRootCapability) -> Result<(), FsError> {
    verify_root(root)?;
    let lock_path = root.path.join(LOCK_PATH);
    let metadata = fs::symlink_metadata(&lock_path)
        .map_err(|error| FsError::io("inspect project lock", &lock_path, error))?;
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err(FsError::UnsafePath {
            path: lock_path.display().to_string(),
        });
    }
    let lock_file = root
        .lock_file
        .upgrade()
        .ok_or_else(|| FsError::NotLockOwner {
            path: root.path.clone(),
        })?;
    let mut lock_file = lock_file.lock().map_err(|_| FsError::NotLockOwner {
        path: root.path.clone(),
    })?;
    if read_lock_token(&mut lock_file, &lock_path)?.as_bytes() != root.lock_token.as_bytes() {
        return Err(FsError::NotLockOwner {
            path: root.path.clone(),
        });
    }
    Ok(())
}

fn verify_root(root: &ProjectRootCapability) -> Result<(), FsError> {
    let metadata = fs::symlink_metadata(&root.path)
        .map_err(|error| FsError::io("inspect project root", &root.path, error))?;
    if metadata.file_type().is_symlink()
        || !metadata.is_dir()
        || !same_file_node(file_identity_from_metadata(&metadata), root.identity)
    {
        return Err(FsError::UnsafePath {
            path: root.path.display().to_string(),
        });
    }
    let identity_path = root.path.join(ROOT_ID_PATH);
    let identity_metadata = fs::symlink_metadata(&identity_path)
        .map_err(|error| FsError::io("inspect root identity", &identity_path, error))?;
    if !identity_metadata.is_file() || identity_metadata.file_type().is_symlink() {
        return Err(FsError::UnsafePath {
            path: identity_path.display().to_string(),
        });
    }
    let identity = fs::read_to_string(&identity_path)
        .map_err(|error| FsError::io("read root identity", &identity_path, error))?;
    if u64::from_str_radix(identity.trim(), 16) != Ok(root.root_id) {
        return Err(FsError::Corrupt {
            path: identity_path,
            reason: "root identity changed".into(),
        });
    }
    Ok(())
}

fn reject_symlink_chain(path: &Path) -> Result<(), FsError> {
    let canonical =
        fs::canonicalize(path).map_err(|error| FsError::io("canonicalize path", path, error))?;
    let mut current = PathBuf::new();
    for component in path.components() {
        current.push(component.as_os_str());
        if let Ok(metadata) = fs::symlink_metadata(&current)
            && metadata.file_type().is_symlink()
        {
            return Err(FsError::UnsafePath {
                path: current.display().to_string(),
            });
        }
    }
    if !canonical.is_absolute() {
        return Err(FsError::UnsafePath {
            path: path.display().to_string(),
        });
    }
    Ok(())
}

fn reject_git_worktree(path: &Path) -> Result<(), FsError> {
    let mut existing = path;
    while !existing.exists() {
        existing = existing.parent().ok_or_else(|| FsError::UnsafePath {
            path: path.display().to_string(),
        })?;
    }
    let canonical = fs::canonicalize(existing)
        .map_err(|error| FsError::io("canonicalize creation parent", existing, error))?;
    let temporary_root = fs::canonicalize(std::env::temp_dir()).ok();
    for ancestor in canonical.ancestors() {
        if temporary_root.as_deref() == Some(ancestor) {
            break;
        }
        let marker = ancestor.join(".git");
        match fs::symlink_metadata(&marker) {
            Ok(_) => {
                return Err(FsError::UnsafePath {
                    path: format!("{} is inside a Git worktree", path.display()),
                });
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(FsError::io("inspect Git marker", marker, error)),
        }
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct FileIdentity {
    kind: u8,
    length: u64,
    modified_nanos: u128,
    platform_a: u64,
    platform_b: u64,
}

impl FileIdentity {
    const MISSING: Self = Self {
        kind: 0,
        length: 0,
        modified_nanos: 0,
        platform_a: 0,
        platform_b: 0,
    };
}

fn file_identity(path: &Path) -> Result<FileIdentity, FsError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => Ok(file_identity_from_metadata(&metadata)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(FileIdentity::MISSING),
        Err(error) => Err(FsError::io("inspect file identity", path, error)),
    }
}

fn file_identity_from_metadata(metadata: &fs::Metadata) -> FileIdentity {
    let kind = if metadata.is_dir() {
        1
    } else if metadata.is_file() {
        2
    } else {
        3
    };
    let modified_nanos = metadata
        .modified()
        .ok()
        .and_then(|modified| modified.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |duration| duration.as_nanos());
    let (platform_a, platform_b) = platform_file_identity(metadata);
    FileIdentity {
        kind,
        length: metadata.len(),
        modified_nanos,
        platform_a,
        platform_b,
    }
}

fn same_file_node(first: FileIdentity, second: FileIdentity) -> bool {
    if first.platform_a != 0
        || first.platform_b != 0
        || second.platform_a != 0
        || second.platform_b != 0
    {
        first.kind == second.kind
            && first.platform_a == second.platform_a
            && first.platform_b == second.platform_b
    } else {
        first.kind == second.kind
    }
}

#[cfg(unix)]
fn platform_file_identity(metadata: &fs::Metadata) -> (u64, u64) {
    use std::os::unix::fs::MetadataExt;
    (metadata.dev(), metadata.ino())
}

#[cfg(not(unix))]
fn platform_file_identity(_metadata: &fs::Metadata) -> (u64, u64) {
    (0, 0)
}

#[cfg(unix)]
fn sync_directory(path: &Path) -> Result<(), FsError> {
    File::open(path)
        .and_then(|directory| directory.sync_all())
        .map_err(|error| FsError::io("flush directory", path, error))
}

#[cfg(not(unix))]
fn sync_directory(path: &Path) -> Result<(), FsError> {
    fs::metadata(path)
        .map(|_| ())
        .map_err(|error| FsError::io("inspect directory before flush", path, error))
}

fn write_new_synced(path: &Path, bytes: &[u8]) -> Result<(), FsError> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|error| FsError::io("create file", path, error))?;
    file.write_all(bytes)
        .map_err(|error| FsError::io("write file", path, error))?;
    file.sync_all()
        .map_err(|error| FsError::io("flush file", path, error))
}

fn portable_key(path: &str) -> String {
    path.chars().flat_map(char::to_lowercase).collect()
}

fn validate_manifest(codec: &ProjectFormatCodec, bytes: &[u8]) -> Result<(), String> {
    codec
        .decode_manifest(bytes)
        .map(|_| ())
        .map_err(|error| error.to_string())
}

struct ValidatedPlanPaths {
    writes: Vec<CanonicalRelativePath>,
    deletions: Vec<CanonicalRelativePath>,
}

fn validate_plan_paths(plan: &AtomicWritePlan) -> Result<ValidatedPlanPaths, WriteError> {
    if plan.writes.is_empty() && plan.deletions.is_empty() {
        return Err(WriteError::InvalidTransition);
    }
    let mut seen = BTreeMap::new();
    let mut paths = Vec::with_capacity(plan.writes.len());
    for write in &plan.writes {
        let path = CanonicalRelativePath::parse(&write.path)
            .map_err(|error| WriteError::UnsafePath(error.to_string()))?;
        let key = portable_key(path.as_str());
        if let Some(first) = seen.insert(key, path.as_str().to_owned()) {
            return Err(WriteError::UnsafePath(format!(
                "portable path collision between {first:?} and {:?}",
                path.as_str()
            )));
        }
        paths.push(path);
    }
    let mut deletions = Vec::with_capacity(plan.deletions.len());
    for deletion in &plan.deletions {
        let path = CanonicalRelativePath::parse(deletion)
            .map_err(|error| WriteError::UnsafePath(error.to_string()))?;
        let key = portable_key(path.as_str());
        if let Some(first) = seen.insert(key, path.as_str().to_owned()) {
            return Err(WriteError::UnsafePath(format!(
                "portable path collision between {first:?} and {:?}",
                path.as_str()
            )));
        }
        deletions.push(path);
    }
    Ok(ValidatedPlanPaths {
        writes: paths,
        deletions,
    })
}

fn ensure_portable_component(directory: &Path, requested: &str) -> Result<(), FsError> {
    let requested_key = portable_key(requested);
    let entries = match fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(FsError::io("read path parent", directory, error)),
    };
    for entry in entries {
        let entry = entry.map_err(|error| FsError::io("read path entry", directory, error))?;
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            return Err(FsError::UnsafePath {
                path: entry.path().display().to_string(),
            });
        };
        if portable_key(name) == requested_key && name != requested {
            return Err(FsError::UnsafePath {
                path: format!("{} collides with {requested:?}", entry.path().display()),
            });
        }
    }
    Ok(())
}

/// A target whose root, parent, and current file identity have been checked.
#[derive(Debug, Clone)]
pub struct CheckedTarget {
    root: ProjectRootCapability,
    relative: CanonicalRelativePath,
    path: PathBuf,
    parent_identity: FileIdentity,
    identity: FileIdentity,
}

fn checked_target(
    root: &ProjectRootCapability,
    relative: &CanonicalRelativePath,
    create_parents: bool,
) -> Result<CheckedTarget, FsError> {
    verify_lock_owner(root)?;
    let segments: Vec<_> = relative.as_str().split('/').collect();
    let mut directory = root.path.clone();
    for segment in &segments[..segments.len() - 1] {
        ensure_portable_component(&directory, segment)?;
        let next = directory.join(segment);
        match fs::symlink_metadata(&next) {
            Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => {}
            Ok(_) => {
                return Err(FsError::UnsafePath {
                    path: next.display().to_string(),
                });
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound && create_parents => {
                fs::create_dir(&next)
                    .map_err(|error| FsError::io("create canonical directory", &next, error))?;
                sync_directory(&directory)?;
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                return Err(FsError::Missing { path: next });
            }
            Err(error) => return Err(FsError::io("inspect canonical directory", next, error)),
        }
        directory = next;
    }
    let filename = segments.last().expect("canonical paths have one segment");
    ensure_portable_component(&directory, filename)?;
    let parent_identity = file_identity(&directory)?;
    let path = directory.join(filename);
    let identity = file_identity(&path)?;
    if identity.kind == 3 {
        return Err(FsError::UnsafePath {
            path: path.display().to_string(),
        });
    }
    Ok(CheckedTarget {
        root: root.clone(),
        relative: relative.clone(),
        path,
        parent_identity,
        identity,
    })
}

fn recheck_parent(target: &CheckedTarget) -> Result<(), FsError> {
    verify_lock_owner(&target.root)?;
    let parent = target.path.parent().ok_or_else(|| FsError::UnsafePath {
        path: target.path.display().to_string(),
    })?;
    if !same_file_node(file_identity(parent)?, target.parent_identity) {
        return Err(FsError::UnsafePath {
            path: target.relative.to_string(),
        });
    }
    let canonical_parent = fs::canonicalize(parent)
        .map_err(|error| FsError::io("canonicalize target parent", parent, error))?;
    if !canonical_parent.starts_with(&target.root.path) {
        return Err(FsError::UnsafePath {
            path: target.relative.to_string(),
        });
    }
    Ok(())
}

fn recheck_target(target: &CheckedTarget) -> Result<(), FsError> {
    recheck_parent(target)?;
    if file_identity(&target.path)? != target.identity {
        return Err(FsError::UnsafePath {
            path: target.relative.to_string(),
        });
    }
    Ok(())
}

/// The data needed to create and flush one temporary file beside its target.
#[derive(Debug, Clone)]
pub struct TemporaryWrite {
    path: CanonicalRelativePath,
    bytes: Vec<u8>,
    generation: u64,
    ordinal: usize,
}

/// A flushed temporary file and the target identity captured before it was written.
#[derive(Debug, Clone)]
pub struct TemporaryFile {
    root: ProjectRootCapability,
    path: PathBuf,
    target: CheckedTarget,
}

/// The replace operations that can be wrapped for deterministic fault injection.
pub trait AtomicFileOps: Send + Sync {
    fn write_temporary(&self, write: TemporaryWrite) -> Result<TemporaryFile, FsError>;
    fn flush_file(&self, file: &TemporaryFile) -> Result<(), FsError>;
    fn replace(&self, file: TemporaryFile, target: &CheckedTarget) -> Result<(), FsError>;
    fn remove(&self, target: &CheckedTarget) -> Result<(), FsError> {
        recheck_target(target)?;
        if target.identity == FileIdentity::MISSING {
            return Ok(());
        }
        fs::remove_file(&target.path)
            .map_err(|error| FsError::io("remove canonical file", &target.path, error))
    }
    fn flush_parent(&self, target: &CheckedTarget) -> Result<(), FsError>;

    fn root(&self) -> Option<&ProjectRootCapability> {
        None
    }
}

/// Native temporary-write and atomic-replace operations.
#[derive(Debug)]
pub struct NativeAtomicFileOps {
    root: ProjectRootCapability,
}

impl NativeAtomicFileOps {
    pub fn new(root: ProjectRootCapability) -> Self {
        Self { root }
    }
}

impl AtomicFileOps for NativeAtomicFileOps {
    fn write_temporary(&self, write: TemporaryWrite) -> Result<TemporaryFile, FsError> {
        let target = checked_target(&self.root, &write.path, true)?;
        let parent = target.path.parent().expect("checked target has a parent");
        let temp_path = parent.join(format!(
            ".parchmint-{:016x}-{}-{}.tmp",
            self.root.root_id, write.generation, write.ordinal
        ));
        write_new_synced(&temp_path, &write.bytes)?;
        Ok(TemporaryFile {
            root: self.root.clone(),
            path: temp_path,
            target,
        })
    }

    fn flush_file(&self, file: &TemporaryFile) -> Result<(), FsError> {
        verify_lock_owner(&file.root)?;
        // `write_temporary` uses `write_new_synced`, which flushes the file before
        // returning it. Reopening the temporary file just to flush it again fails
        // on Windows while the filesystem is still resolving the newly-created
        // path.
        Ok(())
    }

    fn replace(&self, file: TemporaryFile, target: &CheckedTarget) -> Result<(), FsError> {
        if file.root.root_id != target.root.root_id || file.target.relative != target.relative {
            return Err(FsError::UnsafePath {
                path: target.relative.to_string(),
            });
        }
        recheck_target(target)?;
        let temporary_metadata = fs::symlink_metadata(&file.path)
            .map_err(|error| FsError::io("inspect temporary file", &file.path, error))?;
        if !temporary_metadata.is_file() || temporary_metadata.file_type().is_symlink() {
            return Err(FsError::UnsafePath {
                path: file.path.display().to_string(),
            });
        }
        let temporary_parent = file.path.parent().ok_or_else(|| FsError::UnsafePath {
            path: file.path.display().to_string(),
        })?;
        if !same_file_node(file_identity(temporary_parent)?, target.parent_identity) {
            return Err(FsError::UnsafePath {
                path: file.path.display().to_string(),
            });
        }
        replace_path(&file.path, &target.path)
    }

    fn remove(&self, target: &CheckedTarget) -> Result<(), FsError> {
        recheck_target(target)?;
        if target.identity == FileIdentity::MISSING {
            return Ok(());
        }
        fs::remove_file(&target.path)
            .map_err(|error| FsError::io("remove canonical file", &target.path, error))
    }

    fn flush_parent(&self, target: &CheckedTarget) -> Result<(), FsError> {
        recheck_parent(target)?;
        sync_directory(target.path.parent().expect("checked target has a parent"))
    }

    fn root(&self) -> Option<&ProjectRootCapability> {
        Some(&self.root)
    }
}

fn replace_path(source: &Path, target: &Path) -> Result<(), FsError> {
    match fs::rename(source, target) {
        Ok(()) => Ok(()),
        #[cfg(windows)]
        Err(error)
            if matches!(
                error.kind(),
                io::ErrorKind::AlreadyExists | io::ErrorKind::PermissionDenied
            ) =>
        {
            let displaced =
                target.with_extension(format!("parchmint-old-{:016x}", next_unique_id()));
            fs::rename(target, &displaced)
                .map_err(|error| FsError::io("move old target", target, error))?;
            if let Err(error) = fs::rename(source, target) {
                let _ = fs::rename(&displaced, target);
                return Err(FsError::io("replace target", target, error));
            }
            fs::remove_file(&displaced)
                .map_err(|error| FsError::io("remove displaced target", displaced, error))
        }
        Err(error) => Err(FsError::io("replace target", target, error)),
    }
}

#[derive(Debug, Clone)]
struct TransactionItem {
    relative: CanonicalRelativePath,
    temp_relative: CanonicalRelativePath,
    backup_relative: Option<CanonicalRelativePath>,
    new_hash: [u8; 32],
}

#[derive(Debug, Clone)]
struct TransactionDeletion {
    relative: CanonicalRelativePath,
    backup_relative: Option<CanonicalRelativePath>,
}

#[derive(Debug, Clone)]
struct DiskTransaction {
    root_id: u64,
    generation: u64,
    items: Vec<TransactionItem>,
    deletions: Vec<TransactionDeletion>,
}

#[derive(Debug, Clone)]
struct ActiveTransaction {
    plan: AtomicWritePlan,
    temporary: Vec<TemporaryFile>,
    disk: DiskTransaction,
    started: bool,
}

#[derive(Debug, Default)]
struct WriterState {
    staged: BTreeMap<u64, ActiveTransaction>,
    next_receipt: u64,
}

/// Recoverable multi-file writer built on a small replace-operation boundary.
#[derive(Debug)]
pub struct FsAtomicWriter<F: AtomicFileOps> {
    files: F,
    state: Mutex<WriterState>,
}

impl<F: AtomicFileOps> FsAtomicWriter<F> {
    pub fn new(files: F) -> Self {
        Self {
            files,
            state: Mutex::new(WriterState::default()),
        }
    }
}

impl<F: AtomicFileOps> AtomicWriter for FsAtomicWriter<F> {
    fn stage(&self, plan: AtomicWritePlan) -> Result<StagedWrite, WriteError> {
        let paths = validate_plan_paths(&plan)?;
        let generation = next_unique_id();
        let mut temporary = Vec::with_capacity(plan.writes.len());
        for (ordinal, (write, path)) in plan.writes.iter().zip(paths.writes).enumerate() {
            match self.files.write_temporary(TemporaryWrite {
                path,
                bytes: write.bytes.clone(),
                generation,
                ordinal,
            }) {
                Ok(file) => {
                    if let Err(error) = self.files.flush_file(&file) {
                        cleanup_temporary(&temporary);
                        let _ = fs::remove_file(&file.path);
                        return Err(map_write_error(error));
                    }
                    temporary.push(file);
                }
                Err(error) => {
                    cleanup_temporary(&temporary);
                    return Err(map_write_error(error));
                }
            }
        }
        let root = self
            .files
            .root()
            .cloned()
            .or_else(|| temporary.first().map(|file| file.root.clone()))
            .ok_or(WriteError::ForeignRoot)?;
        if temporary
            .iter()
            .any(|file| file.root.root_id != root.root_id)
        {
            cleanup_temporary(&temporary);
            return Err(WriteError::ForeignRoot);
        }
        let disk = match persist_transaction(&root, generation, &plan, &temporary, &paths.deletions)
        {
            Ok(record) => record,
            Err(error) => {
                cleanup_temporary(&temporary);
                return Err(map_write_error(error));
            }
        };
        let staged = StagedWrite::new(root.contract_root(), generation, plan.clone());
        self.state.lock().expect("writer state lock").staged.insert(
            generation,
            ActiveTransaction {
                plan,
                temporary,
                disk,
                started: false,
            },
        );
        Ok(staged)
    }

    fn validate_staged(&self, staged: &StagedWrite) -> ValidationReport {
        let state = self.state.lock().expect("writer state lock");
        let valid = state
            .staged
            .get(&staged.generation())
            .is_some_and(|transaction| {
                staged.root().id() == transaction.disk.root_id && staged.plan() == &transaction.plan
            });
        ValidationReport::new(valid)
    }

    fn commit(&self, staged: StagedWrite) -> Result<CommitReceipt, WriteError> {
        let root = self
            .files
            .root()
            .cloned()
            .or_else(|| {
                self.state.lock().ok().and_then(|state| {
                    state
                        .staged
                        .get(&staged.generation())
                        .and_then(|transaction| transaction.temporary.first())
                        .map(|file| file.root.clone())
                })
            })
            .ok_or(WriteError::ForeignRoot)?;
        if root.contract_root() != staged.root() {
            return Err(WriteError::ForeignRoot);
        }
        let transaction = {
            let mut state = self.state.lock().expect("writer state lock");
            let Some(transaction) = state.staged.get_mut(&staged.generation()) else {
                return Err(WriteError::InvalidTransition);
            };
            if transaction.disk.root_id != staged.root().id() {
                return Err(WriteError::ForeignRoot);
            }
            if transaction.plan != *staged.plan() {
                return Err(WriteError::InvalidTransition);
            }
            transaction.started = true;
            transaction.clone()
        };

        for temporary in &transaction.temporary {
            self.files
                .replace(temporary.clone(), &temporary.target)
                .map_err(map_write_error)?;
            self.files
                .flush_parent(&temporary.target)
                .map_err(map_write_error)?;
        }
        for deletion in &transaction.disk.deletions {
            let target =
                checked_target(&root, &deletion.relative, false).map_err(map_write_error)?;
            self.files.remove(&target).map_err(map_write_error)?;
            self.files.flush_parent(&target).map_err(map_write_error)?;
        }
        cleanup_transaction(&root, &transaction.disk).map_err(map_write_error)?;
        let mut state = self.state.lock().expect("writer state lock");
        state.staged.remove(&staged.generation());
        state.next_receipt += 1;
        Ok(CommitReceipt::new(state.next_receipt))
    }

    fn reconcile(&self, record: SaveTransactionRecord) -> Result<Reconciliation, WriteError> {
        let root = self.files.root().ok_or(WriteError::ForeignRoot)?;
        if record.root != root.contract_root() {
            return Err(WriteError::ForeignRoot);
        }
        let disk = read_transaction(root, record.generation).map_err(map_write_error)?;
        if disk.root_id != root.root_id {
            return Err(WriteError::ForeignRoot);
        }
        reconcile_transaction(&self.files, root, &disk)?;
        Ok(Reconciliation::new(true))
    }

    fn abandon(&self, staged: StagedWrite) -> Result<Abandonment, WriteError> {
        let transaction = {
            let mut state = self.state.lock().expect("writer state lock");
            let Some(transaction) = state.staged.get(&staged.generation()) else {
                return Err(WriteError::InvalidTransition);
            };
            if transaction.disk.root_id != staged.root().id() {
                return Err(WriteError::ForeignRoot);
            }
            if transaction.started || transaction.plan != *staged.plan() {
                return Err(WriteError::InvalidTransition);
            }
            state
                .staged
                .remove(&staged.generation())
                .expect("staged transaction still exists")
        };
        let root = self
            .files
            .root()
            .cloned()
            .or_else(|| transaction.temporary.first().map(|file| file.root.clone()))
            .ok_or(WriteError::ForeignRoot)?;
        cleanup_transaction(&root, &transaction.disk).map_err(map_write_error)?;
        Ok(Abandonment::new(true))
    }
}

fn map_write_error(error: FsError) -> WriteError {
    match error {
        FsError::UnsafePath { path } => WriteError::UnsafePath(path),
        FsError::NotLockOwner { .. } => WriteError::ForeignRoot,
        FsError::Missing { .. } | FsError::Corrupt { .. } => WriteError::Stale,
        FsError::Injected { .. }
        | FsError::AlreadyExists { .. }
        | FsError::Locked { .. }
        | FsError::Io { .. } => WriteError::Interrupted,
    }
}

fn cleanup_temporary(files: &[TemporaryFile]) {
    for file in files {
        let _ = fs::remove_file(&file.path);
    }
}

fn transaction_directory(root: &ProjectRootCapability, generation: u64) -> PathBuf {
    root.path
        .join(TRANSACTIONS_PATH)
        .join(generation.to_string())
}

fn persist_transaction(
    root: &ProjectRootCapability,
    generation: u64,
    plan: &AtomicWritePlan,
    temporary: &[TemporaryFile],
    deletion_paths: &[CanonicalRelativePath],
) -> Result<DiskTransaction, FsError> {
    verify_lock_owner(root)?;
    let transactions = root.path.join(TRANSACTIONS_PATH);
    fs::create_dir_all(&transactions)
        .map_err(|error| FsError::io("create transaction directory", &transactions, error))?;
    let directory = transaction_directory(root, generation);
    fs::create_dir(&directory)
        .map_err(|error| FsError::io("create transaction", &directory, error))?;

    let result = (|| {
        let mut items = Vec::with_capacity(temporary.len());
        for (ordinal, (write, file)) in plan.writes.iter().zip(temporary).enumerate() {
            recheck_target(&file.target)?;
            let backup_relative = if file.target.identity == FileIdentity::MISSING {
                None
            } else {
                let bytes = fs::read(&file.target.path).map_err(|error| {
                    FsError::io("read transaction backup", &file.target.path, error)
                })?;
                let backup_path = directory.join(format!("backup-{ordinal}"));
                write_new_synced(&backup_path, &bytes)?;
                Some(relative_from_root(root, &backup_path)?)
            };
            items.push(TransactionItem {
                relative: file.target.relative.clone(),
                temp_relative: relative_from_root(root, &file.path)?,
                backup_relative,
                new_hash: Sha256::digest(&write.bytes).into(),
            });
        }
        let mut deletions = Vec::with_capacity(deletion_paths.len());
        for (offset, relative) in deletion_paths.iter().enumerate() {
            let target = checked_target(root, relative, false)?;
            let backup_relative = if target.identity == FileIdentity::MISSING {
                None
            } else {
                let bytes = fs::read(&target.path)
                    .map_err(|error| FsError::io("read deletion backup", &target.path, error))?;
                let backup_path = directory.join(format!("backup-{}", temporary.len() + offset));
                write_new_synced(&backup_path, &bytes)?;
                Some(relative_from_root(root, &backup_path)?)
            };
            deletions.push(TransactionDeletion {
                relative: relative.clone(),
                backup_relative,
            });
        }
        let record = DiskTransaction {
            root_id: root.root_id,
            generation,
            items,
            deletions,
        };
        let record_path = directory.join(RECORD_NAME);
        write_new_synced(&record_path, &encode_transaction(&record)?)?;
        sync_directory(&directory)?;
        sync_directory(&transactions)?;
        Ok(record)
    })();
    if result.is_err() {
        let _ = fs::remove_dir_all(&directory);
    }
    result
}

fn relative_from_root(
    root: &ProjectRootCapability,
    path: &Path,
) -> Result<CanonicalRelativePath, FsError> {
    let relative = path
        .strip_prefix(&root.path)
        .map_err(|_| FsError::UnsafePath {
            path: path.display().to_string(),
        })?;
    let text = relative
        .components()
        .map(|component| component.as_os_str().to_str())
        .collect::<Option<Vec<_>>>()
        .ok_or_else(|| FsError::UnsafePath {
            path: relative.display().to_string(),
        })?
        .join("/");
    CanonicalRelativePath::parse(&text).map_err(|error| FsError::UnsafePath {
        path: error.to_string(),
    })
}

fn encode_transaction(record: &DiskTransaction) -> Result<Vec<u8>, FsError> {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(RECORD_MAGIC_V2);
    bytes.extend_from_slice(&record.root_id.to_le_bytes());
    bytes.extend_from_slice(&record.generation.to_le_bytes());
    let count = u32::try_from(record.items.len()).map_err(|error| FsError::Corrupt {
        path: PathBuf::from(RECORD_NAME),
        reason: error.to_string(),
    })?;
    bytes.extend_from_slice(&count.to_le_bytes());
    for item in &record.items {
        encode_string(&mut bytes, item.relative.as_str())?;
        encode_string(&mut bytes, item.temp_relative.as_str())?;
        match &item.backup_relative {
            Some(path) => {
                bytes.push(1);
                encode_string(&mut bytes, path.as_str())?;
            }
            None => bytes.push(0),
        }
        bytes.extend_from_slice(&item.new_hash);
    }
    let deletion_count =
        u32::try_from(record.deletions.len()).map_err(|error| FsError::Corrupt {
            path: PathBuf::from(RECORD_NAME),
            reason: error.to_string(),
        })?;
    bytes.extend_from_slice(&deletion_count.to_le_bytes());
    for deletion in &record.deletions {
        encode_string(&mut bytes, deletion.relative.as_str())?;
        match &deletion.backup_relative {
            Some(path) => {
                bytes.push(1);
                encode_string(&mut bytes, path.as_str())?;
            }
            None => bytes.push(0),
        }
    }
    Ok(bytes)
}

fn encode_string(output: &mut Vec<u8>, value: &str) -> Result<(), FsError> {
    let length = u32::try_from(value.len()).map_err(|error| FsError::Corrupt {
        path: PathBuf::from(RECORD_NAME),
        reason: error.to_string(),
    })?;
    output.extend_from_slice(&length.to_le_bytes());
    output.extend_from_slice(value.as_bytes());
    Ok(())
}

fn read_transaction(
    root: &ProjectRootCapability,
    generation: u64,
) -> Result<DiskTransaction, FsError> {
    read_transaction_file(
        root,
        &transaction_directory(root, generation).join(RECORD_NAME),
    )
}

fn read_transaction_file(
    root: &ProjectRootCapability,
    path: &Path,
) -> Result<DiskTransaction, FsError> {
    verify_lock_owner(root)?;
    let bytes = fs::read(path).map_err(|error| {
        if error.kind() == io::ErrorKind::NotFound {
            FsError::Missing {
                path: path.to_path_buf(),
            }
        } else {
            FsError::io("read transaction record", path, error)
        }
    })?;
    decode_transaction(path, &bytes)
}

fn decode_transaction(path: &Path, bytes: &[u8]) -> Result<DiskTransaction, FsError> {
    let mut cursor = io::Cursor::new(bytes);
    let mut magic = [0; 8];
    cursor
        .read_exact(&mut magic)
        .map_err(|error| corrupt_record(path, error))?;
    if &magic != RECORD_MAGIC_V1 && &magic != RECORD_MAGIC_V2 {
        return Err(FsError::Corrupt {
            path: path.to_path_buf(),
            reason: "unknown transaction record version".into(),
        });
    }
    let root_id = read_u64(&mut cursor, path)?;
    let generation = read_u64(&mut cursor, path)?;
    let count = read_u32(&mut cursor, path)?;
    let mut items = Vec::with_capacity(count as usize);
    for _ in 0..count {
        let relative = decode_relative(&mut cursor, path)?;
        let temp_relative = decode_relative(&mut cursor, path)?;
        let mut present = [0];
        cursor
            .read_exact(&mut present)
            .map_err(|error| corrupt_record(path, error))?;
        let backup_relative = match present[0] {
            0 => None,
            1 => Some(decode_relative(&mut cursor, path)?),
            _ => {
                return Err(FsError::Corrupt {
                    path: path.to_path_buf(),
                    reason: "invalid transaction backup marker".into(),
                });
            }
        };
        let mut new_hash = [0; 32];
        cursor
            .read_exact(&mut new_hash)
            .map_err(|error| corrupt_record(path, error))?;
        items.push(TransactionItem {
            relative,
            temp_relative,
            backup_relative,
            new_hash,
        });
    }
    let mut deletions = Vec::new();
    if &magic == RECORD_MAGIC_V2 {
        let count = read_u32(&mut cursor, path)?;
        deletions.reserve(count as usize);
        for _ in 0..count {
            let relative = decode_relative(&mut cursor, path)?;
            let mut present = [0];
            cursor
                .read_exact(&mut present)
                .map_err(|error| corrupt_record(path, error))?;
            let backup_relative = match present[0] {
                0 => None,
                1 => Some(decode_relative(&mut cursor, path)?),
                _ => {
                    return Err(FsError::Corrupt {
                        path: path.to_path_buf(),
                        reason: "invalid deletion backup marker".into(),
                    });
                }
            };
            deletions.push(TransactionDeletion {
                relative,
                backup_relative,
            });
        }
    }
    if cursor.position() != bytes.len() as u64 {
        return Err(FsError::Corrupt {
            path: path.to_path_buf(),
            reason: "trailing transaction record data".into(),
        });
    }
    let record = DiskTransaction {
        root_id,
        generation,
        items,
        deletions,
    };
    validate_transaction_paths(path, &record)?;
    Ok(record)
}

fn validate_transaction_paths(path: &Path, record: &DiskTransaction) -> Result<(), FsError> {
    let backup_prefix = format!("{TRANSACTIONS_PATH}/{}/backup-", record.generation);
    let temp_prefix = format!(".parchmint-{:016x}-{}-", record.root_id, record.generation);
    for item in &record.items {
        let target_parent = item
            .relative
            .as_str()
            .rsplit_once('/')
            .map(|(parent, _)| parent);
        let (temp_parent, temp_name) = item
            .temp_relative
            .as_str()
            .rsplit_once('/')
            .unwrap_or(("", item.temp_relative.as_str()));
        if target_parent.unwrap_or("") != temp_parent || !temp_name.starts_with(&temp_prefix) {
            return Err(FsError::Corrupt {
                path: path.to_path_buf(),
                reason: "transaction temporary does not belong to its target".into(),
            });
        }
        if item
            .backup_relative
            .as_ref()
            .is_some_and(|backup| !backup.as_str().starts_with(&backup_prefix))
        {
            return Err(FsError::Corrupt {
                path: path.to_path_buf(),
                reason: "transaction backup is outside its transaction".into(),
            });
        }
    }
    for deletion in &record.deletions {
        if deletion
            .backup_relative
            .as_ref()
            .is_some_and(|backup| !backup.as_str().starts_with(&backup_prefix))
        {
            return Err(FsError::Corrupt {
                path: path.to_path_buf(),
                reason: "transaction deletion backup is outside its transaction".into(),
            });
        }
    }
    Ok(())
}

fn read_u64(cursor: &mut io::Cursor<&[u8]>, path: &Path) -> Result<u64, FsError> {
    let mut bytes = [0; 8];
    cursor
        .read_exact(&mut bytes)
        .map_err(|error| corrupt_record(path, error))?;
    Ok(u64::from_le_bytes(bytes))
}

fn read_u32(cursor: &mut io::Cursor<&[u8]>, path: &Path) -> Result<u32, FsError> {
    let mut bytes = [0; 4];
    cursor
        .read_exact(&mut bytes)
        .map_err(|error| corrupt_record(path, error))?;
    Ok(u32::from_le_bytes(bytes))
}

fn decode_relative(
    cursor: &mut io::Cursor<&[u8]>,
    record_path: &Path,
) -> Result<CanonicalRelativePath, FsError> {
    let length = read_u32(cursor, record_path)? as usize;
    let mut bytes = vec![0; length];
    cursor
        .read_exact(&mut bytes)
        .map_err(|error| corrupt_record(record_path, error))?;
    let text = String::from_utf8(bytes).map_err(|error| FsError::Corrupt {
        path: record_path.to_path_buf(),
        reason: error.to_string(),
    })?;
    CanonicalRelativePath::parse(text).map_err(|error| FsError::Corrupt {
        path: record_path.to_path_buf(),
        reason: error.to_string(),
    })
}

fn corrupt_record(path: &Path, error: io::Error) -> FsError {
    FsError::Corrupt {
        path: path.to_path_buf(),
        reason: error.to_string(),
    }
}

fn target_hash(
    root: &ProjectRootCapability,
    item: &TransactionItem,
) -> Result<Option<[u8; 32]>, FsError> {
    let target = checked_target(root, &item.relative, false);
    match target {
        Ok(target) if target.identity != FileIdentity::MISSING => fs::read(&target.path)
            .map(|bytes| Some(Sha256::digest(bytes).into()))
            .map_err(|error| FsError::io("read transaction target", target.path, error)),
        Ok(_) | Err(FsError::Missing { .. }) => Ok(None),
        Err(error) => Err(error),
    }
}

fn reconcile_transaction<F: AtomicFileOps>(
    files: &F,
    root: &ProjectRootCapability,
    record: &DiskTransaction,
) -> Result<(), WriteError> {
    let writes_can_finish = record.items.iter().try_fold(true, |possible, item| {
        let target_is_new =
            target_hash(root, item).map_err(map_write_error)? == Some(item.new_hash);
        let temp_exists = checked_target(root, &item.temp_relative, false)
            .map(|target| target.identity != FileIdentity::MISSING)
            .or_else(|error| match error {
                FsError::Missing { .. } => Ok(false),
                other => Err(other),
            })
            .map_err(map_write_error)?;
        Ok::<_, WriteError>(possible && (target_is_new || temp_exists))
    })?;
    let deletions_can_finish = record
        .deletions
        .iter()
        .try_fold(true, |possible, deletion| {
            let target_exists = checked_target(root, &deletion.relative, false)
                .map(|target| target.identity != FileIdentity::MISSING)
                .or_else(|error| match error {
                    FsError::Missing { .. } => Ok(false),
                    other => Err(other),
                })
                .map_err(map_write_error)?;
            let backup_exists = deletion
                .backup_relative
                .as_ref()
                .map(|backup| checked_target(root, backup, false))
                .transpose()
                .map_err(map_write_error)?
                .is_some_and(|backup| backup.identity != FileIdentity::MISSING);
            Ok::<_, WriteError>(
                possible
                    && if deletion.backup_relative.is_some() {
                        backup_exists
                    } else {
                        !target_exists
                    },
            )
        })?;
    let can_finish_new = writes_can_finish && deletions_can_finish;
    if can_finish_new {
        for item in &record.items {
            if target_hash(root, item).map_err(map_write_error)? == Some(item.new_hash) {
                continue;
            }
            let target = checked_target(root, &item.relative, true).map_err(map_write_error)?;
            let temporary = TemporaryFile {
                root: root.clone(),
                path: root.path.join(item.temp_relative.as_str()),
                target: target.clone(),
            };
            files.replace(temporary, &target).map_err(map_write_error)?;
            files.flush_parent(&target).map_err(map_write_error)?;
        }
        for deletion in &record.deletions {
            let target =
                checked_target(root, &deletion.relative, false).map_err(map_write_error)?;
            files.remove(&target).map_err(map_write_error)?;
            files.flush_parent(&target).map_err(map_write_error)?;
        }
    } else {
        restore_old_generation(files, root, record)?;
    }
    if can_finish_new {
        for item in &record.items {
            if target_hash(root, item).map_err(map_write_error)? != Some(item.new_hash) {
                return Err(WriteError::Interrupted);
            }
        }
        for deletion in &record.deletions {
            let target =
                checked_target(root, &deletion.relative, false).map_err(map_write_error)?;
            if target.identity != FileIdentity::MISSING {
                return Err(WriteError::Interrupted);
            }
        }
    }
    cleanup_transaction(root, record).map_err(map_write_error)
}

fn restore_old_generation<F: AtomicFileOps>(
    files: &F,
    root: &ProjectRootCapability,
    record: &DiskTransaction,
) -> Result<(), WriteError> {
    for (ordinal, item) in record.items.iter().enumerate() {
        if let Some(backup) = &item.backup_relative {
            let backup = checked_target(root, backup, false).map_err(map_write_error)?;
            if backup.identity == FileIdentity::MISSING {
                return Err(WriteError::Stale);
            }
            let bytes = fs::read(&backup.path).map_err(|error| {
                map_write_error(FsError::io("read recovery backup", &backup.path, error))
            })?;
            let target = checked_target(root, &item.relative, true).map_err(map_write_error)?;
            let temporary = files
                .write_temporary(TemporaryWrite {
                    path: item.relative.clone(),
                    bytes,
                    generation: record.generation,
                    ordinal: record.items.len() + ordinal,
                })
                .map_err(map_write_error)?;
            files.flush_file(&temporary).map_err(map_write_error)?;
            files.replace(temporary, &target).map_err(map_write_error)?;
            files.flush_parent(&target).map_err(map_write_error)?;
        } else {
            let target = checked_target(root, &item.relative, false);
            match target {
                Ok(target) if target.identity != FileIdentity::MISSING => {
                    recheck_target(&target).map_err(map_write_error)?;
                    fs::remove_file(&target.path).map_err(|error| {
                        map_write_error(FsError::io(
                            "remove rolled-back target",
                            &target.path,
                            error,
                        ))
                    })?;
                    sync_directory(target.path.parent().expect("checked target has parent"))
                        .map_err(map_write_error)?;
                }
                Ok(_) | Err(FsError::Missing { .. }) => {}
                Err(error) => return Err(map_write_error(error)),
            }
        }
    }
    for (offset, deletion) in record.deletions.iter().enumerate() {
        let Some(backup) = &deletion.backup_relative else {
            continue;
        };
        let backup = checked_target(root, backup, false).map_err(map_write_error)?;
        if backup.identity == FileIdentity::MISSING {
            return Err(WriteError::Stale);
        }
        let bytes = fs::read(&backup.path).map_err(|error| {
            map_write_error(FsError::io(
                "read deletion recovery backup",
                &backup.path,
                error,
            ))
        })?;
        let temporary = files
            .write_temporary(TemporaryWrite {
                path: deletion.relative.clone(),
                bytes,
                generation: record.generation,
                ordinal: record.items.len() + offset,
            })
            .map_err(map_write_error)?;
        files.flush_file(&temporary).map_err(map_write_error)?;
        let target = checked_target(root, &deletion.relative, true).map_err(map_write_error)?;
        files.replace(temporary, &target).map_err(map_write_error)?;
        files.flush_parent(&target).map_err(map_write_error)?;
    }
    Ok(())
}

fn cleanup_transaction(
    root: &ProjectRootCapability,
    record: &DiskTransaction,
) -> Result<(), FsError> {
    verify_lock_owner(root)?;
    for item in &record.items {
        let temp = root.path.join(item.temp_relative.as_str());
        match fs::remove_file(&temp) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(FsError::io("remove transaction temporary", temp, error)),
        }
    }
    let directory = transaction_directory(root, record.generation);
    fs::remove_dir_all(&directory)
        .map_err(|error| FsError::io("remove completed transaction", &directory, error))?;
    sync_directory(&root.path.join(TRANSACTIONS_PATH))
}

#[derive(Debug, Clone)]
struct ActiveProject {
    root: ProjectRootCapability,
    documents: BTreeMap<DocumentId, CanonicalRelativePath>,
}

/// Directory-backed implementation of the project repository contract.
#[derive(Debug)]
pub struct FsProjectRepository<F: ProjectFileSystem = NativeProjectFileSystem> {
    files: F,
    active: Mutex<Option<ActiveProject>>,
}

impl Default for FsProjectRepository<NativeProjectFileSystem> {
    fn default() -> Self {
        Self {
            files: NativeProjectFileSystem,
            active: Mutex::new(None),
        }
    }
}

impl FsProjectRepository<NativeProjectFileSystem> {
    pub fn native() -> Self {
        Self::default()
    }
}

impl<F: ProjectFileSystem> FsProjectRepository<F> {
    pub fn new(files: F) -> Self {
        Self {
            files,
            active: Mutex::new(None),
        }
    }

    /// Returns the native root capability retained by the currently opened
    /// repository session.
    ///
    /// The returned capability remains authorized only while the matching
    /// [`OpenProject`] lease is alive. Production composition uses this seam
    /// to construct History, recovery, search, and save services without
    /// acquiring a second project lock.
    pub fn active_root(&self) -> Option<ProjectRootCapability> {
        self.active
            .lock()
            .expect("active project lock")
            .as_ref()
            .map(|active| active.root.clone())
    }

    fn map_repository_error(path: &ProjectPath, error: FsError) -> RepositoryError {
        match error {
            FsError::Missing { .. } => RepositoryError::MissingResource { path: path.clone() },
            FsError::Locked { .. } => RepositoryError::Locked { path: path.clone() },
            FsError::UnsafePath { path } => RepositoryError::UnsafePath { path },
            FsError::Injected { .. }
            | FsError::AlreadyExists { .. }
            | FsError::Corrupt { .. }
            | FsError::NotLockOwner { .. }
            | FsError::Io { .. } => RepositoryError::Integrity {
                path: path.clone(),
                reason: error.to_string(),
            },
        }
    }

    fn open_validated(
        &self,
        path: ProjectPath,
        root: ProjectRootCapability,
        lease: ProjectLockLease,
    ) -> Result<OpenProject, RepositoryError> {
        let records = self
            .files
            .transaction_records(&root)
            .map_err(|error| Self::map_repository_error(&path, error))?;
        if !records.is_empty() {
            let writer = FsAtomicWriter::new(NativeAtomicFileOps::new(root.clone()));
            for record in records {
                writer
                    .reconcile(record)
                    .map_err(|_| RepositoryError::Interrupted { path: path.clone() })?;
            }
        }
        let control_path =
            CanonicalRelativePath::parse(CONTROL_PATH).expect("static path is canonical");
        let manifest_path =
            CanonicalRelativePath::parse(MANIFEST_PATH).expect("static path is canonical");
        let control = self
            .files
            .read(&root, &control_path)
            .map_err(|error| Self::map_repository_error(&path, error))?;
        let manifest_bytes = self
            .files
            .read(&root, &manifest_path)
            .map_err(|error| Self::map_repository_error(&path, error))?;
        let codec = ProjectFormatCodec::default();
        codec
            .detect(&control)
            .map_err(|error| RepositoryError::Integrity {
                path: path.clone(),
                reason: error.to_string(),
            })?;
        validate_manifest(&codec, &manifest_bytes).map_err(|reason| {
            RepositoryError::Integrity {
                path: path.clone(),
                reason,
            }
        })?;
        let manifest =
            String::from_utf8(manifest_bytes).map_err(|error| RepositoryError::Integrity {
                path: path.clone(),
                reason: error.to_string(),
            })?;
        let documents =
            scan_document_index(&root).map_err(|error| Self::map_repository_error(&path, error))?;
        let snapshot = ProjectSnapshot {
            path,
            manifest,
            document_ids: documents.keys().cloned().collect(),
        };
        *self.active.lock().expect("active project lock") = Some(ActiveProject { root, documents });
        Ok(OpenProject::with_lease(snapshot, lease))
    }
}

impl<F: ProjectFileSystem> ProjectRepository for FsProjectRepository<F> {
    fn create(&self, request: CreateProject) -> Result<OpenProject, RepositoryError> {
        reject_git_worktree(request.path.as_path())
            .map_err(|error| Self::map_repository_error(&request.path, error))?;
        let codec = ProjectFormatCodec::default();
        validate_manifest(&codec, request.manifest.as_bytes()).map_err(|reason| {
            RepositoryError::Integrity {
                path: request.path.clone(),
                reason,
            }
        })?;
        let mut writes = vec![
            (
                CanonicalRelativePath::parse(CONTROL_PATH).expect("static path"),
                b"1\n".to_vec(),
            ),
            (
                CanonicalRelativePath::parse(MANIFEST_PATH).expect("static path"),
                request.manifest.as_bytes().to_vec(),
            ),
        ];
        let mut seen = BTreeSet::new();
        for (id, bytes) in &request.documents {
            codec
                .decode_document(bytes)
                .map_err(|error| RepositoryError::Integrity {
                    path: request.path.clone(),
                    reason: error.to_string(),
                })?;
            let relative = CanonicalRelativePath::parse(format!("manuscript/{}.html", id.as_str()))
                .map_err(|error| RepositoryError::UnsafePath {
                    path: error.to_string(),
                })?;
            if !seen.insert(portable_key(relative.as_str())) {
                return Err(RepositoryError::UnsafePath {
                    path: relative.to_string(),
                });
            }
            writes.push((relative, bytes.clone()));
        }
        let (root, lease) = self
            .files
            .create_root(UntrustedProjectPath::new(request.path.as_path()))
            .map_err(|error| Self::map_repository_error(&request.path, error))?;
        let write_result = writes.iter().try_for_each(|(path, bytes)| {
            let target = checked_target(&root, path, true)?;
            if target.identity != FileIdentity::MISSING {
                return Err(FsError::AlreadyExists { path: target.path });
            }
            write_new_synced(&target.path, bytes)?;
            sync_directory(target.path.parent().expect("checked target has parent"))
        });
        if let Err(error) = write_result {
            let root_path = root.path.clone();
            drop(lease);
            let _ = fs::remove_dir_all(root_path);
            return Err(Self::map_repository_error(&request.path, error));
        }
        self.open_validated(request.path, root, lease)
    }

    fn open(&self, path: ProjectPath) -> Result<OpenProject, RepositoryError> {
        let (root, lease) = self
            .files
            .acquire(UntrustedProjectPath::new(path.as_path()))
            .map_err(|error| match error {
                FsError::Missing { .. } => RepositoryError::Missing { path: path.clone() },
                other => Self::map_repository_error(&path, other),
            })?;
        self.open_validated(path, root, lease)
    }

    fn load_document(&self, document: DocumentId) -> Result<Vec<u8>, RepositoryError> {
        let active = self
            .active
            .lock()
            .expect("active project lock")
            .clone()
            .ok_or_else(|| RepositoryError::NotFound {
                document: document.clone(),
            })?;
        let path = active
            .documents
            .get(&document)
            .ok_or_else(|| RepositoryError::NotFound {
                document: document.clone(),
            })?;
        let bytes =
            self.files
                .read(&active.root, path)
                .map_err(|error| RepositoryError::Integrity {
                    path: ProjectPath::new(active.root.path.clone()),
                    reason: error.to_string(),
                })?;
        ProjectFormatCodec::default()
            .decode_document(&bytes)
            .map_err(|error| RepositoryError::Integrity {
                path: ProjectPath::new(active.root.path),
                reason: error.to_string(),
            })?;
        Ok(bytes)
    }
}

fn scan_document_index(
    root: &ProjectRootCapability,
) -> Result<BTreeMap<DocumentId, CanonicalRelativePath>, FsError> {
    verify_lock_owner(root)?;
    let manuscript = root.path.join("manuscript");
    let metadata = match fs::symlink_metadata(&manuscript) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(BTreeMap::new()),
        Err(error) => {
            return Err(FsError::io(
                "inspect manuscript directory",
                manuscript,
                error,
            ));
        }
    };
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err(FsError::UnsafePath {
            path: manuscript.display().to_string(),
        });
    }
    let mut documents = BTreeMap::new();
    let mut portable_names = BTreeSet::new();
    for entry in fs::read_dir(&manuscript)
        .map_err(|error| FsError::io("read manuscript directory", &manuscript, error))?
    {
        let entry =
            entry.map_err(|error| FsError::io("read manuscript entry", &manuscript, error))?;
        let file_type = entry
            .file_type()
            .map_err(|error| FsError::io("inspect manuscript entry", entry.path(), error))?;
        if file_type.is_symlink() {
            return Err(FsError::UnsafePath {
                path: entry.path().display().to_string(),
            });
        }
        if !file_type.is_file()
            || entry.path().extension().and_then(|value| value.to_str()) != Some("html")
        {
            continue;
        }
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| FsError::UnsafePath {
                path: entry.path().display().to_string(),
            })?;
        let relative =
            CanonicalRelativePath::parse(format!("manuscript/{name}")).map_err(|error| {
                FsError::UnsafePath {
                    path: error.to_string(),
                }
            })?;
        if !portable_names.insert(portable_key(relative.as_str())) {
            return Err(FsError::UnsafePath {
                path: relative.to_string(),
            });
        }
        let id = name.strip_suffix(".html").expect("extension was checked");
        documents.insert(DocumentId::new(id), relative);
    }
    Ok(documents)
}
