#![allow(missing_docs)] // Public bridge surface is summarized in the Stage 07 handoff.
//! Worker-owned compile/export use case.
//!
//! This module intentionally accepts only a frozen `CompileInput`, never a Qt
//! document or mutable `ProjectWorkspace`. Completions retain their original
//! `WorkStamp`; a bridge must compare that stamp with the live project and
//! revision before presenting success.

use parchmint_compile::{
    CancellationToken, CompileError, CompileInput, ExportError, ExportOptions, ExportReport,
    compile, export_cancellable,
};
use parchmint_domain::{CompilePreset, WorkStamp};
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::thread::{self, JoinHandle};
use thiserror::Error;

/// Complete, immutable work submitted from the UI thread.
pub struct CompileExportJob {
    pub stamp: WorkStamp,
    pub input: CompileInput,
    pub preset: CompilePreset,
    pub options: ExportOptions,
    pub cancellation: CancellationToken,
}

/// Completion delivered without touching a Qt object on the worker thread.
pub struct CompileExportCompletion {
    pub stamp: WorkStamp,
    pub outcome: Result<ExportReport, CompileExportError>,
}

/// Typed distinction between compiler and destination/export failures.
#[derive(Debug, Error)]
pub enum CompileExportError {
    #[error(transparent)]
    Compile(#[from] CompileError),
    #[error(transparent)]
    Export(#[from] ExportError),
}

/// A serial worker: writes to a user destination are ordered, while UI edits
/// remain responsive and cancel superseded work through its token.
pub struct CompileExportWorker {
    jobs: Option<Sender<CompileExportJob>>,
    results: Receiver<CompileExportCompletion>,
    worker: Option<JoinHandle<()>>,
}

impl CompileExportWorker {
    pub fn start(name: &str) -> Result<Self, std::io::Error> {
        let (job_sender, job_receiver) = mpsc::channel();
        let (result_sender, result_receiver) = mpsc::channel();
        let worker = thread::Builder::new()
            .name(name.into())
            .spawn(move || export_worker_loop(&job_receiver, &result_sender))?;
        Ok(Self {
            jobs: Some(job_sender),
            results: result_receiver,
            worker: Some(worker),
        })
    }

    pub fn submit(&self, job: CompileExportJob) -> Result<(), CompileExportWorkerError> {
        self.jobs
            .as_ref()
            .ok_or(CompileExportWorkerError::Closed)?
            .send(job)
            .map_err(|_| CompileExportWorkerError::Closed)
    }

    pub fn try_result(&self) -> Result<Option<CompileExportCompletion>, CompileExportWorkerError> {
        match self.results.try_recv() {
            Ok(result) => Ok(Some(result)),
            Err(TryRecvError::Empty) => Ok(None),
            Err(TryRecvError::Disconnected) => Err(CompileExportWorkerError::Closed),
        }
    }
}

impl Drop for CompileExportWorker {
    fn drop(&mut self) {
        drop(self.jobs.take());
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

fn export_worker_loop(
    jobs: &Receiver<CompileExportJob>,
    results: &Sender<CompileExportCompletion>,
) {
    while let Ok(job) = jobs.recv() {
        let outcome = compile(&job.input, &job.preset, &job.cancellation)
            .map_err(CompileExportError::Compile)
            .and_then(|(ir, _warnings)| {
                export_cancellable(&ir, &job.options, &job.cancellation)
                    .map_err(CompileExportError::Export)
            });
        if results
            .send(CompileExportCompletion {
                stamp: job.stamp,
                outcome,
            })
            .is_err()
        {
            break;
        }
    }
}

#[derive(Debug, Error)]
pub enum CompileExportWorkerError {
    #[error("compile/export worker is closed")]
    Closed,
}
