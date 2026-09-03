//! Concrete native-desktop production composition.
//!
//! This private façade owns the public controls and values exposed by the
//! desktop crate. Implementation details are grouped by composition, project
//! session, workflow adaptation, and native callbacks.

mod dependencies {
    pub(super) use std::{
        collections::{BTreeMap, VecDeque},
        fs,
        future::Future,
        io::Write,
        path::{Path, PathBuf},
        process::{Command, Stdio},
        sync::{
            Arc, Mutex, Weak,
            atomic::{AtomicU64, Ordering},
        },
        task::{Context, Poll, Wake, Waker},
        thread,
    };

    pub(super) use parchmint_application::{
        ApplicationError, DocumentSnapshot, DocumentSnapshotLoader, DocumentVisibility,
        EditorPersistenceCoordinator, LazyDocumentSummary, NativeDocumentStateOwner,
        NativeProjectCommandDispatcher, PersistenceSaveKind, ProjectPersistenceCoordinator,
    };
    pub(super) use parchmint_contracts::{AnnotationAnchor, AnnotationThread};
    #[cfg(feature = "diagnostics")]
    pub(super) use parchmint_diagnostics::{self as diagnostics, Level as DiagnosticLevel};
    pub(super) use parchmint_domain::{
        BlockId, DocumentId, NodeId, NodeKind, Project, ProjectCommand, ProjectExportSetting,
        ProjectId, apply_project_command,
    };
    pub(super) use parchmint_editor_api::{
        BlockId as EditorBlockId, CanonicalComment, CanonicalCommentAnchor,
        CanonicalCommentMessage, CommentId, DocumentPosition, EditorRevision, EditorSelection,
    };
    pub(super) use parchmint_editor_iced::{EditorIcedAdapter, EditorIcedConfig};
    pub(super) use parchmint_export_api::{
        CancelOutcome, ExportDefaults, ExportError, ExportHandle, ExportNode, ExportPlan,
        ExportProgress, ExportProgressSink, ExportRequest, ExportRunOptions, ExportSettings,
        ExportSink, ExportSource, ExportStyleCatalog, ExportValidationReport, Exporter,
        InheritedSetting, ProjectSnapshot as ExportProjectSnapshot, SourceRevision,
    };
    pub(super) use parchmint_export_html::HtmlExporter;
    pub(super) use parchmint_history_api::{
        self as history, HistoryStore, ProjectRootCapability as HistoryRoot,
    };
    pub(super) use parchmint_history_git2::Git2HistoryStore;
    pub(super) use parchmint_platform_api::{PathDialog, PathDialogKind, WindowCapability};
    pub(super) use parchmint_platform_native::{NativePlatform, iced_adapter::IcedWindowRegistry};
    pub(super) use parchmint_preferences::{
        AppearanceController, AppearanceMode, AppearanceService, FilePreferenceStore,
        PreferenceChange, PreferenceCoordinator, PreferenceService, ResolvedAppearance,
    };
    pub(super) use parchmint_project_format::{
        CanonicalCodec, CanonicalDocumentSummary, CanonicalProjectPathMap, CanonicalRelativePath,
        ContentHash, ProjectFormatCodec,
    };
    pub(super) use parchmint_project_fs::{
        FsAtomicWriter, FsProjectRepository, NativeAtomicFileOps, NativeProjectFileSystem,
        ProjectFileSystem,
    };
    pub(super) use parchmint_project_repository::{
        CreateProject as RepositoryCreateProject, DocumentId as RepositoryDocumentId, OpenProject,
        ProjectPath, ProjectRepository, RepositoryError,
    };
    pub(super) use parchmint_recovery_api::{self as recovery, RecoveryJournal};
    pub(super) use parchmint_recovery_fs::FsRecoveryJournal;
    pub(super) use parchmint_save::{
        CheckpointIntent, CheckpointIntentStore, CheckpointReceipt, IntentStoreError,
        ProjectSaveCoordinator, SaveCoordinator, SaveRequest, SaveStatusSnapshot, SaveTicket,
    };
    pub(super) use parchmint_search_api::{
        self as search, RevisionId, SearchDocumentProjection, SearchField, SearchIndex,
        SearchProjectionSource, SearchProjectionVisitor, SearchTextProjection,
    };
    pub(super) use parchmint_search_sqlite::SqliteSearchIndex;
    pub(super) use parchmint_spellcheck_api::{
        DictionaryRevision, ProjectId as SpellcheckProjectId, SpellcheckService,
    };
    pub(super) use parchmint_spellcheck_en_us::{
        DictionaryLoadError, EnUsSpellcheckConfig, EnUsSpellcheckService, SavedDictionarySource,
        SpellcheckError, SpellcheckOperation,
    };
    pub(super) use parchmint_ui_api::{
        ApplicationServices as UiApplicationServices, CreateDocumentWorkflow, DocumentSummary,
        DocumentWordCount, DuplicateSubtreesWorkflow, ExportArtifact, ExportArtifactAction,
        ExportArtifactToken, ExportOperationToken, ExportOutcome, HistoryMaintenanceStatus,
        MoveNodesWorkflow, PlatformServices as UiPlatformServices,
        ProjectDuplicateWorkflowSnapshot, ProjectExportPort, ProjectHistoryMaintenancePort,
        ProjectPersistencePort, ProjectQueryError, ProjectSaveStatus,
        ProjectSnapshot as UiProjectSnapshot, ProjectSnapshotQuery, ProjectUiProject,
        ProjectUiServices, ProjectWorkflowPort, ProjectWorkflowSnapshot,
    };
    pub(super) use parchmint_ui_iced::{
        NativeCaptureRequest, NativeDesktopCallbacks, NativeDesktopError, NativeDesktopStartup,
        NativeNewProjectRequest, NativeProjectOpenResult, NativeProjectWindow, run_native_desktop,
    };
    pub(super) use parchmint_workspace_state::FileWorkspaceStateStore;
    pub(super) use sha2::{Digest, Sha256};

    pub(super) use crate::{
        ApplicationServices, DesktopBootstrap, DesktopRuntime, DesktopStartup, DesktopUi,
        DesktopUiError, NewProjectRequest, PlatformServices, ProjectFilesystemError,
        ProjectFilesystemService, ProjectSession, RequestedProjectPath, StartupError,
        resolved_appearance,
    };
}

use dependencies::*;

mod composition;
#[cfg(feature = "interaction-harness")]
mod interaction_harness;
mod native_callbacks;
mod project_session;
mod workflow_adapters;

pub use composition::ProductionApplicationGraph;
pub(crate) use composition::{assemble, assemble_with_controls};
#[cfg(feature = "interaction-harness")]
pub use interaction_harness::{DesktopInteractionHarness, InteractionHarnessError};
pub use project_session::{ProductionHistoryStatus, ProductionProjectSession};

/// Named production boundaries that an integration driver may fail once.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ProductionFaultPoint {
    ProjectOpen,
    FinalSave,
    Recovery,
    History,
    Search,
    Spellcheck,
    Export,
}

/// A deterministic failure selected by a complete-application test driver.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProductionFaultKind {
    Io,
    Corruption,
    Cancelled,
    WorkerStopped,
}

/// An operation observed at the production composition boundary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProductionObservation {
    ComponentReady(&'static str),
    ProjectOpened {
        path: PathBuf,
        project: ProjectId,
    },
    ProjectLocked {
        path: PathBuf,
    },
    FinalSaveReconciled {
        path: PathBuf,
    },
    FaultInjected {
        point: ProductionFaultPoint,
        kind: ProductionFaultKind,
    },
    ServiceOperation {
        point: ProductionFaultPoint,
        operation: &'static str,
        succeeded: bool,
    },
    WindowOpened {
        window: WindowCapability,
        session_id: u64,
        session_generation: u64,
        typed_ports: bool,
        native_editor: bool,
    },
    WindowFocused(WindowCapability),
    WindowRetained(WindowCapability),
    WindowClosed(WindowCapability),
    FinalSaveFailed {
        window: WindowCapability,
        reason: String,
    },
}

#[derive(Debug, Default)]
struct ControlState {
    faults: BTreeMap<ProductionFaultPoint, VecDeque<ProductionFaultKind>>,
    observations: Vec<ProductionObservation>,
}

/// Shared observation and one-shot fault controls.
///
/// Production uses an empty instance. Tests must explicitly enqueue faults;
/// consuming a fault records the exact boundary and kind.
#[derive(Debug, Clone, Default)]
pub struct ProductionControls {
    state: Arc<Mutex<ControlState>>,
}

impl ProductionControls {
    pub fn fail_next(&self, point: ProductionFaultPoint, kind: ProductionFaultKind) {
        self.state
            .lock()
            .expect("production controls mutex poisoned")
            .faults
            .entry(point)
            .or_default()
            .push_back(kind);
    }

    pub fn observations(&self) -> Vec<ProductionObservation> {
        self.state
            .lock()
            .expect("production controls mutex poisoned")
            .observations
            .clone()
    }

    fn observe(&self, observation: ProductionObservation) {
        self.state
            .lock()
            .expect("production controls mutex poisoned")
            .observations
            .push(observation);
    }

    fn take_fault(&self, point: ProductionFaultPoint) -> Option<ProductionFaultKind> {
        let mut state = self
            .state
            .lock()
            .expect("production controls mutex poisoned");
        let kind = state.faults.get_mut(&point)?.pop_front()?;
        state
            .observations
            .push(ProductionObservation::FaultInjected { point, kind });
        Some(kind)
    }

    fn service_operation(
        &self,
        point: ProductionFaultPoint,
        operation: &'static str,
        succeeded: bool,
    ) {
        self.observe(ProductionObservation::ServiceOperation {
            point,
            operation,
            succeeded,
        });
    }
}

struct ThreadWake(thread::Thread);

impl Wake for ThreadWake {
    fn wake(self: Arc<Self>) {
        self.0.unpark();
    }

    fn wake_by_ref(self: &Arc<Self>) {
        self.0.unpark();
    }
}

pub(crate) fn block_on<T>(future: impl Future<Output = T>) -> T {
    let mut future = Box::pin(future);
    let waker = Waker::from(Arc::new(ThreadWake(thread::current())));
    let mut context = Context::from_waker(&waker);
    loop {
        match future.as_mut().poll(&mut context) {
            Poll::Ready(value) => return value,
            Poll::Pending => thread::park(),
        }
    }
}

#[cfg(test)]
mod dictionary_source_tests {
    use super::composition::ProductionDictionarySource;
    use super::workflow_adapters::NativeExportSink;
    use super::*;
    use std::{
        fs,
        time::{SystemTime, UNIX_EPOCH},
    };

    use parchmint_preferences::PreferenceCommand;
    use parchmint_spellcheck_api::{
        EditorRevision, LanguageId, RevisionedTextRange, SpellcheckGeneration, SpellcheckPriority,
        SpellcheckRequest,
    };

    #[test]
    fn cancelled_native_export_preserves_target_and_removes_temporary_file() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let directory = std::env::temp_dir().join(format!("parchmint-export-cancel-{nonce}"));
        fs::create_dir(&directory).expect("temporary export directory");
        let target = directory.join("manuscript.html");
        fs::write(&target, b"previous output").expect("existing target");
        let (mut sink, name, _) = NativeExportSink::acquire(&target).expect("acquire output");
        let temporary = sink.temporary.clone();
        let target_capability =
            parchmint_export_api::ExportTargetCapability::checked(name).expect("target");
        let handle = ExportHandle::new();
        assert_eq!(handle.cancel(), CancelOutcome::Cancelled);
        assert!(matches!(
            handle.begin_temporary(&mut sink, &target_capability),
            Err(ExportError::Cancelled)
        ));
        drop(sink);

        assert_eq!(
            fs::read(&target).expect("preserved target"),
            b"previous output"
        );
        assert!(!temporary.exists(), "cancel must remove temporary output");
        fs::remove_file(&target).ok();
        fs::remove_dir(directory).ok();
    }

    struct FixedProjectQuery {
        snapshot: UiProjectSnapshot,
    }

    impl ProjectSnapshotQuery for FixedProjectQuery {
        fn snapshot(&self) -> Result<UiProjectSnapshot, ProjectQueryError> {
            Ok(self.snapshot.clone())
        }
    }

    fn project_snapshot(id: ProjectId, word: &str) -> UiProjectSnapshot {
        let mut project = Project::new(id);
        project
            .dictionary
            .insert(word)
            .expect("test dictionary word");
        UiProjectSnapshot {
            project,
            document_summaries: Vec::new(),
            documents: Vec::new(),
            styles_css: String::new(),
        }
    }

    #[test]
    fn persisted_dictionary_source_scopes_projects_and_reloads_global_preferences() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("test clock")
            .as_nanos();
        let path = std::env::temp_dir().join(format!("parchmint-dictionary-source-{nonce}.json"));
        let preferences: Arc<dyn PreferenceService> = Arc::new(PreferenceCoordinator::new(
            Arc::new(FilePreferenceStore::new(&path)),
        ));
        let source = Arc::new(ProductionDictionarySource::new(preferences.clone()));
        let first = ProjectId::from_bytes([11; 16]);
        let second = ProjectId::from_bytes([12; 16]);
        let first_query: Arc<dyn ProjectSnapshotQuery> = Arc::new(FixedProjectQuery {
            snapshot: project_snapshot(first, "Quillflux"),
        });
        let second_query: Arc<dyn ProjectSnapshotQuery> = Arc::new(FixedProjectQuery {
            snapshot: project_snapshot(second, "Fablewright"),
        });
        source.register_project(first, Arc::clone(&first_query));
        source.register_project(second, Arc::clone(&second_query));

        assert_eq!(
            source
                .project_words(first, DictionaryRevision::from(1))
                .expect("first project dictionary"),
            ["Quillflux"]
        );
        assert_eq!(
            source
                .project_words(second, DictionaryRevision::from(1))
                .expect("second project dictionary"),
            ["Fablewright"]
        );

        let current = block_on(preferences.load()).expect("load preferences");
        let updated = block_on(preferences.update(
            current.revision,
            PreferenceCommand::AddGlobalDictionaryWord("Globalthread".to_owned()),
        ))
        .expect("persist global dictionary word");
        assert_eq!(
            source
                .global_words(DictionaryRevision::from(updated.revision.value()))
                .expect("global dictionary"),
            ["Globalthread"]
        );

        let service = EnUsSpellcheckService::new(EnUsSpellcheckConfig {
            saved_dictionaries: source,
            ..EnUsSpellcheckConfig::default()
        })
        .expect("spellcheck service");
        block_on(service.reload_project_dictionary(first, DictionaryRevision::from(1)))
            .expect("hydrate project dictionary");
        block_on(
            service.reload_global_dictionary(DictionaryRevision::from(updated.revision.value())),
        )
        .expect("hydrate global dictionary");
        let request = SpellcheckRequest {
            language: LanguageId::EnUs,
            document_id: DocumentId::from_bytes([13; 16]),
            project_id: first,
            document_revision: EditorRevision::default(),
            blocks: vec![RevisionedTextRange {
                block_id: BlockId::from_bytes([13; 16]),
                range: parchmint_editor_api::EditorSelection::new(0_u64.into(), 22_u64.into()),
                text: "Quillflux Globalthread".to_owned(),
            }],
            project_dictionary: DictionaryRevision::from(1),
            global_dictionary: DictionaryRevision::from(updated.revision.value()),
            generation: SpellcheckGeneration::from(1),
            priority: SpellcheckPriority::Visible,
        };
        let mut results = block_on(service.check(request)).expect("spellcheck request");
        assert!(
            results.next().expect("spellcheck result").issues.is_empty(),
            "both persisted dictionary scopes must be active before recheck"
        );
        let _ = fs::remove_file(path);
    }

    #[test]
    fn project_registry_releases_and_prunes_closed_or_failed_open_queries() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("test clock")
            .as_nanos();
        let path =
            std::env::temp_dir().join(format!("parchmint-dictionary-source-lifetime-{nonce}.json"));
        let preferences: Arc<dyn PreferenceService> = Arc::new(PreferenceCoordinator::new(
            Arc::new(FilePreferenceStore::new(&path)),
        ));
        let source = ProductionDictionarySource::new(preferences);
        let project = ProjectId::from_bytes([14; 16]);
        let query: Arc<dyn ProjectSnapshotQuery> = Arc::new(FixedProjectQuery {
            snapshot: project_snapshot(project, "Ephemeralword"),
        });
        let query_lifetime = Arc::downgrade(&query);

        source.register_project(project, Arc::clone(&query));
        drop(query);

        assert!(
            query_lifetime.upgrade().is_none(),
            "the process-wide dictionary registry must not retain a closed session or a partial open"
        );
        let error = source
            .project_words(project, DictionaryRevision::from(1))
            .expect_err("a released project query must be unavailable");
        assert!(error.to_string().contains("unavailable"));
        assert!(
            source.projects.lock().expect("project registry").is_empty(),
            "failed lookup must prune the stale registry entry"
        );
        let _ = fs::remove_file(path);
    }

    #[test]
    fn project_registry_prunes_expired_entries_when_registering_a_project() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("test clock")
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "parchmint-dictionary-source-register-prune-{nonce}.json"
        ));
        let preferences: Arc<dyn PreferenceService> = Arc::new(PreferenceCoordinator::new(
            Arc::new(FilePreferenceStore::new(&path)),
        ));
        let source = ProductionDictionarySource::new(preferences);
        let expired_one = ProjectId::from_bytes([15; 16]);
        let expired_two = ProjectId::from_bytes([16; 16]);
        let live = ProjectId::from_bytes([17; 16]);

        for project in [expired_one, expired_two] {
            let query: Arc<dyn ProjectSnapshotQuery> = Arc::new(FixedProjectQuery {
                snapshot: project_snapshot(project, "Ephemeralword"),
            });
            source.register_project(project, query);
        }

        let live_query: Arc<dyn ProjectSnapshotQuery> = Arc::new(FixedProjectQuery {
            snapshot: project_snapshot(live, "Retainedword"),
        });
        source.register_project(live, Arc::clone(&live_query));

        let projects = source.projects.lock().expect("project registry");
        assert_eq!(projects.len(), 1);
        assert!(projects.contains_key(&live));
        drop(projects);
        let _ = fs::remove_file(path);
    }
}
