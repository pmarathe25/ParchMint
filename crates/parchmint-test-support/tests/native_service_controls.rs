//! Native contract tests for deterministic service controls.
//!
//! These tests are intentionally written before the service wrappers are
//! implemented.  They describe the small surface that production adapters
//! must expose to deterministic tests; they do not run the services.

use std::sync::{Arc, Mutex, mpsc};
use std::thread;

use parchmint_test_support::{
    ControlledExecutor, FaultAction, FaultKind, FaultPoint, FaultSchedule, FaultingAtomicFileOps,
    FaultingEditorAdapter, FaultingHistoryStore, FaultingRecoveryJournal, FaultingSearchIndex,
    InjectedFault, PauseHandle, TaskId, at_fault_point,
};

#[derive(Clone)]
struct ScriptedSchedule {
    action: FaultAction,
    seen: Arc<Mutex<Vec<FaultPoint>>>,
    started: Option<mpsc::SyncSender<FaultPoint>>,
}

impl FaultSchedule for ScriptedSchedule {
    fn action_at(&self, point: &FaultPoint) -> FaultAction {
        self.seen
            .lock()
            .expect("schedule lock should not be poisoned")
            .push(point.clone());
        if let Some(started) = &self.started {
            started
                .send(point.clone())
                .expect("pause observer should be listening");
        }
        self.action.clone()
    }
}

fn schedule(action: FaultAction) -> Arc<ScriptedSchedule> {
    Arc::new(ScriptedSchedule {
        action,
        seen: Arc::new(Mutex::new(Vec::new())),
        started: None,
    })
}

#[test]
fn every_service_wrapper_exposes_the_same_named_fault_gate() {
    let schedule = schedule(FaultAction::Fail(FaultKind::Io));
    let point = FaultPoint::BeforeWrite;

    macro_rules! assert_wrapper_fault {
        ($wrapper:ident) => {
            assert_eq!(
                $wrapper::new((), schedule.clone()).schedule_at(point.clone()),
                Err(InjectedFault::Failed(point.clone(), FaultKind::Io))
            );
        };
    }

    assert_wrapper_fault!(FaultingAtomicFileOps);
    assert_wrapper_fault!(FaultingHistoryStore);
    assert_wrapper_fault!(FaultingSearchIndex);
    assert_wrapper_fault!(FaultingRecoveryJournal);
    assert_wrapper_fault!(FaultingEditorAdapter);

    assert_eq!(
        *schedule
            .seen
            .lock()
            .expect("schedule lock should not be poisoned"),
        vec![
            point.clone(),
            point.clone(),
            point.clone(),
            point.clone(),
            point
        ],
    );
}

#[test]
fn named_fault_points_can_cancel_without_touching_a_project() {
    let schedule = schedule(FaultAction::Cancel);
    let points = [
        FaultPoint::BeforeWrite,
        FaultPoint::BeforeCheckpoint,
        FaultPoint::DuringRecoveryCompaction,
        FaultPoint::SearchBatch(7),
    ];

    for point in points {
        assert_eq!(
            at_fault_point(schedule.as_ref(), point.clone()),
            Err(InjectedFault::Cancelled(point)),
        );
    }
}

#[test]
fn a_pause_is_observable_and_released_without_a_sleep() {
    let gate = PauseHandle::new();
    let (started_tx, started_rx) = mpsc::sync_channel(0);
    let schedule = Arc::new(ScriptedSchedule {
        action: FaultAction::Pause(gate.clone()),
        seen: Arc::new(Mutex::new(Vec::new())),
        started: Some(started_tx),
    });
    let worker_schedule = schedule.clone();

    let worker = thread::spawn(move || {
        at_fault_point(worker_schedule.as_ref(), FaultPoint::AfterCanonicalCommit)
    });

    let point = started_rx
        .recv()
        .expect("worker should reach the named gate");
    assert_eq!(point, FaultPoint::AfterCanonicalCommit);
    gate.release();
    assert_eq!(
        worker.join().expect("worker should finish after release"),
        Err(InjectedFault::Paused(FaultPoint::AfterCanonicalCommit)),
    );
}

#[test]
fn controlled_executor_reorders_named_work_without_time_or_thread_races() {
    let mut executor = ControlledExecutor::new();
    let first = TaskId::new(1);
    let second = TaskId::new(2);
    let third = TaskId::new(3);

    executor.enqueue(first);
    executor.enqueue(second);
    executor.enqueue(third);
    assert_eq!(executor.pending(), vec![first, second, third]);

    assert!(executor.run_named(third));
    assert_eq!(executor.pending(), vec![first, second]);
    assert!(executor.run_next());
    assert_eq!(executor.pending(), vec![second]);
    assert!(executor.run_named(second));
    assert!(executor.pending().is_empty());
}
