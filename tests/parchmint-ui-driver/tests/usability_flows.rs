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
fn creation_has_one_primary_entry_and_keeps_writing_controls_usable() {
    let run = IsolatedRun::new("creation-usability").unwrap();
    let project = run.root().join("novel.parchmint");
    let harness = create_project(&run, &project, "Creation usability");
    assert!(
        !harness
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
    harness
        .right_click_text(HarnessWindow::Project, "Manuscript")
        .unwrap();
    assert!(
        harness
            .text_is_visible(HarnessWindow::Project, "Create group")
            .unwrap()
    );
    capture(&harness, "creation-menu");
    harness
        .click_text(HarnessWindow::Project, "Create group")
        .unwrap();
    harness
        .replace_text_and_submit(HarnessWindow::Project, "New Group", "Drafts")
        .unwrap();
    assert!(
        !harness
            .text_is_visible(HarnessWindow::Project, "Created item")
            .unwrap()
    );
    capture(&harness, "creation-banner");
    harness
        .right_click_text(HarnessWindow::Project, "Drafts")
        .unwrap();
    assert!(
        harness
            .text_is_visible(HarnessWindow::Project, "Create document")
            .unwrap()
    );
    harness
        .click_text(HarnessWindow::Project, "Create document")
        .unwrap();
    harness
        .replace_text_and_submit(HarnessWindow::Project, "Untitled", "Opening")
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
        !harness
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
    capture(&harness, "appearance-dark-compact");
    for (category, name) in [
        ("Styles", "styles-dark-compact"),
        ("Metadata fields", "metadata-dark-compact"),
        ("Dictionaries", "dictionary-dark-compact"),
    ] {
        harness
            .click_text(HarnessWindow::Project, category)
            .unwrap();
        if category == "Styles" {
            harness.click_text(HarnessWindow::Project, "Body").unwrap();
        }
        capture(&harness, name);
    }
    route(&harness, RibbonDestination::Export);
    capture(&harness, "export-dark-compact");
    route(&harness, RibbonDestination::Cards);
    capture(&harness, "outline-dark-compact");
    route(&harness, RibbonDestination::Editor);
    harness
        .right_click_text(HarnessWindow::Project, "Later")
        .unwrap();
    assert!(
        harness
            .text_is_visible(HarnessWindow::Project, "Create document")
            .unwrap()
    );
    capture(&harness, "creation-menu-dark-compact");
    harness
        .press_key(
            HarnessWindow::Project,
            parchmint_desktop::HarnessKey::Escape,
        )
        .unwrap();
    assert!(
        !harness
            .contains_text(HarnessWindow::Project, "Create document")
            .unwrap()
    );
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
            .text_is_visible(HarnessWindow::Project, "No changes since this version.")
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
    // Expire the deletion banner before making unsaved edits. Capture time
    // must not decide whether History includes a banner or how far it scrolls.
    harness.elapse_notifications().unwrap();
    assert!(
        !harness
            .text_is_visible(HarnessWindow::Project, "Moved item to Recently Deleted")
            .unwrap()
    );
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
        "Saved version",
        "Current",
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
            .text_is_visible(HarnessWindow::Project, "Saved version")
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

#[test]
fn reduced_motion_choice_survives_reopening_the_application() {
    let run = IsolatedRun::new("motion-preference").unwrap();
    let project = run.root().join("novel.parchmint");
    let preferences = run.root().join("configuration/preferences.json");
    let harness = create_project(&run, &project, "Motion preference");
    route(&harness, RibbonDestination::Settings);
    harness
        .click_text(HarnessWindow::Project, "Reduce motion")
        .unwrap();
    let saved: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&preferences).unwrap()).unwrap();
    assert_eq!(saved["preferences"]["reduced_motion"], true);
    harness.close(HarnessWindow::Project).unwrap();
    harness.shutdown().unwrap();

    let harness = DesktopInteractionHarness::launch(
        run.root(),
        parchmint_desktop::LaunchRequest::open(&project),
    )
    .unwrap();
    route(&harness, RibbonDestination::Settings);
    harness
        .click_text(HarnessWindow::Project, "Reduce motion")
        .unwrap();
    let saved: serde_json::Value =
        serde_json::from_slice(&std::fs::read(preferences).unwrap()).unwrap();
    assert_eq!(saved["preferences"]["reduced_motion"], false);
    harness.close(HarnessWindow::Project).unwrap();
    harness.shutdown().unwrap();
}

#[test]
fn launch_resumes_the_project_and_title_opens_a_safe_project_chooser() {
    let run = IsolatedRun::new("resume-chooser").unwrap();
    let project = run.root().join("novel.parchmint");
    create_project(&run, &project, "Resume novel")
        .shutdown()
        .unwrap();
    let harness =
        DesktopInteractionHarness::launch(run.root(), parchmint_desktop::LaunchRequest::launcher())
            .unwrap();
    assert!(harness.has_window(HarnessWindow::Project).unwrap());
    assert!(!harness.has_window(HarnessWindow::Launcher).unwrap());
    harness
        .click_text(HarnessWindow::Project, "Resume novel")
        .unwrap();
    assert!(
        harness
            .text_is_visible(HarnessWindow::Project, "Recent projects")
            .unwrap()
    );
    capture(&harness, "project-chooser");
    harness
        .click_text(HarnessWindow::Project, "Create Project")
        .unwrap();
    harness
        .type_into(HarnessWindow::Project, "Project title", "Another story")
        .unwrap();
    assert!(
        !harness
            .contains_text(HarnessWindow::Project, "Author · optional")
            .unwrap()
    );
    capture(&harness, "project-create");
    harness
        .press_key(
            HarnessWindow::Project,
            parchmint_desktop::HarnessKey::Escape,
        )
        .unwrap();
    assert!(
        !harness
            .text_is_visible(HarnessWindow::Project, "Create and Open")
            .unwrap()
    );
    harness
        .click_text(HarnessWindow::Project, "Resume novel")
        .unwrap();
    harness
        .click_text(HarnessWindow::Project, "Resume novel")
        .unwrap();
    assert!(
        !harness
            .text_is_visible(HarnessWindow::Project, "Recent projects")
            .unwrap()
    );
    harness
        .click_text(HarnessWindow::Project, "Resume novel")
        .unwrap();
    harness
        .click_text(HarnessWindow::Project, "Create Project")
        .unwrap();
    let second = run.root().join("another.parchmint");
    harness
        .type_into(
            HarnessWindow::Project,
            "Project destination",
            second.to_string_lossy(),
        )
        .unwrap();
    harness
        .click_text(HarnessWindow::Project, "Create and Open")
        .unwrap();
    assert!(second.join("project.toml").exists());
    harness.shutdown().unwrap();

    let harness =
        DesktopInteractionHarness::launch(run.root(), parchmint_desktop::LaunchRequest::launcher())
            .unwrap();
    std::fs::rename(&project, run.root().join("moved.parchmint")).unwrap();
    harness
        .click_text(HarnessWindow::Project, "Another story")
        .unwrap();
    assert!(
        !harness
            .contains_text(HarnessWindow::Project, "Resume novel")
            .unwrap()
    );
    harness.shutdown().unwrap();
}

#[test]
fn deleted_recent_project_is_pruned_and_startup_returns_to_the_chooser() {
    let run = IsolatedRun::new("missing-recent-project").unwrap();
    let project = run.root().join("novel.parchmint");
    create_project(&run, &project, "Missing novel")
        .shutdown()
        .unwrap();
    std::fs::rename(&project, run.root().join("moved.parchmint")).unwrap();
    let harness =
        DesktopInteractionHarness::launch(run.root(), parchmint_desktop::LaunchRequest::launcher())
            .unwrap();
    assert!(harness.has_window(HarnessWindow::Launcher).unwrap());
    assert!(
        !harness
            .contains_text(HarnessWindow::Launcher, "Missing novel")
            .unwrap()
    );
    assert!(
        harness
            .text_is_visible(HarnessWindow::Launcher, "Create Project")
            .unwrap()
    );
    harness.shutdown().unwrap();
}
