# `parchmint-test-support`

## What it does

This development-only crate makes core tests deterministic and easy to replay.

It supplies builders for valid projects and fixtures containing the standard
project-file bytes. It also supplies fixed clocks, predictable IDs, controlled
task execution, temporary project directories, captured output, and named
places where a test can cause a failure. Production builds do not include this
crate or a test-mode switch.

Builders create values through the same public API used by production code.
Tests that need invalid input provide invalid bytes to the real parser.

## How it works

```text
fixed seed + fixture
        |
        v
build public values -> run code -> capture result -> assert -> replay
                           ^
                    named fault schedule
```

Tests use explicit controls to pause, release, cancel, or reorder work at a
specific point. They do not depend on timing or sleep calls.

## Public API

```rust
pub struct ProjectBuilder {
    pub ids: DeterministicIdSource,
    pub clock: ManualClock,
}

impl ProjectBuilder {
    pub fn with_seed(seed: u64) -> Self;
    pub fn group(self, title: &str) -> Self;
    pub fn document(self, title: &str, body: &str) -> Self;
    pub fn build(self) -> Result<Project, BuildError>;
}

pub struct ScopedProject {
    pub root: ProjectRootCapability,
}

impl ScopedProject {
    pub fn from_fixture(fixture: &str) -> Result<Self, FixtureError>;
    pub fn canonical_bytes(&self) -> Result<CanonicalResourceSet, FixtureError>;
}

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

pub enum FaultAction {
    Continue,
    Fail(FaultKind),
    Pause(PauseHandle),
    Cancel,
}

pub trait FaultSchedule: Send + Sync {
    fn action_at(&self, point: &FaultPoint) -> FaultAction;
}

pub async fn project_repository_contract(
    make: impl Fn() -> Box<dyn ProjectRepository>,
) -> Result<(), ContractTestFailure> {
    run_repository_checks(make).await
}

pub struct FaultingAtomicFileOps<F> {
    inner: F,
    schedule: Arc<dyn FaultSchedule>,
}

impl<F> FaultingAtomicFileOps<F> {
    pub fn new(inner: F, schedule: Arc<dyn FaultSchedule>) -> Self {
        Self { inner, schedule }
    }
}

pub struct FaultingHistoryStore<H> {
    inner: H,
    schedule: Arc<dyn FaultSchedule>,
}

impl<H: HistoryStore> FaultingHistoryStore<H> {
    pub fn new(inner: H, schedule: Arc<dyn FaultSchedule>) -> Self {
        Self { inner, schedule }
    }
}

pub struct ControlledExecutor;

impl ControlledExecutor {
    pub fn run_next(&mut self) -> bool;
    pub fn run_named(&mut self, task: TaskId) -> bool;
    pub fn pending(&self) -> Vec<TaskId>;
}
```

## Implementation

IDs, time, ordering, text normalization, line endings, path casing, and Unicode
forms all come from explicit fixture data. Randomized runs always print the seed
and the smallest failing sequence.

```rust
fn at_fault_point(
    schedule: &dyn FaultSchedule,
    point: FaultPoint,
) -> Result<(), InjectedFault> {
    match schedule.action_at(&point) {
        FaultAction::Continue => Ok(()),
        FaultAction::Fail(kind) => Err(InjectedFault::new(point, kind)),
        FaultAction::Pause(handle) => handle.wait_for_release(),
        FaultAction::Cancel => Err(InjectedFault::cancelled(point)),
    }
}
```

A simulated failure reports whether it happened before the operation changed
anything, after the change reached durable storage, or at a point where the
outcome is unknown. Tests check the returned error and then reopen the project
to check the stored data.

Every implementation of an interface runs the same shared contract tests. For
example, the in-memory repository and filesystem repository both run
`project_repository_contract`.

`FaultingAtomicFileOps` wraps the disk interface and can pause or fail one disk
operation. Similar wrappers implement `HistoryStore`, `SearchIndex`,
`RecoveryJournal`, and `EditorAdapter`. Tests therefore exercise the same
interfaces used by the application. `ControlledExecutor` can pause the
application immediately before it reports a completed save.

Cleanup removes files only from the temporary project directory created by the
test. The test reports a cleanup failure. Expected output files do not contain
the machine-specific temporary path.
