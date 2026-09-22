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
