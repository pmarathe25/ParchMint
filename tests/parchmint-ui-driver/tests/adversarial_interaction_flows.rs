use std::path::Path;

use parchmint_desktop::{
    DesktopInteractionHarness, EditorPane, HarnessKey, HarnessTarget, HarnessWindow, LaunchRequest,
    RibbonDestination,
};
use parchmint_ui_driver::IsolatedRun;

#[test]
fn explorer_rename_escape_preserves_selection_before_enter_commits() {
    let run = IsolatedRun::new("adversarial-rename-escape").expect("isolated run");
    let project = run.root().join("adversarial-rename-escape.parchmint");
    let harness = create_project(&run, &project, "Rename Escape");

    for _ in 0..3 {
        harness
            .press_key(HarnessWindow::Project, HarnessKey::F6)
            .expect("move keyboard focus to Explorer");
    }
    harness
        .click_text(HarnessWindow::Project, "Untitled Document")
        .expect("select the default document");
    harness
        .press_key(HarnessWindow::Project, HarnessKey::F2)
        .expect("start Explorer rename");
    harness
        .type_focused(HarnessWindow::Project, "Draft One")
        .expect("replace the selected title");
    harness
        .press_key(HarnessWindow::Project, HarnessKey::Escape)
        .expect("cancel the first rename");
    assert!(
        harness
            .hierarchy_titles()
            .expect("read hierarchy after cancellation")
            .iter()
            .any(|title| title == "Untitled Document")
    );

    harness
        .press_key(HarnessWindow::Project, HarnessKey::F2)
        .expect("restart rename after Escape");
    harness
        .type_focused(HarnessWindow::Project, "Draft One")
        .expect("enter the replacement title");
    harness
        .press_key(HarnessWindow::Project, HarnessKey::Enter)
        .expect("commit the replacement title");
    assert!(
        harness
            .hierarchy_titles()
            .expect("read committed hierarchy")
            .iter()
            .any(|title| title == "Draft One")
    );
    close(harness);
}

#[test]
fn comment_popover_escape_does_not_leak_into_inspector_or_editor() {
    let run = IsolatedRun::new("adversarial-popover-dismissal").expect("isolated run");
    let project = run.root().join("adversarial-popover-dismissal.parchmint");
    let harness = create_project(&run, &project, "Popover Dismissal");

    harness
        .type_into_target(
            HarnessWindow::Project,
            HarnessTarget::EditorPrimary,
            "A sentence worth annotating.",
        )
        .expect("write an anchorable sentence");
    harness
        .select_editor_text(
            HarnessWindow::Project,
            EditorPane::Primary,
            "worth annotating",
        )
        .expect("select the annotation anchor");
    harness
        .right_click_target_at(
            HarnessWindow::Project,
            HarnessTarget::EditorPrimary,
            (0.5, 0.5),
        )
        .expect("open the editor popover");
    assert!(visible(&harness, "Add Comment"));
    harness
        .press_key(HarnessWindow::Project, HarnessKey::Escape)
        .expect("dismiss the editor popover");
    assert!(!visible(&harness, "Add Comment"));

    harness
        .click_target(
            HarnessWindow::Project,
            HarnessTarget::Ribbon(RibbonDestination::Cards),
        )
        .expect("switch route after dismissing the popover");
    close(harness);
}

#[test]
fn cards_selection_and_explorer_selection_survive_route_switching() {
    let run = IsolatedRun::new("adversarial-route-selection").expect("isolated run");
    let project = run.root().join("adversarial-route-selection.parchmint");
    let harness = create_project(&run, &project, "Route Selection");
    create_group(&harness, "Manuscript", "Act One");
    create_document(&harness, "Act One", "Opening");

    harness
        .click_target(
            HarnessWindow::Project,
            HarnessTarget::Ribbon(RibbonDestination::Cards),
        )
        .expect("open Cards route");
    harness
        .click_text(HarnessWindow::Project, "Opening")
        .expect("select the card");
    harness
        .click_target(
            HarnessWindow::Project,
            HarnessTarget::Ribbon(RibbonDestination::History),
        )
        .expect("switch to History");
    harness
        .click_target(
            HarnessWindow::Project,
            HarnessTarget::Ribbon(RibbonDestination::Editor),
        )
        .expect("return to Editor");
    let opening = harness
        .hierarchy_node("Opening")
        .expect("resolve the selected document");
    harness
        .click_hierarchy_node(HarnessWindow::Project, opening)
        .expect("select the same document in Explorer");
    assert!(
        harness
            .target_is_visible(HarnessWindow::Project, HarnessTarget::InspectorSynopsis)
            .expect("inspect the selected document Inspector")
    );
    assert_eq!(
        harness
            .active_editor_tab_title()
            .expect("read selected editor tab"),
        "Opening"
    );
    close(harness);
}

#[test]
fn tab_overflow_escape_keeps_active_tab_before_selecting_a_hidden_tab() {
    let run = IsolatedRun::new("adversarial-tab-overflow").expect("isolated run");
    let project = run.root().join("adversarial-tab-overflow.parchmint");
    let harness = create_project(&run, &project, "Tab Overflow");
    for title in [
        "Chapter One",
        "Chapter Two",
        "Chapter Three",
        "Chapter Four",
        "Chapter Five",
        "Chapter Six",
        "Chapter Seven",
        "Chapter Eight",
        "Chapter Nine",
        "Chapter Ten",
    ] {
        create_document(&harness, "Manuscript", title);
        harness
            .right_click_text(HarnessWindow::Project, title)
            .expect("open document context menu");
        harness
            .click_text(HarnessWindow::Project, "Open")
            .expect("open document as a tab");
    }
    harness
        .resize(HarnessWindow::Project, 1280.0, 720.0)
        .expect("constrain the tab strip");
    harness
        .click_target(
            HarnessWindow::Project,
            HarnessTarget::TabOverflow(EditorPane::Primary),
        )
        .expect("open tab overflow");
    let active_before_dismissal = harness
        .active_editor_tab_title()
        .expect("read active tab before dismissal");
    harness
        .press_key(HarnessWindow::Project, HarnessKey::Escape)
        .expect("dismiss tab overflow");
    assert_eq!(
        harness
            .active_editor_tab_title()
            .expect("read active tab after dismissal"),
        active_before_dismissal,
        "Escape must not change the active tab"
    );
    harness
        .click_target(
            HarnessWindow::Project,
            HarnessTarget::TabOverflow(EditorPane::Primary),
        )
        .expect("reopen tab overflow");
    harness
        .click_text(HarnessWindow::Project, "Chapter One")
        .expect("activate a hidden tab from overflow");
    assert_eq!(
        harness
            .active_editor_tab_title()
            .expect("read activated overflow tab"),
        "Chapter One"
    );
    close(harness);
}

fn create_project(run: &IsolatedRun, project: &Path, title: &str) -> DesktopInteractionHarness {
    let harness = DesktopInteractionHarness::launch(run.root(), LaunchRequest::launcher())
        .expect("launch application");
    harness
        .click_text(HarnessWindow::Launcher, "Create Project")
        .expect("open project form");
    harness
        .type_into(HarnessWindow::Launcher, "Project title", title)
        .expect("enter project title");
    harness
        .type_into(
            HarnessWindow::Launcher,
            "Project destination",
            project.to_string_lossy(),
        )
        .expect("enter project destination");
    harness
        .click_text(HarnessWindow::Launcher, "Create and Open")
        .expect("create project");
    harness
}

fn create_group(harness: &DesktopInteractionHarness, parent: &str, title: &str) {
    harness
        .right_click_text(HarnessWindow::Project, parent)
        .expect("open parent menu");
    harness
        .click_text(HarnessWindow::Project, "Create group")
        .expect("create group");
    harness
        .replace_text_and_submit(HarnessWindow::Project, "New Group", title)
        .expect("name group");
}

fn create_document(harness: &DesktopInteractionHarness, parent: &str, title: &str) {
    harness
        .right_click_text(HarnessWindow::Project, parent)
        .expect("open parent menu");
    harness
        .click_text(HarnessWindow::Project, "Create document")
        .expect("create document");
    harness
        .replace_text_and_submit(HarnessWindow::Project, "Untitled", title)
        .expect("name document");
}

fn visible(harness: &DesktopInteractionHarness, text: &str) -> bool {
    harness
        .contains_text(HarnessWindow::Project, text)
        .expect("query project surface")
}

fn close(harness: DesktopInteractionHarness) {
    harness
        .close(HarnessWindow::Project)
        .expect("close project");
    harness.shutdown().expect("stop application");
}
