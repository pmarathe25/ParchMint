//! Small, deterministic repository boundary used by the project layer.
//!
//! The implementation is deliberately in-memory.  It models the important
//! repository invariants (leases, validation, and atomic state transitions)
//! without making filesystem policy part of this crate.

use std::{
    collections::BTreeMap,
    error::Error,
    fmt,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ProjectPath(PathBuf);

impl ProjectPath {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self(path.into())
    }
    pub fn as_path(&self) -> &Path {
        &self.0
    }
}

impl From<PathBuf> for ProjectPath {
    fn from(path: PathBuf) -> Self {
        Self(path)
    }
}
impl From<&Path> for ProjectPath {
    fn from(path: &Path) -> Self {
        Self::new(path)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DocumentId(String);

impl DocumentId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreateProject {
    pub path: ProjectPath,
    pub manifest: String,
    pub documents: BTreeMap<DocumentId, Vec<u8>>,
}

impl CreateProject {
    pub fn new(path: impl Into<ProjectPath>) -> Self {
        Self {
            path: path.into(),
            manifest: "[project]\n".into(),
            documents: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectSnapshot {
    pub path: ProjectPath,
    pub manifest: String,
    pub document_ids: Vec<DocumentId>,
}

pub struct OpenProject {
    pub snapshot: ProjectSnapshot,
    _lease: Box<dyn Send + Sync>,
}

impl OpenProject {
    /// Builds an opened project whose writable lease lives for as long as this value.
    pub fn with_lease(snapshot: ProjectSnapshot, lease: impl Send + Sync + 'static) -> Self {
        Self {
            snapshot,
            _lease: Box::new(lease),
        }
    }
}

impl fmt::Debug for OpenProject {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OpenProject")
            .field("snapshot", &self.snapshot)
            .finish_non_exhaustive()
    }
}

#[derive(Debug)]
struct Lease {
    state: Arc<Mutex<RepositoryState>>,
    path: ProjectPath,
}
impl Drop for Lease {
    fn drop(&mut self) {
        if let Ok(mut state) = self.state.lock()
            && state.leases.get(&self.path).copied() == Some(true)
        {
            state.leases.insert(self.path.clone(), false);
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RepositoryError {
    Missing { path: ProjectPath },
    Locked { path: ProjectPath },
    MissingResource { path: ProjectPath },
    UnsafePath { path: String },
    Interrupted { path: ProjectPath },
    Integrity { path: ProjectPath, reason: String },
    NotFound { document: DocumentId },
}
impl fmt::Display for RepositoryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
impl Error for RepositoryError {}

pub trait ProjectRepository: Send + Sync {
    fn create(&self, request: CreateProject) -> Result<OpenProject, RepositoryError>;
    fn open(&self, path: ProjectPath) -> Result<OpenProject, RepositoryError>;
    fn load_document(&self, document: DocumentId) -> Result<Vec<u8>, RepositoryError>;
}

#[derive(Debug, Default)]
struct RepositoryState {
    projects: BTreeMap<ProjectPath, StoredProject>,
    leases: BTreeMap<ProjectPath, bool>,
    active: Option<ProjectPath>,
    body_loads: usize,
}
#[derive(Debug, Clone)]
struct StoredProject {
    manifest: String,
    documents: BTreeMap<DocumentId, Vec<u8>>,
    missing: bool,
    unsafe_path: bool,
    interrupted: bool,
}

#[derive(Debug, Clone, Default)]
pub struct InMemoryProjectRepository {
    state: Arc<Mutex<RepositoryState>>,
}

impl InMemoryProjectRepository {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn body_load_count(&self) -> usize {
        self.state.lock().expect("repository lock").body_loads
    }
    fn update_project(&self, path: &Path, update: impl FnOnce(&mut StoredProject)) {
        if let Some(project) = self
            .state
            .lock()
            .expect("repository lock")
            .projects
            .get_mut(&ProjectPath::from(path))
        {
            update(project);
        }
    }
    pub fn remove_required_resource(&self, path: &Path) {
        self.update_project(path, |project| project.missing = true);
    }
    pub fn mark_unsafe_manifest_path(&self, path: &Path) {
        self.update_project(path, |project| project.unsafe_path = true);
    }
    pub fn interrupt_save(&self, path: &Path) {
        self.update_project(path, |project| project.interrupted = true);
    }
    fn snapshot(path: &ProjectPath, project: &StoredProject) -> ProjectSnapshot {
        ProjectSnapshot {
            path: path.clone(),
            manifest: project.manifest.clone(),
            document_ids: project.documents.keys().cloned().collect(),
        }
    }
}

impl ProjectRepository for InMemoryProjectRepository {
    fn create(&self, request: CreateProject) -> Result<OpenProject, RepositoryError> {
        let mut state = self.state.lock().expect("repository lock");
        if state.projects.contains_key(&request.path) {
            return Err(RepositoryError::Locked { path: request.path });
        }
        let path = request.path.clone();
        let project = StoredProject {
            manifest: request.manifest,
            documents: request.documents,
            missing: false,
            unsafe_path: false,
            interrupted: false,
        };
        let snapshot = Self::snapshot(&path, &project);
        state.projects.insert(path.clone(), project);
        state.leases.insert(path.clone(), true);
        state.active = Some(path.clone());
        Ok(OpenProject::with_lease(
            snapshot,
            Lease {
                state: self.state.clone(),
                path,
            },
        ))
    }
    fn open(&self, path: ProjectPath) -> Result<OpenProject, RepositoryError> {
        let mut state = self.state.lock().expect("repository lock");
        let project = state
            .projects
            .get(&path)
            .ok_or_else(|| RepositoryError::Missing { path: path.clone() })?;
        if project.unsafe_path {
            return Err(RepositoryError::UnsafePath {
                path: path.0.display().to_string(),
            });
        }
        if project.missing {
            return Err(RepositoryError::MissingResource { path });
        }
        if project.interrupted {
            return Err(RepositoryError::Interrupted { path });
        }
        if state.leases.get(&path).copied().unwrap_or(false) {
            return Err(RepositoryError::Locked { path });
        }
        let snapshot = Self::snapshot(&path, project);
        state.leases.insert(path.clone(), true);
        state.active = Some(path.clone());
        Ok(OpenProject::with_lease(
            snapshot,
            Lease {
                state: self.state.clone(),
                path,
            },
        ))
    }
    fn load_document(&self, document: DocumentId) -> Result<Vec<u8>, RepositoryError> {
        let mut state = self.state.lock().expect("repository lock");
        let path = state
            .active
            .clone()
            .ok_or_else(|| RepositoryError::NotFound {
                document: document.clone(),
            })?;
        let bytes = state
            .projects
            .get(&path)
            .and_then(|p| p.documents.get(&document))
            .cloned()
            .ok_or(RepositoryError::NotFound { document })?;
        state.body_loads += 1;
        Ok(bytes)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectRootCapability(u64);
impl ProjectRootCapability {
    pub fn new(id: u64) -> Self {
        Self(id)
    }

    pub fn id(&self) -> u64 {
        self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AtomicWritePlan {
    pub writes: Vec<StagedResource>,
    /// Canonical project-relative files removed by the same transaction.
    pub deletions: Vec<String>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StagedResource {
    pub path: String,
    pub bytes: Vec<u8>,
}
impl AtomicWritePlan {
    pub fn new(writes: Vec<StagedResource>) -> Self {
        Self {
            writes,
            deletions: Vec::new(),
        }
    }

    pub fn with_deletions(writes: Vec<StagedResource>, deletions: Vec<String>) -> Self {
        Self { writes, deletions }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StagedWrite {
    root: u64,
    generation: u64,
    plan: AtomicWritePlan,
}
impl StagedWrite {
    pub fn new(root: ProjectRootCapability, generation: u64, plan: AtomicWritePlan) -> Self {
        Self {
            root: root.0,
            generation,
            plan,
        }
    }

    pub fn root(&self) -> ProjectRootCapability {
        ProjectRootCapability::new(self.root)
    }

    pub fn generation(&self) -> u64 {
        self.generation
    }

    pub fn plan(&self) -> &AtomicWritePlan {
        &self.plan
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SaveTransactionRecord {
    pub root: ProjectRootCapability,
    pub generation: u64,
}
impl SaveTransactionRecord {
    pub fn new(root: ProjectRootCapability, generation: u64) -> Self {
        Self { root, generation }
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitReceipt(u64);
impl CommitReceipt {
    pub fn new(id: u64) -> Self {
        Self(id)
    }

    pub const fn id(&self) -> u64 {
        self.0
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationReport {
    valid: bool,
}
impl ValidationReport {
    pub fn new(valid: bool) -> Self {
        Self { valid }
    }

    pub fn is_valid(&self) -> bool {
        self.valid
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reconciliation {
    reconciled: bool,
}
impl Reconciliation {
    pub fn new(reconciled: bool) -> Self {
        Self { reconciled }
    }

    pub fn is_reconciled(&self) -> bool {
        self.reconciled
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Abandonment {
    abandoned: bool,
}
impl Abandonment {
    pub fn new(abandoned: bool) -> Self {
        Self { abandoned }
    }

    pub fn was_abandoned(&self) -> bool {
        self.abandoned
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WriteError {
    InvalidTransition,
    Stale,
    ForeignRoot,
    UnsafePath(String),
    Interrupted,
}
impl fmt::Display for WriteError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
impl Error for WriteError {}

pub trait AtomicWriter: Send + Sync {
    fn stage(&self, plan: AtomicWritePlan) -> Result<StagedWrite, WriteError>;
    fn validate_staged(&self, staged: &StagedWrite) -> ValidationReport;
    fn commit(&self, staged: StagedWrite) -> Result<CommitReceipt, WriteError>;
    fn reconcile(&self, record: SaveTransactionRecord) -> Result<Reconciliation, WriteError>;
    fn abandon(&self, staged: StagedWrite) -> Result<Abandonment, WriteError>;
}

#[derive(Debug)]
pub struct InMemoryAtomicWriter {
    root: ProjectRootCapability,
    state: Mutex<WriterState>,
}
#[derive(Debug, Default)]
struct WriterState {
    generation: u64,
    next_receipt: u64,
    staged: BTreeMap<(u64, u64), AtomicWritePlan>,
}
impl InMemoryAtomicWriter {
    pub fn new(root: ProjectRootCapability) -> Self {
        Self {
            root,
            state: Mutex::new(WriterState::default()),
        }
    }
    fn valid_plan(plan: &AtomicWritePlan) -> bool {
        plan.writes
            .iter()
            .all(|write| !write.path.is_empty() && !write.path.contains(".."))
            && plan
                .deletions
                .iter()
                .all(|path| !path.is_empty() && !path.contains(".."))
    }
    fn is_staged(state: &WriterState, staged: &StagedWrite) -> bool {
        state.staged.get(&(staged.root, staged.generation)) == Some(&staged.plan)
    }
}
impl AtomicWriter for InMemoryAtomicWriter {
    fn stage(&self, plan: AtomicWritePlan) -> Result<StagedWrite, WriteError> {
        let mut s = self.state.lock().expect("writer lock");
        s.generation += 1;
        let key = (self.root.0, s.generation);
        s.staged.insert(key, plan.clone());
        Ok(StagedWrite {
            root: self.root.0,
            generation: key.1,
            plan,
        })
    }
    fn validate_staged(&self, staged: &StagedWrite) -> ValidationReport {
        let s = self.state.lock().expect("writer lock");
        ValidationReport {
            valid: staged.root == self.root.0
                && Self::is_staged(&s, staged)
                && Self::valid_plan(&staged.plan),
        }
    }
    fn commit(&self, staged: StagedWrite) -> Result<CommitReceipt, WriteError> {
        let mut s = self.state.lock().expect("writer lock");
        if staged.root != self.root.0 {
            return Err(WriteError::ForeignRoot);
        }
        if staged.generation == 0 || staged.generation > s.generation {
            return Err(WriteError::Stale);
        }
        if !Self::is_staged(&s, &staged) {
            return Err(WriteError::InvalidTransition);
        }
        if !Self::valid_plan(&staged.plan) {
            return Err(WriteError::UnsafePath("invalid staged path".into()));
        }
        s.staged.remove(&(staged.root, staged.generation));
        s.next_receipt += 1;
        Ok(CommitReceipt(s.next_receipt))
    }
    fn reconcile(&self, record: SaveTransactionRecord) -> Result<Reconciliation, WriteError> {
        if record.root != self.root {
            return Err(WriteError::ForeignRoot);
        }
        let mut s = self.state.lock().expect("writer lock");
        s.generation = s.generation.max(record.generation);
        Ok(Reconciliation { reconciled: true })
    }
    fn abandon(&self, staged: StagedWrite) -> Result<Abandonment, WriteError> {
        let mut s = self.state.lock().expect("writer lock");
        if staged.root != self.root.0 {
            return Err(WriteError::ForeignRoot);
        }
        if s.staged.remove(&(staged.root, staged.generation)).is_some() {
            Ok(Abandonment { abandoned: true })
        } else {
            Err(WriteError::InvalidTransition)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(path: &str) -> CreateProject {
        let mut r = CreateProject::new(ProjectPath::new(path));
        r.documents.insert(DocumentId::new("doc"), b"body".to_vec());
        r
    }

    fn plan() -> AtomicWritePlan {
        AtomicWritePlan::new(vec![StagedResource {
            path: "project.toml".into(),
            bytes: b"x".to_vec(),
        }])
    }

    fn invalid_plan() -> AtomicWritePlan {
        AtomicWritePlan::new(vec![StagedResource {
            path: "../outside".into(),
            bytes: b"x".to_vec(),
        }])
    }

    #[test]
    fn repository_creation_and_open_preserve_snapshot() {
        let repo = InMemoryProjectRepository::new();
        let created = repo.create(request("project")).unwrap();
        let expected = created.snapshot.clone();
        drop(created);
        assert_eq!(
            expected,
            repo.open(ProjectPath::new("project")).unwrap().snapshot
        );
    }

    #[test]
    fn repository_rejects_missing_projects_and_locked_opens() {
        let repo = InMemoryProjectRepository::new();
        assert!(matches!(
            repo.open(ProjectPath::new("missing")),
            Err(RepositoryError::Missing { .. })
        ));

        let open = repo.create(request("project")).unwrap();
        assert!(matches!(
            repo.open(ProjectPath::new("project")),
            Err(RepositoryError::Locked { .. })
        ));
        drop(open);
        assert!(repo.open(ProjectPath::new("project")).is_ok());
    }

    #[test]
    fn repository_loads_document_bodies_lazily() {
        let repo = InMemoryProjectRepository::new();
        let open = repo.create(request("project")).unwrap();
        assert_eq!(repo.body_load_count(), 0);
        assert_eq!(repo.load_document(DocumentId::new("doc")).unwrap(), b"body");
        assert_eq!(repo.body_load_count(), 1);
        drop(open);
    }

    #[test]
    fn repository_rejects_unsafe_paths() {
        let repo = InMemoryProjectRepository::new();
        let open = repo.create(request("project")).unwrap();
        drop(open);
        repo.mark_unsafe_manifest_path(Path::new("project"));
        assert!(matches!(
            repo.open(ProjectPath::new("project")),
            Err(RepositoryError::UnsafePath { .. })
        ));
    }

    #[test]
    fn repository_rejects_missing_resources_and_interrupted_saves() {
        let repo = InMemoryProjectRepository::new();
        let open = repo.create(request("missing-resource")).unwrap();
        drop(open);
        repo.remove_required_resource(Path::new("missing-resource"));
        assert!(matches!(
            repo.open(ProjectPath::new("missing-resource")),
            Err(RepositoryError::MissingResource { .. })
        ));

        let open = repo.create(request("interrupted-save")).unwrap();
        drop(open);
        repo.interrupt_save(Path::new("interrupted-save"));
        assert!(matches!(
            repo.open(ProjectPath::new("interrupted-save")),
            Err(RepositoryError::Interrupted { .. })
        ));
    }

    #[test]
    fn writer_rejects_stale_foreign_invalid_and_repeated_transitions() {
        let writer = InMemoryAtomicWriter::new(ProjectRootCapability::new(1));
        let stale = StagedWrite {
            root: 1,
            generation: 99,
            plan: plan(),
        };
        assert!(matches!(writer.commit(stale), Err(WriteError::Stale)));
        let foreign = StagedWrite::new(ProjectRootCapability::new(2), 1, plan());
        assert!(!writer.validate_staged(&foreign).is_valid());
        assert!(matches!(
            writer.commit(foreign),
            Err(WriteError::ForeignRoot)
        ));

        let invalid = writer.stage(invalid_plan()).unwrap();
        assert!(!writer.validate_staged(&invalid).is_valid());
        assert!(matches!(
            writer.commit(invalid),
            Err(WriteError::UnsafePath(_))
        ));

        let staged = writer.stage(plan()).unwrap();
        let forged = StagedWrite {
            plan: invalid_plan(),
            ..staged.clone()
        };
        assert!(!writer.validate_staged(&forged).is_valid());
        assert!(matches!(
            writer.commit(forged),
            Err(WriteError::InvalidTransition)
        ));

        writer.commit(staged.clone()).unwrap();
        assert!(matches!(
            writer.commit(staged),
            Err(WriteError::InvalidTransition)
        ));
    }

    #[test]
    fn writer_receipts_are_unique() {
        let writer = InMemoryAtomicWriter::new(ProjectRootCapability::new(1));
        let first = writer.commit(writer.stage(plan()).unwrap()).unwrap();
        let second = writer.commit(writer.stage(plan()).unwrap()).unwrap();
        assert_ne!(first, second);
    }

    #[test]
    fn writer_reconciles_generation_and_rejects_foreign_records() {
        let writer = InMemoryAtomicWriter::new(ProjectRootCapability::new(1));
        assert!(
            writer
                .reconcile(SaveTransactionRecord::new(ProjectRootCapability::new(1), 4,))
                .unwrap()
                .is_reconciled()
        );
        assert_eq!(writer.stage(plan()).unwrap().generation, 5);
        assert!(matches!(
            writer.reconcile(SaveTransactionRecord::new(ProjectRootCapability::new(2), 5)),
            Err(WriteError::ForeignRoot)
        ));
    }

    #[test]
    fn writer_abandonment_is_terminal() {
        let writer = InMemoryAtomicWriter::new(ProjectRootCapability::new(1));
        let staged = writer.stage(plan()).unwrap();
        assert!(writer.abandon(staged.clone()).unwrap().was_abandoned());
        assert!(matches!(
            writer.abandon(staged),
            Err(WriteError::InvalidTransition)
        ));
    }
}
