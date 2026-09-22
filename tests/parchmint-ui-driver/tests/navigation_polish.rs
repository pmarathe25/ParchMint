use parchmint_desktop::{
    DesktopInteractionHarness, HarnessKey, HarnessTarget, HarnessWindow, LaunchRequest,
    RibbonDestination,
};
use parchmint_ui_driver::{IsolatedRun, create_document, create_project};
const WINDOW: HarnessWindow = HarnessWindow::Project;

#[test]
fn keyboard_creation_takes_focus_away_from_the_document() {
    let run = IsolatedRun::new("creation-keyboard-focus").unwrap();
    let project = run.root().join("novel.parchmint");
    let harness = create_project(&run, &project, "Creation focus");
    create_document(&harness, "Manuscript", "Chapter");
    harness
        .click_target(WINDOW, HarnessTarget::EditorPrimary)
        .unwrap();
    harness.type_focused(WINDOW, "Original body").unwrap();
    let before = harness.active_editor_body().unwrap();
    let titles = harness.hierarchy_titles().unwrap();
    harness.press_command_shift_key(WINDOW, 'n').unwrap();
    harness.type_focused(WINDOW, "Cancelled name").unwrap();
    harness.press_key(WINDOW, HarnessKey::Escape).unwrap();
    assert_eq!(harness.hierarchy_titles().unwrap(), titles);
    assert_eq!(harness.active_editor_body().unwrap(), before);
    harness.shutdown().unwrap();
}

#[test]
fn canceled_outline_creation_never_reaches_the_project_and_confirmed_names_persist() {
    let run = IsolatedRun::new("cancel-outline-creation").unwrap();
    let project = run.root().join("novel.parchmint");
    let harness = create_project(&run, &project, "Creation review");
    let before = harness.hierarchy_titles().unwrap();
    for page in [RibbonDestination::Editor, RibbonDestination::Cards] {
        harness
            .click_target(WINDOW, HarnessTarget::Ribbon(page))
            .unwrap();
        for action in ["New document", "New group"] {
            harness.right_click_text(WINDOW, "Manuscript").unwrap();
            harness.click_text(WINDOW, action).unwrap();
            harness.type_focused(WINDOW, "Discard this name").unwrap();
            harness.press_key(WINDOW, HarnessKey::Escape).unwrap();
            assert_eq!(harness.hierarchy_titles().unwrap(), before);
        }
    }
    harness
        .click_target(WINDOW, HarnessTarget::Ribbon(RibbonDestination::Editor))
        .unwrap();
    create_document(&harness, "Manuscript", "Confirmed chapter");
    harness.press_command_key(WINDOW, 's').unwrap();
    harness.shutdown().unwrap();
    let reopened =
        DesktopInteractionHarness::launch(run.root(), LaunchRequest::open(&project)).unwrap();
    assert!(
        reopened
            .hierarchy_titles()
            .unwrap()
            .iter()
            .any(|title| title == "Confirmed chapter")
    );
    assert!(
        !reopened
            .hierarchy_titles()
            .unwrap()
            .iter()
            .any(|title| title.contains("Discard"))
    );
    reopened.shutdown().unwrap();
}

#[test]
fn reassigned_formatting_uses_the_default_command_and_survives_reopening() {
    let run = IsolatedRun::new("custom-shortcuts").unwrap();
    let project = run.root().join("novel.parchmint");
    let harness = create_project(&run, &project, "Shortcut review");
    create_document(&harness, "Manuscript", "Chapter");
    harness
        .click_target(WINDOW, HarnessTarget::Ribbon(RibbonDestination::Settings))
        .unwrap();
    harness.click_text(WINDOW, "Keyboard shortcuts").unwrap();
    harness
        .type_into(WINDOW, "Search commands", "Bold")
        .unwrap();
    harness
        .click_text(
            WINDOW,
            if cfg!(target_os = "macos") {
                "Cmd+B"
            } else {
                "Ctrl+B"
            },
        )
        .unwrap();
    harness.press_command_shift_key(WINDOW, 'b').unwrap();
    assert!(
        harness
            .contains_text(
                WINDOW,
                if cfg!(target_os = "macos") {
                    "Cmd+Shift+B"
                } else {
                    "Ctrl+Shift+B"
                }
            )
            .unwrap()
    );
    harness
        .click_target(WINDOW, HarnessTarget::Ribbon(RibbonDestination::Editor))
        .unwrap();
    harness
        .click_target(WINDOW, HarnessTarget::EditorPrimary)
        .unwrap();
    harness.press_command_shift_key(WINDOW, 'b').unwrap();
    harness.type_focused(WINDOW, "CUSTOMBOLD").unwrap();
    harness.press_command_shift_key(WINDOW, 'b').unwrap();
    harness.press_command_key(WINDOW, 'b').unwrap();
    harness.type_focused(WINDOW, " PLAIN").unwrap();
    harness.press_command_key(WINDOW, 's').unwrap();
    let body = harness.active_editor_body().unwrap();
    assert!(body.contains("<strong>CUSTOMBOLD</strong>"), "{body}");
    assert!(
        !body.contains("<strong> PLAIN"),
        "disabled default must not toggle bold: {body}"
    );
    harness.shutdown().unwrap();
    let reopened =
        DesktopInteractionHarness::launch(run.root(), LaunchRequest::open(&project)).unwrap();
    reopened
        .click_target(WINDOW, HarnessTarget::Ribbon(RibbonDestination::Settings))
        .unwrap();
    reopened.click_text(WINDOW, "Keyboard shortcuts").unwrap();
    reopened
        .type_into(WINDOW, "Search commands", "Bold")
        .unwrap();
    assert!(
        reopened
            .contains_text(
                WINDOW,
                if cfg!(target_os = "macos") {
                    "Cmd+Shift+B"
                } else {
                    "Ctrl+Shift+B"
                }
            )
            .unwrap()
    );
    reopened.shutdown().unwrap();
}

#[test]
fn search_shortcuts_focus_queries_and_escape_dismisses_them() {
    let run = IsolatedRun::new("search-focus").unwrap();
    let project = run.root().join("novel.parchmint");
    let harness = create_project(&run, &project, "Search focus");
    create_document(&harness, "Manuscript", "Chapter");
    harness
        .type_into_target(WINDOW, HarnessTarget::EditorPrimary, "Original prose")
        .unwrap();
    let body = harness.active_editor_body().unwrap();
    harness.press_command_key(WINDOW, 'f').unwrap();
    assert!(
        harness
            .target_is_focused(
                WINDOW,
                HarnessTarget::LocalFind(parchmint_desktop::EditorPane::Primary)
            )
            .unwrap()
    );
    harness.type_focused(WINDOW, "prose").unwrap();
    harness.press_key(WINDOW, HarnessKey::Escape).unwrap();
    assert!(
        !harness
            .target_is_visible(
                WINDOW,
                HarnessTarget::LocalFind(parchmint_desktop::EditorPane::Primary)
            )
            .unwrap()
    );
    harness.press_command_shift_key(WINDOW, 'f').unwrap();
    assert!(
        harness
            .target_is_focused(WINDOW, HarnessTarget::GlobalSearchQuery)
            .unwrap()
    );
    harness.type_focused(WINDOW, "Original").unwrap();
    harness.press_key(WINDOW, HarnessKey::Escape).unwrap();
    assert!(
        !harness
            .target_is_visible(WINDOW, HarnessTarget::GlobalSearchQuery)
            .unwrap()
    );
    assert_eq!(harness.active_editor_body().unwrap(), body);
    harness.shutdown().unwrap();
}

#[test]
fn failed_close_can_exit_without_retrying_the_save() {
    use parchmint_desktop::{ProductionFaultKind, ProductionFaultPoint};
    let run = IsolatedRun::new("exit-without-save").unwrap();
    let project = run.root().join("novel.parchmint");
    let harness = create_project(&run, &project, "Failed close");
    create_document(&harness, "Manuscript", "Chapter");
    harness
        .type_into_target(WINDOW, HarnessTarget::EditorPrimary, "Unsaved text")
        .unwrap();
    harness.fail_next(ProductionFaultPoint::FinalSave, ProductionFaultKind::Io);
    harness.close(WINDOW).expect_err("save fails");
    harness.click_text(WINDOW, "Exit without saving").unwrap();
    assert!(!harness.has_window(WINDOW).unwrap());
    harness.shutdown().unwrap();
}

#[test]
fn group_comments_include_nested_documents_after_reopening() {
    use parchmint_ui_driver::create_group;
    let run = IsolatedRun::new("group-comments").unwrap();
    let project = run.root().join("novel.parchmint");
    let harness = create_project(&run, &project, "Group comments");
    create_group(&harness, "Manuscript", "Act");
    create_group(&harness, "Act", "Nested");
    for (parent, title, comment) in [
        ("Act", "First", "First comment"),
        ("Nested", "Second", "Second comment"),
    ] {
        create_document(&harness, parent, title);
        harness
            .type_into_target(WINDOW, HarnessTarget::EditorPrimary, "Commented prose")
            .unwrap();
        harness
            .select_editor_text(WINDOW, parchmint_desktop::EditorPane::Primary, "Commented")
            .unwrap();
        harness
            .click_target(WINDOW, HarnessTarget::AddComment)
            .unwrap();
        harness.type_focused(WINDOW, comment).unwrap();
        harness.click_text(WINDOW, "Add comment").unwrap();
        harness.press_command_key(WINDOW, 's').unwrap();
    }
    harness.shutdown().unwrap();
    let harness =
        DesktopInteractionHarness::launch(run.root(), LaunchRequest::open(&project)).unwrap();
    let act = harness.hierarchy_node("Act").unwrap();
    harness.click_hierarchy_node(WINDOW, act).unwrap();
    harness
        .click_target(WINDOW, HarnessTarget::ToggleInspector)
        .unwrap();
    assert!(harness.contains_text(WINDOW, "First comment").unwrap());
    assert!(harness.contains_text(WINDOW, "Second comment").unwrap());
    harness.click_text(WINDOW, "First comment").unwrap();
    assert!(
        harness
            .active_editor_body()
            .unwrap()
            .contains("Commented prose")
    );
    harness.shutdown().unwrap();
}

#[test]
fn secondary_click_outside_context_menu_closes_it() {
    let run = IsolatedRun::new("context-dismiss").unwrap();
    let project = run.root().join("novel.parchmint");
    let harness = create_project(&run, &project, "Context dismissal");
    create_document(&harness, "Manuscript", "Chapter");
    let chapter = harness.hierarchy_node("Chapter").unwrap();
    harness.right_click_hierarchy_node(WINDOW, chapter).unwrap();
    assert!(harness.contains_text(WINDOW, "Delete").unwrap());
    harness
        .right_click_target(WINDOW, HarnessTarget::Ribbon(RibbonDestination::Settings))
        .unwrap();
    assert!(!harness.contains_text(WINDOW, "Delete").unwrap());
    harness
        .type_into_target(WINDOW, HarnessTarget::EditorPrimary, "Context text")
        .unwrap();
    harness
        .right_click_target(WINDOW, HarnessTarget::EditorPrimary)
        .unwrap();
    assert!(harness.contains_text(WINDOW, "Copy").unwrap());
    harness
        .right_click_target(WINDOW, HarnessTarget::Ribbon(RibbonDestination::Settings))
        .unwrap();
    assert!(!harness.contains_text(WINDOW, "Copy").unwrap());
    harness.shutdown().unwrap();
}

#[test]
fn page_break_with_a_selection_preserves_selected_text() {
    let run = IsolatedRun::new("selected-break").unwrap();
    let project = run.root().join("novel.parchmint");
    let harness = create_project(&run, &project, "Break insertion");
    create_document(&harness, "Manuscript", "Chapter");
    harness
        .type_into_target(WINDOW, HarnessTarget::EditorPrimary, "Keep these words")
        .unwrap();
    harness
        .select_editor_text(WINDOW, parchmint_desktop::EditorPane::Primary, "these")
        .unwrap();
    harness
        .click_target(WINDOW, HarnessTarget::BreakMenu)
        .unwrap();
    harness.press_key(WINDOW, HarnessKey::ArrowDown).unwrap();
    harness.press_key(WINDOW, HarnessKey::Enter).unwrap();
    let body = harness.active_editor_body().unwrap();
    assert!(body.contains("Keep these"), "{body}");
    assert!(body.contains("page-break"), "{body}");
    harness.shutdown().unwrap();
}
