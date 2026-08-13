# `parchmint-test-support`

## What it does

This development-only crate makes core tests deterministic and easy to replay.

It supplies builders for valid projects and fixtures containing the standard
project-file bytes, fixed clocks, predictable IDs, controlled task execution,
temporary project directories, and named places where a test can cause a
failure. Production builds do not include this crate or a test-mode switch.

Status: a captured-output utility is documented here but has no implementation
in the current source.

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

## Interface

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

pub enum FaultKind {
    Timeout,
    Io,
    Corruption,
    Interrupted,
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

// Status: the shared project-repository contract runner is documented but not
// yet implemented; InMemoryProjectRepository (parchmint-project-repository)
// and FsProjectRepository (parchmint-project-fs) still run their own tests.

pub struct FaultingService<T> {
    inner: T,
    schedule: Arc<dyn FaultSchedule>,
}

impl<T> FaultingService<T> {
    pub fn new(inner: T, schedule: Arc<dyn FaultSchedule>) -> Self {
        Self { inner, schedule }
    }

    pub fn schedule_at(&self, point: FaultPoint) -> Result<(), InjectedFault>;
    pub fn inner(&self) -> &T;
}

pub type FaultingAtomicFileOps<T> = FaultingService<T>;
pub type FaultingHistoryStore<T> = FaultingService<T>;
pub type FaultingSearchIndex<T> = FaultingService<T>;
pub type FaultingRecoveryJournal<T> = FaultingService<T>;
pub type FaultingEditorAdapter<T> = FaultingService<T>;

pub struct TaskId(u64);

impl TaskId {
    pub const fn new(value: u64) -> Self;
}

pub struct ControlledExecutor {
    pending: VecDeque<TaskId>,
}

impl ControlledExecutor {
    pub const fn new() -> Self;
    pub fn enqueue(&mut self, task: TaskId);
    pub fn run_next(&mut self) -> bool;
    pub fn run_named(&mut self, task: TaskId) -> bool;
    pub fn pending(&self) -> Vec<TaskId>;
}
```

## Implementation

IDs, time, ordering, text normalization, line endings, path casing, and Unicode
forms all come from explicit fixture data.

Status: a randomized-run reporter that prints the seed and the smallest failing
sequence is documented but not implemented.

```rust
fn at_fault_point(
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
```

A simulated failure reports whether it happened before the operation changed
anything, after the change reached durable storage, or at a point where the
outcome is unknown. Tests check the returned error and then reopen the project
to check the stored data.

The design goal is that every implementation of an interface runs the same
shared contract tests: for example, the in-memory repository and the filesystem
repository would both run the same project-repository contract. Status: the
shared contract-test runner is not yet implemented in this crate.

`FaultingService` wraps a service and can pause or fail one named operation.
The typed aliases `FaultingAtomicFileOps`, `FaultingHistoryStore`,
`FaultingSearchIndex`, `FaultingRecoveryJournal`, and `FaultingEditorAdapter`
expose that same gate, so tests can exercise the same interfaces used by the
application without depending on their concrete implementations. Status: no
production crate wraps its adapters in these wrappers yet. `ControlledExecutor`
runs or reorders the named tasks the test enqueued, without timing, thread, or
sleep dependencies.

Cleanup removes files only from the temporary project directory that
`ScopedProject` created; the directory is removed when the scoped value drops.
Expected output files do not contain the machine-specific temporary path.
