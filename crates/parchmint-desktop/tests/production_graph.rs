//! Developer-owned checks for the Stage 38 production composition boundary.

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{SystemTime, UNIX_EPOCH},
};

use parchmint_application::{DocumentVisibility, ProjectCommandDispatcher};
use parchmint_desktop::{
    DesktopBootstrap, FinalSaveResolution, LaunchRequest, NewProjectRequest, OpenProjectResult,
    ProductionControls, ProductionFaultKind, ProductionFaultPoint, ProductionHistoryStatus,
    ProductionObservation, ProductionProjectSession, ProjectFilesystemError, RequestedProjectPath,
};
use parchmint_domain::{
    MetadataApplicability, MetadataFieldDefinition, MetadataFieldId, MetadataTextKind, NodeId,
    NodeKind, ProjectCommand, ProjectExportSettings, ProjectSection,
};
use parchmint_editor_api::{
    AnnotationValue, BlockId, CanonicalComment, CanonicalCommentAnchor, CanonicalCommentMessage,
    CommentId, DocumentPosition, EditorSelection,
};
use parchmint_export_api::{
    ExportDefaults, ExportRequest, ExportRunOptions, ExportStyleCatalog, IgnoreExportProgress,
    ProjectSnapshot,
};
use parchmint_platform_api::UntrustedPathSelection;
use parchmint_search_api::{
    SearchBatch, SearchBatchSink, SearchField, SearchHit, SearchIndex, SearchQuery,
    SearchRebuildStatus,
};
use parchmint_spellcheck_api::{
    DictionaryRevision, DocumentId, EditorRevision, LanguageId, SpellcheckGeneration,
    SpellcheckPriority, SpellcheckRequest,
};
use parchmint_test_support::ScopedProject;
use parchmint_ui_api::{
    CanonicalProjection, CreateDocumentWorkflow, DocumentWordCount, ExportArtifactAction,
    ExportArtifactToken, ExportOutcome, ProjectSaveKind,
};

#[test]
fn legacy_summary_hydration_runs_in_background_without_opening_unselected_documents() {
    let project = ScopedProject::from_fixture("canonical/minimal-project").unwrap();
    fs::write(
        project.root.as_path().join("manuscript/chapter-2.html"),
        b"<p>second legacy chapter</p>",
    )
    .unwrap();
    let bootstrap = DesktopBootstrap::production().unwrap();
    let session = bootstrap
        .project_filesystem
        .open(&RequestedProjectPath::new(project.root.as_path()))
        .unwrap();
    let production = session
        .as_any()
        .downcast_ref::<ProductionProjectSession>()
        .unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        if !matches!(
            production.search().rebuild_status(),
            SearchRebuildStatus::Running { .. }
        ) {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "summary hydration stalled"
        );
        std::thread::yield_now();
    }

    let snapshot = production.ui_snapshot().unwrap();
    assert_eq!(snapshot.document_summaries.len(), 2);
    assert_eq!(
        snapshot.documents.len(),
        1,
        "background hydration must not open editor sessions"
    );
    assert!(
        snapshot
            .document_summaries
            .iter()
            .all(|summary| matches!(summary.word_count, DocumentWordCount::Known(_)))
    );
}

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
        vec![CanonicalComment {
            id: CommentId::from_bytes([21; 16]),
            messages: vec![CanonicalCommentMessage {
                id: CommentId::from_bytes([22; 16]),
                body: "Durable note".into(),
                unknown_fields: BTreeMap::from([(
                    "future_author".into(),
                    AnnotationValue::String("preserved".into()),
                )]),
            }],
            resolved: false,
            anchor: CanonicalCommentAnchor::Text {
                block: BlockId::from_bytes(*document.as_bytes()),
                range: EditorSelection::new(DocumentPosition::from(0), DocumentPosition::from(6)),
                quote: "second".into(),
                context_before: String::new(),
                context_after: " edit".into(),
                orphaned: false,
                unknown_fields: BTreeMap::new(),
            },
            unknown_fields: BTreeMap::new(),
        }],
        vec![],
        2,
    );
    access
        .persistence(|persistence| persistence.persist_editor_projection(second))
        .unwrap()
        .expect("the editor must remain writable after a recovery failure");
    let current = access
        .snapshot(|query| query.snapshot())
        .unwrap()
        .expect("the persisted comment should be queryable before editing");
    let mut edited_comments = current
        .documents
        .iter()
        .find(|snapshot| snapshot.document_id == document)
        .unwrap()
        .comments
        .clone();
    edited_comments[0].messages[0].body = "Edited durable note".into();
    access
        .persistence(|persistence| {
            persistence.persist_editor_projection(CanonicalProjection::new(
                document,
                EditorRevision::from(3),
                "<p>second edit</p>",
                edited_comments,
                vec![],
                2,
            ))
        })
        .unwrap()
        .expect("a message edit should participate in normal durable projection");
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
    assert_eq!(saved.written.documents[&document], EditorRevision::from(3));

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
    assert_eq!(reopened_document.revision, EditorRevision::from(3));
    assert_eq!(reopened_document.comments.len(), 1);
    assert_eq!(
        reopened_document.comments[0].messages[0].body,
        "Edited durable note"
    );
    assert_eq!(
        reopened_document.comments[0].messages[0].unknown_fields["future_author"],
        AnnotationValue::String("preserved".into())
    );
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
    let mut excluded =
        access
            .commands_service()
            .unwrap()
            .execute(ProjectCommand::set_node_export_settings(
                NodeId::from_bytes([90; 16]),
                ProjectExportSettings {
                    excluded: true,
                    ..ProjectExportSettings::default()
                },
            ));
    poll_ready(excluded.as_mut()).expect("legacy per-node exclusion should remain a valid edit");
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
            let stale = export.begin_export(Arc::new(IgnoreExportProgress))?;
            let operation = export.begin_export(Arc::new(IgnoreExportProgress))?;
            assert!(
                export
                    .export_to_path(
                        stale,
                        UntrustedPathSelection::new(&output),
                        ExportRunOptions::default(),
                    )
                    .is_err(),
                "replaced operation tokens must be rejected"
            );
            export.export_to_path(
                operation,
                UntrustedPathSelection::new(&output),
                ExportRunOptions::default(),
            )
        })
        .unwrap()
        .expect("production export");
    let ExportOutcome::Completed(artifact) = artifact else {
        panic!("production export was unexpectedly cancelled")
    };
    let html = fs::read_to_string(&output).expect("completed export is visible");
    assert!(html.contains("Created Chapter"));
    assert!(html.contains("<p></p>"));
    assert_eq!(artifact.display_name, "exported-project.html");
    let second_directory = project.root.as_path().join("alternate-export");
    fs::create_dir(&second_directory).expect("second export directory");
    let second_output = second_directory.join("exported-project.html");
    let second = access
        .export_target(|export| {
            let operation = export.begin_export(Arc::new(IgnoreExportProgress))?;
            export.export_to_path(
                operation,
                UntrustedPathSelection::new(&second_output),
                ExportRunOptions::default(),
            )
        })
        .unwrap()
        .expect("second production export");
    let ExportOutcome::Completed(second) = second else {
        panic!("second production export was unexpectedly cancelled")
    };
    assert_eq!(second.display_name, artifact.display_name);
    assert_ne!(second.token, artifact.token);
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
fn lazy_multi_document_export_materializes_every_manuscript_body() {
    let project = ScopedProject::from_fixture("canonical/minimal-project").unwrap();
    fs::write(
        project.root.as_path().join("manuscript/chapter-2.html"),
        b"<p>second lazy export body</p>",
    )
    .unwrap();
    let bootstrap = DesktopBootstrap::production().unwrap();
    let runtime = block_on(bootstrap.start(LaunchRequest::launcher())).unwrap();
    let OpenProjectResult::Opened { session, .. } = runtime
        .open_project(project.root.as_path())
        .expect("lazy project opens")
    else {
        panic!("project should open")
    };
    let project_ui = runtime.project_ui(session).unwrap().unwrap();
    assert_eq!(project_ui.snapshot.document_summaries.len(), 2);
    assert_eq!(
        project_ui.snapshot.documents.len(),
        1,
        "project open must retain only the initial body"
    );
    let access = project_ui.ports.access().unwrap();
    let output = project.root.as_path().join("lazy-export.html");
    let outcome = access
        .export_target(|export| {
            let operation = export.begin_export(Arc::new(IgnoreExportProgress))?;
            export.export_to_path(
                operation,
                UntrustedPathSelection::new(&output),
                ExportRunOptions::default(),
            )
        })
        .unwrap()
        .unwrap();
    assert!(matches!(outcome, ExportOutcome::Completed(_)));
    let html = fs::read_to_string(output).unwrap();
    assert!(html.contains("hello"));
    assert!(html.contains("second lazy export body"));
}

#[test]
fn live_search_reconciles_body_title_synopsis_metadata_create_delete_save_and_reopen() {
    let project = ScopedProject::from_fixture("canonical/minimal-project")
        .expect("canonical project fixture should exist");
    let bootstrap = DesktopBootstrap::production().expect("production graph should assemble");
    let requested = RequestedProjectPath::new(project.root.as_path());
    let session = bootstrap
        .project_filesystem
        .open(&requested)
        .expect("production project should open");
    let production = session
        .as_any()
        .downcast_ref::<ProductionProjectSession>()
        .expect("typed production project");
    let snapshot = production.ui_snapshot().expect("initial project snapshot");
    let document = snapshot.documents[0].document_id;
    let node = snapshot
        .project
        .nodes
        .iter()
        .find_map(|(id, node)| (node.kind == NodeKind::Document(document)).then_some(*id))
        .expect("initial document node");
    let search = production.search();
    let commands = production.commands();

    let initial = search_hits(search.as_ref(), "hello", SearchField::Body);
    assert_eq!(initial.len(), 1);
    assert_eq!(
        initial[0].indexed_revision.value(),
        snapshot.documents[0].revision.value(),
        "body hits must identify the canonical document revision"
    );

    production
        .project_persistence()
        .persist_editor_projection(CanonicalProjection::new(
            document,
            EditorRevision::from(1),
            "<p>fresh body phrase</p>",
            Vec::new(),
            Vec::new(),
            3,
        ))
        .expect("body edit should persist to recovery");
    assert!(search_hits(search.as_ref(), "hello", SearchField::Body).is_empty());
    let body_hit = search_hits(search.as_ref(), "fresh body phrase", SearchField::Body);
    assert_eq!(body_hit.len(), 1);
    let edited_revision = production
        .ui_snapshot()
        .expect("edited project snapshot")
        .documents[0]
        .revision
        .value();
    assert_eq!(body_hit[0].indexed_revision.value(), edited_revision);

    for command in [
        ProjectCommand::rename_node(node, "Revised Search Title"),
        ProjectCommand::set_synopsis(node, "Distinct synopsis phrase"),
    ] {
        let mut command = commands.execute(command);
        poll_ready(command.as_mut()).expect("searchable project edit should succeed");
    }
    let title_hit = search_hits(
        search.as_ref(),
        "Revised Search Title",
        SearchField::DisplayTitle,
    );
    let synopsis_hit = search_hits(
        search.as_ref(),
        "Distinct synopsis phrase",
        SearchField::Synopsis,
    );
    assert_eq!(title_hit.len(), 1);
    assert_eq!(synopsis_hit.len(), 1);
    assert_eq!(title_hit[0].indexed_revision.value(), edited_revision);
    assert_eq!(synopsis_hit[0].indexed_revision.value(), edited_revision);

    let metadata = MetadataFieldId::from_bytes([73; 16]);
    for command in [
        ProjectCommand::upsert_metadata_field(MetadataFieldDefinition {
            id: metadata,
            label: "Status".into(),
            description: None,
            applicability: MetadataApplicability::Documents,
            text_kind: MetadataTextKind::SingleLine,
            default_value: None,
            visible_on_cards: true,
        }),
        ProjectCommand::set_metadata_value(node, metadata, Some("Metadata search phrase".into())),
    ] {
        let mut command = commands.execute(command);
        poll_ready(command.as_mut()).expect("metadata edit should succeed");
    }
    let metadata_hit = search_hits(
        search.as_ref(),
        "Metadata search phrase",
        SearchField::Metadata(metadata),
    );
    assert_eq!(metadata_hit.len(), 1);
    assert_eq!(metadata_hit[0].indexed_revision.value(), edited_revision);

    let created_node = NodeId::from_bytes([74; 16]);
    let created_document = DocumentId::from_bytes([75; 16]);
    let mut create = commands.execute(ProjectCommand::create_document(
        created_node,
        created_document,
        NodeId::manuscript_root(),
        snapshot
            .project
            .nodes
            .children(NodeId::manuscript_root())
            .len(),
        "Transient searchable chapter",
    ));
    poll_ready(create.as_mut()).expect("document creation should succeed");
    assert_eq!(
        search_hits(
            search.as_ref(),
            "Transient searchable chapter",
            SearchField::DisplayTitle,
        )
        .len(),
        1
    );
    let mut delete = commands.execute(ProjectCommand::delete_node(created_node));
    poll_ready(delete.as_mut()).expect("document deletion should succeed");
    assert!(
        search_hits(
            search.as_ref(),
            "Transient searchable chapter",
            SearchField::DisplayTitle,
        )
        .is_empty(),
        "deleted documents must not leave stale indexed text"
    );

    bootstrap
        .project_filesystem
        .begin_final_save(session.as_ref())
        .expect("final save should persist the searchable state");
    drop(session);

    let reopened = bootstrap
        .project_filesystem
        .open(&requested)
        .expect("saved project should reopen");
    let reopened = reopened
        .as_any()
        .downcast_ref::<ProductionProjectSession>()
        .expect("typed reopened project");
    let reopened_body = search_hits(
        reopened.search().as_ref(),
        "fresh body phrase",
        SearchField::Body,
    );
    assert_eq!(reopened_body.len(), 1);
    assert_eq!(
        reopened_body[0].indexed_revision.value(),
        reopened
            .ui_snapshot()
            .expect("reopened project snapshot")
            .documents[0]
            .revision
            .value()
    );
    assert!(
        search_hits(
            reopened.search().as_ref(),
            "Transient searchable chapter",
            SearchField::DisplayTitle,
        )
        .is_empty()
    );
}

#[test]
fn corrupt_embedded_history_does_not_prevent_open_and_reports_typed_recovery_availability() {
    let project = ScopedProject::from_fixture("canonical/minimal-project")
        .expect("canonical project fixture should exist");
    let git = project.root.as_path().join(".git");
    fs::create_dir(&git).expect("corrupt embedded History directory");
    fs::write(git.join("HEAD"), b"not a valid embedded Git repository\n")
        .expect("corrupt embedded History marker");

    let bootstrap = DesktopBootstrap::production().expect("production graph should assemble");
    let session = bootstrap
        .project_filesystem
        .open(&RequestedProjectPath::new(project.root.as_path()))
        .expect("canonical project data must open despite corrupt History");
    let production = session
        .as_any()
        .downcast_ref::<ProductionProjectSession>()
        .expect("typed production project");
    let ProductionHistoryStatus::Unavailable {
        problem,
        reinitialize,
    } = production.history_status()
    else {
        panic!("corrupt History should be reported as unavailable")
    };
    assert!(problem.contains("corrupt"));
    assert!(matches!(
        reinitialize,
        parchmint_history_api::HistoryReinitializeAvailability::Blocked { .. }
    ));
    assert_eq!(production.ui_snapshot().unwrap().documents.len(), 1);
    assert!(
        production
            .history()
            .list(parchmint_history_api::HistoryPageQuery::newest_first(25))
            .is_err()
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
        project_id: production
            .ui_snapshot()
            .expect("project snapshot")
            .project
            .id,
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

#[derive(Clone, Default)]
struct SearchHits(Arc<Mutex<Vec<SearchHit>>>);

impl SearchBatchSink for SearchHits {
    fn push(&self, batch: SearchBatch) {
        self.0
            .lock()
            .expect("search hit collector lock")
            .extend(batch.hits);
    }
}

fn search_hits(index: &dyn SearchIndex, text: &str, field: SearchField) -> Vec<SearchHit> {
    let sink = SearchHits::default();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        let result = index.query(
            SearchQuery {
                text: text.into(),
                fields: BTreeSet::from([field]),
                case_sensitive: false,
                whole_word: false,
                generation: 1,
            },
            Box::new(sink.clone()),
        );
        match result {
            Ok(()) => break,
            Err(parchmint_search_api::SearchError::Rebuilding { .. }) => {
                assert!(
                    std::time::Instant::now() < deadline,
                    "production search rebuild did not settle"
                );
                std::thread::yield_now();
            }
            Err(error) => panic!("production search query: {error}"),
        }
    }
    sink.0.lock().expect("search hit collector lock").clone()
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
