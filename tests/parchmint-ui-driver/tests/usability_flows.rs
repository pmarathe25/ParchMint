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

#[test]
fn dictionary_refresh_preserves_typing_and_added_words_survive_reopening() {
    let run = IsolatedRun::new("dictionary-drafts").unwrap();
    let project = run.root().join("dictionary.parchmint");
    let harness = create_project(&run, &project, "Dictionary drafts");
    route(&harness, RibbonDestination::Settings);
    harness
        .click_text(HarnessWindow::Project, "Dictionaries")
        .unwrap();
    harness
        .type_into(HarnessWindow::Project, "Enter a dictionary word", "harbor")
        .unwrap();
    harness
        .click_text(HarnessWindow::Project, "Add word")
        .unwrap();
    assert!(
        harness
            .text_is_visible(HarnessWindow::Project, "harbor")
            .unwrap()
    );

    harness
        .type_into(
            HarnessWindow::Project,
            "Enter a dictionary word",
            " harbor ",
        )
        .unwrap();
    // Reentering this category refreshes global words in the background while
    // the project dictionary and its unsubmitted input remain visible.
    harness
        .click_text(HarnessWindow::Project, "Dictionaries")
        .unwrap();
    harness
        .replace_text(HarnessWindow::Project, " harbor ", "lantern")
        .unwrap();
    capture(&harness, "dictionary-draft-light");
    harness
        .click_text(HarnessWindow::Project, "Add word")
        .unwrap();
    harness
        .click_text(HarnessWindow::Project, "Appearance")
        .unwrap();
    harness.click_text(HarnessWindow::Project, "Dark").unwrap();
    harness
        .click_text(HarnessWindow::Project, "Dictionaries")
        .unwrap();
    capture(&harness, "dictionary-words-dark");
    harness.close(HarnessWindow::Project).unwrap();
    harness.shutdown().unwrap();
    assert_eq!(
        std::fs::read_to_string(project.join("dictionary.txt")).unwrap(),
        "harbor\nlantern\n"
    );

    let reopened = DesktopInteractionHarness::launch(
        run.root(),
        parchmint_desktop::LaunchRequest::open(&project),
    )
    .unwrap();
    route(&reopened, RibbonDestination::Settings);
    reopened
        .click_text(HarnessWindow::Project, "Dictionaries")
        .unwrap();
    capture(&reopened, "dictionary-reopened-dark");
    for word in ["harbor", "lantern"] {
        assert!(
            reopened
                .text_is_visible(HarnessWindow::Project, word)
                .unwrap(),
            "missing dictionary word after reopening: {word}"
        );
    }
    reopened.close(HarnessWindow::Project).unwrap();
    reopened.shutdown().unwrap();
}

#[test]
fn inline_fonts_and_contextual_managers_preserve_writing_and_saved_data() {
    use parchmint_desktop::{EditorPane, HarnessKey, LaunchRequest};
    let run = IsolatedRun::new("inline-fonts").unwrap();
    let project = run.root().join("fonts.parchmint");
    let harness = create_project(&run, &project, "Font controls");
    harness
        .type_into_target(
            HarnessWindow::Project,
            HarnessTarget::EditorPrimary,
            format!(
                "A larger word beside ordinary prose. {}",
                "The harbor bells rang across the water. ".repeat(8)
            ),
        )
        .unwrap();
    harness
        .select_editor_text(HarnessWindow::Project, EditorPane::Primary, "larger")
        .unwrap();
    for (target, index) in [
        (HarnessTarget::FontFamily, 2),
        (HarnessTarget::FontSize, 13),
    ] {
        harness
            .click_target(HarnessWindow::Project, target)
            .unwrap();
        for _ in 0..=index {
            harness
                .press_key(HarnessWindow::Project, HarnessKey::ArrowDown)
                .unwrap();
        }
        harness
            .press_key(HarnessWindow::Project, HarnessKey::Enter)
            .unwrap();
    }
    let formatted = harness.active_editor_body().unwrap();
    assert!(
        formatted.contains("data-font-family=\"sans-serif\""),
        "{formatted}"
    );
    assert!(
        formatted.contains("data-font-size=\"32\">larger"),
        "{formatted}"
    );
    capture(&harness, "font-mixed-light");
    harness
        .press_command_key(HarnessWindow::Project, 'z')
        .unwrap();
    assert!(
        !harness
            .active_editor_body()
            .unwrap()
            .contains("data-font-size")
    );
    #[cfg(target_os = "macos")]
    harness
        .press_command_shift_key(HarnessWindow::Project, 'z')
        .unwrap();
    #[cfg(not(target_os = "macos"))]
    harness
        .press_command_key(HarnessWindow::Project, 'y')
        .unwrap();
    assert_eq!(harness.active_editor_body().unwrap(), formatted);

    harness
        .click_target(HarnessWindow::Project, HarnessTarget::ParagraphStyle)
        .unwrap();
    harness
        .click_target(HarnessWindow::Project, HarnessTarget::ManageStyles)
        .unwrap();
    assert!(
        harness
            .target_is_visible(HarnessWindow::Project, HarnessTarget::CreateStyle)
            .unwrap()
    );
    capture(&harness, "styles-context-light");
    harness
        .click_target(HarnessWindow::Project, HarnessTarget::CreateStyle)
        .unwrap();
    harness.click_text(HarnessWindow::Project, "Save").unwrap();
    assert_eq!(harness.active_editor_body().unwrap(), formatted);

    // Caret controls change future typing, not the paragraph's shared style.
    harness
        .select_editor_text(HarnessWindow::Project, EditorPane::Primary, "larger")
        .unwrap();
    harness
        .press_key(HarnessWindow::Project, HarnessKey::ArrowRight)
        .unwrap();
    harness
        .click_target(HarnessWindow::Project, HarnessTarget::FontSize)
        .unwrap();
    for _ in 0..=11 {
        harness
            .press_key(HarnessWindow::Project, HarnessKey::ArrowDown)
            .unwrap();
    }
    harness
        .press_key(HarnessWindow::Project, HarnessKey::Enter)
        .unwrap();
    harness
        .type_focused(HarnessWindow::Project, "typedmarker")
        .unwrap();
    assert!(
        harness
            .active_editor_body()
            .unwrap()
            .contains("data-font-size=\"24\">typedmarker")
    );
    harness
        .select_editor_text(HarnessWindow::Project, EditorPane::Primary, "typedmarker")
        .unwrap();
    harness
        .press_command_key(HarnessWindow::Project, 'c')
        .unwrap();
    assert_eq!(
        harness.clipboard_contents().0.as_deref(),
        Some("typedmarker")
    );
    harness
        .press_key(HarnessWindow::Project, HarnessKey::ArrowRight)
        .unwrap();
    harness
        .press_command_key(HarnessWindow::Project, 'v')
        .unwrap();
    let pasted = harness.active_editor_body().unwrap();
    // Native copy currently publishes plain text; it must not change its source.
    assert!(
        pasted.contains("data-font-size=\"24\">typedmarker</span></span>typedmarker"),
        "{pasted}"
    );
    harness
        .press_command_key(HarnessWindow::Project, 'z')
        .unwrap();
    harness.seed_clipboard(
        Some("typedmarker"),
        Some("<span data-font-family=\"sans-serif\"><span data-font-size=\"24\">typedmarker</span></span>"),
    );
    harness
        .press_command_key(HarnessWindow::Project, 'v')
        .unwrap();
    let pasted = harness.active_editor_body().unwrap();
    assert!(
        pasted.contains("data-font-size=\"24\">typedmarkertypedmarker"),
        "{pasted}"
    );
    harness
        .press_command_key(HarnessWindow::Project, 'z')
        .unwrap();

    let title = harness.active_editor_tab_title().unwrap();
    harness
        .right_click_text(HarnessWindow::Project, &title)
        .unwrap();
    harness
        .click_text(HarnessWindow::Project, "Open beside")
        .unwrap();
    harness
        .select_editor_text(HarnessWindow::Project, EditorPane::Companion, "ordinary")
        .unwrap();
    harness
        .click_target(HarnessWindow::Project, HarnessTarget::FontFamily)
        .unwrap();
    for _ in 0..=3 {
        harness
            .press_key(HarnessWindow::Project, HarnessKey::ArrowDown)
            .unwrap();
    }
    harness
        .press_key(HarnessWindow::Project, HarnessKey::Enter)
        .unwrap();
    let shared = harness.active_editor_body().unwrap();
    assert!(shared.contains("data-font-family=\"monospace\">ordinary"));
    harness
        .click_target(HarnessWindow::Project, HarnessTarget::EditorPrimary)
        .unwrap();
    assert_eq!(harness.active_editor_body().unwrap(), shared);
    capture(&harness, "font-two-panes-light");
    harness
        .click_target(HarnessWindow::Project, HarnessTarget::ToggleCompanion)
        .unwrap();

    harness
        .resize(HarnessWindow::Project, 1280.0, 720.0)
        .unwrap();
    route(&harness, RibbonDestination::Settings);
    harness
        .click_text(HarnessWindow::Project, "Appearance")
        .unwrap();
    harness.click_text(HarnessWindow::Project, "Dark").unwrap();
    route(&harness, RibbonDestination::Editor);
    capture(&harness, "font-mixed-dark-compact");
    harness
        .click_target(HarnessWindow::Project, HarnessTarget::ParagraphStyle)
        .unwrap();
    harness
        .click_target(HarnessWindow::Project, HarnessTarget::ManageStyles)
        .unwrap();
    capture(&harness, "styles-context-dark-compact");
    harness
        .press_key(HarnessWindow::Project, HarnessKey::Tab)
        .unwrap();
    harness
        .press_key(HarnessWindow::Project, HarnessKey::Escape)
        .unwrap();

    route(&harness, RibbonDestination::Cards);
    harness
        .click_target(HarnessWindow::Project, HarnessTarget::ManageMetadata)
        .unwrap();
    harness
        .click_target(HarnessWindow::Project, HarnessTarget::CreateMetadataField)
        .unwrap();
    harness
        .type_into_target(
            HarnessWindow::Project,
            HarnessTarget::MetadataFieldName,
            "Viewpoint",
        )
        .unwrap();
    harness
        .click_text(HarnessWindow::Project, "Add field")
        .unwrap();
    capture(&harness, "metadata-context-dark-compact");
    harness.click_text(HarnessWindow::Project, "Save").unwrap();
    assert!(
        !harness
            .text_is_visible(HarnessWindow::Project, "Viewpoint")
            .unwrap(),
        "new fields should stay out of collapsed cards"
    );
    let card = harness.hierarchy_node(&title).unwrap();
    harness
        .toggle_card_details(HarnessWindow::Project, card.clone())
        .unwrap();
    assert!(
        harness
            .text_is_visible(HarnessWindow::Project, "Viewpoint")
            .unwrap()
    );
    harness.click_text(HarnessWindow::Project, "—").unwrap();
    harness
        .type_focused(HarnessWindow::Project, "Planning viewpoint marker")
        .unwrap();
    harness
        .press_key(HarnessWindow::Project, HarnessKey::Escape)
        .unwrap();
    harness
        .toggle_card_details(HarnessWindow::Project, card)
        .unwrap();
    route(&harness, RibbonDestination::Editor);
    let saved = harness.active_editor_body().unwrap();
    harness.close(HarnessWindow::Project).unwrap();
    harness.shutdown().unwrap();
    let reopened =
        DesktopInteractionHarness::launch(run.root(), LaunchRequest::open(&project)).unwrap();
    assert_eq!(reopened.active_editor_body().unwrap(), saved);
    assert!(
        std::fs::read_to_string(project.join("project.toml"))
            .unwrap()
            .contains("Planning viewpoint marker")
    );
    assert!(
        std::fs::read_to_string(project.join("project.toml"))
            .unwrap()
            .contains("Viewpoint")
    );
    assert!(
        std::fs::read_to_string(project.join("project.toml"))
            .unwrap()
            .contains("New style")
    );
    reopened.close(HarnessWindow::Project).unwrap();
    reopened.shutdown().unwrap();
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
            .text_is_visible(HarnessWindow::Project, "New group")
            .unwrap()
    );
    capture(&harness, "creation-menu");
    harness
        .click_text(HarnessWindow::Project, "New group")
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
            .text_is_visible(HarnessWindow::Project, "New document")
            .unwrap()
    );
    harness
        .click_text(HarnessWindow::Project, "New document")
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
    harness
        .click_text(HarnessWindow::Project, "Dictionaries")
        .unwrap();
    capture(&harness, "dictionary-dark-compact");
    route(&harness, RibbonDestination::Editor);
    harness
        .click_target(HarnessWindow::Project, HarnessTarget::ParagraphStyle)
        .unwrap();
    harness
        .click_target(HarnessWindow::Project, HarnessTarget::ManageStyles)
        .unwrap();
    capture(&harness, "styles-dark-compact");
    harness.click_text(HarnessWindow::Project, "Save").unwrap();
    route(&harness, RibbonDestination::Cards);
    harness
        .click_target(HarnessWindow::Project, HarnessTarget::ManageMetadata)
        .unwrap();
    capture(&harness, "metadata-dark-compact");
    harness.click_text(HarnessWindow::Project, "Save").unwrap();
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
            .text_is_visible(HarnessWindow::Project, "New document")
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
            .contains_text(HarnessWindow::Project, "New document")
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
    create_group(&harness, "Manuscript", "Early draft");
    create_document(&harness, "Early draft", "Unchanged chapter");
    harness
        .type_into_target(
            HarnessWindow::Project,
            HarnessTarget::EditorPrimary,
            "Unchanged prose",
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
    harness
        .right_click_text(HarnessWindow::Project, "Early draft")
        .unwrap();
    harness
        .click_text(HarnessWindow::Project, "Rename")
        .unwrap();
    harness
        .replace_text_and_submit(HarnessWindow::Project, "Early draft", "Revised draft")
        .unwrap();
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
    assert!(
        !harness
            .contains_text(HarnessWindow::Project, "Unchanged prose")
            .unwrap()
    );
    for text in [
        "Checkpoint",
        "Current project",
        "Early draft",
        "Revised draft",
        "New ending",
        "Old ending",
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
            .text_is_visible(HarnessWindow::Project, "Checkpoint")
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
        .click_text(HarnessWindow::Project, "Notifications")
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
fn appearance_and_motion_choices_survive_reopening_the_application() {
    let run = IsolatedRun::new("motion-preference").unwrap();
    let project = run.root().join("novel.parchmint");
    let preferences = run.root().join("configuration/preferences.json");
    let harness = create_project(&run, &project, "Motion preference");
    route(&harness, RibbonDestination::Settings);
    harness.click_text(HarnessWindow::Project, "Dark").unwrap();
    assert!(
        harness
            .text_is_visible(HarnessWindow::Project, "Appearance")
            .unwrap()
    );
    harness
        .click_text(HarnessWindow::Project, "Reduce motion")
        .unwrap();
    let saved: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&preferences).unwrap()).unwrap();
    assert_eq!(saved["preferences"]["reduced_motion"], true);
    assert_eq!(saved["preferences"]["appearance"], "Dark");
    harness.close(HarnessWindow::Project).unwrap();
    harness.shutdown().unwrap();

    let harness = DesktopInteractionHarness::launch(
        run.root(),
        parchmint_desktop::LaunchRequest::open(&project),
    )
    .unwrap();
    route(&harness, RibbonDestination::Settings);
    assert!(
        harness
            .text_is_visible(HarnessWindow::Project, "Appearance")
            .unwrap()
    );
    harness.click_text(HarnessWindow::Project, "Light").unwrap();
    assert!(
        harness
            .text_is_visible(HarnessWindow::Project, "Appearance")
            .unwrap()
    );
    harness
        .click_text(HarnessWindow::Project, "Reduce motion")
        .unwrap();
    let saved: serde_json::Value =
        serde_json::from_slice(&std::fs::read(preferences).unwrap()).unwrap();
    assert_eq!(saved["preferences"]["reduced_motion"], false);
    assert_eq!(saved["preferences"]["appearance"], "Light");
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
            .text_is_visible(HarnessWindow::Project, "RECENT PROJECTS")
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
            .text_is_visible(HarnessWindow::Project, "RECENT PROJECTS")
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
fn deleted_recent_project_is_pruned_and_startup_opens_the_workspace() {
    let run = IsolatedRun::new("missing-recent-project").unwrap();
    let project = run.root().join("novel.parchmint");
    create_project(&run, &project, "Missing novel")
        .shutdown()
        .unwrap();
    std::fs::rename(&project, run.root().join("moved.parchmint")).unwrap();
    let harness =
        DesktopInteractionHarness::launch(run.root(), parchmint_desktop::LaunchRequest::launcher())
            .unwrap();
    assert!(!harness.has_window(HarnessWindow::Launcher).unwrap());
    harness
        .click_text(HarnessWindow::Project, "My Writing")
        .unwrap();
    assert!(
        !harness
            .contains_text(HarnessWindow::Project, "Missing novel")
            .unwrap()
    );
    assert!(
        harness
            .text_is_visible(HarnessWindow::Project, "Create Project")
            .unwrap()
    );
    harness.shutdown().unwrap();
}
