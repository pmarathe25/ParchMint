use std::path::Path;

use parchmint_desktop::{
    DesktopInteractionHarness, EditorPane, HarnessTarget, HarnessWindow, LaunchRequest,
};
use parchmint_ui_driver::IsolatedRun;

#[test]
fn editor_can_research_and_revise_the_same_document_from_both_panes() {
    let run = IsolatedRun::new("same-document-research").expect("isolated run");
    let project = run.root().join("same-document-research.parchmint");
    let harness = create_project(&run, &project, "Same Document Research");

    create_group(&harness, "Manuscript", "Field Notes");
    create_document(&harness, "Field Notes", "Tide Journal");
    harness
        .right_click_text(HarnessWindow::Project, "Tide Journal")
        .expect("open research document menu");
    harness
        .click_text(HarnessWindow::Project, "Open")
        .expect("open research document in primary pane");
    harness
        .type_into_target(
            HarnessWindow::Project,
            HarnessTarget::EditorPrimary,
            "The tide turns at dusk.",
        )
        .expect("write source note in primary pane");
    let document_id = harness
        .active_editor_document_id(EditorPane::Primary)
        .expect("read primary document identity");

    harness
        .right_click_text(HarnessWindow::Project, "Tide Journal")
        .expect("reopen research document menu");
    harness
        .click_text(HarnessWindow::Project, "Open in companion")
        .expect("open the same source in companion pane");
    assert_eq!(
        document_id,
        harness
            .active_editor_document_id(EditorPane::Companion)
            .expect("read companion document identity")
    );
    harness
        .type_into_target(
            HarnessWindow::Project,
            HarnessTarget::EditorCompanion,
            " The lantern stays lit.",
        )
        .expect("append research detail through the companion pane");
    assert!(
        harness
            .active_editor_body()
            .expect("read companion body")
            .contains("lantern")
    );

    harness
        .type_into_target(
            HarnessWindow::Project,
            HarnessTarget::EditorPrimary,
            " Verified by the log.",
        )
        .expect("revise from the primary pane after a companion edit");
    assert!(
        harness
            .active_editor_body()
            .expect("read primary body after focus transfer")
            .contains("Verified by the log")
    );
    harness
        .close(HarnessWindow::Project)
        .expect("close project");
    harness.shutdown().expect("stop application");
}

#[test]
fn editor_supports_local_then_project_wide_phrase_revision() {
    let run = IsolatedRun::new("local-global-revision").expect("isolated run");
    let project = run.root().join("local-global-revision.parchmint");
    let harness = create_project(&run, &project, "Local Global Revision");
    harness
        .type_into_target(
            HarnessWindow::Project,
            HarnessTarget::EditorPrimary,
            "draft draft stays provisional",
        )
        .expect("draft repeated phrase");

    harness
        .press_command_key(HarnessWindow::Project, 'f')
        .expect("open local find");
    harness
        .type_into_target(
            HarnessWindow::Project,
            HarnessTarget::LocalFind(EditorPane::Primary),
            "draft",
        )
        .expect("search current document");
    assert!(visible(&harness, "2 matches"));
    harness
        .click_text(HarnessWindow::Project, "Replace")
        .expect("open local replace");
    harness
        .type_into_target(
            HarnessWindow::Project,
            HarnessTarget::LocalReplace(EditorPane::Primary),
            "revision",
        )
        .expect("enter local replacement");
    harness
        .click_text(HarnessWindow::Project, "Replace all")
        .expect("apply local replacement");
    assert!(
        harness
            .active_editor_body()
            .expect("read local revision")
            .contains("revision revision")
    );
    harness
        .elapse_autosave_idle()
        .expect("save the local revision before project-wide search");
    assert!(
        canonical_bodies(&project)
            .iter()
            .any(|body| body.contains("revision revision")),
        "local replacement must reach canonical storage before global search"
    );

    harness
        .click_target(HarnessWindow::Project, HarnessTarget::ExplorerSearch)
        .expect("open project-wide search");
    harness
        .type_into_target(
            HarnessWindow::Project,
            HarnessTarget::GlobalSearchQuery,
            "revision",
        )
        .expect("search revised phrase globally");
    harness
        .redraw(HarnessWindow::Project)
        .expect("render the updated global-search results");
    assert!(
        visible(&harness, "2 matches in 1 document"),
        "global search: {}; trace: {:?}",
        harness
            .global_search_status()
            .expect("read global-search status"),
        harness.trace().expect("read global-search trace"),
    );
    harness
        .type_into_target(
            HarnessWindow::Project,
            HarnessTarget::GlobalReplacement,
            "final",
        )
        .expect("enter global replacement");
    harness
        .click_text(HarnessWindow::Project, "Replace")
        .expect("open replacement preview");
    harness
        .click_text(HarnessWindow::Project, "Revalidate selection")
        .expect("revalidate global matches");
    harness
        .click_text(HarnessWindow::Project, "Apply replacement")
        .expect("apply global replacement");
    assert!(
        harness
            .active_editor_body()
            .expect("read final revision")
            .contains("final final")
    );
    harness
        .close(HarnessWindow::Project)
        .expect("close project");
    harness.shutdown().expect("stop application");
}

#[test]
fn editor_selection_formatting_can_be_undone_and_redone_with_keyboard_focus() {
    let run = IsolatedRun::new("format-history-flow").expect("isolated run");
    let project = run.root().join("format-history-flow.parchmint");
    let harness = create_project(&run, &project, "Format History Flow");
    harness
        .type_into_target(
            HarnessWindow::Project,
            HarnessTarget::EditorPrimary,
            "A decisive opening.",
        )
        .expect("draft opening");
    harness
        .select_editor_text(HarnessWindow::Project, EditorPane::Primary, "decisive")
        .expect("select adjective");
    harness
        .press_command_key(HarnessWindow::Project, 'b')
        .expect("bold selected adjective");
    harness
        .press_command_key(HarnessWindow::Project, 'z')
        .expect("undo formatting");
    assert!(
        !harness
            .active_editor_body()
            .expect("read undone prose")
            .contains("<strong>")
    );
    #[cfg(target_os = "macos")]
    harness
        .press_command_shift_key(HarnessWindow::Project, 'z')
        .expect("redo formatting on macOS");
    #[cfg(not(target_os = "macos"))]
    harness
        .press_command_key(HarnessWindow::Project, 'y')
        .expect("redo formatting");
    harness
        .elapse_autosave_idle()
        .expect("save formatted revision");
    assert!(
        canonical_bodies(&project)
            .iter()
            .any(|body| body.contains("<strong>decisive</strong>")),
        "redo should restore canonical formatting"
    );
    harness
        .close(HarnessWindow::Project)
        .expect("close project");
    harness.shutdown().expect("stop application");
}

fn create_project(run: &IsolatedRun, project: &Path, title: &str) -> DesktopInteractionHarness {
    let harness = DesktopInteractionHarness::launch(run.root(), LaunchRequest::launcher())
        .expect("launch application");
    harness
        .click_text(HarnessWindow::Launcher, "Create Project")
        .expect("open project form");
    harness
        .type_into(HarnessWindow::Launcher, "Project title", title)
        .expect("enter title");
    harness
        .type_into(
            HarnessWindow::Launcher,
            "Project destination",
            project.to_string_lossy(),
        )
        .expect("enter destination");
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

fn canonical_bodies(project: &Path) -> Vec<String> {
    ["manuscript", "research"]
        .into_iter()
        .flat_map(|directory| canonical_bodies_in(&project.join(directory)))
        .collect()
}

fn canonical_bodies_in(directory: &Path) -> Vec<String> {
    std::fs::read_dir(directory)
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .flat_map(|entry| {
            let path = entry.path();
            if path.is_dir() {
                canonical_bodies_in(&path)
            } else if path.extension().is_some_and(|value| value == "html") {
                vec![std::fs::read_to_string(path).expect("read canonical document")]
            } else {
                Vec::new()
            }
        })
        .collect()
}
