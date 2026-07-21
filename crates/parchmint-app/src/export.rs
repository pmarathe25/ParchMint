#![allow(missing_docs)] // Public export surface follows docs/reference/export-fidelity.md.
//! Worker-owned compile/export use case.
//!
//! This module intentionally accepts only a frozen `CompileInput`, never a Qt
//! document or mutable `ProjectWorkspace`. Completions retain their original
//! `WorkStamp`; a bridge must compare that stamp with the live project and
//! revision before presenting success.

use parchmint_compile::{
    CancellationToken, CompileError, CompileInput, CompileIr, ExportError, ExportOptions,
    PreparedExport, compile, prepare_export,
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
    /// PDF is rendered by the Qt owner after Rust compilation. All other
    /// formats are fully prepared on this worker.
    pub defer_pdf_render_to_ui: bool,
}

/// Worker result before a destination is committed.
pub enum CompileExportOutput {
    Prepared(PreparedExport),
    Compiled {
        ir: Box<CompileIr>,
        options: ExportOptions,
    },
}

/// Completion delivered without touching a Qt object on the worker thread.
pub struct CompileExportCompletion {
    pub stamp: WorkStamp,
    /// A validated temporary artifact. The UI owner compares the stamp and
    /// commits it; workers never mutate the requested destination.
    pub outcome: Result<CompileExportOutput, CompileExportError>,
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
                if job.defer_pdf_render_to_ui {
                    Ok(CompileExportOutput::Compiled {
                        ir: Box::new(ir),
                        options: job.options,
                    })
                } else {
                    prepare_export(&ir, &job.options, &job.cancellation)
                        .map(CompileExportOutput::Prepared)
                        .map_err(CompileExportError::Export)
                }
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

#[cfg(test)]
mod tests {
    use super::*;
    use parchmint_compile::{ExportFormat, commit_prepared_export};
    use parchmint_domain::{
        DocumentId, DocumentMetadata, DocumentRecord, Node, NodeId, NodeKind, ProjectCommand,
        ProjectGeneration, RelativeProjectPath, Revision,
    };
    use parchmint_storage::ProjectStorage;

    fn stamp() -> WorkStamp {
        WorkStamp {
            generation: ProjectGeneration::new(1).unwrap(),
            revision: Revision::new(7),
        }
    }

    fn sample_input() -> (tempfile::TempDir, CompileInput) {
        let directory = tempfile::tempdir().unwrap();
        let mut opened = ProjectStorage::create(directory.path(), "Export").unwrap();
        let node_id = NodeId::new();
        let document_id = DocumentId::new();
        opened
            .execute(ProjectCommand::Create {
                parent: opened.project.manuscript_root(),
                node: Node {
                    id: node_id,
                    kind: NodeKind::Document { document_id },
                    parent: Some(opened.project.manuscript_root()),
                    children: Vec::new(),
                },
                document: DocumentRecord {
                    id: document_id,
                    node_id,
                    path: RelativeProjectPath::new(format!("manuscript/{node_id}.md")).unwrap(),
                    metadata: DocumentMetadata {
                        title: "Chapter".into(),
                        ..DocumentMetadata::default()
                    },
                },
                index: 0,
            })
            .unwrap();
        opened
            .set_body(document_id, "A short *chapter*.\n".into())
            .unwrap();
        let input = CompileInput::from_open_project(&opened, stamp()).unwrap();
        (directory, input)
    }

    fn wait_for_completion(worker: &CompileExportWorker) -> CompileExportCompletion {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            match worker.try_result() {
                Ok(Some(completion)) => return completion,
                Ok(None) => {
                    assert!(
                        std::time::Instant::now() < deadline,
                        "compile/export worker timed out"
                    );
                    std::thread::yield_now();
                }
                Err(error) => panic!("compile/export worker closed unexpectedly: {error}"),
            }
        }
    }

    #[test]
    fn deferred_pdf_job_returns_ir_with_the_original_stamp() {
        let (_directory, input) = sample_input();
        let destination = tempfile::tempdir().unwrap();
        let worker = CompileExportWorker::start("test-export").unwrap();
        worker
            .submit(CompileExportJob {
                stamp: stamp(),
                input,
                preset: CompilePreset::manuscript("Manuscript"),
                options: ExportOptions::file(ExportFormat::Pdf, destination.path().join("out.pdf")),
                cancellation: CancellationToken::default(),
                defer_pdf_render_to_ui: true,
            })
            .unwrap();
        let completion = wait_for_completion(&worker);
        assert_eq!(completion.stamp, stamp());
        match completion.outcome {
            Ok(CompileExportOutput::Compiled { ir, options }) => {
                assert!(!ir.blocks.is_empty());
                assert_eq!(options.format, ExportFormat::Pdf);
            }
            Ok(CompileExportOutput::Prepared(_)) => {
                panic!("a deferred PDF must not produce a prepared file artifact")
            }
            Err(error) => panic!("deferred PDF job failed: {error}"),
        }
    }

    #[test]
    fn markdown_job_prepares_a_committable_export() {
        let (_directory, input) = sample_input();
        let destination = tempfile::tempdir().unwrap();
        let target = destination.path().join("out.md");
        let worker = CompileExportWorker::start("test-export").unwrap();
        worker
            .submit(CompileExportJob {
                stamp: stamp(),
                input,
                preset: CompilePreset::manuscript("Manuscript"),
                options: ExportOptions::file(ExportFormat::Markdown, target.clone()),
                cancellation: CancellationToken::default(),
                defer_pdf_render_to_ui: false,
            })
            .unwrap();
        let completion = wait_for_completion(&worker);
        assert_eq!(completion.stamp, stamp());
        let Ok(CompileExportOutput::Prepared(prepared)) = completion.outcome else {
            panic!("markdown job must produce a prepared export")
        };
        commit_prepared_export(prepared, &CancellationToken::default()).unwrap();
        let text = std::fs::read_to_string(&target).unwrap();
        assert!(text.contains("A short *chapter*."));
    }

    #[test]
    fn cancelled_job_reports_compile_cancellation() {
        let (_directory, input) = sample_input();
        let destination = tempfile::tempdir().unwrap();
        let cancellation = CancellationToken::default();
        cancellation.cancel();
        let worker = CompileExportWorker::start("test-export").unwrap();
        worker
            .submit(CompileExportJob {
                stamp: stamp(),
                input,
                preset: CompilePreset::manuscript("Manuscript"),
                options: ExportOptions::file(
                    ExportFormat::PlainText,
                    destination.path().join("out.txt"),
                ),
                cancellation,
                defer_pdf_render_to_ui: false,
            })
            .unwrap();
        let completion = wait_for_completion(&worker);
        assert_eq!(completion.stamp, stamp());
        assert!(matches!(
            completion.outcome,
            Err(CompileExportError::Compile(CompileError::Cancelled))
        ));
    }

    #[test]
    fn closed_channels_surface_typed_errors() {
        let (job_sender, job_receiver) = mpsc::channel::<CompileExportJob>();
        drop(job_receiver);
        let (_result_sender, results) = mpsc::channel::<CompileExportCompletion>();
        let worker = CompileExportWorker {
            jobs: Some(job_sender),
            results,
            worker: None,
        };
        let (_directory, input) = sample_input();
        let destination = tempfile::tempdir().unwrap();
        assert!(matches!(
            worker.submit(CompileExportJob {
                stamp: stamp(),
                input,
                preset: CompilePreset::manuscript("Manuscript"),
                options: ExportOptions::file(
                    ExportFormat::Markdown,
                    destination.path().join("out.md")
                ),
                cancellation: CancellationToken::default(),
                defer_pdf_render_to_ui: false,
            }),
            Err(CompileExportWorkerError::Closed)
        ));

        let (orphaned_sender, orphaned_results) = mpsc::channel::<CompileExportCompletion>();
        drop(orphaned_sender);
        let orphaned = CompileExportWorker {
            jobs: None,
            results: orphaned_results,
            worker: None,
        };
        assert!(matches!(
            orphaned.try_result(),
            Err(CompileExportWorkerError::Closed)
        ));
    }
}
