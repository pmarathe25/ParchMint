use super::dependencies::*;
use super::project_session::{CanonicalDocumentLoader, ControlledHistory, ControlledSearch};
use super::*;

pub(super) struct ControlledExporter {
    pub(super) inner: HtmlExporter,
    pub(super) controls: ProductionControls,
}

impl Exporter for ControlledExporter {
    fn plan(
        &self,
        request: ExportRequest,
        project: &ExportProjectSnapshot,
    ) -> Result<ExportPlan, ExportError> {
        if let Some(kind) = self.controls.take_fault(ProductionFaultPoint::Export) {
            self.controls
                .service_operation(ProductionFaultPoint::Export, "plan", false);
            return Err(export_fault("plan", kind));
        }
        let result = self.inner.plan(request, project);
        self.controls
            .service_operation(ProductionFaultPoint::Export, "plan", result.is_ok());
        result
    }

    fn validate(&self, plan: &ExportPlan) -> ExportValidationReport {
        self.inner.validate(plan)
    }

    fn export(
        &self,
        plan: ExportPlan,
        sink: Box<dyn ExportSink>,
        handle: ExportHandle,
        progress: Arc<dyn ExportProgressSink>,
    ) -> Result<parchmint_export_api::ExportCompletion, ExportError> {
        if let Some(kind) = self.controls.take_fault(ProductionFaultPoint::Export) {
            self.controls
                .service_operation(ProductionFaultPoint::Export, "write", false);
            return Err(export_fault("write", kind));
        }
        let result = self.inner.export(plan, sink, handle, progress);
        self.controls
            .service_operation(ProductionFaultPoint::Export, "write", result.is_ok());
        result
    }

    fn cancel(&self, handle: &ExportHandle) {
        self.inner.cancel(handle);
        self.controls
            .service_operation(ProductionFaultPoint::Export, "cancel", true);
    }
}

pub(super) struct ProductionProjectQuery {
    pub(super) commands: Arc<NativeProjectCommandDispatcher>,
    pub(super) documents: Arc<NativeDocumentStateOwner>,
    pub(super) persistence: Arc<ProjectPersistenceCoordinator>,
    pub(super) persisted_summaries: BTreeMap<DocumentId, CanonicalDocumentSummary>,
    pub(super) document_loader: Arc<CanonicalDocumentLoader>,
    pub(super) search: Arc<ControlledSearch>,
}

impl ProjectSnapshotQuery for ProductionProjectQuery {
    fn snapshot(&self) -> Result<UiProjectSnapshot, ProjectQueryError> {
        // The dispatcher captures tree and document state under one operation
        // boundary. Reading them separately can otherwise combine a new tree
        // with an older document catalog while a command is completing.
        let authored = self
            .commands
            .authored_snapshot()
            .map_err(map_project_query_error)?;
        let project = authored.project;
        let documents = authored.documents;
        let loaded = documents
            .iter()
            .map(|document| (document.document_id, document))
            .collect::<BTreeMap<_, _>>();
        let document_summaries = authored
            .document_summaries
            .into_iter()
            .map(|summary| {
                let persisted = self
                    .persisted_summaries
                    .get(&summary.document_id)
                    .cloned()
                    .or_else(|| self.document_loader.hydrated_summary(summary.document_id));
                let loaded = loaded.get(&summary.document_id);
                DocumentSummary {
                    document_id: summary.document_id,
                    revision: summary.revision,
                    visibility: summary.visibility,
                    content_hash: loaded
                        .is_none()
                        .then(|| {
                            persisted
                                .as_ref()
                                .map(|summary| *summary.content_hash.as_bytes())
                        })
                        .flatten(),
                    word_count: loaded.map_or_else(
                        || {
                            persisted
                                .as_ref()
                                .map_or(DocumentWordCount::Pending, |summary| {
                                    DocumentWordCount::Known(summary.word_count)
                                })
                        },
                        |document| {
                            DocumentWordCount::Known(document.body.split_whitespace().count())
                        },
                    ),
                }
            })
            .collect();
        Ok(UiProjectSnapshot {
            project,
            document_summaries,
            documents,
            styles_css: self
                .persistence
                .canonical_text("styles.css")
                .map_err(map_project_query_error)?
                .unwrap_or_default(),
        })
    }

    fn load_document(&self, document: DocumentId) -> Result<DocumentSnapshot, ProjectQueryError> {
        let snapshot = self.documents.snapshot(document).map_err(|error| {
            let revision = self
                .documents
                .summaries()
                .ok()
                .and_then(|summaries| {
                    summaries
                        .into_iter()
                        .find(|summary| summary.document_id == document)
                })
                .map(|summary| RevisionId::from(summary.revision.value()))
                .unwrap_or_default();
            let _ = self.search.delete_document(document, revision);
            map_project_query_error(error)
        })?;
        if let Some(hash) = self.document_loader.recovery_hash(document) {
            self.persistence
                .register_loaded_document_base(document, snapshot.revision, hash)
                .map_err(map_project_query_error)?;
        }
        Ok(snapshot)
    }

    fn snapshot_for_export(&self) -> Result<UiProjectSnapshot, ProjectQueryError> {
        let mut snapshot = self.snapshot()?;
        snapshot.documents = self
            .documents
            .snapshots()
            .map_err(map_project_query_error)?;
        Ok(snapshot)
    }
}

fn map_project_query_error(error: impl std::fmt::Display) -> ProjectQueryError {
    ProjectQueryError::new(error.to_string())
}

pub(super) struct ProductionSaveStatus {
    pub(super) save: Arc<ProjectSaveCoordinator>,
}

impl ProjectSaveStatus for ProductionSaveStatus {
    fn status(&self) -> SaveStatusSnapshot {
        self.save.status()
    }
}

pub(super) struct SearchRefreshingCommands {
    pub(super) inner: Arc<NativeProjectCommandDispatcher>,
    pub(super) search: Arc<ControlledSearch>,
}

impl parchmint_application::ProjectCommandDispatcher for SearchRefreshingCommands {
    fn execute(
        &self,
        command: ProjectCommand,
    ) -> parchmint_application::AppFuture<
        '_,
        Result<
            parchmint_application::ProjectCommandResult,
            parchmint_application::ApplicationError,
        >,
    > {
        Box::pin(async move {
            let result = parchmint_application::ProjectCommandDispatcher::execute(
                self.inner.as_ref(),
                command,
            )
            .await;
            if result.is_ok() {
                let _ = self.search.refresh_live();
            }
            result
        })
    }

    fn undo(
        &self,
    ) -> parchmint_application::AppFuture<
        '_,
        Result<
            parchmint_application::ProjectCommandResult,
            parchmint_application::ApplicationError,
        >,
    > {
        Box::pin(async move {
            let result =
                parchmint_application::ProjectCommandDispatcher::undo(self.inner.as_ref()).await;
            if result.is_ok() {
                let _ = self.search.refresh_live();
            }
            result
        })
    }

    fn redo(
        &self,
    ) -> parchmint_application::AppFuture<
        '_,
        Result<
            parchmint_application::ProjectCommandResult,
            parchmint_application::ApplicationError,
        >,
    > {
        Box::pin(async move {
            let result =
                parchmint_application::ProjectCommandDispatcher::redo(self.inner.as_ref()).await;
            if result.is_ok() {
                let _ = self.search.refresh_live();
            }
            result
        })
    }

    fn undo_state(&self) -> parchmint_application::ProjectUndoState {
        parchmint_application::ProjectCommandDispatcher::undo_state(self.inner.as_ref())
    }

    fn reset_undo(&self, reason: parchmint_application::UndoResetReason) {
        parchmint_application::ProjectCommandDispatcher::reset_undo(self.inner.as_ref(), reason);
    }
}

impl parchmint_application::GlobalReplacement for SearchRefreshingCommands {
    fn preview(
        &self,
        selection: parchmint_application::ReplacementSelection,
    ) -> parchmint_application::AppFuture<
        '_,
        Result<parchmint_application::ReplacementPreview, parchmint_application::ApplicationError>,
    > {
        parchmint_application::GlobalReplacement::preview(self.inner.as_ref(), selection)
    }

    fn apply(
        &self,
        selection: parchmint_application::ReplacementSelection,
    ) -> parchmint_application::AppFuture<
        '_,
        Result<
            parchmint_application::ProjectCommandResult,
            parchmint_application::ApplicationError,
        >,
    > {
        Box::pin(async move {
            let result =
                parchmint_application::GlobalReplacement::apply(self.inner.as_ref(), selection)
                    .await;
            if result.is_ok() {
                let _ = self.search.refresh_live();
            }
            result
        })
    }
}

pub(super) struct SearchRefreshingPersistence {
    pub(super) inner: Arc<ProjectPersistenceCoordinator>,
    pub(super) search: Arc<ControlledSearch>,
}

impl ProjectPersistencePort for SearchRefreshingPersistence {
    fn persist_editor_projection(
        &self,
        projection: parchmint_ui_api::CanonicalProjection,
    ) -> Result<parchmint_ui_api::DurableProjectionAck, parchmint_ui_api::ProjectPersistenceError>
    {
        let result = self.inner.persist_editor_projection(projection);
        if result.is_ok() {
            let _ = self.search.refresh_live();
        }
        result
    }

    fn request_save(
        &self,
        kind: parchmint_ui_api::ProjectSaveKind,
    ) -> Result<
        (
            parchmint_ui_api::ProjectSaveHandle,
            parchmint_ui_api::ProjectPersistenceRevision,
        ),
        parchmint_ui_api::ProjectPersistenceError,
    > {
        self.inner.request_save(kind)
    }

    fn request_save_if_changed(
        &self,
        kind: parchmint_ui_api::ProjectSaveKind,
    ) -> Result<
        Option<(
            parchmint_ui_api::ProjectSaveHandle,
            parchmint_ui_api::ProjectPersistenceRevision,
        )>,
        parchmint_ui_api::ProjectPersistenceError,
    > {
        self.inner.request_save_if_changed(kind)
    }

    fn await_save(
        &self,
        handle: parchmint_ui_api::ProjectSaveHandle,
    ) -> Result<parchmint_ui_api::SavedProjectRevision, parchmint_ui_api::ProjectPersistenceError>
    {
        let result = self.inner.await_save(handle);
        if result.is_ok() {
            let _ = self.search.refresh_live();
        }
        result
    }

    fn status(&self) -> parchmint_ui_api::ProjectPersistenceStatus {
        self.inner.status()
    }

    fn reconcile_recovery(
        &self,
    ) -> Result<parchmint_ui_api::ProjectRecoveryState, parchmint_ui_api::ProjectPersistenceError>
    {
        self.inner.reconcile_recovery()
    }

    fn accept_recovery(
        &self,
        acceptance: parchmint_ui_api::ProjectRecoveryAcceptance,
    ) -> Result<parchmint_ui_api::ProjectRecoveryState, parchmint_ui_api::ProjectPersistenceError>
    {
        let result = self.inner.accept_recovery(acceptance);
        if result.is_ok() {
            let _ = self.search.refresh_live();
        }
        result
    }

    fn discard_recovery(
        &self,
        acceptance: parchmint_ui_api::ProjectRecoveryAcceptance,
    ) -> Result<parchmint_ui_api::ProjectRecoveryState, parchmint_ui_api::ProjectPersistenceError>
    {
        self.inner.discard_recovery(acceptance)
    }
}

pub(super) struct ProductionProjectWorkflows {
    pub(super) history: Arc<ControlledHistory>,
    pub(super) persistence: Arc<ProjectPersistenceCoordinator>,
    pub(super) query: Arc<ProductionProjectQuery>,
    pub(super) exporter: Arc<ControlledExporter>,
    pub(super) search: Arc<ControlledSearch>,
    pub(super) artifacts: Mutex<BTreeMap<ExportArtifactToken, PathBuf>>,
    pub(super) next_artifact: AtomicU64,
    pub(super) active_export: Mutex<Option<ActiveExportOperation>>,
    pub(super) next_export_operation: AtomicU64,
}

#[derive(Clone)]
pub(super) struct ActiveExportOperation {
    token: ExportOperationToken,
    handle: ExportHandle,
    progress: Arc<dyn ExportProgressSink>,
}

impl ProjectWorkflowPort for ProductionProjectWorkflows {
    fn create_document(
        &self,
        request: CreateDocumentWorkflow,
    ) -> Result<ProjectWorkflowSnapshot, ProjectQueryError> {
        let saved = self
            .persistence
            .create_document(request)
            .map_err(map_project_workflow_error)?;
        self.refresh_search();
        Ok(ProjectWorkflowSnapshot {
            snapshot: self.query.snapshot()?,
            checkpoint: saved.revision.checkpoint,
        })
    }

    fn restore_checkpoint(
        &self,
        checkpoint: parchmint_domain::CheckpointId,
    ) -> Result<ProjectWorkflowSnapshot, ProjectQueryError> {
        let plan = self
            .history
            .restore(checkpoint)
            .map_err(|error| ProjectQueryError::new(error.to_string()))?;
        let restored = self
            .persistence
            .restore_history(plan)
            .map_err(map_project_workflow_error)?;
        self.refresh_search();
        Ok(ProjectWorkflowSnapshot {
            snapshot: self.query.snapshot()?,
            checkpoint: restored.revision.checkpoint,
        })
    }

    fn create_named_snapshot(
        &self,
        name: String,
    ) -> Result<ProjectWorkflowSnapshot, ProjectQueryError> {
        let saved = self
            .persistence
            .create_named_snapshot(name)
            .map_err(map_project_workflow_error)?;
        Ok(ProjectWorkflowSnapshot {
            snapshot: self.query.snapshot()?,
            checkpoint: saved.checkpoint,
        })
    }

    fn delete_subtrees(
        &self,
        request: parchmint_ui_api::DeleteSubtreesWorkflow,
    ) -> Result<ProjectWorkflowSnapshot, ProjectQueryError> {
        let deleted = self
            .persistence
            .delete_subtrees(request)
            .map_err(map_project_workflow_error)?;
        self.refresh_search();
        Ok(ProjectWorkflowSnapshot {
            snapshot: self.query.snapshot()?,
            checkpoint: deleted.revision.checkpoint,
        })
    }

    fn move_nodes(
        &self,
        request: MoveNodesWorkflow,
    ) -> Result<ProjectWorkflowSnapshot, ProjectQueryError> {
        let saved = self
            .persistence
            .move_nodes(request)
            .map_err(map_project_workflow_error)?;
        self.refresh_search();
        Ok(ProjectWorkflowSnapshot {
            snapshot: self.query.snapshot()?,
            checkpoint: saved.checkpoint,
        })
    }

    fn duplicate_subtrees(
        &self,
        request: DuplicateSubtreesWorkflow,
    ) -> Result<ProjectDuplicateWorkflowSnapshot, ProjectQueryError> {
        let duplicated = self
            .persistence
            .duplicate_subtrees(request)
            .map_err(map_project_workflow_error)?;
        self.refresh_search();
        Ok(ProjectDuplicateWorkflowSnapshot {
            workflow: ProjectWorkflowSnapshot {
                snapshot: self.query.snapshot()?,
                checkpoint: duplicated.revision.checkpoint,
            },
            created_roots: duplicated.created_roots,
            node_ids: duplicated.node_ids,
            document_ids: duplicated.document_ids,
        })
    }
}

impl ProjectExportPort for ProductionProjectWorkflows {
    fn begin_export(
        &self,
        progress: Arc<dyn ExportProgressSink>,
    ) -> Result<ExportOperationToken, ProjectQueryError> {
        let token = ExportOperationToken::from_raw(
            self.next_export_operation.fetch_add(1, Ordering::Relaxed),
        );
        let operation = ActiveExportOperation {
            token,
            handle: ExportHandle::new(),
            progress,
        };
        let replaced = self
            .active_export
            .lock()
            .map_err(|_| ProjectQueryError::new("export operation registry is unavailable"))?
            .replace(operation);
        if let Some(replaced) = replaced {
            let _ = replaced.handle.cancel();
        }
        Ok(token)
    }

    fn export_to_path(
        &self,
        operation: ExportOperationToken,
        selection: parchmint_platform_api::UntrustedPathSelection,
        options: ExportRunOptions,
    ) -> Result<ExportOutcome, ProjectQueryError> {
        let active = self.active_export(operation)?;
        active.progress.report(ExportProgress::Planning);
        let result = (|| {
            if active.handle.status() == parchmint_export_api::ExportStatus::Cancelled {
                return Ok(ExportOutcome::Cancelled);
            }
            let snapshot = self.query.snapshot_for_export()?;
            let project = export_snapshot(&snapshot)?;
            let (sink, output_name, completed_path) =
                NativeExportSink::acquire(selection.as_path()).map_err(map_export_error)?;
            let request = ExportRequest::new(output_name.clone(), options);
            let plan = self.exporter.plan(request, &project).map_err(|error| {
                active.handle.fail();
                map_export_error(error)
            })?;
            let report = self.exporter.validate(&plan);
            if !report.is_valid() {
                active.handle.fail();
                return Err(map_export_error(ExportError::Validation(report)));
            }
            match self.exporter.export(
                plan,
                Box::new(sink),
                active.handle.clone(),
                Arc::clone(&active.progress),
            ) {
                Ok(_) => {}
                Err(ExportError::Cancelled) => return Ok(ExportOutcome::Cancelled),
                Err(error) => return Err(map_export_error(error)),
            }
            if !self.export_is_active(operation)? {
                return Err(ProjectQueryError::new("export operation is stale"));
            }
            let artifact =
                ExportArtifactToken::from_raw(self.next_artifact.fetch_add(1, Ordering::Relaxed));
            self.artifacts
                .lock()
                .map_err(|_| ProjectQueryError::new("export artifact registry is unavailable"))?
                .insert(artifact, completed_path);
            Ok(ExportOutcome::Completed(ExportArtifact {
                token: artifact,
                display_name: output_name,
            }))
        })();
        self.clear_export_if_active(operation)?;
        result
    }

    fn cancel_export(
        &self,
        operation: ExportOperationToken,
    ) -> Result<CancelOutcome, ProjectQueryError> {
        let active = self.active_export(operation)?;
        Ok(active.handle.cancel())
    }

    fn act_on_artifact(
        &self,
        artifact: ExportArtifactToken,
        action: ExportArtifactAction,
    ) -> Result<(), ProjectQueryError> {
        let path = self
            .artifacts
            .lock()
            .map_err(|_| ProjectQueryError::new("export artifact registry is unavailable"))?
            .get(&artifact)
            .cloned()
            .ok_or_else(|| ProjectQueryError::new("export artifact token is unknown"))?;
        open_export_artifact(&path, action)
    }
}

impl ProductionProjectWorkflows {
    fn refresh_search(&self) {
        let _ = self.search.refresh_live();
    }

    fn active_export(
        &self,
        token: ExportOperationToken,
    ) -> Result<ActiveExportOperation, ProjectQueryError> {
        self.active_export
            .lock()
            .map_err(|_| ProjectQueryError::new("export operation registry is unavailable"))?
            .as_ref()
            .filter(|active| active.token == token)
            .cloned()
            .ok_or_else(|| ProjectQueryError::new("export operation is stale"))
    }

    fn export_is_active(&self, token: ExportOperationToken) -> Result<bool, ProjectQueryError> {
        Ok(self
            .active_export
            .lock()
            .map_err(|_| ProjectQueryError::new("export operation registry is unavailable"))?
            .as_ref()
            .is_some_and(|active| active.token == token))
    }

    fn clear_export_if_active(&self, token: ExportOperationToken) -> Result<(), ProjectQueryError> {
        let mut active = self
            .active_export
            .lock()
            .map_err(|_| ProjectQueryError::new("export operation registry is unavailable"))?;
        if active.as_ref().is_some_and(|active| active.token == token) {
            *active = None;
        }
        Ok(())
    }
}

fn map_project_workflow_error(error: impl std::fmt::Display) -> ProjectQueryError {
    ProjectQueryError::new(error.to_string())
}

fn map_export_error(error: ExportError) -> ProjectQueryError {
    ProjectQueryError::new(error.to_string())
}

pub(super) struct NativeExportSink {
    target: PathBuf,
    pub(super) temporary: PathBuf,
    file: Option<fs::File>,
    expected_name: String,
    started: bool,
}

impl NativeExportSink {
    pub(super) fn acquire(path: &Path) -> Result<(Self, String, PathBuf), ExportError> {
        if !path.is_absolute() {
            return Err(export_sink_error(
                "authorize",
                "target must be an absolute path",
            ));
        }
        let parent = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .ok_or_else(|| export_sink_error("authorize", "target has no parent directory"))?;
        let parent_metadata = fs::symlink_metadata(parent)
            .map_err(|error| export_sink_error("authorize", error.to_string()))?;
        if !parent_metadata.is_dir() || parent_metadata.file_type().is_symlink() {
            return Err(export_sink_error(
                "authorize",
                "target parent is not a direct directory",
            ));
        }
        if let Ok(metadata) = fs::symlink_metadata(path)
            && (!metadata.is_file() || metadata.file_type().is_symlink())
        {
            return Err(export_sink_error(
                "authorize",
                "existing target is not a direct regular file",
            ));
        }
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .filter(|name| !name.is_empty())
            .ok_or_else(|| export_sink_error("authorize", "target filename is invalid"))?
            .to_owned();
        // The export planner accepts a portable name, never the OS path.
        parchmint_export_api::ExportTargetCapability::checked(&name)
            .map_err(|issue| ExportError::Validation(ExportValidationReport::from_issue(issue)))?;
        static NEXT_TEMPORARY: AtomicU64 = AtomicU64::new(1);
        let sequence = NEXT_TEMPORARY.fetch_add(1, Ordering::Relaxed);
        let temporary = parent.join(format!(
            ".{name}.parchmint-export-{}-{sequence}.tmp",
            std::process::id()
        ));
        let file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .map_err(|error| export_sink_error("authorize", error.to_string()))?;
        Ok((
            Self {
                target: path.to_path_buf(),
                temporary,
                file: Some(file),
                expected_name: name.clone(),
                started: false,
            },
            name,
            path.to_path_buf(),
        ))
    }
}

impl ExportSink for NativeExportSink {
    fn start(
        &mut self,
        target: &parchmint_export_api::ExportTargetCapability,
    ) -> Result<(), ExportError> {
        if self.started || target.name().as_str() != self.expected_name {
            return Err(ExportError::InvalidState);
        }
        self.started = true;
        Ok(())
    }

    fn write_chunk(&mut self, bytes: &[u8]) -> Result<(), ExportError> {
        if !self.started {
            return Err(ExportError::InvalidState);
        }
        self.file
            .as_mut()
            .ok_or(ExportError::InvalidState)?
            .write_all(bytes)
            .map_err(|error| export_sink_error("write", error.to_string()))
    }

    fn finish(&mut self) -> Result<(), ExportError> {
        let mut file = self.file.take().ok_or(ExportError::InvalidState)?;
        file.flush()
            .and_then(|_| file.sync_all())
            .map_err(|error| export_sink_error("finish", error.to_string()))?;
        drop(file);
        fs::rename(&self.temporary, &self.target)
            .map_err(|error| export_sink_error("finish", error.to_string()))
    }

    fn abort(&mut self) {
        self.file.take();
        let _ = fs::remove_file(&self.temporary);
    }
}

impl Drop for NativeExportSink {
    fn drop(&mut self) {
        if self.file.is_some() {
            self.abort();
        }
    }
}

fn export_sink_error(operation: &'static str, reason: impl Into<String>) -> ExportError {
    ExportError::Sink {
        operation,
        reason: reason.into(),
    }
}

fn export_snapshot(
    snapshot: &UiProjectSnapshot,
) -> Result<ExportProjectSnapshot, ProjectQueryError> {
    let sources = snapshot
        .documents
        .iter()
        .map(|document| {
            (
                document.document_id,
                ExportSource {
                    revision: SourceRevision::from(document.revision.value()),
                    body: document.body.clone(),
                },
            )
        })
        .collect();
    let manuscript = export_nodes(&snapshot.project, NodeId::manuscript_root())?;
    let research = export_nodes(&snapshot.project, NodeId::research_root())?;
    let mut project = ExportProjectSnapshot::new(
        ExportStyleCatalog::new(snapshot.styles_css.clone()),
        ExportDefaults {
            emit_titles: snapshot.project.export_settings.emit_titles
                != ProjectExportSetting::Disabled,
            start_new_page: snapshot.project.export_settings.starts_new_page,
        },
        manuscript,
        sources,
    );
    project.research = research;
    Ok(project)
}

fn export_nodes(project: &Project, parent: NodeId) -> Result<Vec<ExportNode>, ProjectQueryError> {
    project
        .nodes
        .children(parent)
        .iter()
        .filter_map(|id| {
            let node = project.nodes.get(*id)?;
            let settings = ExportSettings {
                emit_titles: export_setting(node.export_settings.emit_titles),
                start_new_page: if node.export_settings.starts_new_page {
                    InheritedSetting::Enabled
                } else {
                    InheritedSetting::Inherit
                },
            };
            Some(match node.kind {
                NodeKind::Document(document) => {
                    Ok(ExportNode::document(document, node.title.clone(), settings))
                }
                NodeKind::Group => export_nodes(project, *id)
                    .map(|children| ExportNode::group(node.title.clone(), settings, children)),
                NodeKind::Root(_) => Err(ProjectQueryError::new(
                    "project section contains a nested root",
                )),
            })
        })
        .collect()
}

fn export_setting(setting: ProjectExportSetting) -> InheritedSetting {
    match setting {
        ProjectExportSetting::Inherit => InheritedSetting::Inherit,
        ProjectExportSetting::Enabled => InheritedSetting::Enabled,
        ProjectExportSetting::Disabled => InheritedSetting::Disabled,
    }
}

fn open_export_artifact(
    path: &Path,
    action: ExportArtifactAction,
) -> Result<(), ProjectQueryError> {
    let mut command = if cfg!(target_os = "macos") {
        let mut command = Command::new("open");
        if action == ExportArtifactAction::Reveal {
            command.arg("-R");
        }
        command.arg(path);
        command
    } else if cfg!(target_os = "windows") {
        let mut command = Command::new("explorer");
        if action == ExportArtifactAction::Reveal {
            command.arg(format!("/select,{}", path.display()));
        } else {
            command.arg(path);
        }
        command
    } else {
        let mut command = Command::new("xdg-open");
        command.arg(if action == ExportArtifactAction::Reveal {
            path.parent().unwrap_or(path)
        } else {
            path
        });
        command
    };
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map(|_| ())
        .map_err(|error| ProjectQueryError::new(format!("open export artifact failed: {error}")))
}

fn export_fault(operation: &'static str, kind: ProductionFaultKind) -> ExportError {
    match kind {
        ProductionFaultKind::Cancelled => ExportError::Cancelled,
        ProductionFaultKind::Io
        | ProductionFaultKind::Corruption
        | ProductionFaultKind::WorkerStopped => ExportError::Sink {
            operation,
            reason: format!("injected {kind:?} fault"),
        },
    }
}

pub(super) fn spellcheck_fault(kind: ProductionFaultKind) -> SpellcheckError {
    match kind {
        ProductionFaultKind::Cancelled => SpellcheckError::QueueFull,
        ProductionFaultKind::Io
        | ProductionFaultKind::Corruption
        | ProductionFaultKind::WorkerStopped => SpellcheckError::WorkerStopped,
    }
}
