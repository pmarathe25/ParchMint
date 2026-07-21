//! Application-service infrastructure: lazy snapshots and stale-safe workers.

mod commands;
mod diagnostics;
mod document;
mod export;
mod path;
mod search;
mod workspace;

pub use commands::*;
pub use diagnostics::*;
pub use document::*;
pub use export::*;
pub use path::*;
pub use search::*;
pub use workspace::*;

/// Qt-independent compile/export primitives used by the application bridge.
pub use parchmint_compile::{
    CancellationToken, CollisionPolicy, CompileIr, ExportFormat, ExportOptions, HtmlAssetMode,
    PreparedExport, commit_prepared_export, prepare_export_bytes, render_html,
};
/// Stable search query and row types returned by the project service.
pub use parchmint_index::{SearchQuery, SearchResult};

use parchmint_domain::WorkStamp;
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::thread::{self, JoinHandle};
use thiserror::Error;

/// A compact immutable node row suitable for a lazy Qt model adapter.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TreeRow {
    /// Stable row key.
    pub id: u32,
    /// Optional parent row key.
    pub parent: Option<u32>,
    /// Cached indentation depth.
    pub depth: u16,
    /// Display title.
    pub title: String,
}

/// Rust-owned outline snapshot. Qt adapters request only visible row ranges.
#[derive(Clone, Debug, Default)]
pub struct LazyTreeSnapshot {
    rows: Vec<TreeRow>,
}

impl LazyTreeSnapshot {
    /// Generates the deterministic 10,000-node stress tree.
    pub fn stress_fixture(count: u32) -> Self {
        let rows = (0..count)
            .map(|id| {
                let parent = (id > 0).then(|| (id - 1) / 10);
                let depth = if id == 0 {
                    0
                } else {
                    1 + u16::from((id - 1) / 10 > 0)
                };
                TreeRow {
                    id,
                    parent,
                    depth,
                    title: format!("Section {id}"),
                }
            })
            .collect();
        Self { rows }
    }

    /// Returns total row count without materializing Qt objects.
    pub fn len(&self) -> usize {
        self.rows.len()
    }

    /// Returns whether no rows exist.
    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    /// Borrows only the requested visible range.
    pub fn visible_rows(&self, start: usize, count: usize) -> &[TreeRow] {
        let start = start.min(self.rows.len());
        let end = start.saturating_add(count).min(self.rows.len());
        &self.rows[start..end]
    }
}

/// Unit of blocking work submitted away from the UI thread.
pub struct BackgroundJob {
    /// State correlation captured on submission.
    pub stamp: WorkStamp,
    /// Human-readable operation name for diagnostics.
    pub label: String,
    /// Blocking operation. It cannot access Qt UI objects.
    pub operation: Box<dyn FnOnce() -> Result<Vec<u8>, String> + Send + 'static>,
}

/// Completed background result retaining its original correlation stamp.
#[derive(Debug)]
pub struct BackgroundResult {
    /// State correlation captured on submission.
    pub stamp: WorkStamp,
    /// Human-readable operation name.
    pub label: String,
    /// Operation payload or user-displayable error.
    pub outcome: Result<Vec<u8>, String>,
}

/// Single-worker spike proving UI submissions are non-blocking.
pub struct BackgroundWorker {
    jobs: Option<Sender<BackgroundJob>>,
    results: Receiver<BackgroundResult>,
    worker: Option<JoinHandle<()>>,
}

impl BackgroundWorker {
    /// Starts the worker thread.
    pub fn start(name: &str) -> Result<Self, WorkerError> {
        let (job_sender, job_receiver) = mpsc::channel::<BackgroundJob>();
        let (result_sender, result_receiver) = mpsc::channel();
        let worker = thread::Builder::new()
            .name(name.to_owned())
            .spawn(move || worker_loop(&job_receiver, &result_sender))
            .map_err(WorkerError::Spawn)?;
        Ok(Self {
            jobs: Some(job_sender),
            results: result_receiver,
            worker: Some(worker),
        })
    }

    /// Enqueues work without executing it on the caller thread.
    pub fn submit(&self, job: BackgroundJob) -> Result<(), WorkerError> {
        self.jobs
            .as_ref()
            .ok_or(WorkerError::Closed)?
            .send(job)
            .map_err(|_| WorkerError::Closed)
    }

    /// Polls a completion without waiting, suitable for a Qt event-loop timer.
    pub fn try_result(&self) -> Result<Option<BackgroundResult>, WorkerError> {
        match self.results.try_recv() {
            Ok(result) => Ok(Some(result)),
            Err(TryRecvError::Empty) => Ok(None),
            Err(TryRecvError::Disconnected) => Err(WorkerError::Closed),
        }
    }
}

impl Drop for BackgroundWorker {
    fn drop(&mut self) {
        drop(self.jobs.take());
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

fn worker_loop(jobs: &Receiver<BackgroundJob>, results: &Sender<BackgroundResult>) {
    while let Ok(job) = jobs.recv() {
        let result = BackgroundResult {
            stamp: job.stamp,
            label: job.label,
            outcome: (job.operation)(),
        };
        if results.send(result).is_err() {
            break;
        }
    }
}

/// Worker lifecycle failure.
#[derive(Debug, Error)]
pub enum WorkerError {
    /// Worker thread could not start.
    #[error("could not start background worker: {0}")]
    Spawn(std::io::Error),
    /// Worker has already shut down.
    #[error("background worker is closed")]
    Closed,
}

#[cfg(test)]
mod tests {
    use super::*;
    use parchmint_domain::{ProjectGeneration, Revision};
    use std::time::{Duration, Instant};

    #[test]
    fn ten_thousand_node_snapshot_is_lazy_and_fast() {
        let start = Instant::now();
        let tree = LazyTreeSnapshot::stress_fixture(10_000);
        let elapsed = start.elapsed();
        assert_eq!(tree.len(), 10_000);
        assert_eq!(tree.visible_rows(9_990, 40).len(), 10);
        assert!(
            elapsed < Duration::from_millis(100),
            "construction took {elapsed:?}"
        );
    }

    #[test]
    fn background_job_submission_never_runs_on_caller() {
        let worker = BackgroundWorker::start("test-worker").unwrap();
        let generation = ProjectGeneration::new(1).unwrap();
        let stamp = WorkStamp {
            generation,
            revision: Revision::new(3),
        };
        let ui_thread = thread::current().id();
        worker
            .submit(BackgroundJob {
                stamp,
                label: "index".into(),
                operation: Box::new(move || {
                    assert_ne!(thread::current().id(), ui_thread);
                    Ok(b"complete".to_vec())
                }),
            })
            .unwrap();

        let deadline = Instant::now() + Duration::from_secs(2);
        let result = loop {
            if let Some(result) = worker.try_result().unwrap() {
                break result;
            }
            assert!(Instant::now() < deadline);
            thread::yield_now();
        };
        assert_eq!(result.outcome.unwrap(), b"complete");
        assert!(result.stamp.is_current(generation, Revision::new(3)));
        assert!(!result.stamp.is_current(generation, Revision::new(4)));
    }

    #[test]
    #[cfg_attr(debug_assertions, ignore = "release-mode performance gate")]
    fn records_tree_stress_measurement() {
        for nodes in [100, 1_000, 10_000] {
            let start = Instant::now();
            let tree = LazyTreeSnapshot::stress_fixture(nodes);
            let build = start.elapsed();
            let scroll_start = Instant::now();
            let mut observed = 0;
            for start in (0..nodes).step_by(12) {
                observed += tree
                    .visible_rows(usize::try_from(start).unwrap_or(usize::MAX), 40)
                    .len();
            }
            let scroll = scroll_start.elapsed();
            eprintln!(
                "scale nodes={nodes} tree-build={build:?}; simulated-scroll={scroll:?}; observed={observed}"
            );
            if nodes == 10_000 {
                assert!(
                    build < Duration::from_millis(100),
                    "tree build took {build:?}"
                );
                assert!(
                    scroll < Duration::from_millis(100),
                    "consumer projection exceeded 100 ms"
                );
            }
        }
    }
}
