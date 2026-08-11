//! Developer-owned checks for the Stage 38 production composition boundary.

use std::{
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use parchmint_application::{DocumentVisibility, ProjectCommandDispatcher};
use parchmint_desktop::{
    DesktopBootstrap, FinalSaveResolution, LaunchRequest, NewProjectRequest, OpenProjectResult,
    ProductionControls, ProductionFaultKind, ProductionFaultPoint, ProductionObservation,
    ProductionProjectSession, ProjectFilesystemError, RequestedProjectPath,
};
use parchmint_domain::{NodeId, NodeKind, ProjectCommand, ProjectSection};
use parchmint_export_api::{
    ExportDefaults, ExportRequest, ExportRunOptions, ExportStyleCatalog, ProjectSnapshot,
};
use parchmint_platform_api::UntrustedPathSelection;
use parchmint_spellcheck_api::{
    DictionaryRevision, DocumentId, EditorRevision, LanguageId, SpellcheckGeneration,
    SpellcheckPriority, SpellcheckRequest,
};
use parchmint_test_support::ScopedProject;
use parchmint_ui_api::{
    CanonicalProjection, CreateDocumentWorkflow, ExportArtifactAction, ExportArtifactToken,
    ProjectSaveKind,
};

#[test]
fn production_constructor_retains_every_application_wide_service() {
    let bootstrap = DesktopBootstrap::production().expect("production graph should assemble");
    let graph = bootstrap
        .production_graph()
        .expect("production bootstrap should retain the typed graph");

    let ready = graph
        .controls()
        .observations()
        .into_iter()
        .filter_map(|observation| match observation {
            ProductionObservation::ComponentReady(component) => Some(component),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        ready,
        [
            "platform",
            "preferences",
            "appearance",
            "editor",
            "spellcheck",
            "export",
            "workspace-state",
            "project-service-factory",
            "iced-ui",
        ]
    );
    let _editor = graph.editor();
    let _spellcheck = graph.spellcheck();
    let _exporter = graph.exporter();
    let _workspace = graph.workspace_state();
}

#[test]
fn one_project_lease_retains_concrete_history_recovery_search_save_and_commands() {
    let project = ScopedProject::from_fixture("canonical/minimal-project")
        .expect("canonical project fixture should exist");
    let controls = ProductionControls::default();
    let bootstrap = DesktopBootstrap::production_with_controls(controls.clone())
        .expect("production graph should assemble");
    let requested = RequestedProjectPath::new(project.root.as_path());
    let session = bootstrap
        .project_filesystem
        .open(&requested)
        .expect("production project graph should open");
    let production = session
        .as_any()
        .downcast_ref::<ProductionProjectSession>()
        .expect("production filesystem should retain a typed project graph");

    assert_eq!(production.path(), project.root.as_path());
    assert!(production.history().verify().is_ok());
    assert!(production.recovery().inspect().is_ok());
    assert!(production.search().verify().is_ok());
    assert!(production.commands().project().is_ok());
    assert!(production.save().reconcile_open().is_ok());
    bootstrap
        .project_filesystem
        .begin_final_save(session.as_ref())
        .expect("clean final-save reconciliation should succeed");

    let observations = controls.observations();
    assert!(observations.iter().any(|observation| matches!(
        observation,
        ProductionObservation::ProjectOpened { path, .. } if path == project.root.as_path()
    )));
    assert!(observations.iter().any(|observation| matches!(
        observation,
        ProductionObservation::FinalSaveReconciled { path } if path == project.root.as_path()
    )));
    assert!(observations.iter().any(|observation| matches!(
        observation,
        ProductionObservation::ServiceOperation {
            point: ProductionFaultPoint::ProjectOpen,
            operation: "open",
            succeeded: true,
        }
    )));
    assert!(observations.iter().any(|observation| matches!(
        observation,
        ProductionObservation::ServiceOperation {
            point: ProductionFaultPoint::FinalSave,
            operation: "reconcile final save",
            succeeded: true,
        }
    )));

    controls.fail_next(
        ProductionFaultPoint::History,
        ProductionFaultKind::Corruption,
    );
    assert!(production.history().verify().is_err());
    assert!(production.history().verify().is_ok());
    assert_fault_and_operation(
        &controls,
        ProductionFaultPoint::History,
        ProductionFaultKind::Corruption,
        "verify",
    );
    controls.fail_next(
        ProductionFaultPoint::Recovery,
        ProductionFaultKind::Corruption,
    );
    assert!(production.recovery().inspect().is_err());
    assert!(production.recovery().inspect().is_ok());
    assert_fault_and_operation(
        &controls,
        ProductionFaultPoint::Recovery,
        ProductionFaultKind::Corruption,
        "inspect",
    );
    controls.fail_next(
        ProductionFaultPoint::Search,
        ProductionFaultKind::Corruption,
    );
    assert!(production.search().verify().is_err());
    assert!(production.search().verify().is_ok());
    assert_fault_and_operation(
        &controls,
        ProductionFaultPoint::Search,
        ProductionFaultKind::Corruption,
        "verify",
    );

    let competing = DesktopBootstrap::production().expect("competing graph should assemble");
    assert!(matches!(
        competing.project_filesystem.open(&requested),
        Err(ProjectFilesystemError::Locked { .. })
    ));
    drop(session);
    let reopened = competing
        .project_filesystem
        .open(&requested)
        .expect("dropping the exact production session must release its project lease");
    drop(reopened);
}

#[test]
fn production_open_delivers_current_typed_ports_that_retire_with_the_lease() {
    let project = ScopedProject::from_fixture("canonical/minimal-project")
        .expect("canonical project fixture should exist");
    let controls = ProductionControls::default();
    let bootstrap = DesktopBootstrap::production_with_controls(controls.clone())
        .expect("production graph should assemble");
    let runtime = block_on(bootstrap.start(LaunchRequest::launcher()))
        .expect("desktop startup should complete");
    let OpenProjectResult::Opened { window, session } = runtime
        .open_project(project.root.as_path())
        .expect("production project should open")
    else {
        panic!("production project should create a new session");
    };
    let project_ui = runtime
        .project_ui(session)
        .expect("typed project query should succeed")
        .expect("production sessions must expose typed UI ports");

    assert_eq!(project_ui.session(), session);
    let access = project_ui
        .ports
        .access()
        .expect("the newly opened generation should be authorized");
    assert_eq!(
        access
            .snapshot(|query| query.snapshot())
            .expect("current session should authorize snapshot access")
            .expect("current snapshot query should succeed")
            .project
            .id,
        project_ui.snapshot.project.id
    );
    assert!(access.history(|history| history.verify()).unwrap().is_ok());
    assert!(
        access
            .recovery(|recovery| recovery.inspect())
            .unwrap()
            .is_ok()
    );
    assert!(access.search(|search| search.verify()).unwrap().is_ok());
    let _ = access.save_status(|save| save.status()).unwrap();
    let _ = access
        .persistence(|persistence| persistence.status())
        .unwrap();
    access.workflows(|_| ()).unwrap();
    access.export_target(|_| ()).unwrap();
    let _ = access.commands(|commands| commands.undo_state()).unwrap();
    access.editor(|_| ()).unwrap();
    access.exporter(|_| ()).unwrap();
    access.workspace_state(|_| ()).unwrap();
    access.preferences(|_| ()).unwrap();
    access.appearance(|_| ()).unwrap();
    access.platform(|_| ()).unwrap();
    assert!(controls.observations().iter().any(|observation| matches!(
        observation,
        ProductionObservation::WindowOpened {
            window: observed_window,
            session_id,
            session_generation,
            typed_ports: true,
            native_editor: true,
        } if *observed_window == window
            && *session_id == session.session_id()
            && *session_generation == session.generation()
    )));

    let document = project_ui.snapshot.documents[0].document_id;
    controls.fail_next(ProductionFaultPoint::Recovery, ProductionFaultKind::Io);
    let first = CanonicalProjection::new(
        document,
        EditorRevision::from(1),
        "<p>first edit</p>",
        vec![],
        vec![],
        2,
    );
    assert!(
        access
            .persistence(|persistence| persistence.persist_editor_projection(first))
            .unwrap()
            .is_err(),
        "a recovery failure must prevent projection acknowledgement"
    );
    let second = CanonicalProjection::new(
        document,
        EditorRevision::from(2),
        "<p>second edit</p>",
        vec![],
        vec![],
        2,
    );
    access
        .persistence(|persistence| persistence.persist_editor_projection(second))
        .unwrap()
        .expect("the editor must remain writable after a recovery failure");
    assert!(
        !access
            .recovery(|recovery| recovery.inspect())
            .unwrap()
            .unwrap()
            .records
            .is_empty(),
        "recovery must be durable before the projection can be acknowledged"
    );
    let (handle, requested) = access
        .persistence(|persistence| persistence.request_save(ProjectSaveKind::Explicit))
        .unwrap()
        .expect("explicit save should capture the current revision");
    let saved = access
        .persistence(|persistence| persistence.await_save(handle))
        .unwrap()
        .expect("explicit save should durably complete");
    assert_eq!(saved.requested, requested);
    assert_eq!(saved.written.documents[&document], EditorRevision::from(2));

    let close = runtime
        .begin_final_save(project.root.as_path())
        .expect("clean project should start its final save");
    assert_eq!(
        runtime.resolve_final_save(close, Ok(())),
        Ok(FinalSaveResolution::Closed(window))
    );
    assert!(!runtime.is_current_session(session));
    assert!(project_ui.ports.access().is_err());
    assert!(access.commands(|commands| commands.undo_state()).is_err());
    assert!(
        access
            .persistence(|persistence| persistence.status())
            .is_err()
    );
    let OpenProjectResult::Opened {
        session: reopened, ..
    } = runtime
        .open_project(project.root.as_path())
        .expect("saved project should reopen")
    else {
        panic!("closed project should create a fresh session");
    };
    let reopened_ui = runtime
        .project_ui(reopened)
        .expect("reopened project query should succeed")
        .expect("reopened project should expose typed UI state");
    let reopened_document = reopened_ui
        .snapshot
        .documents
        .iter()
        .find(|snapshot| snapshot.document_id == document)
        .expect("saved document identity should survive reopen");
    assert_eq!(reopened_document.revision, EditorRevision::from(2));
    assert_eq!(reopened_document.body, "<p>second edit</p>");
}

#[test]
fn production_workflows_create_a_complete_document_and_export_to_an_authorized_path() {
    let project = ScopedProject::from_fixture("canonical/minimal-project")
        .expect("canonical project fixture should exist");
    let bootstrap = DesktopBootstrap::production().expect("production graph should assemble");
    let runtime = block_on(bootstrap.start(LaunchRequest::launcher())).unwrap();
    let OpenProjectResult::Opened { session, .. } = runtime
        .open_project(project.root.as_path())
        .expect("production project should open")
    else {
        panic!("production project should create a new session");
    };
    let project_ui = runtime
        .project_ui(session)
        .unwrap()
        .expect("typed project UI");
    let access = project_ui.ports.access().unwrap();
    let document = DocumentId::from_bytes([91; 16]);
    let created = access
        .workflows(|workflows| {
            workflows.create_document(CreateDocumentWorkflow {
                node: NodeId::from_bytes([90; 16]),
                document,
                parent: NodeId::manuscript_root(),
                index: usize::MAX,
                title: "Created Chapter".into(),
            })
        })
        .unwrap()
        .expect("complete document workflow");
    assert_eq!(
        created
            .snapshot
            .documents
            .iter()
            .find(|snapshot| snapshot.document_id == document)
            .expect("created document snapshot")
            .body,
        "<p></p>"
    );
    let named = access
        .workflows(|workflows| workflows.create_named_snapshot("Before export".into()))
        .unwrap()
        .expect("named snapshot workflow");
    let preview = access
        .history(|history| history.preview(named.checkpoint))
        .unwrap()
        .expect("named snapshot is present in History");
    assert_eq!(
        preview.checkpoint.category,
        parchmint_history_api::CheckpointCategory::NamedSnapshot
    );
    assert_eq!(
        preview.checkpoint.name.as_ref().map(|name| name.as_str()),
        Some("Before export")
    );

    let output = project.root.as_path().join("exported-project.html");
    let artifact = access
        .export_target(|export| {
            export.export_to_path(
                UntrustedPathSelection::new(&output),
                ExportRunOptions::default(),
            )
        })
        .unwrap()
        .expect("production export");
    let html = fs::read_to_string(&output).expect("completed export is visible");
    assert!(html.contains("Created Chapter"));
    assert!(html.contains("<p></p>"));
    assert_eq!(artifact.display_name, "exported-project.html");
    assert!(
        access
            .export_target(|export| export.act_on_artifact(
                ExportArtifactToken::from_raw(artifact.token.value() + 1000),
                ExportArtifactAction::Reveal,
            ))
            .unwrap()
            .is_err(),
        "forged artifact tokens must not expose arbitrary file actions"
    );
}

#[test]
fn production_create_opens_the_required_initial_document_and_empty_research_section() {
    let temporary = TemporaryDirectory::new("create-project");
    let destination = temporary.path().join("my-new-project");
    let bootstrap = DesktopBootstrap::production().expect("production graph should assemble");
    let session = bootstrap
        .project_filesystem
        .create(&NewProjectRequest::new(
            "My New Project",
            &destination,
            Some("A. Writer".to_owned()),
        ))
        .expect("repository creation should return an opened writable lease");
    let production = session
        .as_any()
        .downcast_ref::<ProductionProjectSession>()
        .expect("production creation should retain its typed session");
    let snapshot = production
        .ui_snapshot()
        .expect("new project should expose an initial UI snapshot");

    assert_eq!(snapshot.project.display_title, "My New Project");
    assert_eq!(snapshot.project.author.as_deref(), Some("A. Writer"));
    let manuscript = snapshot
        .project
        .nodes
        .children(ProjectSection::Manuscript.root_id());
    assert_eq!(manuscript.len(), 1);
    let initial_node = snapshot
        .project
        .nodes
        .get(manuscript[0])
        .expect("manuscript child should exist");
    assert_eq!(initial_node.title, "Untitled Document");
    let NodeKind::Document(initial_document) = initial_node.kind else {
        panic!("initial manuscript child should be a document");
    };
    assert!(
        snapshot
            .project
            .nodes
            .children(ProjectSection::Research.root_id())
            .is_empty()
    );
    assert_eq!(snapshot.documents.len(), 1);
    assert_eq!(snapshot.documents[0].document_id, initial_document);
    assert_eq!(snapshot.documents[0].visibility, DocumentVisibility::Open);
    assert_eq!(snapshot.documents[0].body, "<p></p>");
    assert!(destination.join("project.toml").is_file());
    assert!(
        destination
            .join("manuscript/untitled-document.html")
            .is_file()
    );

    bootstrap
        .project_filesystem
        .begin_final_save(session.as_ref())
        .expect("unchanged new project should close cleanly");
    drop(session);
}

#[test]
fn production_faults_are_explicit_one_shot_failures() {
    let project = ScopedProject::from_fixture("canonical/minimal-project")
        .expect("canonical project fixture should exist");
    let controls = ProductionControls::default();
    controls.fail_next(ProductionFaultPoint::ProjectOpen, ProductionFaultKind::Io);
    let bootstrap = DesktopBootstrap::production_with_controls(controls.clone())
        .expect("production graph should assemble");
    let requested = RequestedProjectPath::new(project.root.as_path());

    assert!(bootstrap.project_filesystem.open(&requested).is_err());
    assert_fault_and_operation(
        &controls,
        ProductionFaultPoint::ProjectOpen,
        ProductionFaultKind::Io,
        "open",
    );
    let session = bootstrap
        .project_filesystem
        .open(&requested)
        .expect("one-shot project-open fault must be consumed");
    controls.fail_next(ProductionFaultPoint::FinalSave, ProductionFaultKind::Io);
    assert!(
        bootstrap
            .project_filesystem
            .begin_final_save(session.as_ref())
            .is_err()
    );
    assert_fault_and_operation(
        &controls,
        ProductionFaultPoint::FinalSave,
        ProductionFaultKind::Io,
        "reconcile final save",
    );
    bootstrap
        .project_filesystem
        .begin_final_save(session.as_ref())
        .expect("one-shot final-save fault must be consumed");
    let production = session
        .as_any()
        .downcast_ref::<ProductionProjectSession>()
        .expect("typed project graph");
    let commands = production.commands();
    let mut edit = commands.execute(ProjectCommand::add_dictionary_word("parchmint"));
    poll_ready(edit.as_mut()).expect("project edit should be accepted");
    bootstrap
        .project_filesystem
        .begin_final_save(session.as_ref())
        .expect("final save should encode and acknowledge the dirty frontier");

    controls.fail_next(
        ProductionFaultPoint::Spellcheck,
        ProductionFaultKind::WorkerStopped,
    );
    let graph = bootstrap
        .production_graph()
        .expect("typed production graph");
    let mut spellcheck = graph.check_spelling(SpellcheckRequest {
        language: LanguageId::EnUs,
        document_id: DocumentId::from_bytes([1; 16]),
        document_revision: EditorRevision::default(),
        blocks: Vec::new(),
        project_dictionary: DictionaryRevision::default(),
        global_dictionary: DictionaryRevision::default(),
        generation: SpellcheckGeneration::default(),
        priority: SpellcheckPriority::Visible,
    });
    assert!(poll_ready(spellcheck.as_mut()).is_err());
    assert_fault_and_operation(
        &controls,
        ProductionFaultPoint::Spellcheck,
        ProductionFaultKind::WorkerStopped,
        "check",
    );

    controls.fail_next(ProductionFaultPoint::Export, ProductionFaultKind::Cancelled);
    let snapshot = ProjectSnapshot::new(
        ExportStyleCatalog::default(),
        ExportDefaults::default(),
        Vec::new(),
        Default::default(),
    );
    assert!(
        graph
            .exporter()
            .plan(
                ExportRequest::new("project.html", ExportRunOptions::default()),
                &snapshot,
            )
            .is_err()
    );
    assert_fault_and_operation(
        &controls,
        ProductionFaultPoint::Export,
        ProductionFaultKind::Cancelled,
        "plan",
    );
}

fn assert_fault_and_operation(
    controls: &ProductionControls,
    point: ProductionFaultPoint,
    kind: ProductionFaultKind,
    operation: &'static str,
) {
    let observations = controls.observations();
    assert!(observations.iter().any(|observation| matches!(
        observation,
        ProductionObservation::FaultInjected {
            point: observed_point,
            kind: observed_kind,
        } if *observed_point == point && *observed_kind == kind
    )));
    assert!(observations.iter().any(|observation| matches!(
        observation,
        ProductionObservation::ServiceOperation {
            point: observed_point,
            operation: observed_operation,
            succeeded: false,
        } if *observed_point == point && *observed_operation == operation
    )));
}

fn poll_ready<T>(mut future: std::pin::Pin<&mut dyn std::future::Future<Output = T>>) -> T {
    use std::task::{Context, Poll, Waker};

    let mut context = Context::from_waker(Waker::noop());
    match future.as_mut().poll(&mut context) {
        Poll::Ready(value) => value,
        Poll::Pending => panic!("injected operation should settle immediately"),
    }
}

fn block_on<T>(future: impl std::future::Future<Output = T>) -> T {
    use std::task::{Context, Poll, Waker};

    let mut future = Box::pin(future);
    let mut context = Context::from_waker(Waker::noop());
    loop {
        match future.as_mut().poll(&mut context) {
            Poll::Ready(value) => return value,
            Poll::Pending => std::thread::yield_now(),
        }
    }
}

struct TemporaryDirectory(PathBuf);

impl TemporaryDirectory {
    fn new(label: &str) -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("test clock should follow the Unix epoch")
            .as_nanos();
        let path = std::env::temp_dir().join(format!("parchmint-desktop-{label}-{nonce}"));
        fs::create_dir_all(&path).expect("temporary parent should be created");
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TemporaryDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
