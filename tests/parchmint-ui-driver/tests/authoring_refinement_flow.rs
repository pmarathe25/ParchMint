use std::path::Path;

use parchmint_desktop::{
    DesktopInteractionHarness, EditorPane, HarnessTarget, HarnessWindow, LaunchRequest,
    RibbonDestination,
};
use parchmint_ui_driver::IsolatedRun;

#[test]
fn history_reloads_prior_checkpoints_after_comment_activity() {
    let run = IsolatedRun::new("history-after-comment").expect("isolated run");
    let project = run.root().join("history-after-comment.parchmint");
    let harness = create_project(&run, &project, "History After Comment");

    create_group(&harness, "Research", "Sources");
    create_document(&harness, "Sources", "Tide Ledger");

    harness
        .type_into_target(
            HarnessWindow::Project,
            HarnessTarget::EditorPrimary,
            "Lantern light crosses the harbor.",
        )
        .expect("write first revision");
    harness
        .elapse_autosave_idle()
        .expect("create first automatic checkpoint");
    harness
        .type_into_target(
            HarnessWindow::Project,
            HarnessTarget::EditorPrimary,
            " The tide turns before dawn.",
        )
        .expect("write second revision");
    harness
        .elapse_autosave_idle()
        .expect("create second automatic checkpoint");
    harness
        .right_click_text(HarnessWindow::Project, "Tide Ledger")
        .expect("open research context menu");
    harness
        .click_text(HarnessWindow::Project, "Open in companion")
        .expect("open a companion document");
    harness
        .click_target(
            HarnessWindow::Project,
            HarnessTarget::Ribbon(RibbonDestination::Cards),
        )
        .expect("visit Cards before searching");
    harness
        .click_target(HarnessWindow::Project, HarnessTarget::ExplorerSearch)
        .expect("open global search from Explorer");
    harness
        .click_target(
            HarnessWindow::Project,
            HarnessTarget::Ribbon(RibbonDestination::Editor),
        )
        .expect("return to editor after searching");
    harness
        .select_editor_text(HarnessWindow::Project, EditorPane::Primary, "Lantern light")
        .expect("select comment anchor");
    harness
        .right_click_target_at(
            HarnessWindow::Project,
            HarnessTarget::EditorPrimary,
            (0.5, 0.5),
        )
        .expect("open comment actions");
    harness
        .click_text(HarnessWindow::Project, "Add Comment")
        .expect("begin comment");
    harness
        .type_into_target(
            HarnessWindow::Project,
            HarnessTarget::CommentDraft,
            "Confirm the lantern image.",
        )
        .expect("write comment");
    harness
        .click_text(HarnessWindow::Project, "Add comment")
        .expect("persist comment");

    harness
        .click_target(
            HarnessWindow::Project,
            HarnessTarget::Ribbon(RibbonDestination::History),
        )
        .expect("open history after comment activity");
    let checkpoints = harness
        .history_checkpoints()
        .expect("read history after comment activity");
    assert!(
        checkpoints.len() >= 2,
        "comment activity must not empty the loaded history timeline: {checkpoints:?}"
    );

    harness
        .close(HarnessWindow::Project)
        .expect("close history-after-comment project");
    harness
        .shutdown()
        .expect("stop history-after-comment application");
}

fn create_project(run: &IsolatedRun, project: &Path, title: &str) -> DesktopInteractionHarness {
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
        .replace_text_and_submit(HarnessWindow::Project, "New Group", title)
        .expect("name group");
}

fn create_document(harness: &DesktopInteractionHarness, parent: &str, title: &str) {
    harness
        .right_click_text(HarnessWindow::Project, parent)
        .expect("open parent context menu");
    harness
        .click_text(HarnessWindow::Project, "Create document")
        .expect("create document");
    harness
        .replace_text_and_submit(HarnessWindow::Project, "Untitled", title)
        .expect("name document");
}
