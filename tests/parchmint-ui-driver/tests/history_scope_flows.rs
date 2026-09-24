use parchmint_desktop::{
    DesktopInteractionHarness, EditorPane, HarnessTarget, HarnessWindow, LaunchRequest,
    RibbonDestination,
};
use parchmint_ui_driver::{IsolatedRun, create_document, create_project};

fn route(harness: &DesktopInteractionHarness, destination: RibbonDestination) {
    harness
        .click_target(HarnessWindow::Project, HarnessTarget::Ribbon(destination))
        .unwrap();
}

fn capture(harness: &DesktopInteractionHarness, name: &str) {
    if let Some(root) = std::env::var_os("PARCHMINT_REVIEW_ARTIFACTS") {
        harness
            .snapshot(
                HarnessWindow::Project,
                std::path::PathBuf::from(root).join(name),
            )
            .unwrap();
    }
}

fn open_document_history(harness: &DesktopInteractionHarness, title: &str) {
    route(harness, RibbonDestination::Editor);
    harness
        .right_click_text(HarnessWindow::Project, title)
        .unwrap();
    harness
        .click_text(HarnessWindow::Project, "History")
        .unwrap();
}

#[test]
fn explicit_document_history_restores_only_that_document_and_preserves_other_pane_undo() {
    let run = IsolatedRun::new("document-history").unwrap();
    let project = run.root().join("novel.parchmint");
    let harness = create_project(&run, &project, "Document History");
    harness
        .resize(HarnessWindow::Project, 1280.0, 720.0)
        .unwrap();
    create_document(&harness, "Manuscript", "Chapter");
    harness
        .type_into_target(
            HarnessWindow::Project,
            HarnessTarget::EditorPrimary,
            "The original chapter.",
        )
        .unwrap();
    let chapter = harness
        .active_editor_document_id(EditorPane::Primary)
        .unwrap();
    create_document(&harness, "Research", "Notes");
    let note = harness.hierarchy_node("Notes").unwrap();
    harness
        .drag_hierarchy_node_to_pane(HarnessWindow::Project, note, EditorPane::Companion)
        .unwrap();
    harness
        .type_into_target(
            HarnessWindow::Project,
            HarnessTarget::EditorCompanion,
            "The original notes.",
        )
        .unwrap();
    let notes = harness
        .active_editor_document_id(EditorPane::Companion)
        .unwrap();
    harness
        .click_target(HarnessWindow::Project, HarnessTarget::EditorPrimary)
        .unwrap();
    harness
        .click_text(HarnessWindow::Project, "Chapter")
        .unwrap();
    assert_eq!(
        harness
            .active_editor_document_id(EditorPane::Primary)
            .unwrap(),
        chapter
    );
    harness.elapse_autosave_idle().unwrap();
    route(&harness, RibbonDestination::History);
    harness
        .type_into(HarnessWindow::Project, "Milestone name", "Before revision")
        .unwrap();
    harness
        .click_text(HarnessWindow::Project, "Create milestone")
        .unwrap();
    let milestone = harness
        .history_checkpoints()
        .unwrap()
        .into_iter()
        .find(|checkpoint| checkpoint.label == "Before revision")
        .unwrap();

    route(&harness, RibbonDestination::Editor);
    harness
        .type_into_target(
            HarnessWindow::Project,
            HarnessTarget::EditorPrimary,
            " New chapter writing.",
        )
        .unwrap();
    harness
        .type_into_target(
            HarnessWindow::Project,
            HarnessTarget::EditorCompanion,
            " Unrelated notes must stay.",
        )
        .unwrap();
    open_document_history(&harness, "Chapter");
    harness
        .click_history_checkpoint_by_id(HarnessWindow::Project, milestone.id.clone())
        .unwrap();
    capture(&harness, "history-document-light");
    assert!(
        harness
            .contains_text(HarnessWindow::Project, "Content")
            .unwrap()
    );
    assert!(
        !harness
            .contains_text(HarnessWindow::Project, "Unrelated notes must stay.")
            .unwrap()
    );
    assert!(
        !harness
            .contains_text(HarnessWindow::Project, "Restore project…")
            .unwrap()
    );
    harness
        .click_text(HarnessWindow::Project, "Restore document…")
        .unwrap();
    capture(&harness, "history-document-confirmation");
    assert!(harness.contains_text(HarnessWindow::Project,
        "Restore “Manuscript > Chapter” to “Before revision”? Only its text, formatting, and comments change.").unwrap());
    harness
        .click_text(HarnessWindow::Project, "Restore document")
        .unwrap();
    let saved_notes =
        std::fs::read_to_string(project.join(format!("research/{notes}.html"))).unwrap();
    assert!(
        saved_notes.contains("Unrelated notes must stay."),
        "restoration must save the other pane's latest edits before reporting success"
    );
    assert!(
        harness
            .history_checkpoints()
            .unwrap()
            .iter()
            .any(|checkpoint| checkpoint.category == "Restoration")
    );

    route(&harness, RibbonDestination::Editor);
    assert_eq!(
        harness
            .active_editor_document_id(EditorPane::Primary)
            .unwrap(),
        chapter
    );
    assert_eq!(
        harness
            .active_editor_document_id(EditorPane::Companion)
            .unwrap(),
        notes
    );
    harness
        .click_target(HarnessWindow::Project, HarnessTarget::EditorPrimary)
        .unwrap();
    let body = harness.active_editor_body().unwrap();
    assert!(body.contains("The original chapter."));
    assert!(!body.contains("New chapter writing."));
    harness
        .click_target(HarnessWindow::Project, HarnessTarget::EditorCompanion)
        .unwrap();
    assert!(
        harness
            .active_editor_body()
            .unwrap()
            .contains("Unrelated notes must stay.")
    );
    harness
        .press_command_key(HarnessWindow::Project, 'z')
        .unwrap();
    let body = harness.active_editor_body().unwrap();
    assert!(body.contains("The original notes."));
    assert!(
        !body.contains("Unrelated notes must stay."),
        "other pane must retain its undo stack"
    );
    harness.elapse_autosave_idle().unwrap();
    route(&harness, RibbonDestination::Settings);
    harness.click_text(HarnessWindow::Project, "Dark").unwrap();
    open_document_history(&harness, "Chapter");
    assert!(
        !harness
            .contains_text(HarnessWindow::Project, "Milestone name")
            .unwrap()
    );
    capture(&harness, "history-document-dark");
    route(&harness, RibbonDestination::History);
    assert!(
        harness
            .contains_text(HarnessWindow::Project, "Create milestone")
            .unwrap()
    );
    harness.close(HarnessWindow::Project).unwrap();
    harness.shutdown().unwrap();

    let reopened =
        DesktopInteractionHarness::launch(run.root(), LaunchRequest::open(&project)).unwrap();
    reopened
        .click_text(HarnessWindow::Project, "Chapter")
        .unwrap();
    assert!(
        reopened
            .active_editor_body()
            .unwrap()
            .contains("The original chapter.")
    );
    assert!(
        !reopened
            .active_editor_body()
            .unwrap()
            .contains("New chapter writing.")
    );
    reopened
        .click_text(HarnessWindow::Project, "Notes")
        .unwrap();
    assert!(
        reopened
            .active_editor_body()
            .unwrap()
            .contains("The original notes.")
    );
    reopened.close(HarnessWindow::Project).unwrap();
    reopened.shutdown().unwrap();
}
