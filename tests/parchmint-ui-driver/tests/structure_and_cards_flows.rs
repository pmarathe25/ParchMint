use std::path::PathBuf;

use parchmint_desktop::{
    DesktopInteractionHarness, EditorPane, HarnessDropPosition, HarnessHierarchySurface,
    HarnessKey, HarnessTarget, HarnessWindow, LaunchRequest, RibbonDestination,
};
use parchmint_ui_driver::IsolatedRun;

#[test]
fn explorer_document_click_reuses_a_temporary_preview_until_opened_permanently() {
    let run = IsolatedRun::new("preview-replacement").expect("isolated run");
    let project = run.root().join("preview-replacement.parchmint");
    let harness = create_project(&run, &project, "Preview Replacement");
    create_document(&harness, "Manuscript", "First Note");
    create_document(&harness, "Manuscript", "Second Note");
    let first = harness
        .hierarchy_node("First Note")
        .expect("resolve the first Explorer note");
    let second = harness
        .hierarchy_node("Second Note")
        .expect("resolve the second Explorer note");

    harness
        .click_hierarchy_node(HarnessWindow::Project, first.clone())
        .expect("activate the first permanent creation tab");
    let first_id = harness
        .active_editor_document_id(EditorPane::Primary)
        .expect("read first document id");
    harness
        .close_editor_tab(HarnessWindow::Project, EditorPane::Primary, first_id)
        .expect("close the first permanent creation tab");
    harness
        .click_hierarchy_node(HarnessWindow::Project, second.clone())
        .expect("activate the second permanent creation tab");
    let second_id = harness
        .active_editor_document_id(EditorPane::Primary)
        .expect("read second document id");
    harness
        .close_editor_tab(HarnessWindow::Project, EditorPane::Primary, second_id)
        .expect("close the second permanent creation tab");

    let baseline_tab_count = harness.tab_titles().expect("read baseline tab strip").len();

    harness
        .click_hierarchy_node(HarnessWindow::Project, first.clone())
        .expect("preview the first note");
    assert_eq!(
        harness
            .tab_titles()
            .expect("read first preview tab strip")
            .len(),
        baseline_tab_count + 1
    );

    harness
        .click_hierarchy_node(HarnessWindow::Project, second.clone())
        .expect("replace the temporary preview");
    assert_eq!(
        harness
            .tab_titles()
            .expect("read replaced preview tab strip")
            .len(),
        baseline_tab_count + 1,
        "a new preview must replace the old one"
    );

    harness
        .right_click_hierarchy_node(HarnessWindow::Project, second)
        .expect("open the second note context menu");
    harness
        .click_text(HarnessWindow::Project, "Open")
        .expect("promote the preview to a permanent tab");
    harness
        .click_hierarchy_node(HarnessWindow::Project, first)
        .expect("preview the first note again");
    assert_eq!(
        harness.tab_titles().expect("read promoted tab strip").len(),
        baseline_tab_count + 2,
        "a promoted tab must survive a later preview"
    );

    close(harness);
}

#[test]
fn explorer_keyboard_navigation_reaches_a_document_and_group_click_collapses_it() {
    let run = IsolatedRun::new("explorer-keyboard-structure").expect("isolated run");
    let project = run.root().join("explorer-keyboard-structure.parchmint");
    let harness = create_project(&run, &project, "Explorer Keyboard Structure");
    create_group(&harness, "Manuscript", "Act One");
    create_document(&harness, "Act One", "Opening Scene");
    let act_one = harness
        .hierarchy_node("Act One")
        .expect("resolve the Act One group");
    let opening_scene = harness
        .hierarchy_node("Opening Scene")
        .expect("resolve the opening-scene document");

    assert!(
        harness
            .hierarchy_node_is_visible(HarnessWindow::Project, opening_scene.clone())
            .expect("inspect expanded Explorer group")
    );
    harness
        .click_hierarchy_node(HarnessWindow::Project, act_one.clone())
        .expect("collapse the act with one click");
    assert!(
        !harness
            .hierarchy_node_is_visible(HarnessWindow::Project, opening_scene)
            .expect("inspect collapsed Explorer group")
    );
    harness
        .click_hierarchy_node(HarnessWindow::Project, act_one)
        .expect("reopen the act");

    for _ in 0..3 {
        harness
            .press_key(HarnessWindow::Project, HarnessKey::F6)
            .expect("focus Explorer through the keyboard regions");
    }
    harness
        .press_key(HarnessWindow::Project, HarnessKey::ArrowDown)
        .expect("navigate from the act to its document");
    harness
        .press_key(HarnessWindow::Project, HarnessKey::Enter)
        .expect("open the keyboard-selected document");
    assert_eq!(
        harness
            .active_editor_tab_title()
            .expect("read keyboard-opened tab"),
        "Opening Scene"
    );

    close(harness);
}

#[test]
fn explorer_add_menu_builds_a_multi_level_outline_in_the_selected_context() {
    let run = IsolatedRun::new("nested-explorer-add").expect("isolated run");
    let project = run.root().join("nested-explorer-add.parchmint");
    let harness = create_project(&run, &project, "Nested Explorer Add");

    add_group(&harness, "Manuscript", "Act One");
    add_group(&harness, "Act One", "Scene One");
    add_document(&harness, "Scene One", "Beat One");
    add_document(&harness, "Act One", "Interlude");

    let titles = harness.hierarchy_titles().expect("read nested hierarchy");
    assert_order(&titles, &["Act One", "Scene One", "Beat One", "Interlude"]);

    close(harness);
}

#[test]
fn cards_selection_can_move_an_outline_item_into_another_group() {
    let run = IsolatedRun::new("cards-cross-group").expect("isolated run");
    let project = run.root().join("cards-cross-group.parchmint");
    let harness = create_project(&run, &project, "Cards Cross Group");
    create_group(&harness, "Manuscript", "Act One");
    create_group(&harness, "Manuscript", "Act Two");
    create_document(&harness, "Act One", "Opening");
    create_document(&harness, "Act Two", "Closing");

    harness
        .click_target(
            HarnessWindow::Project,
            HarnessTarget::Ribbon(RibbonDestination::Cards),
        )
        .expect("open the Cards outline");
    harness
        .click_text(HarnessWindow::Project, "Opening")
        .expect("select an outline card");
    harness
        .click_text(HarnessWindow::Project, "Act Two")
        .expect("select the destination group card");
    let opening = harness
        .hierarchy_node("Opening")
        .expect("resolve source card");
    let act_two = harness
        .hierarchy_node("Act Two")
        .expect("resolve destination card");
    harness
        .drag_hierarchy_node(
            HarnessWindow::Project,
            HarnessHierarchySurface::Cards,
            opening,
            act_two,
            HarnessDropPosition::Into,
        )
        .expect("move the selected card into Act Two");
    let titles = harness.hierarchy_titles().expect("read moved outline");
    assert_order(&titles, &["Act One", "Act Two", "Closing", "Opening"]);

    close(harness);
}

fn create_project(run: &IsolatedRun, project: &PathBuf, title: &str) -> DesktopInteractionHarness {
    let harness = DesktopInteractionHarness::launch(run.root(), LaunchRequest::launcher())
        .expect("launch application");
    harness
        .click_text(HarnessWindow::Launcher, "Create Project")
        .expect("open project creation form");
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
        .expect("open parent context menu");
    harness
        .click_text(HarnessWindow::Project, "Create group")
        .expect("create group");
    harness
        .redraw(HarnessWindow::Project)
        .expect("render the new group-name field");
    harness
        .type_focused(HarnessWindow::Project, title)
        .expect("replace the selected group name");
    harness
        .press_key(HarnessWindow::Project, HarnessKey::Enter)
        .expect("commit group name");
}

fn create_document(harness: &DesktopInteractionHarness, parent: &str, title: &str) {
    harness
        .right_click_text(HarnessWindow::Project, parent)
        .expect("open parent context menu");
    harness
        .click_text(HarnessWindow::Project, "Create document")
        .expect("create document");
    harness
        .redraw(HarnessWindow::Project)
        .expect("render the new document-name field");
    harness
        .type_focused(HarnessWindow::Project, title)
        .expect("replace the selected document name");
    harness
        .press_key(HarnessWindow::Project, HarnessKey::Enter)
        .expect("commit document name");
}

fn add_group(harness: &DesktopInteractionHarness, parent: &str, title: &str) {
    harness
        .click_text(HarnessWindow::Project, parent)
        .expect("select the contextual add parent");
    harness
        .click_target(HarnessWindow::Project, HarnessTarget::ExplorerAdd)
        .expect("open contextual Explorer add");
    harness
        .click_text(HarnessWindow::Project, "Group")
        .expect("choose nested group");
    harness
        .redraw(HarnessWindow::Project)
        .expect("render the nested group-name field");
    harness
        .type_focused(HarnessWindow::Project, title)
        .expect("replace the selected nested group name");
    harness
        .press_key(HarnessWindow::Project, HarnessKey::Enter)
        .expect("commit nested group name");
}

fn add_document(harness: &DesktopInteractionHarness, parent: &str, title: &str) {
    harness
        .click_text(HarnessWindow::Project, parent)
        .expect("select the contextual document parent");
    harness
        .click_target(HarnessWindow::Project, HarnessTarget::ExplorerAdd)
        .expect("open contextual Explorer add");
    harness
        .click_text(HarnessWindow::Project, "Document")
        .expect("choose nested document");
    harness
        .redraw(HarnessWindow::Project)
        .expect("render the nested document-name field");
    harness
        .type_focused(HarnessWindow::Project, title)
        .expect("replace the selected nested document name");
    harness
        .press_key(HarnessWindow::Project, HarnessKey::Enter)
        .expect("commit nested document name");
}

fn assert_order(titles: &[String], expected: &[&str]) {
    let positions = expected
        .iter()
        .map(|title| {
            titles
                .iter()
                .position(|candidate| candidate == title)
                .expect("expected title")
        })
        .collect::<Vec<_>>();
    assert!(
        positions.windows(2).all(|pair| pair[0] < pair[1]),
        "unexpected hierarchy order: {titles:?}"
    );
}

fn close(harness: DesktopInteractionHarness) {
    harness
        .close(HarnessWindow::Project)
        .expect("close project");
    harness.shutdown().expect("stop application");
}
