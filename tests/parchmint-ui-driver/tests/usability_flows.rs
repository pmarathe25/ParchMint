use parchmint_desktop::{
    DesktopInteractionHarness, HarnessTarget, HarnessWindow, RibbonDestination,
};
use parchmint_ui_driver::{IsolatedRun, create_document, create_group, create_project};

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

fn route(harness: &DesktopInteractionHarness, destination: RibbonDestination) {
    harness
        .click_target(HarnessWindow::Project, HarnessTarget::Ribbon(destination))
        .unwrap();
}

#[test]
fn creation_has_one_primary_entry_and_banners_leave_controls_usable() {
    let run = IsolatedRun::new("creation-usability").unwrap();
    let project = run.root().join("novel.parchmint");
    let harness = create_project(&run, &project, "Creation usability");
    assert!(
        harness
            .text_is_visible(HarnessWindow::Project, "+ New")
            .unwrap()
    );
    assert!(
        !harness
            .contains_text(HarnessWindow::Project, "New document")
            .unwrap()
    );
    assert!(
        !harness
            .contains_text(HarnessWindow::Project, "New group")
            .unwrap()
    );
    harness.click_text(HarnessWindow::Project, "+ New").unwrap();
    assert!(
        harness
            .text_is_visible(HarnessWindow::Project, "Add to Manuscript")
            .unwrap()
    );
    capture(&harness, "creation-menu");
    harness.click_text(HarnessWindow::Project, "Group").unwrap();
    harness
        .replace_text_and_submit(HarnessWindow::Project, "New Group", "Drafts")
        .unwrap();
    assert!(
        harness
            .text_is_visible(HarnessWindow::Project, "Created item")
            .unwrap()
    );
    capture(&harness, "creation-banner");
    harness.click_text(HarnessWindow::Project, "+ New").unwrap();
    assert!(
        harness
            .text_is_visible(HarnessWindow::Project, "Add to Drafts")
            .unwrap()
    );
    harness
        .click_text(HarnessWindow::Project, "Document")
        .unwrap();
    harness
        .replace_text_and_submit(HarnessWindow::Project, "Untitled", "Opening")
        .unwrap();
    harness
        .click_text(HarnessWindow::Project, "Dismiss")
        .unwrap();
    harness.elapse_notifications().unwrap();
    assert!(
        !harness
            .text_is_visible(HarnessWindow::Project, "Created item")
            .unwrap()
    );
    harness
        .type_into_target(
            HarnessWindow::Project,
            HarnessTarget::EditorPrimary,
            "Writing remains accessible.",
        )
        .unwrap();
    assert!(
        harness
            .active_editor_body()
            .unwrap()
            .contains("Writing remains accessible.")
    );
    harness
        .resize(HarnessWindow::Project, 1440.0, 900.0)
        .unwrap();
    harness
        .type_focused(HarnessWindow::Project, " After resizing.")
        .unwrap();
    assert!(
        harness
            .active_editor_body()
            .unwrap()
            .contains("Writing remains accessible. After resizing.")
    );
    create_group(&harness, "Manuscript", "Later");
    assert!(
        harness
            .text_is_visible(HarnessWindow::Project, "Created item")
            .unwrap()
    );
    harness.elapse_notifications().unwrap();
    assert!(
        !harness
            .text_is_visible(HarnessWindow::Project, "Created item")
            .unwrap()
    );
    harness
        .resize(HarnessWindow::Project, 1280.0, 720.0)
        .unwrap();
    capture(&harness, "creation-expired");
    route(&harness, RibbonDestination::Settings);
    harness.click_text(HarnessWindow::Project, "Dark").unwrap();
    route(&harness, RibbonDestination::Editor);
    harness.click_text(HarnessWindow::Project, "+ New").unwrap();
    assert!(
        harness
            .text_is_visible(HarnessWindow::Project, "Document")
            .unwrap()
    );
    capture(&harness, "creation-menu-dark-compact");
    harness
        .press_key(
            HarnessWindow::Project,
            parchmint_desktop::HarnessKey::Escape,
        )
        .unwrap();
    harness.close(HarnessWindow::Project).unwrap();
    harness.shutdown().unwrap();
}

#[test]
fn history_compares_the_project_including_added_deleted_and_unsaved_documents() {
    let run = IsolatedRun::new("history-usability").unwrap();
    let project = run.root().join("novel.parchmint");
    let harness = create_project(&run, &project, "History usability");
    harness
        .type_into_target(
            HarnessWindow::Project,
            HarnessTarget::EditorPrimary,
            "The lantern was blue.",
        )
        .unwrap();
    harness.elapse_autosave_idle().unwrap();
    create_document(&harness, "Manuscript", "Old ending");
    harness
        .type_into_target(
            HarnessWindow::Project,
            HarnessTarget::EditorPrimary,
            "They sailed away.",
        )
        .unwrap();
    harness.elapse_autosave_idle().unwrap();
    route(&harness, RibbonDestination::History);
    let baseline = harness.history_checkpoints().unwrap()[0].id.clone();
    harness
        .click_history_checkpoint_by_id(HarnessWindow::Project, &baseline)
        .unwrap();
    assert!(
        harness
            .text_is_visible(HarnessWindow::Project, "No changes since this checkpoint.")
            .unwrap()
    );
    capture(&harness, "history-unchanged");
    route(&harness, RibbonDestination::Editor);
    harness
        .right_click_text(HarnessWindow::Project, "Old ending")
        .unwrap();
    harness
        .click_text(HarnessWindow::Project, "Delete")
        .unwrap();
    create_group(&harness, "Manuscript", "Revised draft");
    create_document(&harness, "Revised draft", "New ending");
    harness
        .type_into_target(
            HarnessWindow::Project,
            HarnessTarget::EditorPrimary,
            "They stayed ashore.",
        )
        .unwrap();
    harness.elapse_autosave_idle().unwrap();
    harness
        .type_focused(HarnessWindow::Project, " They lit a fire.")
        .unwrap();
    assert!(
        harness
            .active_editor_body()
            .unwrap()
            .contains("They stayed ashore. They lit a fire."),
        "{}",
        harness.active_editor_body().unwrap()
    );
    harness
        .click_text(HarnessWindow::Project, "Untitled Document")
        .unwrap();
    harness
        .select_editor_text(
            HarnessWindow::Project,
            parchmint_desktop::EditorPane::Primary,
            "blue",
        )
        .unwrap();
    harness
        .type_focused(HarnessWindow::Project, "green")
        .unwrap();
    // Keep this edit unsaved, then leave its document to select a checkpoint.
    route(&harness, RibbonDestination::History);
    let checkpoints = harness.history_checkpoints().unwrap();
    harness
        .scroll_target_by(
            HarnessWindow::Project,
            HarnessTarget::HistoryTimeline,
            -400.0,
        )
        .unwrap();
    harness
        .click_history_checkpoint_by_id(HarnessWindow::Project, &baseline)
        .unwrap();
    assert_eq!(
        harness.history_checkpoints().unwrap(),
        checkpoints,
        "reading History must not save an unsaved draft"
    );
    capture(&harness, "history-project-changes");
    for text in [
        "Changes since checkpoint",
        "Project outline and settings",
        "Added document · New ending",
        "Deleted document · Old ending",
        "The lantern was blue.",
        "The lantern was green.",
        "They sailed away.",
        "They stayed ashore. They lit a fire.",
    ] {
        assert!(
            harness.contains_text(HarnessWindow::Project, text).unwrap(),
            "missing History evidence: {text}"
        );
    }
    assert!(
        harness
            .text_is_visible(HarnessWindow::Project, "Changes since checkpoint")
            .unwrap()
    );
    route(&harness, RibbonDestination::Editor);
    harness
        .select_editor_text(
            HarnessWindow::Project,
            parchmint_desktop::EditorPane::Primary,
            "green",
        )
        .unwrap();
    harness
        .type_focused(HarnessWindow::Project, "amber")
        .unwrap();
    route(&harness, RibbonDestination::History);
    assert!(
        harness
            .contains_text(HarnessWindow::Project, "The lantern was amber.")
            .unwrap(),
        "returning to History must refresh the selected comparison"
    );
    capture(&harness, "history-refreshed");
    harness.close(HarnessWindow::Project).unwrap();
    harness.shutdown().unwrap();
}

#[test]
fn a_failed_history_action_is_reported_and_its_banner_expires() {
    use parchmint_desktop::{ProductionFaultKind, ProductionFaultPoint};
    let run = IsolatedRun::new("error-notification-usability").unwrap();
    let project = run.root().join("novel.parchmint");
    let harness = create_project(&run, &project, "Notification recovery");
    route(&harness, RibbonDestination::History);
    harness
        .type_into(HarnessWindow::Project, "Milestone name", "Before revision")
        .unwrap();
    harness.fail_next(ProductionFaultPoint::History, ProductionFaultKind::Io);
    let error = harness
        .click_text(HarnessWindow::Project, "Create milestone")
        .expect_err("a History failure must fail the UI action");
    assert!(error.to_string().contains("History"), "{error}");
    harness
        .click_target(HarnessWindow::Project, HarnessTarget::ModalCancel)
        .unwrap();
    capture(&harness, "error-banner");
    harness.elapse_notifications().unwrap();
    assert!(
        !harness
            .text_is_visible(HarnessWindow::Project, "Dismiss")
            .unwrap(),
        "an expired error banner must leave the workspace"
    );
    harness
        .click_text(HarnessWindow::Project, "Notifications 1")
        .unwrap();
    capture(&harness, "error-drawer");
    assert!(
        harness
            .text_is_visible(HarnessWindow::Project, "Dismiss")
            .unwrap(),
        "the error remains accessible in the drawer"
    );
    harness.click_text(HarnessWindow::Project, "Close").unwrap();
    assert!(
        !harness
            .text_is_visible(HarnessWindow::Project, "Dismiss")
            .unwrap()
    );
    harness
        .click_text(HarnessWindow::Project, "Create milestone")
        .unwrap();
    capture(&harness, "error-retry");
    assert!(
        harness
            .history_checkpoints()
            .unwrap()
            .iter()
            .any(|checkpoint| checkpoint.label == "Before revision"),
        "{:?}",
        harness.history_checkpoints().unwrap()
    );
    harness.close(HarnessWindow::Project).unwrap();
    harness.shutdown().unwrap();
}
