use std::path::Path;

use parchmint_desktop::{
    DesktopInteractionHarness, EditorPane, HarnessDropPosition, HarnessHierarchySurface,
    HarnessTarget, HarnessWindow, LaunchRequest, RibbonDestination,
};
use parchmint_ui_driver::IsolatedRun;

/// Exercise several structural commands without allowing an intermediate
/// selection or expansion state to hide a stale hierarchy projection.
#[test]
fn rapid_nested_moves_and_renames_keep_one_consistent_hierarchy() {
    let run = IsolatedRun::new("adversarial-hierarchy").expect("isolated run");
    let project = run.root().join("adversarial-hierarchy.parchmint");
    let harness = create_project(&run, &project, "Adversarial Hierarchy");

    create_group(&harness, "Manuscript", "Act One");
    create_group(&harness, "Manuscript", "Act Two");
    create_document(&harness, "Act One", "Opening");
    create_document(&harness, "Act One", "Climax");
    create_document(&harness, "Act Two", "Epilogue");

    let opening = harness.hierarchy_node("Opening").expect("resolve Opening");
    let act_two = harness.hierarchy_node("Act Two").expect("resolve Act Two");
    harness
        .drag_hierarchy_node(
            HarnessWindow::Project,
            HarnessHierarchySurface::Explorer,
            opening,
            act_two,
            HarnessDropPosition::Into,
        )
        .expect("move Opening into Act Two");
    harness
        .click_text(HarnessWindow::Project, "Opening")
        .expect("select moved Opening");
    harness
        .replace_target(
            HarnessWindow::Project,
            HarnessTarget::InspectorTitle,
            "Revised Opening",
        )
        .expect("rename moved document in Inspector");

    let titles = harness.hierarchy_titles().expect("read final hierarchy");
    assert!(
        titles.contains(&"Act One".to_owned()) && titles.contains(&"Act Two".to_owned()),
        "both parent groups must survive rapid structural edits: {titles:?}"
    );
    assert!(
        titles.contains(&"Climax".to_owned()) && titles.contains(&"Epilogue".to_owned()),
        "unmodified siblings must survive rapid structural edits: {titles:?}"
    );
    assert!(
        titles.contains(&"Revised Opening".to_owned()) && !titles.contains(&"Opening".to_owned()),
        "the moved node must have exactly one renamed projection: {titles:?}"
    );
    assert_order(&titles, &["Act Two", "Epilogue", "Revised Opening"]);
    close(harness);
}

/// A temporary preview must be replaceable repeatedly, while a promoted tab
/// remains addressable and does not get silently replaced by the preview.
#[test]
fn repeated_previews_do_not_replace_a_promoted_document_tab() {
    let run = IsolatedRun::new("adversarial-tabs").expect("isolated run");
    let project = run.root().join("adversarial-tabs.parchmint");
    let harness = create_project(&run, &project, "Adversarial Tabs");
    create_document(&harness, "Manuscript", "Alpha");
    create_document(&harness, "Manuscript", "Beta");
    create_document(&harness, "Manuscript", "Gamma");

    let alpha = harness.hierarchy_node("Alpha").expect("resolve Alpha");
    let beta = harness.hierarchy_node("Beta").expect("resolve Beta");
    let gamma = harness.hierarchy_node("Gamma").expect("resolve Gamma");

    // Creation opens permanent tabs. Close them so the following selections
    // exercise the preview-replacement path rather than merely refocusing
    // already-open documents.
    for document in [&alpha, &beta, &gamma] {
        harness
            .click_hierarchy_node(HarnessWindow::Project, document.clone())
            .expect("focus permanent creation tab before closing it");
        let document_id = harness
            .active_editor_document_id(EditorPane::Primary)
            .expect("read permanent creation tab identity");
        harness
            .close_editor_tab(HarnessWindow::Project, EditorPane::Primary, document_id)
            .expect("close permanent creation tab");
    }
    let baseline = harness.tab_titles().expect("read initial tabs").len();
    harness
        .click_hierarchy_node(HarnessWindow::Project, alpha.clone())
        .expect("preview Alpha");
    harness
        .click_hierarchy_node(HarnessWindow::Project, beta.clone())
        .expect("replace preview with Beta");
    harness
        .click_hierarchy_node(HarnessWindow::Project, gamma.clone())
        .expect("replace preview with Gamma");
    assert_eq!(
        harness.tab_titles().expect("read preview tabs").len(),
        baseline + 1,
        "three previews must occupy one temporary slot"
    );

    harness
        .right_click_hierarchy_node(HarnessWindow::Project, beta.clone())
        .expect("open Beta menu");
    harness
        .click_text(HarnessWindow::Project, "Open")
        .expect("promote Beta to a permanent tab");
    let beta_id = harness
        .active_editor_document_id(EditorPane::Primary)
        .expect("read promoted Beta identity");
    harness
        .click_hierarchy_node(HarnessWindow::Project, alpha)
        .expect("preview Alpha after promotion");
    harness
        .click_hierarchy_node(HarnessWindow::Project, gamma)
        .expect("preview Gamma after promotion");
    let tabs = harness.tab_titles().expect("read tabs after promotion");
    assert_eq!(
        tabs.len(),
        baseline + 2,
        "promoted Beta must survive previews: {tabs:?}"
    );
    harness
        .click_hierarchy_node(HarnessWindow::Project, beta)
        .expect("focus promoted Beta");
    assert_eq!(
        harness
            .active_editor_document_id(EditorPane::Primary)
            .expect("read Beta identity after previews"),
        beta_id,
        "preview navigation must not duplicate or replace the permanent Beta tab"
    );
    close(harness);
}

/// Keep the deleted item open while restoring it, then verify the restored
/// hierarchy and editor identity after a full process restart.
#[test]
fn deleting_an_open_document_and_restoring_after_restart_recovers_identity() {
    let run = IsolatedRun::new("adversarial-delete-restart").expect("isolated run");
    let project = run.root().join("adversarial-delete-restart.parchmint");
    let harness = create_project(&run, &project, "Delete Restart");
    create_document(&harness, "Manuscript", "Recover Me");
    let recovered_node = harness
        .hierarchy_node("Recover Me")
        .expect("resolve document before deletion");
    harness
        .right_click_hierarchy_node(HarnessWindow::Project, recovered_node.clone())
        .expect("open document menu");
    harness
        .click_text(HarnessWindow::Project, "Open")
        .expect("open document before deletion");
    harness
        .type_into_target(
            HarnessWindow::Project,
            HarnessTarget::EditorPrimary,
            "This prose must survive deletion and restoration.",
        )
        .expect("write the recoverable document body");
    harness
        .elapse_autosave_idle()
        .expect("persist the recoverable document body before deletion");
    let document_id = harness
        .active_editor_document_id(EditorPane::Primary)
        .expect("read document identity before deletion");
    harness
        .right_click_hierarchy_node(HarnessWindow::Project, recovered_node)
        .expect("reopen document menu");
    harness
        .redraw(HarnessWindow::Project)
        .expect("render document menu before deletion");
    harness
        .click_text(HarnessWindow::Project, "Delete")
        .expect("delete open document");
    assert!(
        !harness
            .hierarchy_titles()
            .expect("read hierarchy after deletion")
            .contains(&"Recover Me".to_owned()),
        "deleted document must leave the live hierarchy"
    );
    close(harness);

    let reopened = DesktopInteractionHarness::launch(run.root(), LaunchRequest::launcher())
        .expect("relaunch application");
    reopened
        .click_text(HarnessWindow::Launcher, "Delete Restart")
        .expect("reopen project");
    reopened
        .click_target(
            HarnessWindow::Project,
            HarnessTarget::Ribbon(RibbonDestination::RecentlyDeleted),
        )
        .expect("open Recently Deleted after restart");
    assert!(contains(&reopened, "Recover Me"));
    reopened
        .click_text(HarnessWindow::Project, "Recover Me")
        .expect("select deleted document");
    reopened
        .click_text(HarnessWindow::Project, "Restore item")
        .expect("restore deleted document");
    reopened
        .click_target(
            HarnessWindow::Project,
            HarnessTarget::Ribbon(RibbonDestination::Editor),
        )
        .expect("return to Editor after restore");
    let titles = reopened
        .hierarchy_titles()
        .expect("read hierarchy after restoration");
    assert!(
        titles.iter().any(|title| title == "Recover Me"),
        "restoration must return the document to the live hierarchy: {titles:?}"
    );
    let recovered = reopened
        .hierarchy_node("Recover Me")
        .expect("resolve restored document");
    reopened
        .click_hierarchy_node(HarnessWindow::Project, recovered)
        .expect("open restored document");
    assert_eq!(
        reopened
            .active_editor_document_id(EditorPane::Primary)
            .expect("read restored document identity"),
        document_id,
        "restore must retain the document identity across restart"
    );
    assert!(
        reopened
            .active_editor_body()
            .expect("read restored document body")
            .contains("This prose must survive deletion and restoration."),
        "restore must recover the pre-delete document body"
    );
    close(reopened);
}

/// Search state is intentionally changed between local and global modes; the
/// global replacement must operate on the current query and all matching docs.
#[test]
fn replacing_after_switching_search_modes_does_not_use_stale_query_state() {
    let run = IsolatedRun::new("adversarial-search-state").expect("isolated run");
    let project = run.root().join("adversarial-search-state.parchmint");
    let harness = create_project(&run, &project, "Adversarial Search State");
    create_document(&harness, "Manuscript", "First");
    create_document(&harness, "Manuscript", "Second");
    for title in ["First", "Second"] {
        harness
            .right_click_text(HarnessWindow::Project, title)
            .expect("open document menu");
        harness
            .click_text(HarnessWindow::Project, "Open")
            .expect("open search fixture document");
        harness
            .type_into_target(
                HarnessWindow::Project,
                HarnessTarget::EditorPrimary,
                "old old",
            )
            .expect("seed repeated search phrase");
        harness
            .elapse_autosave_idle()
            .expect("persist search fixture document");
    }
    harness
        .press_command_key(HarnessWindow::Project, 'f')
        .expect("open local search before switching modes");
    harness
        .type_into_target(
            HarnessWindow::Project,
            HarnessTarget::LocalFind(EditorPane::Primary),
            "old",
        )
        .expect("set local search query");
    assert!(
        visible(&harness, "2 matches"),
        "local search should find both occurrences"
    );
    harness
        .click_target(HarnessWindow::Project, HarnessTarget::ExplorerSearch)
        .expect("open global search");
    harness
        .type_into_target(
            HarnessWindow::Project,
            HarnessTarget::GlobalSearchQuery,
            "old",
        )
        .expect("set current global query");
    harness
        .redraw(HarnessWindow::Project)
        .expect("render search results");
    assert!(
        visible(&harness, "4 matches in 2 documents"),
        "global search must see both documents; status: {} trace: {:?}",
        harness.global_search_status().expect("read search status"),
        harness.trace().expect("read interaction trace")
    );
    harness
        .type_into_target(
            HarnessWindow::Project,
            HarnessTarget::GlobalReplacement,
            "new",
        )
        .expect("set global replacement");
    harness
        .click_text(HarnessWindow::Project, "Replace")
        .expect("open replacement preview");
    harness
        .click_text(HarnessWindow::Project, "Revalidate selection")
        .expect("revalidate current selection");
    harness
        .click_text(HarnessWindow::Project, "Apply replacement")
        .expect("apply replacement to both documents");
    assert!(
        !harness
            .active_editor_body()
            .expect("read editor after replacement")
            .contains("old"),
        "replacement must not leave stale query text in the focused document"
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

fn assert_order(titles: &[String], expected: &[&str]) {
    let mut previous = None;
    for title in expected {
        let index = titles
            .iter()
            .position(|candidate| candidate == title)
            .unwrap_or_else(|| panic!("missing {title:?} in hierarchy: {titles:?}"));
        if let Some(previous_index) = previous {
            assert!(
                previous_index < index,
                "hierarchy order is incorrect for {expected:?}: {titles:?}"
            );
        }
        previous = Some(index);
    }
}

fn contains(harness: &DesktopInteractionHarness, text: &str) -> bool {
    harness
        .contains_text(HarnessWindow::Project, text)
        .expect("query project surface")
}

fn visible(harness: &DesktopInteractionHarness, text: &str) -> bool {
    contains(harness, text)
}

fn close(harness: DesktopInteractionHarness) {
    harness
        .close(HarnessWindow::Project)
        .expect("close project");
    harness.shutdown().expect("stop application");
}
