use std::{
    future::Future,
    sync::Arc,
    task::{Context, Poll, Waker},
};

use super::*;

fn wait<T>(future: impl Future<Output = T>) -> T {
    let mut context = Context::from_waker(Waker::noop());
    let mut future = Box::pin(future);
    loop {
        match future.as_mut().poll(&mut context) {
            Poll::Ready(value) => return value,
            Poll::Pending => std::thread::yield_now(),
        }
    }
}

fn stable_id(value: u8) -> [u8; 16] {
    [value; 16]
}

fn project_id() -> ProjectId {
    ProjectId::from_bytes(stable_id(1))
}

fn group_id() -> parchmint_domain::NodeId {
    parchmint_domain::NodeId::from_bytes(stable_id(2))
}

fn open_document() -> DocumentId {
    DocumentId::from_bytes(stable_id(3))
}

fn closed_document() -> DocumentId {
    DocumentId::from_bytes(stable_id(4))
}

fn sample_project() -> Project {
    let project = Project::new(project_id());
    parchmint_domain::apply_project_command(
        &project,
        project.revision,
        ProjectCommand::create_group(
            group_id(),
            parchmint_domain::NodeId::manuscript_root(),
            0,
            "Draft",
        ),
    )
    .expect("sample group is valid")
    .project
}

fn sample_documents() -> Arc<NativeDocumentStateOwner> {
    Arc::new(NativeDocumentStateOwner::new([
        DocumentSnapshot {
            document_id: open_document(),
            body: "alpha needle".into(),
            revision: EditorRevision::from(0),
            visibility: DocumentVisibility::Open,
        },
        DocumentSnapshot {
            document_id: closed_document(),
            body: "closed needle".into(),
            revision: EditorRevision::from(0),
            visibility: DocumentVisibility::Closed,
        },
    ]))
}

fn setup() -> (
    NativeProjectCommandDispatcher,
    Arc<NativeDocumentStateOwner>,
) {
    let documents = sample_documents();
    let dispatcher = NativeProjectCommandDispatcher::new(sample_project(), documents.clone());
    (dispatcher, documents)
}

fn replacement() -> ReplacementSelection {
    ReplacementSelection {
        label: "Replace All".into(),
        edits: vec![
            ReplacementEdit {
                document_id: open_document(),
                observed_revision: EditorRevision::from(0),
                expected_body: "alpha needle".into(),
                replacement_body: "alpha replaced".into(),
            },
            ReplacementEdit {
                document_id: closed_document(),
                observed_revision: EditorRevision::from(0),
                expected_body: "closed needle".into(),
                replacement_body: "closed replaced".into(),
            },
        ],
    }
}

#[test]
fn focus_routes_to_exactly_one_undo_owner() {
    let document = open_document();
    assert_eq!(
        FocusTarget::Editor(document).undo_domain(),
        UndoDomain::Document(document)
    );
    assert_eq!(
        FocusTarget::Comment(document).undo_domain(),
        UndoDomain::Document(document)
    );
    for focus in [
        FocusTarget::Tree,
        FocusTarget::Cards,
        FocusTarget::Settings,
        FocusTarget::Inspector,
    ] {
        assert_eq!(focus.undo_domain(), UndoDomain::Project);
    }
    assert_eq!(FocusTarget::TextInput.undo_domain(), UndoDomain::TextInput);
}

#[test]
fn project_commands_undo_redo_with_new_revisions_checkpoints_and_redo_invalidation() {
    let (dispatcher, documents) = setup();
    let initial = dispatcher.project().unwrap().revision;
    let execute = wait(dispatcher.execute(ProjectCommand::rename_node(group_id(), "Final")))
        .expect("rename succeeds");
    assert_eq!(dispatcher.project_undo_entries().unwrap().len(), 1);
    assert_eq!(documents.document_undo_len(open_document()).unwrap(), 0);

    let undo = wait(dispatcher.undo()).expect("undo succeeds");
    assert!(dispatcher.undo_state().can_redo);
    let redo = wait(dispatcher.redo()).expect("redo succeeds");

    assert_eq!(execute.revision, initial.next());
    assert_eq!(undo.revision, execute.revision.next());
    assert_eq!(redo.revision, undo.revision.next());
    assert_eq!(dispatcher.pending_checkpoints().unwrap().len(), 3);
    assert_eq!(
        dispatcher
            .project()
            .unwrap()
            .nodes
            .get(group_id())
            .unwrap()
            .title,
        "Final"
    );

    wait(dispatcher.undo()).unwrap();
    wait(dispatcher.execute(ProjectCommand::rename_node(group_id(), "Published"))).unwrap();
    assert!(!dispatcher.undo_state().can_redo);
    assert_eq!(dispatcher.project_undo_entries().unwrap().len(), 1);
}

#[test]
fn unopened_document_commands_create_a_hidden_session_and_use_document_undo() {
    let (dispatcher, documents) = setup();
    let result = dispatcher
        .execute_document(DocumentCommand {
            document_id: closed_document(),
            observed_revision: EditorRevision::from(0),
            body: "edited while unopened".into(),
        })
        .unwrap();
    assert!(result.opened_session);
    assert_eq!(documents.document_undo_len(closed_document()).unwrap(), 1);
    assert_eq!(
        documents.snapshot(closed_document()).unwrap().visibility,
        DocumentVisibility::Hidden
    );
    let save = dispatcher.capture_save_request().unwrap();
    assert_eq!(save.open_documents[&closed_document()].value(), 1);
    assert!(!save.closed_documents.contains_key(&closed_document()));

    let undo = dispatcher
        .undo_focused(FocusTarget::Editor(closed_document()))
        .unwrap();
    assert!(matches!(undo, FocusedUndoResult::Document { .. }));
    assert_eq!(
        documents.snapshot(closed_document()).unwrap().body,
        "closed needle"
    );
    dispatcher
        .redo_focused(FocusTarget::Comment(closed_document()))
        .unwrap();
    assert_eq!(
        documents.snapshot(closed_document()).unwrap().body,
        "edited while unopened"
    );
    assert_eq!(
        dispatcher.undo_focused(FocusTarget::TextInput).unwrap(),
        FocusedUndoResult::NativeTextInput
    );
}

#[test]
fn save_requests_capture_open_and_closed_revisions_at_one_boundary() {
    let (dispatcher, documents) = setup();
    wait(GlobalReplacement::apply(
        &dispatcher,
        ReplacementSelection {
            label: "Closed replacement".into(),
            edits: vec![ReplacementEdit {
                document_id: closed_document(),
                observed_revision: EditorRevision::from(0),
                expected_body: "closed needle".into(),
                replacement_body: "closed changed".into(),
            }],
        },
    ))
    .unwrap();
    dispatcher
        .execute_document(DocumentCommand {
            document_id: open_document(),
            observed_revision: EditorRevision::from(0),
            body: "open changed".into(),
        })
        .unwrap();

    let captured = dispatcher.capture_save_request().unwrap();
    dispatcher
        .execute_document(DocumentCommand {
            document_id: open_document(),
            observed_revision: EditorRevision::from(1),
            body: "open changed again".into(),
        })
        .unwrap();
    let current = dispatcher.capture_save_request().unwrap();

    assert_eq!(captured.generation, 1);
    assert_eq!(captured.open_documents[&open_document()].value(), 1);
    assert_eq!(captured.closed_documents[&closed_document()].value(), 1);
    assert_eq!(current.generation, 2);
    assert_eq!(current.open_documents[&open_document()].value(), 2);
    assert_eq!(
        documents
            .snapshot(open_document())
            .unwrap()
            .revision
            .value(),
        2
    );
    assert_eq!(captured.checkpoint_groups.len(), 2);
}

#[test]
fn global_replacement_has_one_inverse_project_undo_and_checkpoint() {
    let (dispatcher, documents) = setup();
    let initial = dispatcher.project().unwrap().revision;
    let preview = wait(GlobalReplacement::preview(&dispatcher, replacement())).unwrap();
    let result = wait(GlobalReplacement::apply(&dispatcher, replacement())).unwrap();
    let entries = dispatcher.project_undo_entries().unwrap();

    assert_eq!(preview.affected_documents.len(), 2);
    assert_eq!(entries.len(), 1);
    assert!(matches!(
        entries[0].forward,
        ProjectPatch::Documents {
            direction: PatchDirection::Forward,
            ..
        }
    ));
    assert!(matches!(
        entries[0].inverse,
        ProjectPatch::Documents {
            direction: PatchDirection::Inverse,
            ..
        }
    ));
    assert_eq!(entries[0].checkpoint_group, result.checkpoint_group);
    assert_eq!(
        dispatcher.pending_checkpoints().unwrap(),
        vec![result.checkpoint_group]
    );
    assert_eq!(documents.document_undo_len(open_document()).unwrap(), 0);
    assert_eq!(documents.document_undo_len(closed_document()).unwrap(), 0);
    assert_eq!(
        documents.project_boundary_count(open_document()).unwrap(),
        1
    );
    assert_eq!(
        documents.project_boundary_count(closed_document()).unwrap(),
        1
    );

    assert_eq!(result.revision, initial.next());
    let undo = wait(dispatcher.undo()).unwrap();
    assert_eq!(undo.revision, result.revision.next());
    assert_eq!(
        documents.snapshot(open_document()).unwrap().body,
        "alpha needle"
    );
    assert_eq!(
        documents.snapshot(closed_document()).unwrap().body,
        "closed needle"
    );
    let redo = wait(dispatcher.redo()).unwrap();
    assert_eq!(redo.revision, undo.revision.next());
    assert_eq!(
        documents.snapshot(open_document()).unwrap().body,
        "alpha replaced"
    );
    assert_eq!(
        documents.snapshot(closed_document()).unwrap().body,
        "closed replaced"
    );
    assert_eq!(
        documents.project_boundary_count(open_document()).unwrap(),
        3
    );
    assert_eq!(dispatcher.pending_checkpoints().unwrap().len(), 3);
}

#[test]
fn composite_apply_failure_rolls_back_open_and_closed_documents_before_publish() {
    let (dispatcher, documents) = setup();
    let project_before = dispatcher.project().unwrap();
    let open_before = documents.snapshot(open_document()).unwrap();
    let closed_before = documents.snapshot(closed_document()).unwrap();
    documents.fail_next_composite_at(closed_document());

    let result = wait(GlobalReplacement::apply(&dispatcher, replacement()));

    assert!(matches!(
        result,
        Err(ApplicationError::CompositeApplyFailed { document })
            if document == closed_document()
    ));
    assert_eq!(documents.snapshot(open_document()).unwrap(), open_before);
    assert_eq!(
        documents.snapshot(closed_document()).unwrap(),
        closed_before
    );
    assert_eq!(dispatcher.project().unwrap(), project_before);
    assert!(dispatcher.project_undo_entries().unwrap().is_empty());
    assert!(dispatcher.pending_checkpoints().unwrap().is_empty());
    assert_eq!(
        documents.project_boundary_count(open_document()).unwrap(),
        0
    );
    assert_eq!(
        documents.project_boundary_count(closed_document()).unwrap(),
        0
    );
}

#[test]
fn composite_validation_prepares_every_inverse_before_any_publication() {
    let (dispatcher, documents) = setup();
    let project_before = dispatcher.project().unwrap();
    let open_before = documents.snapshot(open_document()).unwrap();
    let selection = ReplacementSelection {
        label: "Invalid replacement".into(),
        edits: vec![
            replacement().edits[0].clone(),
            ReplacementEdit {
                document_id: DocumentId::from_bytes(stable_id(99)),
                observed_revision: EditorRevision::from(0),
                expected_body: "missing".into(),
                replacement_body: "invalid".into(),
            },
        ],
    };

    assert!(matches!(
        wait(GlobalReplacement::apply(&dispatcher, selection)),
        Err(ApplicationError::MissingDocument { .. })
    ));
    let duplicate = replacement().edits[0].clone();
    assert!(matches!(
        wait(GlobalReplacement::apply(
            &dispatcher,
            ReplacementSelection {
                label: "Duplicate replacement".into(),
                edits: vec![duplicate.clone(), duplicate],
            }
        )),
        Err(ApplicationError::DuplicateDocument { document }) if document == open_document()
    ));
    assert_eq!(documents.snapshot(open_document()).unwrap(), open_before);
    assert_eq!(dispatcher.project().unwrap(), project_before);
    assert!(dispatcher.project_undo_entries().unwrap().is_empty());
    assert!(dispatcher.pending_checkpoints().unwrap().is_empty());
    assert_eq!(
        documents.project_boundary_count(open_document()).unwrap(),
        0
    );
}

#[test]
fn recovery_migration_restore_and_close_reset_both_undo_owners() {
    for reason in [
        UndoResetReason::RecoveryAccepted,
        UndoResetReason::MigrationCompleted,
        UndoResetReason::HistoryRestored,
        UndoResetReason::ProjectClosed,
    ] {
        let (dispatcher, documents) = setup();
        wait(dispatcher.execute(ProjectCommand::rename_node(group_id(), "Changed"))).unwrap();
        dispatcher
            .execute_document(DocumentCommand {
                document_id: open_document(),
                observed_revision: EditorRevision::from(0),
                body: "changed".into(),
            })
            .unwrap();
        wait(dispatcher.undo()).unwrap();
        dispatcher
            .undo_focused(FocusTarget::Editor(open_document()))
            .unwrap();

        dispatcher.reset_undo(reason);

        assert!(!dispatcher.undo_state().can_undo, "{reason:?}");
        assert!(!dispatcher.undo_state().can_redo, "{reason:?}");
        assert_eq!(documents.document_undo_len(open_document()).unwrap(), 0);
        assert!(matches!(
            dispatcher.undo_focused(FocusTarget::Editor(open_document())),
            Err(ApplicationError::DocumentUndoEmpty { .. })
        ));
        assert!(matches!(
            dispatcher.redo_focused(FocusTarget::Editor(open_document())),
            Err(ApplicationError::DocumentRedoEmpty { .. })
        ));
    }
}
