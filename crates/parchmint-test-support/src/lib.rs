use std::collections::{BTreeMap, VecDeque};
use std::fmt;
use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU64, Ordering},
};
use std::thread;

use parchmint_domain::{
    DocumentId, DomainError, NodeId, Project, ProjectCommand, ProjectId, apply_project_command,
};
use parchmint_project_format::{
    CanonicalCodec, CanonicalInputSet, CanonicalRelativePath, CanonicalResource,
    CanonicalResourceSet, FormatError, ProjectFormatCodec, ResourceId,
};

#[derive(Debug, Clone)]
pub struct DeterministicIdSource {
    state: u64,
}

impl DeterministicIdSource {
    pub fn with_seed(seed: u64) -> Self {
        Self {
            state: seed ^ 0xA5A5_A5A5_5A5A_5A5A,
        }
    }

    fn next_u64(&mut self) -> u64 {
        let mut x = self.state;
        x ^= x << 7;
        x ^= x >> 9;
        x ^= x << 8;
        self.state = x;
        x
    }

    fn next_id_bytes(&mut self) -> [u8; 16] {
        let first = self.next_u64().to_be_bytes();
        let second = self.next_u64().to_be_bytes();
        let mut bytes = [0u8; 16];
        bytes[..8].copy_from_slice(&first);
        bytes[8..].copy_from_slice(&second);
        bytes
    }

    pub fn next_node_id(&mut self) -> NodeId {
        NodeId::from_bytes(self.next_id_bytes())
    }

    pub fn next_document_id(&mut self) -> DocumentId {
        DocumentId::from_bytes(self.next_id_bytes())
    }

    pub fn next_project_id(&mut self) -> ProjectId {
        ProjectId::from_bytes(self.next_id_bytes())
    }
}

#[derive(Debug, Clone)]
pub struct ManualClock {
    now: u64,
    step: u64,
}

impl ManualClock {
    pub fn with_seed(seed: u64) -> Self {
        Self { now: seed, step: 1 }
    }

    pub fn now(&self) -> u64 {
        self.now
    }

    pub fn tick(&mut self) -> u64 {
        let current = self.now;
        self.now = self.now.wrapping_add(self.step);
        current
    }
}

impl Default for ManualClock {
    fn default() -> Self {
        Self::with_seed(0)
    }
}

#[derive(Debug)]
pub struct ProjectBuilder {
    pub ids: DeterministicIdSource,
    pub clock: ManualClock,
    project: Project,
    current_parent: NodeId,
    failure: Option<BuildError>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BuildError {
    Domain(DomainError),
}

impl fmt::Display for BuildError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Domain(error) => write!(formatter, "project build failed: {error}"),
        }
    }
}

impl std::error::Error for BuildError {}

impl From<DomainError> for BuildError {
    fn from(error: DomainError) -> Self {
        Self::Domain(error)
    }
}

impl ProjectBuilder {
    pub fn with_seed(seed: u64) -> Self {
        let mut ids = DeterministicIdSource::with_seed(seed);
        let project_id = ids.next_project_id();
        Self {
            project: Project::new(project_id),
            ids,
            clock: ManualClock::with_seed(seed),
            current_parent: NodeId::manuscript_root(),
            failure: None,
        }
    }

    fn push_command(&mut self, command: ProjectCommand) {
        if self.failure.is_some() {
            return;
        }

        let applied = apply_project_command(&self.project, self.project.revision, command);
        match applied {
            Ok(update) => {
                self.project = update.project;
                let _ = update.changed_resources;
                let _ = update.inverse;
                let _ = self.clock.tick();
            }
            Err(error) => {
                self.failure = Some(BuildError::Domain(error));
            }
        }
    }

    pub fn group(mut self, title: &str) -> Self {
        let index = self.project.nodes.children(self.current_parent).len();
        let id = self.ids.next_node_id();

        self.push_command(ProjectCommand::create_group(
            id,
            self.current_parent,
            index,
            title.to_owned(),
        ));
        if self.failure.is_none() {
            self.current_parent = id;
        }
        self
    }

    pub fn document(mut self, title: &str, _body: &str) -> Self {
        let index = self.project.nodes.children(self.current_parent).len();
        let id = self.ids.next_node_id();
        let document_id = self.ids.next_document_id();

        self.push_command(ProjectCommand::create_document(
            id,
            document_id,
            self.current_parent,
            index,
            title.to_owned(),
        ));
        self
    }

    pub fn build(self) -> Result<Project, BuildError> {
        if let Some(error) = self.failure {
            return Err(error);
        }
        self.project.validate().map_err(BuildError::from)?;
        Ok(self.project)
    }
}

#[derive(Debug, Clone)]
pub struct ProjectRootCapability {
    path: PathBuf,
}

impl ProjectRootCapability {
    pub fn as_path(&self) -> &Path {
        &self.path
    }
}

pub struct ScopedProject {
    pub root: ProjectRootCapability,
}

impl Drop for ScopedProject {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root.path);
    }
}

#[derive(Debug)]
pub enum FixtureError {
    MissingFixture(PathBuf),
    InvalidFixture(io::Error),
    InvalidFormat(FormatError),
}

impl fmt::Display for FixtureError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingFixture(path) => write!(formatter, "fixture not found: {path:?}"),
            Self::InvalidFixture(error) => write!(formatter, "fixture is invalid: {error}"),
            Self::InvalidFormat(error) => write!(formatter, "fixture format is invalid: {error}"),
        }
    }
}

impl std::error::Error for FixtureError {}

impl From<io::Error> for FixtureError {
    fn from(error: io::Error) -> Self {
        Self::InvalidFixture(error)
    }
}

impl From<FormatError> for FixtureError {
    fn from(error: FormatError) -> Self {
        Self::InvalidFormat(error)
    }
}

static FIXTURE_COUNTER: AtomicU64 = AtomicU64::new(0);

impl ScopedProject {
    pub fn from_fixture(fixture: &str) -> Result<Self, FixtureError> {
        let source = resolve_fixture_path(fixture);
        if !source.is_dir() {
            return Err(FixtureError::MissingFixture(source));
        }

        let target = temporary_root();
        copy_dir_recursive(&source, &target)?;

        Ok(Self {
            root: ProjectRootCapability { path: target },
        })
    }

    pub fn canonical_bytes(&self) -> Result<CanonicalResourceSet, FixtureError> {
        let codec = ProjectFormatCodec::default();
        let mut files = read_fixture_files(self.root.as_path())?;
        let format_control = files.remove(".parchmint/format-version").ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "fixture is missing format control",
            )
        })?;

        let decoded = codec.decode_project(CanonicalInputSet {
            format_control: Some(format_control),
            resources: files
                .into_iter()
                .map(|(path, bytes)| Ok((CanonicalRelativePath::parse(path)?, bytes)))
                .collect::<Result<BTreeMap<_, _>, FormatError>>()?,
        })?;

        let mut resources = BTreeMap::new();
        for (path, resource) in decoded.resources {
            let mut canonical = codec.encode(&resource)?;
            canonical.path = path;
            resources.insert(canonical.path.clone(), canonical);
        }

        let control = codec.encode(&CanonicalResource::FormatControl(decoded.format_version))?;
        resources.insert(control.path.clone(), control);

        Ok(CanonicalResourceSet {
            format_version: decoded.format_version,
            resources,
        })
    }

    pub fn canonical_document_bytes(&self) -> Result<CanonicalResourceSet, FixtureError> {
        self.canonical_bytes()
    }
}

fn temporary_root() -> PathBuf {
    let sequence = FIXTURE_COUNTER.fetch_add(1, Ordering::Relaxed);
    let mut path = std::env::temp_dir();
    path.push("parchmint-test-support");
    path.push(format!(
        "fixture-{pid}-{seed}",
        pid = std::process::id(),
        seed = sequence,
    ));
    path
}

fn resolve_fixture_path(fixture: &str) -> PathBuf {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let direct = manifest_dir.join("fixtures").join(fixture);
    if direct.is_dir() {
        return direct;
    }

    let with_extension = direct.with_extension("tmx");
    if with_extension.is_dir() {
        return with_extension;
    }

    direct
}

fn read_fixture_files(root: &Path) -> Result<BTreeMap<String, Vec<u8>>, FixtureError> {
    let mut output = BTreeMap::new();
    collect_files(root, root, &mut output)?;
    Ok(output)
}

fn collect_files(
    root: &Path,
    current: &Path,
    output: &mut BTreeMap<String, Vec<u8>>,
) -> io::Result<()> {
    for entry in fs::read_dir(current)? {
        let entry = entry?;
        let metadata = entry.file_type()?;
        if metadata.is_dir() {
            collect_files(root, &entry.path(), output)?;
            continue;
        }

        if !metadata.is_file() {
            continue;
        }

        let mut bytes = Vec::new();
        fs::File::open(entry.path())?.read_to_end(&mut bytes)?;

        let entry_path = entry.path();
        let rel = entry_path.strip_prefix(root).unwrap_or(&entry_path);
        let path = rel.to_string_lossy().replace('\\', "/");
        output.insert(path, bytes);
    }
    Ok(())
}

fn copy_dir_recursive(source: &Path, target: &Path) -> io::Result<()> {
    fs::create_dir_all(target)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let source_path = entry.path();
        let target_path = target.join(entry.file_name());
        let file_type = entry.file_type()?;

        if file_type.is_dir() {
            copy_dir_recursive(&source_path, &target_path)?;
        } else if file_type.is_file() {
            fs::copy(&source_path, &target_path)?;
        }
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FaultPoint {
    BeforeWrite,
    AfterTemporaryFlush,
    DuringReplace(ResourceId),
    AfterCanonicalCommit,
    BeforeCheckpoint,
    AfterCheckpoint,
    BeforeSaveAcknowledgement,
    DuringRecoveryCompaction,
    DuringCompositeReplacement(DocumentId),
    SearchBatch(u32),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FaultKind {
    Timeout,
    Io,
    Corruption,
    Interrupted,
}

#[derive(Debug, Clone)]
pub enum FaultAction {
    Continue,
    Fail(FaultKind),
    Pause(PauseHandle),
    Cancel,
}

pub trait FaultSchedule: Send + Sync {
    fn action_at(&self, point: &FaultPoint) -> FaultAction;
}

#[derive(Debug, Clone)]
pub struct PauseHandle {
    released: Arc<AtomicBool>,
}

impl PauseHandle {
    pub fn new() -> Self {
        Self {
            released: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn release(&self) {
        self.released.store(true, Ordering::SeqCst);
    }

    pub fn wait_for_release(&self) {
        while !self.released.load(Ordering::SeqCst) {
            thread::yield_now();
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InjectedFault {
    Cancelled(FaultPoint),
    Failed(FaultPoint, FaultKind),
    Paused(FaultPoint),
}

impl fmt::Display for InjectedFault {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Cancelled(point) => write!(formatter, "operation cancelled at {point:?}"),
            Self::Failed(point, kind) => {
                write!(formatter, "operation failed at {point:?}: {kind:?}")
            }
            Self::Paused(point) => write!(formatter, "operation paused at {point:?}"),
        }
    }
}

impl std::error::Error for InjectedFault {}

pub fn at_fault_point(
    schedule: &dyn FaultSchedule,
    point: FaultPoint,
) -> Result<(), InjectedFault> {
    match schedule.action_at(&point) {
        FaultAction::Continue => Ok(()),
        FaultAction::Fail(kind) => Err(InjectedFault::Failed(point, kind)),
        FaultAction::Pause(handle) => {
            handle.wait_for_release();
            Err(InjectedFault::Paused(point))
        }
        FaultAction::Cancel => Err(InjectedFault::Cancelled(point)),
    }
}

pub struct FaultingService<T> {
    inner: T,
    schedule: Arc<dyn FaultSchedule>,
}

impl<T> FaultingService<T> {
    pub fn new(inner: T, schedule: Arc<dyn FaultSchedule>) -> Self {
        Self { inner, schedule }
    }

    pub fn schedule_at(&self, point: FaultPoint) -> Result<(), InjectedFault> {
        at_fault_point(self.schedule.as_ref(), point)
    }

    pub fn inner(&self) -> &T {
        &self.inner
    }
}

pub type FaultingAtomicFileOps<T> = FaultingService<T>;
pub type FaultingHistoryStore<T> = FaultingService<T>;
pub type FaultingSearchIndex<T> = FaultingService<T>;
pub type FaultingRecoveryJournal<T> = FaultingService<T>;
pub type FaultingEditorAdapter<T> = FaultingService<T>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TaskId(u64);

impl TaskId {
    pub const fn new(value: u64) -> Self {
        Self(value)
    }
}

#[derive(Debug, Default)]
pub struct ControlledExecutor {
    pending: VecDeque<TaskId>,
}

impl ControlledExecutor {
    pub const fn new() -> Self {
        Self {
            pending: VecDeque::new(),
        }
    }

    pub fn enqueue(&mut self, task: TaskId) {
        self.pending.push_back(task);
    }

    pub fn run_next(&mut self) -> bool {
        self.pending.pop_front().is_some()
    }

    pub fn run_named(&mut self, task: TaskId) -> bool {
        let Some(position) = self.pending.iter().position(|queued| *queued == task) else {
            return false;
        };
        self.pending.remove(position);
        true
    }

    pub fn pending(&self) -> Vec<TaskId> {
        self.pending.iter().copied().collect()
    }
}

impl Default for PauseHandle {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seeded_builder_preserves_order_deterministically() {
        let build = || {
            ProjectBuilder::with_seed(0x5EED_5EED_5EED_5EED)
                .group("Part A")
                .document("First", "One")
                .document("Second", "Two")
                .build()
                .expect("builder should produce a valid project")
        };

        let first = build();
        let second = build();
        assert_eq!(first, second);

        let children = first.nodes.children(NodeId::manuscript_root());
        assert_eq!(children.len(), 1);
        let group = first.nodes.get(children[0]).expect("group should exist");
        assert_eq!(group.title, "Part A");
        assert_eq!(first.nodes.children(group.id).len(), 2);
    }

    #[test]
    fn canonical_fixture_bytes_are_stable() {
        let first =
            ScopedProject::from_fixture("canonical/minimal-project").expect("fixture should exist");
        let second =
            ScopedProject::from_fixture("canonical/minimal-project").expect("fixture should exist");

        assert_eq!(
            first.canonical_bytes().expect("fixture should decode"),
            second.canonical_bytes().expect("fixture should decode")
        );
    }

    struct ContinueSchedule;

    impl FaultSchedule for ContinueSchedule {
        fn action_at(&self, _point: &FaultPoint) -> FaultAction {
            FaultAction::Continue
        }
    }

    #[test]
    fn fault_schedule_supports_shared_wrappers_and_actions() {
        let schedule = Arc::new(ContinueSchedule);
        let atomic = FaultingAtomicFileOps::new((), schedule.clone());
        let history = FaultingHistoryStore::new((), schedule);

        assert!(atomic.schedule_at(FaultPoint::BeforeWrite).is_ok());
        assert!(history.schedule_at(FaultPoint::AfterCheckpoint).is_ok());
        assert!(matches!(
            at_fault_point(&CancelSchedule, FaultPoint::BeforeWrite),
            Err(InjectedFault::Cancelled(FaultPoint::BeforeWrite))
        ));
    }

    struct CancelSchedule;

    impl FaultSchedule for CancelSchedule {
        fn action_at(&self, point: &FaultPoint) -> FaultAction {
            match point {
                FaultPoint::BeforeWrite => FaultAction::Cancel,
                _ => FaultAction::Continue,
            }
        }
    }

    #[test]
    fn controlled_executor_runs_named_tasks_in_queue_order() {
        let mut executor = ControlledExecutor::new();
        assert!(executor.pending().is_empty());
        executor.enqueue(TaskId(1));
        executor.enqueue(TaskId(2));
        assert_eq!(executor.pending(), vec![TaskId(1), TaskId(2)]);
        assert!(executor.run_next());
        assert_eq!(executor.pending(), vec![TaskId(2)]);
        assert!(executor.run_next());
        assert!(!executor.run_next());
    }
}
