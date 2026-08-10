//! Developer-owned checks for the Stage 38 production composition boundary.

use parchmint_application::ProjectCommandDispatcher;
use parchmint_desktop::{
    DesktopBootstrap, ProductionControls, ProductionFaultKind, ProductionFaultPoint,
    ProductionObservation, ProductionProjectSession, ProjectFilesystemError, RequestedProjectPath,
};
use parchmint_domain::ProjectCommand;
use parchmint_export_api::{
    ExportDefaults, ExportRequest, ExportRunOptions, ExportStyleCatalog, ProjectSnapshot,
};
use parchmint_spellcheck_api::{
    DictionaryRevision, DocumentId, EditorRevision, LanguageId, SpellcheckGeneration,
    SpellcheckPriority, SpellcheckRequest,
};
use parchmint_test_support::ScopedProject;

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
    assert!(
        bootstrap
            .project_filesystem
            .begin_final_save(session.as_ref())
            .is_err(),
        "close must not claim success for an unencoded dirty frontier"
    );

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
